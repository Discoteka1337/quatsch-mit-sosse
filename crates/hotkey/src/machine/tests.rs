use super::StateMachine;
use crate::HotkeyEvent::{Cancel, ReinsertLast, Start, Stop};
use crate::{HotkeyConfig, HotkeyEvent, HotkeyMode};

const SPACE: u32 = 0x20;
const ESC: u32 = 0x1b;
const F8: u32 = 0x77;
const LEFT_CTRL: u32 = 0xa2;
const RIGHT_CTRL: u32 = 0xa3;
const LEFT_SHIFT: u32 = 0xa0;
const RIGHT_SHIFT: u32 = 0xa1;
const LEFT_ALT: u32 = 0xa4;
const RIGHT_ALT: u32 = 0xa5;
const LEFT_WIN: u32 = 0x5b;

fn machine(dictation: &str, mode: HotkeyMode) -> StateMachine {
    StateMachine::new(HotkeyConfig {
        dictation: dictation.parse().unwrap(),
        reinsert: "Strg+Win+F8".parse().unwrap(),
        mode,
    })
}

fn standard(mode: HotkeyMode) -> StateMachine {
    machine("Strg+Umschalt+Leertaste", mode)
}

fn stroke(machine: &mut StateMachine, vk: u32, down: bool, event: Option<HotkeyEvent>, eat: bool) {
    let outcome = machine.key(vk, down);
    assert_eq!((outcome.event, outcome.suppress), (event, eat));
}

fn start(machine: &mut StateMachine) {
    stroke(machine, RIGHT_SHIFT, true, None, false);
    stroke(machine, LEFT_CTRL, true, None, false);
    stroke(machine, SPACE, true, Some(Start), true);
}

#[test]
fn hold_starts_and_stops_once_and_swallows_the_trigger() {
    let mut machine = standard(HotkeyMode::Hold);
    start(&mut machine);
    stroke(&mut machine, SPACE, true, None, true);
    stroke(&mut machine, SPACE, false, Some(Stop), true);
    stroke(&mut machine, LEFT_CTRL, false, None, false);
    stroke(&mut machine, RIGHT_SHIFT, false, None, false);
}

#[test]
fn first_missing_modifier_stops_hold_but_trigger_remains_captured() {
    let mut machine = standard(HotkeyMode::Hold);
    start(&mut machine);
    stroke(&mut machine, RIGHT_CTRL, true, None, false);
    stroke(&mut machine, LEFT_CTRL, false, None, false);
    stroke(&mut machine, RIGHT_CTRL, false, Some(Stop), false);
    stroke(&mut machine, LEFT_CTRL, true, None, false);
    stroke(&mut machine, SPACE, true, None, true);
    stroke(&mut machine, SPACE, false, None, true);
    stroke(&mut machine, SPACE, true, Some(Start), true);
}

#[test]
fn toggle_needs_two_distinct_trigger_presses() {
    let mut machine = standard(HotkeyMode::Toggle);
    start(&mut machine);
    stroke(&mut machine, SPACE, true, None, true);
    stroke(&mut machine, LEFT_CTRL, false, None, false);
    stroke(&mut machine, SPACE, false, None, true);
    stroke(&mut machine, RIGHT_CTRL, true, None, false);
    stroke(&mut machine, SPACE, true, Some(Stop), true);
    stroke(&mut machine, SPACE, true, None, true);
    stroke(&mut machine, SPACE, false, None, true);
}

#[test]
fn escape_cancels_both_modes_and_only_captures_the_active_press() {
    for mode in [HotkeyMode::Hold, HotkeyMode::Toggle] {
        let mut machine = standard(mode);
        stroke(&mut machine, ESC, true, None, false);
        stroke(&mut machine, ESC, false, None, false);
        start(&mut machine);
        stroke(&mut machine, ESC, true, Some(Cancel), true);
        stroke(&mut machine, ESC, true, None, true);
        stroke(&mut machine, ESC, false, None, true);
        stroke(&mut machine, SPACE, false, None, true);
        stroke(&mut machine, ESC, true, None, false);
        stroke(&mut machine, ESC, false, None, false);
        stroke(&mut machine, SPACE, true, Some(Start), true);
    }
}

#[test]
fn reinsert_emits_once_and_keeps_toggle_recording_active() {
    let mut machine = standard(HotkeyMode::Toggle);
    start(&mut machine);
    stroke(&mut machine, SPACE, false, None, true);
    stroke(&mut machine, RIGHT_SHIFT, false, None, false);
    stroke(&mut machine, LEFT_WIN, true, None, false);
    stroke(&mut machine, F8, true, Some(ReinsertLast), true);
    stroke(&mut machine, F8, true, None, true);
    stroke(&mut machine, LEFT_WIN, false, None, false);
    stroke(&mut machine, F8, false, None, true);
    stroke(&mut machine, ESC, true, Some(Cancel), true);
}

#[test]
fn modifier_only_chord_activates_on_the_last_required_modifier() {
    let mut machine = machine("Strg+Win", HotkeyMode::Hold);
    stroke(&mut machine, LEFT_WIN, true, None, false);
    stroke(&mut machine, RIGHT_CTRL, true, Some(Start), true);
    stroke(&mut machine, RIGHT_CTRL, true, None, true);
    // Forwarding the second Control would give the target the complete shortcut.
    stroke(&mut machine, LEFT_CTRL, true, None, true);
    stroke(&mut machine, LEFT_CTRL, true, None, true);
    stroke(&mut machine, RIGHT_CTRL, false, None, true);
    stroke(&mut machine, LEFT_WIN, false, Some(Stop), false);
    stroke(&mut machine, LEFT_CTRL, false, None, true);
}

#[test]
fn altgr_typing_passes_through_including_a_configured_control_alt_binding() {
    for dictation in ["Strg+Umschalt+Leertaste", "Strg+Alt+Leertaste"] {
        let mut machine = machine(dictation, HotkeyMode::Hold);
        stroke(&mut machine, LEFT_CTRL, true, None, false);
        stroke(&mut machine, RIGHT_ALT, true, None, false);
        stroke(&mut machine, SPACE, true, None, false);
        stroke(&mut machine, SPACE, false, None, false);
        stroke(&mut machine, RIGHT_ALT, false, None, false);
    }
    let mut machine = machine("Strg+Alt+Leertaste", HotkeyMode::Hold);
    stroke(&mut machine, RIGHT_CTRL, true, None, false);
    stroke(&mut machine, LEFT_ALT, true, None, false);
    stroke(&mut machine, SPACE, true, Some(Start), true);
}

#[test]
fn altgr_without_a_visible_control_prefix_cannot_activate_alt_bindings() {
    for dictation in ["Alt", "Alt+Q"] {
        let mut machine = machine(dictation, HotkeyMode::Hold);
        stroke(&mut machine, RIGHT_ALT, true, None, false);
        stroke(&mut machine, u32::from(b'Q'), true, None, false);
        stroke(&mut machine, u32::from(b'Q'), false, None, false);
        stroke(&mut machine, RIGHT_ALT, false, None, false);
    }
}

#[test]
fn altgr_does_not_keep_an_alt_only_hold_alive_after_left_alt_is_released() {
    let mut machine = machine("Alt", HotkeyMode::Hold);
    stroke(&mut machine, LEFT_ALT, true, Some(Start), true);
    stroke(&mut machine, RIGHT_ALT, true, None, false);
    stroke(&mut machine, LEFT_ALT, false, Some(Stop), true);
    stroke(&mut machine, RIGHT_ALT, false, None, false);
}

#[test]
fn new_modifiers_do_not_turn_a_forwarded_key_repeat_into_a_hotkey() {
    let mut machine = standard(HotkeyMode::Toggle);
    stroke(&mut machine, SPACE, true, None, false);
    stroke(&mut machine, LEFT_CTRL, true, None, false);
    stroke(&mut machine, RIGHT_SHIFT, true, None, false);
    stroke(&mut machine, SPACE, true, None, false);
    stroke(&mut machine, SPACE, false, None, false);
    stroke(&mut machine, SPACE, true, Some(Start), true);
}

#[test]
fn extra_modifiers_and_unrelated_keys_pass_through() {
    let mut machine = standard(HotkeyMode::Hold);
    stroke(&mut machine, RIGHT_CTRL, true, None, false);
    stroke(&mut machine, LEFT_SHIFT, true, None, false);
    stroke(&mut machine, LEFT_ALT, true, None, false);
    stroke(&mut machine, SPACE, true, None, false);
    stroke(&mut machine, SPACE, false, None, false);
    stroke(&mut machine, 'A' as u32, true, None, false);
    stroke(&mut machine, 'A' as u32, false, None, false);
    stroke(&mut machine, u32::MAX, true, None, false);
}

#[test]
fn resync_preserves_capture_for_held_keys_and_clears_missing_keyups() {
    let mut machine = standard(HotkeyMode::Hold);
    start(&mut machine);
    assert!(machine.cancel_recording());
    assert!(!machine.cancel_recording());
    machine.sync_down(SPACE, true);
    stroke(&mut machine, SPACE, true, None, true);
    stroke(&mut machine, SPACE, false, None, true);
    stroke(&mut machine, SPACE, true, Some(Start), true);
    machine.cancel_recording();
    machine.sync_down(SPACE, false);
    machine.sync_down(LEFT_CTRL, false);
    machine.sync_down(RIGHT_CTRL, true);
    stroke(&mut machine, SPACE, true, Some(Start), true);
}

#[test]
fn already_held_trigger_at_installation_is_released_to_target() {
    let mut machine = standard(HotkeyMode::Hold);
    for vk in [LEFT_CTRL, RIGHT_SHIFT, SPACE] {
        machine.sync_down(vk, true);
    }
    stroke(&mut machine, SPACE, true, None, false);
    stroke(&mut machine, SPACE, false, None, false);
    stroke(&mut machine, SPACE, true, Some(Start), true);
}
