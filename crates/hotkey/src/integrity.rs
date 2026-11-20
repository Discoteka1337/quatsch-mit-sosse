use std::ptr;
use windows::Win32::Foundation::{ERROR_INVALID_SID, HANDLE, HWND};
use windows::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, SE_SID,
    TOKEN_MANDATORY_LABEL, TOKEN_QUERY, TokenIntegrityLevel,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
use windows::core::{Error, Owned, Result};

#[derive(Clone, Copy)]
pub(crate) struct IntegrityLevel(u32);

// GetTokenInformation writes both the label and its pointed-to SID into one buffer.
#[repr(C)]
#[derive(Default)]
struct LabelBuffer {
    label: TOKEN_MANDATORY_LABEL,
    sid_storage: SE_SID,
}

impl IntegrityLevel {
    pub(crate) fn current() -> Result<Self> {
        // GetCurrentProcess returns a borrowed pseudo-handle.
        Self::from_process(unsafe { GetCurrentProcess() })
    }

    pub(crate) fn window_is_higher(self, window: HWND) -> Result<bool> {
        let mut process_id = 0;
        if unsafe { GetWindowThreadProcessId(window, Some(&mut process_id)) } == 0 {
            return Err(Error::from_thread());
        }
        self.process_is_higher(process_id)
    }

    fn process_is_higher(self, process_id: u32) -> Result<bool> {
        let process = unsafe {
            Owned::new(OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION,
                false,
                process_id,
            )?)
        };
        Ok(Self::from_process(*process)?.0 > self.0)
    }

    fn from_process(process: HANDLE) -> Result<Self> {
        let mut token = HANDLE::default();
        unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token)? };
        let token = unsafe { Owned::new(token) };
        let mut buffer = LabelBuffer::default();
        let mut used = 0;
        unsafe {
            GetTokenInformation(
                *token,
                TokenIntegrityLevel,
                Some(ptr::from_mut(&mut buffer).cast()),
                size_of::<LabelBuffer>() as u32,
                &mut used,
            )?;
            // Windows returned a mandatory-label SID inside the live, aligned buffer.
            // The final subauthority is the integrity RID, including intermediate levels.
            let sid = buffer.label.Label.Sid;
            let count = *GetSidSubAuthorityCount(sid);
            let last = count.checked_sub(1).ok_or(ERROR_INVALID_SID)?;
            Ok(Self(*GetSidSubAuthority(sid, u32::from(last))))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_target_with_the_callers_exact_integrity_level() {
        let own = IntegrityLevel::current().unwrap();
        let pid = std::process::id();
        assert!(!own.process_is_higher(pid).unwrap());
        assert!(!IntegrityLevel(own.0 + 1).process_is_higher(pid).unwrap());
        if own.0 > 0 {
            assert!(IntegrityLevel(own.0 - 1).process_is_higher(pid).unwrap());
        }
    }

    #[test]
    fn invalid_window_and_unqueryable_process_are_errors() {
        let own = IntegrityLevel::current().unwrap();
        assert!(own.window_is_higher(HWND::default()).is_err());
        assert!(own.process_is_higher(0).is_err());
    }
}
