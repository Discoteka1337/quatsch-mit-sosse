use super::*;
use SegmentKind::{Added, Removed, Uncertain, Unchanged};
use quatsch_mit_sosse_textfx::{Pipeline, Rule};

fn record(original: &str, finished: &str) -> DictationRecord {
    DictationRecord {
        original: original.to_owned(),
        finished: finished.to_owned(),
        uncertain: Vec::new(),
        cursor: None,
    }
}

fn runs<'a>(segments: &[Segment<'a>]) -> Vec<(&'a str, SegmentKind)> {
    segments.iter().map(|part| (part.text, part.kind)).collect()
}

#[test]
fn unchanged_multiline_text_is_one_unmarked_run_on_each_side() {
    let text = "Grüße 👩🏽‍💻\r\n\nDie Soße!  ";
    let entry = record(text, text);
    let view = entry.comparison();
    assert_eq!(runs(&view.before), [(text, Unchanged)]);
    assert_eq!(runs(&view.after), [(text, Unchanged)]);
}

#[test]
fn dictionary_replacements_and_new_punctuation_leave_other_words_unmarked() {
    let entry = record("Ich nutze kodex für Grüße", "Ich nutze Codex für Grüße.");
    let view = entry.comparison();
    assert_eq!(
        runs(&view.before),
        [
            ("Ich nutze ", Unchanged),
            ("kodex", Removed),
            (" für Grüße", Unchanged)
        ]
    );
    assert_eq!(
        runs(&view.after),
        [
            ("Ich nutze ", Unchanged),
            ("Codex", Added),
            (" für Grüße", Unchanged),
            (".", Added)
        ]
    );
}

#[test]
fn visible_filler_deletion_preserves_the_original_and_the_full_final_text() {
    let raw = "Wir, äh, warten.";
    let cleaned = Pipeline::default().apply_with_review(raw);
    let entry = record(raw, &cleaned.text);
    let view = entry.comparison();
    assert_eq!(runs(&view.after), [("Wir warten.", Unchanged)]);
    assert_eq!(
        view.before.iter().map(|part| part.text).collect::<String>(),
        raw
    );
    assert!(
        view.before
            .iter()
            .any(|part| part.kind == Removed && part.text.contains("äh"))
    );
}

#[test]
fn uncertainty_follows_the_pipeline_final_offsets_after_shortening_and_repeated_words() {
    let raw = "Über dreizehn Fälle. Das scheint seltsam, äh, schwierig. Noch einmal seltsam, äh, schwierig.";
    let result = Pipeline::new([Rule::SelfCorrections, Rule::Numbers]).apply_with_review(raw);
    let entry = DictationRecord {
        original: raw.to_owned(),
        finished: result.text,
        uncertain: result.uncertain,
        cursor: result.cursor,
    };
    let view = entry.comparison();
    assert_eq!(
        runs(&view.after),
        [
            ("Über ", Unchanged),
            ("13", Added),
            (" Fälle. Das scheint ", Unchanged),
            ("seltsam, äh, schwierig", Uncertain),
            (". Noch einmal ", Unchanged),
            ("seltsam, äh, schwierig", Uncertain),
            (".", Unchanged),
        ]
    );
}

#[test]
fn uncertainty_overrides_an_edit_and_can_split_a_word_at_utf8_boundaries() {
    let mut entry = record("A!", "Öl!");
    entry.uncertain.push("Ö".len()..entry.finished.len());
    let view = entry.comparison();
    assert_eq!(runs(&view.after), [("Ö", Added), ("l!", Uncertain)]);
    assert_eq!(runs(&view.before), [("A", Removed), ("!", Unchanged)]);
}

#[test]
fn an_entirely_added_or_removed_text_and_an_empty_record_are_displayable() {
    let empty = record("", "");
    assert!(empty.comparison().before.is_empty());
    assert!(empty.comparison().after.is_empty());
    let addition = record("", "ß\n👋");
    let deletion = record("ß\n👋", "");
    assert_eq!(runs(&addition.comparison().after), [("ß\n👋", Added)]);
    assert_eq!(runs(&deletion.comparison().before), [("ß\n👋", Removed)]);
}
