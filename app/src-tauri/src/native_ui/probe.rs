use super::LABELS;
use quatsch_mit_sosse_audio::{AudioCapture, AudioEvent, DeviceSelection};
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc::{self, Sender},
    },
    thread,
    time::Duration,
};

#[derive(Default)]
struct Signal {
    current: AtomicU8,
    detected: AtomicBool,
    unavailable: AtomicBool,
    finished: AtomicBool,
}

pub(super) struct Probe {
    // Dropping this sender ends the test when the settings window closes.
    _stop: Sender<()>,
    signal: Arc<Signal>,
}

impl Probe {
    pub(super) fn start(device: Option<String>) -> io::Result<Self> {
        let signal = Arc::new(Signal::default());
        let worker = signal.clone();
        let (stop, stopped) = mpsc::channel();
        thread::Builder::new()
            .name("qms-microphone-test".into())
            .spawn(move || {
                let levels = worker.clone();
                let events = worker.clone();
                let selected = device.map(DeviceSelection::Named).unwrap_or_default();
                let capture = AudioCapture::start(
                    selected,
                    move |rms| {
                        if !levels.unavailable.load(Ordering::Acquire) {
                            let level = (rms.sqrt().clamp(0.0, 1.0) * 100.0).round() as u8;
                            levels.current.store(level, Ordering::Relaxed);
                            if rms >= 0.002 {
                                levels.detected.store(true, Ordering::Relaxed);
                            }
                        }
                    },
                    move |event| {
                        let unavailable = match event {
                            AudioEvent::DeviceChanged { fallback, .. } => fallback,
                            AudioEvent::DeviceUnavailable { .. }
                            | AudioEvent::DeviceLost { .. } => true,
                            _ => return,
                        };
                        events.unavailable.store(unavailable, Ordering::Release);
                        if unavailable {
                            events.current.store(0, Ordering::Relaxed);
                            events.detected.store(false, Ordering::Relaxed);
                        }
                    },
                );
                if let Ok(capture) = capture {
                    let _ = stopped.recv_timeout(Duration::from_secs(5));
                    drop(capture);
                } else {
                    worker.unavailable.store(true, Ordering::Release);
                }
                worker.finished.store(true, Ordering::Release);
            })?;
        Ok(Self {
            _stop: stop,
            signal,
        })
    }
    pub(super) fn level(&self) -> usize {
        usize::from(self.signal.current.load(Ordering::Relaxed))
    }
    pub(super) fn finished(&self) -> bool {
        self.signal.finished.load(Ordering::Acquire)
    }
    pub(super) fn result(&self) -> &'static str {
        if self.signal.unavailable.load(Ordering::Acquire) {
            &LABELS.general_test_unavailable
        } else if self.signal.detected.load(Ordering::Relaxed) {
            &LABELS.general_test_ok
        } else {
            &LABELS.general_test_silent
        }
    }
}
