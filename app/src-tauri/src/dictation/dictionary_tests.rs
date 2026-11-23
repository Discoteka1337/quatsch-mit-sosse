use super::*;
use crate::speech::{DEFAULT_MODEL, FALLBACK_MODEL};
use quatsch_mit_sosse_store::{DictionaryDraft, Source, Store};

#[test]
fn live_cleanup_uses_the_saved_rule_switches_and_digit_threshold() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let db = dir.path().join("words.db");
    let input = "morgen um vierzehn Uhr dreißig";
    let dictate = |text: &str| {
        let settings = crate::settings::SettingsStore::load(dir.path()).unwrap();
        Transcript::from_dictation(
            DEFAULT_MODEL,
            text.into(),
            settings.current().cleanup,
            || Store::open(&db),
        )
        .record
        .finished
    };
    assert_eq!(dictate(input), "Morgen um 14:30 Uhr.");
    assert!(dictate("erste Zeile neuer Absatz").contains("neuer Absatz"));
    std::fs::write(&path, r#"{"schemaVersion":1,"cleanup":{"voiceCommands":true,"digitsFrom":3,"punctuation":false,"fillerWords":"strong"}}"#).unwrap();
    assert_eq!(dictate("also drei Plätze neuer Absatz"), "3 Plätze\n\n");
    std::fs::write(&path, r#"{"cleanup":{"dictionary":false,"snippets":false,"fillerWords":"off","selfCorrections":false,"numbers":false,"punctuation":false}}"#).unwrap();
    let raw = "äh kodex Strasse vierzehn. Gedanken ordnen Prompt.";
    assert_eq!(dictate(raw), raw);
}

#[test]
fn review_preserves_asr_text_and_reports_uncertain_byte_ranges_in_the_finished_text() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("review.db");
    let raw = "Ähm, kodex. Das scheint seltsam, äh, schwierig.";
    let result = Transcript::from_dictation(DEFAULT_MODEL, raw.into(), Default::default(), || {
        Store::open(&db)
    });
    assert_eq!(result.record.original, raw);
    assert_eq!(
        result.record.finished,
        "Codex. Das scheint seltsam, äh, schwierig."
    );
    assert_eq!(
        result
            .record
            .uncertain
            .iter()
            .map(|range| &result.record.finished[range.clone()])
            .collect::<Vec<_>>(),
        ["seltsam, äh, schwierig"]
    );
}

#[test]
fn snippet_edits_apply_on_the_next_dictation_and_repeat_keeps_the_expanded_value_and_caret() {
    use quatsch_mit_sosse_store::SnippetDraft;
    use quatsch_mit_sosse_textfx::SnippetValue;
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("Snippets.db");
    let store = Store::open(&db).unwrap();
    let id = store
        .add_snippet(&SnippetDraft {
            trigger: "mein Gruß".into(),
            text: "Grüße\n{zwischenablage}{cursor}!".into(),
        })
        .unwrap();
    let mut session = setup();
    let mut dictate = |expected: &str, caret| {
        session.start(Some(1));
        session.stop();
        session.recognized(Ok(Transcript::prepare(
            DEFAULT_MODEL,
            "MEIN GRUSS!".into(),
            Default::default(),
            || Store::open(&db),
            |value| {
                assert_eq!(value, SnippetValue::Clipboard);
                Ok("🦦".into())
            },
        )));
        session.inserted(Ok(()));
        session.repeat(Some(2));
        assert_eq!(
            &session.ports.inserts[session.ports.inserts.len() - 2..],
            &[(1, expected.into()), (2, expected.into())]
        );
        assert_eq!(
            &session.ports.carets[session.ports.carets.len() - 2..],
            &[caret, caret]
        );
        session.inserted(Ok(()));
    };
    dictate("Grüße\n🦦!", Some("Grüße\n🦦".len()));
    store
        .update_snippet(
            id,
            &SnippetDraft {
                trigger: "mein Gruß".into(),
                text: "Bis dann!".into(),
            },
        )
        .unwrap();
    dictate("Bis dann!", None);
}

#[test]
fn a_busy_clipboard_preserves_the_trigger_and_returns_a_content_free_warning() {
    use quatsch_mit_sosse_store::SnippetDraft;
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("Snippets.db");
    let store = Store::open(&db).unwrap();
    store
        .add_snippet(&SnippetDraft {
            trigger: "private Vorlage".into(),
            text: "A{zwischenablage}B".into(),
        })
        .unwrap();
    let transcript = Transcript::prepare(
        DEFAULT_MODEL,
        "Gedanken ordnen Prompt. private Vorlage!".into(),
        Default::default(),
        || Store::open(&db),
        |_| Err(quatsch_mit_sosse_inject::InjectError::Unavailable),
    );
    assert_eq!(
        transcript.record.finished,
        "Ordne diese unstrukturierten Gedanken zu einer klaren, gut lesbaren Fassung, ohne Inhalte hinzuzufügen oder wegzulassen. Private Vorlage!"
    );
    assert_eq!(transcript.record.cursor, None);
    assert_eq!(
        transcript.warning.as_ref(),
        Some(&crate::messages().snippet_clipboard_warning)
    );
}

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
        session.recognized(Ok(Transcript::from_dictation(
            DEFAULT_MODEL,
            "äh kodex und vierzehn".into(),
            Default::default(),
            || Store::open(&database),
        )));
        assert_eq!(session.ports.inserts.last(), Some(&(1, expected.into())));
        session.inserted(Ok(()));
        session.repeat(Some(2));
        assert_eq!(session.ports.inserts.last(), Some(&(2, expected.into())));
        session.inserted(Ok(()));
    };
    dictate("Codex und 14.");
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
    dictate("Mein CODE und 14.");
    store.delete(id).unwrap();
    dictate("Kodex und 14.");
}

#[test]
fn unreadable_dictionary_keeps_raw_text_for_insertion_and_displays_the_warning() {
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("kein-sqlite.db");
    std::fs::write(&database, b"invalid sqlite database").unwrap();
    let mut session = setup();
    session.start(Some(1));
    session.stop();
    session.recognized(Ok(Transcript::from_dictation(
        DEFAULT_MODEL,
        "kodex Strasse".into(),
        Default::default(),
        || Store::open(&database),
    )));
    assert_eq!(session.ports.inserts, [(1, "kodex Strasse".into())]);
    session.inserted(Ok(()));
    assert!(
        matches!(&session.ports.overlay, OverlayState::Error { message, text }
        if message.contains("Wörterbuch") && text == "kodex Strasse")
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
    let output = |model| {
        Transcript::from_dictation(
            model,
            input.into(),
            crate::settings::CleanupSettings {
                punctuation: false,
                ..Default::default()
            },
            || Store::open(&path),
        )
        .record
        .finished
    };
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
            let transcript = Transcript::from_dictation(
                model,
                input.to_owned(),
                crate::settings::CleanupSettings {
                    punctuation: false,
                    ..Default::default()
                },
                || Store::open(&path),
            );
            assert_eq!(transcript.record.finished, expected);
            assert!(transcript.warning.is_none());
        }
    }
}

#[test]
fn silence_skips_the_dictionary() {
    let transcript =
        Transcript::from_dictation(DEFAULT_MODEL, "  ".into(), Default::default(), || {
            panic!("unexpected database access")
        });
    assert_eq!(transcript.record.finished, "  ");
    assert!(transcript.warning.is_none());
}
