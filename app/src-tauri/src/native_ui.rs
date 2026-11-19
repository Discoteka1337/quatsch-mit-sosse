use std::{io, sync::Mutex, thread::ThreadId};
use tauri::{Runtime, Window, WindowEvent};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, RECT, WPARAM},
        Graphics::Gdi::{
            CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, DEFAULT_CHARSET, DEFAULT_GUI_FONT,
            DEFAULT_PITCH, DeleteObject, FF_DONTCARE, GetStockObject, HGDIOBJ, OUT_DEFAULT_PRECIS,
        },
        System::SystemServices::{SS_CENTER, SS_CENTERIMAGE, SS_NOPREFIX},
        UI::{
            HiDpi::GetDpiForWindow,
            WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, GetClientRect, SWP_NOACTIVATE, SWP_NOZORDER,
                SendMessageW, SetWindowPos, WINDOW_EX_STYLE, WINDOW_STYLE, WM_SETFONT, WS_CHILD,
                WS_VISIBLE,
            },
        },
    },
    core::{HSTRING, w},
};

#[derive(Clone, Copy)]
pub enum Content<'a> {
    Settings {
        app_name: &'a str,
        heading: &'a str,
        placeholder: &'a str,
    },
    Overlay {
        placeholder: &'a str,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Settings,
    Overlay,
}

#[derive(Clone, Copy, Debug)]
struct Label<'a> {
    text: &'a str,
    size: i32,
    weight: i32,
}

impl<'a> Content<'a> {
    fn labels(self) -> (Kind, Vec<Label<'a>>) {
        match self {
            Self::Settings {
                app_name,
                heading,
                placeholder,
            } => (
                Kind::Settings,
                vec![
                    Label {
                        text: app_name,
                        size: 14,
                        weight: 400,
                    },
                    Label {
                        text: heading,
                        size: 24,
                        weight: 600,
                    },
                    Label {
                        text: placeholder,
                        size: 14,
                        weight: 400,
                    },
                ],
            ),
            Self::Overlay { placeholder } => (
                Kind::Overlay,
                vec![Label {
                    text: placeholder,
                    size: 14,
                    weight: 400,
                }],
            ),
        }
    }
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

fn layout(kind: Kind, width: i32, height: i32, dpi: u32) -> Vec<Rect> {
    let width = width.max(0);
    let height = height.max(0);
    let margin = scale(if kind == Kind::Settings { 24 } else { 12 }, dpi);
    let x = margin.min(width / 2);
    let box_width = (width - 2 * x).max(0);
    let rect = |y: i32, box_height: i32| {
        let y = y.min(height);
        Rect {
            x,
            y,
            width: box_width,
            height: box_height.min(height - y).max(0),
        }
    };
    match kind {
        Kind::Settings => vec![
            rect(scale(24, dpi), scale(24, dpi)),
            rect(scale(64, dpi), scale(34, dpi)),
            rect(scale(116, dpi), (height - scale(140, dpi)).max(0)),
        ],
        Kind::Overlay => {
            let y = margin.min(height / 2);
            vec![rect(y, height - 2 * y)]
        }
    }
}

fn native_error() -> io::Error {
    io::Error::other("The native window content could not be initialized")
}

struct Font(usize);

impl Font {
    fn create(size: i32, weight: i32, dpi: u32) -> io::Result<Self> {
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
    kind: Kind,
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
            self.kind,
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

/// Attach on the owning window thread; all visible strings come from the caller's catalog.
pub fn attach<R: Runtime>(window: &Window<R>, content: Content<'_>) -> tauri::Result<()> {
    let parent = window.hwnd()?;
    let dpi = unsafe { GetDpiForWindow(parent) }.max(96);
    let (kind, labels) = content.labels();
    let mut native = NativeControls {
        owner: std::thread::current().id(),
        parent: parent.0 as usize,
        kind,
        dpi,
        controls: Vec::new(),
        parent_owned: false,
    };
    for label in labels {
        let font = Font::create(label.size, label.weight, dpi)?;
        let alignment = if kind == Kind::Overlay {
            SS_CENTER.0 | SS_CENTERIMAGE.0
        } else {
            0
        };
        let handle = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                &HSTRING::from(label.text),
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(SS_NOPREFIX.0 | alignment),
                0,
                0,
                0,
                0,
                Some(parent),
                None,
                None,
                None,
            )
        }
        .map_err(|_| native_error())?;
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
        let (kind, labels) = Content::Settings {
            app_name: "Anwendung mit ß & Umlauten",
            heading: "Überschrift",
            placeholder: "Platzhalter aus dem Katalog",
        }
        .labels();
        assert_eq!(kind, Kind::Settings);
        assert_eq!(
            labels.iter().map(|label| label.text).collect::<Vec<_>>(),
            [
                "Anwendung mit ß & Umlauten",
                "Überschrift",
                "Platzhalter aus dem Katalog"
            ]
        );
    }

    #[test]
    fn native_overlay_contains_only_its_placeholder() {
        let (kind, labels) = Content::Overlay {
            placeholder: "Overlay aus dem Katalog",
        }
        .labels();
        assert_eq!(kind, Kind::Overlay);
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].text, "Overlay aus dem Katalog");
    }

    #[test]
    fn minimum_window_layouts_remain_inside_client_area_at_each_dpi() {
        for dpi in [96, 120, 144, 192] {
            for (kind, logical_width, logical_height, count) in
                [(Kind::Settings, 360, 220, 3), (Kind::Overlay, 280, 72, 1)]
            {
                let width = logical_width * dpi as i32 / 96;
                let height = logical_height * dpi as i32 / 96;
                let boxes = layout(kind, width, height, dpi);
                assert_eq!(boxes.len(), count);
                for rect in boxes {
                    assert!(rect.x >= 0 && rect.y >= 0);
                    assert!(rect.width > 0 && rect.height > 0);
                    assert!(rect.x + rect.width <= width);
                    assert!(rect.y + rect.height <= height);
                }
            }
        }
    }

    #[test]
    fn resizing_extends_the_wrapped_placeholder_instead_of_clipping_it() {
        let small = layout(Kind::Settings, 360, 220, 96);
        let large = layout(Kind::Settings, 760, 520, 96);
        assert_eq!(small.len(), 3);
        assert_eq!(large.len(), 3);
        assert!(large[2].width > small[2].width);
        assert!(large[2].height > small[2].height);
        assert_eq!(large[2].y, small[2].y);
    }
}
