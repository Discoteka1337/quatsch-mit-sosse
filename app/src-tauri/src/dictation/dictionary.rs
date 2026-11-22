use quatsch_mit_sosse_store::Store;
use quatsch_mit_sosse_textfx::{DictionaryEntry, Pipeline, Rule};

pub(super) struct Transcript {
    pub(super) text: String,
    pub(super) warning: Option<String>,
}

impl Transcript {
    pub(super) fn plain(text: String) -> Self {
        Self {
            text,
            warning: None,
        }
    }

    pub(super) fn with_dictionary(
        model: &str,
        text: String,
        open_store: impl FnOnce() -> quatsch_mit_sosse_store::Result<Store>,
    ) -> Self {
        if text.trim().is_empty() {
            return Self::plain(text);
        }
        let mut pipeline = Pipeline::new([]);
        pipeline.primeline_spelling = model == crate::speech::PRIMELINE_MODEL;
        let Ok(entries) = open_store().and_then(|store| store.list("")) else {
            return Self {
                text: pipeline.apply(&text),
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
        Self::plain(pipeline.apply(&text))
    }
}
