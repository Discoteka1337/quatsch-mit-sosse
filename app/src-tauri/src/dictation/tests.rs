use super::*;

#[derive(Default)]
struct Fake {
    installed: Option<&'static str>,
    calls: Vec<&'static str>,
    overlay: OverlayState,
    inserts: Vec<(u8, String)>,
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
    fn insert(&mut self, target: u8, text: &str) -> Result<(), String> {
        self.inserts.push((target, text.into()));
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
fn records_transcribes_then_keeps_the_exact_text_after_failed_insertion() {
    let mut s = setup();
    s.start(Some(1));
    s.level(62);
    assert!(matches!(
        s.ports.overlay,
        OverlayState::Listening { level: 62 }
    ));
    s.stop();
    assert!(s.ports.stopped_before_processing);
    assert!(matches!(s.ports.overlay, OverlayState::Processing));
    s.start(Some(2));
    s.stop();
    s.recognized(Ok("Grüße aus Köln.".into()));
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
    s.start(Some(1));
    s.ports.installed = Some("test");
    s.stop();
    assert_eq!(s.ports.calls, ["models"]);
    s.start(Some(1));
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
    s.start(Some(1));
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
        s.recognized(result);
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
    s.recognized(Ok("Zwei.".into()));
    assert!(s.ports.inserts.is_empty());
    assert!(matches!(&s.ports.overlay, OverlayState::Error { text, .. } if text == "Zwei."));
    s.target_available();
    assert!(matches!(&s.ports.overlay, OverlayState::Error { text, .. } if text == "Zwei."));
}

#[test]
fn leaving_an_administrator_window_clears_its_hint() {
    let mut s = setup();
    s.elevated();
    assert!(matches!(s.ports.overlay, OverlayState::Administrator));
    s.target_available();
    assert!(matches!(s.ports.overlay, OverlayState::Ready));
}
