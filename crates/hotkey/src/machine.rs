use crate::binding::{ALT, CTRL, SHIFT, WIN};
use crate::{Hotkey, HotkeyConfig, HotkeyEvent, HotkeyMode};

const ESCAPE: u32 = 0x1b;
const RIGHT_ALT: u32 = 0xa5;
const MODIFIERS: [(u8, &[u32]); 4] = [
    (CTRL, &[0xa2, 0xa3]),
    (SHIFT, &[0xa0, 0xa1]),
    (ALT, &[0xa4]), // Right Alt is reserved for AltGr.
    (WIN, &[0x5b, 0x5c]),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum KeyState {
    Up,
    Forwarded,
    Captured,
}

pub(crate) struct KeyOutcome {
    pub(crate) event: Option<HotkeyEvent>,
    pub(crate) suppress: bool,
}

pub(crate) struct StateMachine {
    config: HotkeyConfig,
    recording: bool,
    // Only modifiers, Escape and the configured triggers enter this table.
    keys: [KeyState; 256],
}

impl StateMachine {
    pub(crate) fn new(config: HotkeyConfig) -> Self {
        Self {
            config,
            recording: false,
            keys: [KeyState::Up; 256],
        }
    }

    pub(crate) fn cancel_recording(&mut self) -> bool {
        std::mem::take(&mut self.recording)
    }

    // The hook can miss transitions while an elevated window has focus.
    pub(crate) fn sync_down(&mut self, vk: u32, down: bool) {
        if self.is_relevant(vk) {
            let state = &mut self.keys[vk as usize];
            if !down {
                *state = KeyState::Up;
            } else if *state == KeyState::Up {
                *state = KeyState::Forwarded;
            }
        }
    }

    pub(crate) fn key(&mut self, vk: u32, down: bool) -> KeyOutcome {
        if !self.is_relevant(vk) {
            return KeyOutcome {
                event: None,
                suppress: false,
            };
        }
        let index = vk as usize;
        let previous = self.keys[index];
        if !down {
            self.keys[index] = KeyState::Up;
            let event = if self.recording
                && self.config.mode == HotkeyMode::Hold
                && !self.still_held(self.config.dictation)
            {
                self.recording = false;
                Some(HotkeyEvent::Stop)
            } else {
                None
            };
            return KeyOutcome {
                event,
                suppress: previous == KeyState::Captured,
            };
        }
        if previous != KeyState::Up {
            return KeyOutcome {
                event: None,
                suppress: previous == KeyState::Captured,
            };
        }

        let before = self.modifiers();
        self.keys[index] = KeyState::Forwarded;
        let event = self.press(vk, before);
        let suppress = event.is_some() || self.joins_captured_modifier(vk);
        if suppress {
            self.keys[index] = KeyState::Captured;
        }
        KeyOutcome { suppress, event }
    }

    fn press(&mut self, vk: u32, before: u8) -> Option<HotkeyEvent> {
        if vk == ESCAPE && self.cancel_recording() {
            return Some(HotkeyEvent::Cancel);
        }

        let modifiers = self.modifiers();
        // AltGr must remain available even if its synthetic Control prefix was filtered.
        if self.is_down(RIGHT_ALT) {
            return None;
        }
        let activates = |hotkey: Hotkey| {
            modifiers == hotkey.modifiers()
                && match hotkey.trigger() {
                    Some(trigger) => trigger == vk,
                    None => modifiers != before,
                }
        };
        if activates(self.config.dictation) {
            self.recording = !self.recording;
            return Some(if self.recording {
                HotkeyEvent::Start
            } else {
                HotkeyEvent::Stop
            });
        }
        activates(self.config.reinsert).then_some(HotkeyEvent::ReinsertLast)
    }

    fn still_held(&self, hotkey: Hotkey) -> bool {
        let required = hotkey.modifiers();
        self.modifiers() & required == required
            && hotkey.trigger().is_none_or(|trigger| self.is_down(trigger))
    }

    fn modifiers(&self) -> u8 {
        MODIFIERS.iter().fold(0, |mask, &(bit, sides)| {
            if sides.iter().any(|&vk| self.is_down(vk)) {
                mask | bit
            } else {
                mask
            }
        })
    }

    fn joins_captured_modifier(&self, vk: u32) -> bool {
        MODIFIERS.iter().any(|(_, sides)| {
            sides.contains(&vk)
                && sides
                    .iter()
                    .any(|&side| self.keys[side as usize] == KeyState::Captured)
        })
    }

    fn is_down(&self, vk: u32) -> bool {
        self.keys[vk as usize] != KeyState::Up
    }

    pub(crate) fn is_relevant(&self, vk: u32) -> bool {
        self.relevant_keys().any(|key| key == vk)
    }

    pub(crate) fn relevant_keys(&self) -> impl Iterator<Item = u32> + use<> {
        MODIFIERS
            .into_iter()
            .flat_map(|(_, sides)| sides.iter().copied())
            .chain([RIGHT_ALT, ESCAPE])
            .chain(self.config.dictation.trigger())
            .chain(self.config.reinsert.trigger())
    }
}

#[cfg(test)]
mod tests;
