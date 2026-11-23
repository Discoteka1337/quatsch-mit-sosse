use super::*;
use std::ops::Range;
use windows::{
    Win32::{
        Foundation::POINT,
        Graphics::Gdi::ClientToScreen,
        System::SystemServices::{APPCOMMAND_COPY, APPCOMMAND_CUT},
        UI::{
            Controls::{EM_CANUNDO, EM_GETSEL, EM_REPLACESEL, EM_SETSEL},
            Input::KeyboardAndMouse::{
                IsWindowEnabled, VIRTUAL_KEY, VK_C, VK_CONTROL, VK_DELETE, VK_INSERT, VK_X,
            },
        },
    },
    core::Owned,
};

pub(in crate::native_ui) const REQUEST: u32 = WM_APP + 27;

fn collection(id: u16) -> Option<Collection> {
    let page = Collection::for_field(id)?;
    matches!(id - page.offset(), SEARCH | TERM | REPLACEMENT | ALIASES).then_some(page)
}

pub(in crate::native_ui) fn route(edit: HWND, id: u16, msg: u32, wp: WPARAM, lp: LPARAM) -> bool {
    if collection(id).is_none() {
        return false;
    }
    let down = |key: VIRTUAL_KEY| unsafe { GetKeyState(i32::from(key.0)) < 0 };
    let key = VIRTUAL_KEY(wp.0 as u16);
    let app_command = (lp.0 as u32 >> 16) & 0xfff;
    let command = match msg {
        WM_COPY | WM_CUT | WM_CONTEXTMENU => msg,
        WM_COMMAND if lp.0 == 0 && matches!(wp.0 as u32, WM_COPY | WM_CUT) => wp.0 as u32,
        WM_APPCOMMAND if app_command == APPCOMMAND_COPY.0 => WM_COPY,
        WM_APPCOMMAND if app_command == APPCOMMAND_CUT.0 => WM_CUT,
        WM_KEYDOWN if matches!(key, VK_C | VK_INSERT) && down(VK_CONTROL) => WM_COPY,
        WM_KEYDOWN if (key == VK_X && down(VK_CONTROL)) || (key == VK_DELETE && down(VK_SHIFT)) => {
            WM_CUT
        }
        // TranslateMessage can also post ETX/CAN after the key-down was handled.
        WM_CHAR if matches!(wp.0, 3 | 24) => return true,
        _ => return false,
    };
    if let Ok(parent) = unsafe { GetParent(edit) } {
        send(
            parent,
            REQUEST,
            id as usize | (command as usize) << 16,
            lp.0,
        );
    }
    true
}

pub(in crate::native_ui) fn respond(form: &Form, wp: WPARAM, lp: LPARAM) {
    let id = wp.0 as u16;
    let Some(page) = collection(id) else {
        return;
    };
    let edit = form.control(id);
    let command = (wp.0 >> 16) as u32;
    let result = if command == WM_CONTEXTMENU {
        menu(edit, lp).map_err(io::Error::from)
    } else {
        Ok(command)
    }
    .and_then(|command| {
        execute(edit, command, |text| {
            quatsch_mit_sosse_inject::copy_recovered_text(form.hwnd, text).map_err(io::Error::other)
        })
    });
    if let Err(error) = result {
        form.editor(page)
            .set_status(if error.kind() == io::ErrorKind::Interrupted {
                &LABELS.edit_selection_changed
            } else {
                &crate::messages().overlay_copy_error
            });
    }
}

pub(super) fn execute(
    edit: HWND,
    command: u32,
    publish: impl FnOnce(&str) -> io::Result<()>,
) -> io::Result<()> {
    match command {
        WM_CUT | WM_COPY => transfer(edit, command == WM_CUT, publish),
        WM_CLEAR | WM_PASTE | WM_UNDO => {
            send(edit, command, 0, 0);
            Ok(())
        }
        EM_SETSEL => {
            send(edit, EM_SETSEL, 0, -1);
            Ok(())
        }
        _ => Ok(()),
    }
}

fn range(edit: HWND) -> Range<usize> {
    let (mut start, mut end) = (0u32, 0u32);
    // The packed return value loses offsets above 65,535 UTF-16 code units.
    send(
        edit,
        EM_GETSEL,
        (&raw mut start) as usize,
        (&raw mut end) as isize,
    );
    start as usize..end as usize
}

fn selected_text(edit: HWND, selected: Range<usize>) -> io::Result<String> {
    let mut buffer = vec![0; selected.end + 1];
    let count = unsafe { GetWindowTextW(edit, &mut buffer) } as usize;
    let units = buffer[..count]
        .get(selected)
        .ok_or(io::ErrorKind::InvalidData)?;
    String::from_utf16(units).map_err(io::Error::other)
}

fn writable(edit: HWND) -> bool {
    unsafe { IsWindowEnabled(edit).as_bool() && GetWindowLongW(edit, GWL_STYLE) & ES_READONLY == 0 }
}

fn transfer(edit: HWND, cut: bool, publish: impl FnOnce(&str) -> io::Result<()>) -> io::Result<()> {
    let selected = range(edit);
    if selected.is_empty() || (cut && !writable(edit)) {
        return Ok(());
    }
    let text = selected_text(edit, selected.clone())?;
    publish(&text)?;
    if cut {
        // Publishing can reenter the window. Delete only the text we just copied.
        if !writable(edit) || range(edit) != selected || selected_text(edit, selected)? != text {
            return Err(io::ErrorKind::Interrupted.into());
        }
        send(edit, EM_REPLACESEL, 1, w!("").as_ptr() as isize);
    }
    Ok(())
}

fn menu(edit: HWND, position: LPARAM) -> windows::core::Result<u32> {
    let selected = !range(edit).is_empty();
    let writable = writable(edit);
    unsafe {
        let menu = Owned::new(CreatePopupMenu()?);
        for (command, label, enabled) in [
            (
                WM_UNDO,
                &LABELS.edit_undo,
                writable && send(edit, EM_CANUNDO, 0, 0).0 != 0,
            ),
            (WM_CUT, &LABELS.edit_cut, selected && writable),
            (WM_COPY, &crate::messages().overlay_copy, selected),
            (WM_PASTE, &LABELS.edit_paste, writable),
            (WM_CLEAR, &LABELS.dictionary_delete, selected && writable),
            (
                EM_SETSEL,
                &LABELS.edit_select_all,
                GetWindowTextLengthW(edit) != 0,
            ),
        ] {
            let flags = if enabled {
                MF_STRING
            } else {
                MF_STRING | MF_GRAYED
            };
            AppendMenuW(*menu, flags, command as usize, &HSTRING::from(label))?;
        }
        let point = if position.0 == -1 {
            let mut point = POINT::default();
            let _ = GetCaretPos(&mut point);
            ClientToScreen(edit, &mut point).ok()?;
            point
        } else {
            POINT {
                x: i32::from(position.0 as i16),
                y: i32::from((position.0 >> 16) as i16),
            }
        };
        Ok(TrackPopupMenu(
            *menu,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            None,
            edit,
            None,
        )
        .0 as u32)
    }
}
