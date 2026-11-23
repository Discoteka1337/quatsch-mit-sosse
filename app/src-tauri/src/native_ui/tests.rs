use super::*;
use quatsch_mit_sosse_hotkey::Hotkey;

#[test]
fn cleanup_controls_edit_each_setting_independently() {
    use crate::settings::CleanupSettings;
    use values::cleanup_switch;
    let mut cleanup = CleanupSettings::default();
    for id in [
        VOICE_COMMANDS,
        USE_DICTIONARY,
        SELF_CORRECTIONS,
        WRITE_NUMBERS,
        PUNCTUATION,
        USE_SNIPPETS,
    ] {
        let old = cleanup;
        let flag = cleanup_switch(&mut cleanup, id).unwrap();
        *flag = !*flag;
        assert_ne!(cleanup, old);
        let saved: serde_json::Value = serde_json::to_value(cleanup).unwrap();
        let key = match id {
            VOICE_COMMANDS => "voiceCommands",
            USE_DICTIONARY => "dictionary",
            SELF_CORRECTIONS => "selfCorrections",
            WRITE_NUMBERS => "numbers",
            PUNCTUATION => "punctuation",
            _ => "snippets",
        };
        assert_ne!(saved[key], serde_json::to_value(old).unwrap()[key]);
    }
    assert!(values::digit_threshold("00000000000").is_none());
    assert_eq!(values::digit_threshold("0"), Some(0));
    assert_eq!(values::digit_threshold("13"), Some(13));
    assert!(values::digit_threshold("-1").is_none());
    assert!(values::digit_threshold("4294967296").is_none());
}

#[test]
fn device_and_model_scans_deliver_independently_and_wait_for_their_own_open_combo() {
    use std::{
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };

    for block_devices in [false, true] {
        let (release_devices, device_gate) = mpsc::channel();
        let (release_models, model_gate) = mpsc::channel();
        let mut inventory = discovery::Inventory::default();
        inventory
            .request(
                Some(move || {
                    if block_devices {
                        device_gate.recv_timeout(Duration::from_secs(5)).unwrap();
                    }
                    Ok(vec!["Mikrofon".into()])
                }),
                Some(move || {
                    if !block_devices {
                        model_gate.recv_timeout(Duration::from_secs(5)).unwrap();
                    }
                    Ok(Vec::new())
                }),
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let updates = inventory.poll(true, true);
            if block_devices {
                assert!(updates.devices.unwrap().is_none());
                if updates.models.unwrap().is_some() {
                    break;
                }
            } else {
                assert!(updates.models.unwrap().is_none());
                if updates.devices.unwrap().is_some() {
                    break;
                }
            }
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        if block_devices {
            release_devices.send(()).unwrap();
        } else {
            release_models.send(()).unwrap();
        }
        let held = inventory.poll(false, false);
        assert!(held.devices.unwrap().is_none() && held.models.unwrap().is_none());
        assert!(inventory.pending());
        loop {
            let remaining = inventory.poll(true, true);
            if remaining.devices.unwrap().is_some() || remaining.models.unwrap().is_some() {
                break;
            }
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        assert!(!inventory.pending());
    }
}

#[test]
fn a_slow_device_scan_keeps_the_window_responsive_and_coalesces_refreshes() {
    use std::{sync::mpsc, thread, time::Duration};

    let mut scan = discovery::Discovery::default();
    let (release, wait) = mpsc::channel();
    let (started, running) = mpsc::channel();
    scan.request(move || {
        started.send(thread::current().id()).unwrap();
        wait.recv_timeout(Duration::from_secs(5)).unwrap();
        42
    })
    .unwrap();
    assert_ne!(running.recv().unwrap(), thread::current().id());
    scan.request(|| panic!("a second enumeration must wait"))
        .unwrap();
    assert!(scan.poll().unwrap().is_none());
    release.send(()).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some((value, refresh_again)) = scan.poll().unwrap() {
            assert_eq!(value, 42);
            assert!(refresh_again);
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        thread::yield_now();
    }
}

#[test]
fn capturing_waits_for_release_and_supports_modifier_only_chords() {
    let mut capture = Chord::default();
    for key in [0x11, 0x10, 0x20, 0x20] {
        assert!(capture.key(key, true).is_none());
    }
    assert!(capture.key(0x20, false).is_none());
    assert!(capture.key(0x10, false).is_none());
    assert_eq!(
        capture.key(0x11, false),
        Some(Some("Strg+Umschalt+Leertaste".parse::<Hotkey>().unwrap()))
    );
    let mut capture = Chord::default();
    capture.key(0x11, true);
    capture.key(0x5b, true);
    capture.key(0x11, false);
    assert_eq!(
        capture.key(0x5b, false),
        Some(Some("Strg+Win".parse().unwrap()))
    );
}

#[test]
fn recorded_altgr_is_rejected_and_left_alt_can_still_be_bound() {
    let left_chord: Hotkey = "Strg+Alt+Q".parse().unwrap();
    for (key, flags, expected) in [
        (0x12, 1 << 24, None),
        (0xa5, 0, None),
        (0x12, 0, Some(left_chord)),
    ] {
        let mut chord = Chord::default();
        let alt = capture::virtual_key(key, flags);
        for key in [0x11, alt, 0x51] {
            assert!(chord.key(key, true).is_none());
        }
        for key in [0x51, alt] {
            assert!(chord.key(key, false).is_none());
        }
        assert_eq!(chord.key(0x11, false), Some(expected));
    }
}

#[test]
fn unsupported_and_multi_trigger_chords_are_rejected() {
    for keys in [vec![0x41, 0x42], vec![0xba]] {
        let mut capture = Chord::default();
        for key in &keys {
            capture.key(*key, true);
        }
        let mut final_result = None;
        for key in keys {
            final_result = capture.key(key, false);
        }
        assert_eq!(final_result, Some(None));
    }
}

#[test]
fn layout_keeps_columns_separate_at_minimum_width_and_high_dpi() {
    for dpi in [96, 144, 192] {
        let width = scale(550, dpi);
        let left = placement(DICTATION, width, dpi, 0);
        let right = placement(REINSERT, width, dpi, 0);
        assert!(left.x + left.width < right.x);
        assert!(right.x + right.width <= width);
        let comparison = placement(COMPARISON_SHORTCUT, width, dpi, 0);
        assert!(left.y + left.height <= comparison.y);
        assert!(comparison.y + comparison.height <= placement(203, width, dpi, 0).y);
        let general_height = content_height(true);
        let editor_height = content_height(false);
        for field in FIELDS.iter() {
            let limit = if Collection::for_field(field.id).is_none() {
                general_height
            } else {
                editor_height
            };
            let rect = placement(field.id, width, dpi, 0);
            assert!(rect.y + rect.height <= scale(limit, dpi));
        }
        assert!(editor_height + 150 < general_height);
        let last = placement(HIDE_FROM_CAPTURE, width, dpi, 0);
        assert!(last.y + last.height <= scale(general_height, dpi));
        assert_eq!(placement(HIDE_FROM_CAPTURE, width, dpi, 30).y, last.y - 30);
    }
}

#[test]
fn dictionary_csv_commands_round_trip_the_selected_files() {
    use entry_editor::{Action, Collection};
    use quatsch_mit_sosse_store::Store;
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("test.db")).unwrap();
    let csv = dir.path().join("Meine Wörter.csv");
    Action::Export(csv.clone())
        .run(Collection::Dictionary, &mut store)
        .unwrap();
    let codex = store.list("kodex").unwrap().remove(0);
    store.delete(codex.id).unwrap();
    Action::Import(csv)
        .run(Collection::Dictionary, &mut store)
        .unwrap();
    let restored = store.list("kodex").unwrap().remove(0);
    assert_eq!(restored.term, "Codex");
    assert_eq!(restored.aliases, codex.aliases);
}

#[test]
fn snippet_csv_commands_round_trip_multiline_text_and_leave_dictionary_entries_alone() {
    use entry_editor::{Action, Collection};
    use quatsch_mit_sosse_store::{SnippetDraft, Store};
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("test.db")).unwrap();
    let draft = SnippetDraft {
        trigger: "Meine Vorlage".into(),
        text: "Zeile eins, mit Komma\r\"{cursor}\"\r\nEnde\n".into(),
    };
    let id = store.add_snippet(&draft).unwrap();
    let csv = dir.path().join("Meine Vorlagen.csv");
    Action::Export(csv.clone())
        .run(Collection::Snippets, &mut store)
        .unwrap();
    store.delete_snippet(id).unwrap();
    Action::Import(csv)
        .run(Collection::Snippets, &mut store)
        .unwrap();
    let imported = store.list_snippets("Vorlage").unwrap().remove(0);
    assert_eq!(imported.text, draft.text);
    let displayed = entry_editor::windows_lines(&imported.text);
    assert_eq!(
        displayed,
        "Zeile eins, mit Komma\r\n\"{cursor}\"\r\nEnde\r\n"
    );
    let renamed = SnippetDraft {
        trigger: "Neue Vorlage".into(),
        text: entry_editor::edited_text(Some(&imported.text), &displayed),
    };
    Action::SaveSnippet(Some(imported.id), renamed)
        .run(Collection::Snippets, &mut store)
        .unwrap();
    assert_eq!(
        store.list_snippets("Neue Vorlage").unwrap()[0].text,
        draft.text
    );
    assert_eq!(store.list("").unwrap().len(), 10);
}
