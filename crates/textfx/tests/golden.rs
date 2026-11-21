use std::{collections::HashSet, fs, path::Path};

use quatsch_mit_sosse_textfx::{FillerStrength, Pipeline, Rule};
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
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Strength {
    #[default]
    Standard,
    Strong,
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

    let mut count = 0;
    let mut failures = Vec::new();
    for path in paths {
        let file = path.file_name().unwrap().to_string_lossy();
        let cases = fs::read_to_string(&path).unwrap();
        let suite: Suite = toml::from_str(&cases).unwrap_or_else(|err| panic!("{file}: {err}"));
        assert!(!suite.case.is_empty(), "{file} is empty");
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
            let actual = pipeline.apply(&case.input);
            if actual != case.expected {
                failures.push(format!(
                    "{id}: expected {:?}, got {actual:?}",
                    case.expected
                ));
            }
            count += 1;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        count >= 80,
        "Expected at least 80 golden cases, found {count}"
    );
}
