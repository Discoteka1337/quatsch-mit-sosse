use std::path::PathBuf;

use super::*;

fn unloaded() -> SileroVad {
    SileroVad::new(PathBuf::from("not-installed.onnx"), VadConfig::default()).unwrap()
}

#[test]
fn construction_and_empty_audio_do_not_load_a_model() {
    let mut vad = unloaded();
    assert!(!vad.is_loaded());
    assert!(vad.speech_segments(&[], SAMPLE_RATE).unwrap().is_empty());
    assert!(!vad.has_speech(&[], SAMPLE_RATE).unwrap());
    assert!(!vad.is_loaded());
}

#[test]
fn invalid_input_is_rejected_before_model_loading() {
    let mut vad = unloaded();
    assert!(matches!(
        vad.has_speech(&[0.0], 48_000),
        Err(Error::SampleRate)
    ));
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.1, -1.1] {
        assert!(matches!(
            vad.has_speech(&[invalid], SAMPLE_RATE),
            Err(Error::InvalidSamples)
        ));
    }
    assert!(!vad.is_loaded());
}

#[test]
fn missing_model_is_reported_on_first_nonempty_recording() {
    let mut vad = unloaded();
    assert!(matches!(
        vad.has_speech(&[0.0; 512], SAMPLE_RATE),
        Err(Error::ModelUnavailable)
    ));
    assert!(!vad.is_loaded());
}

#[test]
fn configuration_rejects_native_unsafe_values() {
    for threads in [0, u32::MAX] {
        assert!(
            SileroVad::new(
                PathBuf::from("model.onnx"),
                VadConfig {
                    num_threads: threads,
                    ..VadConfig::default()
                }
            )
            .is_err()
        );
    }
    for threshold in [f32::NAN, -0.1, 0.0, 1.1] {
        assert!(
            SileroVad::new(
                PathBuf::from("model.onnx"),
                VadConfig {
                    threshold,
                    ..VadConfig::default()
                }
            )
            .is_err()
        );
    }
    for duration in [f32::NAN, f32::INFINITY, -1.0, 0.0, 11.0] {
        assert!(
            SileroVad::new(
                PathBuf::from("model.onnx"),
                VadConfig {
                    min_silence_duration: duration,
                    ..VadConfig::default()
                }
            )
            .is_err()
        );
        assert!(
            SileroVad::new(
                PathBuf::from("model.onnx"),
                VadConfig {
                    min_speech_duration: duration,
                    ..VadConfig::default()
                }
            )
            .is_err()
        );
    }
}

#[test]
fn nul_in_path_is_rejected_without_entering_native_code() {
    assert!(matches!(
        SileroVad::new(PathBuf::from("model\0.onnx"), VadConfig::default()),
        Err(Error::InvalidModelPath)
    ));
}

#[test]
fn segment_padding_is_clamped_and_merges_touching_segments() {
    let mut segments = Vec::new();
    append_segment(&mut segments, 50, 100, 1_000, 100).unwrap();
    append_segment(&mut segments, 300, 100, 1_000, 100).unwrap();
    append_segment(&mut segments, 900, 300, 1_000, 100).unwrap();
    merge_segments(&mut segments);
    assert_eq!(
        segments,
        vec![
            SpeechSegment { start: 0, end: 500 },
            SpeechSegment {
                start: 800,
                end: 1_000
            }
        ]
    );
}

#[test]
fn analysis_boundaries_do_not_become_speech_pauses() {
    let mut segments = Vec::new();
    append_segment(&mut segments, 0, 320_000, 700_000, 0).unwrap();
    append_segment(&mut segments, 320_000, 320_000, 700_000, 0).unwrap();
    append_segment(&mut segments, 640_000, 60_000, 700_000, 0).unwrap();
    merge_segments(&mut segments);
    assert_eq!(
        segments,
        vec![SpeechSegment {
            start: 0,
            end: 700_000
        }]
    );
}

#[test]
fn padded_tail_is_not_reported_as_recorded_audio() {
    let mut segments = Vec::new();
    append_segment(&mut segments, 1_100, 512, 1_000, 100).unwrap();
    assert!(segments.is_empty());
    assert!(append_segment(&mut segments, -1, 100, 1_000, 100).is_err());
    assert!(append_segment(&mut segments, 0, -1, 1_000, 100).is_err());
}

#[test]
fn analysis_windows_overlap_and_cover_every_sample() {
    let ranges = analysis_ranges(1_250_123, &VadConfig::default());
    assert_eq!(ranges.first().unwrap().start, 0);
    assert_eq!(ranges.last().unwrap().end, 1_250_123);
    assert!(ranges.iter().all(|range| range.len() <= ANALYSIS_SAMPLES));
    assert!(ranges.windows(2).all(|pair| pair[1].start < pair[0].end));
    assert!(analysis_ranges(0, &VadConfig::default()).is_empty());
}

#[test]
fn overlapping_analysis_results_are_merged_in_recording_order() {
    let mut segments = vec![
        SpeechSegment {
            start: 500,
            end: 800,
        },
        SpeechSegment {
            start: 100,
            end: 300,
        },
        SpeechSegment {
            start: 290,
            end: 600,
        },
        SpeechSegment {
            start: 1_000,
            end: 1_100,
        },
    ];
    merge_segments(&mut segments);
    assert_eq!(
        segments,
        vec![
            SpeechSegment {
                start: 100,
                end: 800
            },
            SpeechSegment {
                start: 1_000,
                end: 1_100
            }
        ]
    );
}
