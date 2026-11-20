use crate::speech::{DEFAULT_MODEL, FALLBACK_MODEL, VAD_MODEL, installed_asr};
use quatsch_mit_sosse_models::{Manager as Models, Model, Progress, ProgressPhase};
use serde::Deserialize;
use std::{
    cell::RefCell,
    io,
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicBool, AtomicIsize, Ordering},
    },
    thread,
};
use tauri::AppHandle;
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, S_FALSE, S_OK, WPARAM},
        UI::{
            Controls::{
                TASKDIALOG_BUTTON, TASKDIALOG_MESSAGES, TASKDIALOG_NOTIFICATIONS, TASKDIALOGCONFIG,
                TDE_CONTENT, TDF_ALLOW_DIALOG_CANCELLATION, TDF_CALLBACK_TIMER,
                TDF_SHOW_PROGRESS_BAR, TDM_ENABLE_BUTTON, TDM_SET_ELEMENT_TEXT,
                TDM_SET_PROGRESS_BAR_POS, TDN_BUTTON_CLICKED, TDN_CREATED, TDN_DESTROYED,
                TDN_TIMER, TaskDialogIndirect,
            },
            WindowsAndMessaging::{IDCANCEL, SendMessageW, SetForegroundWindow},
        },
    },
    core::{HRESULT, HSTRING, PCWSTR},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Labels {
    model_title: String,
    model_credits: String,
    model_hide_credits: String,
    model_missing: String,
    model_download: String,
    model_close: String,
    model_starting: String,
    model_downloading: String,
    model_verifying: String,
    model_extracting: String,
    model_ready: String,
    model_window_error: String,
    model_worker_error: String,
}

static LABELS: LazyLock<Labels> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../ui/src/locales/de.json"))
        .expect("The embedded model manager labels must be valid")
});

fn required_ids(
    preferred: Option<&str>,
    mut installed: impl FnMut(&str) -> Result<bool, String>,
) -> Result<Vec<&str>, String> {
    let mut ids = Vec::new();
    let requested = preferred.unwrap_or(DEFAULT_MODEL);
    if !installed(requested)? && (preferred.is_some() || !installed(FALLBACK_MODEL)?) {
        ids.push(requested);
    }
    if !installed(VAD_MODEL)? {
        ids.push(VAD_MODEL);
    }
    Ok(ids)
}

fn missing_models<'a>(
    manager: &'a Models,
    preferred: Option<&str>,
) -> Result<Vec<&'a Model>, String> {
    required_ids(preferred, |id| {
        manager.is_installed(id).map_err(|e| e.to_string())
    })?
    .into_iter()
    .map(|id| {
        manager
            .list()
            .iter()
            .find(|m| m.id == id)
            .ok_or_else(|| quatsch_mit_sosse_models::Error::UnknownModel.to_string())
    })
    .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Prompt,
    Installing,
    Ready,
    Failed,
}

#[derive(Clone, PartialEq, Eq)]
struct View {
    stage: Stage,
    text: String,
    percent: u8,
}

impl View {
    fn prompt(text: String) -> Self {
        Self {
            stage: Stage::Prompt,
            text,
            percent: 0,
        }
    }

    fn ready() -> Self {
        Self {
            stage: Stage::Ready,
            text: LABELS.model_ready.clone(),
            percent: 100,
        }
    }

    fn failure(text: String) -> Self {
        Self {
            stage: Stage::Failed,
            text,
            percent: 0,
        }
    }

    fn can_install(&self) -> bool {
        matches!(self.stage, Stage::Prompt | Stage::Failed)
    }
}

fn inspect(manager: &Models, preferred: Option<&str>) -> Result<View, String> {
    let models = missing_models(manager, preferred)?;
    if models.is_empty() {
        return Ok(View::ready());
    }
    let names = models
        .iter()
        .map(|m| m.display_name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let bytes = models.iter().map(|m| m.download_size()).sum::<u64>();
    Ok(View::prompt(
        LABELS
            .model_missing
            .replace("{models}", &names)
            .replace("{size}", &bytes.div_ceil(1_000_000).to_string()),
    ))
}

fn credits(manager: &Models, asr: &str) -> String {
    manager
        .list()
        .iter()
        .filter(|model| model.id == asr || model.id == VAD_MODEL)
        .map(|model| {
            format!(
                "{} · {}\n{}",
                model.display_name, model.license, model.attribution
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn progress_view(name: &str, event: &Progress, previous: u64, total: u64) -> View {
    let percent = ((previous + event.downloaded_bytes) * 100 / total.max(1)).min(100) as u8;
    let phase = match event.phase {
        ProgressPhase::Downloading => &LABELS.model_downloading,
        ProgressPhase::Verifying => &LABELS.model_verifying,
        ProgressPhase::Extracting | ProgressPhase::Complete => &LABELS.model_extracting,
    };
    View {
        stage: Stage::Installing,
        text: format!("{name} · {phase} · {percent} %"),
        percent,
    }
}

struct Shared {
    view: Mutex<View>,
    hwnd: AtomicIsize,
    cancel: AtomicBool,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            view: Mutex::new(View::prompt(String::new())),
            hwnd: AtomicIsize::new(0),
            cancel: AtomicBool::new(false),
        }
    }
}

impl Shared {
    fn set(&self, view: View) {
        *self.view.lock().unwrap_or_else(|e| e.into_inner()) = view;
    }

    fn snapshot(&self) -> View {
        self.view.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn begin(&self) -> bool {
        let mut view = self.view.lock().unwrap_or_else(|e| e.into_inner());
        if !view.can_install() {
            return false;
        }
        self.cancel.store(false, Ordering::Release);
        *view = View {
            stage: Stage::Installing,
            text: LABELS.model_starting.clone(),
            percent: 0,
        };
        true
    }
}

// A finishing download belongs to its original selection, even if the
// settings window has since opened a different model.
static DIALOGS: Mutex<Vec<(Option<String>, Arc<Shared>)>> = Mutex::new(Vec::new());
const DOWNLOAD: i32 = 101;

fn failure(shared: &Shared, app: &AppHandle, message: String) {
    shared.set(View::failure(message.clone()));
    if !shared.cancel.load(Ordering::Acquire) {
        crate::dictation::report_error(app, message);
    }
}

fn install(shared: &Shared, preferred: Option<&str>) -> Result<View, String> {
    let manager = Models::new().map_err(|e| e.to_string())?;
    let models = missing_models(&manager, preferred)?;
    let total = models.iter().map(|m| m.download_size()).sum();
    let mut previous = 0;
    for model in models {
        let outcome = manager.install(&model.id, &shared.cancel, |event| {
            shared.set(progress_view(&model.display_name, &event, previous, total));
        });
        if matches!(outcome, Err(quatsch_mit_sosse_models::Error::Cancelled)) {
            return inspect(&manager, preferred);
        }
        outcome.map_err(|e| e.to_string())?;
        previous += model.download_size();
    }
    inspect(&manager, preferred)
}

fn start_download(shared: &Arc<Shared>, app: &AppHandle, preferred: Option<&str>) {
    if shared.begin() {
        let handle = app.clone();
        let session = shared.clone();
        let preferred = preferred.map(str::to_owned);
        if thread::Builder::new()
            .name("model-download".into())
            .spawn(move || match install(&session, preferred.as_deref()) {
                Ok(view) => session.set(view),
                Err(message) => failure(&session, &handle, message),
            })
            .is_err()
        {
            failure(shared, app, LABELS.model_worker_error.clone());
        }
    }
}

struct Dialog<'a> {
    shared: &'a Shared,
    download: &'a dyn Fn(),
    painted: RefCell<Option<View>>,
}

fn send(hwnd: HWND, msg: TASKDIALOG_MESSAGES, value: usize, parameter: isize) {
    unsafe {
        SendMessageW(
            hwnd,
            msg.0 as u32,
            Some(WPARAM(value)),
            Some(LPARAM(parameter)),
        );
    }
}

impl Dialog<'_> {
    fn render(&self, hwnd: HWND) {
        let view = self.shared.snapshot();
        if self.painted.borrow().as_ref() == Some(&view) {
            return;
        }
        let content = HSTRING::from(&view.text);
        let enabled = isize::from(view.can_install());
        let percent = usize::from(view.percent);
        self.painted.replace(Some(view));
        send(hwnd, TDM_ENABLE_BUTTON, DOWNLOAD as usize, enabled);
        send(
            hwnd,
            TDM_SET_ELEMENT_TEXT,
            TDE_CONTENT.0 as usize,
            content.as_ptr() as isize,
        );
        send(hwnd, TDM_SET_PROGRESS_BAR_POS, percent, 0);
    }
}

unsafe extern "system" fn callback(
    hwnd: HWND,
    msg: TASKDIALOG_NOTIFICATIONS,
    wp: WPARAM,
    _: LPARAM,
    data: isize,
) -> HRESULT {
    // TaskDialogIndirect invokes this callback on its own thread before returning.
    let dialog = unsafe { &*(data as *const Dialog<'_>) };
    match msg {
        TDN_CREATED => {
            dialog.render(hwnd);
            dialog.shared.hwnd.store(hwnd.0 as isize, Ordering::Release);
        }
        TDN_TIMER => dialog.render(hwnd),
        TDN_BUTTON_CLICKED if wp.0 == DOWNLOAD as usize => {
            (dialog.download)();
            dialog.render(hwnd);
            return S_FALSE;
        }
        TDN_DESTROYED => {
            dialog.shared.hwnd.store(0, Ordering::Release);
            dialog.shared.cancel.store(true, Ordering::Release);
        }
        _ => {}
    }
    S_OK
}

fn run_dialog(
    shared: &Arc<Shared>,
    app: &AppHandle,
    preferred: Option<&str>,
) -> windows::core::Result<()> {
    let information = Models::new()
        .map_err(|e| e.to_string())
        .and_then(|manager| {
            let asr = preferred
                .or(installed_asr(&manager)?)
                .unwrap_or(DEFAULT_MODEL);
            shared.set(inspect(&manager, preferred)?);
            Ok(credits(&manager, asr))
        });
    let notes = information.unwrap_or_else(|message| {
        failure(shared, app, message);
        String::new()
    });
    native_dialog(shared, &notes, &|| start_download(shared, app, preferred))
}

fn native_dialog(shared: &Shared, notes: &str, download: &dyn Fn()) -> windows::core::Result<()> {
    let title = HSTRING::from(&crate::messages().app_name);
    let heading = HSTRING::from(&LABELS.model_title);
    let content = HSTRING::from(shared.snapshot().text);
    let action = HSTRING::from(&LABELS.model_download);
    let close = HSTRING::from(&LABELS.model_close);
    let credits = HSTRING::from(notes);
    let expand = HSTRING::from(&LABELS.model_credits);
    let collapse = HSTRING::from(&LABELS.model_hide_credits);
    let buttons = [
        TASKDIALOG_BUTTON {
            nButtonID: DOWNLOAD,
            pszButtonText: PCWSTR(action.as_ptr()),
        },
        TASKDIALOG_BUTTON {
            nButtonID: IDCANCEL.0,
            pszButtonText: PCWSTR(close.as_ptr()),
        },
    ];
    let dialog = Dialog {
        shared,
        download,
        painted: RefCell::default(),
    };
    let flags = TDF_ALLOW_DIALOG_CANCELLATION | TDF_SHOW_PROGRESS_BAR | TDF_CALLBACK_TIMER;
    #[cfg(test)]
    let flags = flags | windows::Win32::UI::Controls::TDF_NO_SET_FOREGROUND;
    let config = TASKDIALOGCONFIG {
        cbSize: size_of::<TASKDIALOGCONFIG>() as u32,
        dwFlags: flags,
        pszWindowTitle: PCWSTR(title.as_ptr()),
        pszMainInstruction: PCWSTR(heading.as_ptr()),
        pszContent: PCWSTR(content.as_ptr()),
        pszExpandedInformation: if notes.is_empty() {
            PCWSTR::null()
        } else {
            PCWSTR(credits.as_ptr())
        },
        pszCollapsedControlText: PCWSTR(expand.as_ptr()),
        pszExpandedControlText: PCWSTR(collapse.as_ptr()),
        cButtons: buttons.len() as u32,
        pButtons: buttons.as_ptr(),
        nDefaultButton: IDCANCEL.0,
        pfCallback: Some(callback),
        lpCallbackData: (&dialog as *const Dialog<'_>) as isize,
        cxWidth: 250,
        ..Default::default()
    };
    unsafe { TaskDialogIndirect(&config, None, None, None) }
}

pub(super) fn show(app: &AppHandle) -> tauri::Result<()> {
    let preferred = crate::current_settings(app)?.model;
    let mut dialogs = DIALOGS.lock().unwrap_or_else(|error| error.into_inner());
    if let Some((_, session)) = dialogs.iter().find(|(key, _)| *key == preferred) {
        let hwnd = session.hwnd.load(Ordering::Acquire);
        drop(dialogs);
        if hwnd != 0 {
            unsafe {
                let _ = SetForegroundWindow(HWND(hwnd as _));
            }
        }
        return Ok(());
    }
    let shared = Arc::new(Shared::default());
    dialogs.push((preferred.clone(), shared.clone()));
    drop(dialogs);
    let app = app.clone();
    let session = shared.clone();
    thread::Builder::new()
        .name("model-dialog".into())
        .spawn(move || {
            if run_dialog(&session, &app, preferred.as_deref()).is_err() {
                crate::dictation::report_error(&app, LABELS.model_window_error.clone());
            }
            forget_dialog(&session);
        })
        .map_err(|_| {
            forget_dialog(&shared);
            io::Error::other(&LABELS.model_window_error[..])
        })?;
    Ok(())
}

fn forget_dialog(shared: &Arc<Shared>) {
    DIALOGS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .retain(|(_, entry)| !Arc::ptr_eq(entry, shared));
}

#[cfg(test)]
mod tests;
