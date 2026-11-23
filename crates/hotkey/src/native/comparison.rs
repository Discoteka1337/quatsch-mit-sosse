use crate::binding::{ALT, CTRL, SHIFT, WIN};
use crate::{Hotkey, HotkeyConfig};
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey, UnregisterHotKey,
};

pub(super) const ID: i32 = 0x514d;

#[derive(Default)]
pub(super) struct Press {
    held: bool,
    origin: Option<HWND>,
}

impl Press {
    pub(super) fn key(&mut self, down: bool, physical_origin: Option<HWND>) {
        if down && !self.held {
            self.origin = physical_origin;
        }
        self.held = down;
    }

    pub(super) fn take(&mut self, foreground: HWND) -> bool {
        self.origin.take() == Some(foreground)
    }
}

pub(super) fn choose(
    config: HotkeyConfig,
    mut register: impl FnMut(Hotkey) -> bool,
) -> Option<Hotkey> {
    // Keep custom dictation bindings, including a modifiers-only prefix, usable.
    [
        "Strg+Win+F9",
        "Strg+Win+F10",
        "Strg+Win+F11",
        "Umschalt+Alt+J",
        "F13",
    ]
    .into_iter()
    .map(|text| text.parse::<Hotkey>().expect("valid built-in shortcut"))
    .find(|&candidate| {
        [config.dictation, config.reinsert]
            .into_iter()
            .all(|existing| {
                existing != candidate
                    && (existing.trigger().is_some()
                        || candidate.modifiers() & existing.modifiers() != existing.modifiers())
            })
            && register(candidate)
    })
}

fn register(key: Hotkey) -> bool {
    let mut flags = MOD_NOREPEAT;
    for (bit, modifier) in [
        (CTRL, MOD_CONTROL),
        (ALT, MOD_ALT),
        (SHIFT, MOD_SHIFT),
        (WIN, MOD_WIN),
    ] {
        if key.modifiers() & bit != 0 {
            flags |= modifier;
        }
    }
    unsafe { RegisterHotKey(None, ID, flags, key.trigger().expect("comparison trigger")).is_ok() }
}

pub(super) struct Registration {
    active: Option<Hotkey>,
    published: Arc<Mutex<Option<Hotkey>>>,
}

impl Registration {
    pub(super) fn trigger(&self) -> Option<u32> {
        self.active.and_then(Hotkey::trigger)
    }

    pub(super) fn new(config: HotkeyConfig, published: Arc<Mutex<Option<Hotkey>>>) -> Self {
        let active = choose(config, register);
        *published.lock().unwrap() = active;
        Self { active, published }
    }

    pub(super) fn update(&mut self, config: Option<HotkeyConfig>) {
        if self.active.take().is_some() {
            unsafe {
                let _ = UnregisterHotKey(None, ID);
            }
        }
        self.active = config.and_then(|config| choose(config, register));
        *self.published.lock().unwrap() = self.active;
    }

    pub(super) fn matches(&self, message: isize) -> bool {
        self.active.is_some_and(|key| {
            let flags = message as u32 & 0xffff;
            let trigger = message as u32 >> 16;
            Hotkey::from_virtual_keys(
                flags & MOD_CONTROL.0 != 0,
                flags & MOD_SHIFT.0 != 0,
                flags & MOD_ALT.0 != 0,
                flags & MOD_WIN.0 != 0,
                Some(trigger),
            ) == Some(key)
        })
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.update(None);
    }
}
