mod phrases;

use std::iter;

use crate::tokens::{Kind, Token, word_key};

pub(super) fn apply(tokens: &mut Vec<Token>) {
    let mut input = std::mem::take(tokens).into_iter();
    while let Some(token) = input.next() {
        if token.kind != Kind::Word {
            tokens.push(token);
            continue;
        }
        let rest = input.as_slice();
        let edit = match word_key(&token.text).as_str() {
            "nein" => explicit(tokens, rest),
            "sondern" => contrast(tokens, rest),
            "äh" if hesitation(tokens, rest) == Hesitation::Alternative => {
                replacement(tokens, after_pause(rest), Alternative::Hesitation)
            }
            "streich" => undo(tokens, rest, "das"),
            "kommando" => undo(tokens, rest, "zurück"),
            _ => None,
        };
        let Some(edit) = edit else {
            tokens.push(token);
            continue;
        };
        let marker = iter::once(token).chain(input.by_ref().take(edit.consume));
        if edit.uncertain {
            let text: String = tokens
                .drain(edit.start..)
                .chain(marker)
                .map(|t| t.text)
                .collect();
            tokens.push(Token::new(Kind::Uncertain, text));
        } else {
            tokens.truncate(edit.start);
            marker.for_each(drop);
        }
    }
}

struct Edit {
    start: usize,   // Retain all output tokens before this index.
    consume: usize, // Input tokens consumed after the trigger word.
    uncertain: bool,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Alternative {
    Explicit,
    Hesitation,
    Contrast,
}

fn word(token: Option<&Token>, key: &str) -> bool {
    token.is_some_and(|t| t.kind == Kind::Word && word_key(&t.text) == key)
}

fn is_comma(token: &Token) -> bool {
    token.kind == Kind::Punctuation && token.text == ","
}

fn is_term(token: &Token) -> bool {
    matches!(token.kind, Kind::Word | Kind::Protected)
}

fn is_joiner(t: &Token) -> bool {
    t.kind == Kind::Punctuation && matches!(t.text.as_str(), "-" | "‑" | "‐" | "’" | "'")
}

fn without_trailing_space(tokens: &[Token]) -> &[Token] {
    let end = tokens
        .iter()
        .rposition(|t| t.kind != Kind::Space)
        .map_or(0, |i| i + 1);
    &tokens[..end]
}

fn skip_space(tokens: &[Token]) -> usize {
    tokens.iter().take_while(|t| t.kind == Kind::Space).count()
}

fn phrase_end(tokens: &[Token]) -> usize {
    let end = tokens
        .iter()
        .take_while(|t| {
            (t.kind == Kind::Space || is_term(t) || is_joiner(t))
                && !(t.kind == Kind::Word
                    && matches!(word_key(&t.text).as_str(), "und" | "aber" | "oder" | "denn"))
        })
        .count();
    without_trailing_space(&tokens[..end]).len()
}

fn before_pause(left: &[Token]) -> &[Token] {
    let left = without_trailing_space(left);
    if left.last().is_some_and(is_comma) {
        without_trailing_space(&left[..left.len() - 1])
    } else {
        left
    }
}

// The offset counts spaces and an optional recognizer comma.
fn after_pause(tokens: &[Token]) -> (&[Token], usize) {
    let mut at = skip_space(tokens);
    if tokens.get(at).is_some_and(is_comma) {
        at += 1;
        at += skip_space(&tokens[at..]);
    }
    (&tokens[at..], at)
}

#[derive(PartialEq, Eq)]
pub(super) enum Hesitation {
    Ordinary,
    Alternative,
    BeforeExplicit,
}

pub(super) fn hesitation(left: &[Token], rest: &[Token]) -> Hesitation {
    let right = after_pause(rest).0;
    if word(right.first(), "nein")
        && explicit_tail(&right[1..]).is_some_and(|(rhs, _)| rhs.first().is_some_and(is_term))
        && before_pause(left).last().is_some_and(is_term)
    {
        return Hesitation::BeforeExplicit;
    }
    if !without_trailing_space(left).last().is_some_and(is_comma)
        || !rest.get(skip_space(rest)).is_some_and(is_comma)
    {
        return Hesitation::Ordinary;
    }
    let lhs = before_pause(left).last();
    if lhs.is_some_and(phrases::possible_left) && right.first().is_some_and(phrases::possible_right)
    {
        Hesitation::Alternative
    } else {
        Hesitation::Ordinary
    }
}

fn explicit_tail(rest: &[Token]) -> Option<(&[Token], usize)> {
    let (words, prefix) = after_pause(rest);
    let [ich, space, meine, tail @ ..] = words else {
        return None;
    };
    if !word(Some(ich), "ich") || space.kind != Kind::Space || !word(Some(meine), "meine") {
        return None;
    }
    let gap = if tail.get(skip_space(tail)).is_some_and(is_term) {
        skip_space(tail)
    } else {
        0
    };
    Some((&tail[gap..], prefix + 3 + gap))
}

fn explicit(left: &[Token], rest: &[Token]) -> Option<Edit> {
    let tail = explicit_tail(rest)?;
    let old = before_pause(left);
    // The explicit marker owns an immediately preceding hesitation, including
    // its pause commas. Keeping it until now also preserves uncertain repairs.
    let left = if word(old.last(), "äh") && tail.0.first().is_some_and(is_term) {
        &old[..old.len() - 1]
    } else {
        left
    };
    replacement(left, tail, Alternative::Explicit)
}

fn replacement(
    left: &[Token],
    (right, consume): (&[Token], usize),
    mode: Alternative,
) -> Option<Edit> {
    let old = before_pause(left);
    old.last().filter(|t| is_term(t))?;
    if let Some(start) = phrases::pair(old, right, mode) {
        return Some(Edit {
            start,
            consume,
            uncertain: false,
        });
    }
    // Include the two adjacent alternatives and the intervening marker. The
    // rest of the preceding clause remains available for other rules.
    let mut start = old.len() - 1;
    while start >= 2 && is_joiner(&old[start - 1]) && is_term(&old[start - 2]) {
        start -= 2;
    }
    Some(Edit {
        start,
        consume: consume + phrase_end(right),
        uncertain: true,
    })
}

fn contrast(left: &[Token], rest: &[Token]) -> Option<Edit> {
    let old = before_pause(left);
    let start = old
        .iter()
        .rposition(|t| t.kind != Kind::Space && !is_term(t))
        .map_or(0, |i| i + 1);
    let negation = old[start..].iter().rposition(|t| word(Some(t), "nicht"))? + start;
    let gap = skip_space(&old[negation + 1..]);
    let first = negation + 1 + gap;
    let rhs_start = skip_space(rest);
    let right = &rest[rhs_start..];
    if ["nur", "allein", "bloß"]
        .iter()
        .any(|key| word(old.get(first), key))
        || word(right.first(), "auch")
    {
        return None;
    }
    let clear = phrases::pair(&old[first..], right, Alternative::Contrast) == Some(0);
    Some(Edit {
        start: negation,
        consume: rhs_start + if clear { 0 } else { phrase_end(right) },
        uncertain: !clear,
    })
}

fn undo(left: &[Token], rest: &[Token], second: &str) -> Option<Edit> {
    let after_first = skip_space(rest);
    if after_first == 0 || !word(rest.get(after_first), second) {
        return None;
    }
    let command_len = after_first + 1;
    let tail = &rest[command_len..];
    let next = tail.get(skip_space(tail));
    if next.is_some_and(|t| {
        t.kind != Kind::LineBreak
            && !(matches!(t.kind, Kind::Punctuation | Kind::CommandPunctuation)
                && matches!(t.text.as_str(), "." | "!" | ","))
    }) {
        return None;
    }
    let old = before_pause(left);
    let last = old.last();
    // The article in "das Kommando zurück" belongs to literal dictation.
    if (second == "zurück" && word(last, "das")) || word(last, "bitte") {
        return None;
    }
    if let Some(start) = phrases::undo_target(old) {
        let (replacement, skipped) = after_pause(tail);
        let continues = next.is_some_and(is_comma) && replacement.first().is_some_and(is_term);
        let prefix = without_trailing_space(&old[..start]);
        let whole_fragment = prefix.last().is_none_or(|t| {
            t.kind == Kind::LineBreak
                || (t.kind == Kind::Punctuation && matches!(t.text.as_str(), "." | "?" | "!"))
        });
        let mut consume = command_len;
        let mut trim_left = !continues;
        if continues {
            consume += skipped;
        } else if whole_fragment && next.is_some_and(|t| t.kind == Kind::Punctuation) {
            consume += skip_space(tail) + 1;
            consume += skip_space(&rest[consume..]);
            trim_left = false;
        }
        return Some(Edit {
            start: if trim_left {
                before_pause(prefix).len()
            } else {
                start
            },
            consume,
            uncertain: false,
        });
    }
    Some(Edit {
        start: if last.is_some_and(is_term) {
            old.len() - 1
        } else {
            left.len()
        },
        consume: command_len,
        uncertain: true,
    })
}
