use quatsch_mit_sosse_textfx::{DictionaryEntry, Pipeline, Rule};

fn entry(term: &str, replacement: Option<&str>, aliases: &[&str]) -> DictionaryEntry {
    DictionaryEntry {
        term: term.into(),
        replacement: replacement.map(str::to_owned),
        aliases: aliases.iter().map(|alias| (*alias).into()).collect(),
    }
}

#[test]
fn writes_canonical_case_for_terms_and_whole_spoken_forms() {
    let mut rules = Pipeline::new([Rule::Dictionary]);
    rules.dictionary = vec![
        entry("Codex", None, &["kodex"]),
        entry("Anton-Mall-Stadion", None, &["Anton Mall Stadion"]),
        entry("Süß & Co.", None, &["süße firma"]),
    ];
    assert_eq!(
        rules.apply("KODEX, codex und Anton   Mall Stadion bei SU\u{308}ẞE Firma!"),
        "Codex, Codex und Anton-Mall-Stadion bei Süß & Co.!"
    );
    let protected = "kodexe mein_kodex #kodex /kodex kodex.de kodex@test.de Anton\nMall Stadion";
    assert_eq!(rules.apply(protected), protected);
    rules.set_enabled(Rule::Dictionary, false);
    assert_eq!(rules.apply("kodex"), "kodex");
}

#[test]
fn address_parts_never_expand_as_aliases_but_a_whole_registered_address_can() {
    let mut rules = Pipeline::new([Rule::Dictionary]);
    rules.dictionary = vec![
        entry("Werkzeug", None, &["kodex"]),
        entry("Postfach", None, &["brief@example.test"]),
    ];
    assert_eq!(
        rules.apply(
            "kodex+post@example.test, https://host/k=k+kOdex%20neu und kodex+tag.firma. kodex!"
        ),
        "kodex+post@example.test, https://host/k=k+kOdex%20neu und kodex+tag.firma. Werkzeug!"
    );
    assert_eq!(rules.apply("„brief@example.test“!"), "„Postfach“!");
    assert_eq!(
        rules.apply("brief@example.test+extra"),
        "brief@example.test+extra"
    );
}

#[test]
fn dictionary_aliases_preserve_existing_numeric_literals_and_their_spelling() {
    let mut rules = Pipeline::new([Rule::Dictionary]);
    rules.dictionary = vec![
        entry("zwölf", None, &["12"]),
        entry("Kennung 99", None, &["Kennung 0099"]),
        entry("Preis", None, &["2,25 Euro"]),
        entry("Halle 12", None, &["halle zwölf"]),
    ];
    assert_eq!(
        rules.apply("12 Kennung 0099 für 2,25 Euro"),
        "12 Kennung 0099 für 2,25 Euro"
    );
    assert_eq!(
        rules.apply("halle 12 und halle zwölf"),
        "Halle 12 und Halle 12"
    );
}

#[test]
fn honors_the_longest_phrase_and_does_not_chain_or_reformat_replacements() {
    let mut rules = Pipeline::new([Rule::Dictionary, Rule::FillerWords, Rule::Punctuation]);
    rules.dictionary = vec![
        entry("Claude", None, &["klood"]),
        entry("Team", Some("iTeam"), &["klood code"]),
        entry("iTeam", Some("Andere"), &[]),
        entry("Schluss", Some("ähm"), &["das ende"]),
    ];
    assert_eq!(
        rules.apply("klood code, team, klood, das ende"),
        "iTeam, iTeam, Claude, ähm."
    );
}

#[test]
fn handles_ambiguous_aliases_without_using_storage_order() {
    let mut rules = Pipeline::new([Rule::Dictionary]);
    rules.dictionary = vec![
        entry("Saal A", None, &["große halle"]),
        entry("Saal B", None, &["GROẞE HALLE"]),
        entry("Halle", None, &[]),
    ];
    for _ in 0..2 {
        assert_eq!(rules.apply("die große halle"), "die große halle");
        rules.dictionary.reverse();
    }
}

#[test]
fn supports_existing_protected_names_and_alphanumeric_terms() {
    let mut rules = quatsch_mit_sosse_textfx::software_dictation_pipeline();
    rules.dictionary = vec![
        entry("ChatGPT", Some("GPT-4o"), &["chatbot"]),
        entry("N8", None, &["en acht"]),
    ];
    assert_eq!(
        rules.apply("chatgpt und en acht oder n8"),
        "GPT-4o und N8 oder N8."
    );
}

#[test]
fn keeps_terms_with_punctuation_out_of_longer_identifiers() {
    let mut rules = Pipeline::new([Rule::Dictionary]);
    rules.dictionary = vec![
        entry(".NET", Some("dotnet"), &[]),
        entry("C++", Some("C plus plus"), &[]),
    ];
    assert_eq!(
        rules.apply("code.net oder c++String, aber .net und c++!"),
        "code.net oder c++String, aber dotnet und C plus plus!"
    );
}

#[test]
fn shields_registered_names_from_commands_and_fillers_even_when_replacement_is_off() {
    let mut rules = Pipeline::new([Rule::VoiceCommands, Rule::Dictionary, Rule::FillerWords]);
    rules.dictionary = vec![
        entry("Haus Punkt", Some("Haus P."), &[]),
        entry("Ähm", None, &[]),
        entry("Frage", None, &["Fragezeichen"]),
    ];
    assert_eq!(rules.apply("ähm im haus punkt"), "Ähm im Haus P.");
    assert_eq!(rules.apply("Alles klar Fragezeichen"), "Alles klar?");
    rules.set_enabled(Rule::Dictionary, false);
    assert_eq!(rules.apply("ähm im haus punkt"), "ähm im haus punkt");
}
