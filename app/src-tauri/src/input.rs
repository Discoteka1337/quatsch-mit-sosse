use quatsch_mit_sosse_hotkey::{HookNotice, HotkeyConfig, HotkeyEvent, HotkeyHook};
use quatsch_mit_sosse_inject::{InjectConfig, InjectError, InsertTarget};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId,
};

pub(super) enum Action {
    Start(Option<Target>),
    Stop,
    Cancel,
    Reinsert(Option<Target>),
    Elevated,
    TargetAvailable,
    TargetUnavailable,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Target(InsertTarget);

impl Target {
    pub(super) fn insert(self, text: &str) -> Result<(), InjectError> {
        self.0.check_foreground()?;
        // A repeat shortcut fires on key-down. SendInput must see released modifiers.
        await_release(
            || {
                [VK_CONTROL, VK_SHIFT, VK_MENU, VK_LWIN, VK_RWIN]
                    .into_iter()
                    .any(|key| unsafe { GetAsyncKeyState(i32::from(key.0)) < 0 })
            },
            || std::thread::sleep(Duration::from_millis(15)),
            100,
        )?;
        self.0.insert(text, &InjectConfig::load()?)?;
        Ok(())
    }
}

fn await_release(
    mut held: impl FnMut() -> bool,
    mut wait: impl FnMut(),
    attempts: usize,
) -> Result<(), InjectError> {
    for _ in 0..attempts {
        if !held() {
            return Ok(());
        }
        wait();
    }
    Err(InjectError::ModifiersHeld)
}

#[derive(Clone, Copy)]
enum Surface {
    External(Target),
    Tray,
    Blocked,
}

fn is_tray_class(name: &str, own: bool) -> bool {
    matches!(
        name,
        "Shell_TrayWnd"
            | "Shell_SecondaryTrayWnd"
            | "NotifyIconOverflowWindow"
            | "TopLevelWindowForOverflowXamlIsland"
            | "#32768"
    ) || (own && name == "tray_icon_app")
}

fn surface(hwnd: HWND, available: bool) -> Surface {
    let mut process = 0;
    let mut class = [0; 256];
    let length = unsafe { GetClassNameW(hwnd, &mut class) };
    if length == 0 || unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process)) } == 0 {
        return Surface::Blocked;
    }
    let name = String::from_utf16_lossy(&class[..length as usize]);
    let own = process == std::process::id();
    if is_tray_class(&name, own) {
        return Surface::Tray;
    }
    if !available || own || matches!(name.as_str(), "Progman" | "WorkerW") {
        return Surface::Blocked;
    }
    InsertTarget::from_window(hwnd)
        .map(|target| Surface::External(Target(target)))
        .unwrap_or(Surface::Blocked)
}

#[derive(Default)]
struct Focus {
    last_external: Option<Target>,
    menu_target: Option<Target>,
}

impl Focus {
    fn observe(&mut self, surface: Surface) {
        match surface {
            Surface::External(target) => self.last_external = Some(target),
            Surface::Tray => self.menu_target = self.last_external,
            Surface::Blocked => *self = Self::default(),
        }
    }

    fn tray_selection(&self, active: Surface) -> Option<Target> {
        match active {
            // The menu command may arrive before its queued foreground notification.
            Surface::Tray => self.last_external,
            // Windows may already have restored the menu's application.
            Surface::External(target) => self.menu_target.filter(|former| *former == target),
            Surface::Blocked => None,
        }
    }
}

pub(super) struct Shortcuts {
    _hook: HotkeyHook,
    focus: Arc<Mutex<Focus>>,
    dispatch: Arc<dyn Fn(Action) + Send + Sync>,
}

impl Shortcuts {
    pub(super) fn start(
        dispatch: impl Fn(Action) + Send + Sync + 'static,
    ) -> std::io::Result<Self> {
        let dispatch = Arc::new(dispatch);
        let focus = Arc::new(Mutex::new(Focus::default()));
        let hook_dispatch = dispatch.clone();
        let hook_focus = focus.clone();
        let mut current = None;
        let hook = HotkeyHook::start_observed(HotkeyConfig::default(), move |notice| {
            let action = match notice {
                HookNotice::Foreground { window, available } => {
                    let surface = surface(HWND(window as _), available);
                    current = match surface {
                        Surface::External(target) => Some(target),
                        _ => None,
                    };
                    hook_focus
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .observe(surface);
                    if available {
                        hook_dispatch(Action::TargetAvailable);
                    }
                    return;
                }
                HookNotice::Action(event) => match event {
                    HotkeyEvent::Start => Action::Start(current),
                    HotkeyEvent::Stop => Action::Stop,
                    HotkeyEvent::Cancel => Action::Cancel,
                    HotkeyEvent::ReinsertLast => Action::Reinsert(current),
                    HotkeyEvent::ElevatedTarget => Action::Elevated,
                },
                HookNotice::Unavailable => Action::TargetUnavailable,
                HookNotice::Stopped => Action::Unavailable,
            };
            hook_dispatch(action);
        })?;
        Ok(Self {
            _hook: hook,
            focus,
            dispatch,
        })
    }

    pub(super) fn reinsert_from_tray(&self) {
        let menu_owner = unsafe { GetForegroundWindow() };
        let active = surface(menu_owner, true);
        let selected = self
            .focus
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .tray_selection(active);
        // Release the lock before asking Windows to activate the previous editor.
        let target = selected.filter(|target| target.0.restore_after_menu(menu_owner).is_ok());
        (self.dispatch)(Action::Reinsert(target));
    }
}

#[cfg(test)]
mod tests;
