use super::{Kind, Token, cardinal, following_word, is_joiner};

fn ordinal_day(ordinal: &str) -> Option<u32> {
    let stem = ["en", "em", "er", "es", "e"]
        .into_iter()
        .find_map(|ending| ordinal.strip_suffix(ending))?;
    Some(match stem {
        "erst" => 1,
        "dritt" => 3,
        "siebt" | "siebent" => 7,
        "acht" => 8,
        _ => stem
            .strip_suffix("st")
            .and_then(cardinal)
            .filter(|n| (20..=31).contains(n))
            .or_else(|| cardinal(stem.strip_suffix('t')?).filter(|n| (2..=19).contains(n)))?,
    })
}

pub(crate) fn named_month(month: &str) -> Option<(&'static str, u32)> {
    Some(match month {
        "januar" => ("Januar", 31),
        "februar" => ("Februar", 29),
        "märz" => ("März", 31),
        "april" => ("April", 30),
        "mai" => ("Mai", 31),
        "juni" => ("Juni", 30),
        "juli" => ("Juli", 31),
        "august" => ("August", 31),
        "september" => ("September", 30),
        "oktober" => ("Oktober", 31),
        "november" => ("November", 30),
        "dezember" => ("Dezember", 31),
        _ => return None,
    })
}

pub(super) fn date(ordinal: &str, month: &str, input: &[Token]) -> Option<(usize, String)> {
    let day = ordinal_day(ordinal)?;
    if let Some((name, last_day)) = named_month(month) {
        return (day <= last_day).then(|| (3, format!("{day}. {name}")));
    }
    let month = ordinal_day(month)?;
    let (end, year) = full_year(input)?;
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            28 + u32::from(
                year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)),
            )
        }
        _ => return None,
    };
    (day <= days).then(|| (end, format!("{day:02}.{month:02}.{year}")))
}

fn full_year(input: &[Token]) -> Option<(usize, u32)> {
    let [space, first, ..] = input.get(3..)? else {
        return None;
    };
    if space.kind != Kind::Space || input.get(5).is_some_and(is_joiner) {
        return None;
    }
    if first.kind == Kind::Protected {
        let year = first.text.parse::<u32>().ok()?;
        return (first.text.len() == 4 && (1000..=9999).contains(&year)).then_some((5, year));
    }

    // The recognizer may split a single German numeral at constituent boundaries.
    // Seven words suffice even for "neun tausend neun hundert neun und neunzig".
    let mut joined = String::new();
    let mut candidate = None;
    for end in (3..=15).step_by(2) {
        let Some(word) = following_word(input, end) else {
            break;
        };
        if !matches!(word.as_str(), "ein" | "und") && cardinal(&word).is_none() {
            break;
        }
        joined.push_str(&word);
        if let Some(year) = cardinal(&joined).filter(|year| (1000..=9999).contains(year)) {
            candidate = Some((end + 2, year));
        }
    }
    candidate
}
