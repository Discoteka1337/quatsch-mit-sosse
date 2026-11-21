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

fn dictionary_key(token: &Token) -> String {
    match token.kind {
        Kind::Word => word_key(&token.text),
        Kind::Space => " ".to_owned(),
        _ => token.text.clone(),
    }
}

fn mark_names(tokens: &[Token], names: &[String], protected: &mut [bool]) {
    if names.is_empty() {
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
        let sign = matches!(previous.text.as_str(), "+" | "-" | "−")
            && before_separator.is_none_or(|left| left.kind != Kind::Word || starts_exponent(left));
        let internal_separator = before_separator
            .is_some_and(|left| left.text.ends_with(char::is_numeric))
            && is_number_separator(previous, token);
        if sign || internal_separator {
            protected[index - 1] = true;
        }
    }
}

pub(super) fn tokenize(input: &str, proper_names: &[String]) -> Vec<Token> {
    let tokens = split(input);
    let mut protected = vec![false; tokens.len()];
    mark_names(&tokens, proper_names, &mut protected);
    mark_numbers(&tokens, &mut protected);
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
