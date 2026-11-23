use super::*;

#[test]
fn unqueryable_foreground_invalidates_the_pending_target_even_without_a_keypress() {
    use windows::Win32::UI::WindowsAndMessaging::GetDesktopWindow;
    let former = Target(InsertTarget::from_window(unsafe { GetDesktopWindow() }).unwrap());
    let focus = Mutex::new(Focus {
        last_external: Some(former),
        menu_target: Some(former),
    });
    let mut current = Some(former);
    let action = action_for_notice(
        HookNotice::Foreground {
            window: 0,
            available: false,
        },
        None,
        &mut current,
        &focus,
    );
    assert!(matches!(action, Action::ForegroundBlocked));
    assert!(current.is_none());
    assert!(
        focus
            .lock()
            .unwrap()
            .tray_selection(Surface::Tray)
            .is_none()
    );
}

#[test]
fn only_tray_surfaces_preserve_the_previous_application() {
    for name in [
        "Shell_TrayWnd",
        "Shell_SecondaryTrayWnd",
        "NotifyIconOverflowWindow",
        "TopLevelWindowForOverflowXamlIsland",
        "#32768",
    ] {
        assert!(is_tray_class(name, false));
    }
    assert!(is_tray_class("tray_icon_app", true));
    assert!(!is_tray_class("tray_icon_app", false));
    assert!(!is_tray_class("CabinetWClass", false));
    assert!(!is_tray_class("Window Class", true));
}

#[test]
fn shortcut_modifiers_can_be_released_before_delivery_or_time_out() {
    let mut probes = 0;
    assert_eq!(
        await_release(
            || {
                probes += 1;
                probes <= 2
            },
            || {},
            3
        ),
        Ok(())
    );
    assert_eq!(probes, 3);
    assert_eq!(
        await_release(|| true, || {}, 3),
        Err(InjectError::ModifiersHeld)
    );
}

#[test]
fn menu_command_never_follows_a_switch_to_an_unrelated_window() {
    use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow};
    use windows::core::w;
    let windows = [(); 2].map(|_| unsafe {
        CreateWindowExW(
            Default::default(),
            w!("STATIC"),
            None,
            Default::default(),
            0,
            0,
            1,
            1,
            None,
            None,
            None,
            None,
        )
        .unwrap()
    });
    let [editor, other] = windows.map(|window| Target(InsertTarget::from_window(window).unwrap()));
    let mut focus = Focus::default();
    focus.observe(Surface::External(editor));
    focus.observe(Surface::Tray);
    assert_eq!(focus.tray_selection(Surface::Tray), Some(editor));
    assert_eq!(
        focus.tray_selection(Surface::External(editor)),
        Some(editor)
    );
    focus.observe(Surface::External(other));
    assert_eq!(focus.tray_selection(Surface::External(other)), None);
    focus.observe(Surface::Blocked);
    focus.observe(Surface::Tray);
    assert_eq!(focus.tray_selection(Surface::Tray), None);
    for window in windows {
        unsafe { DestroyWindow(window).unwrap() }
    }
}

#[test]
fn comparison_receives_the_foreground_at_the_time_of_the_hotkey() {
    use windows::Win32::UI::WindowsAndMessaging::GetDesktopWindow;
    let editor = Target(InsertTarget::from_window(unsafe { GetDesktopWindow() }).unwrap());
    let focus = Mutex::new(Focus::default());
    for expected in [Some(editor), None] {
        let mut current = expected;
        let action = action_for_notice(
            HookNotice::Action(HotkeyEvent::ShowChanges),
            None,
            &mut current,
            &focus,
        );
        assert!(matches!(action, Action::ShowChanges(target) if target == expected));
    }
}
