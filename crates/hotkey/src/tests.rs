use super::*;

#[test]
fn defaults_match_the_two_documented_actions() {
    let config = HotkeyConfig::default();
    assert_eq!(config.mode, HotkeyMode::Hold);
    assert_eq!(config.dictation.to_string(), "Strg+Umschalt+Leertaste");
    assert_eq!(config.reinsert.to_string(), "Strg+Win+F8");
    assert!(config.reinsert.warnings().is_empty());
    assert!(config.validate().is_ok());
}

#[test]
fn duplicate_actions_and_reserved_escape_are_rejected_before_installation() {
    let mut config = HotkeyConfig::default();
    config.reinsert = config.dictation;
    assert!(config.validate().is_err());
    config.reinsert = "Strg+Esc".parse().unwrap();
    assert!(config.validate().is_err());
    config.dictation = "Esc".parse().unwrap();
    config.reinsert = HotkeyConfig::default().reinsert;
    assert!(config.validate().is_err());
}
