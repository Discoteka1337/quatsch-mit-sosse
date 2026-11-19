//! Lokale Mikrofonaufnahme über WASAPI, mit 500 ms Vorlauf und 16 kHz Monoausgabe.
//!
//! Erst `AudioCapture::start` öffnet ein Mikrofon. Callbacks laufen auf dem
//! Verwaltungs-Thread: kurz halten, nicht blockieren und keine synchronen
//! Methoden derselben Aufnahme daraus aufrufen. `stop_ready` beendet alle Threads.

mod backend;
mod capture;
mod dsp;
mod worker;

use crossbeam_channel::{Sender, bounded};
use std::{fmt, thread, time::Duration};

/// Abtastrate aller ausgegebenen Aufnahmen.
pub const SAMPLE_RATE: u32 = dsp::SAMPLE_RATE;
/// Maximale Länge des Vorlaufpuffers.
pub const PRE_ROLL: Duration = Duration::from_millis(500);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum DeviceSelection {
    #[default]
    Default,
    /// Exakter Anzeigename; mehrdeutige Namen werden abgewiesen.
    Named(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceInfo {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioError {
    NoInputDevice,
    DeviceNotFound(String),
    AmbiguousDeviceName(String),
    Backend(String),
    UnsupportedPlatform,
    NotReady,
    AlreadyRecording,
    NotRecording,
    WorkerStopped,
    StreamGlitch,
    RealtimeDenied,
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoInputDevice => write!(f, "Kein Standardmikrofon verfügbar."),
            Self::DeviceNotFound(name) => write!(f, "Mikrofon nicht gefunden: {name}"),
            Self::AmbiguousDeviceName(name) => write!(
                f,
                "Mehrere Mikrofone heißen {name}. Bitte in Windows eindeutig benennen."
            ),
            Self::Backend(message) => f.write_str(message),
            Self::UnsupportedPlatform => {
                write!(f, "Mikrofonaufnahme wird nur unter Windows unterstützt.")
            }
            Self::NotReady => write!(f, "Die Aufnahmebereitschaft ist beendet."),
            Self::AlreadyRecording => write!(f, "Es läuft bereits eine Aufnahme."),
            Self::NotRecording => write!(f, "Es läuft keine Aufnahme."),
            Self::WorkerStopped => write!(f, "Die Audioverwaltung wurde unerwartet beendet."),
            Self::StreamGlitch => write!(f, "Der Audiotreiber meldet eine Unterbrechung."),
            Self::RealtimeDenied => write!(f, "Der Audiothread erhält keine erhöhte Priorität."),
        }
    }
}

impl std::error::Error for AudioError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioEvent {
    DeviceChanged {
        device: DeviceInfo,
        fallback: bool,
    },
    DeviceLost {
        device: DeviceInfo,
        error: AudioError,
    },
    DeviceUnavailable {
        error: AudioError,
    },
    /// Audioblöcke gingen verloren; der Stream wird neu aufgebaut.
    InputOverflow,
    /// Der Treiber meldet verlorene Daten; die Gerätewahl bleibt erhalten.
    InputDiscontinuity,
    /// Aufnahme läuft weiter, jedoch ohne erhöhte Threadpriorität.
    RealtimeUnavailable,
    Stopped,
}

/// Mono-f32-Samples mit 16 kHz. Debug zeigt ausschließlich die Sampleanzahl.
pub struct Recording {
    samples: Vec<f32>,
}

impl Recording {
    fn new(samples: Vec<f32>) -> Self {
        Self { samples }
    }
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }
    pub fn into_samples(self) -> Vec<f32> {
        self.samples
    }
    pub fn duration(&self) -> Duration {
        Duration::from_secs_f64(self.samples.len() as f64 / f64::from(SAMPLE_RATE))
    }
}

impl fmt::Debug for Recording {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Recording")
            .field("sample_count", &self.samples.len())
            .finish()
    }
}

/// Listet Eingabegeräte auf, ohne einen Aufnahmestream zu starten.
pub fn input_devices() -> Result<Vec<DeviceInfo>, AudioError> {
    backend::input_devices()
}

/// Liefert das aktuelle Windows-Standardmikrofon, ohne es aufzunehmen.
pub fn default_input_device() -> Result<Option<DeviceInfo>, AudioError> {
    backend::default_input_device()
}

pub struct AudioCapture {
    commands: Option<Sender<worker::Command>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl AudioCapture {
    /// Aktiviert die Bereitschaft. Bei fehlendem Gerät bleibt sie aktiv und
    /// versucht alle 500 ms das Standardgerät; Ereignisse melden den Zustand.
    /// RMS liegt in 0..=1 und wird bei fließendem Audio etwa 30-mal/s geliefert.
    pub fn start(
        selection: DeviceSelection,
        on_level: impl FnMut(f32) + Send + 'static,
        on_event: impl FnMut(AudioEvent) + Send + 'static,
    ) -> Result<Self, AudioError> {
        Self::spawn(backend::CpalBackend::new, selection, on_level, on_event)
    }

    fn spawn<B: backend::Backend + 'static>(
        factory: impl FnOnce() -> Result<B, AudioError> + Send + 'static,
        selection: DeviceSelection,
        on_level: impl FnMut(f32) + Send + 'static,
        on_event: impl FnMut(AudioEvent) + Send + 'static,
    ) -> Result<Self, AudioError> {
        let (commands, receiver) = bounded(16);
        let (started_tx, started_rx) = bounded(1);
        let worker = thread::Builder::new()
            .name("qms-audio".into())
            .spawn(move || match factory() {
                Ok(backend) => worker::Worker::new(backend, selection, on_level, on_event)
                    .run(receiver, started_tx),
                Err(error) => {
                    let _ = started_tx.send(Err(error));
                }
            })
            .map_err(|_| AudioError::WorkerStopped)?;
        match started_rx.recv().unwrap_or(Err(AudioError::WorkerStopped)) {
            Ok(()) => Ok(Self {
                commands: Some(commands),
                worker: Some(worker),
            }),
            Err(error) => {
                let _ = worker.join();
                Err(error)
            }
        }
    }

    /// Beginnt eine Aufnahme mit dem aktuell vorhandenen Vorlauf (maximal 500 ms).
    pub fn start_recording(&self) -> Result<(), AudioError> {
        self.request(worker::Command::Start)
    }

    /// Beendet nur die Aufnahme; Mikrofon und Vorlauf bleiben aktiv.
    pub fn finish_recording(&self) -> Result<Recording, AudioError> {
        self.request(worker::Command::Finish)
    }

    /// Wechselt das Gerät, ohne die laufende Aufnahme zu verwerfen. Bei einem
    /// Fehler wird das Standardgerät versucht und der Auswahlfehler zurückgegeben.
    pub fn select_device(&self, selection: DeviceSelection) -> Result<(), AudioError> {
        self.request(|reply| worker::Command::Select(selection, reply))
    }

    /// Gibt das Mikrofon frei und wartet auf das Ende aller Audio-Threads.
    /// Liefert eine noch laufende Aufnahme zurück. Mehrfaches Stoppen ist erlaubt.
    /// Zum erneuten Aktivieren `AudioCapture::start` verwenden.
    pub fn stop_ready(&mut self) -> Result<Option<Recording>, AudioError> {
        let result = if self.commands.is_some() {
            self.request(worker::Command::Stop)
        } else {
            Ok(None)
        };
        self.commands.take();
        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| AudioError::WorkerStopped)?;
        }
        result
    }

    fn request<T>(
        &self,
        command: impl FnOnce(Sender<Result<T, AudioError>>) -> worker::Command,
    ) -> Result<T, AudioError> {
        let sender = self.commands.as_ref().ok_or(AudioError::NotReady)?;
        let (reply, receive) = bounded(1);
        sender
            .send(command(reply))
            .map_err(|_| AudioError::WorkerStopped)?;
        receive.recv().map_err(|_| AudioError::WorkerStopped)?
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        let _ = self.stop_ready();
    }
}

#[cfg(test)]
mod tests;
