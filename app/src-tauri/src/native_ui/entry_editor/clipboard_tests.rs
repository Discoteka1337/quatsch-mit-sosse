use super::*;

#[test]
fn private_edit_commands_and_context_menus_never_reach_the_default_clipboard_handler() {
    use windows::Win32::{
        System::SystemServices::{APPCOMMAND_COPY, APPCOMMAND_CUT},
        UI::Input::KeyboardAndMouse::{
            GetKeyboardState, SetKeyboardState, VK_C, VK_CONTROL, VK_DELETE, VK_INSERT, VK_X,
        },
    };
    #[derive(Default)]
    struct Events {
        leaked: Cell<u32>,
        routed: RefCell<Vec<(u16, u32)>>,
    }
    unsafe extern "system" fn observe(
        hwnd: HWND,
        msg: u32,
        wp: WPARAM,
        lp: LPARAM,
        id: usize,
        data: usize,
    ) -> LRESULT {
        let events = unsafe { &*(data as *const Events) };
        if id == 1
            && matches!(
                msg,
                WM_CUT
                    | WM_COPY
                    | WM_CONTEXTMENU
                    | WM_CHAR
                    | WM_KEYDOWN
                    | WM_APPCOMMAND
                    | WM_COMMAND
            )
        {
            // Catch fallthrough before the native test control could access the session clipboard.
            events.leaked.set(events.leaked.get() + 1);
            return LRESULT(0);
        }
        if id == 2 && msg == clipboard::REQUEST {
            events
                .routed
                .borrow_mut()
                .push((wp.0 as u16, (wp.0 >> 16) as u32));
            return LRESULT(0);
        }
        unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
    }
    let events = Events::default();
    let data = (&raw const events) as usize;
    let fields: Vec<_> = FIELDS
        .iter()
        .filter(|f| {
            Collection::for_field(f.id).is_some()
                && matches!(f.kind, layout::Kind::Edit | layout::Kind::Multiline)
        })
        .collect();
    assert_eq!(fields.len(), 7);
    let mut expected = Vec::new();
    unsafe {
        let parent = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            w!(""),
            WS_POPUP,
            0,
            0,
            200,
            100,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert!(SetWindowSubclass(parent, Some(observe), 2, data).as_bool());
        let mut old = [0; 256];
        GetKeyboardState(&mut old).unwrap();
        for f in fields {
            let edit = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("EDIT"),
                w!("Vorlage"),
                WS_CHILD,
                0,
                0,
                160,
                24,
                Some(parent),
                Some(HMENU(f.id as usize as _)),
                None,
                None,
            )
            .unwrap();
            assert!(SetWindowSubclass(edit, Some(observe), 1, data).as_bool());
            assert!(SetWindowSubclass(edit, Some(child_proc), f.id as usize, 0).as_bool());
            let mut pressed = [0; 256];
            pressed[VK_CONTROL.0 as usize] = 0x80;
            SetKeyboardState(&pressed).unwrap();
            for (msg, wp, lp) in [
                (WM_KEYDOWN, VK_C.0 as usize, 0),
                (WM_CHAR, 3, 0),
                (WM_KEYDOWN, VK_X.0 as usize, 0),
                (WM_CHAR, 24, 0),
                (WM_KEYDOWN, VK_INSERT.0 as usize, 0),
                (WM_COPY, 0, 0),
                (WM_CUT, 0, 0),
                (WM_APPCOMMAND, 0, (APPCOMMAND_COPY.0 << 16) as isize),
                (WM_APPCOMMAND, 0, (APPCOMMAND_CUT.0 << 16) as isize),
                (WM_CONTEXTMENU, edit.0 as usize, -1),
                (WM_COMMAND, WM_CUT as usize, 0),
            ] {
                send(edit, msg, wp, lp);
            }
            pressed[VK_CONTROL.0 as usize] = 0;
            pressed[VK_SHIFT.0 as usize] = 0x80;
            SetKeyboardState(&pressed).unwrap();
            send(edit, WM_KEYDOWN, VK_DELETE.0 as usize, 0);
            expected.extend(
                [
                    WM_COPY,
                    WM_CUT,
                    WM_COPY,
                    WM_COPY,
                    WM_CUT,
                    WM_COPY,
                    WM_CUT,
                    WM_CONTEXTMENU,
                    WM_CUT,
                    WM_CUT,
                ]
                .into_iter()
                .map(|cmd| (f.id, cmd)),
            );
        }
        SetKeyboardState(&old).unwrap();
        DestroyWindow(parent).unwrap();
    }
    assert_eq!(events.leaked.get(), 0);
    assert_eq!(events.routed.into_inner(), expected);
}

#[test]
fn a_private_cut_preserves_the_selection_on_failure_and_supports_undo() {
    use windows::Win32::UI::Controls::{EM_LIMITTEXT, EM_SETSEL};
    let prefix = "v".repeat(66_000);
    let selected = "🧭\r\nPrüfung";
    let original = format!("{prefix}{selected} Ende");
    let start = prefix.len();
    let end = start + selected.encode_utf16().count();
    unsafe {
        let edit = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("EDIT"),
            w!(""),
            WS_POPUP
                | WS_VSCROLL
                | WINDOW_STYLE((ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN) as u32),
            0,
            0,
            200,
            100,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        send(edit, EM_LIMITTEXT, original.len(), 0);
        SetWindowTextW(edit, &HSTRING::from(&original)).unwrap();
        send(edit, EM_SETSEL, start, end as isize);
        assert!(
            clipboard::execute(edit, WM_CUT, |value| {
                assert_eq!(value, selected);
                Err(io::Error::other("busy"))
            })
            .is_err()
        );
        let size = || GetWindowTextLengthW(edit) as usize;
        assert_eq!(size(), original.encode_utf16().count());
        clipboard::execute(edit, WM_COPY, |value| {
            assert_eq!(value, selected);
            Ok(())
        })
        .unwrap();
        assert_eq!(size(), original.encode_utf16().count());
        clipboard::execute(edit, WM_CUT, |value| {
            assert_eq!(value, selected);
            Ok(())
        })
        .unwrap();
        assert_eq!(size(), start + 5);
        assert_ne!(send(edit, WM_UNDO, 0, 0).0, 0);
        assert_eq!(size(), original.encode_utf16().count());
        send(edit, EM_SETSEL, start, end as isize);
        assert!(
            clipboard::execute(edit, WM_CUT, |_| {
                send(edit, EM_SETSEL, 0, 1);
                Ok(())
            })
            .is_err()
        );
        assert_eq!(size(), original.encode_utf16().count());
        send(edit, EM_SETSEL, 0, 0);
        clipboard::execute(edit, WM_CUT, |_| panic!("empty selection must not publish")).unwrap();
        DestroyWindow(edit).unwrap();
    }
}
