use super::*;
use crate::comparison::{Segment, SegmentKind::*};
use std::{cell::RefCell, mem::size_of};
use windows::Win32::UI::Controls::{
    EM_SETSEL,
    RichEdit::{CHARFORMAT2W, CHARFORMATW, EM_GETCHARFORMAT, SCF_SELECTION},
};

mod lifetime;

#[test]
fn prepared_pages_bound_native_work_and_preserve_all_text_and_style_boundaries() {
    let block = "🧭x\r\n";
    let expanded = block.repeat(100_000);
    let last_word = expanded.len() - block.len()..expanded.len() - 2;
    let record = std::sync::Arc::new(DictationRecord {
        original: "Textbaustein einsetzen".into(),
        finished: expanded,
        uncertain: std::iter::once(last_word).collect(),
        ..Default::default()
    });
    let view = PreparedComparison::new(record);
    assert!(view.pages() > 1);
    assert_eq!(view.before.len(), 1);
    let last = view.after.last().unwrap();
    assert!(
        last.runs
            .contains(&(last.text.len() - 4, last.text.len() - 1, Uncertain))
    );
    let decoded = view
        .after
        .iter()
        .map(|page| {
            assert!(page.text.len() <= text::PAGE_UNITS);
            assert!(page.runs.len() <= text::PAGE_RUNS);
            String::from_utf16(&page.text).unwrap()
        })
        .collect::<String>();
    assert_eq!(decoded, "🧭x\r".repeat(100_000));
    let alternating = (0..500)
        .map(|i| Segment {
            text: "x",
            kind: if i % 2 == 0 { Added } else { Uncertain },
        })
        .collect::<Vec<_>>();
    let pages = text::pages(&alternating);
    assert!(pages.len() > 1);
    assert_eq!(pages.iter().map(|p| p.runs.len()).sum::<usize>(), 500);
}

#[test]
fn native_copy_routes_selected_text_without_giving_richedit_the_system_clipboard() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardState, SetKeyboardState};
    #[derive(Default)]
    struct Events {
        native_default: Cell<u32>,
        selected: RefCell<Vec<String>>,
    }

    unsafe extern "system" fn observe(
        hwnd: HWND,
        msg: u32,
        wp: WPARAM,
        lp: LPARAM,
        id: usize,
        data: usize,
    ) -> LRESULT {
        unsafe {
            let events = &*(data as *const Events);
            if id == 1
                && matches!(
                    msg,
                    WM_COPY | WM_CUT | WM_KEYDOWN | WM_SYSKEYDOWN | WM_APPCOMMAND | WM_CHAR
                )
            {
                // Stop the test control before its standard handler can touch the clipboard.
                events.native_default.set(events.native_default.get() + 1);
                return LRESULT(0);
            }
            if id == 2 && msg == COPY_SELECTION {
                if let Ok(Some(text)) =
                    GetDlgItem(Some(hwnd), wp.0 as i32).and_then(clipboard::selection)
                {
                    events.selected.borrow_mut().push(text);
                }
                return LRESULT(0);
            }
            DefSubclassProc(hwnd, msg, wp, lp)
        }
    }
    let events = Events::default();
    let data = (&raw const events) as usize;
    let _library = load_editor().unwrap();
    let font = Font::create(15, 400, 96).unwrap();
    let window = create_window(RECT {
        right: 250,
        bottom: 150,
        ..Default::default()
    })
    .unwrap();
    let pane = child(
        window,
        BEFORE,
        MSFTEDIT_CLASS,
        "",
        WINDOW_STYLE((ES_MULTILINE | ES_READONLY) as u32),
        &font,
    )
    .unwrap();
    let original = "A🧭\nGrüße!";
    let pages = text::pages(&[Segment {
        text: original,
        kind: Unchanged,
    }]);
    text::render_page(pane, &pages[0]).unwrap();
    send(pane, EM_SETSEL, 1, 9);
    assert_eq!(
        clipboard::selection(pane).unwrap().as_deref(),
        Some("🧭\r\nGrüße")
    );
    unsafe {
        assert!(SetWindowSubclass(pane, Some(observe), 1, data).as_bool());
        assert!(SetWindowSubclass(window, Some(observe), 2, data).as_bool());
        send(pane, WM_COPY, 0, 0);
        assert_eq!(events.native_default.get(), 1);
        assert!(SetWindowSubclass(pane, Some(child_proc), BEFORE, 0).as_bool());
        let mut saved = [0u8; 256];
        GetKeyboardState(&mut saved).unwrap();
        let mut pressed = saved;
        pressed[VK_CONTROL.0 as usize] |= 0x80;
        SetKeyboardState(&pressed).unwrap();
        send(pane, WM_KEYDOWN, VK_C.0 as usize, 0);
        send(pane, WM_CHAR, 3, 0);
        send(pane, WM_KEYDOWN, VK_INSERT.0 as usize, 0);
        send(pane, WM_CUT, 0, 0);
        send(pane, WM_KEYDOWN, VK_X.0 as usize, 0);
        send(pane, WM_CHAR, 24, 0);
        pressed[VK_SHIFT.0 as usize] |= 0x80;
        pressed[VK_MENU.0 as usize] |= 0x80;
        SetKeyboardState(&pressed).unwrap();
        send(pane, WM_KEYDOWN, VK_DELETE.0 as usize, 0);
        send(pane, WM_SYSKEYDOWN, VK_F11.0 as usize, 0);
        SetKeyboardState(&saved).unwrap();
        send(pane, WM_COPY, 0, 0);
        send(pane, WM_APPCOMMAND, 0, (APPCOMMAND_COPY.0 << 16) as isize);
        DestroyWindow(window).unwrap();
    }
    assert_eq!(events.native_default.get(), 1);
    assert_eq!(*events.selected.borrow(), ["🧭\r\nGrüße"; 4]);
}

#[test]
fn escape_from_a_read_only_pane_or_button_dispatches_close_to_its_window() {
    let _library = load_editor().unwrap();
    let font = Font::create(15, 400, 96).unwrap();
    for (id, class) in [(BEFORE, MSFTEDIT_CLASS), (INSERT, w!("BUTTON"))] {
        let window = create_window(RECT {
            right: 250,
            bottom: 150,
            ..Default::default()
        })
        .unwrap();
        let control = child(window, id, class, "", WS_TABSTOP, &font).unwrap();
        assert!(is_window(window));
        assert!(!is_window(control));
        send(control, WM_KEYDOWN, usize::from(VK_ESCAPE.0), 0);
        let mut message = MSG::default();
        unsafe {
            assert!(
                PeekMessageW(&mut message, Some(window), WM_CLOSE, WM_CLOSE, PM_REMOVE).as_bool()
            );
            DispatchMessageW(&message);
            assert!(!IsWindow(Some(window)).as_bool());
        }
    }
}

#[test]
fn native_text_pane_preserves_literal_unicode_and_highlights_only_its_runs() {
    let _library = load_editor().unwrap();
    let pane = unsafe {
        CreateWindowExW(
            Default::default(),
            MSFTEDIT_CLASS,
            None,
            WS_POPUP | WINDOW_STYLE((ES_MULTILINE | ES_READONLY) as u32),
            0,
            0,
            240,
            180,
            None,
            None,
            None,
            None,
        )
        .unwrap()
    };
    let pages = text::pages(&[
        Segment {
            text: "🧭{\\äß}\n\r",
            kind: Unchanged,
        },
        Segment {
            text: "\nx",
            kind: Removed,
        },
        Segment {
            text: "y",
            kind: Added,
        },
        Segment {
            text: "?",
            kind: Uncertain,
        },
        Segment {
            text: "z",
            kind: Unchanged,
        },
    ]);
    render_page(pane, &pages[0]).unwrap();
    let mut buffer = [0; 40];
    let count = unsafe { GetWindowTextW(pane, &mut buffer) };
    assert_eq!(
        String::from_utf16(&buffer[..count as usize])
            .unwrap()
            .replace("\r\n", "\n")
            .replace('\r', "\n"),
        "🧭{\\äß}\n\nxy?z"
    );
    let background = |offset| {
        send(pane, EM_SETSEL, offset, offset as isize + 1);
        let mut format = CHARFORMAT2W {
            Base: CHARFORMATW {
                cbSize: size_of::<CHARFORMAT2W>() as u32,
                ..Default::default()
            },
            ..Default::default()
        };
        send(
            pane,
            EM_GETCHARFORMAT,
            SCF_SELECTION as usize,
            (&raw mut format) as isize,
        );
        format.crBackColor.0
    };
    assert_eq!(background(9), 0xDFE0FF);
    assert_eq!(background(10), 0xDEF1D5);
    assert_eq!(background(11), 0x6DE3FF);
    assert_ne!(background(12), background(11));
    let long_run = "🧭".repeat(text::PAGE_UNITS);
    let pages = text::pages(&[
        Segment {
            text: &long_run,
            kind: Uncertain,
        },
        Segment {
            text: "Gut",
            kind: Unchanged,
        },
    ]);
    for page in &pages[..2] {
        render_page(pane, page).unwrap();
        assert_eq!(background(0), 0x6DE3FF);
    }
    render_page(pane, &pages[2]).unwrap();
    assert_ne!(background(0), 0x6DE3FF);
    unsafe { DestroyWindow(pane).unwrap() };
}

#[test]
fn placement_keeps_both_edges_on_the_overlay_monitor_and_prefers_space_above() {
    let area = RECT {
        left: -1200,
        top: -100,
        right: 0,
        bottom: 800,
    };
    let low = RECT {
        left: -160,
        top: 750,
        right: 0,
        bottom: 790,
    };
    let panel = placement(low, area, 144);
    assert_eq!(panel.right, area.right);
    assert!(panel.bottom < low.top && panel.top >= area.top);
    let high = RECT {
        left: -1250,
        top: -100,
        right: -1090,
        bottom: -64,
    };
    let panel = placement(high, area, 144);
    assert_eq!(panel.left, area.left);
    assert!(panel.top > high.bottom && panel.bottom <= area.bottom);
}

#[test]
fn actions_restore_focus_before_insertion_and_copy_failure_leaves_the_review_open() {
    for (action, copied, restored, expected, calls) in [
        (
            Action::Dismiss,
            false,
            true,
            Outcome::Close,
            vec!["restore"],
        ),
        (
            Action::Insert,
            false,
            true,
            Outcome::Insert,
            vec!["restore"],
        ),
        (
            Action::Insert,
            false,
            false,
            Outcome::TargetLost,
            vec!["restore"],
        ),
        (Action::Copy, false, true, Outcome::CopyFailed, vec!["copy"]),
        (
            Action::Copy,
            true,
            true,
            Outcome::Close,
            vec!["copy", "restore"],
        ),
    ] {
        let events = RefCell::new(Vec::new());
        let outcome = finish(
            action,
            || {
                events.borrow_mut().push("copy");
                copied
            },
            || {
                events.borrow_mut().push("restore");
                restored
            },
        );
        assert_eq!(outcome, expected);
        assert_eq!(*events.borrow(), calls);
    }
}

#[test]
fn closing_the_target_keeps_the_displayed_original_available_for_copy() {
    let bounds = RECT {
        right: 220,
        bottom: 100,
        ..Default::default()
    };
    let previous = create_window(bounds).unwrap();
    let target = Target::from_window(previous).unwrap();
    let hwnd = create_window(bounds).unwrap();
    unsafe { DestroyWindow(previous).unwrap() };
    let errors = Rc::new(Cell::new(0));
    let reported = errors.clone();
    let review = Review {
        target,
        view: PreparedComparison::new(Arc::new(DictationRecord {
            original: "ein, äh, Satz".into(),
            finished: "ein Satz".into(),
            ..Default::default()
        })),
        page: Cell::new(0),
        insert: RefCell::new(Some(Box::new(|_, _| panic!("unexpected insertion")))),
        error: Box::new(move |_| reported.set(reported.get() + 1)),
        closing: Cell::new(false),
        font: Font::create(15, 400, 96).unwrap(),
        _editor: load_editor().unwrap(),
    };
    review.finish(hwnd, Action::Insert);
    assert_eq!(errors.get(), 1);
    assert!(!review.closing.get());
    assert!(unsafe { IsWindow(Some(hwnd)).as_bool() });
    review.finish(hwnd, Action::Dismiss);
    assert!(!unsafe { IsWindow(Some(hwnd)).as_bool() });
}
