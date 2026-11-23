use super::*;
use windows::Win32::{
    Graphics::Gdi::{GdiFlush, GetObjectType, HGDIOBJ, OBJ_FONT},
    System::Threading::{GR_GDIOBJECTS, GR_USEROBJECTS, GetCurrentProcess, GetGuiResources},
    UI::Shell::GetWindowSubclass,
};

#[test]
fn repeated_hidden_reviews_release_states_controls_and_gdi_fonts() {
    // Process-wide GDI/USER counters must exclude unrelated parallel UI tests.
    const CHILD: &str = "QMS_REVIEW_LIFETIME_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let test_thread = std::thread::current();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test_thread.name().unwrap(), "--nocapture"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("1 passed;"),
            "{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        print!("{stdout}");
        return;
    }
    // Warm up Windows class registration, RichEdit and GDI's process-local font cache.
    let mut baseline = None;
    for cycle in 0..68 {
        let hwnd = create_window(RECT {
            right: 584,
            bottom: 290,
            ..Default::default()
        })
        .unwrap();
        let original = "x".repeat(text::PAGE_UNITS + 1);
        let record = Arc::new(DictationRecord {
            finished: original.clone(),
            original,
            ..Default::default()
        });
        let dictation = Arc::downgrade(&record);
        let review = Rc::new(Review {
            target: Target::from_window(hwnd).unwrap(),
            view: PreparedComparison::new(record),
            page: Cell::new(0),
            insert: RefCell::new(None),
            error: Box::new(|_| {}),
            closing: Cell::new(false),
            font: Font::create(15, 400, 96).unwrap(),
            _editor: load_editor().unwrap(),
        });
        let lifetime = Rc::downgrade(&review);
        let font = HGDIOBJ(review.font.0 as _);
        contents(hwnd, &review).unwrap();
        review.attach(hwnd).unwrap();
        drop(review);
        unsafe {
            assert_eq!(lifetime.strong_count(), 1);
            assert_eq!(GetObjectType(font), OBJ_FONT.0 as u32);
            assert!(GetWindowSubclass(hwnd, Some(parent_proc), 1, None).as_bool());
            let controls = [
                BEFORE_LABEL,
                BEFORE,
                AFTER_LABEL,
                AFTER,
                INSERT,
                COPY,
                PREVIOUS,
                NEXT,
            ]
            .map(|id| {
                let child = GetDlgItem(Some(hwnd), id as i32).unwrap();
                if !matches!(id, BEFORE_LABEL | AFTER_LABEL) {
                    assert!(GetWindowSubclass(child, Some(child_proc), id, None).as_bool());
                }
                child
            });
            send(hwnd, WM_COMMAND, NEXT, 0);
            match cycle % 3 {
                0 => {
                    send(hwnd, WM_CLOSE, 0, 0);
                }
                1 => {
                    send(hwnd, CLOSE_ON_BLUR, 0, 0);
                }
                _ => DestroyWindow(hwnd).unwrap(),
            }
            assert!(!IsWindow(Some(hwnd)).as_bool());
            assert!(
                controls
                    .iter()
                    .all(|child| !IsWindow(Some(*child)).as_bool())
            );
            assert!(lifetime.upgrade().is_none());
            assert!(dictation.upgrade().is_none());
            let _ = GdiFlush();
            let resources = (
                GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS),
                GetGuiResources(GetCurrentProcess(), GR_USEROBJECTS),
            );
            if cycle == 3 {
                baseline = Some(resources);
            } else if let Some(baseline) = baseline {
                assert_eq!(resources, baseline, "resource counts after cycle {cycle}");
                if cycle == 67 {
                    println!("64 measured cycles, GDI/USER objects {baseline:?} -> {resources:?}");
                }
            }
        }
    }
}
