use std::collections::HashMap;
use std::error::Error;
use std::fmt::{self, Display};
use std::str::FromStr;
use std::sync::OnceLock;

pub(crate) const CTRL: u8 = 1;
pub(crate) const SHIFT: u8 = 2;
pub(crate) const ALT: u8 = 4;
pub(crate) const WIN: u8 = 8;

// The first name is used for display. Key codes are Win32 virtual-key codes.
const MODIFIERS: &[(u8, &[&str])] = &[
    (CTRL, &["Strg", "Ctrl", "Control"]),
    (SHIFT, &["Umschalt", "Shift"]),
    (ALT, &["Alt"]),
    (WIN, &["Win", "Windows"]),
];
const NAMED_KEYS: &[(u32, &[&str])] = &[
    (0x08, &["Rücktaste", "Backspace"]),
    (0x09, &["Tab", "Tabulator"]),
    (0x0d, &["Eingabe", "Enter", "Return"]),
    (0x1b, &["Esc", "Escape"]),
    (0x20, &["Leertaste", "Space"]),
    (0x21, &["Bild auf", "PageUp"]),
    (0x22, &["Bild ab", "PageDown"]),
    (0x23, &["Ende", "End"]),
    (0x24, &["Pos1", "Home"]),
    (0x25, &["Links", "Left"]),
    (0x26, &["Oben", "Up"]),
    (0x27, &["Rechts", "Right"]),
    (0x28, &["Unten", "Down"]),
    (0x2d, &["Einfg", "Insert"]),
    (0x2e, &["Entf", "Delete"]),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hotkey {
    modifiers: u8,
    trigger: Option<u32>,
}

impl Hotkey {
    pub(crate) fn modifiers(self) -> u8 {
        self.modifiers
    }

    pub(crate) fn trigger(self) -> Option<u32> {
        self.trigger
    }

    /// Checks the bundled defaults and layout risks when configuring a chord.
    pub fn warnings(self) -> Vec<HotkeyWarning> {
        let mut warnings = Vec::new();
        if let Some(info) = known_shortcuts().get(&self.to_string()) {
            warnings.push(HotkeyWarning::KnownShortcut(info));
        }
        if self.trigger.is_none() {
            warnings.push(HotkeyWarning::ModifiersOnly);
        }
        if self.modifiers & (CTRL | ALT) == (CTRL | ALT) {
            warnings.push(HotkeyWarning::AltGr);
        }
        warnings
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyWarning {
    KnownShortcut(&'static str),
    ModifiersOnly,
    AltGr,
}

impl Display for HotkeyWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::KnownShortcut(info) => info,
            Self::ModifiersOnly => "Das Kürzel besteht nur aus Sondertasten und kann beim Drücken anderer Kürzel auslösen.",
            Self::AltGr => "Strg+Alt kann bei deutscher Tastenbelegung mit AltGr kollidieren.",
        })
    }
}

fn known_shortcuts() -> &'static HashMap<String, String> {
    static SHORTCUTS: OnceLock<HashMap<String, String>> = OnceLock::new();
    SHORTCUTS.get_or_init(|| {
        toml::from_str(include_str!("../conflicts.toml")).expect("invalid bundled conflicts.toml")
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseHotkeyError {
    EmptyKey,
    UnknownKey,
    DuplicateModifier,
    MultipleTriggers,
}

impl Display for ParseHotkeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EmptyKey => "Im Tastenkürzel fehlt ein Tastenname.",
            Self::UnknownKey => "Unbekannter Tastenname im Tastenkürzel.",
            Self::DuplicateModifier => "Das Tastenkürzel enthält dieselbe Sondertaste mehrfach.",
            Self::MultipleTriggers => {
                "Neben Strg, Umschalt, Alt und Win ist nur eine weitere Taste möglich."
            }
        })
    }
}

impl Error for ParseHotkeyError {}

impl FromStr for Hotkey {
    type Err = ParseHotkeyError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let mut modifiers = 0;
        let mut trigger = None;
        for token in input.split('+') {
            let name = token.trim().to_lowercase();
            if name.is_empty() {
                return Err(ParseHotkeyError::EmptyKey);
            }
            if let Some((bit, _)) = MODIFIERS
                .iter()
                .find(|(_, names)| matches_name(&name, names))
            {
                if modifiers & bit != 0 {
                    return Err(ParseHotkeyError::DuplicateModifier);
                }
                modifiers |= bit;
            } else {
                let vk = parse_trigger(&name).ok_or(ParseHotkeyError::UnknownKey)?;
                if trigger.replace(vk).is_some() {
                    return Err(ParseHotkeyError::MultipleTriggers);
                }
            }
        }
        Ok(Self { modifiers, trigger })
    }
}

fn matches_name(lowercase: &str, names: &[&str]) -> bool {
    names
        .iter()
        .any(|name| name.eq_ignore_ascii_case(lowercase))
}

fn parse_trigger(name: &str) -> Option<u32> {
    if let [byte] = name.as_bytes()
        && byte.is_ascii_alphanumeric()
    {
        return Some(u32::from(byte.to_ascii_uppercase()));
    }
    if let Some(suffix) = name.strip_prefix('f') {
        if suffix.starts_with('0') || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let number: u32 = suffix.parse().ok()?;
        return (1..=24).contains(&number).then(|| 0x70 + number - 1);
    }
    NAMED_KEYS
        .iter()
        .find(|(_, names)| matches_name(name, names))
        .map(|(vk, _)| *vk)
}

impl Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut separator = "";
        for &(bit, names) in MODIFIERS {
            if self.modifiers & bit != 0 {
                write!(f, "{separator}{}", names[0])?;
                separator = "+";
            }
        }
        if let Some(vk) = self.trigger {
            f.write_str(separator)?;
            match vk {
                0x30..=0x39 | 0x41..=0x5a => write!(f, "{}", char::from(vk as u8))?,
                0x70..=0x87 => write!(f, "F{}", vk - 0x70 + 1)?,
                _ => {
                    let (_, names) = NAMED_KEYS
                        .iter()
                        .find(|(code, _)| *code == vk)
                        .ok_or(fmt::Error)?;
                    f.write_str(names[0])?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "binding_tests.rs"]
mod tests;
