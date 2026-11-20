use super::*;
use quatsch_mit_sosse_hotkey::Hotkey;

#[test]
fn capturing_waits_for_release_and_supports_modifier_only_chords() {
    let mut capture = Chord::default();
    for key in [0x11, 0x10, 0x20, 0x20] {
        assert!(capture.key(key, true).is_none());
    }
    assert!(capture.key(0x20, false).is_none());
    assert!(capture.key(0x10, false).is_none());
    assert_eq!(
        capture.key(0x11, false),
        Some(Some("Strg+Umschalt+Leertaste".parse::<Hotkey>().unwrap()))
    );
    let mut capture = Chord::default();
    capture.key(0x11, true);
    capture.key(0x5b, true);
    capture.key(0x11, false);
    assert_eq!(
        capture.key(0x5b, false),
        Some(Some("Strg+Win".parse().unwrap()))
    );
}

#[test]
fn unsupported_and_multi_trigger_chords_are_rejected() {
    for keys in [vec![0x41, 0x42], vec![0xba]] {
        let mut capture = Chord::default();
        for key in &keys {
            capture.key(*key, true);
        }
        let mut final_result = None;
        for key in keys {
            final_result = capture.key(key, false);
        }
        assert_eq!(final_result, Some(None));
    }
}

#[test]
fn layout_keeps_columns_separate_at_minimum_width_and_high_dpi() {
    for dpi in [96, 144, 192] {
        let width = scale(550, dpi);
        let left = placement(DICTATION, width, dpi, 0);
        let right = placement(REINSERT, width, dpi, 0);
        assert!(left.x + left.width < right.x);
        assert!(right.x + right.width <= width);
        let last = placement(HIDE_FROM_CAPTURE, width, dpi, 0);
        assert!(last.y + last.height <= scale(CONTENT_HEIGHT, dpi));
        assert_eq!(placement(HIDE_FROM_CAPTURE, width, dpi, 30).y, last.y - 30);
    }
}
