use quatsch_mit_sosse_textfx::{FillerStrength, Pipeline, Rule};

#[test]
fn stages_have_stable_names_and_order_and_can_be_toggled_separately() {
    assert_eq!(
        Rule::ORDER.map(Rule::name),
        [
            "voice_commands",
            "dictionary",
            "filler_words",
            "self_corrections",
            "numbers",
            "punctuation",
            "snippets",
            "style"
        ]
    );
    let mut pipeline = Pipeline::new([]);
    for selected in Rule::ORDER {
        pipeline.set_enabled(selected, true);
        for rule in Rule::ORDER {
            assert_eq!(pipeline.is_enabled(rule), rule == selected);
        }
        pipeline.set_enabled(selected, false);
    }
}

#[test]
fn default_removes_only_standard_fillers_and_keeps_spoken_punctuation() {
    let pipeline = Pipeline::default();
    assert!(!pipeline.is_enabled(Rule::VoiceCommands));
    assert_eq!(pipeline.filler_strength, FillerStrength::Standard);
    assert_eq!(
        pipeline.apply("Wir, ähm, warten halt Punkt"),
        "Wir warten halt Punkt"
    );
}

#[test]
fn toggling_after_first_use_changes_the_next_result() {
    let mut pipeline = Pipeline::default();
    pipeline.set_enabled(Rule::VoiceCommands, true);
    assert_eq!(pipeline.apply("Ja äh Punkt"), "Ja.");
    pipeline.set_enabled(Rule::FillerWords, false);
    assert_eq!(pipeline.apply("Ja äh Punkt"), "Ja äh.");
    pipeline.set_enabled(Rule::VoiceCommands, false);
    assert_eq!(pipeline.apply("Ja äh Punkt"), "Ja äh Punkt");
}

#[test]
fn review_ranges_point_to_each_occurrence_in_the_finished_utf8_text() {
    let mut pipeline = Pipeline::new([Rule::Numbers, Rule::SelfCorrections, Rule::Punctuation]);
    let input = "über dreizehn Fälle. Das scheint seltsam, äh, schwierig. Noch einmal seltsam, äh, schwierig.";
    let phrase = "seltsam, äh, schwierig";
    let result = pipeline.apply_with_review(input);
    assert_eq!(
        result.text,
        "Über 13 Fälle. Das scheint seltsam, äh, schwierig. Noch einmal seltsam, äh, schwierig."
    );
    let expected_ranges: Vec<_> = result
        .text
        .match_indices(phrase)
        .map(|(start, matched)| start..start + matched.len())
        .collect();
    assert_eq!(result.uncertain, expected_ranges);
    assert_eq!(pipeline.apply(input), result.text);
    assert_eq!(pipeline.apply_with_review(&result.text), result);
    pipeline.set_enabled(Rule::SelfCorrections, false);
    assert!(pipeline.apply_with_review(input).uncertain.is_empty());
}

#[test]
fn number_formatting_preserves_words_inside_addresses_and_paths() {
    let pipeline = Pipeline::new([Rule::Numbers]);
    assert_eq!(
        pipeline.apply("dreizehn+post@beispiel.de https://beispiel.de/vierzehn dreizehn.de C:\\Post\\zwanzig für dreizehn Personen"),
        "dreizehn+post@beispiel.de https://beispiel.de/vierzehn dreizehn.de C:\\Post\\zwanzig für 13 Personen"
    );
}

#[test]
fn an_ambiguous_correction_never_replaces_just_the_suffix_of_an_address() {
    let pipeline = Pipeline::new([Rule::SelfCorrections]);
    for text in [
        "Schreib an post@beispiel.de, nein, ich meine Post.",
        "Rufe beispiel.de, nein, ich meine Post.",
        "Schreib an post@beispiel.de, streich das.",
    ] {
        let result = pipeline.apply_with_review(text);
        assert_eq!(result.text, text);
        assert_eq!(result.uncertain.len(), 1);
    }
}

#[test]
fn punctuation_keeps_a_standalone_address_or_file_path_ready_to_paste() {
    let pipeline = Pipeline::new([Rule::Punctuation]);
    for text in [
        "post+abteilung@example.de",
        "https://example.de/suche?q=grün",
        "beispiel.de",
        "pfad/ZurDatei",
        "  c:\\Post\\Abholung.txt\r\n",
    ] {
        assert_eq!(pipeline.apply(text), text);
    }
    assert_eq!(
        pipeline.apply("antwort an post@example.de"),
        "Antwort an post@example.de."
    );
}
