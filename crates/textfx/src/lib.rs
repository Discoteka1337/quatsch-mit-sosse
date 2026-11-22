mod commands;
mod corrections;
mod dictionary;
mod dictionary_entries;
mod fillers;
mod german_spelling;
mod numbers;
mod punctuation;
mod snippets;
mod tokens;

pub use dictionary::software_dictation_pipeline;
pub use dictionary_entries::DictionaryEntry;
pub use snippets::{Snippet, SnippetValue, snippet_trigger_key};
use std::ops::Range;
use tokens::Kind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupResult {
    pub text: String,
    /// Nonoverlapping UTF-8 byte ranges in `text`, in reading order.
    pub uncertain: Vec<Range<usize>>,
    /// UTF-8 byte offset of the first requested caret in the finished text.
    pub cursor: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    VoiceCommands,
    Dictionary,
    FillerWords,
    SelfCorrections,
    Numbers,
    Punctuation,
    Snippets,
    Style,
}

impl Rule {
    /// Execution order, independent of the order in which rules were enabled.
    pub const ORDER: [Self; 8] = [
        Self::VoiceCommands,
        Self::Dictionary,
        Self::FillerWords,
        Self::SelfCorrections,
        Self::Numbers,
        Self::Punctuation,
        Self::Snippets,
        Self::Style,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::VoiceCommands => "voice_commands",
            Self::Dictionary => "dictionary",
            Self::FillerWords => "filler_words",
            Self::SelfCorrections => "self_corrections",
            Self::Numbers => "numbers",
            Self::Punctuation => "punctuation",
            Self::Snippets => "snippets",
            Self::Style => "style",
        }
    }
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillerStrength {
    #[default]
    Standard,
    Strong,
}

pub struct Pipeline {
    enabled: [bool; Rule::ORDER.len()],
    pub filler_strength: FillerStrength,
    /// Minimum value written as digits for ordinary cardinal numbers.
    pub digits_from: u32,
    /// Dictionary terms whose original spelling and internal punctuation are preserved.
    pub proper_names: Vec<String>,
    /// Canonical spellings, optional written replacements and spoken variants.
    pub dictionary: Vec<DictionaryEntry>,
    pub snippets: Vec<Snippet>,
    /// Restore observed primeLine spellings of ß in unprotected words.
    pub primeline_spelling: bool,
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new([Rule::FillerWords])
    }
}

impl Pipeline {
    /// Creates a pipeline with exactly the given rules enabled.
    pub fn new(rules: impl IntoIterator<Item = Rule>) -> Self {
        let mut pipeline = Self {
            enabled: [false; Rule::ORDER.len()],
            filler_strength: FillerStrength::Standard,
            digits_from: 13,
            proper_names: Vec::new(),
            dictionary: Vec::new(),
            snippets: Vec::new(),
            primeline_spelling: false,
        };
        for rule in rules {
            pipeline.set_enabled(rule, true);
        }
        pipeline
    }

    pub fn set_enabled(&mut self, rule: Rule, enabled: bool) {
        self.enabled[rule as usize] = enabled;
    }

    pub fn is_enabled(&self, rule: Rule) -> bool {
        self.enabled[rule as usize]
    }

    pub fn apply(&self, input: &str) -> String {
        self.apply_with_review(input).text
    }

    pub fn apply_with_review(&self, input: &str) -> CleanupResult {
        self.apply_with_snippet_values(input, |_| None)
    }

    /// Resolves external placeholder values only when a matching template uses them.
    pub fn apply_with_snippets(
        &self,
        input: &str,
        mut value: impl FnMut(SnippetValue) -> String,
    ) -> CleanupResult {
        self.apply_with_snippet_values(input, |kind| Some(value(kind)))
    }

    /// Keeps the original sentence when a requested placeholder is unavailable.
    pub fn apply_with_snippet_values(
        &self,
        input: &str,
        mut value: impl FnMut(SnippetValue) -> Option<String>,
    ) -> CleanupResult {
        if !self.enabled.contains(&true) && !self.primeline_spelling {
            return CleanupResult {
                text: input.to_owned(),
                uncertain: Vec::new(),
                cursor: None,
            };
        }
        let replace = self.is_enabled(Rule::Dictionary);
        let mut tokens = if !replace || self.is_enabled(Rule::VoiceCommands) {
            tokens::tokenize_with_terms(input, &self.proper_names, &self.dictionary, replace)
        } else {
            tokens::tokenize(input, &self.proper_names)
        };
        for rule in Rule::ORDER {
            if self.is_enabled(rule) {
                match rule {
                    Rule::VoiceCommands => commands::apply(&mut tokens),
                    Rule::Dictionary => {
                        dictionary_entries::apply(
                            &mut tokens,
                            &self.dictionary,
                            &self.proper_names,
                        );
                        dictionary::apply(&mut tokens, &self.proper_names);
                        german_spelling::apply(&mut tokens);
                    }
                    Rule::FillerWords => fillers::apply(
                        &mut tokens,
                        self.filler_strength,
                        self.is_enabled(Rule::SelfCorrections),
                    ),
                    Rule::SelfCorrections => corrections::apply(&mut tokens),
                    Rule::Numbers => numbers::apply(&mut tokens, self.digits_from),
                    Rule::Punctuation => punctuation::apply(&mut tokens),
                    Rule::Snippets => snippets::apply(&mut tokens, &self.snippets, &mut value),
                    Rule::Style => {}
                }
            }
        }
        if self.primeline_spelling {
            german_spelling::apply_primeline(&mut tokens);
        }
        let mut text = String::with_capacity(input.len());
        let mut uncertain = Vec::new();
        let mut cursor = None;
        for token in tokens {
            let start = text.len();
            text.push_str(&token.text);
            if token.kind == Kind::Uncertain {
                uncertain.push(start..text.len());
            } else if token.kind == Kind::Cursor {
                cursor.get_or_insert(start);
            }
        }
        CleanupResult {
            text,
            uncertain,
            cursor,
        }
    }
}
