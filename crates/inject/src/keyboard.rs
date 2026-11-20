use std::{thread, time::Duration};

use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RETURN, VK_RWIN,
    VK_SHIFT, VK_V,
};

use super::{InjectError, Target};

const BLOCK_CHARS: usize = 16;

pub(crate) fn insert(target: &Target, text: &str) -> Result<(), InjectError> {
    type_text(
        text,
        |started| {
            if started {
                // Give the receiver an opportunity to process input and any resulting focus change.
                thread::sleep(Duration::from_millis(4));
            }
            check_modifiers()?;
            target.check()
        },
        send,
    )?;
    target.check()
}

pub(crate) fn paste_shortcut(target: &Target) -> Result<(), InjectError> {
    check_modifiers()?;
    if key_is_down(VK_V) {
        return Err(InjectError::ModifiersHeld);
    }
    target.check()?;
    send_block(&paste_events(), false, &mut send)
}

fn check_modifiers() -> Result<(), InjectError> {
    if [VK_CONTROL, VK_SHIFT, VK_MENU, VK_LWIN, VK_RWIN]
        .into_iter()
        .any(key_is_down)
    {
        return Err(InjectError::ModifiersHeld);
    }
    Ok(())
}

fn key_is_down(key: VIRTUAL_KEY) -> bool {
    // Only the high bit reports the current state.
    unsafe { GetAsyncKeyState(i32::from(key.0)) < 0 }
}

fn type_text(
    text: &str,
    mut before_block: impl FnMut(bool) -> Result<(), InjectError>,
    mut send: impl FnMut(&[KEYBDINPUT]) -> usize,
) -> Result<(), InjectError> {
    let mut chars = text.chars().peekable();
    let mut block = Vec::with_capacity(BLOCK_CHARS * 4);
    let mut started = false;
    while chars.peek().is_some() {
        block.clear();
        for _ in 0..BLOCK_CHARS {
            let Some(character) = chars.next() else { break };
            if matches!(character, '\r' | '\n') {
                if character == '\r' {
                    chars.next_if_eq(&'\n');
                }
                let enter = virtual_key(VK_RETURN);
                block.extend([enter, key_up(enter)]);
                // Enter may submit a form or open a new window.
                break;
            }
            for &unit in character.encode_utf16(&mut [0; 2]).iter() {
                let down = KEYBDINPUT {
                    wScan: unit,
                    dwFlags: KEYEVENTF_UNICODE,
                    ..Default::default()
                };
                block.extend([down, key_up(down)]);
            }
        }
        before_block(started)?;
        send_block(&block, started, &mut send)?;
        started = true;
    }
    Ok(())
}

fn virtual_key(key: VIRTUAL_KEY) -> KEYBDINPUT {
    KEYBDINPUT {
        wVk: key,
        ..Default::default()
    }
}

fn key_up(mut key: KEYBDINPUT) -> KEYBDINPUT {
    key.dwFlags |= KEYEVENTF_KEYUP;
    key
}

fn paste_events() -> [KEYBDINPUT; 4] {
    let control = virtual_key(VK_CONTROL);
    let v = virtual_key(VK_V);
    [control, v, key_up(v), key_up(control)]
}

fn send_block(
    keys: &[KEYBDINPUT],
    started: bool,
    send: &mut impl FnMut(&[KEYBDINPUT]) -> usize,
) -> Result<(), InjectError> {
    let accepted = send(keys);
    if accepted == keys.len() {
        return Ok(());
    }
    if accepted == 0 && !started {
        return Err(InjectError::Unavailable);
    }
    // The chord and all character pairs release keys in reverse press order.
    let mut releases = Vec::new();
    for &key in &keys[..accepted] {
        if key.dwFlags.contains(KEYEVENTF_KEYUP) {
            releases.pop();
        } else {
            releases.push(key_up(key));
        }
    }
    if !releases.is_empty() {
        releases.reverse();
        send(&releases);
    }
    Err(InjectError::DeliveryUncertain)
}

fn send(keys: &[KEYBDINPUT]) -> usize {
    let inputs: Vec<_> = keys
        .iter()
        .map(|&key| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: key },
        })
        .collect();
    // The slice contains initialized keyboard inputs and fits in a single bounded block.
    unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) as usize }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn umlauts_sharp_s_and_surrogates_use_paired_unicode_packets() {
        let text = "äÖüß🙂";
        let mut actual = Vec::new();
        type_text(
            text,
            |_| Ok(()),
            |events| {
                actual.extend_from_slice(events);
                events.len()
            },
        )
        .unwrap();
        let (packets, remainder) = actual.as_chunks::<2>();
        assert!(remainder.is_empty());
        assert_eq!(packets.len(), text.encode_utf16().count());
        for (pair, scan) in packets.iter().zip(text.encode_utf16()) {
            assert_eq!(
                (pair[0].wVk.0, pair[0].wScan, pair[0].dwFlags),
                (0, scan, KEYEVENTF_UNICODE)
            );
            assert_eq!(
                (pair[1].wVk.0, pair[1].wScan, pair[1].dwFlags),
                (0, scan, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP)
            );
        }
    }

    #[test]
    fn crlf_cr_and_lf_each_send_one_enter_and_end_the_block() {
        let mut blocks = Vec::new();
        type_text(
            "ä\r\nö\rß\n🙂",
            |_| Ok(()),
            |events| {
                blocks.push(events.to_vec());
                events.len()
            },
        )
        .unwrap();
        assert_eq!(blocks.len(), 4);
        for block in &blocks[..3] {
            assert_eq!(block.len(), 4);
            assert_eq!(block[2].wVk, VK_RETURN);
            assert_eq!(block[2].dwFlags.0, 0);
            assert_eq!(block[3].wVk, VK_RETURN);
            assert_eq!(block[3].dwFlags, KEYEVENTF_KEYUP);
        }
        assert_eq!(blocks[3].len(), 4);
    }

    #[test]
    fn batch_boundary_keeps_the_entire_supplementary_character_together() {
        let text = "a".repeat(BLOCK_CHARS - 1) + "🙂X";
        let mut lengths = Vec::new();
        type_text(
            &text,
            |_| Ok(()),
            |events| {
                lengths.push(events.len());
                events.len()
            },
        )
        .unwrap();
        assert_eq!(lengths, [2 * (BLOCK_CHARS - 1) + 4, 2]);
    }

    #[test]
    fn focus_loss_between_blocks_stops_before_sending_more_text() {
        let mut sends = 0;
        let result = type_text(
            "x\ny",
            |started| {
                if started {
                    Err(InjectError::TargetChanged)
                } else {
                    Ok(())
                }
            },
            |events| {
                sends += 1;
                events.len()
            },
        );
        assert!(matches!(result, Err(InjectError::TargetChanged)));
        assert_eq!(sends, 1);
    }

    #[test]
    fn only_a_completely_rejected_first_block_allows_fallback() {
        assert!(matches!(
            type_text("x", |_| Ok(()), |_| 0),
            Err(InjectError::Unavailable)
        ));
        let mut calls = 0;
        let result = type_text(
            "x\ny",
            |_| Ok(()),
            |events| {
                calls += 1;
                if calls == 1 { events.len() } else { 0 }
            },
        );
        assert!(matches!(result, Err(InjectError::DeliveryUncertain)));
    }

    #[test]
    fn partial_paste_shortcut_releases_only_the_keys_pressed() {
        for (accepted, expected) in [
            (1, vec![VK_CONTROL]),
            (2, vec![VK_V, VK_CONTROL]),
            (3, vec![VK_CONTROL]),
        ] {
            let mut calls = Vec::new();
            let result = send_block(&paste_events(), false, &mut |events| {
                calls.push(events.to_vec());
                if calls.len() == 1 {
                    accepted
                } else {
                    events.len()
                }
            });
            assert!(matches!(result, Err(InjectError::DeliveryUncertain)));
            assert_eq!(calls.len(), 2);
            assert!(calls[1].iter().all(|key| key.dwFlags == KEYEVENTF_KEYUP));
            assert_eq!(
                calls[1].iter().map(|key| key.wVk).collect::<Vec<_>>(),
                expected
            );
        }
    }

    #[test]
    fn a_partial_unicode_block_never_retypes_its_delivered_prefix() {
        let mut calls = Vec::new();
        let result = type_text(
            "äö",
            |_| Ok(()),
            |events| {
                calls.push(events.to_vec());
                if calls.len() == 1 { 3 } else { events.len() }
            },
        );
        assert!(matches!(result, Err(InjectError::DeliveryUncertain)));
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1].len(), 1);
        assert_eq!(calls[1][0].dwFlags, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP);
        assert_eq!(calls[1][0].wScan, 'ö' as u16);
    }
}
