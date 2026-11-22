use crate::tokens::{Kind, Token, dictionary_key, identifier_edge, tokenize};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DictionaryEntry {
    pub term: String,
    pub replacement: Option<String>,
    pub aliases: Vec<String>,
}

struct Variant<'a> {
    keys: Vec<String>,
    written: &'a str,
}

fn index_variants<'a>(
    entries: &'a [DictionaryEntry],
    names: &[String],
) -> HashMap<String, Vec<Variant<'a>>> {
    let mut variants = HashMap::<String, Vec<Variant<'_>>>::new();
    for entry in entries {
        let written = entry.replacement.as_deref().unwrap_or(&entry.term);
        for form in std::iter::once(&entry.term).chain(&entry.aliases) {
            let pattern = tokenize(form.trim(), names);
            if pattern.is_empty() || pattern.iter().any(|t| t.kind == Kind::LineBreak) {
                continue;
            }
            let keys: Vec<_> = pattern.iter().map(dictionary_key).collect();
            variants
                .entry(keys[0].clone())
                .or_default()
                .push(Variant { keys, written });
        }
    }
    variants
}

fn touches_identifier<'a>(mut neighbors: impl Iterator<Item = &'a Token> + Clone) -> bool {
    identifier_edge(neighbors.clone())
        || neighbors
            .next()
            .is_some_and(|t| matches!(t.kind, Kind::Word | Kind::Protected))
}

pub(super) fn apply(tokens: &mut Vec<Token>, entries: &[DictionaryEntry], names: &[String]) {
    if entries.is_empty() || tokens.is_empty() {
        return;
    }
    let variants = index_variants(entries, names);
    let input = std::mem::take(tokens);
    let keys: Vec<_> = input.iter().map(dictionary_key).collect();
    let mut start = 0;
    while start < input.len() {
        let mut longest = 0;
        let mut written = None;
        if let Some(candidates) = variants.get(&keys[start])
            && !touches_identifier(input[..start].iter().rev())
        {
            for candidate in candidates {
                let length = candidate.keys.len();
                let end = start + length;
                if length < longest
                    || keys.get(start..end) != Some(&candidate.keys)
                    || touches_identifier(input[end..].iter())
                    || input[start..end]
                        .iter()
                        .any(|t| t.kind == Kind::CommandPunctuation)
                {
                    continue;
                }
                if length > longest {
                    longest = length;
                    written = Some(candidate.written);
                } else if written != Some(candidate.written) {
                    written = None;
                }
            }
        }
        if let Some(text) = written {
            tokens.push(Token::new(Kind::Protected, text));
        } else {
            // Consume the whole ambiguous phrase so a shorter alias cannot rewrite its tail.
            tokens.extend_from_slice(&input[start..start + longest.max(1)]);
        }
        start += longest.max(1);
    }
}
