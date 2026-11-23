use std::{ffi::OsStr, io, os::windows::ffi::OsStrExt, path::Path};
use windows::{
    Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS},
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, REG_SZ, RRF_RT_ANY, RegDeleteKeyValueW, RegGetValueW,
            RegSetKeyValueW,
        },
    },
    core::{PCWSTR, w},
};

const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const ENTRY: PCWSTR = w!("quatsch-mit-sosse");

pub(super) fn command(executable: &Path) -> io::Result<Vec<u16>> {
    let command: Vec<u16> = OsStr::new("\"")
        .encode_wide()
        .chain(executable.as_os_str().encode_wide())
        .chain(OsStr::new("\" --autostart").encode_wide())
        .chain([0])
        .collect();
    // Windows limits a Run value to a 260-character command line.
    if command.len() - 1 > 260 {
        return Err(io::Error::other(
            "Der Programmpfad ist für den Windows-Autostart zu lang.",
        ));
    }
    Ok(command)
}

pub(super) fn set(enabled: bool) -> io::Result<()> {
    set_at(HKEY_CURRENT_USER, RUN_KEY, enabled)
}

pub(crate) fn repair(enabled: bool) -> io::Result<()> {
    repair_at(HKEY_CURRENT_USER, RUN_KEY, enabled)
}

fn repair_at(root: HKEY, subkey: PCWSTR, enabled: bool) -> io::Result<()> {
    if !enabled {
        // A crash can leave a registration even when the settings file says
        // disabled. Avoid asking for write access on normal, clean launches.
        let status = unsafe { RegGetValueW(root, subkey, ENTRY, RRF_RT_ANY, None, None, None) };
        if matches!(status, ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) {
            return Ok(());
        }
        status.ok()?;
    }
    set_at(root, subkey, enabled)
}

fn set_at(root: HKEY, subkey: PCWSTR, enabled: bool) -> io::Result<()> {
    let status = if enabled {
        let value = command(&std::env::current_exe()?)?;
        unsafe {
            RegSetKeyValueW(
                root,
                subkey,
                ENTRY,
                REG_SZ.0,
                Some(value.as_ptr().cast()),
                std::mem::size_of_val(value.as_slice()) as u32,
            )
        }
    } else {
        let status = unsafe { RegDeleteKeyValueW(root, subkey, ENTRY) };
        if matches!(status, ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) {
            return Ok(());
        }
        status
    };
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(status.0 as i32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::{
        Win32::System::Registry::{
            KEY_READ, KEY_WRITE, REG_OPTION_VOLATILE, RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW,
            RegDeleteKeyW, RegGetValueW,
        },
        core::{GUID, HSTRING},
    };

    struct TestKey(HKEY, HSTRING);

    impl Drop for TestKey {
        fn drop(&mut self) {
            unsafe {
                let _ = RegCloseKey(self.0);
                let _ = RegDeleteKeyW(HKEY_CURRENT_USER, &self.1);
            }
        }
    }

    #[test]
    fn login_entry_round_trips_through_an_isolated_registry_key() {
        let path = HSTRING::from(format!(
            r"Software\qms-autostart-test-{:?}",
            GUID::new().unwrap()
        ));
        let mut handle = HKEY::default();
        unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                &path,
                None,
                None,
                REG_OPTION_VOLATILE,
                KEY_READ | KEY_WRITE,
                None,
                &mut handle,
                None,
            )
            .ok()
            .unwrap();
        }
        let key = TestKey(handle, path);
        let read = |buffer: &mut [u16], length: &mut u32| unsafe {
            RegGetValueW(
                key.0,
                None,
                ENTRY,
                RRF_RT_REG_SZ,
                None,
                Some(buffer.as_mut_ptr().cast()),
                Some(length),
            )
        };
        set_at(key.0, PCWSTR::null(), true).unwrap();
        let mut value = [0; 261];
        let mut size = size_of_val(&value) as u32;
        assert_eq!(read(&mut value, &mut size), ERROR_SUCCESS);
        assert_eq!(
            &value[..(size as usize / 2)],
            command(&std::env::current_exe().unwrap()).unwrap()
        );
        set_at(key.0, PCWSTR::null(), false).unwrap();
        assert_eq!(read(&mut value, &mut size), ERROR_FILE_NOT_FOUND);
        set_at(key.0, PCWSTR::null(), false).unwrap();
        set_at(key.0, PCWSTR::null(), true).unwrap();
        repair_at(key.0, PCWSTR::null(), false).unwrap();
        assert_eq!(read(&mut value, &mut size), ERROR_FILE_NOT_FOUND);
        repair_at(key.0, PCWSTR::null(), false).unwrap();
    }
}
