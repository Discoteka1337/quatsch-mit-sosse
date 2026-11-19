use super::*;
use tauri::test::{mock_builder, mock_context, noop_assets};

fn config() -> tauri::Config {
    serde_json::from_str(include_str!("../tauri.conf.json")).unwrap()
}

#[test]
fn window_defaults_keep_the_app_in_the_tray() {
    let config = config();
    assert_eq!(config.identifier, "de.quatschmitsosse.app");
    assert_eq!(
        config.product_name.as_deref(),
        Some(messages().app_name.as_str())
    );
    assert_eq!(
        config.main_binary_name.as_deref(),
        Some("quatsch-mit-sosse")
    );
    assert_eq!(config.app.windows.len(), 2);

    let settings = config
        .app
        .windows
        .iter()
        .find(|window| window.label == "settings")
        .unwrap();
    assert!(!settings.visible);
    assert_eq!(settings.title, messages().app_name);

    let overlay = config
        .app
        .windows
        .iter()
        .find(|window| window.label == "overlay")
        .unwrap();
    assert!(!overlay.visible);
    assert!(!overlay.decorations);
    assert!(overlay.transparent);
    assert!(overlay.always_on_top);
    assert!(overlay.skip_taskbar);
    assert!(!overlay.focus);
}

#[test]
fn windows_never_start_a_browser_automatically() {
    for window in config().app.windows {
        assert!(
            !window.create,
            "{} must be created explicitly as a native window",
            window.label
        );
    }
    let json: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    assert_eq!(
        json["bundle"]["windows"]["webviewInstallMode"]["type"],
        "skip"
    );
}

#[test]
fn native_texts_come_from_the_shared_german_catalog() {
    let text = messages();
    assert_eq!(text.app_name, "Quatsch mit Soße");
    assert_eq!(text.settings, "Einstellungen");
    assert_eq!(text.quit, "Beenden");
    for source in [
        include_str!("../../ui/src/locales/de.json"),
        include_str!("../../ui/src/locales/en.json"),
    ] {
        let locale: Messages = serde_json::from_str(source).unwrap();
        assert!(!locale.settings_open_error.is_empty());
        assert!(!locale.startup_error.is_empty());
        assert!(!locale.settings_placeholder.is_empty());
        assert!(!locale.overlay_placeholder.is_empty());
    }
}

#[test]
fn missing_settings_are_created_and_reused_without_duplicates() {
    let mut context = mock_context(noop_assets());
    *context.config_mut() = config();
    let app = mock_builder().build(context).unwrap();
    assert!(app.windows().is_empty());

    let create = || create_native_window(app.handle(), "settings");
    show_settings_using(app.handle(), create).unwrap();
    assert!(app.get_window("settings").is_some());
    show_settings_using(app.handle(), || panic!("Existing settings must be reused")).unwrap();
    assert_eq!(app.windows().len(), 1);
    assert!(app.webview_windows().is_empty());
}

#[test]
fn both_scaffold_windows_are_native_and_unknown_labels_fail() {
    let mut context = mock_context(noop_assets());
    *context.config_mut() = config();
    let app = mock_builder().build(context).unwrap();
    create_native_window(app.handle(), "settings").unwrap();
    create_native_window(app.handle(), "overlay").unwrap();
    assert_eq!(app.windows().len(), 2);
    assert!(app.webview_windows().is_empty());
    assert!(create_native_window(app.handle(), "unknown").is_err());
}
