use quatsch_mit_sosse_models::Manager;
use quatsch_mit_sosse_vad::{SAMPLE_RATE, SileroVad, VadConfig};

#[test]
#[ignore = "Benötigt installiertes silero-vad; Startanleitung im README."]
fn installed_silero_handles_silence_and_repeated_recordings() {
    let manager = Manager::new().expect("Modellverzeichnis nicht verfügbar");
    let model = manager
        .path_of("silero-vad")
        .unwrap()
        .expect("silero-vad zuerst installieren");
    let mut vad = SileroVad::new(model.join("silero_vad.onnx"), VadConfig::default()).unwrap();
    assert!(!vad.is_loaded());
    for length in [SAMPLE_RATE as usize, 321_123, 537] {
        assert!(!vad.has_speech(&vec![0.0; length], SAMPLE_RATE).unwrap());
        assert!(vad.is_loaded());
    }
    vad.unload();
    assert!(!vad.is_loaded());
    assert!(
        !vad.has_speech(&vec![0.0; SAMPLE_RATE as usize], SAMPLE_RATE)
            .unwrap()
    );
}
