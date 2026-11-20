use std::{io, sync::Mutex, thread::ThreadId};
use tauri::{AppHandle, Manager, Window, WindowEvent};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, DEFAULT_CHARSET, DEFAULT_GUI_FONT,
            DEFAULT_PITCH, DeleteObject, FF_DONTCARE, GetStockObject, HGDIOBJ, OUT_DEFAULT_PRECIS,
        },
        System::SystemServices::SS_NOPREFIX,
        UI::{
            HiDpi::GetDpiForWindow,
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::{
                BM_SETCHECK, BN_CLICKED, BS_CHECKBOX, CreateWindowExW, DestroyWindow,
                GetClientRect, HMENU, MB_ICONERROR, MB_OK, MessageBoxW, SWP_NOACTIVATE,
                SWP_NOZORDER, SendMessageW, SetWindowPos, WINDOW_EX_STYLE, WINDOW_STYLE,
                WM_COMMAND, WM_NCDESTROY, WM_SETFONT, WS_CHILD, WS_TABSTOP, WS_VISIBLE,
            },
        },
    },
    core::{HSTRING, w},
};

const SHOW_READY: u16 = 101;
const HIDE_FROM_CAPTURE: u16 = 102;

#[derive(Clone, Copy, Debug)]
struct Label<'a> {
    text: &'a str,
    size: i32,
    weight: i32,
    checkbox: Option<u16>,
}

fn settings_controls(text: &crate::Messages) -> [Label<'_>; 5] {
    let label = |text, size, weight, checkbox| Label {
        text,
        size,
        weight,
        checkbox,
    };
    [
        label(&text.app_name, 14, 400, None),
        label(&text.settings, 24, 600, None),
        label(&text.settings_placeholder, 14, 400, None),
        label(&text.overlay_show_ready, 14, 400, Some(SHOW_READY)),
        label(
            &text.overlay_hide_from_capture,
            14,
            400,
            Some(HIDE_FROM_CAPTURE),
        ),
    ]
}

#[derive(Debug, PartialEq)]
struct Rect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

fn scale(value: i32, dpi: u32) -> i32 {
    (value as i64 * dpi.max(96) as i64 / 96) as i32
}

fn layout(width: i32, height: i32, dpi: u32) -> [Rect; 5] {
    let width = width.max(0);
    let height = height.max(0);
    let margin = scale(24, dpi);
    let x = margin.min(width / 2);
    let box_width = (width - 2 * x).max(0);
    let rect = |y: i32, box_height: i32| {
        let y = scale(y, dpi).min(height);
        Rect {
            x,
            y,
            width: box_width,
            height: scale(box_height, dpi).min(height - y).max(0),
        }
    };
    [
        rect(24, 24),
        rect(62, 36),
        rect(110, 44),
        rect(158, 32),
        rect(196, 32),
    ]
}

fn native_error() -> io::Error {
    io::Error::other("The native window content could not be initialized")
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
            return Err(native_error());
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

struct Control {
    handle: usize,
    font: Font,
    size: i32,
    weight: i32,
}

impl Control {
    fn set_font(&self, font: &Font) {
        unsafe {
            SendMessageW(
                HWND(self.handle as _),
                WM_SETFONT,
                Some(WPARAM(font.0)),
                Some(LPARAM(1)),
            );
        }
    }
}

struct NativeControls {
    owner: ThreadId,
    parent: usize,
    dpi: u32,
    controls: Vec<Control>,
    parent_owned: bool,
}

impl NativeControls {
    fn detach_fonts(&self) {
        // Tao emits Destroyed at WM_DESTROY, when child HWNDs can still exist.
        // Select a system-owned font before releasing our own GDI objects.
        let stock_font = unsafe { GetStockObject(DEFAULT_GUI_FONT) };
        for control in &self.controls {
            unsafe {
                SendMessageW(
                    HWND(control.handle as _),
                    WM_SETFONT,
                    Some(WPARAM(stock_font.0 as usize)),
                    Some(LPARAM(0)),
                );
            }
        }
    }

    fn relayout(&mut self) -> io::Result<()> {
        if self.owner != std::thread::current().id() {
            return Err(native_error());
        }
        let parent = HWND(self.parent as _);
        let mut bounds = RECT::default();
        unsafe { GetClientRect(parent, &mut bounds) }.map_err(|_| native_error())?;
        let dpi = unsafe { GetDpiForWindow(parent) }.max(96);
        if dpi != self.dpi {
            // Prepare every replacement before touching controls or releasing old fonts.
            let fonts = self
                .controls
                .iter()
                .map(|control| Font::create(control.size, control.weight, dpi))
                .collect::<io::Result<Vec<_>>>()?;
            for (control, font) in self.controls.iter_mut().zip(fonts) {
                control.set_font(&font);
                control.font = font;
            }
            self.dpi = dpi;
        }
        for (control, bounds) in self.controls.iter().zip(layout(
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            dpi,
        )) {
            unsafe {
                SetWindowPos(
                    HWND(control.handle as _),
                    None,
                    bounds.x,
                    bounds.y,
                    bounds.width,
                    bounds.height,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                )
            }
            .map_err(|_| native_error())?;
        }
        Ok(())
    }
}

impl Drop for NativeControls {
    fn drop(&mut self) {
        // On initialization failure, remove children before dropping their fonts.
        // Successful children belong to the parent; Destroyed first detaches fonts.
        if !self.parent_owned {
            for control in &self.controls {
                unsafe {
                    let _ = DestroyWindow(HWND(control.handle as _));
                }
            }
        }
    }
}

unsafe extern "system" fn settings_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT {
    if msg == WM_NCDESTROY {
        unsafe {
            let _ = RemoveWindowSubclass(hwnd, Some(settings_proc), id);
            drop(Box::from_raw(data as *mut AppHandle));
        }
    } else if msg == WM_COMMAND && (wp.0 >> 16) as u32 == BN_CLICKED && lp.0 != 0 {
        let control = wp.0 as u16;
        if matches!(control, SHOW_READY | HIDE_FROM_CAPTURE) {
            // Clone before calling Win32 again: a nested message may destroy the window.
            let app = unsafe { (&*(data as *const AppHandle)).clone() };
            match crate::overlay::toggle_preference(&app, control == SHOW_READY) {
                Ok(checked) => unsafe {
                    SendMessageW(
                        HWND(lp.0 as _),
                        BM_SETCHECK,
                        Some(WPARAM(usize::from(checked))),
                        None,
                    );
                },
                Err(_) => {
                    let text = crate::messages();
                    unsafe {
                        MessageBoxW(
                            Some(hwnd),
                            &HSTRING::from(&text.overlay_preferences_error),
                            &HSTRING::from(&text.app_name),
                            MB_OK | MB_ICONERROR,
                        );
                    }
                }
            }
            return LRESULT(0);
        }
    }
    unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
}

/// Attach controls and their command handler on the owning window thread.
pub(super) fn attach(window: &Window) -> tauri::Result<()> {
    let parent = window.hwnd()?;
    let dpi = unsafe { GetDpiForWindow(parent) }.max(96);
    let options = crate::overlay::preferences(window.app_handle())?;
    let mut native = NativeControls {
        owner: std::thread::current().id(),
        parent: parent.0 as usize,
        dpi,
        controls: Vec::new(),
        parent_owned: false,
    };
    for label in settings_controls(crate::messages()) {
        let font = Font::create(label.size, label.weight, dpi)?;
        let (class, style) = if label.checkbox.is_some() {
            (w!("BUTTON"), WS_TABSTOP | WINDOW_STYLE(BS_CHECKBOX as u32))
        } else {
            (w!("STATIC"), WINDOW_STYLE(SS_NOPREFIX.0))
        };
        let handle = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class,
                &HSTRING::from(label.text),
                WS_CHILD | WS_VISIBLE | style,
                0,
                0,
                0,
                0,
                Some(parent),
                label.checkbox.map(|id| HMENU(usize::from(id) as _)),
                None,
                None,
            )
        }
        .map_err(|_| native_error())?;
        if let Some(id) = label.checkbox {
            let checked = if id == SHOW_READY {
                options.show_ready
            } else {
                options.hide_from_capture
            };
            unsafe {
                SendMessageW(
                    handle,
                    BM_SETCHECK,
                    Some(WPARAM(usize::from(checked))),
                    None,
                )
            };
        }
        let control = Control {
            handle: handle.0 as usize,
            font,
            size: label.size,
            weight: label.weight,
        };
        control.set_font(&control.font);
        native.controls.push(control);
    }
    native.relayout()?;
    let app = Box::new(window.app_handle().clone());
    if !unsafe {
        SetWindowSubclass(
            parent,
            Some(settings_proc),
            1,
            (&*app as *const AppHandle) as usize,
        )
    }
    .as_bool()
    {
        return Err(native_error().into());
    }
    // The subclass takes ownership until WM_NCDESTROY.
    let _ = Box::into_raw(app);
    native.parent_owned = true;
    let native = Mutex::new(Some(native));
    window.on_window_event(move |event| {
        if let Ok(mut state) = native.lock() {
            match event {
                WindowEvent::Destroyed => {
                    if let Some(state) = state.as_ref() {
                        state.detach_fonts();
                    }
                    // Child HWNDs are parent-owned; this releases only our GDI fonts.
                    state.take();
                }
                WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                    if let Some(state) = state.as_mut() {
                        // A simultaneous native close can make the last resize obsolete.
                        let _ = state.relayout();
                    }
                }
                _ => {}
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_settings_preserve_all_catalog_texts() {
        let text = crate::messages();
        let labels = settings_controls(text);
        assert_eq!(
            labels.iter().map(|label| label.text).collect::<Vec<_>>(),
            [
                &text.app_name,
                &text.settings,
                &text.settings_placeholder,
                &text.overlay_show_ready,
                &text.overlay_hide_from_capture
            ]
        );
    }

    #[test]
    fn minimum_window_layouts_remain_inside_client_area_at_each_dpi() {
        for dpi in [96, 120, 144, 192] {
            let width = scale(400, dpi);
            let height = scale(300, dpi);
            let boxes = layout(width, height, dpi);
            assert_eq!(boxes.len(), 5);
            assert!(boxes.windows(2).all(|r| r[0].y + r[0].height <= r[1].y));
            for rect in boxes {
                assert!(rect.x >= 0 && rect.y >= 0);
                assert!(rect.width > 0 && rect.height > 0);
                assert!(rect.x + rect.width <= width);
                assert!(rect.y + rect.height <= height);
            }
        }
    }
}
