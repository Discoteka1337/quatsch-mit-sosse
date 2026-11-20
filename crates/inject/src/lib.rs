//! Insert dictated text into the foreground Windows application.

#[cfg(windows)]
mod clipboard;
mod config;
#[cfg(windows)]
mod keyboard;
#[cfg(windows)]
mod native;
#[cfg(windows)]
mod uia;

pub use config::InjectConfig;
#[cfg(windows)]
use native::Target;
#[cfg(windows)]
pub use native::insert;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
pub enum InjectMethod {
    #[serde(rename = "uia")]
    UiAutomation,
    #[serde(rename = "unicode")]
    Unicode,
    #[serde(rename = "clipboard")]
    Clipboard,
}

impl std::fmt::Display for InjectMethod {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::UiAutomation => "UI Automation",
            Self::Unicode => "Unicode-Tastatureingabe",
            Self::Clipboard => "Zwischenablage",
        })
    }
}

/// Errors contain no dictated text or clipboard data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum InjectError {
    #[error("Der Text ist leer oder enthält ein Nullzeichen.")]
    InvalidText,
    #[error("Die Konfigurationsdatei für das Einfügen ist ungültig.")]
    InvalidConfig,
    #[error("Die Konfigurationsdatei für das Einfügen konnte nicht gelesen werden.")]
    ConfigRead,
    #[error("Das Vordergrundfenster oder sein Prozess konnte nicht bestimmt werden.")]
    TargetUnavailable,
    #[error("Das Vordergrundfenster oder der Eingabefokus hat gewechselt.")]
    TargetChanged,
    #[error("Das Eingabefeld ist schreibgeschützt.")]
    ReadOnly,
    #[error("Eine Modifikatortaste ist noch gedrückt.")]
    ModifiersHeld,
    #[error("Der Einfügeweg ist für dieses Ziel nicht verfügbar.")]
    Unavailable,
    #[error("Der Text wurde möglicherweise nur teilweise eingefügt.")]
    DeliveryUncertain,
    #[error("Die Zwischenablage enthält ein Format, das sich nicht verlustfrei sichern lässt.")]
    ClipboardUnsupported,
    #[error("Während des Einfügens wurde etwas anderes kopiert. Die neuere Kopie bleibt erhalten.")]
    ClipboardChanged,
    #[error(
        "Der ursprüngliche Inhalt der Zwischenablage konnte nicht vollständig wiederhergestellt werden."
    )]
    ClipboardRestore,
    #[error("Ein anderer Einfügevorgang läuft bereits.")]
    Busy,
}

#[cfg(any(windows, test))]
fn is_long(text: &str) -> bool {
    text.chars().nth(2000).is_some()
}

#[cfg(any(windows, test))]
fn route(
    text: &str,
    forced: Option<InjectMethod>,
    mut attempt: impl FnMut(InjectMethod) -> Result<(), InjectError>,
) -> Result<InjectMethod, InjectError> {
    use InjectMethod::{Clipboard, UiAutomation, Unicode};
    if text.is_empty() || text.contains('\0') {
        return Err(InjectError::InvalidText);
    }
    let forced = if is_long(text) {
        Some(Clipboard)
    } else {
        forced
    };
    let methods = match &forced {
        Some(method) => std::slice::from_ref(method),
        None => &[UiAutomation, Unicode, Clipboard][..],
    };
    for &method in methods {
        match attempt(method) {
            Ok(()) => return Ok(method),
            Err(InjectError::Unavailable) => continue,
            Err(error) => return Err(error),
        }
    }
    Err(InjectError::Unavailable)
}

#[cfg(test)]
mod tests;
