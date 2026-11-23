use super::*;
use std::{
    ffi::OsString,
    fs::File,
    os::windows::{ffi::OsStringExt, fs::MetadataExt},
    path::{Component, Path, PathBuf, Prefix},
};
use windows::{
    Win32::UI::Controls::Dialogs::{
        CommDlgExtendedError, GetOpenFileNameW, GetSaveFileNameW, OFN_DONTADDTORECENT,
        OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_NOCHANGEDIR, OFN_NONETWORKBUTTON,
        OFN_NOTESTFILECREATE, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
    },
    Win32::{
        Storage::FileSystem::{FILE_ATTRIBUTE_REPARSE_POINT, GetDriveTypeW},
        System::WindowsProgramming::{DRIVE_CDROM, DRIVE_FIXED, DRIVE_RAMDISK, DRIVE_REMOVABLE},
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
            | OFN_NONETWORKBUTTON
            | OFN_NOTESTFILECREATE
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

fn local_error() -> io::Error {
    io::Error::other(&LABELS.dictionary_local_path[..])
}

fn local_drive(path: &Path, is_local: impl FnOnce(u8) -> bool) -> io::Result<()> {
    let mut components = path.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return Err(local_error());
    };
    let (Prefix::Disk(letter) | Prefix::VerbatimDisk(letter)) = prefix.kind() else {
        return Err(local_error());
    };
    if components.next() != Some(Component::RootDir) || !is_local(letter) {
        return Err(local_error());
    }
    Ok(())
}

fn local_path(path: &Path) -> io::Result<PathBuf> {
    resolve(
        path,
        |drive| {
            let root = HSTRING::from(format!("{}:\\", char::from(drive)));
            matches!(
                unsafe { GetDriveTypeW(&root) },
                DRIVE_FIXED | DRIVE_REMOVABLE | DRIVE_CDROM | DRIVE_RAMDISK
            )
        },
        |path| {
            let meta = std::fs::symlink_metadata(path).map_err(|_| file_error())?;
            if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
                std::fs::read_link(path)
                    .map(Some)
                    .map_err(|_| local_error())
            } else {
                Ok(None)
            }
        },
    )
}

// Check each link before following it. Canonicalizing an entire UNC-backed
// junction first would already have contacted its server.
fn resolve(
    path: &Path,
    is_local: impl Fn(u8) -> bool,
    mut link: impl FnMut(&Path) -> io::Result<Option<PathBuf>>,
) -> io::Result<PathBuf> {
    let mut pending = path.to_path_buf();
    'links: for _ in 0..=32 {
        local_drive(&pending, &is_local)?;
        let mut resolved = PathBuf::new();
        let mut parts = pending.components();
        while let Some(part) = parts.next() {
            match part {
                Component::ParentDir => {
                    resolved.pop();
                }
                Component::Normal(_) => {
                    resolved.push(part);
                    if let Some(target) = link(&resolved)? {
                        resolved.pop();
                        resolved.push(target);
                        if !parts.as_path().as_os_str().is_empty() {
                            resolved.push(parts.as_path());
                        }
                        pending = resolved;
                        continue 'links;
                    }
                }
                _ => resolved.push(part),
            }
        }
        return Ok(resolved);
    }
    Err(local_error())
}

pub(super) fn open(path: &Path) -> io::Result<File> {
    File::open(local_path(path)?).map_err(|_| file_error())
}

pub(super) fn export(
    path: &Path,
    write: impl FnOnce(&mut File) -> quatsch_mit_sosse_store::Result<()>,
) -> io::Result<()> {
    if !path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("csv"))
    {
        return Err(io::Error::other(&LABELS.dictionary_extension[..]));
    }
    let folder = local_path(path.parent().ok_or_else(file_error)?)?;
    let path = folder.join(path.file_name().ok_or_else(file_error)?);
    let mut temp = tempfile::NamedTempFile::new_in(&folder).map_err(|_| file_error())?;
    write(temp.as_file_mut()).map_err(io::Error::other)?;
    temp.as_file().sync_all().map_err(|_| file_error())?;
    temp.persist(path).map_err(|_| file_error())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn following_a_directory_link_checks_its_target_before_reading_the_next_component() {
        let mut inspected = Vec::new();
        let path = Path::new(r"C:\Daten\Link\Datei.csv");
        let result = resolve(
            path,
            |_| true,
            |p| {
                inspected.push(p.to_path_buf());
                Ok((p.file_name().unwrap() == "Link")
                    .then(|| PathBuf::from(r"\\example.invalid\share")))
            },
        );
        assert_eq!(
            result.unwrap_err().to_string(),
            LABELS.dictionary_local_path
        );
        assert_eq!(
            inspected,
            [PathBuf::from(r"C:\Daten"), PathBuf::from(r"C:\Daten\Link")]
        );
        let local = resolve(
            path,
            |_| true,
            |p| Ok((p.file_name().unwrap() == "Link").then(|| PathBuf::from("Nebenordner"))),
        )
        .unwrap();
        assert_eq!(local, Path::new(r"C:\Daten\Nebenordner\Datei.csv"));
    }

    #[test]
    fn a_non_file_destination_cannot_receive_an_export_or_start_an_import() {
        let device = Path::new(r"\\.\pipe\qms-review-regression.csv");
        let error = open(device).unwrap_err();
        assert_eq!(error.to_string(), LABELS.dictionary_local_path);
        let error = export(device, |_| panic!("unexpected export")).unwrap_err();
        assert_eq!(error.to_string(), LABELS.dictionary_local_path);
    }

    #[test]
    fn network_and_device_csv_paths_fail_before_any_file_access() {
        for name in [
            r"\\example.invalid\share\words.csv",
            r"\\?\UNC\example.invalid\share\words.csv",
            r"\\.\pipe\words.csv",
            r"Z:words.csv",
        ] {
            assert!(local_drive(Path::new(name), |_| panic!("unexpected volume access")).is_err());
        }
        assert!(local_drive(Path::new(r"Z:\snippets.csv"), |_| false).is_err());
        assert!(local_drive(Path::new(r"\\?\C:\Daten\Wörter.csv"), |_| true).is_ok());
    }
}
