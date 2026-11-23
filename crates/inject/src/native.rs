use crate::{InjectConfig, InjectError, InjectMethod, clipboard, keyboard, route, uia};
use std::cell::Cell;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::Path;
use std::sync::{Mutex, mpsc};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, SMTO_ABORTIFHUNG, SMTO_BLOCK,
    SendMessageTimeoutW, SetForegroundWindow, WM_NULL,
};
use windows::core::{Owned, PWSTR};

static INSERTION: Mutex<()> = Mutex::new(());

/// An opaque window identity that can travel from the hotkey to the recognition worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InsertTarget {
    window_id: usize,
    thread_id: u32,
    process_id: u32,
}

impl InsertTarget {
    pub fn capture() -> Result<Self, InjectError> {
        Self::from_window(unsafe { GetForegroundWindow() })
    }

    pub fn from_window(hwnd: HWND) -> Result<Self, InjectError> {
        let mut process_id = 0;
        let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        if hwnd.is_invalid() || thread_id == 0 || process_id == 0 {
            return Err(InjectError::TargetUnavailable);
        }
        Ok(Self {
            window_id: hwnd.0 as usize,
            thread_id,
            process_id,
        })
    }

    pub(crate) fn hwnd(self) -> HWND {
        HWND(self.window_id as _)
    }

    fn verify_window(self) -> Result<(), InjectError> {
        if Self::from_window(self.hwnd()) == Ok(self) {
            Ok(())
        } else {
            Err(InjectError::TargetChanged)
        }
    }

    /// Restore the captured editor only while the initiating menu or review owns the foreground.
    pub fn restore_after_menu(self, menu_owner: HWND) -> Result<(), InjectError> {
        if self.check_foreground().is_err() {
            self.verify_window()?;
            if menu_owner.is_invalid() || unsafe { GetForegroundWindow() } != menu_owner {
                return Err(InjectError::TargetChanged);
            }
            let accepted = unsafe { SetForegroundWindow(self.hwnd()).as_bool() };
            return confirm_activation(
                accepted,
                || self.check_foreground(),
                || unsafe {
                    // Activation of a different input queue can finish after SetForegroundWindow
                    // returns. A bounded, synchronous no-op lets that queue process the request.
                    SendMessageTimeoutW(
                        self.hwnd(),
                        WM_NULL,
                        WPARAM(0),
                        LPARAM(0),
                        SMTO_ABORTIFHUNG | SMTO_BLOCK,
                        180,
                        None,
                    );
                },
            );
        }
        self.check_foreground()
    }

    pub fn check_foreground(self) -> Result<(), InjectError> {
        let active = Self::capture().map_err(|_| InjectError::TargetChanged)?;
        if active == self {
            Ok(())
        } else {
            Err(InjectError::TargetChanged)
        }
    }

    fn deliver(
        &self,
        text: &str,
        config: &InjectConfig,
        cursor: Option<usize>,
        on_restore_delay: &dyn Fn(),
    ) -> Result<InjectMethod, InjectError> {
        self.check_foreground()?;
        route(
            text,
            config,
            || process_name(self.process_id),
            |method| {
                self.check_foreground()?;
                match method {
                    InjectMethod::UiAutomation => uia::insert(self, text, cursor),
                    InjectMethod::Unicode => uia::with_caret(self, text, cursor, || {
                        keyboard::insert(self, text).map(|()| true)
                    }),
                    InjectMethod::Clipboard => uia::with_caret(self, text, cursor, || {
                        // A delayed restoration may finish long after the user resumes editing.
                        let prompt = Cell::new(true);
                        clipboard::paste(self, text, &|| {
                            prompt.set(false);
                            on_restore_delay();
                        })?;
                        Ok(prompt.get())
                    }),
                }
            },
        )
    }

    /// Run the insertion on a dedicated COM thread bound to the captured window.
    pub fn insert(self, text: &str, config: &InjectConfig) -> Result<InjectMethod, InjectError> {
        let _guard = INSERTION.try_lock().map_err(|_| InjectError::Busy)?;
        std::thread::scope(|scope| {
            let worker = std::thread::Builder::new()
                .name("qms-inject".into())
                .spawn_scoped(scope, move || self.deliver(text, config, None, &|| {}))
                .map_err(|_| InjectError::Unavailable)?;
            worker.join().unwrap_or(Err(InjectError::DeliveryUncertain))
        })
    }

    /// Return to the desktop if another process keeps the clipboard locked after
    /// delivery. The worker retains the snapshot and reports later errors.
    pub fn insert_with_background_restore(
        self,
        text: String,
        config: InjectConfig,
        on_late_error: impl FnOnce(InjectError) + Send + 'static,
    ) -> Result<InjectMethod, InjectError> {
        self.insert_with_cursor(text, config, None, on_late_error)
    }

    /// Insert once and request a caret at the UTF-8 byte offset if the target supports it.
    pub fn insert_with_cursor(
        self,
        text: String,
        config: InjectConfig,
        cursor: Option<usize>,
        on_late_error: impl FnOnce(InjectError) + Send + 'static,
    ) -> Result<InjectMethod, InjectError> {
        let _guard = INSERTION.try_lock().map_err(|_| InjectError::Busy)?;
        with_background_restore(
            move |pending| self.deliver(&text, &config, cursor, pending),
            on_late_error,
        )
    }
}

fn confirm_activation(
    accepted: bool,
    foreground: impl Fn() -> Result<(), InjectError>,
    synchronize: impl FnOnce(),
) -> Result<(), InjectError> {
    let immediate = foreground();
    if !accepted || immediate.is_ok() {
        return immediate;
    }
    synchronize();
    foreground()
}

pub(super) fn with_background_restore(
    deliver: impl FnOnce(&dyn Fn()) -> Result<InjectMethod, InjectError> + Send + 'static,
    on_late_error: impl FnOnce(InjectError) + Send + 'static,
) -> Result<InjectMethod, InjectError> {
    let (reply, result) = mpsc::channel();
    std::thread::Builder::new()
        .name("qms-inject".into())
        .spawn(move || {
            let pending = Cell::new(false);
            let outcome = deliver(&|| {
                if !pending.replace(true) {
                    let _ = reply.send(Err(InjectError::ClipboardRecoveryPending));
                }
            });
            if pending.get() {
                if let Err(error) = outcome {
                    on_late_error(error);
                }
            } else {
                let _ = reply.send(outcome);
            }
        })
        .map_err(|_| InjectError::Unavailable)?;
    result.recv().unwrap_or(Err(InjectError::DeliveryUncertain))
}

fn process_name(process_id: u32) -> Result<String, InjectError> {
    let process = unsafe {
        Owned::new(
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id)
                .map_err(|_| InjectError::TargetUnavailable)?,
        )
    };
    let mut path = vec![0u16; 32768];
    let mut len = path.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            *process,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut len,
        )
    }
    .map_err(|_| InjectError::TargetUnavailable)?;
    let path = OsString::from_wide(&path[..len as usize]);
    Path::new(&path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or(InjectError::TargetUnavailable)
}

/// Capture the foreground target at call time. Call from a background task:
/// this synchronous operation waits for the target and any clipboard restoration.
pub fn insert(text: &str, config: &InjectConfig) -> Result<InjectMethod, InjectError> {
    InsertTarget::capture()?.insert(text, config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, GetWindowTextW};
    use windows::core::w;

    #[test]
    fn waits_for_the_target_queue_only_after_an_accepted_deferred_activation() {
        for (accepted, immediate, final_match, expected_wait, confirmed) in [
            (false, false, false, false, false),
            (false, true, true, false, true),
            (true, true, true, false, true),
            (true, false, true, true, true),
            (true, false, false, true, false),
        ] {
            let waited = Cell::new(false);
            let outcome = confirm_activation(
                accepted,
                || {
                    (immediate || waited.get() && final_match)
                        .then_some(())
                        .ok_or(InjectError::TargetChanged)
                },
                || waited.set(true),
            );
            assert_eq!(waited.get(), expected_wait);
            assert_eq!(outcome.is_ok(), confirmed);
        }
    }

    #[test]
    fn retained_target_rejects_a_different_owner_or_a_closed_window() {
        let hwnd = unsafe {
            CreateWindowExW(
                Default::default(),
                w!("EDIT"),
                w!("Prüffeld"),
                Default::default(),
                0,
                0,
                1,
                1,
                None,
                None,
                None,
                None,
            )
        }
        .unwrap();
        let target = InsertTarget::from_window(hwnd).unwrap();
        assert_eq!(target.verify_window(), Ok(()));
        let other = InsertTarget {
            process_id: target.process_id + 1,
            ..target
        };
        assert_eq!(other.verify_window(), Err(InjectError::TargetChanged));
        assert_eq!(target.check_foreground(), Err(InjectError::TargetChanged));
        let mut config = InjectConfig::default();
        config.apps.insert(
            process_name(std::process::id()).unwrap().to_lowercase(),
            InjectMethod::UiAutomation,
        );
        assert_eq!(
            target.insert("Probe", &config),
            Err(InjectError::TargetChanged)
        );
        let mut buffer = [0; 32];
        let count = unsafe { GetWindowTextW(hwnd, &mut buffer) };
        assert_eq!(
            String::from_utf16(&buffer[..count as usize]).unwrap(),
            "Prüffeld"
        );
        assert_eq!(
            target.restore_after_menu(hwnd),
            Err(InjectError::TargetChanged)
        );
        unsafe { DestroyWindow(hwnd).unwrap() };
        assert_eq!(target.verify_window(), Err(InjectError::TargetChanged));
    }

    #[test]
    fn queries_the_windows_executable_basename_for_the_app_table() {
        let exe = std::env::current_exe().unwrap();
        let basename = exe.file_name().unwrap().to_str().unwrap();
        assert_eq!(process_name(std::process::id()).unwrap(), basename);
        assert_eq!(process_name(0), Err(InjectError::TargetUnavailable));
    }
}
