use super::*;

#[test]
fn native_dialog_waits_for_a_click_and_displays_the_reported_progress() {
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use windows::Win32::UI::Controls::{PBM_GETPOS, TDM_CLICK_BUTTON};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumChildWindows, GetClassNameW, GetWindowTextW,
    };
    use windows::core::BOOL;

    #[derive(Default)]
    struct Controls {
        download: bool,
        progress: Option<usize>,
    }
    unsafe extern "system" fn inspect_child(hwnd: HWND, data: LPARAM) -> BOOL {
        let controls = unsafe { &mut *(data.0 as *mut Controls) };
        let mut buffer = [0u16; 128];
        let len = unsafe { GetClassNameW(hwnd, &mut buffer) } as usize;
        let class = String::from_utf16_lossy(&buffer[..len]);
        if class.eq_ignore_ascii_case("Button") {
            let len = unsafe { GetWindowTextW(hwnd, &mut buffer) } as usize;
            controls.download |= String::from_utf16_lossy(&buffer[..len]) == LABELS.model_download;
        } else if class.eq_ignore_ascii_case("msctls_progress32") {
            controls.progress =
                Some(unsafe { SendMessageW(hwnd, PBM_GETPOS, None, None) }.0 as usize);
        }
        true.into()
    }
    fn controls(hwnd: HWND) -> Controls {
        let mut result = Controls::default();
        unsafe {
            let _ = EnumChildWindows(
                Some(hwnd),
                Some(inspect_child),
                LPARAM((&mut result as *mut Controls) as isize),
            );
        }
        result
    }

    let shared = Arc::new(Shared::default());
    shared.set(View::prompt("Testmodell".into()));
    let panel = Arc::clone(&shared);
    let dialog = thread::spawn(move || {
        native_dialog(&panel, "Testlizenz", &|| {
            if panel.begin() {
                panel.set(View {
                    stage: Stage::Installing,
                    text: "Testmodell · 75 %".into(),
                    percent: 75,
                });
            }
        })
    });
    let timeout = Instant::now() + Duration::from_secs(10);
    let hwnd = loop {
        let handle = shared.hwnd.load(Ordering::Acquire);
        if handle != 0 {
            break HWND(handle as _);
        }
        assert!(!dialog.is_finished(), "Native dialog failed to initialize");
        assert!(Instant::now() < timeout, "Native dialog creation timed out");
        thread::sleep(Duration::from_millis(10));
    };
    let before = controls(hwnd);
    let was_idle = shared.snapshot().can_install();
    send(hwnd, TDM_CLICK_BUTTON, DOWNLOAD as usize, 0);
    let timeout = Instant::now() + Duration::from_secs(5);
    let after = loop {
        let state = controls(hwnd);
        if state.progress == Some(75) || Instant::now() >= timeout {
            break state;
        }
        thread::sleep(Duration::from_millis(10));
    };
    send(hwnd, TDM_CLICK_BUTTON, IDCANCEL.0 as usize, 0);
    dialog.join().unwrap().unwrap();
    assert!(was_idle);
    assert!(before.download);
    assert_eq!(before.progress, Some(0));
    assert_eq!(after.progress, Some(75));
    assert!(shared.cancel.load(Ordering::Acquire));
}

#[test]
fn credits_include_the_selected_asr_and_silero_from_the_manifest() {
    let root = tempfile::tempdir().unwrap();
    let manager = Models::with_root(root.path().into()).unwrap();
    for asr in [DEFAULT_MODEL, crate::speech::FALLBACK_MODEL] {
        let notes = credits(&manager, asr);
        for model in manager.list() {
            let selected = model.id == asr || model.id == VAD_MODEL;
            assert_eq!(notes.contains(&model.attribution), selected);
            if selected {
                assert!(notes.contains(&model.license));
                assert!(notes.contains(&model.display_name));
            }
        }
    }
}

#[test]
fn only_missing_components_are_offered_for_download() {
    use crate::speech::FALLBACK_MODEL;
    assert_eq!(
        required_ids(None, |_| Ok(false)).unwrap(),
        [DEFAULT_MODEL, VAD_MODEL]
    );
    assert_eq!(
        required_ids(None, |id| Ok(id == VAD_MODEL)).unwrap(),
        [DEFAULT_MODEL]
    );
    for asr in [DEFAULT_MODEL, FALLBACK_MODEL] {
        assert_eq!(required_ids(None, |id| Ok(id == asr)).unwrap(), [VAD_MODEL]);
        assert!(
            required_ids(None, |id| Ok(id == asr || id == VAD_MODEL))
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn an_explicit_selection_offers_that_model_even_when_an_alternative_exists() {
    use crate::speech::FALLBACK_MODEL;
    let installed = |id: &str| Ok(id == DEFAULT_MODEL || id == VAD_MODEL);
    assert_eq!(
        required_ids(Some(FALLBACK_MODEL), installed).unwrap(),
        [FALLBACK_MODEL]
    );
    assert!(required_ids(None, installed).unwrap().is_empty());
}

#[test]
fn opening_an_empty_store_lists_the_required_models_without_installing() {
    let root = tempfile::tempdir().unwrap();
    let manager = Models::with_root(root.path().join("models")).unwrap();
    let prompt = inspect(&manager, None).unwrap();
    assert!(prompt.can_install());
    assert!(prompt.text.contains("primeLine"));
    assert!(prompt.text.contains("Silero"));
    assert!(prompt.text.contains("MB"));
    assert!(!root.path().join("models").exists());
}

#[test]
fn clicks_start_one_job_and_allow_retry_after_failure() {
    let shared = Shared::default();
    shared.set(View::prompt("Modelle fehlen".into()));
    assert!(shared.begin());
    assert!(!shared.begin());
    shared.set(View::failure("Download unterbrochen".into()));
    assert!(shared.begin());
    shared.set(View::ready());
    assert!(!shared.begin());
}

#[test]
fn progress_shows_current_model_phase_and_combined_download_size() {
    let mut report = Progress {
        model_id: VAD_MODEL.into(),
        downloaded_bytes: 50,
        total_bytes: 100,
        phase: ProgressPhase::Downloading,
    };
    let progress = progress_view("Silero", &report, 100, 200);
    assert_eq!(progress.percent, 75);
    assert!(progress.text.contains("Silero"));
    assert!(progress.text.contains("75 %"));
    report.phase = ProgressPhase::Verifying;
    assert!(
        progress_view("Silero", &report, 100, 200)
            .text
            .contains("SHA256")
    );
    report.phase = ProgressPhase::Extracting;
    assert!(
        progress_view("Silero", &report, 100, 200)
            .text
            .contains("Install")
    );
    assert!(!progress.can_install());
}

#[test]
fn both_catalogs_have_all_model_manager_labels() {
    for source in [
        include_str!("../../../ui/src/locales/de.json"),
        include_str!("../../../ui/src/locales/en.json"),
    ] {
        let labels: Labels = serde_json::from_str(source).unwrap();
        assert!(labels.model_missing.contains("{models}"));
        assert!(labels.model_missing.contains("{size}"));
        assert!(!labels.model_ready.is_empty());
        assert!(!labels.model_download.is_empty());
    }
}
