use super::*;

/// Keyboard and clipboard delivery can finish before a control processes its input.
/// Move the caret only when that same control confirms the exact inserted range.
pub(crate) fn with_caret(
    target: &Target,
    text: &str,
    cursor: Option<usize>,
    deliver: impl FnOnce() -> Result<bool, InjectError>,
) -> Result<(), InjectError> {
    let Some(prefix) = cursor
        .and_then(|i| text.get(..i))
        .filter(|p| p.len() < text.len())
    else {
        return deliver().map(|_| ());
    };
    if unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() }.is_err() {
        return deliver().map(|_| ());
    }
    let result = {
        let field = TextField::capture(target);
        let result = deliver();
        if result == Ok(true)
            && let Ok(field) = field
        {
            let _ = field.select_after_delivery(target, text, prefix);
        }
        result.map(|_| ())
    };
    unsafe { CoUninitialize() };
    result
}

struct TextField {
    automation: IUIAutomation,
    element: IUIAutomationElement,
    pattern: IUIAutomationTextPattern,
    insertion_start: IUIAutomationTextRange,
}

impl TextField {
    fn capture(target: &Target) -> Result<Self, InjectError> {
        target.check_foreground()?;
        let automation: IUIAutomation =
            unsafe { CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER) }
                .map_err(unavailable)?;
        let element = unsafe { automation.GetFocusedElement() }.map_err(unavailable)?;
        check_window(&automation, &element, target)?;
        if unsafe { element.CurrentIsPassword() }
            .map_err(unavailable)?
            .as_bool()
        {
            return Err(InjectError::Unavailable);
        }
        let pattern =
            unsafe { element.GetCurrentPatternAs(UIA_TextPatternId) }.map_err(unavailable)?;
        let insertion_start =
            current_selection(&element, &pattern)?.ok_or(InjectError::Unavailable)?;
        unsafe {
            insertion_start.MoveEndpointByRange(
                TextPatternRangeEndpoint_End,
                &insertion_start,
                TextPatternRangeEndpoint_Start,
            )
        }
        .map_err(unavailable)?;
        Ok(Self {
            automation,
            element,
            pattern,
            insertion_start,
        })
    }

    fn select_after_delivery(
        &self,
        target: &Target,
        text: &str,
        prefix: &str,
    ) -> Result<(), InjectError> {
        let check_focus = || verify_focus(&self.automation, &self.element, target);
        check_focus()?;
        let caret =
            current_selection(&self.element, &self.pattern)?.ok_or(InjectError::Unavailable)?;
        if unsafe {
            caret.CompareEndpoints(
                TextPatternRangeEndpoint_Start,
                &caret,
                TextPatternRangeEndpoint_End,
            )
        }
        .map_err(unavailable)?
            != 0
        {
            return Err(InjectError::Unavailable);
        }
        // A provider may invalidate or move our saved start while editing. Only
        // change the selection if the range between both anchors matches delivery.
        unsafe {
            caret.MoveEndpointByRange(
                TextPatternRangeEndpoint_Start,
                &self.insertion_start,
                TextPatternRangeEndpoint_Start,
            )
        }
        .map_err(unavailable)?;
        let limit =
            i32::try_from(text.encode_utf16().count() + 1).map_err(|_| InjectError::Unavailable)?;
        if unsafe { caret.GetText(limit) }.map_err(unavailable)? != text {
            return Err(InjectError::Unavailable);
        }
        select_prefix(caret, prefix, check_focus)
    }
}
