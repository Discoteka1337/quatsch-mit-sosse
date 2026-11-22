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
