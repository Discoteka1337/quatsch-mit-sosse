use super::Snippet;
use crate::tokens::word_key;
use icu_properties::{
    CodePointMapData,
    props::{GeneralCategory, GeneralCategoryGroup},
};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Boundary {
    Space,
    Punctuation,
}

type Boundaries = BTreeMap<usize, Boundary>;

// Keep word boundaries separate from the letters. Written punctuation may be
// joined or spoken with a space; two ordinary words must still be two words.
fn parts(text: &str) -> (String, Boundaries) {
    let mut letters = String::new();
    let mut boundaries = BTreeMap::new();
    let mut pending = None;
    let category = CodePointMapData::<GeneralCategory>::new();
    for ch in word_key(text).chars() {
        if ch.is_whitespace() {
            pending = Some(Boundary::Space);
        } else if GeneralCategoryGroup::Punctuation.contains(category.get(ch)) {
            pending.get_or_insert(Boundary::Punctuation);
        } else {
            if let Some(boundary) = pending.take()
                && !letters.is_empty()
            {
                boundaries.insert(letters.len(), boundary);
            }
            if ch == 'ß' {
                letters.push_str("ss");
            } else {
                letters.push(ch);
            }
        }
    }
    (letters, boundaries)
}

/// Canonical key for storing and rejecting duplicate German triggers.
pub fn snippet_trigger_key(trigger: &str) -> String {
    let (letters, boundaries) = parts(trigger);
    let mut key = String::with_capacity(letters.len() + boundaries.len());
    for (offset, ch) in letters.char_indices() {
        if boundaries.get(&offset) == Some(&Boundary::Space) {
            key.push(' ');
        }
        key.push(ch);
    }
    key
}

struct Template<'a> {
    boundaries: Boundaries,
    text: &'a str,
}

pub(super) struct Triggers<'a>(HashMap<String, Vec<Template<'a>>>);

impl<'a> Triggers<'a> {
    pub(super) fn new(snippets: &'a [Snippet]) -> Self {
        let mut index = HashMap::<_, Vec<_>>::new();
        for snippet in snippets.iter().filter(|snippet| !snippet.text.is_empty()) {
            let (key, boundaries) = parts(&snippet.trigger);
            if !key.is_empty() {
                index.entry(key).or_default().push(Template {
                    boundaries,
                    text: &snippet.text,
                });
            }
        }
        Self(index)
    }

    pub(super) fn find(&self, sentence: &str) -> Option<&'a str> {
        let (key, spoken) = parts(sentence);
        let matches = self.0.get(&key)?.iter().filter(|entry| {
            spaces_fit(&spoken, &entry.boundaries) && spaces_fit(&entry.boundaries, &spoken)
        });
        // Prefer exact word divisions when several written forms fit.
        let mut exact = matches
            .clone()
            .filter(|entry| spaces(&spoken).eq(spaces(&entry.boundaries)));
        if let Some(entry) = exact.next() {
            return exact.next().is_none().then_some(entry.text);
        }
        let mut remaining = matches;
        let first = remaining.next()?;
        remaining.next().is_none().then_some(first.text)
    }
}

fn spaces_fit(source: &Boundaries, other: &Boundaries) -> bool {
    spaces(source).all(|at| other.contains_key(at))
}

fn spaces(boundaries: &Boundaries) -> impl Iterator<Item = &usize> {
    boundaries
        .iter()
        .filter_map(|(at, mark)| (*mark == Boundary::Space).then_some(at))
}
