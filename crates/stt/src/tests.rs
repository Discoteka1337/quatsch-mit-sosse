use std::time::Duration;

use quatsch_mit_sosse_vad::SpeechSegment;

use super::*;

fn empty_manager() -> quatsch_mit_sosse_models::Manager {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    quatsch_mit_sosse_models::Manager::with_root(
        std::env::temp_dir().join(format!("qms-stt-absent-{}-{unique}", std::process::id())),
    )
    .unwrap()
}

fn speech(start: usize, end: usize) -> SpeechSegment {
    SpeechSegment { start, end }
}

#[test]
fn no_speech_produces_no_chunks() {
    assert!(split_at_pauses(&[], 1000, 10).unwrap().is_empty());
}

#[test]
fn recognition_keeps_extra_onset_context_only_before_the_first_speech_segment() {
    let chunks =
        chunks::recognition_chunks(&[speech(8_000, 16_000), speech(400_000, 408_000)], 408_000)
            .unwrap();
    assert_eq!(chunks, vec![1_600..16_000, 400_000..408_000]);
    assert!(chunks::recognition_chunks(&[], 16_000).unwrap().is_empty());
}

#[test]
fn onset_context_stops_at_the_recording_start_and_never_forces_an_extra_split() {
    let near_start = chunks::recognition_chunks(&[speech(2_000, 10_000)], 10_000).unwrap();
    assert_eq!(near_start, vec![0..10_000]);
    let near_limit = chunks::recognition_chunks(&[speech(2_000, 321_000)], 321_000).unwrap();
    assert_eq!(near_limit, vec![1_000..321_000]);
    let full = chunks::recognition_chunks(&[speech(2_000, 322_000)], 322_000).unwrap();
    assert_eq!(full, vec![2_000..322_000]);
}

#[test]
fn short_speech_keeps_internal_pauses_and_trims_outer_silence() {
    assert_eq!(
        split_at_pauses(&[speech(10, 70), speech(80, 130)], 150, 10).unwrap(),
        vec![10..130]
    );
}

#[test]
fn last_pause_before_twenty_seconds_is_preferred_over_cutting_speech() {
    assert_eq!(
        split_at_pauses(
            &[speech(0, 90), speech(100, 170), speech(180, 290)],
            300,
            10
        )
        .unwrap(),
        vec![0..170, 180..290]
    );
}

#[test]
fn continuous_speech_is_split_without_loss_or_duplication() {
    assert_eq!(
        split_at_pauses(&[speech(7, 612)], 620, 10).unwrap(),
        vec![7..207, 207..407, 407..607, 607..612]
    );
}

#[test]
fn touching_intervals_are_not_treated_as_a_pause() {
    assert_eq!(
        split_at_pauses(&[speech(0, 90), speech(90, 250)], 250, 10).unwrap(),
        vec![0..200, 200..250]
    );
}

#[test]
fn exact_limit_does_not_create_empty_tail() {
    assert_eq!(
        split_at_pauses(&[speech(0, 200)], 200, 10).unwrap(),
        vec![0..200]
    );
}

#[test]
fn long_pause_is_not_sent_to_the_recognizer() {
    assert_eq!(
        split_at_pauses(&[speech(10, 30), speech(900, 930)], 1000, 10).unwrap(),
        vec![10..30, 900..930]
    );
}

#[test]
fn invalid_segments_and_rates_are_rejected() {
    for segments in [
        vec![speech(3, 3)],
        vec![speech(5, 4)],
        vec![speech(0, 101)],
        vec![speech(5, 10), speech(9, 15)],
        vec![speech(20, 30), speech(0, 10)],
    ] {
        assert!(split_at_pauses(&segments, 100, 10).is_err());
    }
    assert!(split_at_pauses(&[], 0, 0).is_err());
}

#[test]
fn splitting_preserves_every_speech_sample_across_many_boundaries() {
    for length in 1..501 {
        let spans = [speech(3, 3 + length), speech(10 + length, 510 + length)];
        let chunks = split_at_pauses(&spans, 520 + length, 10).unwrap();
        assert!(chunks.iter().all(|c| !c.is_empty() && c.len() <= 200));
        assert!(chunks.windows(2).all(|w| w[0].end <= w[1].start));
        for span in &spans {
            for i in span.start..span.end {
                assert_eq!(chunks.iter().filter(|c| c.contains(&i)).count(), 1);
            }
        }
    }
}

#[test]
fn defaults_and_invalid_configuration() {
    assert_eq!(
        EngineConfig::default().idle_timeout,
        Duration::from_secs(300)
    );
    for num_threads in [0, u32::MAX] {
        assert!(
            EngineConfig {
                num_threads,
                ..EngineConfig::default()
            }
            .validate()
            .is_err()
        );
    }
}

#[test]
fn constructors_and_empty_dictation_need_no_installed_models() {
    let mut engine = TransducerEngine::with_manager(
        empty_manager(),
        "parakeet-v3-int8",
        EngineConfig::default(),
    )
    .unwrap();
    assert!(!engine.is_loaded());
    assert!(
        engine
            .transcribe(&[], 16_000, Some("de"))
            .unwrap()
            .text
            .is_empty()
    );
    assert!(!engine.is_loaded());
    let whisper = WhisperEngine::with_manager(
        empty_manager(),
        "whisper-large-v3-turbo",
        EngineConfig::default(),
    )
    .unwrap();
    assert!(!whisper.is_loaded());
}

#[test]
fn mismatched_models_and_invalid_audio_fail_before_loading() {
    assert!(
        TransducerEngine::with_manager(empty_manager(), "silero-vad", EngineConfig::default())
            .is_err()
    );
    assert!(
        TransducerEngine::with_manager(
            empty_manager(),
            "whisper-large-v3-turbo",
            EngineConfig::default()
        )
        .is_err()
    );
    assert!(
        WhisperEngine::with_manager(empty_manager(), "parakeet-v3-int8", EngineConfig::default())
            .is_err()
    );
    let mut engine = TransducerEngine::with_manager(
        empty_manager(),
        "parakeet-v3-int8",
        EngineConfig::default(),
    )
    .unwrap();
    assert!(engine.transcribe(&[0.0], 44_100, None).is_err());
    assert!(engine.transcribe(&[f32::NAN], 16_000, None).is_err());
    assert!(engine.transcribe(&[f32::INFINITY], 16_000, None).is_err());
    assert!(engine.transcribe(&[0.0], 16_000, Some("de\0")).is_err());
    assert!(!engine.is_loaded());
}

#[test]
fn factory_supports_all_manifest_asr_models_without_loading_and_reports_missing_models() {
    for id in [
        "parakeet-v3-int8",
        "parakeet-primeline-de-int8",
        "whisper-large-v3-turbo",
    ] {
        let mut engine = create_engine(empty_manager(), id, EngineConfig::default()).unwrap();
        assert!(
            engine
                .transcribe(&[], 16_000, None)
                .unwrap()
                .text
                .is_empty()
        );
        assert!(matches!(
            engine.transcribe(&[0.0; 512], 16_000, Some("de")),
            Err(Error::ModelNotInstalled)
        ));
    }
    assert!(create_engine(empty_manager(), "silero-vad", EngineConfig::default()).is_err());
}

#[test]
fn recognition_is_sequential_and_stops_on_error() {
    let samples = [0.0, 1.0, 2.0, 3.0, 4.0];
    let mut calls = Vec::new();
    let text = recognize_chunks(&samples, &[0..2, 3..5], |chunk| {
        calls.push(chunk.to_vec());
        Ok(if calls.len() == 1 {
            " Hallo "
        } else {
            " Welt! "
        }
        .into())
    })
    .unwrap();
    assert_eq!(text, "Hallo Welt!");
    assert_eq!(calls, vec![vec![0.0, 1.0], vec![3.0, 4.0]]);
    let mut count = 0;
    assert!(
        recognize_chunks(&samples, &[0..2, 2..3, 3..5], |_| {
            count += 1;
            Err(Error::Recognition)
        })
        .is_err()
    );
    assert_eq!(count, 1);
}

fn join_recognition(parts: &[&str]) -> String {
    let samples = vec![0.0; parts.len()];
    let chunks = (0..parts.len()).map(|i| i..i + 1).collect::<Vec<_>>();
    let mut parts = parts.iter();
    recognize_chunks(&samples, &chunks, |_| Ok(parts.next().unwrap().to_string())).unwrap()
}

#[test]
fn no_vad_chunks_never_call_the_recognizer() {
    let text = recognize_chunks(&[0.1; 512], &[], |_| panic!("unexpected ASR call")).unwrap();
    assert!(text.is_empty());
}

#[test]
fn removes_only_complete_phantoms_after_joining_all_chunks() {
    for parts in [
        vec!["Vielen Dank", "fu\u{308}rs Zuschauen!"],
        vec!["„VIELEN  DANK\tFÜRS ZUSCHAUEN!“"],
        vec!["", "Untertitel der Amara.org-Community.", ""],
        vec!["Untertitel im Auftrag des ZDF für funk"],
    ] {
        assert!(join_recognition(&parts).is_empty());
    }
}

#[test]
fn preserves_extra_words_symbols_and_original_spelling_around_phantoms() {
    for parts in [
        vec!["Vielen Dank fürs Zuschauen!", "Bis morgen."],
        vec!["Bitte schreibe:", "Vielen Dank fürs Zuschauen!"],
        vec!["Vielen Dank fürs Zuschauen und Zuhören."],
        vec!["Vielen Dank fürs Zuschauen +"],
        vec!["VIELEN   DANK für die Hilfe!"],
        vec!["Untertitel"],
    ] {
        assert_eq!(join_recognition(&parts), parts.join(" "));
    }
}
