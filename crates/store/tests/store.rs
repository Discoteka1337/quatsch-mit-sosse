use std::{fs, path::Path, sync::Barrier, thread};

use quatsch_mit_sosse_store::{DictionaryDraft, Source, Store};
use rusqlite::Connection;
use tempfile::tempdir;

fn draft(term: &str) -> DictionaryDraft {
    DictionaryDraft {
        term: term.into(),
        replacement: Some("Schöne Grüße".into()),
        aliases: vec!["erste Variante".into(), "wörtlich, gesprochen".into()],
        source: Source::Auto,
    }
}

fn backup_files(path: &Path) -> Vec<std::path::PathBuf> {
    fs::read_dir(path.parent().unwrap().join("backups"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect()
}

fn version(connection: &Connection) -> i64 {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn provisions_schema_and_seeds_once_even_on_simultaneous_open() {
    let dir = tempdir().unwrap();
    let path = dir
        .path()
        .join("Pfad mit Soße")
        .join("quatsch-mit-sosse.db");
    let start = Barrier::new(4);
    thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                start.wait();
                assert_eq!(Store::open(&path).unwrap().list("").unwrap().len(), 10);
            });
        }
    });
    let store = Store::open(&path).unwrap();
    let terms: Vec<_> = store
        .list("")
        .unwrap()
        .into_iter()
        .map(|entry| entry.term)
        .collect();
    assert_eq!(
        terms,
        [
            "Anton-Mall-Stadion",
            "Baar",
            "BGV",
            "ChatGPT",
            "Claude",
            "Codex",
            "Denis",
            "Lia",
            "Saatci",
            "Wispr Flow"
        ]
    );
    let codex = store.list("kodex").unwrap().remove(0);
    assert_eq!(codex.term, "Codex");
    assert_eq!(store.list("B G V").unwrap()[0].term, "BGV");
    assert_eq!(store.list("Chat G P T").unwrap()[0].term, "ChatGPT");
    store.delete(codex.id).unwrap();
    drop(store);
    assert!(
        Store::open(&path)
            .unwrap()
            .list("Codex")
            .unwrap()
            .is_empty()
    );
    let db = Connection::open(&path).unwrap();
    assert_eq!(version(&db), 2);
    let timestamp: i64 = db
        .query_row("SELECT created_at FROM snippets", [], |r| r.get(0))
        .unwrap();
    assert!(timestamp > 0);
    assert_eq!(backup_files(&path).len(), 2);
}

#[test]
fn backup_contains_the_pre_migration_schema_and_committed_wal_pages() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.db");
    let existing = Connection::open(&path).unwrap();
    existing
        .execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;
        CREATE TABLE original (n INTEGER); INSERT INTO original VALUES (42);",
        )
        .unwrap();
    Store::open(&path).unwrap();
    let backups = backup_files(&path);
    assert_eq!(backups.len(), 2);
    let backup = backups
        .iter()
        .map(|path| Connection::open(path).unwrap())
        .find(|db| version(db) == 0)
        .unwrap();
    assert_eq!(version(&backup), 0);
    let original: i64 = backup
        .query_row("SELECT n FROM original", [], |row| row.get(0))
        .unwrap();
    assert_eq!(original, 42);
    assert!(backup.prepare("SELECT * FROM dictionary").is_err());
}

#[test]
fn backup_or_migration_failure_and_unknown_versions_leave_existing_data_intact() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.db");
    let db = Connection::open(&path).unwrap();
    fs::write(dir.path().join("backups"), b"occupied").unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(version(&db), 0);
    assert!(db.prepare("SELECT * FROM dictionary").is_err());
    fs::remove_file(dir.path().join("backups")).unwrap();
    db.execute_batch("CREATE TABLE snippets (original INTEGER)")
        .unwrap();
    assert!(Store::open(&path).is_err());
    assert!(db.prepare("SELECT original FROM snippets").is_ok());
    assert!(db.prepare("SELECT * FROM dictionary").is_err());
    assert_eq!(version(&db), 0);
    db.pragma_update(None, "user_version", 72).unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(version(&db), 72);
    assert_eq!(backup_files(&path).len(), 1);
}

#[test]
fn searches_writes_validated_entries_and_preserves_identity_on_edit() {
    let dir = tempdir().unwrap();
    let store = Store::open(dir.path().join("test.db")).unwrap();
    let mut edit = draft("Äther");
    let id = store.add(&edit).unwrap();
    let original = store.list("äTHER").unwrap().remove(0);
    assert_eq!(original.id, id);
    assert_eq!(original.aliases, edit.aliases);
    assert_eq!(original.source, Source::Auto);
    assert_eq!(store.list("SCHÖNE GR").unwrap()[0].id, id);
    assert_eq!(store.list("ÖRTLICH,").unwrap()[0].id, id);
    assert!(store.list("%").unwrap().is_empty());
    assert!(store.add(&draft("äther")).is_err());
    assert!(store.add(&draft(" ")).is_err());
    assert!(store.add(&draft("zwei\nZeilen")).is_err());
    let mut invalid = draft("Unbrauchbar");
    invalid.aliases = vec!["zwei\rVarianten".into()];
    assert!(store.add(&invalid).is_err());
    edit.term = "Codex".into();
    assert!(store.update(id, &edit).is_err());
    assert_eq!(store.list("Äther").unwrap().len(), 1);
    edit.term = "Wörter".into();
    edit.source = Source::Manual;
    edit.replacement = None;
    edit.aliases.clear();
    store.update(id, &edit).unwrap();
    let updated = store.list("Wörter").unwrap().remove(0);
    assert_eq!((updated.id, updated.created_at), (id, original.created_at));
    assert_eq!(updated.replacement, None);
    assert!(updated.aliases.is_empty());
    store.delete(id).unwrap();
    assert!(store.delete(id).is_err());
    assert!(store.update(id, &edit).is_err());
}

#[test]
fn csv_roundtrip_merges_by_term_and_keeps_multiline_fields_and_metadata() {
    let dir = tempdir().unwrap();
    let mut store = Store::open(dir.path().join("test.db")).unwrap();
    let csv = "term,replacement,aliases,source,created_at\r\n\
        \"Rot, Blau\",\"Er sagt \"\"Hallo\"\"\r\nZeile zwei\",\"[\"\"rot blau\"\",\"\"rot, blau\"\"]\",auto,1700000000\r\n";
    assert_eq!(store.import_csv(csv.as_bytes()).unwrap(), 1);
    let original = store.list("Rot, Blau").unwrap().remove(0);
    assert_eq!(original.created_at, 1_700_000_000);
    assert_eq!(original.aliases, ["rot blau", "rot, blau"]);
    assert_eq!(
        original.replacement.as_deref(),
        Some("Er sagt \"Hallo\"\r\nZeile zwei")
    );
    let mut bytes = Vec::new();
    store.export_csv(&mut bytes).unwrap();
    let mut other = Store::open(dir.path().join("import.db")).unwrap();
    for _ in 0..2 {
        assert_eq!(other.import_csv(bytes.as_slice()).unwrap(), 11);
    }
    let copy = other.list("Rot, Blau").unwrap().remove(0);
    assert_eq!(
        (
            copy.term,
            copy.aliases,
            copy.replacement,
            copy.created_at,
            copy.source
        ),
        (
            original.term,
            original.aliases,
            original.replacement,
            original.created_at,
            original.source
        )
    );
    assert_eq!(other.list("").unwrap().len(), 11);
}

#[test]
fn import_rejects_bad_rows_atomically_and_empty_export_keeps_header() {
    let dir = tempdir().unwrap();
    let mut store = Store::open(dir.path().join("test.db")).unwrap();
    assert!(
        store
            .import_csv("term,source\nCodex,auto\nNeu,other\n".as_bytes())
            .is_err()
    );
    assert_eq!(store.list("kodex").unwrap()[0].source, Source::Manual);
    assert!(
        store
            .import_csv("term,aliases\nHallo,42\n".as_bytes())
            .is_err()
    );
    assert!(
        store
            .import_csv("replacement,termx\nx,y".as_bytes())
            .is_err()
    );
    assert_eq!(
        store
            .import_csv("\u{feff}term\nWolke\n".as_bytes())
            .unwrap(),
        1
    );
    for entry in store.list("").unwrap() {
        store.delete(entry.id).unwrap();
    }
    let mut bytes = Vec::new();
    store.export_csv(&mut bytes).unwrap();
    assert_eq!(
        std::str::from_utf8(&bytes).unwrap().trim_end(),
        "term,replacement,aliases,source,created_at"
    );
    assert_eq!(store.import_csv(bytes.as_slice()).unwrap(), 0);
}

#[test]
fn import_can_restore_exported_timestamps_even_for_existing_seed_terms() {
    let dir = tempdir().unwrap();
    let mut store = Store::open(dir.path().join("test.db")).unwrap();
    let id = store.list("Codex").unwrap()[0].id;
    store
        .import_csv("term,created_at\nCodex,1700000000\n".as_bytes())
        .unwrap();
    store.import_csv("term\nCodex\n".as_bytes()).unwrap();
    let entry = store.list("Codex").unwrap().remove(0);
    assert_eq!(entry.id, id);
    assert_eq!(entry.created_at, 1_700_000_000);
    assert_eq!(entry.aliases, ["kodex"]);
}
