use crate::tokens::{Kind, Token, word_key};

pub(super) fn apply(tokens: &mut Vec<Token>) {
    for index in 0..tokens.len() {
        if tokens[index].kind != Kind::Word || !at_sentence_start(&tokens[..index]) {
            continue;
        }
        let word = &tokens[index].text;
        let mut letters = word.chars();
        let first = letters.next().expect("word tokens are nonempty");
        if first.is_lowercase()
            && !letters.any(char::is_uppercase)
            && !starts_identifier(&tokens[index..])
        {
            tokens[index]
                .text
                .replace_range(..first.len_utf8(), &first.to_uppercase().to_string());
        }
    }
    if let Some(index) = final_period_position(tokens) {
        tokens.insert(index, Token::new(Kind::Punctuation, "."));
    }
}

fn whitespace(token: &Token) -> bool {
    matches!(token.kind, Kind::Space | Kind::LineBreak)
}

fn opening(token: &Token) -> bool {
    matches!(
        token.text.as_str(),
        "„" | "‚" | "»" | "›" | "\"" | "(" | "[" | "{"
    )
}

fn at_sentence_start(before: &[Token]) -> bool {
    let mut prefix = before;
    let mut newlines = 0;
    let mut opened = false;
    while let Some((last, rest)) = prefix.split_last() {
        if whitespace(last) {
            newlines += usize::from(last.kind == Kind::LineBreak);
        } else if opening(last) {
            opened = true;
        } else {
            break;
        }
        prefix = rest;
    }
    if newlines >= 2 {
        return true;
    }
    // Two separate quoted sentences may meet, as in: „Schon da.“ „Ja.“
    if opened
        && prefix
            .last()
            .is_some_and(|token| matches!(token.text.as_str(), "“" | "‘" | "«" | "‹"))
    {
        prefix = &prefix[..prefix.len() - 1];
    }
    let Some((mark, previous)) = prefix.split_last() else {
        return true;
    };
    if prefix.len() == before.len()
        || !matches!(mark.kind, Kind::Punctuation | Kind::CommandPunctuation)
    {
        return false;
    }
    match mark.text.as_str() {
        "!" | "?" => true,
        "." if mark.kind == Kind::CommandPunctuation => true,
        "." => previous.last().is_some_and(|word| {
            // Ordinal numbers, initials, abbreviations and ellipses can occur mid-sentence.
            let word_ending = word.kind == Kind::Word
                || (word.kind == Kind::Protected && word.text.ends_with(char::is_alphabetic));
            word.text.ends_with(['%', '€'])
                || (word_ending && word.text.chars().count() > 1 && !abbreviation(&word.text))
        }),
        _ => false,
    }
}

fn abbreviation(word: &str) -> bool {
    matches!(
        word_key(word).as_str(),
        "abb"
            | "abs"
            | "art"
            | "bspw"
            | "bzw"
            | "ca"
            | "dipl"
            | "dr"
            | "etc"
            | "evtl"
            | "gem"
            | "ggf"
            | "inkl"
            | "insb"
            | "jh"
            | "lt"
            | "max"
            | "min"
            | "mio"
            | "mrd"
            | "nr"
            | "phil"
            | "prof"
            | "sog"
            | "usw"
            | "vgl"
            | "zzgl"
    )
}

fn starts_identifier(rest: &[Token]) -> bool {
    // Preserve the spelling of an address, domain, path or URI at sentence start.
    rest.iter()
        .take_while(|token| !whitespace(token))
        .any(|token| token.text == "@")
        || matches!(rest.get(1..3), Some([separator, next]) if
            (separator.text == "." && matches!(next.kind, Kind::Word | Kind::Protected))
            || (separator.text == ":" && next.text == "/"))
}

fn final_period_position(tokens: &[Token]) -> Option<usize> {
    let last = tokens.iter().rposition(|token| !whitespace(token))?;
    let mut content = last;
    while matches!(
        tokens[content].text.as_str(),
        "“" | "‘" | "«" | "‹" | "\"" | ")" | "]" | "}"
    ) {
        content = content.checked_sub(1)?;
    }
    let tail = &tokens[content];
    if !matches!(tail.kind, Kind::Word | Kind::Protected)
        && !matches!(tail.text.as_str(), "%" | "€")
    {
        return None;
    }
    if tail.text.ends_with(['.', '!', '?', '…', ',', ':', ';'])
        || !tokens[..=content]
            .iter()
            .any(|t| matches!(t.kind, Kind::Word | Kind::Protected))
    {
        return None;
    }
    // A whole utterance in quotation marks takes its period inside the quote.
    // In ordinary prose a parenthesis or a quoted term belongs inside the sentence.
    let opening_quote = match tokens[last].text.as_str() {
        "“" => "„",
        "‘" => "‚",
        "«" => "»",
        "‹" => "›",
        "\"" => "\"",
        _ => return Some(last + 1),
    };
    let quoted = tokens[..last]
        .iter()
        .rposition(|token| token.text == opening_quote)
        .is_some_and(|index| at_sentence_start(&tokens[..=index]));
    Some(if quoted { last } else { last + 1 })
}
