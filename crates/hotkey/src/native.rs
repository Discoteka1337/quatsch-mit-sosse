use crate::integrity::IntegrityLevel;
use crate::machine::StateMachine;
use crate::{Hotkey, HotkeyConfig, HotkeyEvent};
use std::cell::RefCell;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, Weak};
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
    WH_KEYBOARD_LL, WINEVENT_OUTOFCONTEXT, WM_HOTKEY, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN,
    WM_SYSKEYUP,
};
use windows::core::Owned;

mod comparison;

/// Foreground identities and actions only. Key events never leave the hook.
pub enum HookNotice {
    Foreground { window: usize, available: bool },
    Action(HotkeyEvent),
    ComparisonShortcutUpdated,
    Unavailable,
    Stopped,
}

/// Keep this ticket while the requested recording is pending or active. Dropping it
/// releases only this start, even after the user has already started another recording.
pub struct RecordingTicket {
    _active: Arc<()>,
}

enum Observer {
    Plain(Box<dyn FnMut(HookNotice) + Send>),
    WithFeedback(Box<dyn FnMut(HookNotice, Option<RecordingTicket>) + Send>),
}

impl Observer {
    fn notify(&mut self, notice: HookNotice) {
        match self {
            Self::Plain(send) => send(notice),
            Self::WithFeedback(send) => send(notice, None),
        }
    }

    fn start(&mut self) -> Option<Weak<()>> {
        let notice = HookNotice::Action(HotkeyEvent::Start);
        match self {
            Self::Plain(send) => {
                send(notice);
                None
            }
            Self::WithFeedback(send) => {
                let active = Arc::new(());
                let pending = Arc::downgrade(&active);
                send(notice, Some(RecordingTicket { _active: active }));
                Some(pending)
            }
        }
    }
}

fn action_sender(events: Sender<HotkeyEvent>) -> impl FnMut(HookNotice) + Send {
    move |notice| {
        if let HookNotice::Action(event) = notice {
            let _ = events.send(event);
        }
    }
}

/// Keeps the hook thread alive. Dropping it removes both hooks and joins the thread.
pub struct HotkeyHook {
    wake: Arc<OwnedHandle>,
    comparison: Arc<Mutex<Option<Hotkey>>>,
    commands: Sender<Option<Control>>,
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
        Self::launch(config, Observer::Plain(Box::new(observe)))
    }

    /// Each Start carries an independent ticket. Other notices carry none.
    pub fn start_with_feedback(
        config: HotkeyConfig,
        observe: impl FnMut(HookNotice, Option<RecordingTicket>) + Send + 'static,
    ) -> io::Result<Self> {
        Self::launch(config, Observer::WithFeedback(Box::new(observe)))
    }

    fn launch(config: HotkeyConfig, observe: Observer) -> io::Result<Self> {
        config.validate()?;
        let (ready, startup) = mpsc::channel();
        // Transfer sole ownership of the unnamed event to an OS handle shared with the worker.
        let wake = Arc::new(unsafe {
            OwnedHandle::from_raw_handle(CreateEventW(None, false, false, None)?.0)
        });
        let signal = Arc::clone(&wake);
        let comparison = Arc::new(Mutex::new(None));
        let binding = comparison.clone();
        let (commands, updates) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("qms-hotkey".into())
            .spawn(move || {
                if let Err(error) = run(config, observe, &ready, &signal, updates, binding) {
                    let _ = ready.send(Err(error));
                }
            })?;
        let hook = Self {
            wake,
            comparison,
            commands,
            worker: Some(worker),
        };
        startup
            .recv()
            .map_err(|_| io::Error::other("Der Hotkey-Thread wurde beim Start beendet."))??;
        Ok(hook)
    }

    pub fn configure(&self, config: HotkeyConfig) -> io::Result<()> {
        config.validate()?;
        self.send(Some(Control::Configure(config)))
    }

    /// The comparison shortcut Windows accepted on this desktop.
    pub fn comparison_shortcut(&self) -> Option<Hotkey> {
        *self.comparison.lock().unwrap()
    }

    /// Pass keys through while the focused settings field records a new binding.
    pub fn capture(&self, active: bool) -> io::Result<()> {
        self.send(Some(Control::Capture(active)))
    }

    fn send(&self, command: Option<Control>) -> io::Result<()> {
        self.commands
            .send(command)
            .map_err(|_| io::Error::other("Der Hotkey-Thread wurde beendet."))?;
        unsafe { SetEvent(HANDLE(self.wake.as_raw_handle()))? };
        Ok(())
    }
}

impl Drop for HotkeyHook {
    fn drop(&mut self) {
        // The event remains valid until both owners have released it.
        let _ = self.send(None);
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
    capturing: bool,
    target: Target,
    observe: Observer,
    recording: Option<Weak<()>>,
}

impl Session {
    fn new(config: HotkeyConfig, observe: Observer) -> Self {
        Self {
            machine: StateMachine::new(config),
            capturing: false,
            target: Target::Unknown,
            observe,
            recording: None,
        }
    }

    fn reconcile_recording(&mut self) {
        if self
            .recording
            .as_ref()
            .is_some_and(|ticket| ticket.strong_count() == 0)
        {
            self.machine.cancel_recording();
            self.recording = None;
        }
    }

    fn change(&mut self, command: Control) {
        self.reconcile_recording();
        if self.machine.cancel_recording() {
            self.recording = None;
            self.observe.notify(HookNotice::Action(HotkeyEvent::Cancel));
        }
        match command {
            Control::Configure(config) => self.machine = StateMachine::new(config),
            Control::Capture(active) => self.capturing = active,
        }
    }

    fn set_target(&mut self, target: Target) {
        self.reconcile_recording();
        if self.target == target {
            return;
        }
        self.target = target;
        if target != Target::Available && self.machine.cancel_recording() {
            self.recording = None;
            self.observe.notify(HookNotice::Action(HotkeyEvent::Cancel));
            if target == Target::Unknown {
                self.observe.notify(HookNotice::Unavailable);
            }
        }
        if target == Target::Elevated {
            self.observe
                .notify(HookNotice::Action(HotkeyEvent::ElevatedTarget));
        }
    }

    fn key(&mut self, vk: u32, down: bool) -> bool {
        if self.capturing {
            return false;
        }
        self.reconcile_recording();
        let outcome = self.machine.key(vk, down);
        if let Some(event) = outcome.event {
            if self.target == Target::Available {
                if event == HotkeyEvent::Start {
                    self.recording = self.observe.start();
                } else {
                    if matches!(event, HotkeyEvent::Stop | HotkeyEvent::Cancel) {
                        self.recording = None;
                    }
                    self.observe.notify(HookNotice::Action(event));
                }
            } else {
                self.machine.cancel_recording();
                if self.target == Target::Unknown {
                    self.observe.notify(HookNotice::Unavailable);
                }
            }
        }
        outcome.suppress
    }

    fn show_changes(&mut self) {
        self.reconcile_recording();
        if !self.capturing && self.target == Target::Available {
            self.observe
                .notify(HookNotice::Action(HotkeyEvent::ShowChanges));
        }
    }
}

struct HookState {
    session: Session,
    own_level: IntegrityLevel,
    foreground: HWND,
    comparison: comparison::Registration,
    comparison_press: comparison::Press,
}

enum Control {
    Configure(HotkeyConfig),
    Capture(bool),
}

impl HookState {
    fn sync_keys(&mut self) {
        let altgr = unsafe { GetAsyncKeyState(i32::from(VK_RMENU.0)) } < 0;
        if let Some(trigger) = self.comparison.trigger()
            && unsafe { GetAsyncKeyState(trigger as i32) } >= 0
        {
            self.comparison_press.key(false, None);
        }
        for vk in self.session.machine.relevant_keys() {
            let down = !(altgr && vk == u32::from(VK_LCONTROL.0))
                && unsafe { GetAsyncKeyState(vk as i32) } < 0;
            self.session.machine.sync_down(vk, down);
        }
    }

    fn refresh_target(&mut self, window: HWND) {
        self.foreground = window;
        let target = match self.own_level.window_is_higher(window) {
            Ok(true) => Target::Elevated,
            Ok(false) => Target::Available,
            Err(_) => Target::Unknown,
        };
        if self.session.target != Target::Available && target == Target::Available {
            // UIPI can hide entire key transitions. Reconcile only shortcut-related keys.
            self.sync_keys();
        }
        self.session.observe.notify(HookNotice::Foreground {
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
    wake: &OwnedHandle,
    updates: Receiver<Option<Control>>,
    binding: Arc<Mutex<Option<Hotkey>>>,
) -> io::Result<()> {
    let own_level = IntegrityLevel::current()?;
    let mut state = HookState {
        session: Session::new(config, observe),
        own_level,
        foreground: HWND::default(),
        comparison: comparison::Registration::new(config, binding),
        comparison_press: comparison::Press::default(),
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
        ready.send(Ok(())).is_err() || pump_messages(HANDLE(wake.as_raw_handle()), &updates);
    if let Some(mut state) = STATE.take() {
        state.session.set_target(Target::Unknown);
        if !stopped_normally {
            state.session.observe.notify(HookNotice::Stopped);
        }
    }
    Ok(())
}

fn pump_messages(wake: HANDLE, updates: &Receiver<Option<Control>>) -> bool {
    let mut message = MSG::default();
    // Wait without polling. PeekMessage also delivers callbacks sent to this thread.
    loop {
        let wait = unsafe {
            MsgWaitForMultipleObjectsEx(Some(&[wake]), INFINITE, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
        };
        if wait == WAIT_OBJECT_0 {
            for command in updates.try_iter() {
                let Some(command) = command else { return true };
                STATE.with_borrow_mut(|state| {
                    if let Some(state) = state {
                        state.session.change(command);
                        state.comparison.update(
                            (!state.session.capturing).then_some(state.session.machine.config),
                        );
                        state.comparison_press = comparison::Press::default();
                        state.sync_keys();
                        if !state.session.capturing {
                            state
                                .session
                                .observe
                                .notify(HookNotice::ComparisonShortcutUpdated);
                        }
                    }
                });
            }
            continue;
        }
        if wait.0 != WAIT_OBJECT_0.0 + 1 {
            return false;
        }
        if unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
            if message.message == WM_QUIT {
                return false;
            }
            if message.message == WM_HOTKEY && message.wParam.0 == comparison::ID as usize {
                STATE.with_borrow_mut(|state| {
                    if let Some(state) = state
                        && state.comparison.matches(message.lParam.0)
                    {
                        let current = unsafe { GetForegroundWindow() };
                        if state.comparison_press.take(current) {
                            state.refresh_target(current);
                            state.session.show_changes();
                        }
                    }
                });
                continue;
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
        let transition = physical_transition(message.0 as u32, input);
        let suppress = STATE.with(|slot| {
            let Ok(mut state) = slot.try_borrow_mut() else {
                return false;
            };
            let Some(state) = state.as_mut() else {
                return false;
            };
            if state.comparison.trigger() == Some(input.vkCode) {
                // The OS posts WM_HOTKEY after this hook. Bind it to the real
                // keypress and discard it if the foreground changes in between.
                state.comparison_press.key(
                    matches!(message.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN),
                    (transition == Some(true)).then(|| unsafe { GetForegroundWindow() }),
                );
            }
            let Some(down) = transition else { return false };
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
    unsafe { CallNextHookEx(None, code, message, data) }
}

#[cfg(test)]
mod tests;
