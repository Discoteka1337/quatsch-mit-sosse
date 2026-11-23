use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::BufReader,
    path::{Path, PathBuf},
};

// References and audio are private and deliberately have no Debug implementation.
pub struct Recording {
    pub id: u16,
    pub samples: Vec<f32>,
    pub reference: String,
}

/// Compares the exact in-memory inputs of the parent and its model workers.
pub fn fingerprint(recordings: &[Recording]) -> String {
    use sha2::{Digest, Sha256};

    let mut hash = Sha256::new();
    for recording in recordings {
        hash.update(recording.id.to_le_bytes());
        hash.update((recording.reference.len() as u64).to_le_bytes());
        hash.update(recording.reference.as_bytes());
        hash.update((recording.samples.len() as u64).to_le_bytes());
        for sample in &recording.samples {
            hash.update(sample.to_le_bytes());
        }
    }
    format!("{:x}", hash.finalize())
}

pub fn load_corpus(directory: &Path) -> Result<Vec<Recording>, &'static str> {
    let entries = fs::read_dir(directory).map_err(|_| "Der private Korpus ist nicht lesbar.")?;
    let mut pairs: BTreeMap<u16, [Option<PathBuf>; 2]> = BTreeMap::new();
    for entry in entries {
        let entry =
            entry.map_err(|_| "Der private Korpus konnte nicht vollständig gelesen werden.")?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let bytes = name.as_bytes();
        if bytes.len() != 7 || !bytes[..3].iter().all(u8::is_ascii_digit) || bytes[3] != b'.' {
            continue;
        }
        let slot = if name[4..].eq_ignore_ascii_case("wav") {
            0
        } else if name[4..].eq_ignore_ascii_case("txt") {
            1
        } else {
            continue;
        };
        if !entry
            .file_type()
            .map_err(|_| "Ein Korpuseintrag konnte nicht geprüft werden.")?
            .is_file()
        {
            return Err("Korpuspaare müssen aus regulären Dateien bestehen.");
        }
        let id = name[..3]
            .parse()
            .map_err(|_| "Eine Aufnahmenummer ist ungültig.")?;
        if pairs.entry(id).or_default()[slot]
            .replace(entry.path())
            .is_some()
        {
            return Err("Der private Korpus enthält eine doppelte Aufnahmenummer.");
        }
    }
    if pairs.is_empty() {
        return Err("Der private Korpus enthält keine nummerierten WAV/TXT-Paare.");
    }
    pairs
        .into_iter()
        .map(|(id, pair)| {
            let [Some(wav), Some(reference)] = pair else {
                return Err("Der private Korpus enthält ein unvollständiges WAV/TXT-Paar.");
            };
            let reference = fs::read_to_string(reference).map_err(
                |_| "Eine Referenzdatei ist nicht lesbar oder enthält ungültiges UTF-8.",
            )?;
            Ok(Recording {
                id,
                samples: read_wav(&wav)?,
                reference: reference.trim_start_matches('\u{feff}').to_owned(),
            })
        })
        .collect()
}

fn read_wav(path: &Path) -> Result<Vec<f32>, &'static str> {
    let file = File::open(path).map_err(|_| "Eine WAV-Datei konnte nicht geöffnet werden.")?;
    let mut reader = hound::WavReader::new(BufReader::new(file))
        .map_err(|_| "Eine Aufnahme ist keine lesbare WAV-Datei.")?;
    let spec = reader.spec();
    if spec.channels != 1 || spec.sample_rate != 16_000 {
        return Err("Die WAV-Dateien müssen 16 kHz mono verwenden.");
    }
    let samples: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, 32) => reader
            .samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(|_| "Die WAV-Samples sind beschädigt oder unvollständig.")?,
        (hound::SampleFormat::Int, bits @ (16 | 24 | 32)) => {
            let scale = (1u64 << (bits - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|value| value as f32 / scale))
                .collect::<Result<_, _>>()
                .map_err(|_| "Die WAV-Samples sind beschädigt oder unvollständig.")?
        }
        _ => return Err("Unterstützt werden Float32 und PCM16/24/32."),
    };
    if samples.is_empty()
        || samples
            .iter()
            .any(|sample| !sample.is_finite() || !(-1.0..=1.0).contains(sample))
    {
        return Err(
            "Die WAV-Dateien müssen normalisierte, endliche Samples enthalten und dürfen nicht leer sein.",
        );
    }
    Ok(samples)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::Cursor};

    fn wave<T: hound::Sample + Copy>(spec: hound::WavSpec, samples: &[T]) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = hound::WavWriter::new(&mut cursor, spec).unwrap();
            for sample in samples {
                writer.write_sample(*sample).unwrap();
            }
            writer.finalize().unwrap();
        }
        cursor.into_inner()
    }

    fn spec(format: hound::SampleFormat, bits: u16) -> hound::WavSpec {
        hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: bits,
            sample_format: format,
        }
    }

    fn save_pair(directory: &Path, id: u16, wav: &[u8], reference: &[u8]) {
        fs::write(directory.join(format!("{id:03}.wav")), wav).unwrap();
        fs::write(directory.join(format!("{id:03}.txt")), reference).unwrap();
    }

    #[test]
    fn loads_all_numeric_pairs_sorted_with_empty_silence_reference() {
        let temporary = tempfile::tempdir().unwrap();
        let directory = temporary.path().join("Korpus mit Soße");
        fs::create_dir(&directory).unwrap();
        let wav = wave(spec(hound::SampleFormat::Float, 32), &[0.0f32, 0.5]);
        save_pair(&directory, 42, &wav, b"");
        save_pair(
            &directory,
            3,
            &wav,
            "\u{feff}Synthetische Äpfel.".as_bytes(),
        );
        fs::write(directory.join("readme.txt"), b"ignored").unwrap();
        fs::write(directory.join("1000.wav"), b"ignored").unwrap();
        let corpus = load_corpus(&directory).unwrap();
        assert_eq!(
            corpus.iter().map(|item| item.id).collect::<Vec<_>>(),
            [3, 42]
        );
        assert_eq!(corpus[0].samples, [0.0, 0.5]);
        assert_eq!(corpus[0].reference, "Synthetische Äpfel.");
        assert!(corpus[1].reference.is_empty());
    }

    #[test]
    fn rejects_missing_directory_empty_corpus_and_both_kinds_of_orphans() {
        let temporary = tempfile::tempdir().unwrap();
        assert!(load_corpus(&temporary.path().join("missing")).is_err());
        assert!(load_corpus(temporary.path()).is_err());
        for suffix in ["wav", "txt"] {
            let directory = temporary.path().join(suffix);
            fs::create_dir(&directory).unwrap();
            fs::write(directory.join(format!("001.{suffix}")), b"").unwrap();
            assert!(load_corpus(&directory).is_err());
        }
    }

    #[test]
    fn accepts_normalized_pcm16_pcm24_pcm32_and_float32() {
        for bits in [16, 24, 32] {
            let temporary = tempfile::tempdir().unwrap();
            let magnitude = 1i64 << (bits - 1);
            let wav = wave(
                spec(hound::SampleFormat::Int, bits),
                &[-magnitude as i32, 0, (magnitude / 2) as i32],
            );
            save_pair(temporary.path(), 1, &wav, b"synthetic");
            let corpus = load_corpus(temporary.path()).unwrap();
            assert_eq!(corpus[0].samples, [-1.0, 0.0, 0.5]);
        }
    }

    #[test]
    fn rejects_unsupported_channels_rates_and_sample_formats() {
        let mut stereo = spec(hound::SampleFormat::Int, 16);
        stereo.channels = 2;
        let mut wrong_rate = spec(hound::SampleFormat::Int, 16);
        wrong_rate.sample_rate = 48_000;
        for invalid_spec in [stereo, wrong_rate, spec(hound::SampleFormat::Int, 8)] {
            let temporary = tempfile::tempdir().unwrap();
            save_pair(temporary.path(), 1, &wave(invalid_spec, &[0i16, 0]), b"");
            assert!(load_corpus(temporary.path()).is_err());
        }
    }

    #[test]
    fn rejects_non_finite_out_of_range_empty_and_truncated_audio() {
        for sample in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.001, -1.001] {
            let temporary = tempfile::tempdir().unwrap();
            let wav = wave(spec(hound::SampleFormat::Float, 32), &[sample]);
            save_pair(temporary.path(), 1, &wav, b"");
            assert!(load_corpus(temporary.path()).is_err());
        }
        let empty = wave::<i16>(spec(hound::SampleFormat::Int, 16), &[]);
        let mut truncated = wave(spec(hound::SampleFormat::Int, 16), &[1i16, 2]);
        truncated.pop();
        for invalid in [empty, truncated, b"invalid".to_vec()] {
            let temporary = tempfile::tempdir().unwrap();
            save_pair(temporary.path(), 1, &invalid, b"");
            assert!(load_corpus(temporary.path()).is_err());
        }
    }

    #[test]
    fn errors_do_not_reveal_invalid_reference_bytes_or_file_names() {
        let temporary = tempfile::tempdir().unwrap();
        let directory = temporary.path().join("private-secret-name");
        fs::create_dir(&directory).unwrap();
        let wav = wave(spec(hound::SampleFormat::Float, 32), &[0.0f32]);
        save_pair(&directory, 1, &wav, b"private-secret-text\xff");
        let error = load_corpus(&directory).err().unwrap();
        assert!(!error.contains("private-secret"));
    }

    #[test]
    fn refuses_directories_in_place_of_recording_files() {
        let temporary = tempfile::tempdir().unwrap();
        fs::create_dir(temporary.path().join("001.wav")).unwrap();
        fs::write(temporary.path().join("001.txt"), b"").unwrap();
        assert!(load_corpus(temporary.path()).is_err());
    }
}
