use std::{
    cell::{Cell, RefCell},
    io,
    rc::Rc,
};
use tauri::{AppHandle, Manager, Window};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, DEFAULT_CHARSET, DEFAULT_GUI_FONT,
            DEFAULT_PITCH, DeleteObject, FF_DONTCARE, GetStockObject, HGDIOBJ, OUT_DEFAULT_PRECIS,
        },
        System::SystemServices::SS_NOPREFIX,
        UI::{
            Controls::{
                ICC_PROGRESS_CLASS, INITCOMMONCONTROLSEX, InitCommonControlsEx, PBM_SETPOS,
            },
            HiDpi::GetDpiForWindow,
            Input::KeyboardAndMouse::{EnableWindow, GetKeyState, SetFocus, VK_SHIFT},
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::*,
        },
    },
    core::{HSTRING, w},
};

mod capture;
mod dictionary;
mod labels;
mod layout;
mod probe;
mod values;
use capture::Chord;
use labels::LABELS;
use layout::*;

const FIELD_KEY_DOWN: u32 = WM_APP + 21;
const FIELD_KEY_UP: u32 = WM_APP + 22;
const FIELD_BLUR: u32 = WM_APP + 23;
const FIELD_FOCUS: u32 = WM_APP + 24;
const PROBE_TIMER: usize = 0x514d;
const REMOVE_TIMER: usize = 0x514e;
const DICTIONARY_TIMER: usize = 0x5150;

fn scale(value: i32, dpi: u32) -> i32 {
    (i64::from(value) * i64::from(dpi.max(96)) / 96) as i32
}

pub(super) struct Font(pub(super) usize);
impl Font {
    pub(super) fn create(size: i32, weight: i32, dpi: u32) -> io::Result<Self> {
        let font = unsafe {
            CreateFontW(
                -scale(size, dpi),
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
                w!("Segoe UI"),
            )
        };
        if font.0.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(font.0 as usize))
    }
}
impl Drop for Font {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(self.0 as _));
        }
    }
}

fn send(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> LRESULT {
    unsafe { SendMessageW(hwnd, msg, Some(WPARAM(wp)), Some(LPARAM(lp))) }
}

struct Form {
    hwnd: HWND,
    app: AppHandle,
    scroll: Cell<i32>,
    dpi: Cell<u32>,
    in_layout: Cell<bool>,
    fonts: RefCell<Option<(Font, Font)>>,
    dictionary: dictionary::Page,
    capturing: Cell<u16>,
    chord: RefCell<Chord>,
    devices: RefCell<Vec<String>>,
    model_ids: RefCell<Vec<String>>,
    probe: RefCell<Option<probe::Probe>>,
    removal: RefCell<Option<std::sync::mpsc::Receiver<Result<(), String>>>>,
}

impl Form {
    fn control(&self, id: u16) -> HWND {
        unsafe { GetDlgItem(Some(self.hwnd), i32::from(id)) }.unwrap_or_default()
    }
    fn set_text(&self, id: u16, text: &str) {
        unsafe {
            let _ = SetWindowTextW(self.control(id), &HSTRING::from(text));
        }
    }
    fn error(&self, error: io::Error) {
        self.set_text(STATUS, &format!("{} {error}", LABELS.general_failure));
    }
    fn viewport(&self) -> RECT {
        let mut rect = RECT::default();
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut rect);
        }
        rect
    }
    fn focus_field(&self, id: u16) {
        if !FIELDS.iter().any(|field| field.id == id) {
            return;
        }
        let area = self.viewport();
        let field = placement(id, area.right, self.dpi.get(), self.scroll.get());
        let top = field.y;
        let bottom = field.y + field.height - area.bottom;
        let delta = if top < 0 { top } else { bottom.max(0) };
        if delta != 0 {
            self.scroll_by(delta);
        }
    }
    fn scroll_by(&self, delta: i32) {
        self.scroll.set(self.scroll.get().saturating_add(delta));
        let _ = layout::layout(self);
    }
    fn detach(&self) {
        self.cancel_capture();
        self.end_probe();
        let stock = unsafe { GetStockObject(DEFAULT_GUI_FONT) };
        for field in FIELDS.iter() {
            send(self.control(field.id), WM_SETFONT, stock.0 as usize, 0);
        }
    }
}

fn navigate(parent: HWND, field: Option<HWND>) {
    let previous = unsafe { GetKeyState(i32::from(VK_SHIFT.0)) < 0 };
    unsafe {
        if let Ok(next) = GetNextDlgTabItem(parent, field, previous) {
            let _ = SetFocus(Some(next));
        }
    }
}

// Only forward key events from shortcut fields to the parent. Its return value
// indicates whether a recording is active. Other controls keep their native input.
unsafe extern "system" fn child_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    id: usize,
    _: usize,
) -> LRESULT {
    if msg == WM_NCDESTROY {
        unsafe {
            let _ = RemoveWindowSubclass(hwnd, Some(child_proc), id);
        }
    } else if let Ok(parent) = unsafe { GetParent(hwnd) } {
        match msg {
            WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP => {
                let down = matches!(msg, WM_KEYDOWN | WM_SYSKEYDOWN);
                if matches!(id as u16, DICTATION | REINSERT) {
                    let event = if down { FIELD_KEY_DOWN } else { FIELD_KEY_UP };
                    if send(parent, event, id, wp.0 as isize).0 != 0 {
                        return LRESULT(0);
                    }
                }
                if down && wp.0 == 9 {
                    navigate(parent, Some(hwnd));
                    return LRESULT(0);
                }
            }
            WM_SETFOCUS => {
                send(parent, FIELD_FOCUS, id, 0);
            }
            WM_KILLFOCUS => {
                send(parent, FIELD_BLUR, id, 0);
            }
            _ => {}
        }
    }
    unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
}

unsafe extern "system" fn settings_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT {
    // A modal dialog can reenter this procedure and destroy the window. Retain
    // one invocation-local reference in addition to the subclass-owned one.
    let ptr = data as *const Form;
    let form = unsafe {
        Rc::increment_strong_count(ptr);
        Rc::from_raw(ptr)
    };
    match msg {
        WM_NCDESTROY => unsafe {
            let _ = RemoveWindowSubclass(hwnd, Some(settings_proc), id);
            drop(Rc::from_raw(ptr));
        },
        WM_DESTROY => form.detach(),
        WM_SIZE | WM_DPICHANGED => {
            let _ = layout::layout(&form);
        }
        WM_ACTIVATE if wp.0 as u16 != 0 => {
            if let Err(error) = form.refresh() {
                form.error(error);
            }
        }
        WM_SHOWWINDOW if wp.0 != 0 && form.model_ids.borrow().is_empty() => {
            if let Err(error) = form.refresh() {
                form.error(error);
            }
        }
        WM_ACTIVATE => form.cancel_capture(),
        WM_KEYDOWN if wp.0 == 9 => {
            navigate(hwnd, None);
            return LRESULT(0);
        }
        FIELD_FOCUS => {
            form.focus_field(wp.0 as u16);
            return LRESULT(0);
        }
        FIELD_BLUR => {
            if form.capturing.get() == wp.0 as u16 {
                form.cancel_capture();
            }
            return LRESULT(0);
        }
        FIELD_KEY_DOWN | FIELD_KEY_UP => {
            if form.capturing.get() == wp.0 as u16 {
                if let Err(error) = form.capture_key(lp.0 as u32, msg == FIELD_KEY_DOWN) {
                    form.error(error);
                }
                return LRESULT(1);
            }
            return LRESULT(0);
        }
        WM_COMMAND
            if lp.0 != 0 && (on_dictionary_page(wp.0 as u16) || wp.0 as u16 == OPEN_DICTIONARY) =>
        {
            if let Err(error) = form.dictionary_command(wp.0 as u16, (wp.0 >> 16) as u32) {
                form.set_text(DICTIONARY_STATUS, &error.to_string());
            }
            return LRESULT(0);
        }
        WM_TIMER if wp.0 == DICTIONARY_TIMER => {
            if let Err(error) = form.poll_dictionary() {
                form.set_text(DICTIONARY_STATUS, &error.to_string());
            }
            return LRESULT(0);
        }
        WM_COMMAND if lp.0 != 0 => {
            if let Err(error) = form.command(wp.0 as u16, (wp.0 >> 16) as u32) {
                let _ = form.refresh();
                form.error(error);
            }
            return LRESULT(0);
        }
        WM_TIMER if wp.0 == PROBE_TIMER => {
            let reading = form
                .probe
                .borrow()
                .as_ref()
                .map(|probe| (probe.level(), probe.finished()));
            if let Some((level, finished)) = reading {
                send(form.control(LEVEL), PBM_SETPOS, level, 0);
                if finished {
                    form.end_probe();
                }
            }
            return LRESULT(0);
        }
        WM_TIMER if wp.0 == REMOVE_TIMER => {
            if let Err(error) = form.poll_removal() {
                form.error(error);
            }
            return LRESULT(0);
        }
        WM_MOUSEWHEEL => {
            let delta = (wp.0 >> 16) as i16;
            form.scroll_by(-i32::from(delta) * scale(48, form.dpi.get()) / 120);
            return LRESULT(0);
        }
        WM_VSCROLL => {
            let line = scale(28, form.dpi.get());
            let page = form.viewport().bottom;
            let delta = match SCROLLBAR_COMMAND(i32::from(wp.0 as u16)) {
                SB_LINEUP => -line,
                SB_LINEDOWN => line,
                SB_PAGEUP => -page,
                SB_PAGEDOWN => page,
                SB_TOP => -form.scroll.get(),
                SB_BOTTOM => scale(CONTENT_HEIGHT, form.dpi.get()),
                SB_THUMBPOSITION | SB_THUMBTRACK => {
                    ((wp.0 >> 16) as u16) as i32 - form.scroll.get()
                }
                _ => 0,
            };
            form.scroll_by(delta);
            return LRESULT(0);
        }
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
}

pub(super) fn attach(window: &Window) -> tauri::Result<()> {
    let parent = window.hwnd()?;
    let init = INITCOMMONCONTROLSEX {
        dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_PROGRESS_CLASS,
    };
    if !unsafe { InitCommonControlsEx(&init) }.as_bool() {
        return Err(io::Error::last_os_error().into());
    }
    unsafe {
        let style = GetWindowLongPtrW(parent, GWL_STYLE);
        SetWindowLongPtrW(parent, GWL_STYLE, style | WS_VSCROLL.0 as isize);
        SetWindowPos(
            parent,
            None,
            0,
            0,
            0,
            0,
            SWP_NOSIZE | SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        )
        .map_err(io::Error::from)?;
    }
    for field in FIELDS.iter() {
        let (class, style) = match field.kind {
            Kind::Text => (w!("STATIC"), WINDOW_STYLE(SS_NOPREFIX.0)),
            Kind::Check => (w!("BUTTON"), WS_TABSTOP | WINDOW_STYLE(BS_CHECKBOX as u32)),
            Kind::Button => (
                w!("BUTTON"),
                WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32),
            ),
            Kind::Combo => (
                w!("COMBOBOX"),
                WS_TABSTOP | WS_VSCROLL | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
            ),
            Kind::Edit => (
                w!("EDIT"),
                WS_TABSTOP | WS_BORDER | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            ),
            Kind::Multiline => (
                w!("EDIT"),
                WS_TABSTOP
                    | WS_BORDER
                    | WS_VSCROLL
                    | WINDOW_STYLE((ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN) as u32),
            ),
            Kind::List => (
                w!("LISTBOX"),
                WS_TABSTOP
                    | WS_BORDER
                    | WS_VSCROLL
                    | WINDOW_STYLE((LBS_NOTIFY | LBS_NOINTEGRALHEIGHT) as u32),
            ),
            Kind::Meter => (w!("msctls_progress32"), WINDOW_STYLE(0)),
        };
        let visibility = if on_dictionary_page(field.id) {
            WINDOW_STYLE::default()
        } else {
            WS_VISIBLE
        };
        let child = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class,
                &HSTRING::from(field.text),
                WS_CHILD | visibility | style,
                0,
                0,
                0,
                0,
                Some(parent),
                Some(HMENU(usize::from(field.id) as _)),
                None,
                None,
            )
            .map_err(io::Error::from)?
        };
        if style.contains(WS_TABSTOP)
            && !unsafe { SetWindowSubclass(child, Some(child_proc), usize::from(field.id), 0) }
                .as_bool()
        {
            return Err(io::Error::last_os_error().into());
        }
    }
    let form = Rc::new(Form {
        hwnd: parent,
        app: window.app_handle().clone(),
        scroll: Cell::default(),
        dpi: Cell::default(),
        in_layout: Cell::default(),
        fonts: RefCell::default(),
        dictionary: dictionary::Page::default(),
        capturing: Cell::default(),
        chord: RefCell::default(),
        devices: RefCell::default(),
        model_ids: RefCell::default(),
        probe: RefCell::default(),
        removal: RefCell::default(),
    });
    form.initialize()?;
    if !unsafe { SetWindowSubclass(parent, Some(settings_proc), 1, Rc::as_ptr(&form) as usize) }
        .as_bool()
    {
        return Err(io::Error::last_os_error().into());
    }
    let _ = Rc::into_raw(form.clone());
    layout::layout(&form)?;
    Ok(())
}

#[cfg(test)]
mod tests;
