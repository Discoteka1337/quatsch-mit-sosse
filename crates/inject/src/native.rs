use crate::{InjectConfig, InjectError, InjectMethod, clipboard, is_long, keyboard, route, uia};
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::Path;
use std::sync::Mutex;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
use windows::core::{Owned, PWSTR};

pub(crate) struct Target {
    pub(crate) hwnd: HWND,
    thread_id: u32,
    process_id: u32,
}

impl Target {
    fn capture() -> Result<Self, InjectError> {
        let hwnd = unsafe { GetForegroundWindow() };
        let mut process_id = 0;
        let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        if hwnd.is_invalid() || thread_id == 0 || process_id == 0 {
            return Err(InjectError::TargetUnavailable);
        }
        Ok(Self {
            hwnd,
            thread_id,
            process_id,
        })
    }

    pub(crate) fn check(&self) -> Result<(), InjectError> {
        let active = Self::capture().map_err(|_| InjectError::TargetChanged)?;
        if active.hwnd == self.hwnd
            && active.thread_id == self.thread_id
            && active.process_id == self.process_id
        {
            Ok(())
        } else {
            Err(InjectError::TargetChanged)
        }
    }

    fn insert(&self, text: &str, config: &InjectConfig) -> Result<InjectMethod, InjectError> {
        self.check()?;
        let forced = if config.apps.is_empty() || is_long(text) {
            None
        } else {
            config.method_for(&process_name(self.process_id)?)
        };
        route(text, forced, |method| {
            self.check()?;
            match method {
                InjectMethod::UiAutomation => uia::insert(self, text),
                InjectMethod::Unicode => keyboard::insert(self, text),
                InjectMethod::Clipboard => clipboard::paste(self, text),
            }
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
    static INSERTION: Mutex<()> = Mutex::new(());
    let _guard = INSERTION.try_lock().map_err(|_| InjectError::Busy)?;
    let Target {
        hwnd,
        thread_id,
        process_id,
    } = Target::capture()?;
    // A window handle is an opaque identity. No window or COM object is moved.
    let window_id = hwnd.0 as usize;
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("qms-inject".into())
            .spawn_scoped(scope, move || {
                let target = Target {
                    hwnd: HWND(window_id as _),
                    thread_id,
                    process_id,
                };
                target.insert(text, config)
            })
            .map_err(|_| InjectError::Unavailable)?;
        worker.join().unwrap_or(Err(InjectError::DeliveryUncertain))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_the_windows_executable_basename_for_the_app_table() {
        let exe = std::env::current_exe().unwrap();
        let basename = exe.file_name().unwrap().to_str().unwrap();
        assert_eq!(process_name(std::process::id()).unwrap(), basename);
        assert_eq!(process_name(0), Err(InjectError::TargetUnavailable));
    }
}
