use crate::tokens::{Kind, Token, word_key};

pub(super) fn apply(tokens: &mut Vec<Token>) {
    let mut input = std::mem::take(tokens).into_iter();
    while let Some(token) = input.next() {
        if token.kind != Kind::Word || !word_boundary(tokens.last()) {
            tokens.push(token);
            continue;
        }
        let rest = input.as_slice();
        let Some((mark, extra_tokens)) = command(&token.text, rest) else {
            tokens.push(token);
            continue;
        };
        let Some(marker_len) = sentence_end(&rest[extra_tokens..]) else {
            tokens.push(token);
            continue;
        };
        for _ in 0..extra_tokens + marker_len {
            input.next();
        }

        let opening = matches!(mark, "„" | "(");
        let line_break = mark.starts_with('\n');
        if !opening && tokens.last().is_some_and(|last| last.kind == Kind::Space) {
            tokens.pop();
        }
        if matches!(mark, "." | "," | "?" | "!" | ":") {
            remove_competing_marks(tokens, mark);
        }
        if line_break {
            tokens.extend(mark.chars().map(|_| Token::new(Kind::LineBreak, "\n")));
        } else {
            tokens.push(Token::new(Kind::CommandPunctuation, mark));
        }
        if (opening || line_break)
            && input
                .as_slice()
                .first()
                .is_some_and(|next| next.kind == Kind::Space)
        {
            input.next();
        }
    }
}

fn remove_competing_marks(tokens: &mut Vec<Token>, mark: &str) {
    while let Some((last, before)) = tokens.split_last() {
        if last.kind != Kind::Punctuation
            || !matches!(last.text.as_str(), "." | "," | "!" | "?" | ":" | ";")
        {
            break;
        }
        // A period following a number can belong to an ordinal, e.g. "am 17.?".
        if last.text == "."
            && mark != "."
            && before.last().is_some_and(|previous| {
                previous.kind == Kind::Protected && previous.text.ends_with(char::is_numeric)
            })
        {
            break;
        }
        tokens.pop();
        if tokens.last().is_some_and(|last| last.kind == Kind::Space) {
            tokens.pop();
        }
    }
}

fn word_boundary(previous: Option<&Token>) -> bool {
    previous.is_none_or(|token| {
        matches!(token.kind, Kind::Space | Kind::LineBreak)
            || (matches!(token.kind, Kind::Punctuation | Kind::CommandPunctuation)
                && matches!(
                    token.text.as_str(),
                    "„" | "‚" | "«" | "»" | "\"" | "(" | "["
                ))
    })
}

fn command(word: &str, rest: &[Token]) -> Option<(&'static str, usize)> {
    let first = word_key(word);
    let mark = match first.as_str() {
        "punkt" => ".",
        "komma" => ",",
        "fragezeichen" => "?",
        "ausrufezeichen" => "!",
        "doppelpunkt" => ":",
        "neue" | "neuer" | "anführungszeichen" | "klammer" => {
            let [space, second, ..] = rest else {
                return None;
            };
            if space.kind != Kind::Space || second.kind != Kind::Word {
                return None;
            }
            let mark = match (first.as_str(), second.text.to_ascii_lowercase().as_str()) {
                ("neue", "zeile") => "\n",
                ("neuer", "absatz") => "\n\n",
                ("anführungszeichen", "auf") => "„",
                ("anführungszeichen", "zu") => "“",
                ("klammer", "auf") => "(",
                ("klammer", "zu") => ")",
                _ => return None,
            };
            return Some((mark, 2));
        }
        _ => return None,
    };
    Some((mark, 0))
}

fn is_sentence_mark(token: &Token) -> bool {
    token.kind == Kind::Punctuation && matches!(token.text.as_str(), "." | "!" | "?" | "…")
}

// Return the number of tokens occupied by recognizer punctuation, if present.
// Closing quotes and brackets are context and stay in the original text.
fn sentence_end(rest: &[Token]) -> Option<usize> {
    let space = usize::from(rest.first().is_some_and(|token| token.kind == Kind::Space));
    let rest = &rest[space..];
    let marks = rest
        .iter()
        .take_while(|token| is_sentence_mark(token))
        .count();
    let mut end = marks;
    if end == 0
        && rest.first().is_some_and(|token| {
            token.kind == Kind::Punctuation && matches!(token.text.as_str(), "," | ":" | ";")
        })
    {
        end = 1;
    }
    let closing = rest[end..]
        .iter()
        .take_while(|token| {
            token.kind == Kind::Punctuation
                && matches!(
                    token.text.as_str(),
                    "“" | "‘" | "«" | "»" | "\"" | ")" | "]"
                )
        })
        .count();
    let mut after = &rest[end + closing..];
    let mut terminal = marks > 0;
    if closing > 0 {
        // Punctuation outside a quote or bracket belongs to the surrounding sentence.
        let outer_marks = after
            .iter()
            .take_while(|token| is_sentence_mark(token))
            .count();
        if outer_marks > 0 {
            terminal = true;
            after = &after[outer_marks..];
        } else if terminal
            && after
                .first()
                .is_some_and(|token| token.kind == Kind::Punctuation && token.text == ",")
        {
            // A reporting clause may follow quoted speech: „Komm!“, rief sie.
            after = &after[1..];
        }
    }
    let at_line_end = after
        .iter()
        .find(|token| token.kind != Kind::Space)
        .is_none_or(|token| token.kind == Kind::LineBreak);
    if at_line_end || (terminal && after.first().is_some_and(|token| token.kind == Kind::Space)) {
        Some(if end == 0 { 0 } else { space + end })
    } else {
        None
    }
}
