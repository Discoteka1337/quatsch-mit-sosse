use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use bzip2::read::BzDecoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    ArchiveFormat, Artifact, Error, Manifest, Model, Result, manifest::safe_relative_path, net,
};

const MARKER: &str = ".installed.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressPhase {
    Downloading,
    Verifying,
    Extracting,
    Complete,
}

#[derive(Debug, Clone)]
pub struct Progress {
    pub model_id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub phase: ProgressPhase,
}

pub struct Manager {
    root: PathBuf,
    pub(crate) manifest: Manifest,
}

/// Keeps a complete model installation available while its files are opened.
pub struct ModelLease {
    directory: PathBuf,
    _lock: File,
}

impl ModelLease {
    pub fn path(&self) -> &Path {
        &self.directory
    }
}

enum LockMode {
    Shared,
    Exclusive,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Installed {
    version: u32,
    manifest_sha256: String,
}

impl Manager {
    pub fn new() -> Result<Self> {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .ok_or(Error::UnsupportedPlatform)?;
        Self::with_root(base.join("QuatschMitSosse").join("models"))
    }

    pub fn with_root(root: PathBuf) -> Result<Self> {
        Self::from_manifest(root, Manifest::parse(include_str!("../manifest.toml"))?)
    }

    pub(crate) fn from_manifest(root: PathBuf, manifest: Manifest) -> Result<Self> {
        if !root.is_absolute() {
            return Err(Error::Manifest(
                "Der Modellordner muss ein absoluter Pfad sein.".into(),
            ));
        }
        manifest.validate()?;
        Ok(Self { root, manifest })
    }

    pub fn list(&self) -> &[Model] {
        &self.manifest.models
    }

    pub fn is_installed(&self, id: &str) -> Result<bool> {
        let model = self.model(id)?;
        let destination = self.root.join(id);
        self.check_path(&destination)?;
        let marker = destination.join(MARKER);
        self.check_path(&marker)?;
        let contents = match fs::read(&marker) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        let Ok(installed) = serde_json::from_slice::<Installed>(&contents) else {
            return Ok(false);
        };
        if installed.version != 1 || installed.manifest_sha256 != fingerprint(model)? {
            return Ok(false);
        }
        for artifact in &model.artifacts {
            for expected in &artifact.files {
                let file = destination.join(&expected.path);
                self.check_path(&file)?;
                match fs::metadata(file) {
                    Ok(metadata) if metadata.is_file() && metadata.len() == expected.size => {}
                    Ok(_) => return Ok(false),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
                    Err(error) => return Err(error.into()),
                }
            }
        }
        Ok(true)
    }

    pub fn path_of(&self, id: &str) -> Result<Option<PathBuf>> {
        Ok(self.is_installed(id)?.then(|| self.root.join(id)))
    }

    pub fn acquire(&self, id: &str) -> Result<Option<ModelLease>> {
        self.model(id)?;
        if !self.root.try_exists()? {
            return Ok(None);
        }
        let lock = self.lock(id, LockMode::Shared)?;
        Ok(self.path_of(id)?.map(|directory| ModelLease {
            directory,
            _lock: lock,
        }))
    }

    pub fn install(
        &self,
        id: &str,
        cancel: &AtomicBool,
        mut progress: impl FnMut(Progress),
    ) -> Result<PathBuf> {
        let model = self.model(id)?;
        let _lock = self.lock(id, LockMode::Exclusive)?;
        let destination = self.root.join(id);
        let downloads = self.root.join(".downloads").join(id);
        let total = model.download_size();
        let mut report = |downloaded_bytes, phase| {
            progress(Progress {
                model_id: id.to_owned(),
                downloaded_bytes,
                total_bytes: total,
                phase,
            })
        };
        if self.is_installed(id)? {
            // A previous process may have exited between publication and cache cleanup.
            self.clear_directory(&downloads)?;
            report(total, ProgressPhase::Complete);
            return Ok(destination);
        }
        check_cancelled(cancel)?;
        let staging = self.root.join(".staging").join(id);
        self.check_path(&downloads)?;
        self.check_path(&staging)?;
        self.clear_directory(&staging)?;
        let expected_fingerprint = fingerprint(model)?;
        let cache_manifest = downloads.join("manifest.sha256");
        self.check_path(&cache_manifest)?;
        match fs::read_to_string(&cache_manifest) {
            Ok(previous) if previous != expected_fingerprint => self.clear_directory(&downloads)?,
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
        fs::create_dir_all(&downloads)?;
        fs::write(&cache_manifest, &expected_fingerprint)?;
        let outcome = (|| {
            let mut completed_bytes = 0;
            for (index, artifact) in model.artifacts.iter().enumerate() {
                let cached = downloads.join(format!("{index}.part"));
                self.check_path(&cached)?;
                net::download(
                    artifact,
                    &cached,
                    &self.manifest.allowed_hosts,
                    cancel,
                    &mut |bytes| report(completed_bytes + bytes, ProgressPhase::Downloading),
                )?;
                let mut input = open_artifact(&cached)?;
                report(completed_bytes + artifact.size, ProgressPhase::Verifying);
                if file_sha256(&mut input, cancel)? != artifact.sha256 {
                    return Err(Error::ChecksumMismatch);
                }
                input.rewind()?;
                check_cancelled(cancel)?;
                fs::create_dir_all(&staging)?;
                report(completed_bytes + artifact.size, ProgressPhase::Extracting);
                extract(artifact, input, &staging, cancel)?;
                completed_bytes += artifact.size;
            }
            check_cancelled(cancel)?;
            let marker = Installed {
                version: 1,
                manifest_sha256: expected_fingerprint,
            };
            let encoded = serde_json::to_vec(&marker).map_err(|_| {
                Error::Manifest("Installationsstatus konnte nicht gespeichert werden.".into())
            })?;
            let mut marker_file = File::create(staging.join(MARKER))?;
            marker_file.write_all(&encoded)?;
            marker_file.sync_all()?;
            drop(marker_file);
            self.clear_directory(&destination)?;
            fs::rename(&staging, &destination)?;
            Ok(())
        })();
        if let Err(error) = outcome {
            self.clear_directory(&staging)?;
            if matches!(error, Error::ChecksumMismatch | Error::InvalidArchive(_)) {
                self.clear_directory(&downloads)?;
                self.clear_directory(&destination)?;
            }
            return Err(error);
        }
        self.clear_directory(&downloads)?;
        report(total, ProgressPhase::Complete);
        Ok(destination)
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        self.model(id)?;
        let _lock = self.lock(id, LockMode::Exclusive)?;
        for path in [
            self.root.join(id),
            self.root.join(".downloads").join(id),
            self.root.join(".staging").join(id),
        ] {
            self.clear_directory(&path)?;
        }
        Ok(())
    }

    fn model(&self, id: &str) -> Result<&Model> {
        self.manifest
            .models
            .iter()
            .find(|model| model.id == id)
            .ok_or(Error::UnknownModel)
    }

    fn lock(&self, id: &str, mode: LockMode) -> Result<File> {
        let locks = self.root.join(".locks");
        self.check_path(&locks)?;
        fs::create_dir_all(&locks)?;
        let path = locks.join(format!("{id}.lock"));
        self.check_path(&path)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        let result = match mode {
            LockMode::Shared => fs2::FileExt::try_lock_shared(&file),
            LockMode::Exclusive => fs2::FileExt::try_lock_exclusive(&file),
        };
        result.map_err(|error| {
            if error.kind() == std::io::ErrorKind::WouldBlock || error.raw_os_error() == Some(33) {
                Error::Busy
            } else {
                Error::Io(error)
            }
        })?;
        Ok(file)
    }

    fn check_path(&self, path: &Path) -> Result<()> {
        if !path.starts_with(&self.root) {
            return Err(unsafe_path());
        }
        for ancestor in path.ancestors() {
            match fs::symlink_metadata(ancestor) {
                Ok(metadata) if is_link(&metadata) => return Err(unsafe_path()),
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    fn clear_directory(&self, path: &Path) -> Result<()> {
        self.check_path(path)?;
        if !path.try_exists()? {
            return Ok(());
        }
        reject_links_recursively(path)?;
        fs::remove_dir_all(path)?;
        Ok(())
    }
}

fn unsafe_path() -> Error {
    Error::Manifest("Der Modellordner enthält einen unsicheren oder verknüpften Pfad.".into())
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn reject_links_recursively(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if is_link(&metadata) {
        return Err(unsafe_path());
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            reject_links_recursively(&entry?.path())?;
        }
    }
    Ok(())
}

fn fingerprint(model: &Model) -> Result<String> {
    let bytes = serde_json::to_vec(model)
        .map_err(|_| Error::Manifest("Modellmetadaten sind nicht lesbar.".into()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn check_cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}

fn open_artifact(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 1;
        options.share_mode(FILE_SHARE_READ);
    }
    Ok(options.open(path)?)
}

fn file_sha256(file: &mut File, cancel: &AtomicBool) -> Result<String> {
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        check_cancelled(cancel)?;
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn copy_checked(
    mut input: impl Read,
    destination: &Path,
    expected_size: u64,
    cancel: &AtomicBool,
) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)?;
    let mut buffer = [0; 64 * 1024];
    let mut total = 0u64;
    loop {
        check_cancelled(cancel)?;
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > expected_size {
            return Err(Error::InvalidArchive(
                "Eine Datei überschreitet die angegebene Größe.".into(),
            ));
        }
        output.write_all(&buffer[..read])?;
    }
    if total != expected_size {
        return Err(Error::InvalidArchive(
            "Eine Datei hat eine unerwartete Größe.".into(),
        ));
    }
    output.sync_all()?;
    Ok(())
}

fn extract(artifact: &Artifact, input: File, staging: &Path, cancel: &AtomicBool) -> Result<()> {
    if artifact.archive == ArchiveFormat::None {
        let file = &artifact.files[0];
        return copy_checked(input, &staging.join(&file.path), file.size, cancel);
    }
    let decoder = BzDecoder::new(input);
    let mut archive = tar::Archive::new(decoder);
    let mut found = std::collections::HashSet::new();
    let invalid =
        || Error::InvalidArchive("Archivstruktur, Pfad oder Dateityp ist ungültig.".into());
    for entry in archive.entries().map_err(|_| invalid())? {
        check_cancelled(cancel)?;
        let mut entry = entry.map_err(|_| invalid())?;
        let path = entry.path().map_err(|_| invalid())?;
        let path = path.to_str().ok_or_else(invalid)?;
        let kind = entry.header().entry_type();
        let path = path.trim_end_matches('/');
        if !safe_relative_path(path) || !(kind.is_file() || kind.is_dir()) {
            return Err(invalid());
        }
        if kind.is_dir() {
            continue;
        }
        let Some(expected) = artifact.files.iter().find(|file| file.archive_path == path) else {
            continue;
        };
        if !found.insert(expected.archive_path.as_str()) || entry.size() != expected.size {
            return Err(invalid());
        }
        copy_checked(
            &mut entry,
            &staging.join(&expected.path),
            expected.size,
            cancel,
        )?;
    }
    if found.len() != artifact.files.len() {
        return Err(Error::InvalidArchive(
            "Eine erforderliche Modelldatei fehlt.".into(),
        ));
    }
    Ok(())
}
