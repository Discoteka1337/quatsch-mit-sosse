use crate::{
    dictation, input, messages, overlay,
    settings::{Settings, SettingsStore},
};
use quatsch_mit_sosse_hotkey::HotkeyConfig;
use std::{io, sync::Mutex};
use tauri::{AppHandle, Manager};

pub(super) fn current_settings(app: &AppHandle) -> io::Result<Settings> {
    let store = app.state::<Mutex<SettingsStore>>();
    Ok(store
        .lock()
        .map_err(|_| io::Error::other(&messages().settings_open_error[..]))?
        .current()
        .clone())
}

pub(super) fn capture_shortcuts(app: &AppHandle, active: bool) {
    if let Some(shortcuts) = app.try_state::<input::Shortcuts>()
        && shortcuts.capture(active).is_err()
    {
        dictation::report_error(app, messages().shortcut_error.clone());
    }
}

fn configure_shortcuts(app: &AppHandle, keys: HotkeyConfig) -> io::Result<()> {
    if let Some(shortcuts) = app.try_state::<input::Shortcuts>() {
        shortcuts.configure(keys)
    } else {
        let dispatch = app.state::<dictation::Controller>().input_handler();
        app.manage(input::Shortcuts::start(keys, dispatch)?);
        Ok(())
    }
}

// Only the settings window thread writes preferences. Keep native calls outside
// the store lock; window messages can re-enter application code.
pub(super) fn apply_settings(
    app: &AppHandle,
    change: impl FnOnce(&mut Settings),
) -> io::Result<()> {
    let previous = current_settings(app)?;
    let mut next = previous.clone();
    change(&mut next);
    if next == previous {
        return Ok(());
    }
    let keys = next.hotkeys()?;
    let old_keys = previous.hotkeys()?;
    let overlay_changed = next.overlay != previous.overlay;
    if overlay_changed {
        overlay::apply_preferences(app, next.overlay).map_err(io::Error::other)?;
    }
    let mut configured = false;
    let result = (|| {
        if keys != old_keys {
            configure_shortcuts(app, keys)?;
            configured = true;
        }
        let store = app.state::<Mutex<SettingsStore>>();
        store
            .lock()
            .map_err(|_| io::Error::other(&messages().settings_open_error[..]))?
            .save(next)
    })();
    if result.is_err() {
        if configured {
            configure_shortcuts(app, old_keys)?;
        }
        if overlay_changed {
            overlay::apply_preferences(app, previous.overlay).map_err(io::Error::other)?;
        }
    }
    result
}
