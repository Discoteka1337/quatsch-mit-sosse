mod cardinal;
mod date;

use crate::tokens::{Kind, Token, word_key};
pub(crate) use cardinal::cardinal;
pub(crate) use date::named_month;

pub(super) fn apply(tokens: &mut Vec<Token>, digits_from: u32) {
    let mut input = std::mem::take(tokens).into_iter();
    while !input.as_slice().is_empty() {
        let (count, replacement) = rewrite(input.as_slice(), tokens, digits_from);
        let consumed = input.by_ref().take(count);
        if let Some(text) = replacement {
            consumed.for_each(drop);
            tokens.push(Token::new(Kind::Protected, text));
        } else {
            tokens.extend(consumed);
        }
    }
}

// Each match consumes whole tokens, including only spaces *inside* its expression.
type Match = (usize, Option<String>);

fn rewrite(input: &[Token], before: &[Token], digits_from: u32) -> Match {
    let Some(first) = word_at(input, 0) else {
        return (1, None);
    };
    if before.last().is_some_and(is_joiner) {
        return (1, None);
    }
    let next = following_word(input, 1);
    if first == "halb" && next.as_deref().and_then(cardinal).is_some_and(|n| n <= 12) {
        return (3, None);
    }
    if let Some((end, date)) = next
        .as_deref()
        .and_then(|month| date::date(&first, month, input))
    {
        return (end, Some(date));
    }
    let Some((value, end)) = magnitude(&first, input) else {
        return (1, None);
    };
    if end == 1 && first == "eine" {
        return (1, None);
    }
    match following_word(input, end).as_deref() {
        Some("komma") => {
            if let Some(result) = decimal(value, input, end + 2) {
                return result;
            }
        }
        Some("uhr") => return clock(value, input, end + 2),
        Some("euro") => return money(value, input, end + 2),
        Some("prozent") => return (end + 2, Some(format!("{value} %"))),
        _ => {}
    }
    let article = end == 1 && first == "ein";
    (
        end,
        (value >= u64::from(digits_from) && !article).then(|| value.to_string()),
    )
}

fn is_joiner(token: &Token) -> bool {
    token.kind == Kind::Punctuation && matches!(token.text.as_str(), "-" | "‐" | "‑" | "'" | "’")
}

fn word_at(input: &[Token], index: usize) -> Option<String> {
    let token = input.get(index)?;
    (token.kind == Kind::Word && !input.get(index + 1).is_some_and(is_joiner))
        .then(|| word_key(&token.text))
}

fn following_word(input: &[Token], end: usize) -> Option<String> {
    if input.get(end)?.kind != Kind::Space {
        return None;
    }
    word_at(input, end + 1)
}

fn magnitude(first: &str, input: &[Token]) -> Option<(u64, usize)> {
    let mut group = match first {
        "ein" | "eine" => 1,
        _ => u64::from(cardinal(first)?),
    };
    let mut total = 0;
    let mut end = 1;
    let mut last_scale = u64::MAX;
    while let Some(unit) = following_word(input, end) {
        let scale = match (unit.as_str(), group) {
            ("million", 1) | ("millionen", 2..) => 1_000_000,
            ("milliarde", 1) | ("milliarden", 2..) => 1_000_000_000,
            _ => break,
        };
        if scale >= last_scale {
            // The last parsed group belongs to a new amount ("zwei Millionen
            // drei Milliarden"). Leave its space and numeral in the input.
            return Some((total, end - 2));
        }
        total += group * scale;
        end += 2;
        let Some(remainder) = following_word(input, end).as_deref().and_then(cardinal) else {
            return Some((total, end));
        };
        group = u64::from(remainder);
        last_scale = scale;
        end += 2;
    }
    Some((total + group, end))
}

fn decimal(whole: u64, input: &[Token], mut end: usize) -> Option<Match> {
    let first = following_word(input, end)?;
    let mut digits = cardinal(&first)?.to_string();
    end += 2;
    if digits.len() == 1 {
        while let Some(digit) = following_word(input, end)
            .as_deref()
            .and_then(cardinal)
            .and_then(|n| char::from_digit(n, 10))
        {
            digits.push(digit);
            end += 2;
        }
    }
    let unit = match following_word(input, end).as_deref() {
        Some("prozent") => " %",
        Some("euro") => " €",
        _ => "",
    };
    Some((
        end + if unit.is_empty() { 0 } else { 2 },
        Some(format!("{whole},{digits}{unit}")),
    ))
}

fn clock(hour: u64, input: &[Token], end: usize) -> Match {
    let Some(mut minute) = following_word(input, end).as_deref().and_then(cardinal) else {
        return (end, (hour <= 24).then(|| format!("{hour} Uhr")));
    };
    let mut minute_end = end + 2;
    if minute == 0
        && let Some(digit) = following_word(input, minute_end)
            .as_deref()
            .and_then(cardinal)
            .filter(|n| *n < 10)
    {
        minute = digit;
        minute_end += 2;
    }
    if hour > 24 || minute > 59 || (hour == 24 && minute != 0) {
        return (minute_end, None);
    }
    if starts_counted_phrase(input, minute_end) {
        return (end, Some(format!("{hour} Uhr")));
    }
    (minute_end, Some(format!("{hour:02}:{minute:02} Uhr")))
}

fn money(euros: u64, input: &[Token], end: usize) -> Match {
    let with_and = following_word(input, end).as_deref() == Some("und");
    let cent_start = end + if with_and { 2 } else { 0 };
    if let Some(word) = following_word(input, cent_start) {
        let cent_end = cent_start + 2;
        let unit = following_word(input, cent_end);
        let has_unit = unit.as_deref() == Some("cent");
        let cents = match (word.as_str(), has_unit) {
            ("ein", true) => Some(1),
            _ => cardinal(&word).filter(|n| *n < 100),
        };
        let other_unit = matches!(unit.as_deref(), Some("euro" | "uhr" | "prozent"));
        if let Some(cents) = cents
            && (has_unit || (!with_and && !other_unit && !starts_counted_phrase(input, cent_end)))
        {
            return (
                cent_end + if has_unit { 2 } else { 0 },
                Some(format!("{euros},{cents:02} €")),
            );
        }
    }
    (end, Some(format!("{euros} €")))
}

// In "um acht Uhr fünf neue Gäste" the number belongs to the noun phrase.
fn starts_counted_phrase(input: &[Token], end: usize) -> bool {
    for [space, word] in input[end..].as_chunks::<2>().0 {
        if space.kind != Kind::Space || !matches!(word.kind, Kind::Word | Kind::Protected) {
            break;
        }
        if word.text.starts_with(char::is_uppercase) {
            return true;
        }
        let key = word_key(&word.text);
        if word.kind != Kind::Word
            || matches!(
                key.as_str(),
                "je" | "ohne"
                    | "gegen"
                    | "wegen"
                    | "hinter"
                    | "unter"
                    | "über"
                    | "gegenüber"
                    | "neben"
                    | "oder"
                    | "heute"
                    | "morgen"
            )
            || !["e", "en", "er", "es", "em"]
                .iter()
                .any(|suffix| key.ends_with(suffix))
        {
            break;
        }
    }
    false
}
