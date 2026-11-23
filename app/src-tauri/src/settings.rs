use crate::overlay_placement::OverlayPreferences;
use quatsch_mit_sosse_hotkey::{HotkeyConfig, HotkeyMode};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub(super) mod autostart;

const SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum FillerMode {
    Off,
    #[default]
    Standard,
    Strong,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct CleanupSettings {
    pub voice_commands: bool,
    pub dictionary: bool,
    pub filler_words: FillerMode,
    pub self_corrections: bool,
    pub numbers: bool,
    pub digits_from: u32,
    pub punctuation: bool,
    pub snippets: bool,
}

impl Default for CleanupSettings {
    fn default() -> Self {
        Self {
            voice_commands: false,
            dictionary: true,
            filler_words: FillerMode::Standard,
            self_corrections: true,
            numbers: true,
            digits_from: 13,
            punctuation: true,
            snippets: true,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "HotkeyMode", rename_all = "lowercase")]
enum Mode {
    Hold,
    Toggle,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct Settings {
    pub dictation_hotkey: String,
    pub reinsert_hotkey: String,
    #[serde(with = "Mode")]
    pub mode: HotkeyMode,
    pub microphone: Option<String>,
    pub model: Option<String>,
    pub tones: bool,
    pub volume_percent: u8,
    pub autostart: bool,
    pub overlay: OverlayPreferences,
    pub cleanup: CleanupSettings,
}

impl Default for Settings {
    fn default() -> Self {
        let keys = HotkeyConfig::default();
        Self {
            dictation_hotkey: keys.dictation.to_string(),
            reinsert_hotkey: keys.reinsert.to_string(),
            mode: keys.mode,
            microphone: None,
            model: None,
            tones: true,
            volume_percent: 20,
            autostart: false,
            overlay: OverlayPreferences::default(),
            cleanup: CleanupSettings::default(),
        }
    }
}

impl Settings {
    pub(crate) fn hotkeys(&self) -> io::Result<HotkeyConfig> {
        let config = HotkeyConfig {
            dictation: self.dictation_hotkey.parse().map_err(io::Error::other)?,
            reinsert: self.reinsert_hotkey.parse().map_err(io::Error::other)?,
            mode: self.mode,
        };
        config.validate()?;
        Ok(config)
    }

    pub(super) fn validate(&self) -> io::Result<()> {
        self.hotkeys()?;
        if self.volume_percent > 100 {
            return Err(io::Error::other(
                "Die Lautstärke muss zwischen 0 und 100 % liegen.",
            ));
        }
        if let Some(name) = &self.microphone
            && (name.trim().is_empty() || name.contains('\0'))
        {
            return Err(io::Error::other("Der Mikrofonname ist ungültig."));
        }
        if let Some(id) = self.model.as_deref()
            && crate::speech::asr_id(id).is_none()
        {
            return Err(io::Error::other("Das gewählte Sprachmodell ist unbekannt."));
        }
        Ok(())
    }
}

pub(crate) fn data_dir() -> io::Result<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| io::Error::other("Der lokale Datenordner ist nicht verfügbar."))?;
    Ok(local.join("QuatschMitSosse").join("data"))
}

pub(crate) struct SettingsStore {
    path: PathBuf,
    current: Settings,
}

impl SettingsStore {
    pub(crate) fn open() -> io::Result<Self> {
        Self::load(&data_dir()?)
    }

    pub(super) fn load(dir: &Path) -> io::Result<Self> {
        let path = dir.join("settings.json");
        let (current, migrated) = match crate::config_file::read(&path) {
            Ok(bytes) => Self::decode(&bytes)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let overlay = legacy_overlay(dir)?;
                (
                    Settings {
                        overlay,
                        ..Settings::default()
                    },
                    true,
                )
            }
            Err(e) => return Err(e),
        };
        current.validate()?;
        if migrated {
            fs::create_dir_all(dir)?;
            stage(&path, &current)?.persist(&path)?;
        }
        Ok(Self { path, current })
    }

    fn decode(bytes: &[u8]) -> io::Result<(Settings, bool)> {
        let mut fields: serde_json::Map<String, serde_json::Value> = serde_json::from_slice(bytes)?;
        let version: u32 = fields
            .remove("schemaVersion")
            .map(serde_json::from_value)
            .transpose()?
            .unwrap_or(0);
        if version > SCHEMA_VERSION {
            return Err(io::Error::other(
                "Diese Einstellungen erfordern eine neuere App-Version.",
            ));
        }
        let current = serde_json::from_value(fields.into())?;
        Ok((current, version < SCHEMA_VERSION))
    }

    pub(crate) fn current(&self) -> &Settings {
        &self.current
    }

    pub(crate) fn save(&mut self, next: Settings) -> io::Result<()> {
        self.save_with(next, autostart::set)
    }

    fn save_with(
        &mut self,
        next: Settings,
        mut set_autostart: impl FnMut(bool) -> io::Result<()>,
    ) -> io::Result<()> {
        next.validate()?;
        if self.current == next {
            return Ok(());
        }
        let file = stage(&self.path, &next)?;
        let login_changed = next.autostart != self.current.autostart;
        if login_changed {
            set_autostart(next.autostart)?;
        }
        if let Err(e) = file.persist(&self.path) {
            if login_changed {
                set_autostart(self.current.autostart)?;
            }
            return Err(e.error);
        }
        self.current = next;
        Ok(())
    }
}

fn legacy_overlay(dir: &Path) -> io::Result<OverlayPreferences> {
    #[derive(Deserialize)]
    struct LegacyOverlay {
        #[serde(default)]
        preferences: OverlayPreferences,
    }
    match crate::config_file::read(&dir.join("overlay.json")) {
        Ok(bytes) => Ok(serde_json::from_slice::<LegacyOverlay>(&bytes)?.preferences),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(OverlayPreferences::default()),
        Err(e) => Err(e),
    }
}

fn stage(path: &Path, settings: &Settings) -> io::Result<tempfile::NamedTempFile> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Document<'a> {
        schema_version: u32,
        #[serde(flatten)]
        settings: &'a Settings,
    }
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::other("Der Datenordner fehlt."))?;
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    serde_json::to_writer_pretty(
        file.as_file_mut(),
        &Document {
            schema_version: SCHEMA_VERSION,
            settings,
        },
    )?;
    crate::config_file::sync(file.as_file())?;
    Ok(file)
}

#[cfg(test)]
mod tests;
