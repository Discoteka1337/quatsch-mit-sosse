use icu_normalizer::ComposingNormalizerBorrowed;
use icu_properties::{
    CodePointMapData,
    props::{GeneralCategory, GeneralCategoryGroup},
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WordErrors {
    pub substitutions: usize,
    pub deletions: usize,
    pub insertions: usize,
    pub reference_words: usize,
}

impl WordErrors {
    pub fn total(&self) -> usize {
        self.substitutions + self.deletions + self.insertions
    }

    pub fn rate(&self) -> Option<f64> {
        (self.reference_words != 0).then(|| self.total() as f64 / self.reference_words as f64)
    }
}

pub fn normalize_words(text: &str) -> Vec<String> {
    let category = CodePointMapData::<GeneralCategory>::new();
    let normalized = ComposingNormalizerBorrowed::new_nfc()
        .normalize(text)
        .chars()
        .filter(|character| !GeneralCategoryGroup::Punctuation.contains(category.get(*character)))
        .flat_map(char::to_lowercase)
        .collect::<String>();
    normalized.split_whitespace().map(str::to_owned).collect()
}

pub fn word_errors(reference: &str, hypothesis: &str) -> WordErrors {
    let reference = normalize_words(reference);
    let hypothesis = normalize_words(hypothesis);
    // Carry edit counts along the minimum-cost alignment using only two rows.
    // Equal-cost paths prefer substitution, then deletion, then insertion.
    let mut previous: Vec<_> = (0..=hypothesis.len())
        .map(|insertions| WordErrors {
            insertions,
            ..WordErrors::default()
        })
        .collect();
    let mut current = vec![WordErrors::default(); hypothesis.len() + 1];
    for (row, expected) in reference.iter().enumerate() {
        current[0] = WordErrors {
            deletions: row + 1,
            ..WordErrors::default()
        };
        for (column, actual) in hypothesis.iter().enumerate() {
            let mut diagonal = previous[column];
            diagonal.substitutions += usize::from(expected != actual);
            let mut deletion = previous[column + 1];
            deletion.deletions += 1;
            let mut insertion = current[column];
            insertion.insertions += 1;
            current[column + 1] = [diagonal, deletion, insertion]
                .into_iter()
                .min_by_key(WordErrors::total)
                .unwrap();
        }
        std::mem::swap(&mut previous, &mut current);
    }
    let mut result = previous[hypothesis.len()];
    result.reference_words = reference.len();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_case_and_unicode_punctuation_preserving_german_letters() {
        assert_eq!(
            normalize_words("„ÄPFEL“, ÖL; ÜBER: SOẞE! E-Mail … geht’s?"),
            ["äpfel", "öl", "über", "soße", "email", "gehts"]
        );
        assert_eq!(word_errors("Straße", "STRASSE").substitutions, 1);
        assert_eq!(word_errors("schön", "schon").substitutions, 1);
    }

    #[test]
    fn normalizes_canonically_equivalent_umlauts_and_whitespace() {
        assert_eq!(
            normalize_words("  A\u{308}PFEL\tO\u{308}l\n U\u{308}ber\u{a0}Soße "),
            ["äpfel", "öl", "über", "soße"]
        );
        assert_eq!(word_errors("Äpfel!", "A\u{308}PFEL").total(), 0);
    }

    #[test]
    fn retains_non_punctuation_symbols_and_does_not_expand_numbers() {
        assert_eq!(normalize_words("15 € + 3,5 %"), ["15", "€", "+", "35"]);
        assert_eq!(word_errors("fünf", "5").substitutions, 1);
    }

    #[test]
    fn distinguishes_substitutions_insertions_and_deletions() {
        let result = word_errors(
            "eins zwei drei vier fünf sechs sieben",
            "eins neu drei fünf sechs sieben acht",
        );
        assert_eq!(
            result,
            WordErrors {
                substitutions: 1,
                deletions: 1,
                insertions: 1,
                reference_words: 7,
            }
        );
        assert_eq!(result.total(), 3);
        assert_eq!(result.rate(), Some(3.0 / 7.0));
    }

    #[test]
    fn handles_empty_references_without_fabricating_a_word_error_rate() {
        let result = word_errors("", "ein phantom");
        assert_eq!(result.insertions, 2);
        assert_eq!(result.reference_words, 0);
        assert_eq!(result.rate(), None);
        assert_eq!(word_errors("…", "").total(), 0);
        assert_eq!(word_errors("eins zwei", "").deletions, 2);
    }

    #[test]
    fn word_error_rate_can_exceed_one_hundred_percent() {
        let result = word_errors("eins", "eins zwei drei");
        assert_eq!(result.insertions, 2);
        assert_eq!(result.rate(), Some(2.0));
    }

    #[test]
    fn repeated_words_choose_an_optimal_alignment() {
        let result = word_errors("ja ja nein", "ja nein nein");
        assert_eq!(result.substitutions, 1);
        assert_eq!(result.total(), 1);
        let shifted = word_errors("a b c", "b c d");
        assert_eq!(shifted.total(), 2);
        assert_eq!(shifted.deletions, 1);
        assert_eq!(shifted.insertions, 1);
    }
}
