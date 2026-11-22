use serde::Deserialize;
use std::sync::{LazyLock, Mutex};
use tauri::{
    AppHandle, Manager, RunEvent, Runtime, Window,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    window::WindowBuilder,
};

mod benchmark;
mod dictation;
mod effects;
mod input;
mod model_window;
mod native_ui;
mod overlay;
mod overlay_placement;
mod overlay_window;
mod preferences;
mod settings;
mod settings_gate;
mod speech;

pub use overlay::{OverlayState, set_overlay_state};
use preferences::{apply_settings, capture_shortcuts, current_settings};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Messages {
    app_name: String,
    audio_overflow: String,
    audio_empty: String,
    recognizer_model_missing: String,
    dictionary_read_warning: String,
    recognizer_fallback_notice: String,
    recognizer_fallback_failed: String,
    settings: String,
    reinsert: String,
    no_last_text: String,
    target_unavailable: String,
    no_speech: String,
    shortcut_error: String,
    worker_error: String,
    overlay_unavailable: String,
    quit: String,
    settings_open_error: String,
    startup_error: String,
    overlay_show_ready: String,
    overlay_hide_from_capture: String,
    overlay_listening: String,
    overlay_processing: String,
    overlay_inserting: String,
    overlay_error: String,
    overlay_copy: String,
    overlay_administrator: String,
    overlay_administrator_help: String,
    overlay_preferences_error: String,
    overlay_copy_error: String,
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

fn show_settings(app: &AppHandle) -> tauri::Result<()> {
    show_settings_using(app, || create_app_window(app, "settings"))
}

fn create_native_window<R: Runtime>(app: &AppHandle<R>, label: &str) -> tauri::Result<Window<R>> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == label)
        .ok_or_else(|| std::io::Error::other("The window configuration is unavailable"))?;
    WindowBuilder::from_config(app, config)?
        .title(&messages().app_name)
        .build()
}

fn attach_native_content(window: &Window, label: &str) -> tauri::Result<()> {
    match label {
        "settings" => native_ui::attach(window),
        "overlay" => overlay::attach(window),
        _ => Err(std::io::Error::other("Unknown native window").into()),
    }
}

fn create_app_window(app: &AppHandle, label: &str) -> tauri::Result<Window> {
    let window = create_native_window(app, label)?;
    let result = if std::thread::current().id() == *app.state::<std::thread::ThreadId>() {
        attach_native_content(&window, label)
    } else {
        let (sender, receiver) = std::sync::mpsc::channel();
        let target = window.clone();
        let label = label.to_owned();
        app.run_on_main_thread(move || {
            let _ = sender.send(attach_native_content(&target, &label));
        })
        .and_then(|()| {
            receiver
                .recv_timeout(std::time::Duration::from_secs(30))
                .map_err(|_| std::io::Error::other("Native window initialization timed out"))?
        })
    };
    if let Err(error) = result {
        // Never leave a partly initialized window registered for the next open request.
        let _ = window.destroy();
        return Err(error);
    }
    Ok(window)
}

fn show_settings_using<R: Runtime>(
    app: &AppHandle<R>,
    create: impl FnOnce() -> tauri::Result<Window<R>>,
) -> tauri::Result<()> {
    let window = match app.get_window("settings") {
        Some(window) => window,
        None => create()?,
    };
    window.unminimize()?;
    window.show()?;
    window.set_focus()
}

fn request_settings(app: &AppHandle) {
    static OPEN_LOCK: Mutex<()> = Mutex::new(());
    let gate = app.state::<Mutex<settings_gate::SettingsGate>>();
    let start = match gate.lock() {
        Ok(mut gate) => gate.request(),
        Err(_) => {
            eprintln!("{}", messages().settings_open_error);
            return;
        }
    };
    if !start {
        return;
    }
    let app = app.clone();
    // Serialize creation on a worker; native control initialization is dispatched to main.
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
    let benchmark = benchmark::BenchmarkConfig::from_environment()?;
    let writer: Box<dyn std::io::Write> = match &benchmark {
        Some(config) => Box::new(
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&config.log_path)?,
        ),
        None => Box::new(std::io::stdout()),
    };
    let is_benchmark = benchmark.is_some();
    let mut ready_signal = benchmark::ReadySignal::new(writer);
    let app = tauri::Builder::default()
        .manage(Mutex::new(settings_gate::SettingsGate::default()))
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if !args.iter().any(|arg| arg == "--autostart") {
                request_settings(app);
            }
        }))
        .setup(move |app| {
            app.manage(std::thread::current().id());
            let store = settings::SettingsStore::open()?;
            let config = store.current().hotkeys()?;
            app.manage(overlay::Controller::open(store.current().overlay)?);
            app.manage(Mutex::new(store));
            let text = messages();
            let settings = MenuItem::with_id(app, "settings", &text.settings, true, None::<&str>)?;
            let reinsert = MenuItem::with_id(app, "reinsert", &text.reinsert, true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", &text.quit, true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&reinsert, &settings, &quit])?;
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
                    "reinsert" => {
                        if let Some(shortcuts) = app.try_state::<input::Shortcuts>() {
                            shortcuts.reinsert_from_tray();
                        } else {
                            dictation::report_error(app, messages().shortcut_error.clone());
                        }
                    }
                    "quit" => app.state::<dictation::Controller>().quit(app, 0),
                    _ => {}
                })
                .build(app)?;
            for label in ["settings", "overlay"] {
                create_app_window(app.handle(), label)?;
            }
            let controller = dictation::Controller::start(app.handle())?;
            let dispatch = controller.input_handler();
            app.manage(controller);
            match input::Shortcuts::start(config, dispatch) {
                Ok(shortcuts) => {
                    app.manage(shortcuts);
                }
                Err(error) if is_benchmark => return Err(error.into()),
                Err(_) => dictation::report_error(app.handle(), messages().shortcut_error.clone()),
            }
            if let Some(config) = benchmark {
                let handle = app.handle().clone();
                benchmark::start_stop_listener(config.stop_event, move |code| {
                    handle.state::<dictation::Controller>().quit(&handle, code)
                })?;
            }
            Ok(())
        })
        .build(tauri::generate_context!())?;

    // Keep the tray process alive while allowing windows and webviews to be destroyed.
    app.run(move |app, event| {
        if matches!(event, RunEvent::Ready) {
            let pending = app
                .state::<Mutex<settings_gate::SettingsGate>>()
                .lock()
                .map(|mut gate| gate.finish_initialization())
                .unwrap_or(false);
            if pending {
                request_settings(app);
            }
            if app.try_state::<input::Shortcuts>().is_some()
                && ready_signal.emit().is_err()
                && is_benchmark
            {
                app.state::<dictation::Controller>().quit(app, 1);
            }
        }
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
