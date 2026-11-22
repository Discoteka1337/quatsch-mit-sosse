pub(crate) fn cardinal(word: &str) -> Option<u32> {
    if let Some((head, tail)) = word.split_once("tausend") {
        let thousands = match head {
            "" | "ein" => 1,
            _ => hundreds(head).filter(|n| (2..1000).contains(n))?,
        };
        let rest = match tail {
            "" => 0,
            _ => hundreds(tail).filter(|n| (1..1000).contains(n))?,
        };
        return Some(thousands * 1000 + rest);
    }
    hundreds(word)
}

fn hundreds(word: &str) -> Option<u32> {
    if let Some((head, tail)) = word.split_once("hundert") {
        let hundreds = match head {
            "" | "ein" => 1,
            // Eleven to nineteen also occur in spoken years.
            _ => under_hundred(head).filter(|n| (2..=19).contains(n))?,
        };
        let rest = match tail {
            "" => 0,
            _ => under_hundred(tail).filter(|n| *n > 0)?,
        };
        return Some(hundreds * 100 + rest);
    }
    under_hundred(word)
}

fn under_hundred(word: &str) -> Option<u32> {
    Some(match word {
        "null" => 0,
        "eins" => 1,
        "zehn" => 10,
        "elf" => 11,
        "zwölf" => 12,
        "dreizehn" => 13,
        "vierzehn" => 14,
        "fünfzehn" => 15,
        "sechzehn" => 16,
        "siebzehn" => 17,
        "achtzehn" => 18,
        "neunzehn" => 19,
        _ => {
            if let Some((unit, ten)) = word.split_once("und") {
                return Some(unit_stem(unit)? + tens(ten)?);
            }
            // The form "ein" on its own is also an indefinite article.
            if word == "ein" {
                return None;
            }
            return unit_stem(word).or_else(|| tens(word));
        }
    })
}

fn unit_stem(word: &str) -> Option<u32> {
    Some(match word {
        "ein" => 1,
        "zwei" => 2,
        "drei" => 3,
        "vier" => 4,
        "fünf" => 5,
        "sechs" => 6,
        "sieben" => 7,
        "acht" => 8,
        "neun" => 9,
        _ => return None,
    })
}

fn tens(word: &str) -> Option<u32> {
    Some(match word {
        "zwanzig" => 20,
        "dreißig" | "dreissig" => 30,
        "vierzig" => 40,
        "fünfzig" => 50,
        "sechzig" => 60,
        "siebzig" => 70,
        "achtzig" => 80,
        "neunzig" => 90,
        _ => return None,
    })
}
