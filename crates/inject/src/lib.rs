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

#[cfg(windows)]
pub use clipboard::{clipboard_text, copy_recovered_text};
pub use config::InjectConfig;
#[cfg(windows)]
use native::InsertTarget as Target;
#[cfg(windows)]
pub use native::{InsertTarget, insert};

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
    #[error("Die Datei inject.toml überschreitet die Größenbegrenzung von 1 MiB.")]
    ConfigTooLarge,
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
    #[error(
        "Das Einfügen über die Zwischenablage ist in Word und im klassischen Outlook gesperrt, weil Strg+V die vorige Kopie einfügen kann. Nutze die Unicode-Tastatureingabe oder „Kopieren“ im Overlay."
    )]
    WordClipboardUnavailable,
    #[error("Während des Einfügens wurde etwas anderes kopiert. Die neuere Kopie bleibt erhalten.")]
    ClipboardChanged,
    #[error(
        "Der ursprüngliche Inhalt der Zwischenablage konnte nicht vollständig wiederhergestellt werden."
    )]
    ClipboardRestore,
    #[error(
        "Zwischenablage belegt. Lass die App zur Wiederherstellung geöffnet und prüfe das Einfügeergebnis im Zielfeld."
    )]
    ClipboardRecoveryPending,
    #[error("Ein anderer Einfügevorgang läuft bereits.")]
    Busy,
}

#[cfg(any(windows, test))]
fn validate_text(text: &str) -> Result<(), InjectError> {
    if text.is_empty() || text.contains('\0') {
        Err(InjectError::InvalidText)
    } else {
        Ok(())
    }
}

#[cfg(any(windows, test))]
fn is_long(text: &str) -> bool {
    text.chars().nth(2000).is_some()
}

#[cfg(any(windows, test))]
fn uses_word_editor(exe: &str) -> bool {
    exe.eq_ignore_ascii_case("winword.exe") || exe.eq_ignore_ascii_case("outlook.exe")
}

#[cfg(any(windows, test))]
fn route(
    text: &str,
    config: &InjectConfig,
    process_name: impl FnOnce() -> Result<String, InjectError>,
    mut attempt: impl FnMut(InjectMethod) -> Result<(), InjectError>,
) -> Result<InjectMethod, InjectError> {
    use InjectMethod::{Clipboard, UiAutomation, Unicode};
    validate_text(text)?;
    // Short automatic UIA/Unicode insertions need no process handle.
    let process = std::cell::LazyCell::new(process_name);
    let executable = || process.as_deref().map_err(|&error| error);
    let configured = if config.apps.is_empty() {
        None
    } else {
        config.method_for(executable()?)
    };
    let preferred = if is_long(text) {
        let exe = executable()?;
        let method = if uses_word_editor(exe)
            || exe.eq_ignore_ascii_case("chrome.exe")
            || exe.eq_ignore_ascii_case("msedge.exe")
        {
            configured.unwrap_or(Unicode)
        } else {
            Clipboard
        };
        Some(method)
    } else {
        configured
    };
    let methods = match &preferred {
        Some(method) => std::slice::from_ref(method),
        None => &[UiAutomation, Unicode, Clipboard][..],
    };
    for &method in methods {
        // Office can read CF_UNICODETEXT for a clipboard preview before the
        // queued Ctrl+V runs. Restoring then pastes the user's previous copy.
        if method == Clipboard && uses_word_editor(executable()?) {
            return Err(InjectError::WordClipboardUnavailable);
        }
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
