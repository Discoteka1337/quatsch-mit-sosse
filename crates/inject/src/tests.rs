use super::*;
use InjectMethod::{Clipboard, UiAutomation, Unicode};

#[test]
fn automatic_insertion_stops_at_the_first_success() {
    for (winner, expected) in [
        (UiAutomation, vec![UiAutomation]),
        (Unicode, vec![UiAutomation, Unicode]),
        (Clipboard, vec![UiAutomation, Unicode, Clipboard]),
    ] {
        let mut tried = Vec::new();
        let result = route("Grüße", None, |method| {
            tried.push(method);
            (method == winner)
                .then_some(())
                .ok_or(InjectError::Unavailable)
        });
        assert_eq!(result, Ok(winner));
        assert_eq!(tried, expected);
    }
}

#[test]
fn forced_routes_are_exclusive_and_exhaustion_is_an_error() {
    for forced in [UiAutomation, Unicode, Clipboard] {
        let mut tried = Vec::new();
        let result = route("Öl", Some(forced), |method| {
            tried.push(method);
            Err(InjectError::Unavailable)
        });
        assert_eq!(result, Err(InjectError::Unavailable));
        assert_eq!(tried, [forced]);
    }
    let mut tried = Vec::new();
    assert_eq!(
        route("Öl", None, |method| {
            tried.push(method);
            Err(InjectError::Unavailable)
        }),
        Err(InjectError::Unavailable)
    );
    assert_eq!(tried, [UiAutomation, Unicode, Clipboard]);
}

#[test]
fn a_terminal_error_never_replays_the_text() {
    for error in [
        InjectError::ReadOnly,
        InjectError::DeliveryUncertain,
        InjectError::TargetChanged,
        InjectError::ClipboardRestore,
        InjectError::ModifiersHeld,
    ] {
        let mut tried = Vec::new();
        let result = route("Öl", None, |method| {
            tried.push(method);
            Err(error)
        });
        assert_eq!(result, Err(error));
        assert_eq!(tried, [UiAutomation]);
    }
}

#[test]
fn threshold_counts_unicode_scalars_and_overrides_even_a_forced_route() {
    let text = "🦀".repeat(2000);
    assert_eq!(route(&text, Some(Unicode), |_| Ok(())), Ok(Unicode));
    for forced in [None, Some(UiAutomation), Some(Unicode)] {
        assert_eq!(
            route(&(text.clone() + "ß"), forced, |_| Ok(())),
            Ok(Clipboard)
        );
    }
}

#[test]
fn empty_input_and_embedded_nul_never_reach_the_target() {
    for text in ["", "eins\0zwei"] {
        assert_eq!(
            route(text, None, |_| panic!("unexpected input")),
            Err(InjectError::InvalidText)
        );
    }
}

#[test]
fn settings_match_the_executable_name_case_insensitively() {
    let config: InjectConfig = r#"
        [apps]
        "WINWORD.EXE" = "clipboard"
        "code.exe" = "unicode"
        "notepad.exe" = "uia"
    "#
    .parse()
    .unwrap();
    assert_eq!(config.method_for("WinWord.exe"), Some(Clipboard));
    assert_eq!(config.method_for("Code.exe"), Some(Unicode));
    assert_eq!(config.method_for("Notepad.exe"), Some(UiAutomation));
    assert_eq!(config.method_for("mycode.exe"), None);
}

#[test]
fn invalid_settings_cannot_silently_change_the_selected_method() {
    for source in [
        "[apps]\n'editor.exe' = 'fast'",
        "[apps]\n'C:/editor.exe' = 'clipboard'",
        "[apps]\n'editor.exe' = 'unicode'\n'Editor.exe' = 'clipboard'",
        "[apps]\n'' = 'uia'",
        "[app]\n'editor.exe' = 'uia'",
    ] {
        assert!(source.parse::<InjectConfig>().is_err());
    }
}

#[test]
fn missing_settings_file_uses_automatic_mode_but_malformed_file_fails() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("inject.toml");
    assert!(
        InjectConfig::load_from(&path)
            .unwrap()
            .method_for("app.exe")
            .is_none()
    );
    std::fs::write(&path, "[apps]\n'app.exe' = 'unicode'").unwrap();
    assert_eq!(
        InjectConfig::load_from(&path)
            .unwrap()
            .method_for("app.exe"),
        Some(Unicode)
    );
    std::fs::write(&path, "invalid!").unwrap();
    assert_eq!(
        InjectConfig::load_from(&path).unwrap_err(),
        InjectError::InvalidConfig
    );
}
