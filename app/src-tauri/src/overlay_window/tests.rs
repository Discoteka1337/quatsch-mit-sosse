use super::*;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, GetForegroundWindow, IsWindowVisible, SendMessageW,
    WS_EX_TOOLWINDOW, WS_POPUP,
};

#[test]
fn error_button_hit_area_scales_with_its_painted_bounds() {
    for dpi in [96, 120, 144, 192] {
        let client = rect(0, 0, px(312, dpi), px(76, dpi));
        let button = copy_bounds(client, dpi);
        assert!(button.left > client.left && button.right < client.right);
        assert!(button.top > client.top && button.bottom < client.bottom);
        assert!(inside(
            button,
            POINT {
                x: button.left,
                y: button.top
            }
        ));
        assert!(!inside(
            button,
            POINT {
                x: button.right,
                y: button.bottom
            }
        ));
    }
}

#[test]
fn meter_is_empty_at_silence_and_saturates_at_full_scale() {
    assert_eq!(lit_bars(0), 0);
    assert_eq!(lit_bars(50), 5);
    assert_eq!(lit_bars(100), 10);
    assert_eq!(lit_bars(255), 10);
}

#[test]
fn overlay_shows_and_handles_copy_without_activating() {
    unsafe {
        let window = CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!(""),
            WS_POPUP,
            -32000,
            -32000,
            312,
            76,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let copies = Rc::new(Cell::new(0));
        let copied = Rc::clone(&copies);
        let moves = Rc::new(Cell::new(0));
        let dragged = Rc::clone(&moves);
        attach(
            window,
            Callbacks {
                copied: Box::new(move || {
                    copied.set(copied.get() + 1);
                    render(window, View::Hidden).unwrap();
                }),
                dragged: Box::new(move || dragged.set(dragged.get() + 1)),
                display_changed: Box::new(|| {}),
            },
        )
        .unwrap();
        render(window, View::Ready).unwrap();
        let point = |x: i32, y: i32| Some(LPARAM(((y << 16) | x) as isize));
        let mut before = RECT::default();
        GetWindowRect(window, &mut before).unwrap();
        SendMessageW(window, WM_LBUTTONDOWN, None, point(80, 18));
        SendMessageW(window, WM_MOUSEMOVE, None, point(103, 60));
        SendMessageW(window, WM_LBUTTONUP, None, point(80, 18));
        let mut after = RECT::default();
        GetWindowRect(window, &mut after).unwrap();
        assert_eq!((after.left - before.left, after.top - before.top), (23, 42));
        assert_eq!(moves.get(), 1);
        assert_ne!(GetForegroundWindow(), window);
        render(
            window,
            View::Error {
                label: "Fehler".into(),
                copy: "Kopieren".into(),
            },
        )
        .unwrap();
        assert!(IsWindowVisible(window).as_bool());
        assert_eq!(
            SendMessageW(window, WM_MOUSEACTIVATE, None, None).0,
            MA_NOACTIVATE as isize
        );
        let button = copy_bounds(client_rect(window).unwrap(), GetDpiForWindow(window));
        let point = LPARAM((((button.top + 1) << 16) | (button.left + 1)) as isize);
        SendMessageW(window, WM_LBUTTONUP, None, Some(point));
        assert_eq!(copies.get(), 0);
        SendMessageW(window, WM_LBUTTONDOWN, None, Some(point));
        SendMessageW(window, WM_LBUTTONUP, None, Some(point));
        assert_eq!(copies.get(), 1);
        assert!(!IsWindowVisible(window).as_bool());
        assert_ne!(GetForegroundWindow(), window);
        DestroyWindow(window).unwrap();
    }
}
