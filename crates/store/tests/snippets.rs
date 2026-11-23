use quatsch_mit_sosse_store::{SnippetDraft, Store};
use rusqlite::Connection;
use tempfile::tempdir;

fn draft(trigger: &str, text: &str) -> SnippetDraft {
    SnippetDraft {
        trigger: trigger.into(),
        text: text.into(),
    }
}

#[test]
fn seeds_exact_templates_once_and_leaves_user_edits_alone() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.db");
    let store = Store::open(&path).unwrap();
    let entries = store.list_snippets("").unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].trigger, "Gedanken ordnen Prompt");
    assert_eq!(
        entries[0].text,
        "Ordne diese unstrukturierten Gedanken zu einer klaren, gut lesbaren Fassung, ohne Inhalte hinzuzufügen oder wegzulassen."
    );
    assert_eq!(entries[1].trigger, "meine E-Mail-Adresse");
    assert!(entries[1].text.is_empty());
    store.delete_snippet(entries[0].id).unwrap();
    store
        .update_snippet(
            entries[1].id,
            &draft(&entries[1].trigger, "adresse@example.org"),
        )
        .unwrap();
    drop(store);
    let restored = Store::open(&path).unwrap().list_snippets("").unwrap();
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].text, "adresse@example.org");
}

#[test]
fn upgrades_v1_with_existing_triggers_and_retains_equivalent_legacy_entries() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("bestanden.db");
    let db = Connection::open(&path).unwrap();
    db.execute_batch(include_str!("../migrations/001_dictionary.sql"))
        .unwrap();
    db.execute_batch(
        "PRAGMA user_version = 1;
        INSERT INTO snippets (trigger, text, created_at) VALUES
        ('Meine E-Mail-Adresse!', 'eigener Inhalt', 123),
        ('GRUSS', 'Erster', 100), ('Gruss?', 'Zweiter', 101);",
    )
    .unwrap();
    drop(db);
    let mut store = Store::open(&path).unwrap();
    let entries = store.list_snippets("").unwrap();
    assert_eq!(entries.len(), 4);
    let existing = store.list_snippets("MEINE").unwrap().remove(0);
    assert_eq!(
        (existing.text.as_str(), existing.created_at),
        ("eigener Inhalt", 123)
    );
    assert_eq!(store.list_snippets("Gruss").unwrap().len(), 2);
    assert_eq!(store.list("").unwrap().len(), 10);
    let mut bytes = Vec::new();
    store.export_snippets_csv(&mut bytes).unwrap();
    store.import_snippets_csv(bytes.as_slice()).unwrap();
    assert_eq!(store.list_snippets("").unwrap(), entries);
    let mut copy = Store::open(dir.path().join("kopie.db")).unwrap();
    copy.import_snippets_csv(bytes.as_slice()).unwrap();
    let copies = copy.list_snippets("Gruss").unwrap();
    assert_eq!(copies.len(), 2);
    assert_eq!(copies[0].text, "Erster");
    assert_eq!(copies[1].text, "Zweiter");
    let legacy = store.list_snippets("Erster").unwrap().remove(0);
    store
        .update_snippet(legacy.id, &draft(&legacy.trigger, "bearbeitet"))
        .unwrap();
    assert_eq!(store.list_snippets("bearbeitet").unwrap()[0].id, legacy.id);
}

#[test]
fn crud_respects_normalized_triggers_preserves_multiline_text_and_creation_time() {
    let dir = tempdir().unwrap();
    let store = Store::open(dir.path().join("test.db")).unwrap();
    let sample = draft("  Übergabe!  ", " Zeile eins\r\nZeile zwei\n{cursor} ");
    let id = store.add_snippet(&sample).unwrap();
    let original = store.list_snippets("ZWEI").unwrap().remove(0);
    assert_eq!(original.id, id);
    assert_eq!(original.trigger, "Übergabe!");
    assert_eq!(original.text, sample.text);
    assert!(original.matches("u\u{308}BER"));
    assert!(store.add_snippet(&draft("U\u{308}BERGABE?", "x")).is_err());
    for trigger in ["", " .?! ", "zwei\nZeilen", "a\0b"] {
        assert!(store.add_snippet(&draft(trigger, "x")).is_err());
    }
    assert!(store.add_snippet(&draft("gültig", "a\0b")).is_err());
    assert!(
        store
            .update_snippet(id, &draft("Gedanken ordnen Prompt.", "x"))
            .is_err()
    );
    store.update_snippet(id, &draft("Gruß", "")).unwrap();
    let updated = store.list_snippets("Gruß").unwrap().remove(0);
    assert_eq!(updated.created_at, original.created_at);
    assert_eq!(updated.id, id);
    assert!(updated.text.is_empty());
    store.delete_snippet(id).unwrap();
    assert!(store.update_snippet(id, &sample).is_err());
    assert!(store.delete_snippet(id).is_err());
}

#[test]
fn version_two_database_retains_its_existing_numeric_trigger_keys() {
    let dir = tempdir().unwrap();
    let mut store = Store::open(dir.path().join("test.db")).unwrap();
    store.add_snippet(&draft("1,3 Mengen", "Dezimal")).unwrap();
    assert!(matches!(
        store.add_snippet(&draft("13 Mengen", "Ganzzahlig")),
        Err(quatsch_mit_sosse_store::Error::DuplicateTrigger)
    ));
    assert!(
        store
            .import_snippets_csv("trigger,text\n13 Mengen,Kein Ersatz\n".as_bytes())
            .is_err()
    );
    assert_eq!(store.list_snippets("Mengen").unwrap()[0].text, "Dezimal");
}

#[test]
fn csv_roundtrip_merges_on_trigger_and_preserves_multiline_templates_and_metadata() {
    let dir = tempdir().unwrap();
    let mut store = Store::open(dir.path().join("test.db")).unwrap();
    let csv = "\u{feff}trigger,text,created_at\r\nGruß,\"Er sagt \"\"Hallo\"\"\r\n{cursor}, {datum}\",17\r\n";
    assert_eq!(store.import_snippets_csv(csv.as_bytes()).unwrap(), 1);
    let entry = store.list_snippets("Gruß").unwrap().remove(0);
    assert_eq!(entry.text, "Er sagt \"Hallo\"\r\n{cursor}, {datum}");
    assert_eq!(entry.created_at, 17);
    let mut bytes = Vec::new();
    store.export_snippets_csv(&mut bytes).unwrap();
    let mut copy = Store::open(dir.path().join("kopie.db")).unwrap();
    for _ in 0..2 {
        assert_eq!(copy.import_snippets_csv(bytes.as_slice()).unwrap(), 3);
    }
    let copied = copy.list_snippets("Gru").unwrap().remove(0);
    assert_eq!(copied.text, entry.text);
    assert_eq!(copied.created_at, entry.created_at);
    copy.import_snippets_csv("text,trigger\n,GRUSS!\n".as_bytes())
        .unwrap();
    let updated = copy.list_snippets("GRUSS!").unwrap().remove(0);
    assert_eq!(updated.id, copied.id);
    assert_eq!(updated.created_at, copied.created_at);
    assert!(updated.text.is_empty());
    assert_eq!(copy.list_snippets("").unwrap().len(), 3);
}

#[test]
fn reads_quoted_fields_across_chunks_and_accepts_an_optional_utf8_bom() {
    struct OneByte<'a>(&'a [u8]);
    impl std::io::Read for OneByte<'_> {
        fn read(&mut self, dest: &mut [u8]) -> std::io::Result<usize> {
            let count = dest.len().min(1);
            self.0.read(&mut dest[..count])
        }
    }
    let dir = tempdir().unwrap();
    let mut store = Store::open(dir.path().join("test.db")).unwrap();
    let csv = "\u{feff}\"trigger\",\"text\"\r\nGruß,\"Zeile 1\n\"\"Zwei\"\"\"";
    assert_eq!(
        store.import_snippets_csv(OneByte(csv.as_bytes())).unwrap(),
        1
    );
    assert_eq!(
        store.list_snippets("Gruß").unwrap()[0].text,
        "Zeile 1\n\"Zwei\""
    );
}

#[test]
fn import_rejects_invalid_rows_atomically_and_checks_headers_even_without_rows() {
    let dir = tempdir().unwrap();
    let mut store = Store::open(dir.path().join("test.db")).unwrap();
    let original = store.list_snippets("").unwrap();
    for csv in [
        "trigger,text\nmeine E-Mail-Adresse,neu\n!!,falsch\n",
        "trigger,text,created_at\nNeu,A,1\nZwei,B,falsch\n",
        "trigger,text\nmeine E-Mail-Adresse,neu\nVorlage,\"nicht geschlossen\nWeitere,Vorlage\n",
        "trigger,text\nVorlage,\"halb\"abgeschlossen\n",
        "trigger,text\nVorlage,unter\"brochen\n",
        "trigger,text,text\n",
        "text\n",
        "trigger\n",
        "trigger,text,unknown\n",
    ] {
        assert!(store.import_snippets_csv(csv.as_bytes()).is_err());
        assert_eq!(store.list_snippets("").unwrap(), original);
    }
    for entry in original {
        store.delete_snippet(entry.id).unwrap();
    }
    let mut bytes = Vec::new();
    store.export_snippets_csv(&mut bytes).unwrap();
    assert_eq!(bytes, b"trigger,text,created_at\r\n");
    assert_eq!(store.import_snippets_csv(bytes.as_slice()).unwrap(), 0);
}
