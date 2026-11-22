use super::*;
use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::Path};
use windows::{
    Win32::UI::Controls::Dialogs::{
        CommDlgExtendedError, GetOpenFileNameW, GetSaveFileNameW, OFN_DONTADDTORECENT,
        OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT,
        OFN_PATHMUSTEXIST, OPENFILENAMEW,
    },
    core::{PCWSTR, PWSTR},
};

pub(super) fn file_error() -> io::Error {
    io::Error::other(&LABELS.dictionary_file_error[..])
}

pub(super) fn choose(owner: HWND, save: bool) -> io::Result<Option<PathBuf>> {
    let mut path = vec![0u16; 32768];
    let title = HSTRING::from(if save {
        &LABELS.dictionary_export
    } else {
        &LABELS.dictionary_import
    });
    let mut dialog = OPENFILENAMEW {
        lStructSize: size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: w!("CSV (*.csv)\0*.csv\0"),
        nFilterIndex: 1,
        lpstrFile: PWSTR(path.as_mut_ptr()),
        nMaxFile: path.len() as u32,
        lpstrTitle: PCWSTR(title.as_ptr()),
        lpstrDefExt: w!("csv"),
        Flags: OFN_EXPLORER
            | OFN_NOCHANGEDIR
            | OFN_DONTADDTORECENT
            | OFN_HIDEREADONLY
            | OFN_PATHMUSTEXIST
            | if save {
                OFN_OVERWRITEPROMPT
            } else {
                OFN_FILEMUSTEXIST
            },
        ..Default::default()
    };
    let selected = unsafe {
        if save {
            GetSaveFileNameW(&mut dialog)
        } else {
            GetOpenFileNameW(&mut dialog)
        }
    };
    if !selected.as_bool() {
        return if unsafe { CommDlgExtendedError() }.0 == 0 {
            Ok(None)
        } else {
            Err(file_error())
        };
    }
    let end = path.iter().position(|c| *c == 0).ok_or_else(file_error)?;
    Ok(Some(PathBuf::from(OsString::from_wide(&path[..end]))))
}

pub(super) fn export(store: &Store, path: &Path) -> io::Result<()> {
    if !path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("csv"))
    {
        return Err(io::Error::other(&LABELS.dictionary_extension[..]));
    }
    let folder = path.parent().ok_or_else(file_error)?;
    let mut temp = tempfile::NamedTempFile::new_in(folder).map_err(|_| file_error())?;
    store
        .export_csv(temp.as_file_mut())
        .map_err(io::Error::other)?;
    temp.as_file().sync_all().map_err(|_| file_error())?;
    temp.persist(path).map_err(|_| file_error())?;
    Ok(())
}
