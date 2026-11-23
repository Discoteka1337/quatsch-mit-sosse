use super::*;
use windows::Win32::UI::Controls::SetScrollInfo;

pub(super) fn content_height(general: bool) -> i32 {
    if general { 752 } else { 544 }
}
pub(super) const TITLE: u16 = 100;
pub(super) const DICTATION: u16 = 101;
pub(super) const REINSERT: u16 = 102;
pub(super) const MODE: u16 = 103;
pub(super) const MICROPHONE: u16 = 104;
pub(super) const TEST_MIC: u16 = 105;
pub(super) const LEVEL: u16 = 106;
pub(super) const MODEL: u16 = 107;
pub(super) const DOWNLOAD: u16 = 108;
pub(super) const REMOVE: u16 = 109;
pub(super) const VOLUME: u16 = 110;
pub(super) const TONES: u16 = 111;
pub(super) const AUTOSTART: u16 = 112;
pub(super) const SHOW_READY: u16 = 113;
pub(super) const HIDE_FROM_CAPTURE: u16 = 114;
pub(super) const STATUS: u16 = 115;
pub(super) const OPEN_DICTIONARY: u16 = 116;
pub(super) const OPEN_SNIPPETS: u16 = 117;
pub(super) const COMPARISON_SHORTCUT: u16 = 118;
pub(super) const VOICE_COMMANDS: u16 = 119;
pub(super) const USE_DICTIONARY: u16 = 120;
pub(super) const SELF_CORRECTIONS: u16 = 121;
pub(super) const WRITE_NUMBERS: u16 = 122;
pub(super) const PUNCTUATION: u16 = 123;
pub(super) const USE_SNIPPETS: u16 = 124;
pub(super) const FILLER_MODE: u16 = 125;
pub(super) const DIGITS_FROM: u16 = 126;
pub(super) const EDITOR_TITLE: u16 = 301;
pub(super) const GENERAL: u16 = 302;
pub(super) const SEARCH: u16 = 303;
pub(super) const ENTRIES: u16 = 304;
pub(super) const TERM: u16 = 305;
pub(super) const REPLACEMENT: u16 = 306;
pub(super) const ALIASES: u16 = 307;
pub(super) const SAVE: u16 = 308;
pub(super) const NEW: u16 = 309;
pub(super) const DELETE: u16 = 310;
pub(super) const IMPORT: u16 = 311;
pub(super) const EXPORT: u16 = 312;
pub(super) const EDITOR_STATUS: u16 = 313;

// Both entry editors use the same relative control IDs. Distinct native
// widgets let pending storage updates complete when the other page is open.
pub(super) const SNIPPET_OFFSET: u16 = 100;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Text,
    Button,
    Check,
    Combo,
    Meter,
    Edit,
    Multiline,
    List,
}
use Kind::*;

#[derive(Clone, Copy)]
enum Column {
    Full,
    Left,
    Right,
    ActionLeft,
    ActionRight,
}
use Column::*;

pub(super) struct Field {
    pub id: u16,
    pub kind: Kind,
    pub text: &'static str,
    column: Column,
    y: i32,
    height: i32,
}

pub(super) static FIELDS: std::sync::LazyLock<Vec<Field>> = std::sync::LazyLock::new(|| {
    let l = &*LABELS;
    let messages = crate::messages();
    let mut fields = vec![
        (TITLE, Text, l.general_title.as_str(), Left, 16, 38),
        (
            OPEN_DICTIONARY,
            Button,
            &l.dictionary_title,
            ActionLeft,
            18,
            32,
        ),
        (
            OPEN_SNIPPETS,
            Button,
            &l.snippets_title,
            ActionRight,
            18,
            32,
        ),
        (201, Text, &l.general_dictation, Left, 66, 22),
        (DICTATION, Button, "", Left, 90, 30),
        (202, Text, &l.general_reinsert, Right, 66, 22),
        (REINSERT, Button, "", Right, 90, 30),
        (COMPARISON_SHORTCUT, Text, "", Full, 128, 22),
        (203, Text, &l.general_mode, Left, 158, 22),
        (MODE, Combo, "", Left, 182, 28),
        (204, Text, &l.general_microphone, Right, 158, 22),
        (MICROPHONE, Combo, "", Right, 182, 28),
        (TEST_MIC, Button, &l.general_microphone_test, Left, 228, 30),
        (206, Text, &l.general_level, Right, 214, 20),
        (LEVEL, Meter, &l.general_level, Right, 238, 14),
        (207, Text, &l.general_model, Full, 270, 22),
        (MODEL, Combo, "", Left, 294, 30),
        (DOWNLOAD, Button, &l.general_download, ActionLeft, 294, 30),
        (REMOVE, Button, &l.general_remove, ActionRight, 294, 30),
        (210, Text, &l.general_volume, Full, 342, 22),
        (VOLUME, Combo, "", Left, 366, 28),
        (TONES, Check, &l.general_tones, Right, 366, 28),
        (AUTOSTART, Check, &l.general_autostart, Full, 402, 28),
        (213, Text, &l.general_overlay, Full, 442, 22),
        (
            SHOW_READY,
            Check,
            &messages.overlay_show_ready,
            Left,
            468,
            28,
        ),
        (
            HIDE_FROM_CAPTURE,
            Check,
            &messages.overlay_hide_from_capture,
            Right,
            468,
            28,
        ),
        (214, Text, &l.cleanup_title, Full, 508, 22),
        (VOICE_COMMANDS, Check, &l.cleanup_commands, Left, 532, 26),
        (USE_DICTIONARY, Check, &l.cleanup_dictionary, Right, 532, 26),
        (
            SELF_CORRECTIONS,
            Check,
            &l.cleanup_corrections,
            Left,
            562,
            26,
        ),
        (WRITE_NUMBERS, Check, &l.cleanup_numbers, Right, 562, 26),
        (PUNCTUATION, Check, &l.cleanup_punctuation, Left, 592, 26),
        (USE_SNIPPETS, Check, &l.cleanup_snippets, Right, 592, 26),
        (215, Text, &l.cleanup_fillers, Left, 628, 22),
        (FILLER_MODE, Combo, "", Left, 652, 28),
        (216, Text, &l.cleanup_threshold, Right, 628, 22),
        (DIGITS_FROM, Edit, "", Right, 652, 28),
        (STATUS, Text, "", Full, 694, 44),
    ];
    for collection in [Collection::Dictionary, Collection::Snippets] {
        let mut controls = vec![
            (EDITOR_TITLE, Text, collection.title(), Left, 16, 38),
            (GENERAL, Button, &l.general_title, Right, 18, 32),
            (320, Text, collection.search_label(), Full, 60, 22),
            (SEARCH, Edit, "", Full, 82, 28),
            (321, Text, &l.dictionary_entries, Full, 118, 22),
        ];
        match collection {
            Collection::Dictionary => controls.extend([
                (ENTRIES, List, "", Full, 140, 100),
                (322, Text, &l.dictionary_term, Left, 248, 22),
                (TERM, Edit, "", Left, 270, 28),
                (323, Text, &l.dictionary_replacement, Right, 248, 22),
                (REPLACEMENT, Multiline, "", Right, 270, 48),
                (324, Text, &l.dictionary_aliases, Full, 322, 22),
                (ALIASES, Multiline, "", Full, 344, 54),
            ]),
            Collection::Snippets => controls.extend([
                (ENTRIES, List, "", Full, 140, 72),
                (322, Text, &l.snippets_trigger, Full, 216, 22),
                (TERM, Edit, "", Full, 238, 28),
                (323, Text, &l.snippets_text, Full, 270, 22),
                (REPLACEMENT, Multiline, "", Full, 292, 74),
                (324, Text, &l.snippets_placeholders, Full, 370, 36),
            ]),
        }
        controls.extend([
            (SAVE, Button, l.dictionary_add.as_str(), Left, 408, 30),
            (NEW, Button, &l.dictionary_new, ActionLeft, 408, 30),
            (DELETE, Button, &l.dictionary_delete, ActionRight, 408, 30),
            (IMPORT, Button, &l.dictionary_import, Left, 450, 30),
            (EXPORT, Button, &l.dictionary_export, Right, 450, 30),
            (EDITOR_STATUS, Text, "", Full, 486, 44),
        ]);
        fields.extend(
            controls
                .into_iter()
                .map(|(id, kind, text, col, y, height)| {
                    (collection.id(id), kind, text, col, y, height)
                }),
        );
    }
    fields
        .into_iter()
        .map(|(id, kind, text, column, y, height)| Field {
            id,
            kind,
            text,
            column,
            y,
            height,
        })
        .collect()
});

pub(super) struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

pub(super) fn placement(id: u16, width: i32, dpi: u32, scroll: i32) -> Rect {
    let field = FIELDS
        .iter()
        .find(|field| field.id == id)
        .expect("Unknown settings control");
    let margin = scale(18, dpi);
    let gap = scale(14, dpi);
    let inner = (width - 2 * margin).max(0);
    let column = (inner - gap).max(0) / 2;
    let right = margin + column + gap;
    let half = (column - gap / 2).max(0) / 2;
    let (x, width) = match field.column {
        Full => (margin, inner),
        Left => (margin, column),
        Right => (right, column),
        ActionLeft => (right, half),
        ActionRight => (right + half + gap / 2, half),
    };
    Rect {
        x,
        y: scale(field.y, dpi) - scroll,
        width,
        height: scale(field.height, dpi),
    }
}

pub(super) fn layout(form: &Form) -> io::Result<()> {
    // SetScrollInfo can synchronously notify us of another WM_SIZE.
    if form.in_layout.replace(true) {
        return Ok(());
    }
    let result = place(form);
    form.in_layout.set(false);
    result
}

fn place(form: &Form) -> io::Result<()> {
    let dpi = unsafe { GetDpiForWindow(form.hwnd) }.max(96);
    let mut bounds = RECT::default();
    unsafe {
        GetClientRect(form.hwnd, &mut bounds)?;
    }
    let page = bounds.bottom.max(1);
    let title_style = unsafe { GetWindowLongW(form.control(TITLE), GWL_STYLE) } as u32;
    let content = scale(content_height(title_style & WS_VISIBLE.0 != 0), dpi);
    let offset = form.scroll.get().clamp(0, (content - page).max(0));
    form.scroll.set(offset);
    let info = SCROLLINFO {
        cbSize: size_of::<SCROLLINFO>() as u32,
        fMask: SIF_POS | SIF_RANGE | SIF_PAGE,
        nMin: 0,
        nMax: content - 1,
        nPage: page as u32,
        nPos: offset,
        ..Default::default()
    };
    unsafe {
        SetScrollInfo(form.hwnd, SB_VERT, &info, true);
        GetClientRect(form.hwnd, &mut bounds)?;
    }
    let new_fonts = if form.dpi.get() != dpi {
        Some((Font::create(15, 400, dpi)?, Font::create(26, 600, dpi)?))
    } else {
        None
    };
    for field in FIELDS.iter() {
        let child = form.control(field.id);
        let rect = placement(field.id, bounds.right, dpi, offset);
        if let Some((body, title)) = &new_fonts {
            let font = if [TITLE, EDITOR_TITLE, Collection::Snippets.id(EDITOR_TITLE)]
                .contains(&field.id)
            {
                title
            } else {
                body
            };
            send(child, WM_SETFONT, font.0, 1);
        }
        let height = if field.kind == Kind::Combo {
            scale(240, dpi)
        } else {
            rect.height
        };
        unsafe {
            SetWindowPos(
                child,
                None,
                rect.x,
                rect.y,
                rect.width,
                height,
                SWP_NOACTIVATE | SWP_NOZORDER,
            )?;
        }
    }
    if let Some(fonts) = new_fonts {
        form.fonts.replace(Some(fonts));
        form.dpi.set(dpi);
    }
    Ok(())
}
