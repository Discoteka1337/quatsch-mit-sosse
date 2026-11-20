use super::*;
use quatsch_mit_sosse_audio::{AudioError, DeviceInfo};
use quatsch_mit_sosse_stt::Transcript;
use std::time::Duration;

fn connected() -> AudioEvent {
    AudioEvent::DeviceChanged {
        device: DeviceInfo {
            name: "Test".into(),
        },
        fallback: false,
    }
}

#[test]
fn dictation_selects_the_benchmarked_german_model_then_the_measured_fallback() {
    for (installed, expected) in [
        (vec![DEFAULT_MODEL, FALLBACK_MODEL], Some(DEFAULT_MODEL)),
        (vec![DEFAULT_MODEL], Some(DEFAULT_MODEL)),
        (vec![FALLBACK_MODEL], Some(FALLBACK_MODEL)),
        (vec!["whisper-large-v3-turbo"], None),
        (vec![], None),
    ] {
        assert_eq!(select_asr(|id| Ok(installed.contains(&id))), Ok(expected));
    }
    assert_eq!(
        select_asr(|_| Err("Installationsstatus unlesbar".to_owned())),
        Err("Installationsstatus unlesbar".to_owned())
    );
}

#[test]
fn missing_installations_can_be_checked_offline_without_creating_a_directory() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("models");
    let manager = Manager::with_root(root.clone()).unwrap();
    assert_eq!(installed_model(&manager), Ok(None));
    assert!(!root.exists());
}

#[test]
fn cold_start_can_begin_with_the_first_device_event_and_without_waiting_for_preroll() {
    let mut health = Health::default();
    assert!(health.begin().is_err());
    health.event(connected());
    assert_eq!(health.begin(), Ok(()));
    assert!(health.listening());
    assert!(health.begin().is_err());
    assert!(health.event(AudioEvent::RealtimeUnavailable).is_none());
    assert_eq!(health.finish(), Ok(()));
    assert!(!health.listening());
}

#[test]
fn reporting_and_finishing_a_broken_take_preserve_the_first_fault_even_after_reconnect() {
    let mut health = Health::default();
    health.event(connected());
    health.begin().unwrap();
    let fault = health.event(AudioEvent::InputOverflow).unwrap();
    assert!(!health.listening());
    assert!(health.event(AudioEvent::InputDiscontinuity).is_none());
    health.event(connected());
    assert!(health.current_fault(&fault));
    assert_eq!(health.finish(), Err(fault.message.clone()));
    health.begin().unwrap();
    assert!(!health.current_fault(&fault));
    assert_eq!(health.finish(), Ok(()));
}

#[test]
fn queued_idle_errors_expire_on_recovery_and_shutdown() {
    let mut health = Health::default();
    let unavailable = || AudioEvent::DeviceUnavailable {
        error: AudioError::NoInputDevice,
    };
    let old = health.event(unavailable()).unwrap();
    assert!(health.current_fault(&old));
    health.event(connected());
    assert!(!health.current_fault(&old));
    let new = health.event(unavailable()).unwrap();
    assert_eq!(health.begin(), Err(new.message.clone()));
    health.event(AudioEvent::Stopped);
    assert!(!health.current_fault(&new));
    assert!(health.begin().is_err());
}

struct FakeEngine;

impl SpeechEngine for FakeEngine {
    fn transcribe(
        &mut self,
        samples: &[f32],
        rate: u32,
        hint: Option<&str>,
    ) -> quatsch_mit_sosse_stt::Result<Transcript> {
        assert_eq!(samples, [0.25, 0.5]);
        assert_eq!(rate, 16_000);
        assert_eq!(hint, Some("de"));
        Ok(Transcript {
            text: "Grüße".into(),
            processing_duration: Duration::ZERO,
        })
    }
}

#[test]
fn reuses_the_selected_speech_engine_and_passes_the_whole_recording() {
    let mut recognizer = Recognizer {
        engine: Some((DEFAULT_MODEL, Box::new(FakeEngine))),
    };
    for _ in 0..2 {
        assert_eq!(
            recognizer.transcribe(DEFAULT_MODEL, &[0.25, 0.5], |_| panic!("unexpected retry")),
            Ok("Grüße".into())
        );
    }
}

#[test]
fn primeline_load_errors_retry_the_same_audio_once_and_announce_v3_before_loading_it() {
    use std::cell::Cell;
    for error in [
        SttError::Recognition,
        SttError::InvalidPath,
        SttError::ModelNotInstalled,
    ] {
        let mut first = Some(error);
        let samples = [0.5, -0.25];
        let notified = Cell::new(false);
        let mut attempts = Vec::new();
        let result = transcribe_with_fallback(
            DEFAULT_MODEL,
            &samples,
            |id, audio| {
                attempts.push(id);
                assert!(std::ptr::eq(audio, samples.as_slice()));
                if id == DEFAULT_MODEL {
                    assert!(!notified.get());
                    return Err(first.take().unwrap());
                }
                assert!(notified.get());
                Ok("Grüße".into())
            },
            |id| Ok([FALLBACK_MODEL, VAD_MODEL].contains(&id)),
            |notice| {
                assert!(!notice.is_empty());
                assert!(!notified.replace(true));
            },
        );
        assert_eq!(result, Ok("Grüße".into()));
        assert_eq!(attempts, [DEFAULT_MODEL, FALLBACK_MODEL]);
    }
}

#[test]
fn shared_errors_and_failed_fallback_models_never_trigger_another_model() {
    for (model, error) in [
        (DEFAULT_MODEL, SttError::InvalidAudio),
        (DEFAULT_MODEL, SttError::Vad),
        (DEFAULT_MODEL, SttError::ModelBusy),
        (DEFAULT_MODEL, SttError::IdleWorker),
        (FALLBACK_MODEL, SttError::Recognition),
    ] {
        let expected = error.to_string();
        let mut error = Some(error);
        let result = transcribe_with_fallback(
            model,
            &[],
            |_, _| Err(error.take().unwrap()),
            |_| panic!("unexpected installation check"),
            |_| panic!("unexpected retry"),
        );
        assert_eq!(result, Err(expected));
    }
}

#[test]
fn missing_vad_or_fallback_and_ambiguous_missing_model_errors_preserve_the_primary_error() {
    for (installed, error) in [
        (vec![VAD_MODEL], SttError::Recognition),
        (vec![FALLBACK_MODEL], SttError::ModelNotInstalled),
        (
            vec![VAD_MODEL, DEFAULT_MODEL, FALLBACK_MODEL],
            SttError::ModelNotInstalled,
        ),
    ] {
        let expected = recognition_message(&error);
        let mut error = Some(error);
        let result = transcribe_with_fallback(
            DEFAULT_MODEL,
            &[],
            |_, _| Err(error.take().unwrap()),
            |id| Ok(installed.contains(&id)),
            |_| panic!("unexpected retry"),
        );
        assert_eq!(result, Err(expected));
    }
}

#[test]
fn a_failed_fallback_status_check_reports_both_errors() {
    let result = transcribe_with_fallback(
        DEFAULT_MODEL,
        &[],
        |_, _| Err(SttError::Recognition),
        |_| Err("Modellordner unzugänglich.".into()),
        |_| panic!("unexpected retry"),
    );
    let message = result.unwrap_err();
    assert!(message.contains(&SttError::Recognition.to_string()));
    assert!(message.contains("Modellordner unzugänglich."));
}

#[test]
fn a_failed_v3_retry_reports_both_models_and_the_fallback_reason() {
    let mut attempts = Vec::new();
    let result = transcribe_with_fallback(
        DEFAULT_MODEL,
        &[],
        |id, _| {
            attempts.push(id);
            Err(if id == DEFAULT_MODEL {
                SttError::Recognition
            } else {
                SttError::ModelBusy
            })
        },
        |_| Ok(true),
        |_| {},
    );
    let message = result.unwrap_err();
    assert_eq!(attempts, [DEFAULT_MODEL, FALLBACK_MODEL]);
    assert!(message.contains("primeLine"));
    assert!(message.contains("Parakeet v3"));
    assert!(message.contains(&SttError::ModelBusy.to_string()));
}
