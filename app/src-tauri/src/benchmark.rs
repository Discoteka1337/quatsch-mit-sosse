use std::ffi::OsString;
use std::io::{self, Write};
use std::path::PathBuf;
use windows::Win32::Foundation::WAIT_OBJECT_0;
use windows::Win32::System::Threading::{
    INFINITE, OpenEventW, SYNCHRONIZATION_SYNCHRONIZE, WaitForSingleObject,
};
use windows::core::{HSTRING, Owned};

pub(crate) struct BenchmarkConfig {
    pub(crate) log_path: PathBuf,
    pub(crate) stop_event: OsString,
}

impl BenchmarkConfig {
    pub(crate) fn from_environment() -> io::Result<Option<Self>> {
        Self::from_values(
            std::env::var_os("QMS_BENCHMARK_LOG"),
            std::env::var_os("QMS_BENCHMARK_STOP_EVENT"),
        )
    }

    fn from_values(log: Option<OsString>, event: Option<OsString>) -> io::Result<Option<Self>> {
        match (log, event) {
            (None, None) => Ok(None),
            (Some(log), Some(event)) if PathBuf::from(&log).is_absolute() && !event.is_empty() => {
                Ok(Some(Self {
                    log_path: PathBuf::from(log),
                    stop_event: event,
                }))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Die Benchmark-Konfiguration ist unvollständig oder ungültig.",
            )),
        }
    }
}

pub(crate) struct ReadySignal<W: Write> {
    writer: W,
    emitted: bool,
}

impl<W: Write> ReadySignal<W> {
    pub(crate) fn new(writer: W) -> Self {
        Self {
            writer,
            emitted: false,
        }
    }

    pub(crate) fn emit(&mut self) -> io::Result<()> {
        if !self.emitted {
            self.writer.write_all(b"QMS_READY\n")?;
            self.writer.flush()?;
            self.emitted = true;
        }
        Ok(())
    }
}

pub(crate) fn start_stop_listener(
    event_name: OsString,
    on_stop: impl FnOnce(i32) + Send + 'static,
) -> io::Result<()> {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("qms-benchmark-stop".into())
        .spawn(move || {
            // Open and own the handle on this thread; no raw handle crosses threads.
            let event = unsafe {
                OpenEventW(
                    SYNCHRONIZATION_SYNCHRONIZE,
                    false,
                    &HSTRING::from(event_name),
                )
            };
            let event = match event {
                Ok(handle) => unsafe { Owned::new(handle) },
                Err(_) => {
                    let _ = sender.send(Err(io::Error::other(
                        "Das Benchmark-Stoppereignis konnte nicht geöffnet werden.",
                    )));
                    return;
                }
            };
            if sender.send(Ok(())).is_err() {
                return;
            }
            // A blocked Windows event has no polling cost during the idle measurement.
            let result = unsafe { WaitForSingleObject(*event, INFINITE) };
            on_stop(if result == WAIT_OBJECT_0 { 0 } else { 1 });
        })?;
    receiver
        .recv()
        .map_err(|_| io::Error::other("Die Benchmark-Steuerung konnte nicht starten."))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::io::{self, Write};

    #[test]
    fn ready_signal_is_exact_and_emitted_once() {
        let mut signal = ReadySignal::new(Vec::new());
        signal.emit().unwrap();
        signal.emit().unwrap();
        assert_eq!(signal.writer, b"QMS_READY\n");
    }

    #[test]
    fn ready_signal_reports_writer_failures() {
        struct FailingWriter;
        impl Write for FailingWriter {
            fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("synthetic failure"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut signal = ReadySignal::new(FailingWriter);
        assert!(signal.emit().is_err());
        assert!(!signal.emitted);
    }

    #[test]
    fn benchmark_settings_are_opt_in_and_require_both_values() {
        assert!(BenchmarkConfig::from_values(None, None).unwrap().is_none());
        let log = Some(OsString::from(r"C:\qms test\ready.log"));
        let event = Some(OsString::from(r"Local\qms-test-event"));
        assert!(BenchmarkConfig::from_values(log.clone(), None).is_err());
        assert!(BenchmarkConfig::from_values(None, event.clone()).is_err());
        let config = BenchmarkConfig::from_values(log, event).unwrap().unwrap();
        assert_eq!(
            config.log_path,
            std::path::PathBuf::from(r"C:\qms test\ready.log")
        );
        assert_eq!(config.stop_event, OsString::from(r"Local\qms-test-event"));
        assert!(
            BenchmarkConfig::from_values(
                Some(OsString::from("relative.log")),
                Some(OsString::from(r"Local\qms-test-event")),
            )
            .is_err()
        );
        assert!(
            BenchmarkConfig::from_values(
                Some(OsString::from(r"C:\qms test\ready.log")),
                Some(OsString::new()),
            )
            .is_err()
        );
    }

    #[test]
    fn stop_listener_rejects_missing_event_without_a_ready_marker() {
        let missing_name = OsString::from(format!(
            "Local\\qms-missing-event-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert!(start_stop_listener(missing_name, |_| panic!("No event was opened")).is_err());
    }

    #[test]
    fn stop_listener_waits_until_the_named_event_is_signalled() {
        use windows::Win32::System::Threading::{CreateEventW, SetEvent};
        let name = OsString::from(format!(
            "Local\\qms-stop-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let event =
            unsafe { Owned::new(CreateEventW(None, true, false, &HSTRING::from(&name)).unwrap()) };
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        start_stop_listener(name, move |code| sender.send(code).unwrap()).unwrap();
        assert!(
            receiver
                .recv_timeout(std::time::Duration::from_millis(50))
                .is_err()
        );
        unsafe { SetEvent(*event).unwrap() };
        assert_eq!(
            receiver
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap(),
            0
        );
    }
}
