use std::sync::atomic::AtomicBool;

use crate::{Manager, Manifest};

fn fixture(payload: &[u8]) -> (tempfile::TempDir, Manager) {
    use sha2::{Digest, Sha256};
    let root = tempfile::tempdir().unwrap();
    let manifest = Manifest::parse(&format!(
        r#"version = 1
allowed_hosts = ["example.org"]
[[models]]
id = "fixture"
display_name = "Testmodell"
languages = ["de"]
kind = "asr"
license = "MIT"
attribution = "Test"
[[models.artifacts]]
urls = ["https://example.org/model"]
archive = "none"
sha256 = "{:x}"
size = {}
[[models.artifacts.files]]
path = "model.onnx"
size = {}
"#,
        Sha256::digest(payload),
        payload.len(),
        payload.len()
    ))
    .unwrap();
    let manager = Manager::from_manifest(root.path().to_path_buf(), manifest).unwrap();
    (root, manager)
}

fn cache(root: &std::path::Path, payload: &[u8]) {
    let download = root.join(".downloads/fixture");
    std::fs::create_dir_all(&download).unwrap();
    std::fs::write(download.join("0.part"), payload).unwrap();
}

#[test]
fn checksum_mismatch_deletes_downloads_and_staging() {
    let (root, manager) = fixture(b"correct bytes");
    cache(root.path(), b"invalid bytes");
    let result = manager.install("fixture", &AtomicBool::new(false), |_| {});
    assert!(matches!(result, Err(crate::Error::ChecksumMismatch)));
    assert!(!root.path().join(".downloads/fixture").exists());
    assert!(!root.path().join(".staging/fixture").exists());
    assert!(!manager.is_installed("fixture").unwrap());
}

#[test]
fn cached_download_installs_atomically_and_remove_clears_everything() {
    let payload = b"synthetic model";
    let (root, manager) = fixture(payload);
    cache(root.path(), payload);
    assert!(!manager.is_installed("fixture").unwrap());
    assert!(manager.path_of("fixture").unwrap().is_none());
    let mut progress = Vec::new();
    let installed = manager
        .install("fixture", &AtomicBool::new(false), |item| {
            progress.push(item)
        })
        .unwrap();
    assert_eq!(
        std::fs::read(installed.join("model.onnx")).unwrap(),
        payload
    );
    assert!(manager.is_installed("fixture").unwrap());
    assert_eq!(manager.path_of("fixture").unwrap(), Some(installed));
    assert_eq!(
        progress.last().unwrap().phase,
        crate::ProgressPhase::Complete
    );
    assert!(!root.path().join(".downloads/fixture").exists());
    manager.remove("fixture").unwrap();
    manager.remove("fixture").unwrap();
    assert!(!manager.is_installed("fixture").unwrap());
}

#[test]
fn interrupted_install_keeps_download_and_resumes_without_network() {
    let payload = b"synthetic model";
    let (root, manager) = fixture(payload);
    cache(root.path(), payload);
    assert!(matches!(
        manager.install("fixture", &AtomicBool::new(true), |_| {}),
        Err(crate::Error::Cancelled)
    ));
    assert!(root.path().join(".downloads/fixture/0.part").exists());
    manager
        .install("fixture", &AtomicBool::new(false), |_| {})
        .unwrap();
    assert!(manager.is_installed("fixture").unwrap());
}

#[test]
fn installed_marker_is_required_and_missing_files_are_detected() {
    let (root, manager) = fixture(b"model");
    std::fs::create_dir(root.path().join("fixture")).unwrap();
    std::fs::write(root.path().join("fixture/model.onnx"), b"model").unwrap();
    assert!(!manager.is_installed("fixture").unwrap());
    cache(root.path(), b"model");
    manager
        .install("fixture", &AtomicBool::new(false), |_| {})
        .unwrap();
    std::fs::remove_file(root.path().join("fixture/model.onnx")).unwrap();
    assert!(!manager.is_installed("fixture").unwrap());
    assert!(manager.path_of("fixture").unwrap().is_none());
}

#[test]
fn unsafe_paths_and_download_hosts_are_rejected_before_installation() {
    let (_, manager) = fixture(b"model");
    let valid = toml::to_string(&manager.manifest).unwrap();
    for unsafe_id in ["..", "../escape", "A", "con", "with space"] {
        assert!(
            Manifest::parse(&valid.replace("id = \"fixture\"", &format!("id = \"{unsafe_id}\"")))
                .is_err()
        );
    }
    for unsafe_path in [
        "../escape",
        "C:/outside",
        "file:stream",
        "a/../b",
        "NUL",
        "a.",
        ".installed.json",
    ] {
        assert!(
            Manifest::parse(&valid.replace(
                "path = \"model.onnx\"",
                &format!("path = \"{unsafe_path}\"")
            ))
            .is_err()
        );
    }
    assert!(
        Manifest::parse(&valid.replace("https://example.org/model", "https://evil.example/model"))
            .is_err()
    );
}

fn archive_payload(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use bzip2::{Compression, write::BzEncoder};
    let mut archive = tar::Builder::new(BzEncoder::new(Vec::new(), Compression::fast()));
    for (path, data) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive.append_data(&mut header, path, *data).unwrap();
    }
    archive.into_inner().unwrap().finish().unwrap()
}

#[test]
fn archive_extracts_only_declared_files_and_flattens_prefix() {
    let payload = archive_payload(&[
        ("bundle/model.onnx", b"model"),
        ("bundle/readme.txt", b"ignored"),
    ]);
    let (root, mut manager) = fixture(&payload);
    let artifact = &mut manager.manifest.models[0].artifacts[0];
    artifact.archive = crate::ArchiveFormat::TarBz2;
    artifact.files[0].archive_path = "bundle/model.onnx".into();
    artifact.files[0].size = 5;
    cache(root.path(), &payload);
    let installed = manager
        .install("fixture", &AtomicBool::new(false), |_| {})
        .unwrap();
    assert_eq!(
        std::fs::read(installed.join("model.onnx")).unwrap(),
        b"model"
    );
    assert!(!installed.join("bundle").exists());
    assert!(!installed.join("readme.txt").exists());
}

#[test]
fn archive_missing_file_or_oversized_entry_never_becomes_installed() {
    for entries in [
        vec![("bundle/other", b"model".as_slice())],
        vec![("bundle/model.onnx", b"too large".as_slice())],
    ] {
        let payload = archive_payload(&entries);
        let (root, mut manager) = fixture(&payload);
        let artifact = &mut manager.manifest.models[0].artifacts[0];
        artifact.archive = crate::ArchiveFormat::TarBz2;
        artifact.files[0].archive_path = "bundle/model.onnx".into();
        artifact.files[0].size = 5;
        cache(root.path(), &payload);
        assert!(matches!(
            manager.install("fixture", &AtomicBool::new(false), |_| {}),
            Err(crate::Error::InvalidArchive(_))
        ));
        assert!(!manager.is_installed("fixture").unwrap());
        assert!(!root.path().join(".staging/fixture").exists());
    }
}

#[test]
fn manifest_change_invalidates_installed_marker() {
    let (root, mut manager) = fixture(b"model");
    cache(root.path(), b"model");
    manager
        .install("fixture", &AtomicBool::new(false), |_| {})
        .unwrap();
    manager.manifest.models[0].artifacts[0].sha256 = "a".repeat(64);
    assert!(!manager.is_installed("fixture").unwrap());
}

#[test]
fn concurrent_install_and_remove_are_rejected() {
    let (root, manager) = fixture(b"model");
    let lock_dir = root.path().join(".locks");
    std::fs::create_dir(&lock_dir).unwrap();
    let lock = std::fs::File::create(lock_dir.join("fixture.lock")).unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();
    assert!(matches!(
        manager.install("fixture", &AtomicBool::new(false), |_| {}),
        Err(crate::Error::Busy)
    ));
    assert!(matches!(manager.remove("fixture"), Err(crate::Error::Busy)));
    drop(lock);
    cache(root.path(), b"model");
    manager
        .install("fixture", &AtomicBool::new(false), |_| {})
        .unwrap();
    assert!(manager.is_installed("fixture").unwrap());
}

#[test]
fn archive_links_and_parent_paths_are_rejected() {
    use bzip2::{Compression, write::BzEncoder};
    for linked in [true, false] {
        let mut archive = tar::Builder::new(BzEncoder::new(Vec::new(), Compression::fast()));
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o644);
        header.set_size(0);
        header.set_path("bundle/model.onnx").unwrap();
        if linked {
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_link_name("../../outside").unwrap();
        } else {
            header.as_mut_bytes()[..100].fill(0);
            header.as_mut_bytes()[..10].copy_from_slice(b"../outside");
        }
        header.set_cksum();
        archive.append(&header, std::io::empty()).unwrap();
        let payload = archive.into_inner().unwrap().finish().unwrap();
        let (root, mut manager) = fixture(&payload);
        let artifact = &mut manager.manifest.models[0].artifacts[0];
        artifact.archive = crate::ArchiveFormat::TarBz2;
        artifact.files[0].archive_path = "bundle/model.onnx".into();
        artifact.files[0].size = 5;
        cache(root.path(), &payload);
        assert!(matches!(
            manager.install("fixture", &AtomicBool::new(false), |_| {}),
            Err(crate::Error::InvalidArchive(_))
        ));
        assert!(!root.path().join("outside").exists());
        assert!(!manager.is_installed("fixture").unwrap());
    }
}

#[test]
fn offline_list_and_status_do_not_create_model_directories() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("missing");
    let manager = Manager::with_root(root.clone()).unwrap();
    assert!(!manager.list().is_empty());
    assert!(!manager.is_installed("silero-vad").unwrap());
    assert!(manager.path_of("silero-vad").unwrap().is_none());
    assert!(!root.exists());
}

#[test]
fn manifest_rejects_unknown_fields_and_unsafe_model_ids() {
    assert!(
        Manifest::parse("version = 1\nallowed_hosts = []\nunknown = true\nmodels = []").is_err()
    );
    assert!(Manifest::parse("version = 1\nallowed_hosts = []\nmodels = []").is_err());
}

#[test]
fn bundled_manifest_contains_required_models_and_real_hashes() {
    let manager = Manager::with_root(std::env::temp_dir().join("qms-unused-model-tests")).unwrap();
    for id in [
        "parakeet-v3-int8",
        "parakeet-primeline-de-int8",
        "silero-vad",
    ] {
        let model = manager.list().iter().find(|model| model.id == id).unwrap();
        assert!(!model.artifacts.is_empty());
        for artifact in &model.artifacts {
            assert_eq!(artifact.sha256.len(), 64);
            assert!(artifact.size > 0);
        }
    }
    assert!(
        manager
            .install("../outside", &AtomicBool::new(false), |_| {})
            .is_err()
    );
}
