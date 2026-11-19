use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{Error, Result, net};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub version: u32,
    pub allowed_hosts: Vec<String>,
    pub models: Vec<Model>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    pub id: String,
    pub display_name: String,
    pub languages: Vec<String>,
    pub kind: ModelKind,
    pub license: String,
    pub attribution: String,
    pub artifacts: Vec<Artifact>,
}

impl Model {
    pub fn download_size(&self) -> u64 {
        self.artifacts.iter().map(|artifact| artifact.size).sum()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelKind {
    Asr,
    Vad,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub urls: Vec<String>,
    pub archive: ArchiveFormat,
    pub sha256: String,
    pub size: u64,
    pub files: Vec<ModelFile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum ArchiveFormat {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "tar.bz2")]
    TarBz2,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelFile {
    pub path: String,
    #[serde(default)]
    pub archive_path: String,
    pub size: u64,
}

impl Manifest {
    pub fn parse(contents: &str) -> Result<Self> {
        let mut manifest: Self = toml::from_str(contents)
            .map_err(|_| Error::Manifest("TOML-Struktur ist nicht lesbar.".into()))?;
        for model in &mut manifest.models {
            for artifact in &mut model.artifacts {
                for file in &mut artifact.files {
                    if file.archive_path.is_empty() {
                        file.archive_path.clone_from(&file.path);
                    }
                }
            }
        }
        manifest.validate()?;
        Ok(manifest)
    }

    pub(crate) fn validate(&self) -> Result<()> {
        let invalid = |message: &str| Error::Manifest(message.to_owned());
        if self.version != 1 || self.models.is_empty() || self.allowed_hosts.is_empty() {
            return Err(invalid("Version, Modelle oder Hostliste fehlen."));
        }
        let mut hosts = HashSet::new();
        for host in &self.allowed_hosts {
            if host.is_empty()
                || !host
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'.' || c == b'-')
                || host.starts_with('.')
                || host.ends_with('.')
                || host.contains("..")
                || !hosts.insert(host)
            {
                return Err(invalid(
                    "Die Hostliste ist ungültig oder enthält Duplikate.",
                ));
            }
        }
        let mut ids = HashSet::new();
        for model in &self.models {
            if model.id.is_empty()
                || !model
                    .id
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
                || !safe_relative_path(&model.id)
                || !ids.insert(&model.id)
                || model.display_name.trim().is_empty()
                || model.languages.is_empty()
                || model
                    .languages
                    .iter()
                    .any(|language| language.trim().is_empty())
                || model.license.trim().is_empty()
                || model.attribution.trim().is_empty()
                || model.artifacts.is_empty()
            {
                return Err(invalid("Ungültige oder doppelte Modellmetadaten."));
            }
            let mut paths = HashSet::new();
            let mut total = 0u64;
            for artifact in &model.artifacts {
                total = total
                    .checked_add(artifact.size)
                    .ok_or_else(|| invalid("Downloadgröße ist zu groß."))?;
                if artifact.urls.is_empty()
                    || artifact.size == 0
                    || artifact.files.is_empty()
                    || artifact.sha256.len() != 64
                    || !artifact
                        .sha256
                        .bytes()
                        .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
                {
                    return Err(invalid("Downloadgröße, SHA256 oder Dateien sind ungültig."));
                }
                for url in &artifact.urls {
                    net::validate_url(url, &self.allowed_hosts)?;
                }
                let mut sources = HashSet::new();
                for file in &artifact.files {
                    let normalized = file.path.to_ascii_lowercase();
                    if !safe_relative_path(&file.path)
                        || !safe_relative_path(&file.archive_path)
                        || file.path.split('/').any(|part| part.starts_with('.'))
                        || !paths.insert(normalized)
                        || !sources.insert(&file.archive_path)
                        || file.size == 0
                    {
                        return Err(invalid("Unsicherer, doppelter oder leerer Dateieintrag."));
                    }
                }
                if artifact.archive == ArchiveFormat::None
                    && (artifact.files.len() != 1 || artifact.files[0].size != artifact.size)
                {
                    return Err(invalid(
                        "Ein unverpackter Download muss genau eine Datei gleicher Größe enthalten.",
                    ));
                }
            }
            for path in &paths {
                if paths
                    .iter()
                    .any(|other| other.starts_with(&format!("{path}/")))
                {
                    return Err(invalid("Datei- und Verzeichnispfade überschneiden sich."));
                }
            }
        }
        Ok(())
    }
}

pub(crate) fn safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && path.is_ascii()
        && !path.contains(['\\', ':', '\0'])
        && path.split('/').all(|part| {
            if part.is_empty()
                || part == "."
                || part == ".."
                || part.ends_with(['.', ' '])
                || part.bytes().any(|c| c < 32 || b"<>\"|?*".contains(&c))
            {
                return false;
            }
            let stem = part
                .split('.')
                .next()
                .unwrap_or_default()
                .to_ascii_uppercase();
            !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                && !(stem.len() == 4
                    && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        })
}
