use std::collections::HashMap;

use icu_normalizer::ComposingNormalizerBorrowed;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Word,
    Space,
    LineBreak,
    Punctuation,
    // An explicit command takes precedence over inferred filler punctuation.
    CommandPunctuation,
    Protected,
    // A correction with an ambiguous scope is kept verbatim for review.
    Uncertain,
    // A zero-width marker between literal pieces of an expanded snippet.
    Cursor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Token {
    pub kind: Kind,
    pub text: String,
}

impl Token {
    pub fn new(kind: Kind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
        }
    }
}

// Inspect adjacent tokens from the word outward. These separators can belong
// to addresses, paths, code identifiers or a longer hyphenated compound.
pub(super) fn identifier_edge<'a>(mut neighbors: impl Iterator<Item = &'a Token>) -> bool {
    let Some(mark) = neighbors.next().filter(|t| t.kind == Kind::Punctuation) else {
        return false;
    };
    match mark.text.as_str() {
        "#" | "@" | "/" | "\\" | "`" | "=" | "<" | ">" | "-" | "‐" | "‑" => true,
        "." | "&" | "?" => neighbors
            .next()
            .is_some_and(|t| matches!(t.kind, Kind::Word | Kind::Protected)),
        ":" => neighbors.next().is_some_and(|t| t.text == ":"),
        "(" => neighbors.next().is_some_and(|t| t.text == ")"),
        _ => false,
    }
}

pub(super) fn is_address_field(field: &[Token]) -> bool {
    field.iter().any(|t| t.text.contains(['@', '/', '\\']))
        || field.windows(3).any(|t| {
            t[1].text == "."
                && matches!(t[0].kind, Kind::Word | Kind::Protected)
                && matches!(t[2].kind, Kind::Word | Kind::Protected)
        })
}

/// Mark the address body while leaving spaces and surrounding prose punctuation outside it.
pub(super) fn address_parts(tokens: &[Token]) -> Vec<bool> {
    let mut marked = vec![false; tokens.len()];
    let mut start = 0;
    for field in tokens.split_inclusive(|t| matches!(t.kind, Kind::Space | Kind::LineBreak)) {
        if is_address_field(field) {
            let prefix = field
                .iter()
                .take_while(|t| {
                    matches!(
                        t.text.as_str(),
                        "„" | "‚" | "»" | "›" | "\"" | "(" | "[" | "{"
                    )
                })
                .count();
            let suffix = field[prefix..]
                .iter()
                .rev()
                .take_while(|t| {
                    matches!(t.kind, Kind::Space | Kind::LineBreak)
                        || matches!(
                            t.text.as_str(),
                            "." | ","
                                | ":"
                                | ";"
                                | "!"
                                | "?"
                                | "…"
                                | "“"
                                | "‘"
                                | "«"
                                | "‹"
                                | "\""
                                | ")"
                                | "]"
                                | "}"
                        )
                })
                .count();
            marked[start + prefix..start + field.len() - suffix].fill(true);
        }
        start += field.len();
    }
    marked
}

// Keys are only used for comparison. Output always uses the original spelling.
pub(super) fn word_key(text: &str) -> String {
    ComposingNormalizerBorrowed::new_nfc()
        .normalize(text)
        .to_lowercase()
}

fn classify(ch: char) -> Kind {
    if matches!(ch, '\r' | '\n') {
        Kind::LineBreak
    } else if ch.is_whitespace() {
        Kind::Space
    } else if ch.is_alphanumeric() || unicode_ident::is_xid_continue(ch) {
        Kind::Word
    } else {
        Kind::Punctuation
    }
}

fn split(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = input.char_indices().peekable();
    while let Some((start, ch)) = chars.next() {
        let kind = classify(ch);
        match kind {
            Kind::LineBreak if ch == '\r' => {
                chars.next_if(|&(_, next)| next == '\n');
            }
            Kind::Word | Kind::Space => {
                while chars.next_if(|&(_, next)| classify(next) == kind).is_some() {}
            }
            _ => {}
        }
        let end = chars.peek().map_or(input.len(), |&(index, _)| index);
        tokens.push(Token::new(kind, &input[start..end]));
    }
    tokens
}

pub(super) fn dictionary_key(token: &Token) -> String {
    match token.kind {
        Kind::Word => word_key(&token.text),
        Kind::Space => " ".to_owned(),
        Kind::Protected => word_key(&token.text.split_whitespace().collect::<Vec<_>>().join(" ")),
        _ => token.text.clone(),
    }
}

fn mark_names<'a>(tokens: &[Token], names: impl Iterator<Item = &'a str>, protected: &mut [bool]) {
    let mut names = names.peekable();
    if names.peek().is_none() {
        return;
    }
    let keys: Vec<_> = tokens.iter().map(dictionary_key).collect();
    let mut positions = HashMap::<&str, Vec<usize>>::new();
    for (index, key) in keys.iter().enumerate() {
        positions.entry(key).or_default().push(index);
    }
    for name in names {
        let pattern: Vec<_> = split(name.trim()).iter().map(dictionary_key).collect();
        let Some(candidates) = pattern
            .first()
            .and_then(|first| positions.get(first.as_str()))
        else {
            continue;
        };
        for &start in candidates {
            let end = start + pattern.len();
            if keys.get(start..end) == Some(pattern.as_slice()) {
                protected[start..end].fill(true);
            }
        }
    }
}

fn starts_exponent(token: &Token) -> bool {
    token
        .text
        .strip_suffix(['e', 'E'])
        .is_some_and(|digits| !digits.is_empty() && digits.chars().all(char::is_numeric))
}

fn is_number_separator(separator: &Token, following: &Token) -> bool {
    match separator.text.as_str() {
        "." | "," | ":" | "/" | "-" | "'" | "’" | "٫" | "٬" => true,
        " " | "\u{a0}" | "\u{202f}" => following.text.chars().count() == 3,
        _ => false,
    }
}

fn mark_numbers(tokens: &[Token], protected: &mut [bool]) {
    for (index, token) in tokens.iter().enumerate() {
        if token.kind != Kind::Word || !token.text.chars().any(char::is_numeric) {
            continue;
        }
        protected[index] = true;
        if index == 0 || !token.text.starts_with(char::is_numeric) {
            continue;
        }
        let previous = &tokens[index - 1];
        let before_separator = tokens[..index - 1].last();
        let internal_separator = before_separator
            .is_some_and(|left| left.text.ends_with(char::is_numeric))
            && is_number_separator(previous, token);
        // A decimal fraction may omit its leading zero. Its separator must
        // touch the digits, unlike a pause comma followed by whitespace.
        let leading_decimal = matches!(previous.text.as_str(), "," | "." | "٫")
            && before_separator.is_none_or(|left| left.kind != Kind::Word);
        let mut start = index;
        if internal_separator || leading_decimal {
            start -= 1;
        }
        if let Some((sign, before)) = tokens[..start].split_last()
            && matches!(sign.text.as_str(), "+" | "-" | "−")
            && before
                .last()
                .is_none_or(|left| left.kind != Kind::Word || starts_exponent(left))
        {
            start -= 1;
        }
        protected[start..index].fill(true);
    }
}

pub(super) fn tokenize(input: &str, proper_names: &[String]) -> Vec<Token> {
    tokenize_with_terms(input, proper_names, &[], false)
}

/// Keep the spelling of each numeric literal, including signs, separators and percentages.
pub(super) fn numeric_literals(text: &str) -> Vec<String> {
    if !text.contains(char::is_numeric) {
        return Vec::new();
    }
    let tokens = tokenize(text, &[]);
    let mut numbers = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if token.kind == Kind::Protected
            && !token.text.starts_with(char::is_alphabetic)
            && token.text.contains(char::is_numeric)
        {
            let mut number = token.text.clone();
            if let Some(unit) = tokens[index + 1..].iter().find(|t| t.kind != Kind::Space)
                && matches!(unit.text.as_str(), "%" | "‰" | "‱")
            {
                number.push_str(&unit.text);
            }
            numbers.push(number);
        }
    }
    numbers
}

pub(super) fn tokenize_with_terms(
    input: &str,
    proper_names: &[String],
    terms: &[crate::DictionaryEntry],
    keep_term_boundaries: bool,
) -> Vec<Token> {
    let mut tokens = split(input);
    let mut protected = vec![false; tokens.len()];
    mark_names(
        &tokens,
        proper_names.iter().map(String::as_str),
        &mut protected,
    );
    mark_numbers(&tokens, &mut protected);
    if !terms.is_empty() {
        let names = terms.iter().map(|entry| entry.term.as_str());
        if keep_term_boundaries {
            // Commands run first. Shield registered words while keeping their
            // boundaries available for a longer spoken variant in the next stage.
            let mut registered = vec![false; tokens.len()];
            mark_names(&tokens, names, &mut registered);
            for (token, named) in tokens.iter_mut().zip(registered) {
                if named && token.kind == Kind::Word {
                    token.kind = Kind::Protected;
                }
            }
        } else {
            mark_names(&tokens, names, &mut protected);
        }
    }
    if !protected.contains(&true) {
        return tokens;
    }

    let mut output: Vec<Token> = Vec::with_capacity(tokens.len());
    for (mut token, protect) in tokens.into_iter().zip(protected) {
        if protect {
            if let Some(last) = output
                .last_mut()
                .filter(|last| last.kind == Kind::Protected)
            {
                last.text.push_str(&token.text);
                continue;
            }
            token.kind = Kind::Protected;
        }
        output.push(token);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_forms_are_indivisible_for_all_rules() {
        for number in [
            "002",
            ",05",
            ".025",
            "−,050",
            "+.25E-03",
            "٫٥",
            "−005,004",
            "+49",
            "1.022.005,30",
            "24\u{202f}000",
            "24 000",
            "23:04:07",
            "09.03.2026",
            "2026-03-09",
            "3/4",
            "١٢٣",
            "12,50kg",
            "-1,25e-03",
        ] {
            let input = format!("({number}),");
            let tokens = tokenize(&input, &[]);
            assert_eq!(tokens.len(), 4);
            assert_eq!(tokens[1], Token::new(Kind::Protected, number));
        }
    }

    #[test]
    fn original_token_boundaries_survive_opaque_spans() {
        let text = "Äh,  Nord\r\n\t18,7   €";
        let tokens = tokenize(text, &[String::from("Äh, Nord")]);
        assert_eq!(
            tokens,
            [
                Token::new(Kind::Protected, "Äh,  Nord"),
                Token::new(Kind::LineBreak, "\r\n"),
                Token::new(Kind::Space, "\t"),
                Token::new(Kind::Protected, "18,7"),
                Token::new(Kind::Space, "   "),
                Token::new(Kind::Punctuation, "€"),
            ]
        );
    }
}
