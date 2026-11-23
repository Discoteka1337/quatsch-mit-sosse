use crate::FillerStrength;
use crate::tokens::{Kind, Token, word_key};

pub(super) fn apply(tokens: &mut Vec<Token>, strength: FillerStrength, self_corrections: bool) {
    let mut cursor = 0;
    while cursor < tokens.len() {
        if !is_filler(tokens, cursor, strength, self_corrections) {
            cursor += 1;
            continue;
        }

        let mut start = cursor;
        while start > 0 && is_separator(&tokens[start - 1]) {
            start -= 1;
        }
        let mut end = cursor + 1;
        while end < tokens.len()
            && (is_separator(&tokens[end]) || is_filler(tokens, end, strength, self_corrections))
        {
            end += 1;
        }
        remove_run(tokens, start, end);
        cursor = end;
    }
    tokens.retain(|token| !token.text.is_empty());
}

fn is_filler(
    tokens: &[Token],
    index: usize,
    strength: FillerStrength,
    self_corrections: bool,
) -> bool {
    if tokens[index].kind != Kind::Word {
        return false;
    }
    let word = word_key(&tokens[index].text);
    let candidate = match word.as_str() {
        "äh" => {
            !self_corrections
                || crate::corrections::hesitation(&tokens[..index], &tokens[index + 1..])
                    == crate::corrections::Hesitation::Ordinary
        }
        "ähm" | "öhm" | "hm" | "ehm" => true,
        "also" | "halt" | "quasi" => strength == FillerStrength::Strong,
        _ => false,
    };
    candidate
        && !joins_words(tokens[..index].iter().rev())
        && !joins_words(tokens[index + 1..].iter())
}

fn joins_words<'a>(mut neighbors: impl Iterator<Item = &'a Token>) -> bool {
    neighbors.next().is_some_and(|token| {
        token.kind == Kind::Punctuation
            && matches!(token.text.as_str(), "." | "-" | "‐" | "‑" | "'" | "’")
    }) && neighbors
        .next()
        .is_some_and(|token| matches!(token.kind, Kind::Word | Kind::Protected))
}

fn is_comma(token: &Token) -> bool {
    token.kind == Kind::Punctuation && token.text == ","
}

fn is_separator(token: &Token) -> bool {
    token.kind == Kind::Space || is_comma(token)
}

fn remove_run(tokens: &mut [Token], start: usize, end: usize) {
    let left = tokens[..start].last();
    let right = tokens.get(end);
    let at_line_start = left.is_none_or(|token| token.kind == Kind::LineBreak);
    let at_line_end = right.is_none_or(|token| token.kind == Kind::LineBreak);
    let keep_indent = at_line_start && !at_line_end && tokens[start].kind == Kind::Space;
    let add_space = !at_line_start
        && !at_line_end
        && !left.is_some_and(is_opening_delimiter)
        && !right.is_some_and(attaches_to_left);
    let keep_comma = add_space
        && left.is_some_and(|token| {
            matches!(token.kind, Kind::Word | Kind::Protected) || is_closing_delimiter(&token.text)
        })
        && tokens[start..end].iter().any(is_comma)
        && (right.is_some_and(introduces_clause)
            || (left.is_some_and(is_number) && right.is_some_and(is_number))
            || ends_subordinate_clause(&tokens[..start], &tokens[end..]));

    // Compact once after all runs. Removing each run separately would shift the
    // entire transcript repeatedly when many hesitations occur.
    let first = start + usize::from(keep_indent);
    for token in &mut tokens[first..end] {
        token.text.clear();
    }
    if keep_comma {
        tokens[start] = Token::new(Kind::Punctuation, ",");
    }
    if add_space {
        tokens[start + usize::from(keep_comma)] = Token::new(Kind::Space, " ");
    }
}

fn is_opening_delimiter(token: &Token) -> bool {
    matches!(token.kind, Kind::Punctuation | Kind::CommandPunctuation)
        && matches!(token.text.as_str(), "(" | "[" | "{" | "„" | "‚" | "»" | "›")
}

fn attaches_to_left(token: &Token) -> bool {
    matches!(token.kind, Kind::Punctuation | Kind::CommandPunctuation)
        && (matches!(token.text.as_str(), "," | "." | "!" | "?" | ":" | ";" | "…")
            || is_closing_delimiter(&token.text))
}

fn is_closing_delimiter(text: &str) -> bool {
    matches!(text, ")" | "]" | "}" | "“" | "‘" | "«" | "‹")
}

fn is_number(token: &Token) -> bool {
    if token.kind != Kind::Protected {
        return false;
    }
    let mut mantissa = token.text.as_str();
    if let Some((base, exponent)) = mantissa.split_once(['e', 'E']) {
        let digits = exponent.strip_prefix(['+', '-', '−']).unwrap_or(exponent);
        if digits.is_empty() || !digits.chars().all(char::is_numeric) {
            return false;
        }
        mantissa = base;
    }
    mantissa.chars().any(char::is_numeric) && !mantissa.chars().any(char::is_alphabetic)
}

fn ends_subordinate_clause(left: &[Token], right: &[Token]) -> bool {
    let main_clause = match right {
        [adverb, space, rest @ ..]
            if adverb.kind == Kind::Word
                && adverb.text.eq_ignore_ascii_case("dann")
                && space.kind == Kind::Space =>
        {
            rest
        }
        _ => right,
    };
    let [verb, space, subject, ..] = main_clause else {
        return false;
    };
    if space.kind != Kind::Space || !verb_agrees_with_pronoun(verb, subject) {
        return false;
    }

    // Look back only within the current clause. Cleared tokens from earlier
    // filler runs are still in the buffer until the final compaction.
    let words = left
        .iter()
        .rev()
        .filter(|token| !token.text.is_empty())
        .take_while(|token| matches!(token.kind, Kind::Space | Kind::Word | Kind::Protected))
        .filter(|token| token.kind != Kind::Space);
    // A subordinator alone, or a subordinator followed only by its subject,
    // does not constitute a finished subordinate clause.
    words
        .skip(2)
        .last()
        .is_some_and(|first| first.kind == Kind::Word && is_subordinator(&word_key(&first.text)))
}

pub(super) fn verb_agrees_with_pronoun(verb: &Token, subject: &Token) -> bool {
    if verb.kind != Kind::Word || subject.kind != Kind::Word {
        return false;
    }
    let verb = word_key(&verb.text);
    let plural = verb.ends_with("en") || verb == "sind";
    let third_singular = verb.ends_with('t') || verb == "wird";
    match word_key(&subject.text).as_str() {
        "wir" => plural,
        "ich" => verb.ends_with('e') || verb == "bin",
        "du" => verb.ends_with("st"),
        "er" | "es" => third_singular,
        "sie" => third_singular || plural,
        // "ihr" can also be the dative object of a preposition ending in -t.
        "ihr" => {
            verb == "seid"
                || (verb.ends_with('t')
                    && !matches!(
                        verb.as_str(),
                        "mit" | "seit" | "samt" | "mitsamt" | "nebst" | "laut"
                    ))
        }
        _ => false,
    }
}

fn introduces_clause(token: &Token) -> bool {
    if token.kind != Kind::Word {
        return false;
    }
    let word = word_key(&token.text);
    is_subordinator(&word) || matches!(word.as_str(), "aber" | "sondern" | "denn")
}

fn is_subordinator(word: &str) -> bool {
    matches!(
        word,
        "dass"
            | "ob"
            | "weil"
            | "obwohl"
            | "wenn"
            | "falls"
            | "bevor"
            | "nachdem"
            | "sobald"
            | "sodass"
            | "sofern"
    )
}
