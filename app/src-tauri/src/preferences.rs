use crate::{
    dictation, input, messages, overlay,
    settings::{Settings, SettingsStore},
};
use quatsch_mit_sosse_hotkey::HotkeyConfig;
use std::{
    collections::VecDeque,
    io,
    sync::{Arc, Mutex, mpsc},
    thread,
};
use tauri::{AppHandle, Manager};

#[cfg(test)]
mod tests;

pub(super) fn current_settings(app: &AppHandle) -> io::Result<Settings> {
    Ok(app.state::<Writer>().committed())
}

pub(super) fn display_settings(app: &AppHandle) -> io::Result<Settings> {
    Ok(app.state::<Writer>().display())
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

type Edit = Arc<dyn Fn(&mut Settings) + Send + Sync>;
type Job = Box<dyn FnOnce(&mut SettingsStore) + Send>;

struct Snapshot {
    committed: Settings,
    pending: VecDeque<Edit>,
    closing: bool,
}

impl Snapshot {
    fn display(&self) -> Settings {
        let mut next = self.committed.clone();
        for change in &self.pending {
            change(&mut next);
        }
        next
    }
}

pub(super) struct Writer {
    snapshot: Arc<Mutex<Snapshot>>,
    tx: mpsc::Sender<Job>,
}

impl Writer {
    pub(super) fn start(mut store: SettingsStore) -> io::Result<Self> {
        let snapshot = Arc::new(Mutex::new(Snapshot {
            committed: store.current().clone(),
            pending: VecDeque::new(),
            closing: false,
        }));
        let (tx, rx) = mpsc::channel::<Job>();
        thread::Builder::new()
            .name("qms-settings".into())
            .spawn(move || {
                for job in rx {
                    job(&mut store);
                }
            })?;
        Ok(Self { snapshot, tx })
    }

    fn committed(&self) -> Settings {
        self.snapshot.lock().unwrap().committed.clone()
    }

    fn display(&self) -> Settings {
        self.snapshot.lock().unwrap().display()
    }

    fn edit(
        &self,
        change: impl Fn(&mut Settings) + Send + Sync + 'static,
        persist: impl FnOnce(&mut SettingsStore, Settings) -> io::Result<()> + Send + 'static,
        notify: impl FnOnce(io::Result<()>) + Send + 'static,
    ) -> io::Result<()> {
        let change: Edit = Arc::new(change);
        let mut state = self.snapshot.lock().unwrap();
        if state.closing {
            return Err(io::Error::other("Die App wird bereits beendet."));
        }
        let mut preview = state.display();
        change(&mut preview);
        preview.validate()?;
        state.pending.push_back(change.clone());
        let snapshot = self.snapshot.clone();
        let job = move |store: &mut SettingsStore| {
            // Reapply this field to the last successful write. A failed edit
            // must not reappear when a later, independent edit succeeds.
            let mut next = store.current().clone();
            change(&mut next);
            let result = persist(store, next);
            {
                let mut state = snapshot.lock().unwrap();
                state.committed = store.current().clone();
                state.pending.pop_front();
            }
            notify(result);
        };
        if self.tx.send(Box::new(job)).is_err() {
            state.pending.pop_back();
            return Err(io::Error::other(&messages().worker_error[..]));
        }
        Ok(())
    }

    fn stop_accepting(&self) {
        self.snapshot.lock().unwrap().closing = true;
    }

    fn finish(&self, done: impl FnOnce() + Send + 'static) -> io::Result<()> {
        self.tx
            .send(Box::new(move |_| done()))
            .map_err(|_| io::Error::other(&messages().worker_error[..]))
    }

    fn repair_login(
        &self,
        repair: impl FnOnce(bool) -> io::Result<()> + Send + 'static,
        failed: impl FnOnce() + Send + 'static,
    ) -> io::Result<()> {
        self.tx
            .send(Box::new(move |store| {
                if repair(store.current().autostart).is_err() {
                    failed();
                }
            }))
            .map_err(|_| io::Error::other(&messages().worker_error[..]))
    }
}

pub(super) fn repair_login(app: &AppHandle) -> io::Result<()> {
    let handle = app.clone();
    app.state::<Writer>()
        .repair_login(crate::settings::autostart::repair, move || {
            dictation::report_error(
                &handle,
                "Der Windows-Autostart konnte nicht abgeglichen werden.".into(),
            );
        })
}

pub(super) fn quit(app: &AppHandle, code: i32) {
    // Release the microphone immediately, even if a filesystem write is blocked.
    app.state::<Writer>().stop_accepting();
    app.state::<dictation::Controller>().quit(app, code);
}

pub(super) fn exit_when_saved(app: &AppHandle, code: i32) {
    let handle = app.clone();
    if app
        .state::<Writer>()
        .finish(move || handle.exit(code))
        .is_err()
    {
        app.exit(code);
    }
}

pub(super) fn apply_settings(
    app: &AppHandle,
    change: impl Fn(&mut Settings) + Send + Sync + 'static,
) -> io::Result<()> {
    let handle = app.clone();
    let notify = app.clone();
    app.state::<Writer>().edit(
        change,
        move |store, next| apply(&handle, store, next),
        move |result| {
            if let Err(error) = result {
                let handle = notify.clone();
                let _ = notify.run_on_main_thread(move || {
                    crate::native_ui::preferences_failed(&handle, error.to_string());
                });
            }
        },
    )
}

fn overlay_preferences(
    app: &AppHandle,
    preferences: crate::overlay_placement::OverlayPreferences,
) -> io::Result<()> {
    let (tx, rx) = mpsc::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let _ = tx.send(overlay::apply_preferences(&handle, preferences).map_err(io::Error::other));
    })
    .map_err(io::Error::other)?;
    rx.recv().map_err(io::Error::other)?
}

fn apply(app: &AppHandle, store: &mut SettingsStore, next: Settings) -> io::Result<()> {
    let previous = store.current().clone();
    if next == previous {
        return Ok(());
    }
    next.validate()?;
    let keys = next.hotkeys()?;
    let old_keys = previous.hotkeys()?;
    let overlay_changed = next.overlay != previous.overlay;
    if overlay_changed {
        overlay_preferences(app, next.overlay)?;
    }
    let mut configured = false;
    let result = (|| {
        if keys != old_keys {
            configure_shortcuts(app, keys)?;
            configured = true;
        }
        store.save(next)
    })();
    if result.is_err() {
        if configured {
            configure_shortcuts(app, old_keys)?;
        }
        if overlay_changed {
            overlay_preferences(app, previous.overlay)?;
        }
    }
    result
}
