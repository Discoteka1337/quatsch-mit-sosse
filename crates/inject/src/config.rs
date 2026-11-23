use crate::{InjectError, InjectMethod};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{ErrorKind, Read};
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
        const MAX_BYTES: u64 = 1024 * 1024;
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Self::default()),
            Err(_) => return Err(InjectError::ConfigRead),
        };
        let mut bytes = Vec::new();
        file.take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| InjectError::ConfigRead)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(InjectError::ConfigTooLarge);
        }
        std::str::from_utf8(&bytes)
            .map_err(|_| InjectError::ConfigRead)?
            .parse()
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
