use super::*;
use windows::Win32::{
    Foundation::COLORREF,
    Graphics::Gdi::*,
    System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW},
};

#[derive(Clone, Copy)]
pub(super) struct Palette {
    background: COLORREF,
    text: COLORREF,
    muted: COLORREF,
    accent: COLORREF,
    track: COLORREF,
    button: COLORREF,
}

const fn rgb(red: u8, green: u8, blue: u8) -> COLORREF {
    COLORREF(red as u32 | ((green as u32) << 8) | ((blue as u32) << 16))
}

impl Palette {
    pub(super) fn system() -> Self {
        let mut light = 1u32;
        let mut bytes = size_of_val(&light) as u32;
        let result = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
                w!("AppsUseLightTheme"),
                RRF_RT_REG_DWORD,
                None,
                Some((&raw mut light).cast()),
                Some(&mut bytes),
            )
        };
        if result.is_ok() && light == 0 {
            Self {
                background: rgb(35, 38, 40),
                text: rgb(244, 246, 246),
                muted: rgb(182, 191, 194),
                accent: rgb(111, 215, 182),
                track: rgb(73, 83, 83),
                button: rgb(64, 85, 82),
            }
        } else {
            Self {
                background: rgb(242, 246, 245),
                text: rgb(27, 39, 38),
                muted: rgb(87, 103, 100),
                accent: rgb(18, 111, 89),
                track: rgb(200, 211, 208),
                button: rgb(205, 226, 219),
            }
        }
    }
}

struct Canvas {
    dc: HDC,
    bounds: RECT,
    dpi: u32,
    colors: Palette,
}

impl Canvas {
    fn fill(&self, bounds: RECT, color: COLORREF) {
        unsafe {
            SetDCBrushColor(self.dc, color);
            FillRect(self.dc, &bounds, HBRUSH(GetStockObject(DC_BRUSH).0));
        }
    }

    fn text(&self, label: &str, mut bounds: RECT, color: COLORREF, alignment: DRAW_TEXT_FORMAT) {
        let mut units: Vec<_> = label.encode_utf16().collect();
        unsafe {
            SetTextColor(self.dc, color);
            DrawTextW(self.dc, &mut units, &mut bounds, alignment | DT_NOPREFIX);
        }
    }

    fn draw(&self, view: &View) {
        let b = self.bounds;
        let s = |v| px(v, self.dpi);
        let c = self.colors;
        let line = DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS;
        if matches!(view, View::Ready) {
            // The window region limits both paint and hit-testing to the dot.
            self.fill(b, c.muted);
            return;
        }
        self.fill(b, c.background);
        let inset = rect(s(12), 0, b.right - s(12), b.bottom);
        match view {
            View::Listening { label, level } => {
                let count = lit_bars(*level);
                let start = b.right - s(68);
                self.text(
                    label,
                    rect(inset.left, 0, start - s(4), b.bottom),
                    c.text,
                    line,
                );
                for i in 0..10 {
                    let x = start + s(i * 6);
                    let height = s(5 + i);
                    let y = (b.bottom - height) / 2;
                    let color = if (i as usize) < count {
                        c.accent
                    } else {
                        c.track
                    };
                    self.fill(rect(x, y, x + s(3), y + height), color);
                }
            }
            View::Status(label) => self.text(label, inset, c.text, DT_CENTER | line),
            View::Error { label, copy } => {
                let button = copy_bounds(b, self.dpi);
                self.text(
                    label,
                    rect(inset.left, s(5), inset.right, button.top - s(1)),
                    c.text,
                    DT_WORDBREAK | DT_END_ELLIPSIS,
                );
                self.fill(button, c.button);
                self.text(copy, button, c.text, DT_CENTER | line);
            }
            View::Administrator { label, detail } => {
                self.text(
                    label,
                    rect(inset.left, s(6), inset.right, s(30)),
                    c.text,
                    line,
                );
                self.text(
                    detail,
                    rect(inset.left, s(32), inset.right, b.bottom - s(6)),
                    c.muted,
                    DT_WORDBREAK | DT_END_ELLIPSIS,
                );
            }
            View::Hidden | View::Ready => {}
        }
    }
}

impl Overlay {
    pub(super) fn paint(&self, hwnd: HWND) {
        let view = self.view.borrow().clone();
        let mut paint = PAINTSTRUCT::default();
        unsafe {
            let dc = BeginPaint(hwnd, &mut paint);
            if !dc.is_invalid() {
                if let Ok(bounds) = client_rect(hwnd) {
                    let dpi = GetDpiForWindow(hwnd);
                    let mut cached = self.font.borrow_mut();
                    if cached.as_ref().is_none_or(|(old, _)| *old != dpi) {
                        *cached = crate::native_ui::Font::create(14, 400, dpi)
                            .ok()
                            .map(|font| (dpi, font));
                    }
                    let font = cached.as_ref().map_or_else(
                        || GetStockObject(DEFAULT_GUI_FONT),
                        |(_, font)| HGDIOBJ(font.0 as _),
                    );
                    let previous = SelectObject(dc, font);
                    SetBkMode(dc, TRANSPARENT);
                    Canvas {
                        dc,
                        bounds,
                        dpi,
                        colors: self.palette.get(),
                    }
                    .draw(&view);
                    SelectObject(dc, previous);
                }
                let _ = EndPaint(hwnd, &paint);
            }
        }
    }
}
