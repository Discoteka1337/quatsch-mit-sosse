mod triggers;

use crate::{
    numbers::named_month,
    punctuation::abbreviation,
    tokens::{Kind, Token, word_key},
};
use std::cell::OnceCell;
use triggers::Triggers;
pub use triggers::snippet_trigger_key;

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
        let address = field
            .iter()
            .any(|t| matches!(t.text.as_str(), "@" | "/" | "\\"))
            || field.windows(3).any(|t| {
                t[1].text == "."
                    && matches!(t[0].kind, Kind::Word | Kind::Protected)
                    && matches!(t[2].kind, Kind::Word | Kind::Protected)
            });
        if address && let Some(last) = field.iter().rposition(|t| !sentence_mark(t) && !closing(t))
        {
            for token in &mut field[..last] {
                if token.kind == Kind::Punctuation && sentence_mark(token) {
                    token.kind = Kind::Protected;
                }
            }
        }
    }
}

fn internal_period(before: &Token, after: &[Token]) -> bool {
    abbreviation(&before.text)
        || (before.kind == Kind::Word && before.text.chars().count() == 1)
        || (before.text.ends_with(char::is_numeric)
            && after
                .iter()
                .find(|t| t.kind != Kind::Space)
                .is_some_and(|t| named_month(&word_key(&t.text)).is_some()))
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
            && at > 0
            && internal_period(&tokens[at - 1], &tokens[end..]);
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

fn lookup<'a>(tokens: &[Token], triggers: &Triggers<'a>) -> Option<&'a str> {
    triggers.find(
        &tokens
            .iter()
            .map(|token| token.text.as_str())
            .collect::<String>(),
    )
}

pub(super) fn apply(
    tokens: &mut Vec<Token>,
    snippets: &[Snippet],
    resolve: &mut impl FnMut(SnippetValue) -> Option<String>,
) {
    if snippets.is_empty() || tokens.is_empty() {
        return;
    }
    let triggers = Triggers::new(snippets);
    let cache = Default::default();
    protect_address_marks(tokens);
    let mut input = std::mem::take(tokens).into_iter();
    let mut whole = lookup(input.as_slice(), &triggers);
    while !input.as_slice().is_empty() {
        let leading = input
            .as_slice()
            .iter()
            .take_while(|t| whitespace(t))
            .count();
        tokens.extend(input.by_ref().take(leading));
        let rest = input.as_slice();
        if rest.is_empty() {
            break;
        }
        let length = if whole.is_some() {
            rest.len()
        } else {
            sentence_length(rest)
        };
        let content = rest[..length].iter().rposition(|t| !whitespace(t)).unwrap() + 1;
        if let Some(template) = whole.take().or_else(|| lookup(&rest[..content], &triggers))
            && let Some(expanded) = expand(template, &cache, resolve)
        {
            input.by_ref().take(content).for_each(drop);
            tokens.extend(expanded);
        } else {
            tokens.extend(input.by_ref().take(content));
        }
    }
}
