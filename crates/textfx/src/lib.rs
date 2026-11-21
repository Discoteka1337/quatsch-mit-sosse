mod commands;
mod fillers;
mod tokens;

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
    /// Dictionary terms whose original spelling and internal punctuation are preserved.
    pub proper_names: Vec<String>,
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
            proper_names: Vec::new(),
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
        if !self.enabled.contains(&true) {
            return input.to_owned();
        }
        let mut tokens = tokens::tokenize(input, &self.proper_names);
        for rule in Rule::ORDER {
            if self.is_enabled(rule) {
                match rule {
                    Rule::VoiceCommands => commands::apply(&mut tokens),
                    Rule::FillerWords => fillers::apply(&mut tokens, self.filler_strength),
                    // These stages have a fixed position for subsequent implementation steps.
                    Rule::Dictionary
                    | Rule::SelfCorrections
                    | Rule::Numbers
                    | Rule::Punctuation
                    | Rule::Snippets
                    | Rule::Style => {}
                }
            }
        }
        tokens.into_iter().map(|token| token.text).collect()
    }
}
