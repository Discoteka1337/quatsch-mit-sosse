use super::*;
use crate::HotkeyMode;

#[test]
fn comparison_uses_a_free_registered_chord_without_displacing_existing_shortcuts() {
    let config = HotkeyConfig::default();
    let choose = |config, unavailable: &[&str]| {
        comparison::choose(config, |key| {
            !unavailable.contains(&key.to_string().as_str())
        })
        .map(|key| key.to_string())
    };
    assert_eq!(choose(config, &[]).as_deref(), Some("Strg+Win+F9"));
    let customized = HotkeyConfig {
        dictation: "Strg+Win+F9".parse().unwrap(),
        ..config
    };
    assert_eq!(
        choose(customized, &["Strg+Win+F10"]).as_deref(),
        Some("Strg+Win+F11")
    );
    for prefix in ["Strg", "Win", "Strg+Win"] {
        let customized = HotkeyConfig {
            reinsert: prefix.parse().unwrap(),
            ..config
        };
        assert_eq!(choose(customized, &[]).as_deref(), Some("Umschalt+Alt+J"));
    }
    let mut attempted = Vec::new();
    assert!(
        comparison::choose(config, |key| {
            assert!(key.warnings().is_empty());
            attempted.push(key.to_string());
            false
        })
        .is_none()
    );
    assert_eq!(
        attempted,
        [
            "Strg+Win+F9",
            "Strg+Win+F10",
            "Strg+Win+F11",
            "Umschalt+Alt+J",
            "F13"
        ]
    );
}

#[test]
fn comparison_messages_require_an_available_foreground_outside_shortcut_capture() {
    let (mut session, events) = session();
    session.set_target(Target::Available);
    session.show_changes();
    assert_eq!(events.try_recv().unwrap(), HotkeyEvent::ShowChanges);
    session.change(Control::Capture(true));
    session.show_changes();
    session.change(Control::Capture(false));
    session.set_target(Target::Unknown);
    session.show_changes();
    assert!(events.try_recv().is_err());
}

#[test]
fn comparison_key_repeats_cannot_retarget_its_pending_message() {
    let editor = HWND(1usize as _);
    let other = HWND(2usize as _);
    let mut press = comparison::Press::default();
    press.key(true, Some(editor));
    press.key(true, Some(other));
    assert!(!press.take(other));
    press.key(false, None);
    press.key(true, None); // Injected key-down.
    assert!(!press.take(editor));
    press.key(false, None);
    press.key(true, Some(editor));
    press.key(false, None);
    assert!(press.take(editor)); // Windows can post WM_HOTKEY after key-up.
    assert!(!press.take(editor));
}

fn toggle_feedback() -> (Session, Receiver<(HookNotice, Option<RecordingTicket>)>) {
    let (send, events) = mpsc::channel();
    let config = HotkeyConfig {
        mode: HotkeyMode::Toggle,
        dictation: "F7".parse().unwrap(),
        ..HotkeyConfig::default()
    };
    let mut session = Session::new(
        config,
        Observer::WithFeedback(Box::new(move |event, ticket| {
            send.send((event, ticket)).unwrap();
        })),
    );
    session.set_target(Target::Available);
    (session, events)
}

#[test]
fn rejected_toggle_start_forwards_escape_and_the_next_press_retries() {
    let (mut session, events) = toggle_feedback();
    assert!(session.key(0x76, true));
    let (HookNotice::Action(HotkeyEvent::Start), Some(rejected)) = events.try_recv().unwrap()
    else {
        panic!()
    };
    drop(rejected);
    assert!(session.key(0x76, false));
    assert!(!session.key(0x1b, true));
    assert!(!session.key(0x1b, false));
    assert!(events.try_recv().is_err());
    assert!(session.key(0x76, true));
    assert!(matches!(
        events.try_recv(),
        Ok((HookNotice::Action(HotkeyEvent::Start), Some(_)))
    ));
}

#[test]
fn late_release_of_a_previous_toggle_does_not_cancel_the_new_recording() {
    let (mut session, events) = toggle_feedback();
    let mut press = || {
        assert!(session.key(0x76, true));
        assert!(session.key(0x76, false));
        events.try_recv().unwrap()
    };
    let (_, Some(first)) = press() else { panic!() };
    assert!(matches!(
        press(),
        (HookNotice::Action(HotkeyEvent::Stop), None)
    ));
    let (_, Some(second)) = press() else { panic!() };
    drop(first);
    assert!(matches!(
        press(),
        (HookNotice::Action(HotkeyEvent::Stop), None)
    ));
    drop(second);
}

#[test]
fn aborted_recording_does_not_emit_a_second_cancel_during_capture_or_focus_change() {
    let (mut session, events) = toggle_feedback();
    session.key(0x76, true);
    session.key(0x76, false);
    let (_, Some(recording)) = events.try_recv().unwrap() else {
        panic!()
    };
    drop(recording);
    session.change(Control::Capture(true));
    session.set_target(Target::Unknown);
    assert!(events.try_recv().is_err());
}

fn session() -> (Session, Receiver<HotkeyEvent>) {
    let (send, events) = mpsc::channel();
    (
        Session::new(
            HotkeyConfig::default(),
            Observer::Plain(Box::new(action_sender(send))),
        ),
        events,
    )
}

#[test]
fn observed_shortcuts_report_an_unavailable_target_on_a_blocked_press() {
    let (send, events) = mpsc::channel();
    let mut session = Session::new(
        HotkeyConfig::default(),
        Observer::Plain(Box::new(move |notice| {
            send.send(notice).unwrap();
        })),
    );
    for vk in [0xa2, 0xa0, 0x20] {
        session.key(vk, true);
    }
    assert!(matches!(events.try_recv(), Ok(HookNotice::Unavailable)));
    session.key(0x20, false);
    assert!(events.try_recv().is_err());
}

#[test]
fn lost_integrity_during_recording_reports_the_reason_after_the_cancel() {
    let (send, events) = mpsc::channel();
    let mut session = Session::new(
        HotkeyConfig::default(),
        Observer::Plain(Box::new(move |notice| send.send(notice).unwrap())),
    );
    session.set_target(Target::Available);
    session.set_target(Target::Unknown);
    assert!(events.try_recv().is_err());
    session.set_target(Target::Available);
    for vk in [0xa2, 0xa0, 0x20] {
        session.key(vk, true);
    }
    session.set_target(Target::Unknown);
    assert!(matches!(
        events.try_recv(),
        Ok(HookNotice::Action(HotkeyEvent::Start))
    ));
    assert!(matches!(
        events.try_recv(),
        Ok(HookNotice::Action(HotkeyEvent::Cancel))
    ));
    assert!(matches!(events.try_recv(), Ok(HookNotice::Unavailable)));
    assert!(events.try_recv().is_err());
}

#[test]
fn elevation_is_reported_on_focus_even_without_keyboard_input() {
    let (mut session, events) = session();
    session.set_target(Target::Elevated);
    session.set_target(Target::Elevated);
    assert_eq!(
        events.try_iter().collect::<Vec<_>>(),
        [HotkeyEvent::ElevatedTarget]
    );
    session.set_target(Target::Available);
    session.set_target(Target::Elevated);
    assert_eq!(events.try_recv(), Ok(HotkeyEvent::ElevatedTarget));
}

#[test]
fn blocked_focus_cancels_recording_and_never_finishes_it_into_the_new_target() {
    let (mut session, events) = session();
    session.set_target(Target::Available);
    for vk in [0xa2, 0xa0, 0x20] {
        session.key(vk, true);
    }
    session.set_target(Target::Elevated);
    session.key(0x20, false);
    session.key(0x20, true);
    session.key(0x20, false);
    session.key(0xa0, false);
    session.key(0x5b, true);
    session.key(0x77, true);
    assert_eq!(
        events.try_iter().collect::<Vec<_>>(),
        [
            HotkeyEvent::Start,
            HotkeyEvent::Cancel,
            HotkeyEvent::ElevatedTarget
        ]
    );
}

#[test]
fn unknown_integrity_blocks_actions_until_the_target_can_be_checked() {
    let (mut session, events) = session();
    session.set_target(Target::Unknown);
    for vk in [0xa2, 0xa0, 0x20] {
        session.key(vk, true);
    }
    session.key(0x20, false);
    assert!(events.try_recv().is_err());
    session.set_target(Target::Available);
    session.key(0x20, true);
    assert_eq!(events.try_recv(), Ok(HotkeyEvent::Start));
}

#[test]
fn only_physical_keyboard_messages_reach_the_machine() {
    let mut input = KBDLLHOOKSTRUCT::default();
    for message in [WM_KEYDOWN, WM_SYSKEYDOWN] {
        assert_eq!(physical_transition(message, &input), Some(true));
    }
    for message in [WM_KEYUP, WM_SYSKEYUP] {
        assert_eq!(physical_transition(message, &input), Some(false));
    }
    assert_eq!(physical_transition(WM_QUIT, &input), None);
    input.flags = LLKHF_INJECTED;
    assert_eq!(physical_transition(WM_KEYDOWN, &input), None);
    input.flags = LLKHF_LOWER_IL_INJECTED;
    assert_eq!(physical_transition(WM_KEYUP, &input), None);
}

#[test]
fn altgrs_synthetic_control_never_activates_a_modifier_only_binding() {
    let mut input = KBDLLHOOKSTRUCT {
        vkCode: 0xa2,
        scanCode: 0x21d,
        ..Default::default()
    };
    assert_eq!(physical_transition(WM_KEYDOWN, &input), None);
    assert_eq!(physical_transition(WM_KEYUP, &input), None);
    input.scanCode = 0x1d;
    assert_eq!(physical_transition(WM_KEYDOWN, &input), Some(true));
}

#[test]
#[ignore = "requires a Windows desktop session"]
fn competing_hooks_get_distinct_registered_comparison_keys_and_release_them() {
    let config = HotkeyConfig::default();
    let (first, _) = HotkeyHook::start(config).unwrap();
    let primary = first
        .comparison_shortcut()
        .expect("free comparison shortcut");
    let (second, _) = HotkeyHook::start(config).unwrap();
    let alternative = second
        .comparison_shortcut()
        .expect("free fallback shortcut");
    assert_ne!(alternative, primary);
    drop(first);
    let (replacement, _) = HotkeyHook::start(config).unwrap();
    assert_eq!(replacement.comparison_shortcut(), Some(primary));
}

#[test]
#[ignore = "requires a Windows desktop session"]
fn installs_and_releases_native_hooks_twice_without_keyboard_input() {
    for _ in 0..2 {
        let config = HotkeyConfig {
            mode: HotkeyMode::Hold,
            dictation: "Strg+Umschalt+Win+F24".parse().unwrap(),
            reinsert: "Strg+Umschalt+Win+F23".parse().unwrap(),
        };
        let (hook, events) = HotkeyHook::start(config).unwrap();
        drop(hook);
        assert!(
            events
                .into_iter()
                .all(|event| event == HotkeyEvent::ElevatedTarget)
        );
    }
}
#[test]
fn recording_a_binding_passes_keys_through_and_resumes_with_the_new_config() {
    let (mut session, events) = session();
    session.set_target(Target::Available);
    for vk in [0xa2, 0xa0, 0x20] {
        session.key(vk, true);
    }
    assert_eq!(events.try_recv().unwrap(), HotkeyEvent::Start);
    session.change(Control::Capture(true));
    assert_eq!(events.try_recv().unwrap(), HotkeyEvent::Cancel);
    for vk in [0x20, 0xa0, 0xa2] {
        session.key(vk, false);
    }
    let config = HotkeyConfig {
        dictation: "F7".parse().unwrap(),
        mode: HotkeyMode::Toggle,
        ..HotkeyConfig::default()
    };
    session.change(Control::Configure(config));
    assert!(!session.key(0x76, true));
    assert!(!session.key(0x76, false));
    assert!(events.try_recv().is_err());
    session.change(Control::Capture(false));
    assert!(session.key(0x76, true));
    assert!(session.key(0x76, false));
    assert_eq!(events.try_recv().unwrap(), HotkeyEvent::Start);
    assert!(events.try_recv().is_err());
    assert!(session.key(0x76, true));
    assert_eq!(events.try_recv().unwrap(), HotkeyEvent::Stop);
}
