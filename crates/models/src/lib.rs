mod manager;
mod manifest;
mod net;

pub use manager::{Manager, Progress, ProgressPhase};
pub use manifest::{ArchiveFormat, Artifact, Manifest, Model, ModelFile, ModelKind};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Dateizugriff fehlgeschlagen: {0}")]
    Io(#[from] std::io::Error),
    #[error("Ungültiges Modellmanifest: {0}")]
    Manifest(String),
    #[error("Unbekanntes Modell: {0}")]
    UnknownModel(String),
    #[error("Installation abgebrochen; der Download kann fortgesetzt werden.")]
    Cancelled,
    #[error("Modelldownload fehlgeschlagen: {0}")]
    Network(String),
    #[error("Downloadhost ist nicht im Manifest freigegeben: {0}")]
    HostNotAllowed(String),
    #[error("Die SHA256-Prüfsumme stimmt nicht; die Installationsdateien wurden gelöscht.")]
    ChecksumMismatch,
    #[error("Ungültiges Modellarchiv: {0}")]
    InvalidArchive(String),
    #[error("Dieses Modell wird bereits in einem anderen Vorgang bearbeitet.")]
    Busy,
    #[error("LOCALAPPDATA fehlt oder enthält keinen absoluten Pfad.")]
    UnsupportedPlatform,
}

#[cfg(test)]
mod tests;
