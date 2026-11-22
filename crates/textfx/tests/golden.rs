use std::{collections::HashSet, fs, path::Path};

use quatsch_mit_sosse_textfx::{FillerStrength, Pipeline, Rule, Snippet, SnippetValue};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Suite {
    case: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    input: String,
    rules: Vec<String>,
    expected: String,
    #[serde(default)]
    strength: Strength,
    #[serde(default)]
    proper_names: Vec<String>,
    digits_from: Option<u32>,
    #[serde(default)]
    uncertain: Vec<String>,
    #[serde(default)]
    snippets: Vec<[String; 2]>,
    clipboard: Option<String>,
    cursor: Option<usize>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Strength {
    #[default]
    Standard,
    Strong,
}

#[test]
fn corrections_fixture_has_no_trailing_blank_line() {
    let last_line = include_str!("golden/corrections.toml").lines().next_back();
    assert_ne!(
        last_line,
        Some(""),
        "corrections.toml ends with a blank line"
    );
}

#[test]
fn run_all_golden_files() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let mut paths: Vec<_> = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    paths.sort();

    let mut part_two_count = 0;
    let mut count = 0;
    let mut failures = Vec::new();
    for path in paths {
        let file = path.file_name().unwrap().to_string_lossy();
        let cases = fs::read_to_string(&path).unwrap();
        let suite: Suite = toml::from_str(&cases).unwrap_or_else(|err| panic!("{file}: {err}"));
        assert!(!suite.case.is_empty(), "{file} is empty");
        if matches!(
            file.as_ref(),
            "corrections.toml" | "numbers.toml" | "punctuation.toml" | "spelling.toml"
        ) {
            part_two_count += suite.case.len();
        }
        let mut names = HashSet::new();
        for case in suite.case {
            let id = format!("{file} / {}", case.name);
            assert!(names.insert(case.name), "Duplicate case: {id}");
            let rules = case.rules.iter().map(|name| {
                Rule::ORDER
                    .into_iter()
                    .find(|rule| rule.name() == name)
                    .unwrap_or_else(|| panic!("Unknown rule {name} in {id}"))
            });
            let mut pipeline = Pipeline::new(rules);
            pipeline.proper_names = case.proper_names;
            pipeline.filler_strength = match case.strength {
                Strength::Standard => FillerStrength::Standard,
                Strength::Strong => FillerStrength::Strong,
            };
            if let Some(threshold) = case.digits_from {
                pipeline.digits_from = threshold;
            }
            pipeline.snippets = case
                .snippets
                .into_iter()
                .map(|[trigger, text]| Snippet { trigger, text })
                .collect();
            let actual = pipeline.apply_with_snippets(&case.input, |value| match value {
                SnippetValue::Date => "22.11.2026".into(),
                SnippetValue::Time => "09:07".into(),
                SnippetValue::Clipboard => case
                    .clipboard
                    .clone()
                    .expect("unexpected clipboard request"),
            });
            let uncertain: Vec<_> = actual
                .uncertain
                .iter()
                .map(|range| &actual.text[range.clone()])
                .collect();
            if actual.text != case.expected
                || uncertain != case.uncertain
                || actual.cursor != case.cursor
            {
                failures.push(format!(
                    "{id}: expected {:?} (uncertain {:?}, cursor {:?}), got {:?} (uncertain {uncertain:?}, cursor {:?})",
                    case.expected, case.uncertain, case.cursor, actual.text, actual.cursor
                ));
            }
            count += 1;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        part_two_count >= 80,
        "Expected 80 additional golden cases, found {part_two_count}"
    );
    assert!(
        count - part_two_count >= 80,
        "Expected at least 80 golden cases from the first part"
    );
}

#[test]
fn a_dictation_reads_each_requested_value_once() {
    let mut pipeline = Pipeline::new([Rule::Snippets]);
    pipeline.snippets = vec![Snippet {
        trigger: "Vorlage".into(),
        text: "{zwischenablage} {zwischenablage}".into(),
    }];
    let mut reads = 0;
    let actual = pipeline.apply_with_snippets("Vorlage. Vorlage!", |value| {
        assert_eq!(value, SnippetValue::Clipboard);
        reads += 1;
        "Kopie".into()
    });
    assert_eq!(actual.text, "Kopie Kopie Kopie Kopie");
    assert_eq!(reads, 1);
}

#[test]
fn an_unavailable_placeholder_keeps_only_the_affected_sentences() {
    let mut pipeline = Pipeline::new([Rule::Snippets]);
    pipeline.snippets = vec![
        Snippet {
            trigger: "Eins".into(),
            text: "Erster Treffer.".into(),
        },
        Snippet {
            trigger: "Zwei".into(),
            text: "Verworfen{cursor}{zwischenablage}".into(),
        },
        Snippet {
            trigger: "Drei".into(),
            text: "Ende{cursor}.".into(),
        },
    ];
    let mut reads = 0;
    let result = pipeline.apply_with_snippet_values("Eins. Zwei?! Zwei. Drei!", |value| {
        assert_eq!(value, SnippetValue::Clipboard);
        reads += 1;
        None
    });
    assert_eq!(reads, 1);
    assert_eq!(result.text, "Erster Treffer. Zwei?! Zwei. Ende.");
    assert_eq!(result.cursor, result.text.rfind('.'));
}
