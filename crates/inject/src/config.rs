use crate::{InjectError, InjectMethod};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Default, Debug)]
pub struct InjectConfig {
    pub(crate) apps: BTreeMap<String, InjectMethod>,
}

impl InjectConfig {
    /// Load the optional process table from the application's local data directory.
    pub fn load() -> Result<Self, InjectError> {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .ok_or(InjectError::ConfigRead)?;
        Self::load_from(
            &base
                .join("QuatschMitSosse")
                .join("data")
                .join("inject.toml"),
        )
    }

    pub fn load_from(path: &Path) -> Result<Self, InjectError> {
        match std::fs::read_to_string(path) {
            Ok(source) => source.parse(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(_) => Err(InjectError::ConfigRead),
        }
    }

    pub fn method_for(&self, process_name: &str) -> Option<InjectMethod> {
        self.apps.get(&process_name.to_lowercase()).copied()
    }
}

impl FromStr for InjectConfig {
    type Err = InjectError;

    fn from_str(source: &str) -> Result<Self, InjectError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct FileConfig {
            #[serde(default)]
            apps: BTreeMap<String, InjectMethod>,
        }

        let raw: FileConfig = toml::from_str(source).map_err(|_| InjectError::InvalidConfig)?;
        let mut config = Self::default();
        for (name, method) in raw.apps {
            if name.trim().is_empty() || name.contains(['/', '\\', ':', '\0']) {
                return Err(InjectError::InvalidConfig);
            }
            if config.apps.insert(name.to_lowercase(), method).is_some() {
                return Err(InjectError::InvalidConfig);
            }
        }
        Ok(config)
    }
}
