use crate::{comparison::DictationRecord, input::Target, native_ui::Font};
use serde::Deserialize;
use std::{
    cell::{Cell, RefCell},
    mem::size_of,
    rc::Rc,
    sync::{Arc, LazyLock},
};
use windows::{
    Win32::{
        Foundation::{
            E_FAIL, ERROR_CLASS_ALREADY_EXISTS, GetLastError, HMODULE, HWND, LPARAM, LRESULT, RECT,
            WPARAM,
        },
        Graphics::Gdi::{
            COLOR_BTNFACE, GetMonitorInfoW, HBRUSH, MONITOR_DEFAULTTONEAREST, MONITORINFO,
            MonitorFromWindow,
        },
        System::{
            LibraryLoader::{GetModuleHandleW, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW},
            SystemServices::{APPCOMMAND_COPY, APPCOMMAND_CUT},
        },
        UI::{
            Controls::RichEdit::{ES_NOOLEDRAGDROP, MSFTEDIT_CLASS},
            HiDpi::GetDpiForWindow,
            Input::KeyboardAndMouse::{
                EnableWindow, GetKeyState, SetFocus, VIRTUAL_KEY, VK_C, VK_CONTROL, VK_DELETE,
                VK_ESCAPE, VK_F11, VK_INSERT, VK_MENU, VK_RETURN, VK_SHIFT, VK_TAB, VK_X,
            },
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::*,
        },
    },
    core::{Error, HSTRING, Owned, PCWSTR, Result, w},
};

mod clipboard;
mod text;
use text::{Page, render_page};

const CLASS: PCWSTR = w!("QmsComparisonWindow");
const BEFORE_LABEL: usize = 1;
const BEFORE: usize = 2;
const AFTER_LABEL: usize = 3;
const AFTER: usize = 4;
const INSERT: usize = 5;
const COPY: usize = 6;
const PREVIOUS: usize = 7;
const NEXT: usize = 8;
const CLOSE_ON_BLUR: u32 = WM_APP + 1;
const COPY_FAILED: u32 = WM_APP + 2;
const COPY_SELECTION: u32 = WM_APP + 3;

pub(super) fn is_window(hwnd: HWND) -> bool {
    let mut name = [0; 256];
    let length = unsafe { GetClassNameW(hwnd, &mut name) } as usize;
    &name[..length] == unsafe { CLASS.as_wide() }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Labels {
    comparison_title: String,
    comparison_before: String,
    comparison_after: String,
    comparison_after_uncertain: String,
    comparison_insert: String,
    comparison_copy: String,
    comparison_previous: String,
    comparison_next: String,
    comparison_page: String,
}

static LABELS: LazyLock<Labels> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../ui/src/locales/de.json"))
        .expect("The embedded comparison labels must be valid")
});

type Insert = Box<dyn FnOnce(Target, String)>;

pub(super) struct PreparedComparison {
    record: Arc<DictationRecord>,
    before: Vec<Page>,
    after: Vec<Page>,
}

impl PreparedComparison {
    pub(super) fn new(record: Arc<DictationRecord>) -> Self {
        let comparison = record.comparison();
        let before = text::pages(&comparison.before);
        let after = text::pages(&comparison.after);
        Self {
            record,
            before,
            after,
        }
    }

    fn pages(&self) -> usize {
        self.before.len().max(self.after.len())
    }
}

struct Review {
    target: Target,
    view: PreparedComparison,
    page: Cell<usize>,
    insert: RefCell<Option<Insert>>,
    error: Box<dyn Fn(&str)>,
    closing: Cell<bool>,
    font: Font,
    _editor: Owned<HMODULE>,
}

#[derive(Clone, Copy)]
enum Action {
    Dismiss,
    Copy,
    Insert,
}
#[derive(Debug, PartialEq)]
enum Outcome {
    Close,
    Insert,
    TargetLost,
    CopyFailed,
}

fn finish(action: Action, copy: impl FnOnce() -> bool, restore: impl FnOnce() -> bool) -> Outcome {
    if matches!(action, Action::Copy) && !copy() {
        return Outcome::CopyFailed;
    }
    let restored = restore();
    match action {
        Action::Insert if restored => Outcome::Insert,
        Action::Insert => Outcome::TargetLost,
        _ => Outcome::Close,
    }
}

impl Review {
    fn attach(self: &Rc<Self>, hwnd: HWND) -> Result<()> {
        // The subclass owns one reference until WM_NCDESTROY.
        let pointer = Rc::into_raw(self.clone());
        unsafe {
            if SetWindowSubclass(hwnd, Some(parent_proc), 1, pointer as usize).as_bool() {
                Ok(())
            } else {
                drop(Rc::from_raw(pointer));
                Err(Error::from_thread())
            }
        }
    }

    fn finish(&self, hwnd: HWND, action: Action) {
        // Activation and destruction can synchronously reenter the window procedure.
        if self.closing.replace(true) {
            return;
        }
        let outcome = finish(
            action,
            || {
                quatsch_mit_sosse_inject::copy_recovered_text(hwnd, &self.view.record.original)
                    .is_ok()
            },
            || self.target.restore(hwnd).is_ok(),
        );
        if matches!(outcome, Outcome::CopyFailed | Outcome::TargetLost) {
            self.closing.set(false);
            (self.error)(if outcome == Outcome::CopyFailed {
                &crate::messages().overlay_copy_error
            } else {
                &crate::messages().target_unavailable
            });
            return;
        }
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        if outcome == Outcome::Insert
            && let Some(insert) = self.insert.take()
        {
            insert(self.target, self.view.record.original.clone());
        }
    }

    fn display_page(&self, hwnd: HWND, page: usize) -> Result<()> {
        let pages = self.view.pages();
        let empty = Page::default();
        unsafe {
            render_page(
                GetDlgItem(Some(hwnd), BEFORE as i32)?,
                self.view.before.get(page).unwrap_or(&empty),
            )?;
            render_page(
                GetDlgItem(Some(hwnd), AFTER as i32)?,
                self.view.after.get(page).unwrap_or(&empty),
            )?;
            if pages > 1 {
                let title = LABELS
                    .comparison_page
                    .replace("{page}", &(page + 1).to_string())
                    .replace("{count}", &pages.to_string());
                SetWindowTextW(hwnd, &HSTRING::from(title))?;
                let _ = EnableWindow(GetDlgItem(Some(hwnd), PREVIOUS as i32)?, page > 0);
                let _ = EnableWindow(GetDlgItem(Some(hwnd), NEXT as i32)?, page + 1 < pages);
            }
        }
        self.page.set(page);
        Ok(())
    }
}

fn send(hwnd: HWND, message: u32, wp: usize, lp: isize) -> LRESULT {
    unsafe { SendMessageW(hwnd, message, Some(WPARAM(wp)), Some(LPARAM(lp))) }
}

fn scale(value: i32, dpi: u32) -> i32 {
    (i64::from(value) * i64::from(dpi.max(96)) / 96) as i32
}

fn placement(anchor: RECT, work: RECT, dpi: u32) -> RECT {
    let width = scale(584, dpi).min(work.right - work.left);
    let height = scale(290, dpi).min(work.bottom - work.top);
    let gap = scale(8, dpi);
    let left = (anchor.left + (anchor.right - anchor.left - width) / 2)
        .clamp(work.left, work.right - width);
    let above = anchor.top - gap - height;
    let top = if above >= work.top {
        above
    } else {
        anchor.bottom + gap
    }
    .clamp(work.top, work.bottom - height);
    RECT {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

fn window_bounds(overlay: HWND) -> Result<RECT> {
    let mut anchor = RECT::default();
    let mut monitor = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe {
        GetWindowRect(overlay, &mut anchor)?;
        GetMonitorInfoW(
            MonitorFromWindow(overlay, MONITOR_DEFAULTTONEAREST),
            &mut monitor,
        )
        .ok()?;
        Ok(placement(anchor, monitor.rcWork, GetDpiForWindow(overlay)))
    }
}

fn load_editor() -> Result<Owned<HMODULE>> {
    unsafe {
        Ok(Owned::new(LoadLibraryExW(
            w!("Msftedit.dll"),
            None,
            LOAD_LIBRARY_SEARCH_SYSTEM32,
        )?))
    }
}

fn create_window(bounds: RECT) -> Result<HWND> {
    unsafe extern "system" fn default_proc(
        hwnd: HWND,
        msg: u32,
        wp: WPARAM,
        lp: LPARAM,
    ) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
    }
    unsafe {
        let instance = GetModuleHandleW(None)?.into();
        let class = WNDCLASSW {
            lpszClassName: CLASS,
            hInstance: instance,
            lpfnWndProc: Some(default_proc),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hbrBackground: HBRUSH((COLOR_BTNFACE.0 + 1) as _),
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            return Err(Error::from_thread());
        }
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            CLASS,
            &HSTRING::from(&LABELS.comparison_title),
            WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN,
            bounds.left,
            bounds.top,
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            None,
            None,
            Some(instance),
            None,
        )
    }
}

fn child(
    parent: HWND,
    id: usize,
    class: PCWSTR,
    title: &str,
    style: WINDOW_STYLE,
    font: &Font,
) -> Result<HWND> {
    unsafe {
        let hwnd = CreateWindowExW(
            Default::default(),
            class,
            &HSTRING::from(title),
            WS_CHILD | WS_VISIBLE | style,
            0,
            0,
            1,
            1,
            Some(parent),
            Some(HMENU(id as _)),
            None,
            None,
        )?;
        send(hwnd, WM_SETFONT, font.0, 0);
        if style.contains(WS_TABSTOP) {
            SetWindowSubclass(hwnd, Some(child_proc), id, 0).ok()?;
        }
        Ok(hwnd)
    }
}

fn contents(hwnd: HWND, review: &Review) -> Result<()> {
    let font = &review.font;
    let edit = WS_TABSTOP
        | WS_BORDER
        | WS_VSCROLL
        | WINDOW_STYLE((ES_MULTILINE | ES_READONLY) as u32 | ES_NOOLEDRAGDROP);
    child(
        hwnd,
        BEFORE_LABEL,
        w!("STATIC"),
        &LABELS.comparison_before,
        WINDOW_STYLE(0),
        font,
    )?;
    child(hwnd, BEFORE, MSFTEDIT_CLASS, "", edit, font)?;
    let caption = if review.view.record.uncertain.is_empty() {
        &LABELS.comparison_after
    } else {
        &LABELS.comparison_after_uncertain
    };
    child(
        hwnd,
        AFTER_LABEL,
        w!("STATIC"),
        caption,
        WINDOW_STYLE(0),
        font,
    )?;
    child(hwnd, AFTER, MSFTEDIT_CLASS, "", edit, font)?;
    if review.view.pages() > 1 {
        child(
            hwnd,
            PREVIOUS,
            w!("BUTTON"),
            &LABELS.comparison_previous,
            WS_TABSTOP,
            font,
        )?;
        child(
            hwnd,
            NEXT,
            w!("BUTTON"),
            &LABELS.comparison_next,
            WS_TABSTOP,
            font,
        )?;
    }
    child(
        hwnd,
        INSERT,
        w!("BUTTON"),
        &LABELS.comparison_insert,
        WS_TABSTOP,
        font,
    )?;
    child(
        hwnd,
        COPY,
        w!("BUTTON"),
        &LABELS.comparison_copy,
        WS_TABSTOP,
        font,
    )?;
    review.display_page(hwnd, 0)?;
    layout(hwnd)
}

fn layout(hwnd: HWND) -> Result<()> {
    let mut client = RECT::default();
    unsafe {
        GetClientRect(hwnd, &mut client)?;
    }
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let gap = scale(10, dpi);
    let pane_width = ((client.right - 3 * gap) / 2).max(1);
    let second = pane_width + 2 * gap;
    let heading = scale(24, dpi);
    let button = scale(32, dpi);
    let button_y = client.bottom - gap - button;
    let pane_y = gap + heading;
    let pane_height = (button_y - gap - pane_y).max(1);
    let previous = unsafe { GetDlgItem(Some(hwnd), PREVIOUS as i32) };
    let available = (client.right - 5 * gap).max(4);
    let button_width = if previous.is_ok() {
        (available / 3).min(scale(150, dpi))
    } else {
        pane_width.min(scale(150, dpi))
    };
    if let Ok(previous) = previous {
        let nav_width = ((available - 2 * button_width) / 2).max(1);
        unsafe {
            MoveWindow(previous, gap, button_y, nav_width, button, true)?;
            MoveWindow(
                GetDlgItem(Some(hwnd), NEXT as i32)?,
                2 * gap + nav_width,
                button_y,
                nav_width,
                button,
                true,
            )?;
        }
    }
    for (id, x, y, width, height) in [
        (BEFORE_LABEL, gap, gap, pane_width, heading),
        (AFTER_LABEL, second, gap, pane_width, heading),
        (BEFORE, gap, pane_y, pane_width, pane_height),
        (AFTER, second, pane_y, pane_width, pane_height),
        (
            INSERT,
            client.right - 2 * (button_width + gap),
            button_y,
            button_width,
            button,
        ),
        (
            COPY,
            client.right - gap - button_width,
            button_y,
            button_width,
            button,
        ),
    ] {
        unsafe {
            MoveWindow(
                GetDlgItem(Some(hwnd), id as i32)?,
                x,
                y,
                width,
                height,
                true,
            )?;
        }
    }
    Ok(())
}

/// Called on the main thread with the foreground identity captured by the shortcut.
pub(super) fn show(
    overlay: HWND,
    hide_from_capture: bool,
    view: PreparedComparison,
    target: Target,
    on_insert: Insert,
    on_error: Box<dyn Fn(&str)>,
) -> Result<()> {
    let editor = load_editor()?;
    let font = Font::create(15, 400, unsafe { GetDpiForWindow(overlay) })
        .map_err(|_| Error::from(E_FAIL))?;
    let hwnd = create_window(window_bounds(overlay)?)?;
    let review = Rc::new(Review {
        target,
        view,
        page: Cell::new(0),
        insert: RefCell::new(Some(on_insert)),
        error: on_error,
        closing: Cell::new(false),
        font,
        _editor: editor,
    });
    let result = (|| unsafe {
        contents(hwnd, &review)?;
        if hide_from_capture {
            SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)?;
        }
        review.attach(hwnd)?;
        target.check_foreground().map_err(|_| Error::from(E_FAIL))?;
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        if !SetForegroundWindow(hwnd).as_bool() || GetForegroundWindow() != hwnd {
            return Err(Error::from(E_FAIL));
        }
        let _ = SetFocus(Some(GetDlgItem(Some(hwnd), BEFORE as i32)?));
        Ok(())
    })();
    if result.is_err() {
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
    result
}

unsafe extern "system" fn child_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    id: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        if msg == WM_NCDESTROY {
            let _ = RemoveWindowSubclass(hwnd, Some(child_proc), id);
        } else if matches!(id, BEFORE | AFTER) && clipboard::handle(hwnd, id, msg, wp, lp) {
            return LRESULT(1);
        }
        if msg == WM_KEYDOWN
            && let Ok(parent) = GetParent(hwnd)
        {
            match wp.0 as u16 {
                key if key == VK_ESCAPE.0 => {
                    let _ = PostMessageW(Some(parent), WM_CLOSE, WPARAM(0), LPARAM(0));
                    return LRESULT(0);
                }
                key if key == VK_TAB.0 => {
                    let previous = GetKeyState(i32::from(VK_SHIFT.0)) < 0;
                    if let Ok(next) = GetNextDlgTabItem(parent, Some(hwnd), previous) {
                        let _ = SetFocus(Some(next));
                    }
                    return LRESULT(0);
                }
                key if key == VK_RETURN.0 && matches!(id, COPY | INSERT | PREVIOUS | NEXT) => {
                    send(hwnd, BM_CLICK, 0, 0);
                    return LRESULT(0);
                }
                _ => {}
            }
        }
        DefSubclassProc(hwnd, msg, wp, lp)
    }
}

unsafe extern "system" fn parent_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT {
    let pointer = data as *const Review;
    unsafe {
        Rc::increment_strong_count(pointer);
        let review = Rc::from_raw(pointer);
        match msg {
            WM_NCDESTROY => {
                let _ = RemoveWindowSubclass(hwnd, Some(parent_proc), id);
                drop(Rc::from_raw(pointer));
            }
            WM_CLOSE | WM_KEYDOWN if msg == WM_CLOSE || wp.0 == usize::from(VK_ESCAPE.0) => {
                review.finish(hwnd, Action::Dismiss);
                return LRESULT(0);
            }
            WM_ACTIVATE if (wp.0 as u16) == WA_INACTIVE as u16 && !review.closing.get() => {
                let _ = PostMessageW(Some(hwnd), CLOSE_ON_BLUR, WPARAM(0), LPARAM(0));
            }
            CLOSE_ON_BLUR if GetForegroundWindow() != hwnd => {
                // The user chose a different window. Never reactivate the previous editor here.
                let _ = DestroyWindow(hwnd);
                return LRESULT(0);
            }
            COPY_FAILED => {
                (review.error)(&crate::messages().overlay_copy_error);
                return LRESULT(0);
            }
            COPY_SELECTION if matches!(wp.0, BEFORE | AFTER) => {
                if GetDlgItem(Some(hwnd), wp.0 as i32)
                    .and_then(clipboard::copy_selection)
                    .is_err()
                {
                    (review.error)(&crate::messages().overlay_copy_error);
                }
                return LRESULT(0);
            }
            WM_COMMAND if matches!(wp.0, PREVIOUS | NEXT) => {
                let page = review.page.get();
                let page = if wp.0 == PREVIOUS {
                    page.saturating_sub(1)
                } else {
                    (page + 1).min(review.view.pages() - 1)
                };
                if review.display_page(hwnd, page).is_err() {
                    (review.error)(&crate::messages().comparison_open_error);
                }
                return LRESULT(0);
            }
            WM_COMMAND if wp.0 >> 16 == BN_CLICKED as usize && matches!(wp.0, COPY | INSERT) => {
                review.finish(
                    hwnd,
                    if wp.0 == COPY {
                        Action::Copy
                    } else {
                        Action::Insert
                    },
                );
                return LRESULT(0);
            }
            WM_SIZE => {
                let _ = layout(hwnd);
            }
            _ => {}
        }
        DefSubclassProc(hwnd, msg, wp, lp)
    }
}

#[cfg(test)]
mod tests;
