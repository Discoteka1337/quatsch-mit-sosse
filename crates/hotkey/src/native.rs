use crate::integrity::IntegrityLevel;
use crate::machine::StateMachine;
use crate::{HotkeyConfig, HotkeyEvent};
use std::cell::RefCell;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WAIT_OBJECT_0, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{CreateEventW, INFINITE, SetEvent};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LCONTROL, VK_RMENU};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, EVENT_SYSTEM_FOREGROUND, GetForegroundWindow, HC_ACTION,
    KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLKHF_LOWER_IL_INJECTED, MSG, MWMO_INPUTAVAILABLE,
    MsgWaitForMultipleObjectsEx, PM_REMOVE, PeekMessageW, QS_ALLINPUT, SetWindowsHookExW,
    WH_KEYBOARD_LL, WINEVENT_OUTOFCONTEXT, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN,
    WM_SYSKEYUP,
};
use windows::core::Owned;

/// Foreground identities and actions only. Key events never leave the hook.
pub enum HookNotice {
    Foreground { window: usize, available: bool },
    Action(HotkeyEvent),
    Unavailable,
    Stopped,
}

type Observer = Box<dyn FnMut(HookNotice) + Send>;

fn action_sender(events: Sender<HotkeyEvent>) -> Observer {
    Box::new(move |notice| {
        if let HookNotice::Action(event) = notice {
            let _ = events.send(event);
        }
    })
}

/// Keeps the hook thread alive. Dropping it removes both hooks and joins the thread.
pub struct HotkeyHook {
    shutdown: Arc<OwnedHandle>,
    worker: Option<JoinHandle<()>>,
}

impl HotkeyHook {
    /// Returns after both hooks are installed. Process events outside the hook thread.
    pub fn start(config: HotkeyConfig) -> io::Result<(Self, Receiver<HotkeyEvent>)> {
        let (events, receiver) = mpsc::channel();
        Ok((
            Self::start_observed(config, action_sender(events))?,
            receiver,
        ))
    }

    /// Called on the hook thread. Observers must only capture identities and enqueue work.
    pub fn start_observed(
        config: HotkeyConfig,
        observe: impl FnMut(HookNotice) + Send + 'static,
    ) -> io::Result<Self> {
        config.validate()?;
        let (ready, startup) = mpsc::channel();
        // Transfer sole ownership of the unnamed event to an OS handle shared with the worker.
        let shutdown = Arc::new(unsafe {
            OwnedHandle::from_raw_handle(CreateEventW(None, true, false, None)?.0)
        });
        let stop = Arc::clone(&shutdown);
        let worker = thread::Builder::new()
            .name("qms-hotkey".into())
            .spawn(move || {
                if let Err(error) = run(config, Box::new(observe), &ready, &stop) {
                    let _ = ready.send(Err(error));
                }
            })?;
        let hook = Self {
            shutdown,
            worker: Some(worker),
        };
        startup
            .recv()
            .map_err(|_| io::Error::other("Der Hotkey-Thread wurde beim Start beendet."))??;
        Ok(hook)
    }
}

impl Drop for HotkeyHook {
    fn drop(&mut self) {
        // The event remains valid until both owners have released it.
        unsafe {
            let _ = SetEvent(HANDLE(self.shutdown.as_raw_handle()));
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Unknown,
    Available,
    Elevated,
}

struct Session {
    machine: StateMachine,
    target: Target,
    observe: Observer,
}

impl Session {
    fn new(config: HotkeyConfig, observe: Observer) -> Self {
        Self {
            machine: StateMachine::new(config),
            target: Target::Unknown,
            observe,
        }
    }

    fn set_target(&mut self, target: Target) {
        if self.target == target {
            return;
        }
        self.target = target;
        if target != Target::Available && self.machine.cancel_recording() {
            (self.observe)(HookNotice::Action(HotkeyEvent::Cancel));
            if target == Target::Unknown {
                (self.observe)(HookNotice::Unavailable);
            }
        }
        if target == Target::Elevated {
            (self.observe)(HookNotice::Action(HotkeyEvent::ElevatedTarget));
        }
    }

    fn key(&mut self, vk: u32, down: bool) -> bool {
        let outcome = self.machine.key(vk, down);
        if let Some(event) = outcome.event {
            if self.target == Target::Available {
                (self.observe)(HookNotice::Action(event));
            } else {
                self.machine.cancel_recording();
                if self.target == Target::Unknown {
                    (self.observe)(HookNotice::Unavailable);
                }
            }
        }
        outcome.suppress
    }
}

struct HookState {
    session: Session,
    own_level: IntegrityLevel,
    foreground: HWND,
}

impl HookState {
    fn refresh_target(&mut self, window: HWND) {
        self.foreground = window;
        let target = match self.own_level.window_is_higher(window) {
            Ok(true) => Target::Elevated,
            Ok(false) => Target::Available,
            Err(_) => Target::Unknown,
        };
        if self.session.target != Target::Available && target == Target::Available {
            // UIPI can hide entire key transitions. Reconcile only shortcut-related keys.
            let altgr = unsafe { GetAsyncKeyState(i32::from(VK_RMENU.0)) } < 0;
            for vk in self.session.machine.relevant_keys() {
                // AltGr may contribute a synthetic down bit for left Control.
                let down = !(altgr && vk == u32::from(VK_LCONTROL.0))
                    && unsafe { GetAsyncKeyState(vk as i32) } < 0;
                self.session.machine.sync_down(vk, down);
            }
        }
        (self.session.observe)(HookNotice::Foreground {
            window: window.0 as usize,
            available: target == Target::Available,
        });
        self.session.set_target(target);
    }
}

// Both out-of-context callbacks run on the thread that installed the hooks.
thread_local! { static STATE: RefCell<Option<HookState>> = const { RefCell::new(None) }; }

fn run(
    config: HotkeyConfig,
    observe: Observer,
    ready: &Sender<io::Result<()>>,
    stop: &OwnedHandle,
) -> io::Result<()> {
    let own_level = IntegrityLevel::current()?;
    let mut state = HookState {
        session: Session::new(config, observe),
        own_level,
        foreground: HWND::default(),
    };
    // Owned hook handles are unregistered on this thread, including on partial startup.
    let _keyboard = unsafe {
        Owned::new(SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(keyboard_proc),
            Some(GetModuleHandleW(None)?.into()),
            0,
        )?)
    };
    let foreground = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(foreground_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };
    if foreground.is_invalid() {
        return Err(io::Error::other(
            "Der Vordergrundwechsel konnte nicht überwacht werden.",
        ));
    }
    let _foreground = unsafe { Owned::new(foreground) };
    state.refresh_target(unsafe { GetForegroundWindow() });
    STATE.set(Some(state));
    let stopped_normally =
        ready.send(Ok(())).is_err() || pump_messages(HANDLE(stop.as_raw_handle()));
    if let Some(mut state) = STATE.take() {
        state.session.set_target(Target::Unknown);
        if !stopped_normally {
            (state.session.observe)(HookNotice::Stopped);
        }
    }
    Ok(())
}

fn pump_messages(stop: HANDLE) -> bool {
    let mut message = MSG::default();
    // Wait without polling. PeekMessage also delivers callbacks sent to this thread.
    loop {
        let wait = unsafe {
            MsgWaitForMultipleObjectsEx(Some(&[stop]), INFINITE, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
        };
        if wait.0 != WAIT_OBJECT_0.0 + 1 {
            return wait == WAIT_OBJECT_0;
        }
        if unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
            if message.message == WM_QUIT {
                return false;
            }
            unsafe {
                DispatchMessageW(&message);
            }
        }
    }
}

unsafe extern "system" fn foreground_proc(
    _: HWINEVENTHOOK,
    _: u32,
    _: HWND,
    _: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    STATE.with(|slot| {
        if let Ok(mut state) = slot.try_borrow_mut()
            && let Some(state) = state.as_mut()
        {
            // Queued notifications can outlive the window's time in the foreground.
            state.refresh_target(unsafe { GetForegroundWindow() });
        }
    });
}

fn physical_transition(message: u32, input: &KBDLLHOOKSTRUCT) -> Option<bool> {
    // The keyboard layout can synthesize left Control for AltGr without LLKHF_INJECTED.
    if (input.flags & (LLKHF_INJECTED | LLKHF_LOWER_IL_INJECTED)).0 != 0
        || (input.vkCode == u32::from(VK_LCONTROL.0) && input.scanCode == 0x21d)
    {
        return None;
    }
    match message {
        WM_KEYDOWN | WM_SYSKEYDOWN => Some(true),
        WM_KEYUP | WM_SYSKEYUP => Some(false),
        _ => None,
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, message: WPARAM, data: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // HC_ACTION guarantees a live KBDLLHOOKSTRUCT for the duration of this call.
        let input = unsafe { &*(data.0 as *const KBDLLHOOKSTRUCT) };
        if let Some(down) = physical_transition(message.0 as u32, input) {
            let suppress = STATE.with(|slot| {
                let Ok(mut state) = slot.try_borrow_mut() else {
                    return false;
                };
                let Some(state) = state.as_mut() else {
                    return false;
                };
                if !state.session.machine.is_relevant(input.vkCode) {
                    return false;
                }
                let window = unsafe { GetForegroundWindow() };
                // A key may arrive before the queued foreground-change notification.
                if window != state.foreground
                    || (down && state.session.target == Target::Unknown && !window.is_invalid())
                {
                    state.refresh_target(window);
                }
                state.session.key(input.vkCode, down)
            });
            if suppress {
                return LRESULT(1);
            }
        }
    }
    unsafe { CallNextHookEx(None, code, message, data) }
}

#[cfg(test)]
mod tests;
