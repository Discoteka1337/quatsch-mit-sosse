use super::*;
use crate::speech::{DEFAULT_MODEL, FALLBACK_MODEL};
use quatsch_mit_sosse_store::{DictionaryDraft, Source, Store};

#[test]
fn each_dictation_reads_the_current_dictionary_before_insertion() {
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("Wörterbuch Test.db");
    let store = Store::open(&database).unwrap();
    let id = store.list("Codex").unwrap()[0].id;
    let mut session = setup();
    let mut dictate = |expected: &str| {
        session.start(Some(1));
        session.stop();
        session.recognized(Ok(Transcript::with_dictionary(
            DEFAULT_MODEL,
            "äh kodex und vierzehn".into(),
            || Store::open(&database),
        )));
        assert_eq!(session.ports.inserts.last(), Some(&(1, expected.into())));
        session.inserted(Ok(()));
        session.repeat(Some(2));
        assert_eq!(session.ports.inserts.last(), Some(&(2, expected.into())));
        session.inserted(Ok(()));
    };
    dictate("äh Codex und vierzehn");
    store
        .update(
            id,
            &DictionaryDraft {
                term: "Codex".into(),
                aliases: vec!["kodex".into()],
                replacement: Some("Mein CODE".into()),
                source: Source::Manual,
            },
        )
        .unwrap();
    dictate("äh Mein CODE und vierzehn");
    store.delete(id).unwrap();
    dictate("äh kodex und vierzehn");
}

#[test]
fn unreadable_dictionary_keeps_raw_text_for_insertion_and_displays_the_warning() {
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("kein-sqlite.db");
    std::fs::write(&database, b"invalid sqlite database").unwrap();
    let mut session = setup();
    session.start(Some(1));
    session.stop();
    session.recognized(Ok(Transcript::with_dictionary(
        DEFAULT_MODEL,
        "kodex Strasse".into(),
        || Store::open(&database),
    )));
    assert_eq!(session.ports.inserts, [(1, "kodex Straße".into())]);
    session.inserted(Ok(()));
    assert!(
        matches!(&session.ports.overlay, OverlayState::Error { message, text }
        if message.contains("Wörterbuch") && text == "kodex Straße")
    );
    session.repeat(Some(2));
    session.inserted(Ok(()));
    assert!(matches!(session.ports.overlay, OverlayState::Ready));
}

#[test]
fn personal_spellings_take_precedence_over_primeline_and_preserve_technical_text() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("Fachbegriffe.db");
    let store = Store::open(&path).unwrap();
    let mut draft = DictionaryDraft {
        term: "Strasse".into(),
        replacement: None,
        aliases: vec!["Strassennamen".into()],
        source: Source::Manual,
    };
    let id = store.add(&draft).unwrap();
    let input = "Grüsse, Strasse, Strassennamen, Strasse-test@example.org, https://test/Fuss";
    let output =
        |model| Transcript::with_dictionary(model, input.into(), || Store::open(&path)).text;
    assert_eq!(
        output(DEFAULT_MODEL),
        "Grüße, Strasse, Strasse, Strasse-test@example.org, https://test/Fuss"
    );
    assert_eq!(
        output(FALLBACK_MODEL),
        "Grüsse, Strasse, Strasse, Strasse-test@example.org, https://test/Fuss"
    );
    draft.replacement = Some("Hauptweg".into());
    store.update(id, &draft).unwrap();
    assert_eq!(
        output(DEFAULT_MODEL),
        "Grüße, Hauptweg, Hauptweg, Strasse-test@example.org, https://test/Fuss"
    );
    store.delete(id).unwrap();
    assert_eq!(
        output(DEFAULT_MODEL),
        "Grüße, Straße, Strassennamen, Strasse-test@example.org, https://test/Fuss"
    );
}

#[test]
fn primeline_restores_whole_german_words_while_preserving_identifiers_and_fallback_output() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("Fachbegriffe.db");
    for (input, primeline) in [
        (
            "Äpfel, Öl & Übermass – 3,5 % für 12,99 €. Die Masse muss stimmen.",
            "Äpfel, Öl & Übermaß – 3,5 % für 12,99 €. Die Masse muss stimmen.",
        ),
        (
            "„übermass“, Übermaß! ÜBERMASS  mein_Übermass Übermass2 Übermassiv Strasse.de",
            "„übermaß“, Übermaß! ÜBERMASS  mein_Übermass Übermass2 Übermassiv Strasse.de",
        ),
        (
            "Grüsse von der Strasse, draussen am Grossen Fuss. Die Masse muss stimmen.",
            "Grüße von der Straße, draußen am Großen Fuß. Die Masse muss stimmen.",
        ),
        (
            r"Strasse, https://example.test/Strasse Fuss@example.test C:\Fuss\a",
            r"Straße, https://example.test/Strasse Fuss@example.test C:\Fuss\a",
        ),
    ] {
        for (model, expected) in [(DEFAULT_MODEL, primeline), (FALLBACK_MODEL, input)] {
            let transcript =
                Transcript::with_dictionary(model, input.to_owned(), || Store::open(&path));
            assert_eq!(transcript.text, expected);
            assert!(transcript.warning.is_none());
        }
    }
}

#[test]
fn silence_skips_the_dictionary() {
    let transcript = Transcript::with_dictionary(DEFAULT_MODEL, "  ".into(), || {
        panic!("unexpected database access")
    });
    assert_eq!(transcript.text, "  ");
    assert!(transcript.warning.is_none());
}
