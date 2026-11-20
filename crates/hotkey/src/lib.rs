//! Global dictation shortcuts. Consumers receive actions, never a stream of typed keys.

mod binding;
#[cfg(windows)]
mod integrity;
#[cfg(any(windows, test))]
mod machine;
#[cfg(windows)]
mod native;

pub use binding::{Hotkey, HotkeyWarning, ParseHotkeyError};
#[cfg(windows)]
pub use native::{HookNotice, HotkeyHook};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyMode {
    Hold,
    Toggle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HotkeyConfig {
    pub dictation: Hotkey,
    pub reinsert: Hotkey,
    pub mode: HotkeyMode,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            dictation: "Strg+Umschalt+Leertaste".parse().unwrap(),
            reinsert: "Strg+Win+F8".parse().unwrap(),
            mode: HotkeyMode::Hold,
        }
    }
}

impl HotkeyConfig {
    /// Validates both actions together. Check each binding's warnings separately.
    pub fn validate(&self) -> std::io::Result<()> {
        let message = if self.dictation == self.reinsert {
            "Für Diktat und erneutes Einfügen sind zwei verschiedene Kürzel erforderlich."
        } else if [self.dictation, self.reinsert]
            .iter()
            .any(|key| key.trigger() == Some(0x1b))
        {
            "Esc ist für den Abbruch einer Aufnahme reserviert."
        } else {
            return Ok(());
        };
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            message,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyEvent {
    Start,
    /// Complete the recording for insertion.
    Stop,
    /// Discard the recording, including any pending insertion.
    Cancel,
    ReinsertLast,
    /// The foreground process has a higher integrity level than this process.
    ElevatedTarget,
}

#[cfg(test)]
mod tests;
