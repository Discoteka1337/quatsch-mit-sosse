use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::{
            Dwm::{DWM_BB_ENABLE, DWM_BLURBEHIND, DwmEnableBlurBehindWindow},
            Gdi::{
                ClientToScreen, CreateEllipticRgnIndirect, CreateRoundRectRgn, DeleteObject,
                InvalidateRect, SetWindowRgn,
            },
        },
        System::Threading::GetCurrentThreadId,
        UI::{
            HiDpi::GetDpiForWindow,
            Input::KeyboardAndMouse::{
                GetAsyncKeyState, GetCapture, ReleaseCapture, SetCapture, VK_LBUTTON, VK_RBUTTON,
            },
            Shell::{DefSubclassProc, GetWindowSubclass, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::*,
        },
    },
    core::{Error, Result, w},
};

mod paint;

#[derive(Clone, PartialEq)]
pub(crate) enum View {
    Hidden,
    Ready,
    Listening { label: String, level: u8 },
    Status(String),
    Error { label: String, copy: String },
    Administrator { label: String, detail: String },
}

impl View {
    pub(crate) fn logical_size(&self) -> (i32, i32) {
        match self {
            Self::Error { .. } => (312, 76),
            Self::Administrator { .. } => (312, 80),
            _ => (160, 36),
        }
    }
}

pub(crate) struct Callbacks {
    pub copied: Box<dyn Fn()>,
    pub dragged: Box<dyn Fn()>,
    pub display_changed: Box<dyn Fn()>,
}

#[derive(Clone, Copy, PartialEq)]
enum Gesture {
    Idle,
    Copy,
    Drag {
        offset: POINT,
        start_dpi: u32,
        moved: bool,
    },
}

struct Overlay {
    view: RefCell<Rc<View>>,
    gesture: Cell<Gesture>,
    shape: Cell<Option<(bool, i32, i32, u32)>>,
    display_pending: Cell<bool>,
    palette: Cell<paint::Palette>,
    font: RefCell<Option<(u32, crate::native_ui::Font)>>,
    callbacks: Callbacks,
}

const SUBCLASS_ID: usize = 0x514d53;
const DRAG_TIMER: usize = SUBCLASS_ID;
const REFRESH_DISPLAY: u32 = WM_APP + 0x351;

/// Called on the UI thread while the Tauri window is still hidden.
pub(crate) fn attach(hwnd: HWND, callbacks: Callbacks) -> Result<()> {
    unsafe {
        if GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_NOACTIVATE.0 as isize == 0 {
            return Err(Error::new(
                windows::Win32::Foundation::E_FAIL,
                "Focusable overlay",
            ));
        }
        if GetWindowSubclass(hwnd, Some(window_proc), SUBCLASS_ID, None).as_bool() {
            return Err(Error::new(
                windows::Win32::Foundation::E_FAIL,
                "Overlay already attached",
            ));
        }
        // The config requests transparency. The region below provides it outside
        // the bar, while disabling Tao's DWM blur keeps opaque GDI text legible.
        DwmEnableBlurBehindWindow(
            hwnd,
            &DWM_BLURBEHIND {
                dwFlags: DWM_BB_ENABLE,
                fEnable: false.into(),
                ..Default::default()
            },
        )?;
        let overlay = Rc::new(Overlay {
            view: RefCell::new(Rc::new(View::Hidden)),
            gesture: Cell::new(Gesture::Idle),
            shape: Cell::new(None),
            display_pending: Cell::new(false),
            palette: Cell::new(paint::Palette::system()),
            font: RefCell::new(None),
            callbacks,
        });
        let pointer = Rc::into_raw(overlay);
        if !SetWindowSubclass(hwnd, Some(window_proc), SUBCLASS_ID, pointer as usize).as_bool() {
            drop(Rc::from_raw(pointer));
            return Err(Error::from_thread());
        }
    }
    Ok(())
}

fn state(hwnd: HWND) -> Result<Rc<Overlay>> {
    let mut pointer = 0;
    unsafe {
        if GetWindowThreadProcessId(hwnd, None) != GetCurrentThreadId()
            || !GetWindowSubclass(hwnd, Some(window_proc), SUBCLASS_ID, Some(&mut pointer))
                .as_bool()
        {
            return Err(Error::new(
                windows::Win32::Foundation::E_FAIL,
                "Overlay unavailable on this thread",
            ));
        }
        Ok(retain(pointer))
    }
}

// The subclass owns one strong reference until WM_NCDESTROY. Keep a second one
// for each active call, as native messages (including destruction) can reenter.
unsafe fn retain(pointer: usize) -> Rc<Overlay> {
    let pointer = pointer as *const Overlay;
    unsafe {
        Rc::increment_strong_count(pointer);
        Rc::from_raw(pointer)
    }
}

pub(crate) fn render(hwnd: HWND, view: View) -> Result<()> {
    let overlay = state(hwnd)?;
    let visible = !matches!(view, View::Hidden);
    overlay.view.replace(Rc::new(view));
    unsafe {
        if visible {
            overlay.update_shape(hwnd)?;
            InvalidateRect(Some(hwnd), None, false).ok()?;
        }
        if visible != IsWindowVisible(hwnd).as_bool() {
            let visibility = if visible {
                SWP_SHOWWINDOW
            } else {
                SWP_HIDEWINDOW
            };
            SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | visibility,
            )?;
        }
    }
    Ok(())
}

pub(crate) fn set_capture_hidden(hwnd: HWND, hide: bool) -> Result<()> {
    let affinity = if hide {
        WDA_EXCLUDEFROMCAPTURE
    } else {
        WDA_NONE
    };
    unsafe { SetWindowDisplayAffinity(hwnd, affinity) }
}

fn client_rect(hwnd: HWND) -> Result<RECT> {
    let mut bounds = RECT::default();
    unsafe { GetClientRect(hwnd, &mut bounds) }?;
    Ok(bounds)
}

fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
    RECT {
        left,
        top,
        right,
        bottom,
    }
}

fn px(logical: i32, dpi: u32) -> i32 {
    (i64::from(logical) * i64::from(dpi.max(96)) / 96) as i32
}

fn copy_bounds(client: RECT, dpi: u32) -> RECT {
    rect(
        client.right - px(84, dpi),
        client.bottom - px(32, dpi),
        client.right - px(8, dpi),
        client.bottom - px(6, dpi),
    )
}

fn inside(bounds: RECT, point: POINT) -> bool {
    (bounds.left..bounds.right).contains(&point.x) && (bounds.top..bounds.bottom).contains(&point.y)
}

fn lit_bars(level: u8) -> usize {
    usize::from(level.min(100)).div_ceil(10)
}

fn message_point(lparam: LPARAM) -> POINT {
    POINT {
        x: lparam.0 as i16 as i32,
        y: (lparam.0 >> 16) as i16 as i32,
    }
}

fn primary_button_down() -> bool {
    unsafe {
        let key = if GetSystemMetrics(SM_SWAPBUTTON) == 0 {
            VK_LBUTTON
        } else {
            VK_RBUTTON
        };
        GetAsyncKeyState(i32::from(key.0)) < 0
    }
}

impl Overlay {
    fn update_shape(&self, hwnd: HWND) -> Result<()> {
        let bounds = client_rect(hwnd)?;
        let dpi = unsafe { GetDpiForWindow(hwnd) };
        let ready = matches!(self.view.borrow().as_ref(), View::Ready);
        let key = (ready, bounds.right, bounds.bottom, dpi);
        if self.shape.get() == Some(key) || bounds.right < 1 || bounds.bottom < 1 {
            return Ok(());
        }
        unsafe {
            let region = if ready {
                let radius = px(6, dpi);
                let (cx, cy) = (bounds.right / 2, bounds.bottom / 2);
                CreateEllipticRgnIndirect(&rect(cx - radius, cy - radius, cx + radius, cy + radius))
            } else {
                let corner = px(16, dpi);
                CreateRoundRectRgn(0, 0, bounds.right + 1, bounds.bottom + 1, corner, corner)
            };
            if region.is_invalid() {
                return Err(Error::from_thread());
            }
            self.shape.set(Some(key));
            if SetWindowRgn(hwnd, Some(region), true) == 0 {
                self.shape.set(None);
                let error = Error::from_thread();
                let _ = DeleteObject(region.into());
                return Err(error);
            }
            // The window manager owns the region after success.
        }
        Ok(())
    }

    fn over_copy(&self, hwnd: HWND, client: POINT) -> bool {
        matches!(self.view.borrow().as_ref(), View::Error { .. })
            && client_rect(hwnd).is_ok_and(|bounds| {
                inside(
                    copy_bounds(bounds, unsafe { GetDpiForWindow(hwnd) }),
                    client,
                )
            })
    }

    fn press(&self, hwnd: HWND, client: POINT) {
        let gesture = if self.over_copy(hwnd, client) {
            Gesture::Copy
        } else {
            Gesture::Drag {
                offset: client,
                start_dpi: unsafe { GetDpiForWindow(hwnd) }.max(96),
                moved: false,
            }
        };
        self.gesture.set(gesture);
        unsafe {
            SetCapture(hwnd);
            // Background windows only receive captured input while under the
            // pointer. Track fast motions outside the bar until release.
            if matches!(gesture, Gesture::Drag { .. }) && primary_button_down() {
                SetTimer(Some(hwnd), DRAG_TIMER, 16, None);
            }
        }
    }

    fn move_to(&self, hwnd: HWND, cursor: POINT) {
        if let Gesture::Drag {
            offset, start_dpi, ..
        } = self.gesture.get()
        {
            let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
            let scaled = |n| (i64::from(n) * i64::from(dpi) / i64::from(start_dpi)) as i32;
            let (x, y) = (cursor.x - scaled(offset.x), cursor.y - scaled(offset.y));
            let mut current = RECT::default();
            unsafe {
                if GetWindowRect(hwnd, &mut current).is_ok()
                    && (current.left, current.top) != (x, y)
                {
                    self.gesture.set(Gesture::Drag {
                        offset,
                        start_dpi,
                        moved: true,
                    });
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        x,
                        y,
                        0,
                        0,
                        SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOSIZE,
                    );
                }
            }
        }
    }

    fn release(&self, hwnd: HWND, client: Option<POINT>) {
        let gesture = self.gesture.replace(Gesture::Idle);
        if gesture == Gesture::Idle {
            return;
        }
        unsafe {
            let _ = KillTimer(Some(hwnd), DRAG_TIMER);
            if GetCapture() == hwnd {
                let _ = ReleaseCapture();
            }
        }
        match gesture {
            Gesture::Copy if client.is_some_and(|point| self.over_copy(hwnd, point)) => {
                (self.callbacks.copied)()
            }
            Gesture::Drag { moved: true, .. } => (self.callbacks.dragged)(),
            _ => {}
        }
    }

    fn schedule_display(&self, hwnd: HWND) {
        if self.gesture.get() == Gesture::Idle
            && !self.display_pending.replace(true)
            && unsafe { PostMessageW(Some(hwnd), REFRESH_DISPLAY, WPARAM(0), LPARAM(0)) }.is_err()
        {
            self.display_pending.set(false);
        }
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    data: usize,
) -> LRESULT {
    let overlay = unsafe { retain(data) };
    unsafe {
        match msg {
            WM_NCDESTROY => {
                let _ = RemoveWindowSubclass(hwnd, Some(window_proc), SUBCLASS_ID);
                drop(Rc::from_raw(data as *const Overlay));
            }
            WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
            WM_ERASEBKGND => return LRESULT(1),
            WM_PAINT => {
                overlay.paint(hwnd);
                return LRESULT(0);
            }
            WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
                overlay.press(hwnd, message_point(lparam));
                return LRESULT(0);
            }
            WM_MOUSEMOVE if matches!(overlay.gesture.get(), Gesture::Drag { .. }) => {
                let mut point = message_point(lparam);
                if ClientToScreen(hwnd, &mut point).as_bool() {
                    overlay.move_to(hwnd, point);
                }
                return LRESULT(0);
            }
            WM_TIMER if wparam.0 == DRAG_TIMER => {
                let mut point = POINT::default();
                if GetCursorPos(&mut point).is_ok() {
                    overlay.move_to(hwnd, point);
                }
                if !primary_button_down() {
                    overlay.release(hwnd, None);
                }
                return LRESULT(0);
            }
            WM_LBUTTONUP => {
                overlay.release(hwnd, Some(message_point(lparam)));
                return LRESULT(0);
            }
            WM_CAPTURECHANGED | WM_CANCELMODE => overlay.release(hwnd, None),
            WM_SIZE => {
                let _ = overlay.update_shape(hwnd);
            }
            WM_DPICHANGED | WM_DISPLAYCHANGE => overlay.schedule_display(hwnd),
            WM_SETTINGCHANGE | WM_THEMECHANGED => {
                overlay.palette.set(paint::Palette::system());
                let _ = InvalidateRect(Some(hwnd), None, false);
                if msg == WM_SETTINGCHANGE && wparam.0 == SPI_SETWORKAREA.0 as usize {
                    overlay.schedule_display(hwnd);
                }
            }
            REFRESH_DISPLAY => {
                overlay.display_pending.set(false);
                if overlay.gesture.get() == Gesture::Idle {
                    (overlay.callbacks.display_changed)();
                }
                return LRESULT(0);
            }
            _ => {}
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }
}

#[cfg(test)]
mod tests;
