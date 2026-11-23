use std::{fs, path::Path, process::Command};

fn save_audio(directory: &Path, sample: f32) {
    let mut wav = hound::WavWriter::create(
        directory.join("001.wav"),
        hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    for _ in 0..16_000 {
        wav.write_sample(sample).unwrap();
    }
    wav.finalize().unwrap();
}

#[test]
fn workers_reject_changed_audio_or_references_before_loading_models() {
    let temporary = tempfile::tempdir().unwrap();
    let corpus = temporary.path().join("corpus");
    fs::create_dir(&corpus).unwrap();
    let reference = corpus.join("001.txt");
    fs::write(&reference, "Synthetischer Satz A").unwrap();
    save_audio(&corpus, 0.25);
    let original = bench::corpus::load_corpus(&corpus).unwrap();
    let fingerprint = bench::corpus::fingerprint(&original);
    let output = temporary.path().join("result.json");
    for (sample, text, changed) in [
        (0.5, "Synthetischer Satz A", true),
        (0.25, "Synthetischer Satz B", true),
        (0.25, "Synthetischer Satz A", false),
    ] {
        save_audio(&corpus, sample);
        fs::write(&reference, text).unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_bench"))
            .env("LOCALAPPDATA", temporary.path())
            .env("QMS_BENCH_CORPUS_FINGERPRINT", &fingerprint)
            .args(["--worker", "parakeet-v3-int8", "2"])
            .arg(&corpus)
            .arg(&output)
            .output()
            .unwrap();
        assert!(!output.exists());
        assert!(result.stdout.is_empty());
        if changed {
            assert_eq!(result.status.code(), Some(2));
            assert!(result.stderr.is_empty());
        } else {
            // Matching data reaches inference on this machine with an empty model directory.
            assert_eq!(result.status.code(), Some(1));
            assert!(String::from_utf8_lossy(&result.stderr).contains("Kaltstart"));
        }
    }
}

#[test]
fn missing_models_fail_without_publishing_a_report_or_echoing_private_inputs() {
    let temporary = tempfile::tempdir().unwrap();
    let corpus = temporary.path().join("Korpus mit Soße");
    let output = temporary.path().join("Berichte");
    fs::create_dir(&corpus).unwrap();
    let sentinel = "SYNTHETIC_PRIVATE_REFERENCE_SENTINEL";
    fs::write(corpus.join("001.txt"), sentinel).unwrap();
    save_audio(&corpus, 0.0);

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
