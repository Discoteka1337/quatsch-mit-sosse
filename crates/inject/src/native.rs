use crate::{InjectConfig, InjectError, InjectMethod, clipboard, is_long, keyboard, route, uia};
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::Path;
use std::sync::Mutex;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
};
use windows::core::{Owned, PWSTR};

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

    /// Restore a known application after the user selected the tray menu command.
    pub fn restore_after_menu(self, menu_owner: HWND) -> Result<(), InjectError> {
        if self.check_foreground().is_err() {
            self.verify_window()?;
            if menu_owner.is_invalid() || unsafe { GetForegroundWindow() } != menu_owner {
                return Err(InjectError::TargetChanged);
            }
            let _ = unsafe { SetForegroundWindow(self.hwnd()) };
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

    fn deliver(&self, text: &str, config: &InjectConfig) -> Result<InjectMethod, InjectError> {
        self.check_foreground()?;
        let forced = if config.apps.is_empty() || is_long(text) {
            None
        } else {
            config.method_for(&process_name(self.process_id)?)
        };
        route(text, forced, |method| {
            self.check_foreground()?;
            match method {
                InjectMethod::UiAutomation => uia::insert(self, text),
                InjectMethod::Unicode => keyboard::insert(self, text),
                InjectMethod::Clipboard => clipboard::paste(self, text),
            }
        })
    }

    /// Run the insertion on a dedicated COM thread bound to the captured window.
    pub fn insert(self, text: &str, config: &InjectConfig) -> Result<InjectMethod, InjectError> {
        static INSERTION: Mutex<()> = Mutex::new(());
        let _guard = INSERTION.try_lock().map_err(|_| InjectError::Busy)?;
        std::thread::scope(|scope| {
            let worker = std::thread::Builder::new()
                .name("qms-inject".into())
                .spawn_scoped(scope, move || self.deliver(text, config))
                .map_err(|_| InjectError::Unavailable)?;
            worker.join().unwrap_or(Err(InjectError::DeliveryUncertain))
        })
    }
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
