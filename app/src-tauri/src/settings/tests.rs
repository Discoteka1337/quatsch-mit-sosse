use super::*;
use tempfile::tempdir;

#[test]
fn version_one_preserves_user_choices_and_acquires_the_live_cleanup_profile() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(
        &path,
        r#"{"schemaVersion":1,"mode":"toggle","volumePercent":63}"#,
    )
    .unwrap();
    let mut store = SettingsStore::load(dir.path()).unwrap();
    let mut settings = store.current().clone();
    assert_eq!(settings.mode, HotkeyMode::Toggle);
    assert_eq!(settings.volume_percent, 63);
    assert_eq!(settings.cleanup, CleanupSettings::default());
    assert!(!settings.cleanup.voice_commands);
    assert_eq!(settings.cleanup.digits_from, 13);
    settings.cleanup.voice_commands = true;
    settings.cleanup.digits_from = 20;
    store.save(settings.clone()).unwrap();
    assert_eq!(
        SettingsStore::load(dir.path()).unwrap().current(),
        &settings
    );
}

#[test]
fn oversized_settings_and_legacy_overlay_do_not_migrate_or_replace_files() {
    for name in ["settings.json", "overlay.json"] {
        let dir = tempdir().unwrap();
        let path = dir.path().join(name);
        let mut bytes = b"{}".to_vec();
        bytes.resize(1024 * 1024 + 1, b' ');
        fs::write(&path, &bytes).unwrap();
        assert!(SettingsStore::load(dir.path()).is_err());
        assert!(fs::read(path).unwrap() == bytes);
        if name == "overlay.json" {
            assert!(!dir.path().join("settings.json").exists());
        }
    }
}

#[test]
fn oversized_settings_write_keeps_the_last_readable_version_and_login_entry() {
    let dir = tempdir().unwrap();
    let mut store = SettingsStore::load(dir.path()).unwrap();
    let before = store.current().clone();
    let next = Settings {
        microphone: Some("a".repeat(1024 * 1024)),
        autostart: true,
        ..before.clone()
    };
    assert!(
        store
            .save_with(next, |_| panic!("unexpected registry update"))
            .is_err()
    );
    assert_eq!(store.current(), &before);
    assert_eq!(SettingsStore::load(dir.path()).unwrap().current(), &before);
}

#[test]
fn existing_overlay_options_migrate_once_without_touching_monitor_positions() {
    let dir = tempdir().unwrap();
    let legacy = dir.path().join("overlay.json");
    let bytes = br#"{"preferences":{"showReady":true,"hideFromCapture":true},"lastMonitor":"A","positions":{"A":{"x":42,"y":17}}}"#;
    fs::write(&legacy, bytes).unwrap();
    let mut store = SettingsStore::load(dir.path()).unwrap();
    let mut next = store.current().clone();
    assert!(next.overlay.show_ready && next.overlay.hide_from_capture);
    assert_eq!(next.volume_percent, 20);
    assert!(next.tones);
    assert!(!next.autostart);
    next.overlay.show_ready = false;
    store.save(next.clone()).unwrap();
    let reopened = SettingsStore::load(dir.path()).unwrap();
    assert_eq!(reopened.current(), &next);
    assert_eq!(fs::read(legacy).unwrap(), bytes);
    let file: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("settings.json")).unwrap()).unwrap();
    assert_eq!(file["schemaVersion"], 2);
}

#[test]
fn unversioned_settings_keep_custom_values_and_acquire_missing_defaults() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, br#"{"volumePercent":0,"tones":false,"mode":"toggle","microphone":"USB","dictationHotkey":"F9"}"#).unwrap();
    let migrated = SettingsStore::load(dir.path()).unwrap();
    let saved = migrated.current();
    assert_eq!(saved.hotkeys().unwrap().mode, HotkeyMode::Toggle);
    assert_eq!(saved.hotkeys().unwrap().dictation, "F9".parse().unwrap());
    assert_eq!(
        saved.reinsert_hotkey,
        HotkeyConfig::default().reinsert.to_string()
    );
    assert_eq!(saved.microphone.as_deref(), Some("USB"));
    assert_eq!(saved.volume_percent, 0);
    assert!(!saved.tones);
    assert_eq!(SettingsStore::load(dir.path()).unwrap().current(), saved);
    let parsed: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(parsed["schemaVersion"], 2);
}

#[test]
fn corrupted_and_newer_settings_are_never_overwritten() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    for bytes in [
        r#"{"schemaVersion":3,"other":true}"#,
        "{",
        r#"{"schemaVersion":1,"volumePercent":101}"#,
        r#"{"dictationHotkey":"F1","reinsertHotkey":"F1"}"#,
    ] {
        fs::write(&path, bytes).unwrap();
        assert!(SettingsStore::load(dir.path()).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), bytes);
    }
}

#[test]
fn failed_autostart_update_restores_settings_and_changed_controls_round_trip() {
    let dir = tempdir().unwrap();
    let mut store = SettingsStore::load(dir.path()).unwrap();
    let path = dir.path().join("settings.json");
    let before = fs::read(&path).unwrap();
    let next = Settings {
        autostart: true,
        tones: false,
        microphone: Some("Mikrofon ß".into()),
        model: Some("whisper-large-v3-turbo".into()),
        dictation_hotkey: "Strg+F12".into(),
        mode: HotkeyMode::Toggle,
        volume_percent: 89,
        ..store.current().clone()
    };
    assert!(
        store
            .save_with(next.clone(), |_| Err(io::Error::other("test")))
            .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(!store.current().autostart);
    store
        .save_with(next.clone(), |enabled| {
            assert!(enabled);
            Ok(())
        })
        .unwrap();
    assert_eq!(SettingsStore::load(dir.path()).unwrap().current(), &next);
    let mut disabled = next;
    disabled.autostart = false;
    store
        .save_with(disabled.clone(), |enabled| {
            assert!(!enabled);
            Ok(())
        })
        .unwrap();
    assert_eq!(
        SettingsStore::load(dir.path()).unwrap().current(),
        &disabled
    );
}

#[test]
fn invalid_changes_do_not_alter_file_memory_or_autostart() {
    let dir = tempdir().unwrap();
    let mut store = SettingsStore::load(dir.path()).unwrap();
    let path = dir.path().join("settings.json");
    let previous = fs::read(&path).unwrap();
    let next = Settings {
        autostart: true,
        volume_percent: 101,
        ..store.current().clone()
    };
    assert!(
        store
            .save_with(next, |_| panic!("invalid preferences"))
            .is_err()
    );
    assert_eq!(fs::read(path).unwrap(), previous);
    assert_eq!(store.current(), &Settings::default());
}

#[test]
fn failure_to_replace_locked_settings_undoes_the_autostart_change() {
    use std::os::windows::fs::OpenOptionsExt;

    let dir = tempdir().unwrap();
    let mut store = SettingsStore::load(dir.path()).unwrap();
    let path = dir.path().join("settings.json");
    let original = fs::read(&path).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&path)
        .unwrap();
    let mut writes = Vec::new();
    let next = Settings {
        autostart: true,
        ..store.current().clone()
    };
    assert!(
        store
            .save_with(next, |enabled| {
                writes.push(enabled);
                Ok(())
            })
            .is_err()
    );
    drop(lock);

    assert_eq!(writes, [true, false]);
    assert_eq!(fs::read(path).unwrap(), original);
    assert_eq!(store.current(), &Settings::default());
}

#[test]
fn login_command_preserves_unicode_and_quotes_the_entire_executable_path() {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    let path = Path::new(r"C:\Diktat mit ß\quatsch-mit-sosse.exe");
    let command = autostart::command(path).unwrap();
    let expected: Vec<u16> = OsStr::new(r#""C:\Diktat mit ß\quatsch-mit-sosse.exe" --autostart"#)
        .encode_wide()
        .chain([0])
        .collect();
    assert_eq!(command, expected);
}
