use std::mem;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::Graphics::Gdi::{
    CopyEnhMetaFileW, DeleteEnhMetaFile, DeleteMetaFile, DeleteObject, HENHMETAFILE, HGDIOBJ,
};
use windows::Win32::System::DataExchange::{
    EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, METAFILEPICT,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GMEM_ZEROINIT, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::Win32::System::Ole::{
    CF_BITMAP, CF_DSPBITMAP, CF_DSPENHMETAFILE, CF_DSPMETAFILEPICT, CF_ENHMETAFILE,
    CF_METAFILEPICT, CF_OWNERDISPLAY, CF_PALETTE, CF_PRIVATEFIRST, CF_PRIVATELAST, CF_UNICODETEXT,
    CLIPBOARD_FORMAT, OleDuplicateData,
};
mod owner;
use owner::{ClipboardLock, OwnerWindow};
use windows::core::{PCWSTR, w};

use super::{InjectError, Target, keyboard};

#[cfg(test)]
mod tests;

static TRANSACTION: std::sync::Mutex<()> = std::sync::Mutex::new(());

const PRIVACY_FORMATS: [PCWSTR; 3] = [
    w!("ExcludeClipboardContentFromMonitorProcessing"),
    w!("CanIncludeInClipboardHistory"),
    w!("CanUploadToCloudClipboard"),
];

/// Read Unicode text for an explicitly requested snippet placeholder.
pub fn clipboard_text() -> Result<String, InjectError> {
    let _guard = TRANSACTION.try_lock().map_err(|_| InjectError::Busy)?;
    let _open = ClipboardLock::open(HWND::default())?;
    if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT.0.into()) }.is_err() {
        return Ok(String::new());
    }
    let handle = unsafe { GetClipboardData(CF_UNICODETEXT.0.into()) }
        .map_err(|_| InjectError::Unavailable)?;
    let memory = HGLOBAL(handle.0);
    let count = unsafe { GlobalSize(memory) } / size_of::<u16>();
    let data = unsafe { GlobalLock(memory) }.cast::<u16>();
    if data.is_null() {
        return Err(InjectError::Unavailable);
    }
    // GlobalSize bounds the search even if another application omitted the terminator.
    let result = decode_clipboard_text(unsafe { std::slice::from_raw_parts(data, count) });
    let _ = unsafe { GlobalUnlock(memory) };
    result
}

fn decode_clipboard_text(units: &[u16]) -> Result<String, InjectError> {
    let end = units
        .iter()
        .position(|&unit| unit == 0)
        .ok_or(InjectError::Unavailable)?;
    String::from_utf16(&units[..end]).map_err(|_| InjectError::Unavailable)
}

/// Replace the clipboard on an explicit copy request, retaining the text for later paste.
/// A busy clipboard can be retried by the caller without changing the foreground window.
pub fn copy_recovered_text(owner: HWND, text: &str) -> Result<(), InjectError> {
    if owner.is_invalid() {
        return Err(InjectError::Unavailable);
    }
    let (flags, text) = protected_formats(text)?;
    let _open = ClipboardLock::open(owner)?;
    unsafe { EmptyClipboard() }.map_err(|_| InjectError::Unavailable)?;
    publish_immediately(flags, text, set_data)
}

fn publish_immediately(
    flags: Vec<OwnedFormat>,
    text: OwnedFormat,
    mut set: impl FnMut(CLIPBOARD_FORMAT, HANDLE) -> Result<(), InjectError>,
) -> Result<(), InjectError> {
    for entry in flags.into_iter().chain([text]) {
        entry.publish_with(&mut set)?;
    }
    Ok(())
}

fn set_data(format: CLIPBOARD_FORMAT, handle: HANDLE) -> Result<(), InjectError> {
    unsafe { SetClipboardData(format.0.into(), Some(handle)) }
        .map(|_| ())
        .map_err(|_| InjectError::Unavailable)
}

pub(crate) fn paste(
    target: &Target,
    text: &str,
    on_restore_delay: &dyn Fn(),
) -> Result<(), InjectError> {
    // A desktop caller may resume before an earlier snapshot has been restored.
    let _guard = TRANSACTION.try_lock().map_err(|_| InjectError::Busy)?;
    let owner = OwnerWindow::new(target, on_restore_delay)?;
    temporary_text(
        &owner,
        text,
        || target.check_foreground(),
        || owner.send_and_wait(target),
    )
}

trait Clipboard {
    type Lock;
    fn open(&self, restoring: bool) -> Result<Self::Lock, InjectError>;
    fn owns_contents(&self) -> bool;
    fn offer_text(&self, text: OwnedFormat) -> Result<(), InjectError>;
    fn clear(&self) -> Result<(), InjectError>;
    fn formats(&self) -> Result<Vec<CLIPBOARD_FORMAT>, InjectError>;
    fn get(&self, format: CLIPBOARD_FORMAT) -> Result<HANDLE, InjectError>;
    fn set(&self, format: CLIPBOARD_FORMAT, handle: HANDLE) -> Result<(), InjectError>;
}

fn protected_formats(text: &str) -> Result<(Vec<OwnedFormat>, OwnedFormat), InjectError> {
    super::validate_text(text)?;
    let mut flags = Vec::with_capacity(PRIVACY_FORMATS.len());
    for name in PRIVACY_FORMATS {
        flags.push(OwnedFormat::from_bytes(
            register(name)?,
            &0u32.to_le_bytes(),
        )?);
    }
    let text = OwnedFormat::from_bytes(CF_UNICODETEXT, &utf16_bytes(text))?;
    Ok((flags, text))
}

fn temporary_text(
    clipboard: &impl Clipboard,
    text: &str,
    check_target: impl FnOnce() -> Result<(), InjectError>,
    use_clipboard: impl FnOnce() -> Result<(), InjectError>,
) -> Result<(), InjectError> {
    let (replacement, text) = protected_formats(text)?;
    let privacy_formats: Vec<_> = replacement.iter().map(|entry| entry.format).collect();
    let backup = {
        let _open = clipboard.open(false)?;
        let available = clipboard.formats()?;
        // A private owner may serve live objects, streams or index-based data. A
        // byte copy of OLE's marshaled DataObject/metadata cannot restore it.
        for name in [w!("DataObject"), w!("Ole Private Data")] {
            if available.contains(&register(name)?) {
                return Err(InjectError::ClipboardUnsupported);
            }
        }
        let backup = available
            .into_iter()
            .map(|format| OwnedFormat::capture(clipboard, format))
            .collect::<Result<Vec<_>, _>>()?;
        check_target()?;
        clipboard.clear()?;
        let published = replacement
            .into_iter()
            .try_for_each(|flag| flag.publish(clipboard))
            .and_then(|()| clipboard.offer_text(text));
        if let Err(error) = published {
            restore(clipboard, backup, &privacy_formats)?;
            return Err(error);
        }
        backup
    };
    if !clipboard.owns_contents() {
        return Err(InjectError::ClipboardChanged);
    }
    let result = use_clipboard();
    let _open = clipboard.open(true)?;
    if !clipboard.owns_contents() {
        // Another application copied new data. Preserve the newer copy.
        return Err(InjectError::ClipboardChanged);
    }
    restore(clipboard, backup, &privacy_formats)?;
    result
}

fn restore(
    clipboard: &impl Clipboard,
    mut backup: Vec<OwnedFormat>,
    privacy_formats: &[CLIPBOARD_FORMAT],
) -> Result<(), InjectError> {
    clipboard
        .clear()
        .map_err(|_| InjectError::ClipboardRestore)?;
    let mut result = Ok(());
    // Preserve the relative priority of all data formats, but restore their
    // original history/cloud policy before making any payload available.
    backup.sort_by_key(|entry| !privacy_formats.contains(&entry.format));
    for entry in backup {
        let protected = privacy_formats.contains(&entry.format);
        let published = entry.publish_with(|format, handle| {
            clipboard
                .set(format, handle)
                .or_else(|_| clipboard.set(format, handle))
        });
        if published.is_err() {
            if protected {
                return Err(InjectError::ClipboardRestore);
            }
            result = Err(InjectError::ClipboardRestore);
        }
    }
    result
}

fn register(name: PCWSTR) -> Result<CLIPBOARD_FORMAT, InjectError> {
    let format = unsafe { RegisterClipboardFormatW(name) };
    if format == 0 {
        return Err(InjectError::Unavailable);
    }
    u16::try_from(format)
        .map(CLIPBOARD_FORMAT)
        .map_err(|_| InjectError::Unavailable)
}

fn utf16_bytes(text: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut units = text.encode_utf16().peekable();
    while let Some(unit) = units.next() {
        match unit {
            10 | 13 => {
                if unit == 13 && units.peek() == Some(&10) {
                    units.next();
                }
                bytes.extend_from_slice(&[13, 0, 10, 0]);
            }
            _ => bytes.extend_from_slice(&unit.to_le_bytes()),
        }
    }
    bytes.extend_from_slice(&[0, 0]);
    bytes
}

struct OwnedFormat {
    format: CLIPBOARD_FORMAT,
    handle: HANDLE,
}

impl OwnedFormat {
    fn from_bytes(format: CLIPBOARD_FORMAT, bytes: &[u8]) -> Result<Self, InjectError> {
        unsafe {
            let memory = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len())
                .map_err(|_| InjectError::Unavailable)?;
            let entry = Self {
                format,
                handle: HANDLE(memory.0),
            };
            let data = GlobalLock(memory).cast::<u8>();
            if data.is_null() {
                return Err(InjectError::Unavailable);
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());
            let _ = GlobalUnlock(memory);
            Ok(entry)
        }
    }

    fn capture(clipboard: &impl Clipboard, format: CLIPBOARD_FORMAT) -> Result<Self, InjectError> {
        if format == CF_OWNERDISPLAY || (CF_PRIVATEFIRST.0..=CF_PRIVATELAST.0).contains(&format.0) {
            return Err(InjectError::ClipboardUnsupported);
        }
        let source = clipboard
            .get(format)
            .map_err(|_| InjectError::ClipboardUnsupported)?;
        unsafe {
            let handle = match storage_format(format) {
                CF_ENHMETAFILE => HANDLE(CopyEnhMetaFileW(HENHMETAFILE(source.0), None).0),
                storage => OleDuplicateData(source, storage, GMEM_MOVEABLE),
            };
            if handle.is_invalid() {
                return Err(InjectError::ClipboardUnsupported);
            }
            Ok(Self { format, handle })
        }
    }

    fn publish(self, clipboard: &impl Clipboard) -> Result<(), InjectError> {
        self.publish_with(|format, handle| clipboard.set(format, handle))
    }

    fn publish_with(
        self,
        set: impl FnOnce(CLIPBOARD_FORMAT, HANDLE) -> Result<(), InjectError>,
    ) -> Result<(), InjectError> {
        set(self.format, self.handle)?;
        // The clipboard now owns the handle.
        mem::forget(self);
        Ok(())
    }
}

fn storage_format(format: CLIPBOARD_FORMAT) -> CLIPBOARD_FORMAT {
    match format {
        CF_DSPBITMAP => CF_BITMAP,
        CF_DSPMETAFILEPICT => CF_METAFILEPICT,
        CF_DSPENHMETAFILE => CF_ENHMETAFILE,
        other => other,
    }
}

impl Drop for OwnedFormat {
    fn drop(&mut self) {
        unsafe {
            let memory = HGLOBAL(self.handle.0);
            match storage_format(self.format) {
                CF_BITMAP | CF_PALETTE => {
                    let _ = DeleteObject(HGDIOBJ(self.handle.0));
                }
                CF_ENHMETAFILE => {
                    let _ = DeleteEnhMetaFile(Some(HENHMETAFILE(self.handle.0)));
                }
                CF_METAFILEPICT => {
                    let picture = GlobalLock(memory).cast::<METAFILEPICT>();
                    if !picture.is_null() {
                        let _ = DeleteMetaFile((*picture).hMF);
                        let _ = GlobalUnlock(memory);
                    }
                    let _ = GlobalFree(Some(memory));
                }
                _ => {
                    let _ = GlobalFree(Some(memory));
                }
            }
        }
    }
}
