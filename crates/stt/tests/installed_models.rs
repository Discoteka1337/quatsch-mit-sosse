use std::{
    fs::{self, File},
    io::BufReader,
    path::PathBuf,
};

use quatsch_mit_sosse_models::Manager;
use quatsch_mit_sosse_stt::{EngineConfig, create_engine};

#[test]
#[ignore = "Benötigt lokal installiertes silero-vad."]
fn silence_skips_recognition_without_an_asr_model() {
    let installed = Manager::new()
        .unwrap()
        .path_of("silero-vad")
        .unwrap()
        .expect("silero-vad muss installiert sein.");
    let temporary = tempfile::tempdir().unwrap();
    let vad_directory = temporary.path().join("silero-vad");
    fs::create_dir(&vad_directory).unwrap();
    for filename in [".installed.json", "silero_vad.onnx"] {
        fs::copy(installed.join(filename), vad_directory.join(filename)).unwrap();
    }

    let samples = vec![0.0; 32_000];
    for id in ["parakeet-primeline-de-int8", "whisper-large-v3-turbo"] {
        let manager = Manager::with_root(temporary.path().to_owned()).unwrap();
        assert!(!manager.is_installed(id).unwrap());
        let mut engine = create_engine(manager, id, EngineConfig::default()).unwrap();
        let transcript = engine.transcribe(&samples, 16_000, Some("de")).unwrap();
        assert!(transcript.text.is_empty());
    }
}

fn transcribe_installed_model(model_id: &str) {
    let manager = Manager::new().expect("Der lokale Modellordner muss verfügbar sein.");
    assert!(
        manager.is_installed(model_id).unwrap_or(false),
        "Das ausgewählte ASR-Modell muss zuvor mit models-cli installiert werden."
    );
    assert!(
        manager.is_installed("silero-vad").unwrap_or(false),
        "silero-vad muss zuvor mit models-cli installiert werden."
    );
    let wav_path = std::env::var_os("QMS_TEST_WAV")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/private/record_test.wav")
        });
    let file = File::open(wav_path)
        .expect("Eine Sprachaufnahme über QMS_TEST_WAV oder record_test.wav bereitstellen.");
    let mut reader = hound::WavReader::new(BufReader::new(file))
        .expect("Die Testaufnahme muss eine gültige WAV-Datei sein.");
    let spec = reader.spec();
    assert!(
        spec.channels == 1 && spec.sample_rate == 16_000,
        "Die Testaufnahme muss 16 kHz mono verwenden."
    );
    let samples: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, 32) => reader
            .samples::<f32>()
            .collect::<Result<_, _>>()
            .expect("Die WAV-Samples müssen lesbar sein."),
        (hound::SampleFormat::Int, bits @ (16 | 24 | 32)) => {
            let scale = (1u64 << (bits - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|value| value as f32 / scale))
                .collect::<Result<_, _>>()
                .expect("Die WAV-Samples müssen lesbar sein.")
        }
        _ => panic!("Die Testaufnahme muss Float32 oder PCM16/24/32 verwenden."),
    };
    assert!(
        !samples.is_empty()
            && samples
                .iter()
                .all(|sample| sample.is_finite() && (-1.0..=1.0).contains(sample)),
        "Die Testaufnahme muss normalisierte, endliche Samples enthalten."
    );
    let mut engine = create_engine(manager, model_id, EngineConfig::default())
        .expect("Die Engine muss für das installierte Modell erstellbar sein.");
    let transcript = engine
        .transcribe(&samples, spec.sample_rate, Some("de"))
        .expect("Die lokale Spracherkennung muss erfolgreich sein.");
    assert!(
        !transcript.text.trim().is_empty(),
        "Die Sprachaufnahme muss ein nicht leeres Transkript liefern."
    );
    assert!(
        !transcript.processing_duration.is_zero(),
        "Die Verarbeitungszeit muss erfasst werden."
    );
}

#[test]
#[ignore = "Benötigt installiertes parakeet-v3-int8, silero-vad und eine private Sprachaufnahme; siehe README."]
fn parakeet_v3_transcribes_an_installed_recording() {
    transcribe_installed_model("parakeet-v3-int8");
}

#[test]
#[ignore = "Benötigt installiertes parakeet-primeline-de-int8, silero-vad und eine private Sprachaufnahme; siehe README."]
fn parakeet_primeline_transcribes_an_installed_recording() {
    transcribe_installed_model("parakeet-primeline-de-int8");
}

#[test]
#[ignore = "Benötigt installiertes whisper-large-v3-turbo, silero-vad und eine private Sprachaufnahme; siehe README."]
fn whisper_transcribes_an_installed_recording() {
    transcribe_installed_model("whisper-large-v3-turbo");
}
