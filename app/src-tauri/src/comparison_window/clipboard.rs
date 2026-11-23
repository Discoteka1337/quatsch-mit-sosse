use super::*;
use windows::Win32::UI::Controls::RichEdit::{
    CHARRANGE, EM_EXGETSEL, EM_GETTEXTEX, GETTEXTEX, GETTEXTEX_FLAGS, GT_SELECTION, GT_USECRLF,
};

pub(super) fn handle(pane: HWND, id: usize, msg: u32, wp: WPARAM, lp: LPARAM) -> bool {
    let down = |vk: VIRTUAL_KEY| unsafe { GetKeyState(i32::from(vk.0)) < 0 };
    let key = VIRTUAL_KEY(wp.0 as u16);
    let app_command = (lp.0 as u32 >> 16) & 0xfff;
    let copy = match msg {
        WM_CONTEXTMENU => copy_menu(pane, lp),
        WM_COPY => Ok(true),
        WM_APPCOMMAND if app_command == APPCOMMAND_COPY.0 => Ok(true),
        WM_KEYDOWN if matches!(key, VK_INSERT | VK_C) && down(VK_CONTROL) => Ok(true),
        // A physical Ctrl+C already requested the copy on key-down. Swallow its
        // translated ETX as well as every cut command in these read-only panes.
        WM_CHAR if matches!(wp.0, 3 | 24) => return true,
        WM_CUT => return true,
        WM_APPCOMMAND if app_command == APPCOMMAND_CUT.0 => return true,
        WM_KEYDOWN if (key == VK_X && down(VK_CONTROL)) || (key == VK_DELETE && down(VK_SHIFT)) => {
            return true;
        }
        // RichEdit's diagnostic shortcut can dump the selection to a temporary file.
        WM_KEYDOWN | WM_SYSKEYDOWN
            if key == VK_F11 && down(VK_CONTROL) && down(VK_SHIFT) && down(VK_MENU) =>
        {
            return true;
        }
        _ => return false,
    };
    if let Ok(parent) = unsafe { GetParent(pane) } {
        match copy {
            Ok(true) => {
                send(parent, COPY_SELECTION, id, 0);
            }
            Err(_) => {
                send(parent, COPY_FAILED, 0, 0);
            }
            Ok(false) => {}
        }
    }
    true
}

pub(super) fn selection(pane: HWND) -> Result<Option<String>> {
    let mut range = CHARRANGE::default();
    send(pane, EM_EXGETSEL, 0, (&raw mut range) as isize);
    if range.cpMin == range.cpMax {
        return Ok(None);
    }
    // A paragraph mark in the selection expands to CRLF in CF_UNICODETEXT.
    let mut buffer = vec![0u16; text::PAGE_UNITS * 2 + 1];
    let options = GETTEXTEX {
        cb: (buffer.len() * 2) as u32,
        flags: GETTEXTEX_FLAGS(GT_SELECTION.0 | GT_USECRLF.0),
        codepage: 1200,
        ..Default::default()
    };
    let count = send(
        pane,
        EM_GETTEXTEX,
        (&raw const options) as usize,
        buffer.as_mut_ptr() as isize,
    )
    .0;
    if count <= 0 || count as usize >= buffer.len() {
        return Err(Error::from(E_FAIL));
    }
    String::from_utf16(&buffer[..count as usize])
        .map(Some)
        .map_err(|_| Error::from(E_FAIL))
}

pub(super) fn copy_selection(pane: HWND) -> Result<()> {
    if let Some(selected) = selection(pane)? {
        quatsch_mit_sosse_inject::copy_recovered_text(pane, &selected)
            .map_err(|_| Error::from(E_FAIL))?;
    }
    Ok(())
}

fn copy_menu(pane: HWND, position: LPARAM) -> Result<bool> {
    unsafe {
        let menu = Owned::new(CreatePopupMenu()?);
        let flags = if selection(pane)?.is_some() {
            MF_STRING
        } else {
            MF_STRING | MF_GRAYED
        };
        AppendMenuW(
            *menu,
            flags,
            1,
            &HSTRING::from(&crate::messages().overlay_copy),
        )?;
        let (x, y) = if position.0 == -1 {
            let mut bounds = RECT::default();
            GetWindowRect(pane, &mut bounds)?;
            (bounds.left, bounds.top)
        } else {
            (
                i32::from(position.0 as i16),
                i32::from((position.0 >> 16) as i16),
            )
        };
        Ok(TrackPopupMenu(
            *menu,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
            x,
            y,
            None,
            pane,
            None,
        )
        .0 == 1)
    }
}
