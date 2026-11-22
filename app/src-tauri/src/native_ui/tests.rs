use super::*;
use quatsch_mit_sosse_hotkey::Hotkey;

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
        let last = placement(HIDE_FROM_CAPTURE, width, dpi, 0);
        assert!(last.y + last.height <= scale(CONTENT_HEIGHT, dpi));
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
