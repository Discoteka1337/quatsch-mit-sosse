use serde::Deserialize;
use std::sync::{LazyLock, Mutex};
use tauri::{
    AppHandle, Manager, RunEvent, Runtime, WebviewWindowBuilder,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Messages {
    app_name: String,
    settings: String,
    quit: String,
    settings_open_error: String,
    startup_error: String,
}

fn messages() -> &'static Messages {
    static MESSAGES: LazyLock<Messages> = LazyLock::new(|| {
        serde_json::from_str(include_str!("../../ui/src/locales/de.json"))
            .expect("The embedded German locale must be valid")
    });
    &MESSAGES
}

pub fn startup_error() -> &'static str {
    &messages().startup_error
}

fn show_settings<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let window = match app.get_webview_window("settings") {
        Some(window) => window,
        None => {
            let config = app
                .config()
                .app
                .windows
                .iter()
                .find(|window| window.label == "settings")
                .expect("The settings window configuration must exist");
            WebviewWindowBuilder::from_config(app, config)?
                .title(&messages().app_name)
                .build()?
        }
    };
    window.unminimize()?;
    window.show()?;
    window.set_focus()
}

fn request_settings(app: &AppHandle) {
    static OPEN_LOCK: Mutex<()> = Mutex::new(());
    let app = app.clone();
    // WebView2 creation must not block a synchronous Windows event handler.
    tauri::async_runtime::spawn_blocking(move || {
        let Ok(_guard) = OPEN_LOCK.lock() else {
            eprintln!("{}", messages().settings_open_error);
            return;
        };
        if show_settings(&app).is_err() {
            eprintln!("{}", messages().settings_open_error);
        }
    });
}

pub fn run() -> tauri::Result<()> {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            request_settings(app);
        }))
        .setup(|app| {
            let text = messages();
            let settings = MenuItem::with_id(app, "settings", &text.settings, true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", &text.quit, true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings, &quit])?;
            TrayIconBuilder::with_id("main")
                .icon(
                    app.default_window_icon()
                        .expect("The app icon must exist")
                        .clone(),
                )
                .tooltip(&text.app_name)
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => request_settings(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            Ok(())
        })
        .build(tauri::generate_context!())?;

    // Keep the tray process alive while allowing windows and webviews to be destroyed.
    app.run(|_app, event| {
        if let RunEvent::ExitRequested {
            code: None, api, ..
        } = event
        {
            api.prevent_exit();
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests;
