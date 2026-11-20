use super::{InjectError, Target};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::System::Variant::VT_BOOL;
use windows::Win32::UI::Accessibility::{
    CUIAutomation8, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern,
    IUIAutomationTextPattern2, IUIAutomationTextRange, IUIAutomationValuePattern,
    TextPatternRangeEndpoint_End, TextPatternRangeEndpoint_Start, UIA_EditControlTypeId,
    UIA_IsReadOnlyAttributeId, UIA_TextPattern2Id, UIA_TextPatternId, UIA_ValuePatternId,
};
use windows::Win32::UI::Controls::{EM_GETSEL, EM_REPLACESEL};
use windows::Win32::UI::WindowsAndMessaging::{
    ES_MULTILINE, ES_READONLY, GA_ROOT, GUITHREADINFO, GWL_STYLE, GetAncestor, GetClassNameW,
    GetGUIThreadInfo, GetWindowLongW, GetWindowThreadProcessId, IsWindowUnicode, SMTO_ABORTIFHUNG,
    SMTO_ERRORONEXIT, SendMessageTimeoutW,
};
use windows::core::{BOOL, BSTR};

pub(crate) fn insert(target: &Target, text: &str) -> Result<(), InjectError> {
    target.check_foreground()?;
    // The caller supplies a dedicated thread without windows or a pre-existing apartment.
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() }.map_err(unavailable)?;
    let result = insert_in_apartment(target, text);
    unsafe { CoUninitialize() };
    result
}

fn unavailable(_: windows::core::Error) -> InjectError {
    InjectError::Unavailable
}

fn insert_in_apartment(target: &Target, text: &str) -> Result<(), InjectError> {
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER) }
            .map_err(unavailable)?;
    let element = unsafe { automation.GetFocusedElement() }.map_err(unavailable)?;
    check_window(&automation, &element, target)?;
    if !unsafe { element.CurrentIsEnabled() }
        .map_err(unavailable)?
        .as_bool()
    {
        return Err(InjectError::ReadOnly);
    }
    let value =
        unsafe { element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }
            .ok();
    let pattern =
        unsafe { element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) }.ok();
    if value.is_none() && pattern.is_none() {
        return Err(InjectError::Unavailable);
    }
    if let Some(value) = &value
        && unsafe { value.CurrentIsReadOnly() }
            .map_err(unavailable)?
            .as_bool()
    {
        return Err(InjectError::ReadOnly);
    }
    let selection = match &pattern {
        Some(pattern) => current_selection(&element, pattern)?,
        None => None,
    };
    if let Some(range) = &selection {
        let attribute =
            unsafe { range.GetAttributeValue(UIA_IsReadOnlyAttributeId) }.map_err(unavailable)?;
        if attribute.vt() == VT_BOOL {
            if bool::try_from(&attribute).map_err(unavailable)? {
                return Err(InjectError::ReadOnly);
            }
        } else if !unsafe { automation.CheckNotSupported(&attribute) }
            .map_err(unavailable)?
            .as_bool()
        {
            // A mixed range can include protected text.
            return Err(InjectError::ReadOnly);
        }
    }
    let check_focus = || {
        let current =
            unsafe { automation.GetFocusedElement() }.map_err(|_| InjectError::TargetChanged)?;
        if !unsafe { automation.CompareElements(&element, &current) }
            .map_err(|_| InjectError::TargetChanged)?
            .as_bool()
        {
            return Err(InjectError::TargetChanged);
        }
        target.check_foreground()
    };
    let hwnd = unsafe { element.CurrentNativeWindowHandle() }.map_err(unavailable)?;
    if let Some(kind) = native_edit(hwnd) {
        return replace_native(hwnd, kind, text, || {
            check_focus()?;
            let mut info = GUITHREADINFO {
                cbSize: size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };
            unsafe { GetGUIThreadInfo(0, &mut info) }.map_err(|_| InjectError::TargetChanged)?;
            if info.hwndFocus != hwnd {
                return Err(InjectError::TargetChanged);
            }
            Ok(())
        });
    }

    // TextPattern has no write method. ValuePattern only writes entire values:
    // use it only where that operation matches the requested insertion exactly.
    let value = value.ok_or(InjectError::Unavailable)?;
    if unsafe { element.CurrentControlType() }.map_err(unavailable)? != UIA_EditControlTypeId
        || unsafe { element.CurrentIsPassword() }
            .map_err(unavailable)?
            .as_bool()
    {
        return Err(InjectError::Unavailable);
    }
    check_focus()?;
    if !unsafe { value.CurrentValue() }
        .map_err(unavailable)?
        .is_empty()
    {
        let pattern = pattern.as_ref().ok_or(InjectError::Unavailable)?;
        let selection = current_selection(&element, pattern)?.ok_or(InjectError::Unavailable)?;
        let document = unsafe { pattern.DocumentRange() }.map_err(unavailable)?;
        for endpoint in [TextPatternRangeEndpoint_Start, TextPatternRangeEndpoint_End] {
            if unsafe { document.CompareEndpoints(endpoint, &selection, endpoint) }
                .map_err(unavailable)?
                != 0
            {
                return Err(InjectError::Unavailable);
            }
        }
    }
    check_focus()?;
    let text = BSTR::from(text);
    unsafe { value.SetValue(&text) }.map_err(|_| InjectError::DeliveryUncertain)?;
    if unsafe { value.CurrentValue() }.map_err(|_| InjectError::DeliveryUncertain)? != text {
        return Err(InjectError::DeliveryUncertain);
    }
    Ok(())
}

fn current_selection(
    element: &IUIAutomationElement,
    pattern: &IUIAutomationTextPattern,
) -> Result<Option<IUIAutomationTextRange>, InjectError> {
    if let Ok(ranges) = unsafe { pattern.GetSelection() } {
        match unsafe { ranges.Length() }.map_err(unavailable)? {
            1 => {
                return unsafe { ranges.GetElement(0) }
                    .map(Some)
                    .map_err(unavailable);
            }
            0 => {}
            _ => return Err(InjectError::Unavailable),
        }
    }
    if let Ok(pattern) =
        unsafe { element.GetCurrentPatternAs::<IUIAutomationTextPattern2>(UIA_TextPattern2Id) }
    {
        let mut active = BOOL::default();
        if let Ok(caret) = unsafe { pattern.GetCaretRange(&mut active) }
            && active.as_bool()
        {
            return Ok(Some(caret));
        }
    }
    Ok(None)
}

fn check_window(
    automation: &IUIAutomation,
    element: &IUIAutomationElement,
    target: &Target,
) -> Result<(), InjectError> {
    // Virtual controls (e.g. in a browser) inherit a native host further up the UIA tree.
    let walker = unsafe { automation.RawViewWalker() }.map_err(unavailable)?;
    let mut parent = element.clone();
    loop {
        let hwnd = unsafe { parent.CurrentNativeWindowHandle() }.map_err(unavailable)?;
        if !hwnd.0.is_null() {
            if unsafe { GetAncestor(hwnd, GA_ROOT) } != target.hwnd() {
                return Err(InjectError::TargetChanged);
            }
            return target.check_foreground();
        }
        parent = unsafe { walker.GetParentElement(&parent) }.map_err(unavailable)?;
    }
}

#[derive(Clone, Copy)]
enum NativeEdit {
    Plain,
    Rich,
}

fn native_edit(hwnd: HWND) -> Option<NativeEdit> {
    if hwnd.0.is_null() || !unsafe { IsWindowUnicode(hwnd) }.as_bool() {
        return None;
    }
    let mut buffer = [0u16; 64];
    let count = unsafe { GetClassNameW(hwnd, &mut buffer) } as usize;
    match String::from_utf16_lossy(&buffer[..count])
        .to_ascii_lowercase()
        .as_str()
    {
        "edit" => Some(NativeEdit::Plain),
        "richedit20w" | "richedit50w" | "richeditd2dpt" | "richeditd2d" => Some(NativeEdit::Rich),
        _ => None,
    }
}

fn replace_native(
    hwnd: HWND,
    kind: NativeEdit,
    text: &str,
    check_focus: impl FnOnce() -> Result<(), InjectError>,
) -> Result<(), InjectError> {
    // Across processes these system messages marshal their buffers. Within our process a
    // timed-out recipient on another thread could retain a pointer beyond its lifetime.
    let mut pid = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if thread_id == 0 || (pid == std::process::id() && thread_id != unsafe { GetCurrentThreadId() })
    {
        return Err(InjectError::Unavailable);
    }
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) };
    if style & ES_READONLY != 0 {
        return Err(InjectError::ReadOnly);
    }
    if text.contains('\0') || (style & ES_MULTILINE == 0 && text.contains(['\r', '\n'])) {
        return Err(InjectError::Unavailable);
    }
    let newline = match kind {
        NativeEdit::Plain => "\r\n",
        NativeEdit::Rich => "\r",
    };
    let normalized = text
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', newline);
    let wide: Vec<_> = normalized.encode_utf16().chain([0]).collect();
    let (start, _) = selection_offsets(hwnd).ok_or(InjectError::Unavailable)?;
    let expected_caret = start as usize + wide.len() - 1;
    check_focus()?;
    let delivered = unsafe {
        SendMessageTimeoutW(
            hwnd,
            EM_REPLACESEL,
            WPARAM(1),
            LPARAM(wide.as_ptr() as isize),
            SMTO_ABORTIFHUNG | SMTO_ERRORONEXIT,
            1000,
            None,
        )
    };
    if delivered.0 == 0
        || selection_offsets(hwnd) != Some((expected_caret as u32, expected_caret as u32))
    {
        return Err(InjectError::DeliveryUncertain);
    }
    Ok(())
}

fn selection_offsets(hwnd: HWND) -> Option<(u32, u32)> {
    // The packed return value truncates offsets above 65535; the out parameters are full-width.
    let mut start = 0u32;
    let mut end = 0u32;
    let delivered = unsafe {
        SendMessageTimeoutW(
            hwnd,
            EM_GETSEL,
            WPARAM(&mut start as *mut _ as usize),
            LPARAM(&mut end as *mut _ as isize),
            SMTO_ABORTIFHUNG | SMTO_ERRORONEXIT,
            1000,
            None,
        )
    };
    (delivered.0 != 0).then_some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::Controls::{EM_SETLIMITTEXT, EM_SETSEL, EM_UNDO};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, GetWindowTextW, SendMessageW, WINDOW_EX_STYLE, WINDOW_STYLE,
    };
    use windows::core::w;

    struct HiddenEdit(HWND);

    impl HiddenEdit {
        fn new(style: i32) -> Self {
            Self(unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    w!("EDIT"),
                    w!("Öl und Öl"),
                    WINDOW_STYLE(style as u32),
                    0,
                    0,
                    240,
                    100,
                    None,
                    None,
                    None,
                    None,
                )
                .unwrap()
            })
        }

        fn select(&self, start: usize, end: isize) {
            unsafe { SendMessageW(self.0, EM_SETSEL, Some(WPARAM(start)), Some(LPARAM(end))) };
        }

        fn content(&self) -> String {
            let mut buffer = [0u16; 80];
            let count = unsafe { GetWindowTextW(self.0, &mut buffer) };
            String::from_utf16(&buffer[..count as usize]).unwrap()
        }
    }

    impl Drop for HiddenEdit {
        fn drop(&mut self) {
            unsafe { DestroyWindow(self.0) }.unwrap();
        }
    }

    #[test]
    fn native_edit_inserts_at_the_caret_and_can_undo() {
        let edit = HiddenEdit::new(ES_MULTILINE);
        edit.select(3, 3);
        replace_native(edit.0, NativeEdit::Plain, "🦦\nß ", || Ok(())).unwrap();
        assert_eq!(edit.content(), "Öl 🦦\r\nß und Öl");
        unsafe { SendMessageW(edit.0, EM_UNDO, None, None) };
        assert_eq!(edit.content(), "Öl und Öl");
    }

    #[test]
    fn native_edit_replaces_only_the_real_selection() {
        let edit = HiddenEdit::new(0);
        edit.select(7, 9);
        replace_native(edit.0, NativeEdit::Plain, "Äpfel", || Ok(())).unwrap();
        assert_eq!(edit.content(), "Öl und Äpfel");
    }

    #[test]
    fn field_limit_does_not_report_a_partial_insert_as_success() {
        let edit = HiddenEdit::new(0);
        unsafe { SendMessageW(edit.0, EM_SETLIMITTEXT, Some(WPARAM(11)), None) };
        edit.select(9, 9);
        let result = replace_native(edit.0, NativeEdit::Plain, "123456", || Ok(()));
        assert!(matches!(result, Err(InjectError::DeliveryUncertain)));
        assert_eq!(edit.content(), "Öl und Öl12");
    }

    #[test]
    fn readonly_and_lost_focus_leave_the_control_untouched() {
        let readonly = HiddenEdit::new(ES_READONLY);
        assert!(matches!(
            replace_native(readonly.0, NativeEdit::Plain, "x", || Ok(())),
            Err(InjectError::ReadOnly)
        ));
        let edit = HiddenEdit::new(0);
        assert!(matches!(
            replace_native(edit.0, NativeEdit::Plain, "x", || Err(
                InjectError::TargetChanged
            )),
            Err(InjectError::TargetChanged)
        ));
        assert_eq!(edit.content(), "Öl und Öl");
        assert_eq!(readonly.content(), "Öl und Öl");
    }
}
