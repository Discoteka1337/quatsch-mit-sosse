use super::*;
use std::cell::{Cell, RefCell};
use windows::Win32::Graphics::Gdi::{CreateBitmap, GetBitmapBits, HBITMAP};
use windows::Win32::System::Memory::GlobalSize;
use windows::Win32::System::Ole::CF_DIB;

#[test]
fn a_pending_temporary_paste_blocks_reads_for_snippet_placeholders() {
    let _pending = TRANSACTION.lock().unwrap();
    assert_eq!(clipboard_text(), Err(InjectError::Busy));
}

#[test]
fn snippet_clipboard_text_stops_at_the_first_nul_and_rejects_invalid_data() {
    assert_eq!(
        decode_clipboard_text(&[0x00df, 13, 10, 0xd83e, 0xdda6, 0, 65]).unwrap(),
        "ß\r\n🦦"
    );
    assert_eq!(decode_clipboard_text(&[0]).unwrap(), "");
    assert!(decode_clipboard_text(&[65]).is_err());
    assert!(decode_clipboard_text(&[0xd800, 0]).is_err());
}

// Only clipboard access is simulated. The real allocation, duplication, publishing
// order, transaction and cleanup code run against independent Win32 handles.
#[derive(Default)]
struct TestClipboard {
    entries: RefCell<Vec<OwnedFormat>>,
    ours: Cell<bool>,
    reject_next: Cell<Option<u32>>,
}

impl Clipboard for TestClipboard {
    type Lock = ();

    fn open(&self, _: bool) -> Result<(), InjectError> {
        Ok(())
    }
    fn owns_contents(&self) -> bool {
        self.ours.get()
    }
    fn offer_text(&self, text: OwnedFormat) -> Result<(), InjectError> {
        text.publish(self)
    }
    fn clear(&self) -> Result<(), InjectError> {
        self.entries.borrow_mut().clear();
        self.ours.set(true);
        Ok(())
    }
    fn formats(&self) -> Result<Vec<CLIPBOARD_FORMAT>, InjectError> {
        Ok(self
            .entries
            .borrow()
            .iter()
            .map(|entry| entry.format)
            .collect())
    }
    fn get(&self, format: CLIPBOARD_FORMAT) -> Result<HANDLE, InjectError> {
        self.entries
            .borrow()
            .iter()
            .find(|entry| entry.format == format)
            .map(|entry| entry.handle)
            .ok_or(InjectError::Unavailable)
    }
    fn set(&self, format: CLIPBOARD_FORMAT, handle: HANDLE) -> Result<(), InjectError> {
        if self.reject_next.get() == Some(u32::from(format.0)) {
            self.reject_next.set(None);
            return Err(InjectError::Unavailable);
        }
        self.entries
            .borrow_mut()
            .push(OwnedFormat { format, handle });
        Ok(())
    }
}

impl TestClipboard {
    fn install(&self, format: CLIPBOARD_FORMAT, bytes: &[u8]) {
        OwnedFormat::from_bytes(format, bytes)
            .unwrap()
            .publish(self)
            .unwrap();
    }

    fn read(&self, format: CLIPBOARD_FORMAT, len: usize) -> Vec<u8> {
        unsafe {
            let handle = HGLOBAL(self.get(format).unwrap().0);
            assert!(GlobalSize(handle) >= len);
            let data = GlobalLock(handle);
            assert!(!data.is_null());
            let bytes = std::slice::from_raw_parts(data.cast::<u8>(), len).to_vec();
            let _ = GlobalUnlock(handle);
            bytes
        }
    }
}

#[test]
fn a_manual_copy_prepares_private_formats_and_preserves_the_dictation() {
    let (flags, text) = protected_formats("Öl\n3,5 %").unwrap();
    let clip = TestClipboard::default();
    publish_immediately(flags, text, |format, handle| clip.set(format, handle)).unwrap();
    for name in PRIVACY_FORMATS {
        assert_eq!(clip.read(register(name).unwrap(), 4), [0; 4]);
    }
    let bytes = utf16_bytes("Öl\r\n3,5 %");
    assert_eq!(clip.read(CF_UNICODETEXT, bytes.len()), bytes);
    assert!(protected_formats("a\0b").is_err());
    assert!(protected_formats("").is_err());
}

#[test]
fn manual_copy_stops_before_publishing_any_text_if_a_privacy_format_is_rejected() {
    let clip = TestClipboard::default();
    let (flags, text) = protected_formats("Wiederhergestellter Text").unwrap();
    let second = register(PRIVACY_FORMATS[1]).unwrap();
    clip.reject_next.set(Some(u32::from(second.0)));
    let result = publish_immediately(flags, text, |format, handle| clip.set(format, handle));
    assert_eq!(result, Err(InjectError::Unavailable));
    assert_eq!(
        clip.formats().unwrap(),
        [register(PRIVACY_FORMATS[0]).unwrap()]
    );
}

#[test]
fn restores_text_html_and_images_after_using_protected_clipboard() {
    let clip = TestClipboard::default();
    let html = register(w!("HTML Format")).unwrap();
    let content = utf16_bytes("Grüße vor dem Einfügen");
    let markup = concat!(
        "Version:1.0\r\nStartHTML:0000000105\r\nEndHTML:0000000184\r\n",
        "StartFragment:0000000137\r\nEndFragment:0000000152\r\n",
        "<html><body><!--StartFragment--><b>Original</b><!--EndFragment--></body></html>\0"
    )
    .as_bytes();
    // BITMAPINFOHEADER and one blue BGR pixel, with DWORD row padding.
    let dib: [u8; 44] = [
        40, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 24, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 0, 0, 0,
    ];
    clip.install(html, markup);
    clip.install(CF_UNICODETEXT, &content);
    clip.install(CF_DIB, &dib);
    let pixels = [0x80u8, 0];
    let bitmap = unsafe { CreateBitmap(2, 1, 1, 1, Some(pixels.as_ptr().cast())) };
    assert!(!bitmap.is_invalid());
    OwnedFormat {
        format: CF_BITMAP,
        handle: HANDLE(bitmap.0),
    }
    .publish(&clip)
    .unwrap();
    let original = clip.formats().unwrap();

    temporary_text(
        &clip,
        "Ü\nß",
        || Ok(()),
        || {
            assert_eq!(
                clip.read(CF_UNICODETEXT, 10),
                [220, 0, 13, 0, 10, 0, 223, 0, 0, 0]
            );
            let mut expected_order = Vec::new();
            for name in PRIVACY_FORMATS {
                let format = register(name)?;
                assert_eq!(clip.read(format, 4), [0; 4]);
                expected_order.push(format);
            }
            expected_order.push(CF_UNICODETEXT);
            assert_eq!(clip.formats()?, expected_order);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(clip.formats().unwrap(), original);
    assert_eq!(clip.read(html, markup.len()), markup);
    assert_eq!(clip.read(CF_UNICODETEXT, content.len()), content);
    assert_eq!(clip.read(CF_DIB, dib.len()), dib);
    let mut restored = [0u8; 2];
    let image = HBITMAP(clip.get(CF_BITMAP).unwrap().0);
    assert_eq!(
        unsafe { GetBitmapBits(image, 2, restored.as_mut_ptr().cast()) },
        2
    );
    assert_eq!(restored[0] & 0xc0, pixels[0]);
}

#[test]
fn rolls_back_after_partial_publication_or_failed_shortcut() {
    let clip = TestClipboard::default();
    let bytes = utf16_bytes("Original");
    clip.install(CF_UNICODETEXT, &bytes);
    clip.reject_next
        .set(Some(u32::from(register(PRIVACY_FORMATS[1]).unwrap().0)));
    assert!(matches!(
        temporary_text(&clip, "Ersatz", || Ok(()), || panic!()),
        Err(InjectError::Unavailable)
    ));
    assert_eq!(clip.formats().unwrap(), [CF_UNICODETEXT]);
    assert_eq!(clip.read(CF_UNICODETEXT, bytes.len()), bytes);

    assert!(matches!(
        temporary_text(
            &clip,
            "Ersatz",
            || Ok(()),
            || Err(InjectError::DeliveryUncertain)
        ),
        Err(InjectError::DeliveryUncertain)
    ));
    assert_eq!(clip.formats().unwrap(), [CF_UNICODETEXT]);
    assert_eq!(clip.read(CF_UNICODETEXT, bytes.len()), bytes);
}

#[test]
fn preserves_a_new_copy_from_another_owner() {
    let clip = TestClipboard::default();
    let newer = utf16_bytes("Neue Kopie");
    assert!(matches!(
        temporary_text(
            &clip,
            "Ersatz",
            || Ok(()),
            || {
                clip.clear()?;
                clip.install(CF_UNICODETEXT, &newer);
                clip.ours.set(false);
                Ok(())
            }
        ),
        Err(InjectError::ClipboardChanged)
    ));
    assert_eq!(clip.read(CF_UNICODETEXT, newer.len()), newer);
}

#[test]
fn leaves_owner_dependent_formats_untouched() {
    for format in [CF_PRIVATEFIRST, register(w!("DataObject")).unwrap()] {
        let clip = TestClipboard::default();
        clip.install(format, b"owner\0");
        assert!(matches!(
            temporary_text(&clip, "Ersatz", || Ok(()), || panic!()),
            Err(InjectError::ClipboardUnsupported)
        ));
        assert_eq!(clip.read(format, 6), b"owner\0");
    }
}
