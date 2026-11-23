use crate::{
    comparison::DictationRecord,
    settings::{CleanupSettings, FillerMode},
};
use quatsch_mit_sosse_inject::{InjectError, clipboard_text};
use quatsch_mit_sosse_store::Store;
use quatsch_mit_sosse_textfx::{
    DictionaryEntry, FillerStrength, Pipeline, Rule, Snippet, SnippetValue,
};
use std::cell::LazyCell;
use windows::Win32::System::SystemInformation::GetLocalTime;

pub(super) struct Transcript {
    pub(super) record: DictationRecord,
    pub(super) warning: Option<String>,
}

impl Transcript {
    pub(super) fn plain(text: String) -> Self {
        Self {
            record: DictationRecord {
                finished: text.clone(),
                original: text,
                ..Default::default()
            },
            warning: None,
        }
    }

    pub(super) fn from_dictation(
        model: &str,
        text: String,
        settings: CleanupSettings,
        open_store: impl FnOnce() -> quatsch_mit_sosse_store::Result<Store>,
    ) -> Self {
        let now = LazyCell::new(|| unsafe { GetLocalTime() });
        Self::prepare(model, text, settings, open_store, |value| match value {
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
        settings: CleanupSettings,
        open_store: impl FnOnce() -> quatsch_mit_sosse_store::Result<Store>,
        mut resolve: impl FnMut(SnippetValue) -> Result<String, InjectError>,
    ) -> Self {
        if text.trim().is_empty() {
            return Self::plain(text);
        }
        let rules = [
            (Rule::VoiceCommands, settings.voice_commands),
            (Rule::Dictionary, settings.dictionary),
            (Rule::FillerWords, settings.filler_words != FillerMode::Off),
            (Rule::SelfCorrections, settings.self_corrections),
            (Rule::Numbers, settings.numbers),
            (Rule::Punctuation, settings.punctuation),
            (Rule::Snippets, settings.snippets),
        ];
        if !rules.iter().any(|(_, enabled)| *enabled) {
            return Self::plain(text);
        }
        let mut pipeline = Pipeline::new(
            rules
                .into_iter()
                .filter_map(|(rule, enabled)| enabled.then_some(rule)),
        );
        pipeline.digits_from = settings.digits_from;
        if settings.filler_words == FillerMode::Strong {
            pipeline.filler_strength = FillerStrength::Strong;
        }
        pipeline.primeline_spelling =
            settings.dictionary && model == crate::speech::PRIMELINE_MODEL;
        // Registered spellings remain protected when replacements are disabled.
        let Ok((entries, snippets)) = open_store().and_then(|store| {
            Ok((
                store.list("")?,
                if settings.snippets {
                    store.list_snippets("")?
                } else {
                    Vec::new()
                },
            ))
        }) else {
            let mut transcript = Self::plain(text);
            transcript.warning = Some(crate::messages().dictionary_read_warning.clone());
            return transcript;
        };
        pipeline.dictionary = entries
            .into_iter()
            .map(|entry| DictionaryEntry {
                term: entry.term,
                replacement: entry.replacement,
                aliases: entry.aliases,
            })
            .collect();
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
            record: DictationRecord {
                original: text,
                finished: result.text,
                uncertain: result.uncertain,
                cursor: result.cursor,
            },
            warning,
        }
    }
}
