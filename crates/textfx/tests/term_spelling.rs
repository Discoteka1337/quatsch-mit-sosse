use quatsch_mit_sosse_textfx::{Pipeline, Rule, software_dictation_pipeline};

fn dictionary(terms: &[&str]) -> Pipeline {
    let mut pipeline = Pipeline::new([Rule::Dictionary]);
    pipeline.proper_names = terms.iter().map(|term| String::from(*term)).collect();
    pipeline
}

#[test]
fn uses_the_configured_spelling_for_spaced_hyphenated_and_joined_parts() {
    let pipeline = dictionary(&["Pull Request", "Affiliate-Link", "CloudWerk"]);
    for (input, expected) in [
        (
            "Der Pull-Request ist fertig.",
            "Der Pull Request ist fertig.",
        ),
        ("Ein pullrequest!", "Ein Pull Request!"),
        (
            "Affiliate Link und AffiliateLink",
            "Affiliate-Link und Affiliate-Link",
        ),
        ("cloud werk", "CloudWerk"),
        (
            "Wir sehen Cloud-Werk und cloud werk.",
            "Wir sehen CloudWerk und CloudWerk.",
        ),
    ] {
        assert_eq!(pipeline.apply(input), expected);
    }
}

#[test]
fn respects_existing_spelling_and_protection_inside_dictionary_entries() {
    let pipeline = dictionary(&["Pull Request", "Studio Drei", "Affiliate-Link"]);
    for input in ["PULL   REQUEST", "studio   Drei", "AFFILIATE-LINK"] {
        assert_eq!(pipeline.apply(input), input);
    }
}

#[test]
fn only_joins_known_parts_at_word_boundaries_on_the_same_line() {
    let pipeline = dictionary(&["Pull Request", "Affiliate-Link", "Heute"]);
    for input in [
        "Pull\nRequest",
        "Affiliate, Link",
        "Pull:Request",
        "Der Affiliate Link-Katalog",
        "Pull - Request",
        "Kunden-PullRequest",
        "PullRequestEdit",
        "der nicht registrierte Pull Requester",
        "Affili ateLink",
        "Heu te",
        "Heute ist es sonnig.",
    ] {
        assert_eq!(pipeline.apply(input), input);
    }
}

#[test]
fn prefers_the_longest_authorized_term_and_leaves_conflicting_spellings_untouched() {
    let longest = dictionary(&["Solar Fabrik", "Solar Fabrik West"]);
    assert_eq!(
        longest.apply("Bei Solarfabrikwest"),
        "Bei Solar Fabrik West"
    );
    for terms in [
        ["Solar Fabrik", "Solar-Fabrik"],
        ["Solar-Fabrik", "Solar Fabrik"],
    ] {
        assert_eq!(dictionary(&terms).apply("Solarfabrik"), "Solarfabrik");
    }
}

#[test]
fn only_runs_when_configured_and_enabled() {
    let mut pipeline = dictionary(&["CloudWerk"]);
    pipeline.set_enabled(Rule::Dictionary, false);
    assert_eq!(pipeline.apply("cloud werk"), "cloud werk");
    assert_eq!(dictionary(&[]).apply("cloud werk"), "cloud werk");
}

#[test]
fn entries_with_markup_or_layout_do_not_inject_formatting() {
    for spelling in [
        "-Nord West",
        "Nord-West-",
        "Nord--West",
        "Nord\nWest",
        "Nord / West",
    ] {
        assert_eq!(dictionary(&[spelling]).apply("Nordwest"), "Nordwest");
    }
}

#[test]
fn handles_acronym_boundaries_and_equivalent_german_umlauts() {
    let pipeline = dictionary(&["XMLParser", "Fu\u{308}hlWerk"]);
    assert_eq!(pipeline.apply("XML Parser"), "XMLParser");
    assert_eq!(pipeline.apply("Fühl Werk"), "Fu\u{308}hlWerk");
}

#[test]
fn leaves_file_paths_addresses_and_identifiers_intact() {
    let pipeline = dictionary(&["Pull Request"]);
    for input in [
        "an hilfe@pullrequest.de",
        "pullrequest.dev",
        "mein.pullrequest",
        "/api/pullrequest",
        "C:\\pullrequest",
        "#pullrequest",
        "pullrequest@example.org",
        "`pullrequest`",
        "dein_pullrequest",
        "q=pullrequest",
        "crate::pullrequest",
        "pullrequest()",
        "pullrequest<Id>",
    ] {
        assert_eq!(pipeline.apply(input), input);
    }
}

#[test]
fn software_profile_combines_term_spelling_with_the_cleanup_rules() {
    let pipeline = software_dictation_pipeline();
    assert_eq!(
        pipeline.apply("ähm, der Pull-Request enthält dreizehn Affiliate Links"),
        "Der Pull Request enthält 13 Affiliate-Links."
    );
    assert_eq!(
        pipeline.apply("Chat GPT und Git Hub"),
        "ChatGPT und GitHub."
    );
    assert!(!pipeline.is_enabled(Rule::VoiceCommands));
}
