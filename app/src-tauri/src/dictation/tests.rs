use super::*;

#[path = "dictionary_tests.rs"]
mod dictionary;

#[test]
fn a_slow_comparison_neither_blocks_dictation_nor_queues_duplicate_windows() {
    use std::{sync::mpsc, thread, time::Duration};
    let mut requests = review::Requests::default();
    let (started, running) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let (dispatch, ui) = mpsc::channel();
    requests
        .start(move |ticket| {
            started.send(thread::current().id()).unwrap();
            gate.recv_timeout(Duration::from_secs(5)).unwrap();
            dispatch.send(ticket).unwrap();
        })
        .unwrap();
    assert_ne!(
        running.recv_timeout(Duration::from_secs(5)).unwrap(),
        thread::current().id()
    );
    assert!(!requests.start(|_| {}).unwrap());
    release.send(()).unwrap();
    let ticket = ui.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(!requests.start(|_| {}).unwrap());
    drop(ticket);
    assert!(requests.start(|_| {}).unwrap());
}

#[test]
fn continuous_overlay_failures_show_one_warning_outside_the_window_thread() {
    use std::{sync::mpsc, thread, time::Duration};
    let notice = std::sync::Arc::new(runtime::OverlayFailure::default());
    let (shown, received) = mpsc::channel();
    let (close, wait) = mpsc::channel();
    let opened = shown.clone();
    let dialog = notice
        .report(move || {
            opened.send(thread::current().id()).unwrap();
            wait.recv_timeout(Duration::from_secs(5)).unwrap();
        })
        .unwrap()
        .unwrap();
    let dialog_thread = received.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_ne!(dialog_thread, thread::current().id());
    assert!(
        notice
            .report(|| panic!("duplicate dialog"))
            .unwrap()
            .is_none()
    );
    close.send(()).unwrap();
    dialog.join().unwrap();
    notice.recovered();
    notice
        .report(move || {
            shown.send(thread::current().id()).unwrap();
        })
        .unwrap();
    received.recv_timeout(Duration::from_secs(5)).unwrap();
}

#[test]
fn mixer_faults_preserve_dictation_and_appear_when_its_status_is_settled() {
    let mut warning = Some("Mixer nicht erreichbar.".into());
    for state in [
        OverlayState::Listening { level: 42 },
        OverlayState::Processing,
        OverlayState::Inserting,
    ] {
        assert!(!matches!(
            with_warning(state, &mut warning, ""),
            OverlayState::Error { .. }
        ));
    }
    assert!(
        matches!(with_warning(OverlayState::Ready, &mut warning, ""),
        OverlayState::Error { message, .. } if message == "Mixer nicht erreichbar.")
    );
    assert!(warning.is_none());
    let failure = OverlayState::Error {
        message: "Einfügen fehlgeschlagen.".into(),
        text: "Mein Diktat".into(),
    };
    assert!(
        matches!(with_warning(failure, &mut Some("Mixerfehler.".into()), ""),
        OverlayState::Error { message, text } if message.contains("Mixerfehler.") && message.contains("Einfügen") && text == "Mein Diktat")
    );
}

#[derive(Default)]
struct Fake {
    installed: Option<&'static str>,
    calls: Vec<&'static str>,
    overlay: OverlayState,
    inserts: Vec<(u8, String)>,
    comparisons: Vec<(u8, Arc<DictationRecord>)>,
    carets: Vec<Option<usize>>,
    stopped_before_processing: bool,
    fail_begin: bool,
    fail_finish: bool,
}

impl Ports for Fake {
    type Target = u8;
    fn installed(&mut self) -> Result<Option<&'static str>, String> {
        Ok(self.installed)
    }
    fn models(&mut self) -> Result<(), String> {
        self.calls.push("models");
        Ok(())
    }
    fn begin(&mut self) -> Result<(), String> {
        self.calls.push("begin");
        if self.fail_begin {
            return Err("Mikrofon fehlt".into());
        }
        Ok(())
    }
    fn finish(&mut self) -> Result<Vec<f32>, String> {
        self.calls.push("finish");
        if self.fail_finish {
            return Err("Aufnahme unterbrochen".into());
        }
        Ok(vec![0.25])
    }
    fn suspend(&mut self) -> Result<(), String> {
        self.calls.push("suspend");
        Ok(())
    }
    fn recognize(&mut self, model: &'static str, samples: Vec<f32>) -> Result<(), String> {
        assert_eq!(Some(model), self.installed);
        assert_eq!(samples, [0.25]);
        self.calls.push("recognize");
        Ok(())
    }
    fn insert(&mut self, target: u8, text: &str, cursor: Option<usize>) -> Result<(), String> {
        self.carets.push(cursor);
        self.inserts.push((target, text.into()));
        Ok(())
    }
    fn compare(&mut self, target: u8, record: Arc<DictationRecord>) -> Result<(), String> {
        self.comparisons.push((target, record));
        Ok(())
    }
    fn show(&mut self, next: OverlayState) {
        if matches!(next, OverlayState::Processing) {
            self.stopped_before_processing = self.calls.last() == Some(&"suspend");
        }
        self.overlay = next;
    }
}

fn setup() -> Session<Fake> {
    let fake = Fake {
        installed: Some("test"),
        ..Fake::default()
    };
    Session::new(fake)
}

#[test]
fn every_completed_dictation_survives_in_memory_even_if_its_final_text_is_empty() {
    let mut s = setup();
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("review.db");
    s.compare(Some(2));
    assert!(s.ports.comparisons.is_empty());
    assert!(matches!(s.ports.overlay, OverlayState::Error { .. }));
    for raw in ["Ich möchte, äh, nach Hause.", "äh"] {
        s.start(Some(1));
        s.stop();
        s.recognized(Ok(Transcript::from_dictation(
            "test",
            raw.into(),
            Default::default(),
            || quatsch_mit_sosse_store::Store::open(&db),
        )));
        if matches!(s.phase, Phase::Inserting { .. }) {
            s.inserted(Err("Ziel geschlossen".into()));
        }
    }
    s.start(Some(1));
    s.stop();
    s.recognized(Ok(Transcript::plain(" ".into())));
    assert_eq!(s.history.len(), 2);
    assert!(
        matches!(&s.ports.overlay, OverlayState::Error { text, .. } if text == "Ich möchte nach Hause.")
    );
    assert_eq!(s.history[0].original, "Ich möchte, äh, nach Hause.");
    assert_eq!(s.history[0].finished, "Ich möchte nach Hause.");
    s.compare(None);
    assert!(s.ports.comparisons.is_empty());
    s.compare(Some(2));
    let (target, record) = &s.ports.comparisons[0];
    assert_eq!(*target, 2);
    assert_eq!(record.original, "äh");
    assert_eq!(record.finished, "");
    s.repeat(Some(3));
    assert_eq!(
        s.ports.inserts.last(),
        Some(&(3, "Ich möchte nach Hause.".into()))
    );
}

#[test]
fn restoring_a_displayed_original_uses_its_snapshot_and_no_expanded_snippet_caret() {
    let mut s = setup();
    s.start(Some(1));
    s.stop();
    let mut result = Transcript::plain("mein Gruß".into());
    result.record.finished = "Grüße!".into();
    result.record.cursor = Some(3);
    s.recognized(Ok(result));
    s.inserted(Ok(()));
    s.compare(Some(2));
    let (target, first) = s.ports.comparisons[0].clone();
    s.start(Some(3));
    s.compare(Some(3));
    s.stop();
    s.recognized(Ok(Transcript::plain("Zwei.".into())));
    s.inserted(Ok(()));
    s.insert_original(target, &first.original);
    s.inserted(Err("Zielfenster geschlossen".into()));
    assert!(matches!(&s.ports.overlay, OverlayState::Error { text, .. } if text == "mein Gruß"));
    s.repeat(Some(3));
    assert_eq!(s.ports.comparisons.len(), 1);
    assert_eq!(
        s.ports.inserts,
        [
            (1, "Grüße!".into()),
            (3, "Zwei.".into()),
            (2, "mein Gruß".into()),
            (3, "Zwei.".into())
        ]
    );
    assert_eq!(s.ports.carets, [Some(3), None, None, None]);
    assert_eq!(s.history.len(), 2);
}

#[test]
fn original_requested_during_another_insert_remains_available_to_copy() {
    let mut s = setup();
    s.start(Some(1));
    s.stop();
    s.recognized(Ok(Transcript::plain("Zwei.".into())));
    s.insert_original(2, "Früheres Original.");
    assert_eq!(s.ports.inserts, [(1, "Zwei.".into())]);
    assert!(
        matches!(&s.ports.overlay, OverlayState::Error { text, .. } if text == "Früheres Original.")
    );
    assert!(matches!(s.phase, Phase::Inserting { .. }));
}

#[test]
fn records_transcribes_then_keeps_the_exact_text_after_failed_insertion() {
    let mut s = setup();
    assert!(s.start(Some(1)));
    assert!(!s.start(Some(2)));
    s.level(62);
    assert!(matches!(
        s.ports.overlay,
        OverlayState::Listening { level: 62 }
    ));
    s.stop();
    assert!(s.ports.stopped_before_processing);
    assert!(matches!(s.ports.overlay, OverlayState::Processing));
    assert!(!s.start(Some(2)));
    s.stop();
    s.recognized(Ok(Transcript::plain("Grüße aus Köln.".into())));
    assert!(matches!(s.ports.overlay, OverlayState::Inserting));
    s.inserted(Err("Ziel gewechselt".into()));
    assert!(
        matches!(&s.ports.overlay, OverlayState::Error { text, .. } if text == "Grüße aus Köln.")
    );
    s.repeat(Some(3));
    s.inserted(Ok(()));
    assert!(matches!(s.ports.overlay, OverlayState::Ready));
    assert_eq!(
        s.ports.inserts,
        [(1, "Grüße aus Köln.".into()), (3, "Grüße aus Köln.".into())]
    );
    assert_eq!(s.ports.calls, ["begin", "finish", "suspend", "recognize"]);
}

#[test]
fn missing_models_open_on_demand_and_release_never_starts_a_recording() {
    let mut s = Session::new(Fake::default());
    assert!(s.ports.calls.is_empty());
    assert!(!s.start(Some(1)));
    s.ports.installed = Some("test");
    s.stop();
    assert_eq!(s.ports.calls, ["models"]);
    assert!(s.start(Some(1)));
    assert_eq!(s.ports.calls, ["models", "begin"]);
}

#[test]
fn microphone_opens_only_for_dictation_and_cancel_releases_it() {
    let mut s = setup();
    s.repeat(Some(1));
    assert!(s.ports.calls.is_empty());
    s.start(Some(1));
    s.cancel();
    s.stop();
    assert_eq!(s.ports.calls, ["begin", "suspend"]);
    assert!(s.ports.inserts.is_empty());
}

#[test]
fn both_start_and_finish_failures_release_the_microphone() {
    let mut s = setup();
    s.ports.fail_begin = true;
    assert!(!s.start(Some(1)));
    assert_eq!(s.ports.calls, ["begin", "suspend"]);
    s.ports.fail_begin = false;
    s.ports.fail_finish = true;
    s.start(Some(1));
    s.stop();
    assert_eq!(
        s.ports.calls,
        ["begin", "suspend", "begin", "finish", "suspend"]
    );
    assert!(matches!(s.ports.overlay, OverlayState::Error { .. }));
}

#[test]
fn silence_and_recognition_errors_preserve_the_previous_result() {
    let mut s = setup();
    s.repeat(Some(1));
    assert!(matches!(s.ports.overlay, OverlayState::Error { .. }));
    for result in [
        Ok("Eins.".into()),
        Ok(String::new()),
        Err("Erkennung fehlgeschlagen".into()),
    ] {
        s.start(Some(1));
        s.stop();
        s.recognized(result.map(Transcript::plain));
        if matches!(s.ports.overlay, OverlayState::Inserting) {
            s.inserted(Ok(()));
        }
    }
    assert!(matches!(&s.ports.overlay, OverlayState::Error { text, .. } if text == "Eins."));
    s.repeat(None);
    assert!(matches!(&s.ports.overlay, OverlayState::Error { text, .. } if text == "Eins."));
    assert_eq!(s.ports.inserts.len(), 1);
}

#[test]
fn elevated_window_during_inference_leaves_the_new_text_for_manual_recovery() {
    let mut s = setup();
    s.start(Some(1));
    s.stop();
    s.elevated();
    s.recognized(Ok(Transcript::plain("Zwei.".into())));
    assert!(s.ports.inserts.is_empty());
    assert!(matches!(&s.ports.overlay, OverlayState::Error { text, .. } if text == "Zwei."));
    s.target_available();
    assert!(matches!(&s.ports.overlay, OverlayState::Error { text, .. } if text == "Zwei."));
}

#[test]
fn losing_target_information_during_recognition_requires_an_explicit_retry() {
    let mut s = setup();
    s.start(Some(1));
    s.stop();
    s.discard_target();
    s.target_available();
    s.recognized(Ok(Transcript::plain("Zwei.".into())));
    assert!(s.ports.inserts.is_empty());
    assert!(matches!(&s.ports.overlay, OverlayState::Error { text, .. } if text == "Zwei."));
    s.repeat(Some(2));
    assert_eq!(s.ports.inserts, [(2, "Zwei.".into())]);
}

#[test]
fn a_stopped_shortcut_worker_releases_the_microphone_and_shows_the_error() {
    let mut s = setup();
    s.start(Some(1));
    s.unavailable(messages().shortcut_error.clone());
    assert_eq!(s.ports.calls, ["begin", "suspend"]);
    assert!(matches!(s.ports.overlay, OverlayState::Error { .. }));
    s.stop();
    assert!(s.ports.inserts.is_empty());
}

#[test]
fn leaving_an_administrator_window_clears_its_hint() {
    let mut s = setup();
    s.elevated();
    assert!(matches!(s.ports.overlay, OverlayState::Administrator));
    s.target_available();
    assert!(matches!(s.ports.overlay, OverlayState::Ready));
}
