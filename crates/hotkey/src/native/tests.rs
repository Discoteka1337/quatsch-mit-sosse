use super::*;
use crate::HotkeyMode;

fn session() -> (Session, Receiver<HotkeyEvent>) {
    let (send, events) = mpsc::channel();
    (Session::new(HotkeyConfig::default(), send), events)
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
