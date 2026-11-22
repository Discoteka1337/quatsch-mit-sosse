use icu_normalizer::ComposingNormalizerBorrowed;
use std::collections::HashMap;

use crate::{
    Pipeline, Rule,
    tokens::{Kind, Token, identifier_edge, word_key},
};

/// Cleanup for German software dictation using the project's published terminology.
pub fn software_dictation_pipeline() -> Pipeline {
    let mut pipeline = Pipeline::new([
        Rule::Dictionary,
        Rule::FillerWords,
        Rule::SelfCorrections,
        Rule::Numbers,
        Rule::Punctuation,
    ]);
    pipeline.proper_names = [
        "Codex",
        "Commit",
        "Pull Request",
        "Pull Requests",
        "Affiliate-Link",
        "Affiliate-Links",
        "Feed",
        "ChatGPT",
        "OpenAI",
        "GitHub",
        "Claude Code",
        "Wispr Flow",
        "PowerShell",
        "E-Mail",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    pipeline
}

struct Term<'a> {
    spelling: &'a str,
    parts: Vec<String>,
}

fn hyphen(ch: char) -> bool {
    matches!(ch, '-' | '‐' | '‑')
}

fn joiner(token: &Token) -> bool {
    token.kind == Kind::Punctuation && matches!(token.text.as_str(), "-" | "‐" | "‑")
}

impl<'a> Term<'a> {
    fn parse(spelling: &'a str) -> Option<Self> {
        // Author-supplied spaces, hyphens and camel case establish the only
        // permitted word boundaries. Never guess a split inside a constituent.
        let spelling = spelling.trim();
        let mut parts = Vec::new();
        let normalized = ComposingNormalizerBorrowed::new_nfc().normalize(spelling);
        for word in normalized.split(|ch| ch == ' ' || hyphen(ch)) {
            if word.is_empty() || !word.chars().all(char::is_alphabetic) {
                return None;
            }
            let mut start = 0;
            let mut previous = None;
            let mut letters = word.char_indices().peekable();
            while let Some((index, ch)) = letters.next() {
                let ends_acronym = letters.peek().is_some_and(|(_, next)| next.is_lowercase());
                if previous.is_some_and(|prev: char| prev.is_lowercase() || ends_acronym)
                    && ch.is_uppercase()
                {
                    parts.push(word_key(&word[start..index]));
                    start = index;
                }
                previous = Some(ch);
            }
            if start < word.len() {
                parts.push(word_key(&word[start..]));
            }
        }
        (parts.len() > 1).then_some(Self { spelling, parts })
    }

    fn match_length(&self, tokens: &[Token]) -> Option<usize> {
        let mut parts = self.parts.iter();
        let mut at = 0;
        loop {
            let token = tokens.get(at)?;
            if token.kind != Kind::Word {
                return None;
            }
            let word = word_key(&token.text);
            let mut rest = word.as_str();
            while !rest.is_empty() {
                rest = rest.strip_prefix(parts.next()?.as_str())?;
            }
            at += 1;
            if parts.len() == 0 {
                return (!identifier_edge(tokens[at..].iter())).then_some(at);
            }
            let separator = tokens.get(at)?;
            if separator.kind != Kind::Space && !joiner(separator) {
                return None;
            }
            at += 1;
        }
    }
}

pub(super) fn apply(tokens: &mut Vec<Token>, names: &[String]) {
    let terms: Vec<_> = names.iter().filter_map(|name| Term::parse(name)).collect();
    if terms.is_empty() {
        return;
    }
    let mut by_start: HashMap<String, Vec<&Term<'_>>> = HashMap::new();
    for term in &terms {
        let mut head = String::new();
        for part in &term.parts {
            head.push_str(part);
            by_start.entry(head.clone()).or_default().push(term);
        }
    }
    let mut input = std::mem::take(tokens).into_iter();
    while let Some(first) = input.as_slice().first() {
        let mut longest = 0;
        let mut spelling = None;
        if first.kind == Kind::Word
            && !identifier_edge(tokens.iter().rev())
            && let Some(candidates) = by_start.get(&word_key(&first.text))
        {
            for term in candidates {
                if let Some(length) = term.match_length(input.as_slice()) {
                    if length > longest {
                        longest = length;
                        spelling = Some(term.spelling);
                    } else if length == longest && spelling != Some(term.spelling) {
                        spelling = None;
                    }
                }
            }
        }
        if let Some(text) = spelling {
            input.by_ref().take(longest).for_each(drop);
            tokens.push(Token::new(Kind::Protected, text));
        } else if let Some(token) = input.next() {
            tokens.push(token);
        }
    }
}
