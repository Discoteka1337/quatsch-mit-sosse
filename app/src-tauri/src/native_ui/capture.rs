use quatsch_mit_sosse_hotkey::Hotkey;
use std::collections::BTreeSet;

pub(super) fn virtual_key(key: u32, message_flags: isize) -> u32 {
    // WM_(SYS)KEY messages use VK_MENU for both Alt keys. Bit 24 marks right Alt.
    if key == 0x12 && message_flags & (1 << 24) != 0 {
        0xa5
    } else {
        key
    }
}

#[derive(Default)]
pub(super) struct Chord {
    down: BTreeSet<u32>,
    modifiers: [bool; 4],
    trigger: Option<u32>,
    invalid: bool,
}

impl Chord {
    // Only the focused recording field receives keyboard events.
    pub(super) fn key(&mut self, key: u32, pressed: bool) -> Option<Option<Hotkey>> {
        if pressed {
            if !self.down.insert(key) {
                return None;
            }
            let modifier = match key {
                0x11 | 0xa2 | 0xa3 => Some(0),
                0x10 | 0xa0 | 0xa1 => Some(1),
                0x12 | 0xa4 => Some(2),
                0xa5 => {
                    // The global hook always leaves AltGr to the active application.
                    self.invalid = true;
                    return None;
                }
                0x5b | 0x5c => Some(3),
                _ => None,
            };
            if let Some(index) = modifier {
                self.modifiers[index] = true;
            } else if self.trigger.replace(key).is_some() {
                self.invalid = true;
            }
            None
        } else if self.down.remove(&key) && self.down.is_empty() {
            let [ctrl, shift, alt, win] = self.modifiers;
            Some(if self.invalid {
                None
            } else {
                Hotkey::from_virtual_keys(ctrl, shift, alt, win, self.trigger)
            })
        } else {
            None
        }
    }
}
