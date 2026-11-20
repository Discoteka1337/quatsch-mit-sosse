use std::{fs, process::Command};

#[test]
fn missing_models_fail_without_publishing_a_report_or_echoing_private_inputs() {
    let temporary = tempfile::tempdir().unwrap();
    let corpus = temporary.path().join("Korpus mit Soße");
    let output = temporary.path().join("Berichte");
    fs::create_dir(&corpus).unwrap();
    let sentinel = "SYNTHETIC_PRIVATE_REFERENCE_SENTINEL";
    fs::write(corpus.join("001.txt"), sentinel).unwrap();
    let mut wav = hound::WavWriter::create(
        corpus.join("001.wav"),
        hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for _ in 0..16_000 {
        wav.write_sample(0.0f32).unwrap();
    }
    wav.finalize().unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_bench"))
        .env("LOCALAPPDATA", temporary.path().join("Leerer Modellordner"))
        .args(["--date", "2026-11-20", "--corpus"])
        .arg(&corpus)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("Kein vollständig installiertes ASR-Modell"));
    assert!(!stdout.contains(sentinel) && !stderr.contains(sentinel));
    assert!(!stdout.contains("Korpus mit Soße") && !stderr.contains("Korpus mit Soße"));
    assert!(!output.join("asr-2026-11-20.md").exists());
    assert_eq!(
        fs::read_to_string(corpus.join("001.txt")).unwrap(),
        sentinel
    );
}
