use super::{Alternative, Kind, Token, word_key};

#[derive(PartialEq)]
enum Class {
    Weekday,
    Month,
    RelativeDay,
    Color,
    Number,
    Direction,
}

fn class(key: &str) -> Option<Class> {
    Some(match key {
        "montag" | "dienstag" | "mittwoch" | "donnerstag" | "freitag" | "samstag" | "sonnabend"
        | "sonntag" => Class::Weekday,
        "januar" | "februar" | "märz" | "april" | "mai" | "juni" | "juli" | "august"
        | "september" | "oktober" | "november" | "dezember" => Class::Month,
        "vorgestern" | "gestern" | "heute" | "morgen" | "übermorgen" => Class::RelativeDay,
        "rot" | "blau" | "grün" | "gelb" | "schwarz" | "weiß" | "grau" | "braun" | "orange"
        | "lila" | "rosa" => Class::Color,
        "links" | "rechts" | "oben" | "unten" | "vorne" | "hinten" => Class::Direction,
        _ if crate::numbers::cardinal(key).is_some() => Class::Number,
        _ => return None,
    })
}

fn article(key: &str) -> bool {
    matches!(
        key,
        "der" | "die" | "das" | "den" | "dem" | "ein" | "eine" | "einen" | "einem" | "einer"
    )
}

fn preposition(key: &str) -> bool {
    matches!(
        key,
        "am" | "an"
            | "auf"
            | "aus"
            | "bei"
            | "beim"
            | "für"
            | "im"
            | "in"
            | "ins"
            | "mit"
            | "nach"
            | "um"
            | "von"
            | "vom"
            | "vor"
            | "zu"
            | "zum"
            | "zur"
    )
}

fn grammatical(key: &str) -> bool {
    matches!(
        key,
        "ich"
            | "du"
            | "er"
            | "sie"
            | "es"
            | "wir"
            | "ihr"
            | "man"
            | "sich"
            | "bin"
            | "bist"
            | "ist"
            | "sind"
            | "seid"
            | "war"
            | "waren"
            | "wäre"
            | "habe"
            | "hast"
            | "hat"
            | "haben"
            | "hatte"
            | "hatten"
            | "wird"
            | "werden"
            | "wurde"
            | "will"
            | "wollen"
            | "soll"
            | "sollen"
            | "muss"
            | "müssen"
            | "kann"
            | "können"
            | "möchte"
            | "möchten"
            | "nicht"
            | "noch"
            | "schon"
            | "sehr"
            | "dass"
            | "wenn"
            | "weil"
            | "ob"
            | "obwohl"
            | "damit"
            | "aber"
            | "und"
            | "oder"
            | "denn"
            | "sondern"
    )
}

pub(super) fn possible_left(t: &Token) -> bool {
    if t.kind == Kind::Protected {
        return true;
    }
    if t.kind != Kind::Word {
        return false;
    }
    let key = word_key(&t.text);
    !article(&key) && !preposition(&key) && !grammatical(&key)
}

pub(super) fn possible_right(t: &Token) -> bool {
    t.kind == Kind::Protected || (t.kind == Kind::Word && !grammatical(&word_key(&t.text)))
}

fn capitalized(word: &Token) -> bool {
    word.text.starts_with(char::is_uppercase)
}

fn nominal(head: &Token, rest: &[&Token]) -> bool {
    let key = word_key(&head.text);
    let content = if preposition(&key) {
        if rest.first().is_some_and(|t| article(&word_key(&t.text))) {
            &rest[1..]
        } else {
            rest
        }
    } else if article(&key) {
        rest
    } else {
        return false;
    };
    match content {
        [noun] => capitalized(noun) || class(&word_key(&noun.text)).is_some(),
        [prefix, name]
            if preposition(&key)
                && matches!(
                    prefix.text.as_str(),
                    "Bad" | "Sankt" | "New" | "San" | "Santa" | "Los"
                ) =>
        {
            capitalized(name)
        }
        [adjective, noun] => {
            !capitalized(adjective)
                && ["en", "em", "er", "es", "e"]
                    .iter()
                    .any(|ending| adjective.text.ends_with(ending))
                && capitalized(noun)
        }
        _ => false,
    }
}

// Only consider a short contiguous phrase. Punctuation, protected dictionary
// entries and uncertain spans are hard boundaries in either direction.
fn left_words(old: &[Token]) -> Vec<&Token> {
    let mut words: Vec<_> = old
        .iter()
        .rev()
        .take_while(|t| matches!(t.kind, Kind::Word | Kind::Space))
        .filter(|t| t.kind == Kind::Word)
        .take(4)
        .collect();
    words.reverse();
    words
}

fn starts_at(old: &[Token], count: usize) -> usize {
    old.iter()
        .enumerate()
        .rev()
        .filter(|(_, t)| t.kind == Kind::Word)
        .nth(count - 1)
        .expect("count came from the clause tail")
        .0
}

fn joined_to_previous(old: &[Token], start: usize) -> bool {
    old[..start].last().is_some_and(|t| {
        t.kind != Kind::Space
            && t.kind != Kind::LineBreak
            && !matches!(t.text.as_str(), "," | "." | ";" | ":" | "?" | "!")
    })
}

pub(super) fn pair(old: &[Token], right: &[Token], mode: Alternative) -> Option<usize> {
    let last = old.last().filter(|t| t.kind == Kind::Word)?;
    let first = right.first().filter(|t| t.kind == Kind::Word)?;
    let lkey = word_key(&last.text);
    let rkey = word_key(&first.text);
    if right.get(1).is_some_and(super::is_joiner) {
        return None;
    }
    if let Some(group) = class(&lkey)
        && class(&rkey) == Some(group)
        && !joined_to_previous(old, old.len() - 1)
    {
        return Some(old.len() - 1);
    }
    let lhs = left_words(old);
    let rhs: Vec<_> = right
        .iter()
        .take_while(|t| matches!(t.kind, Kind::Word | Kind::Space))
        .filter(|t| t.kind == Kind::Word)
        .take(4)
        .collect();
    for (i, head) in lhs.iter().enumerate() {
        let len = lhs.len() - i;
        // Repeated article/preposition and matching phrase structure give us
        // a reproducible left boundary, even with more words after the repair.
        if len >= 2
            && rhs.len() >= 2
            && word_key(&head.text) == rkey
            && nominal(head, &lhs[i + 1..])
            && (2..=rhs.len()).any(|n| nominal(first, &rhs[1..n]))
            && (mode != Alternative::Contrast
                || rkey == "nach"
                || (class(&lkey).is_some() && class(&word_key(&rhs[1].text)) == class(&lkey)))
        {
            let start = starts_at(old, len);
            if !joined_to_previous(old, start) {
                return Some(start);
            }
        }
    }
    // Explicit "nein, ich meine" also licenses isolated alternatives and
    // place/name substitutions after a shared preposition.
    let prefix = super::without_trailing_space(&old[..old.len() - 1]);
    let prior = prefix.last();
    let standalone = prior.is_none_or(|t| {
        t.kind == Kind::LineBreak
            || (t.kind == Kind::Punctuation
                && matches!(t.text.as_str(), "." | "!" | "?" | "," | ":"))
    });
    let shared_preposition =
        prior.is_some_and(|t| t.kind == Kind::Word && preposition(&word_key(&t.text)));
    let after_verb = prefix.split_last().is_some_and(|(verb, before)| {
        super::without_trailing_space(before)
            .last()
            .is_some_and(|subject| crate::fillers::verb_agrees_with_pronoun(verb, subject))
    });
    if mode == Alternative::Explicit
        && possible_left(last)
        && possible_left(first)
        && (standalone
            || ((shared_preposition || after_verb) && capitalized(last) && capitalized(first)))
        && !joined_to_previous(old, old.len() - 1)
    {
        Some(old.len() - 1)
    } else {
        None
    }
}

pub(super) fn undo_target(old: &[Token]) -> Option<usize> {
    let last = old.last().filter(|t| t.kind == Kind::Word)?;
    let lhs = left_words(old);
    for (i, head) in lhs.iter().enumerate() {
        if nominal(head, &lhs[i + 1..]) {
            let start = starts_at(old, lhs.len() - i);
            if !joined_to_previous(old, start) {
                return Some(start);
            }
        }
    }
    let start = old.len() - 1;
    let prior = super::without_trailing_space(&old[..start]).last();
    let standalone = prior.is_none_or(|t| {
        t.kind == Kind::LineBreak
            || (t.kind == Kind::Punctuation
                && matches!(t.text.as_str(), "," | ":" | ";" | "." | "!" | "?"))
    });
    if !joined_to_previous(old, start)
        && possible_left(last)
        && (class(&word_key(&last.text)).is_some() || (standalone && !capitalized(last)))
    {
        Some(start)
    } else {
        None
    }
}
