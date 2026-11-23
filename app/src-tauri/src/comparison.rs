use similar::{ChangeTag, TextDiff};
use std::{ops::Range, time::Duration};

#[derive(Clone, Default)]
pub(crate) struct DictationRecord {
    pub original: String,
    pub finished: String,
    /// Nonoverlapping UTF-8 byte ranges in the finished text, in reading order.
    pub uncertain: Vec<Range<usize>>,
    pub cursor: Option<usize>,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum SegmentKind {
    Unchanged,
    Removed,
    Added,
    Uncertain,
}

pub(crate) struct Segment<'a> {
    pub text: &'a str,
    pub kind: SegmentKind,
}

pub(crate) struct Comparison<'a> {
    pub before: Vec<Segment<'a>>,
    pub after: Vec<Segment<'a>>,
}

impl DictationRecord {
    pub fn comparison(&self) -> Comparison<'_> {
        let mut before = Side::new(&self.original, &[]);
        let mut after = Side::new(&self.finished, &self.uncertain);
        if self.original == self.finished {
            before.append(self.original.len(), SegmentKind::Unchanged);
            after.append(self.finished.len(), SegmentKind::Unchanged);
        } else {
            // A large snippet or clipboard expansion can replace a whole dictation.
            // Bound the search for the closest alignment while retaining all text.
            let diff = TextDiff::configure()
                .timeout(Duration::from_millis(100))
                .diff_unicode_words(self.original.as_str(), self.finished.as_str());
            for change in diff.iter_all_changes() {
                let bytes = change.value().len();
                match change.tag() {
                    ChangeTag::Equal => {
                        before.append(bytes, SegmentKind::Unchanged);
                        after.append(bytes, SegmentKind::Unchanged);
                    }
                    ChangeTag::Delete => before.append(bytes, SegmentKind::Removed),
                    ChangeTag::Insert => after.append(bytes, SegmentKind::Added),
                }
            }
        }
        Comparison {
            before: before.segments,
            after: after.segments,
        }
    }
}

struct Side<'a> {
    text: &'a str,
    uncertain: &'a [Range<usize>],
    offset: usize,
    segments: Vec<Segment<'a>>,
}

impl<'a> Side<'a> {
    fn new(text: &'a str, uncertain: &'a [Range<usize>]) -> Self {
        Self {
            text,
            uncertain,
            offset: 0,
            segments: Vec::new(),
        }
    }

    fn append(&mut self, bytes: usize, kind: SegmentKind) {
        let end = self.offset + bytes;
        while let Some((range, rest)) = self.uncertain.split_first() {
            if range.start >= end {
                break;
            }
            self.push_to(range.start, kind);
            self.push_to(range.end.min(end), SegmentKind::Uncertain);
            if range.end > end {
                break;
            }
            self.uncertain = rest;
        }
        self.push_to(end, kind);
    }

    fn push_to(&mut self, end: usize, kind: SegmentKind) {
        if end <= self.offset {
            return;
        }
        if let Some(last) = self.segments.last_mut().filter(|last| last.kind == kind) {
            let start = self.offset - last.text.len();
            last.text = &self.text[start..end];
        } else {
            self.segments.push(Segment {
                text: &self.text[self.offset..end],
                kind,
            });
        }
        self.offset = end;
    }
}

#[cfg(test)]
#[path = "comparison/tests.rs"]
mod tests;
