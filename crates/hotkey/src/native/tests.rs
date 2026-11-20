use super::*;
use crate::HotkeyMode;

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
