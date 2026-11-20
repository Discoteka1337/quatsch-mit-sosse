use crate::messages;
use crate::overlay_placement::{Bounds, OverlayPreferences, OverlayStore};
use crate::overlay_window::{self, Callbacks, View};
use std::{io, sync::Mutex};
use tauri::{AppHandle, Manager, Window};
use windows::Win32::{
    Foundation::{HWND, RECT},
    UI::WindowsAndMessaging::{GetWindowRect, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos},
};

/// Native presentation input for the dictation pipeline. The level is a percentage.
/// On error, pass a localized explanation and the text available for recovery.
#[derive(Default)]
pub enum OverlayState {
    #[default]
    Ready,
    Listening {
        level: u8,
    },
    Processing,
    Inserting,
    Error {
        message: String,
        text: String,
    },
    Administrator,
}

impl OverlayState {
    fn view(&self, show_ready: bool, copy_failed: bool) -> View {
        let msg = messages();
        match self {
            Self::Ready if show_ready => View::Ready,
            Self::Ready => View::Hidden,
            Self::Listening { level } => View::Listening {
                label: msg.overlay_listening.clone(),
                level: (*level).min(100),
            },
            Self::Processing => View::Status(msg.overlay_processing.clone()),
            Self::Inserting => View::Status(msg.overlay_inserting.clone()),
            Self::Error { message, .. } => View::Error {
                label: if copy_failed {
                    &msg.overlay_copy_error
                } else if message.is_empty() {
                    &msg.overlay_error
                } else {
                    message
                }
                .clone(),
                copy: msg.overlay_copy.clone(),
            },
            Self::Administrator => View::Administrator {
                label: msg.overlay_administrator.clone(),
                detail: msg.overlay_administrator_help.clone(),
            },
        }
    }

    fn copy_text(&self) -> Option<&str> {
        match self {
            Self::Error { text, message } if text.is_empty() => Some(if message.is_empty() {
                &messages().overlay_error
            } else {
                message
            }),
            Self::Error { text, .. } => Some(text),
            _ => None,
        }
    }
}

struct Model {
    store: OverlayStore,
    state: OverlayState,
    copy_failed: bool,
}

impl Model {
    fn view(&self) -> View {
        self.state
            .view(self.store.preferences().show_ready, self.copy_failed)
    }
}

pub(super) struct Controller(Mutex<Model>);

impl Controller {
    pub(super) fn open() -> io::Result<Self> {
        Ok(Self(Mutex::new(Model {
            store: OverlayStore::open()?,
            state: OverlayState::Ready,
            copy_failed: false,
        })))
    }
}

fn model<T>(app: &AppHandle, f: impl FnOnce(&mut Model) -> io::Result<T>) -> io::Result<T> {
    let controller = app.state::<Controller>();
    let mut guard = controller
        .0
        .lock()
        .map_err(|_| io::Error::other("Overlay unavailable"))?;
    f(&mut guard)
}

fn hwnd(app: &AppHandle) -> tauri::Result<HWND> {
    app.get_window("overlay")
        .ok_or_else(|| io::Error::other("Overlay window unavailable"))?
        .hwnd()
}

fn present(app: &AppHandle, hwnd: HWND, view: View, place: bool) -> tauri::Result<()> {
    if place {
        let (width, height) = view.logical_size();
        let b = model(app, |m| m.store.placement(width, height))?;
        unsafe {
            SetWindowPos(
                hwnd,
                None,
                b.x,
                b.y,
                b.width,
                b.height,
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
        }
        .map_err(io::Error::other)?;
    }
    overlay_window::render(hwnd, view).map_err(io::Error::other)?;
    Ok(())
}

fn refresh_location(app: &AppHandle) -> tauri::Result<()> {
    present(app, hwnd(app)?, model(app, |m| Ok(m.view()))?, true)
}

fn remember_location(app: &AppHandle) -> tauri::Result<()> {
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd(app)?, &mut rect) }.map_err(io::Error::other)?;
    model(app, |m| {
        m.store.remember(Bounds {
            x: rect.left,
            y: rect.top,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
        })
    })?;
    refresh_location(app)
}

pub(super) fn preferences(app: &AppHandle) -> io::Result<OverlayPreferences> {
    model(app, |m| Ok(m.store.preferences()))
}

pub(super) fn toggle_preference(app: &AppHandle, ready: bool) -> tauri::Result<bool> {
    let hwnd = hwnd(app)?;
    let old = preferences(app)?;
    let mut new = old;
    let checked = if ready {
        &mut new.show_ready
    } else {
        &mut new.hide_from_capture
    };
    *checked = !*checked;
    let checked = *checked;

    let apply = |options: OverlayPreferences| -> tauri::Result<()> {
        if ready {
            let view = model(app, |m| Ok(m.state.view(options.show_ready, m.copy_failed)))?;
            present(app, hwnd, view, false)
        } else {
            overlay_window::set_capture_hidden(hwnd, options.hide_from_capture)
                .map_err(io::Error::other)?;
            Ok(())
        }
    };
    apply(new)?;
    if let Err(error) = model(app, |m| m.store.set_preferences(new)) {
        apply(old)?;
        return Err(error.into());
    }
    Ok(checked)
}

fn update(app: &AppHandle, next: OverlayState) -> tauri::Result<()> {
    let (view, resize) = model(app, |m| {
        let view = next.view(m.store.preferences().show_ready, false);
        let resize = m.view().logical_size() != view.logical_size();
        Ok((view, resize))
    })?;
    present(app, hwnd(app)?, view, resize)?;
    model(app, |m| {
        m.state = next;
        m.copy_failed = false;
        Ok(())
    })?;
    Ok(())
}

/// Applies a state on the window thread, including when called by a dictation worker.
pub fn set_overlay_state(app: &AppHandle, next: OverlayState) -> tauri::Result<()> {
    if std::thread::current().id() == *app.state::<std::thread::ThreadId>() {
        return update(app, next);
    }
    let (sender, receiver) = std::sync::mpsc::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let _ = sender.send(update(&handle, next));
    })?;
    receiver
        .recv()
        .map_err(|_| io::Error::other("Overlay window thread stopped"))?
}

fn copy(app: &AppHandle) -> tauri::Result<()> {
    let Some(text) = model(app, |m| Ok(m.state.copy_text().map(str::to_owned)))? else {
        return Ok(());
    };
    let hwnd = hwnd(app)?;
    if quatsch_mit_sosse_inject::copy_recovered_text(hwnd, &text).is_ok() {
        return update(app, OverlayState::Ready);
    }
    let view = model(app, |m| {
        m.copy_failed = true;
        Ok(m.view())
    })?;
    present(app, hwnd, view, false)
}

pub(super) fn attach(window: &Window) -> tauri::Result<()> {
    let app = window.app_handle();
    let hwnd = window.hwnd()?;
    let callback = |action: fn(&AppHandle) -> tauri::Result<()>| -> Box<dyn Fn()> {
        let app = app.clone();
        Box::new(move || {
            if action(&app).is_err() {
                eprintln!("{}", messages().overlay_preferences_error);
            }
        })
    };
    overlay_window::attach(
        hwnd,
        Callbacks {
            copied: callback(copy),
            dragged: callback(remember_location),
            display_changed: callback(refresh_location),
        },
    )
    .map_err(io::Error::other)?;
    if preferences(app)?.hide_from_capture {
        overlay_window::set_capture_hidden(hwnd, true).map_err(io::Error::other)?;
    }
    refresh_location(app)
}

#[cfg(test)]
mod tests;
