use crate::tokens::{Kind, Token, identifier_edge, word_key};

// Only these word families have an unambiguous sharp s in German (Germany).
// A general ss replacement would corrupt words such as "Masse" and "Busse".
fn adjective_ending(ending: &str) -> bool {
    matches!(ending, "" | "e" | "er" | "es" | "em" | "en")
}

fn sharp_s_form(key: &str) -> bool {
    if ["gross", "grösser", "grösst"]
        .iter()
        .any(|stem| key.strip_prefix(stem).is_some_and(adjective_ending))
    {
        return true;
    }
    let Some((prefix, ending)) = key.split_once("schliess") else {
        return false;
    };
    matches!(
        prefix,
        "" | "ab" | "an" | "auf" | "aus" | "be" | "ein" | "ent" | "er" | "um" | "ver" | "zu"
    ) && (matches!(ending, "" | "e" | "en" | "t" | "lich")
        || ending.strip_prefix("end").is_some_and(adjective_ending))
}

pub(super) fn apply(tokens: &mut [Token]) {
    for index in 0..tokens.len() {
        let token = &tokens[index];
        let Some(start) = token.text.rfind("ss") else {
            continue;
        };
        if token.kind != Kind::Word
            || identifier_edge(tokens[..index].iter().rev())
            || identifier_edge(tokens[index + 1..].iter())
        {
            continue;
        }
        // Keep capitals, mixed-case names and surnames such as "Gross".
        // Sentence-initial adverbs are also common in ASR output.
        let key = word_key(&token.text);
        let lower = token.text.chars().all(|c| !c.is_uppercase());
        let adverb = matches!(
            token.text.as_str(),
            "Anschliessend"
                | "Abschliessend"
                | "Schliesslich"
                | "Einschliesslich"
                | "Ausschliesslich"
        );
        if (lower || adverb) && sharp_s_form(&key) {
            tokens[index].text.replace_range(start..start + 2, "ß");
        }
    }
}
