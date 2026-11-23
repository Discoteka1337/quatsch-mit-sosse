use super::*;
use crate::comparison::{Segment, SegmentKind};
use std::{mem::size_of, ptr, slice};
use windows::Win32::{
    Foundation::COLORREF,
    UI::Controls::RichEdit::{
        CFE_AUTOBACKCOLOR, CFE_AUTOCOLOR, CFE_BOLD, CFE_STRIKEOUT, CFE_UNDERLINE, CFM_BACKCOLOR,
        CFM_BOLD, CFM_COLOR, CFM_STRIKEOUT, CFM_UNDERLINE, CHARFORMAT2W, CHARFORMATW, CHARRANGE,
        EDITSTREAM, EM_EXLIMITTEXT, EM_EXSETSEL, EM_SETCHARFORMAT, EM_STREAMIN, SCF_SELECTION,
        SF_TEXT, SF_UNICODE,
    },
};

// EM_STREAMIN calls this synchronously until the plain UTF-16 buffer is exhausted.
unsafe extern "system" fn read_text(
    cookie: usize,
    dest: *mut u8,
    size: i32,
    read: *mut i32,
) -> u32 {
    unsafe {
        let remaining = &mut *(cookie as *mut &[u8]);
        let count = remaining.len().min(size as usize);
        ptr::copy_nonoverlapping(remaining.as_ptr(), dest, count);
        *remaining = &remaining[count..];
        *read = count as i32;
    }
    0
}

// Bound both the RichEdit layout work and the number of native format operations per page.
pub(super) const PAGE_UNITS: usize = 8192;
pub(super) const PAGE_RUNS: usize = 96;

#[derive(Default)]
pub(super) struct Page {
    pub(super) text: Vec<u16>,
    pub(super) runs: Vec<(usize, usize, SegmentKind)>,
}

pub(super) fn pages(segments: &[Segment<'_>]) -> Vec<Page> {
    let mut pages = Vec::new();
    let mut page = Page::default();
    let mut after_cr = false;
    for segment in segments {
        for ch in segment.text.chars() {
            // RichEdit counts CRLF as one paragraph mark, including at a run boundary.
            if ch == '\n' && after_cr {
                after_cr = false;
                continue;
            }
            after_cr = ch == '\r';
            let ch = if ch == '\n' { '\r' } else { ch };
            let start = page.text.len();
            let new_run = segment.kind != SegmentKind::Unchanged
                && !page
                    .runs
                    .last()
                    .is_some_and(|&(_, end, kind)| end == start && kind == segment.kind);
            if start + ch.len_utf16() > PAGE_UNITS || (new_run && page.runs.len() == PAGE_RUNS) {
                pages.push(std::mem::take(&mut page));
            }
            let start = page.text.len();
            page.text.extend_from_slice(ch.encode_utf16(&mut [0; 2]));
            if segment.kind != SegmentKind::Unchanged {
                if let Some((_, end, _)) = page
                    .runs
                    .last_mut()
                    .filter(|(_, end, kind)| *end == start && *kind == segment.kind)
                {
                    *end = page.text.len();
                } else {
                    page.runs.push((start, page.text.len(), segment.kind));
                }
            }
        }
    }
    if !page.text.is_empty() || pages.is_empty() {
        pages.push(page);
    }
    pages
}

pub(super) fn render_page(pane: HWND, page: &Page) -> Result<()> {
    let text = &page.text;
    let length = text.len() as i32;
    send(pane, EM_EXLIMITTEXT, 0, length.max(1) as isize);
    let mut bytes = unsafe { slice::from_raw_parts(text.as_ptr().cast::<u8>(), text.len() * 2) };
    let mut stream = EDITSTREAM {
        dwCookie: (&raw mut bytes) as usize,
        pfnCallback: Some(read_text),
        ..Default::default()
    };
    send(
        pane,
        EM_STREAMIN,
        (SF_TEXT | SF_UNICODE) as usize,
        (&raw mut stream) as isize,
    );
    if stream.dwError != 0 || !bytes.is_empty() {
        return Err(Error::from(E_FAIL));
    }
    // Streaming plain text into a previously formatted control can inherit its first run.
    for (start, end, kind) in std::iter::once((0, text.len(), SegmentKind::Unchanged))
        .chain(page.runs.iter().copied())
        .filter(|(start, end, _)| start < end)
    {
        let (color, effect) = match kind {
            SegmentKind::Removed => (0xDFE0FF, CFE_STRIKEOUT),
            SegmentKind::Added => (0xDEF1D5, CFE_UNDERLINE),
            SegmentKind::Uncertain => (0x6DE3FF, CFE_BOLD),
            SegmentKind::Unchanged => (0, CFE_AUTOBACKCOLOR | CFE_AUTOCOLOR),
        };
        let range = CHARRANGE {
            cpMin: start as i32,
            cpMax: end as i32,
        };
        send(pane, EM_EXSETSEL, 0, (&raw const range) as isize);
        let format = CHARFORMAT2W {
            Base: CHARFORMATW {
                cbSize: size_of::<CHARFORMAT2W>() as u32,
                dwMask: CFM_BACKCOLOR | CFM_COLOR | CFM_STRIKEOUT | CFM_UNDERLINE | CFM_BOLD,
                dwEffects: effect,
                crTextColor: COLORREF(0x252218),
                ..Default::default()
            },
            crBackColor: COLORREF(color),
            ..Default::default()
        };
        if send(
            pane,
            EM_SETCHARFORMAT,
            SCF_SELECTION as usize,
            (&raw const format) as isize,
        )
        .0 == 0
        {
            return Err(Error::from(E_FAIL));
        }
    }
    let top = CHARRANGE::default();
    send(pane, EM_EXSETSEL, 0, (&raw const top) as isize);
    Ok(())
}
