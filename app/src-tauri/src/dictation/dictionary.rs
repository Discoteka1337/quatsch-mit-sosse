use quatsch_mit_sosse_inject::{InjectError, clipboard_text};
use quatsch_mit_sosse_store::Store;
use quatsch_mit_sosse_textfx::{DictionaryEntry, Pipeline, Rule, Snippet, SnippetValue};
use std::cell::LazyCell;
use windows::Win32::System::SystemInformation::GetLocalTime;

pub(super) struct Transcript {
    pub(super) text: String,
    pub(super) cursor: Option<usize>,
    pub(super) warning: Option<String>,
    pub(super) heard_speech: bool,
}

impl Transcript {
    pub(super) fn plain(text: String) -> Self {
        Self {
            heard_speech: !text.trim().is_empty(),
            text,
            cursor: None,
            warning: None,
        }
    }

    pub(super) fn from_dictation(
        model: &str,
        text: String,
        open_store: impl FnOnce() -> quatsch_mit_sosse_store::Result<Store>,
    ) -> Self {
        let now = LazyCell::new(|| unsafe { GetLocalTime() });
        Self::prepare(model, text, open_store, |value| match value {
            SnippetValue::Date => Ok(format!(
                "{:02}.{:02}.{:04}",
                now.wDay, now.wMonth, now.wYear
            )),
            SnippetValue::Time => Ok(format!("{:02}:{:02}", now.wHour, now.wMinute)),
            SnippetValue::Clipboard => clipboard_text(),
        })
    }

    pub(super) fn prepare(
        model: &str,
        text: String,
        open_store: impl FnOnce() -> quatsch_mit_sosse_store::Result<Store>,
        mut resolve: impl FnMut(SnippetValue) -> Result<String, InjectError>,
    ) -> Self {
        if text.trim().is_empty() {
            return Self::plain(text);
        }
        let mut pipeline = Pipeline::new([]);
        pipeline.primeline_spelling = model == crate::speech::PRIMELINE_MODEL;
        let Ok((entries, snippets)) =
            open_store().and_then(|store| Ok((store.list("")?, store.list_snippets("")?)))
        else {
            return Self {
                text: pipeline.apply(&text),
                cursor: None,
                heard_speech: true,
                warning: Some(crate::messages().dictionary_read_warning.clone()),
            };
        };
        pipeline.set_enabled(Rule::Dictionary, true);
        pipeline.dictionary = entries
            .into_iter()
            .map(|entry| DictionaryEntry {
                term: entry.term,
                replacement: entry.replacement,
                aliases: entry.aliases,
            })
            .collect();
        pipeline.set_enabled(Rule::Snippets, true);
        pipeline.snippets = snippets
            .into_iter()
            .map(|entry| Snippet {
                trigger: entry.trigger,
                text: entry.text,
            })
            .collect();
        let mut warning = None;
        let result = pipeline.apply_with_snippet_values(&text, |value| {
            resolve(value)
                .inspect_err(|_| {
                    warning = Some(crate::messages().snippet_clipboard_warning.clone());
                })
                .ok()
        });
        Self {
            text: result.text,
            cursor: result.cursor,
            heard_speech: true,
            warning,
        }
    }
}
