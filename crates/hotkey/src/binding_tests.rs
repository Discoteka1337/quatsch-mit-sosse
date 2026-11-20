use super::*;

#[test]
fn german_chords_parse_independently_of_case_spacing_and_order() {
    let hotkey: Hotkey = "Strg+Umschalt+Leertaste".parse().unwrap();
    assert_eq!(hotkey.modifiers(), CTRL | SHIFT);
    assert_eq!(hotkey.trigger(), Some(0x20));
    assert_eq!(" Space + SHIFT + ctrl ".parse(), Ok(hotkey));
    assert_eq!(hotkey.to_string(), "Strg+Umschalt+Leertaste");
}

#[test]
fn named_keys_letters_digits_and_function_keys_round_trip() {
    for (text, vk) in [
        ("Strg+Win+F8", 0x77),
        ("Alt+F24", 0x87),
        ("Alt+Q", 0x51),
        ("Win+7", 0x37),
        ("Strg+Entf", 0x2e),
        ("Bild auf", 0x21),
        ("Esc", 0x1b),
    ] {
        let hotkey: Hotkey = text.parse().unwrap();
        assert_eq!(hotkey.trigger(), Some(vk));
        assert_eq!(hotkey.to_string().parse(), Ok(hotkey));
    }
}

#[test]
fn invalid_or_ambiguous_chords_are_rejected() {
    for text in [
        "",
        "  ",
        "+",
        "Strg+",
        "+Strg",
        "Strg++W",
        "Strg+Ctrl",
        "Strg+U+V",
        "F0",
        "F25",
        "F01",
        "Strg+Wort",
        "Alt+😀",
    ] {
        assert!(text.parse::<Hotkey>().is_err(), "{text:?}");
    }
}

#[test]
fn modifier_only_and_altgr_conflicts_warn_without_rejecting_the_chord() {
    let only: Hotkey = "Win+Strg".parse().unwrap();
    assert_eq!(only.trigger(), None);
    assert_eq!(only.modifiers(), CTRL | WIN);
    assert!(only.warnings().contains(&HotkeyWarning::ModifiersOnly));
    let altgr: Hotkey = "Strg+Alt+Q".parse().unwrap();
    assert!(altgr.warnings().contains(&HotkeyWarning::AltGr));
}

#[test]
fn windows_and_office_conflicts_match_normalized_keys() {
    for (text, program) in [
        ("l+windows", "Windows"),
        ("CTRL+v", "Office"),
        ("Shift+Ctrl+Space", "Word"),
        ("Strg+Umschalt+Leertaste", "Excel"),
    ] {
        assert!(text.parse::<Hotkey>().unwrap().warnings().iter().any(
            |warning| matches!(warning, HotkeyWarning::KnownShortcut(info) if info.contains(program))
        ));
    }
    assert!(
        "Strg+Win+F8"
            .parse::<Hotkey>()
            .unwrap()
            .warnings()
            .is_empty()
    );
}

#[test]
fn every_published_conflict_uses_a_canonical_supported_chord() {
    for (chord, description) in known_shortcuts() {
        let hotkey: Hotkey = chord.parse().unwrap();
        assert_eq!(&hotkey.to_string(), chord);
        assert!(!description.trim().is_empty());
    }
}
