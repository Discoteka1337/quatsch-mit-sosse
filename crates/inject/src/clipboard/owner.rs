use std::cell::Cell;
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use super::{Clipboard, InjectError, OwnedFormat, Target, set_data};
use windows::Win32::Foundation::{
    ERROR_SUCCESS, GetLastError, HANDLE, HWND, LPARAM, LRESULT, SetLastError, WPARAM,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardOwner,
    GetOpenClipboardWindow, IsClipboardFormatAvailable, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Ole::{CF_UNICODETEXT, CLIPBOARD_FORMAT};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetForegroundWindow,
    GetWindowThreadProcessId, HWND_MESSAGE, MSG, PM_REMOVE, PeekMessageW, RegisterClassW,
    WM_RENDERALLFORMATS, WM_RENDERFORMAT, WNDCLASSW,
};
use windows::core::w;

// Each insertion owns a dedicated worker and its message-only clipboard window.
// Keep the payload in that thread instead of a pointer in window message data.
#[derive(Default)]
struct RenderState {
    owner: Cell<HWND>,
    target: Cell<HWND>,
    pid: Cell<u32>,
    text: Cell<Option<OwnedFormat>>,
    armed: Cell<bool>,
    rendered: Cell<bool>,
}

impl RenderState {
    fn during_paste(
        &self,
        paste: impl FnOnce() -> Result<(), InjectError>,
    ) -> Result<(), InjectError> {
        self.armed.set(true);
        let result = paste();
        self.armed.set(false);
        self.text.take();
        result
    }

    fn render_for(
        &self,
        owner: HWND,
        foreground: HWND,
        reader_pid: Option<u32>,
        deliver: impl FnOnce(OwnedFormat) -> Result<(), InjectError>,
    ) {
        if !self.armed.get()
            || owner != self.owner.get()
            || foreground != self.target.get()
            || self.pid.get() == 0
            || reader_pid != Some(self.pid.get())
        {
            return;
        }
        if let Some(entry) = self.text.take() {
            self.rendered.set(deliver(entry).is_ok());
        }
    }
}

thread_local! { static RENDER: RenderState = RenderState::default(); }

pub(super) struct OwnerWindow<'a> {
    hwnd: HWND,
    on_restore_delay: &'a dyn Fn(),
}

impl<'a> OwnerWindow<'a> {
    pub(super) fn new(
        target: &Target,
        on_restore_delay: &'a dyn Fn(),
    ) -> Result<Self, InjectError> {
        const CLASS: windows::core::PCWSTR = w!("QuatschMitSosse.Inject.Clipboard");
        static REGISTERED: OnceLock<bool> = OnceLock::new();
        let instance = unsafe { GetModuleHandleW(None) }
            .map_err(|_| InjectError::Unavailable)?
            .into();
        if !REGISTERED.get_or_init(|| {
            let class = WNDCLASSW {
                hInstance: instance,
                lpszClassName: CLASS,
                lpfnWndProc: Some(window_proc),
                ..Default::default()
            };
            unsafe { RegisterClassW(&class) != 0 }
        }) {
            return Err(InjectError::Unavailable);
        }
        let hwnd = unsafe {
            CreateWindowExW(
                Default::default(),
                CLASS,
                None,
                Default::default(),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(instance),
                None,
            )
        }
        .map_err(|_| InjectError::Unavailable)?;
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(target.hwnd(), Some(&mut pid)) };
        RENDER.with(|state| {
            state.owner.set(hwnd);
            state.target.set(target.hwnd());
            state.pid.set(pid);
            state.rendered.set(false);
        });
        Ok(Self {
            hwnd,
            on_restore_delay,
        })
    }

    pub(super) fn send_and_wait(&self, target: &Target) -> Result<(), InjectError> {
        RENDER.with(|state| {
            state.during_paste(|| {
                let sent = super::keyboard::paste_shortcut(target);
                if sent.is_ok() || matches!(sent, Err(InjectError::DeliveryUncertain)) {
                    let read = self.wait_for_read(target);
                    sent?;
                    return read;
                }
                sent
            })
        })
    }

    fn pump(&self) {
        let mut message = MSG::default();
        unsafe {
            // PeekMessage dispatches synchronous WM_RENDERFORMAT requests as well.
            if PeekMessageW(&mut message, Some(self.hwnd), 0, 0, PM_REMOVE).as_bool() {
                DispatchMessageW(&message);
            }
        }
    }

    pub(super) fn wait_for_read(&self, target: &Target) -> Result<(), InjectError> {
        await_read(Duration::from_secs(2), || {
            self.pump();
            if !self.owns_contents() {
                return Err(InjectError::ClipboardChanged);
            }
            target
                .check_foreground()
                .map_err(|_| InjectError::DeliveryUncertain)?;
            Ok(RENDER.with(|state| state.rendered.get()))
        })
    }
}

fn await_read(
    timeout: Duration,
    mut poll: impl FnMut() -> Result<bool, InjectError>,
) -> Result<(), InjectError> {
    let deadline = Instant::now() + timeout;
    loop {
        if poll()? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(InjectError::DeliveryUncertain);
        }
        thread::sleep(Duration::from_millis(5));
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_RENDERFORMAT if wp.0 == usize::from(CF_UNICODETEXT.0) => {
            // A foreground match alone does not identify an anonymous clipboard reader.
            let reader_pid = unsafe { GetOpenClipboardWindow() }.ok().map(|reader| {
                let mut pid = 0;
                unsafe { GetWindowThreadProcessId(reader, Some(&mut pid)) };
                pid
            });
            RENDER.with(|state| {
                state.render_for(
                    hwnd,
                    unsafe { GetForegroundWindow() },
                    reader_pid,
                    |entry| {
                        // The requester already has the clipboard open for this callback.
                        entry
                            .publish_with(set_data)
                            .map_err(|_| InjectError::DeliveryUncertain)
                    },
                )
            });
            LRESULT(0)
        }
        // Dictation is temporary. Never persist a pending payload on window destruction.
        WM_RENDERALLFORMATS => LRESULT(0),
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

impl Drop for OwnerWindow<'_> {
    fn drop(&mut self) {
        RENDER.with(|state| {
            state.armed.set(false);
            state.text.take();
            state.owner.set(HWND::default());
        });
        let _ = unsafe { DestroyWindow(self.hwnd) };
    }
}

pub(super) struct ClipboardLock;

impl ClipboardLock {
    pub(super) fn open(hwnd: HWND) -> Result<Self, InjectError> {
        unsafe { OpenClipboard(Some(hwnd)) }
            .map(|_| Self)
            .map_err(|_| InjectError::Unavailable)
    }
}

impl Drop for ClipboardLock {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

impl Clipboard for OwnerWindow<'_> {
    type Lock = ClipboardLock;

    fn open(&self, restoring: bool) -> Result<Self::Lock, InjectError> {
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut notify = Some(self.on_restore_delay);
        loop {
            if let Ok(lock) = ClipboardLock::open(self.hwnd) {
                return Ok(lock);
            }
            if restoring {
                // Keep the snapshot until a busy reader releases our data. Otherwise
                // a transient lock could leave dictated text on the clipboard.
                if !self.owns_contents() {
                    return Err(InjectError::ClipboardChanged);
                }
                if Instant::now() >= deadline
                    && let Some(notify) = notify.take()
                {
                    notify();
                }
            } else if Instant::now() >= deadline {
                return Err(InjectError::Unavailable);
            }
            self.pump();
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn owns_contents(&self) -> bool {
        unsafe { GetClipboardOwner() }.ok() == Some(self.hwnd)
    }

    fn offer_text(&self, text: OwnedFormat) -> Result<(), InjectError> {
        let id = CF_UNICODETEXT.0.into();
        // NULL is also the success return for delayed rendering. Verify the
        // format is advertised while the clipboard is still exclusively open.
        let _ = unsafe { SetClipboardData(id, None) };
        unsafe { IsClipboardFormatAvailable(id) }.map_err(|_| InjectError::Unavailable)?;
        RENDER.with(|state| state.text.set(Some(text)));
        Ok(())
    }

    fn clear(&self) -> Result<(), InjectError> {
        RENDER.with(|state| {
            state.text.take();
            state.armed.set(false);
        });
        unsafe { EmptyClipboard() }.map_err(|_| InjectError::Unavailable)
    }

    fn formats(&self) -> Result<Vec<CLIPBOARD_FORMAT>, InjectError> {
        let mut formats = Vec::new();
        let mut current = 0;
        loop {
            unsafe { SetLastError(ERROR_SUCCESS) };
            current = unsafe { EnumClipboardFormats(current) };
            if current == 0 {
                return if unsafe { GetLastError() } == ERROR_SUCCESS {
                    Ok(formats)
                } else {
                    Err(InjectError::Unavailable)
                };
            }
            let id = u16::try_from(current).map_err(|_| InjectError::Unavailable)?;
            formats.push(CLIPBOARD_FORMAT(id));
        }
    }

    fn get(&self, format: CLIPBOARD_FORMAT) -> Result<HANDLE, InjectError> {
        unsafe { GetClipboardData(format.0.into()) }.map_err(|_| InjectError::Unavailable)
    }

    fn set(&self, format: CLIPBOARD_FORMAT, handle: HANDLE) -> Result<(), InjectError> {
        set_data(format, handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::dangling_mut;

    #[test]
    fn only_a_matching_reader_can_complete_the_offer() {
        let state = RenderState::default();
        let owner = HWND(dangling_mut());
        let target = HWND::default();
        state.owner.set(owner);
        state.target.set(target);
        state.pid.set(42);
        state.text.set(Some(
            OwnedFormat::from_bytes(CF_UNICODETEXT, &[0, 0]).unwrap(),
        ));
        state.render_for(owner, target, Some(42), |_| panic!());
        state.armed.set(true);
        state.render_for(owner, target, None, |_| panic!("unidentified reader"));
        state.render_for(owner, target, Some(17), |_| panic!());
        state.render_for(owner, owner, Some(42), |_| panic!());
        assert!(!state.rendered.get());
        state.render_for(owner, target, Some(42), |_| Ok(()));
        assert!(state.rendered.get());
        assert!(state.text.take().is_none());
        state.render_for(owner, target, Some(42), |_| panic!());
    }

    #[test]
    fn a_failed_paste_withdraws_the_offer_before_waiting_for_a_busy_clipboard() {
        let state = RenderState::default();
        state.pid.set(42);
        state.text.set(Some(
            OwnedFormat::from_bytes(CF_UNICODETEXT, &[0, 0]).unwrap(),
        ));
        let result = state.during_paste(|| {
            assert!(state.armed.get());
            Err(InjectError::DeliveryUncertain)
        });
        assert_eq!(result, Err(InjectError::DeliveryUncertain));
        state.render_for(HWND::default(), HWND::default(), Some(42), |_| panic!());
        assert!(state.text.take().is_none());
    }

    #[test]
    fn a_timeout_or_lost_owner_cannot_report_delivery() {
        assert!(matches!(
            await_read(Duration::ZERO, || Ok(false)),
            Err(InjectError::DeliveryUncertain)
        ));
        assert!(matches!(
            await_read(Duration::ZERO, || Err(InjectError::ClipboardChanged)),
            Err(InjectError::ClipboardChanged)
        ));
        assert!(await_read(Duration::ZERO, || Ok(true)).is_ok());
    }
}
