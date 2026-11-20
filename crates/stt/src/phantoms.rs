use icu_normalizer::ComposingNormalizerBorrowed;
use std::{str::SplitWhitespace, sync::LazyLock};

static PHRASES: LazyLock<Vec<String>> = LazyLock::new(|| {
    include_str!("../phantoms.txt")
        .lines()
        .map(fold)
        .filter(|phrase| words(phrase).next().is_some())
        .collect()
});

fn fold(text: &str) -> String {
    ComposingNormalizerBorrowed::new_nfc()
        .normalize(text)
        .to_lowercase()
}

fn words(text: &str) -> SplitWhitespace<'_> {
    text.trim_matches(|c: char| c.is_whitespace() || ".,!?…:;\"'„“”‚‘’«»‹›".contains(c))
        .split_whitespace()
}

pub(super) fn is_phantom(text: &str) -> bool {
    let folded = fold(text);
    let transcript = words(&folded);
    PHRASES
        .iter()
        .any(|phrase| transcript.clone().eq(words(phrase)))
}
