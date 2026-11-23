mod triggers;

use crate::{
    numbers::named_month,
    punctuation::abbreviation,
    tokens::{Kind, Token, is_address_field, tokenize, word_key},
};
use std::{cell::OnceCell, ops::Range};
use triggers::{Match, Triggers};
pub use triggers::{same_snippet_numbers, snippet_trigger_key};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snippet {
    pub trigger: String,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnippetValue {
    Date,
    Time,
    Clipboard,
}

fn whitespace(token: &Token) -> bool {
    matches!(token.kind, Kind::Space | Kind::LineBreak)
}

fn sentence_mark(token: &Token) -> bool {
    matches!(token.kind, Kind::Punctuation | Kind::CommandPunctuation)
        && matches!(token.text.as_str(), "." | "!" | "?" | "…")
}

fn closing(token: &Token) -> bool {
    matches!(
        token.text.as_str(),
        "“" | "‘" | "«" | "‹" | "\"" | ")" | "]" | "}"
    )
}

fn protect_address_marks(tokens: &mut [Token]) {
    for field in tokens.split_mut(whitespace) {
        if is_address_field(field)
            && let Some(last) = field.iter().rposition(|t| !sentence_mark(t) && !closing(t))
        {
            for token in &mut field[..last] {
                if token.kind == Kind::Punctuation && sentence_mark(token) {
                    token.kind = Kind::Protected;
                }
            }
        }
    }
}

fn internal_period(before: &[Token], after: &[Token]) -> bool {
    let Some((word, prefix)) = before.split_last() else {
        return false;
    };
    if abbreviation(&word.text) || (word.kind == Kind::Word && word.text.chars().count() == 1) {
        return true;
    }
    if !word.text.ends_with(char::is_numeric) {
        return false;
    }
    let next = after.iter().find(|t| t.kind != Kind::Space);
    if next.is_some_and(|t| named_month(&word_key(&t.text)).is_some()) {
        return true;
    }
    // A period after a day or ordinal belongs to the phrase, e.g. "am 7.
    // im Büro" or "der 3. Versuch". Do not activate a template in its tail.
    word.text.chars().all(char::is_numeric)
        && (next.is_some_and(|t| t.text.starts_with(char::is_lowercase))
            || prefix
                .iter()
                .rev()
                .find(|t| t.kind != Kind::Space)
                .is_some_and(|t| {
                    matches!(
                        word_key(&t.text).as_str(),
                        "am" | "im"
                            | "vom"
                            | "zum"
                            | "beim"
                            | "der"
                            | "die"
                            | "das"
                            | "dem"
                            | "den"
                            | "des"
                            | "ein"
                            | "eine"
                            | "einem"
                            | "einen"
                            | "einer"
                            | "eines"
                    )
                }))
}

// Work on original token boundaries so a decimal point, a protected name or
// a period inside a URL cannot turn a substring into a standalone sentence.
fn sentence_length(tokens: &[Token]) -> usize {
    let mut at = 0;
    while at < tokens.len() {
        if tokens[at].kind == Kind::LineBreak
            && tokens[at + 1..]
                .iter()
                .take_while(|t| whitespace(t))
                .any(|t| t.kind == Kind::LineBreak)
        {
            return at;
        }
        if !sentence_mark(&tokens[at]) {
            at += 1;
            continue;
        }
        let end_marks = at + tokens[at..].iter().take_while(|t| sentence_mark(t)).count();
        let end = end_marks
            + tokens[end_marks..]
                .iter()
                .take_while(|t| closing(t))
                .count();
        let short_form = tokens[at].kind == Kind::Punctuation
            && tokens[at].text == "."
            && end_marks == at + 1
            && internal_period(&tokens[..at], &tokens[end..]);
        if !short_form
            && (tokens.get(end).is_none_or(whitespace)
                || tokens
                    .get(end)
                    .is_some_and(|t| matches!(t.kind, Kind::Word | Kind::Protected)))
        {
            return end;
        }
        at = end;
    }
    tokens.len()
}

fn expand(
    mut template: &str,
    cache: &[OnceCell<Option<String>>; 3],
    resolve: &mut impl FnMut(SnippetValue) -> Option<String>,
) -> Option<Vec<Token>> {
    let mut output = Vec::new();
    let mut literal = String::new();
    while let Some((before, marker)) = template.split_once('{') {
        literal.push_str(before);
        if let Some(rest) = marker.strip_prefix("cursor}") {
            output.push(Token::new(Kind::Protected, std::mem::take(&mut literal)));
            output.push(Token::new(Kind::Cursor, ""));
            template = rest;
        } else if let Some((kind, rest)) = [
            ("datum}", SnippetValue::Date),
            ("uhrzeit}", SnippetValue::Time),
            ("zwischenablage}", SnippetValue::Clipboard),
        ]
        .into_iter()
        .find_map(|(name, kind)| marker.strip_prefix(name).map(|rest| (kind, rest)))
        {
            literal.push_str(
                cache[kind as usize]
                    .get_or_init(|| resolve(kind))
                    .as_deref()?,
            );
            template = rest;
        } else {
            literal.push('{');
            template = marker;
        }
    }
    literal.push_str(template);
    output.push(Token::new(Kind::Protected, literal));
    Some(output)
}

fn text(tokens: &[Token]) -> String {
    tokens.iter().map(|t| t.text.as_str()).collect()
}

fn trimmed(tokens: &[Token]) -> Range<usize> {
    let start = tokens.iter().take_while(|t| whitespace(t)).count();
    let end = tokens
        .iter()
        .rposition(|t| !whitespace(t))
        .map_or(start, |i| i + 1);
    start..end
}

fn sentences(tokens: &[Token]) -> Vec<Range<usize>> {
    let mut at = 0;
    let mut ranges = Vec::new();
    while let Some(gap) = tokens[at..].iter().position(|t| !whitespace(t)) {
        let start = at + gap;
        at = start + sentence_length(&tokens[start..]);
        let end = start + trimmed(&tokens[start..at]).end;
        ranges.push(start..end);
    }
    ranges
}

fn original_matches<'a>(
    original: &str,
    finished: &[Token],
    ranges: &[Range<usize>],
    triggers: &Triggers<'a>,
    cleanup: impl Fn(&str) -> String,
) -> Vec<Match<'a>> {
    let mut raw = tokenize(original, &[]);
    protect_address_marks(&mut raw);
    let spoken = sentences(&raw);
    if spoken.len() != ranges.len() {
        return Vec::new();
    }
    let spoken: Vec<_> = spoken.into_iter().map(|range| text(&raw[range])).collect();
    let matches: Vec<_> = spoken
        .iter()
        .map(|utterance| triggers.find(utterance))
        .collect();
    // Commands, replacements and corrections can merge or split sentences. Bind
    // a spoken trigger to its position only if every resulting sentence agrees.
    if matches.iter().any(|m| !matches!(m, Match::Missing))
        && spoken.iter().zip(ranges).all(|(utterance, range)| {
            let result = text(&finished[range.clone()]);
            *utterance == result || cleanup(utterance).trim() == result
        })
    {
        matches
    } else {
        Vec::new()
    }
}

pub(super) fn apply(
    tokens: &mut Vec<Token>,
    snippets: &[Snippet],
    original: &str,
    cleanup: impl Fn(&str) -> String,
    resolve: &mut impl FnMut(SnippetValue) -> Option<String>,
) {
    let triggers = Triggers::new(snippets);
    let cache = Default::default();
    protect_address_marks(tokens);
    let whole = triggers
        .find(original)
        .or_else(|| triggers.find(&text(tokens)));
    let edits = match whole {
        Match::Ambiguous => return,
        Match::Unique(template) => vec![(trimmed(tokens), template)],
        Match::Missing => {
            let ranges = sentences(tokens);
            if ranges.len() <= 1 {
                return;
            }
            let spoken = original_matches(original, tokens, &ranges, &triggers, cleanup);
            ranges
                .into_iter()
                .enumerate()
                .filter_map(|(i, range)| {
                    match spoken
                        .get(i)
                        .copied()
                        .unwrap_or_default()
                        .or_else(|| triggers.find(&text(&tokens[range.clone()])))
                    {
                        Match::Unique(template) => Some((range, template)),
                        _ => None,
                    }
                })
                .collect()
        }
    };
    if edits.is_empty() {
        return;
    }
    let mut input = std::mem::take(tokens).into_iter();
    let mut at = 0;
    for (range, template) in edits {
        if let Some(expanded) = expand(template, &cache, resolve) {
            tokens.extend(input.by_ref().take(range.start - at));
            input.by_ref().take(range.len()).for_each(drop);
            tokens.extend(expanded);
            at = range.end;
        }
    }
    tokens.extend(input);
}
