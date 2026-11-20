mod chunks;
mod engine;
mod idle;
mod onnx;
mod phantoms;

use std::time::Duration;

use chunks::recognize_chunks;
pub use chunks::{MAX_CHUNK_SECONDS, split_at_pauses};
pub use engine::{TransducerEngine, WhisperEngine, create_engine};

pub type Result<T> = std::result::Result<T, Error>;

// Do not derive Debug: transcripts may contain private dictation content.
pub struct Transcript {
    pub text: String,
    /// Wall time including cold model loading, VAD and sequential recognition.
    pub processing_duration: Duration,
}

pub trait SpeechEngine: Send {
    /// Accepts normalized, finite 16 kHz mono audio. This blocking call belongs on a worker.
    fn transcribe(
        &mut self,
        samples: &[f32],
        sample_rate: u32,
        language_hint: Option<&str>,
    ) -> Result<Transcript>;
}

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub num_threads: u32,
    pub idle_timeout: Duration,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            num_threads: 2,
            idle_timeout: Duration::from_secs(300),
        }
    }
}

impl EngineConfig {
    fn validate(&self) -> Result<()> {
        if self.num_threads == 0 || self.num_threads > i32::MAX as u32 {
            return Err(Error::InvalidConfig);
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Die Aufnahme muss endliche, normalisierte Mono-Samples mit 16 kHz enthalten.")]
    InvalidAudio,
    #[error("Die Sprachabschnitte sind ungültig oder überlappen sich.")]
    InvalidSegments,
    #[error("Die Thread-Anzahl muss zwischen 1 und 2147483647 liegen.")]
    InvalidConfig,
    #[error("Die gewählte Sprache wird vom Modell nicht unterstützt.")]
    UnsupportedLanguage,
    #[error("Das Modell ist unbekannt oder passt nicht zur gewählten Erkennung.")]
    UnsupportedModel,
    #[error(
        "ASR-Modell oder Silero-VAD ist nicht vollständig installiert. Bitte models-cli verwenden."
    )]
    ModelNotInstalled,
    #[error("Die lokale Modellinstallation konnte nicht geprüft werden.")]
    ModelAccess,
    #[error("Die Modellinstallation wird gerade geändert. Bitte die Erkennung erneut versuchen.")]
    ModelBusy,
    #[error("Der Modellpfad kann nicht an die Erkennung übergeben werden.")]
    InvalidPath,
    #[error("Die lokale Spracherkennung konnte nicht geladen oder ausgeführt werden.")]
    Recognition,
    #[error("Die lokale Sprachaktivitätserkennung ist fehlgeschlagen.")]
    Vad,
    #[error("Die Leerlaufüberwachung konnte nicht gestartet werden.")]
    IdleWorker,
}

#[cfg(test)]
mod tests;
