use std::{
    ffi::OsString,
    fs::{self, File},
    io::{BufReader, Read, Seek},
    path::PathBuf,
};

use quatsch_mit_sosse_models::Manager;
use quatsch_mit_sosse_stt::{EngineConfig, create_engine};

enum Action {
    Help,
    Transcribe { wav: PathBuf, model_id: String },
}

fn parse_args(args: &[OsString]) -> Result<Action, &'static str> {
    match args {
        [flag] if flag == "--help" || flag == "-h" => Ok(Action::Help),
        [wav, model_id] if !wav.is_empty() && !model_id.is_empty() => {
            let model_id = model_id
                .to_str()
                .filter(|id| {
                    id.bytes()
                        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
                })
                .ok_or("Die Modell-ID muss aus ASCII-Kleinbuchstaben, Ziffern oder Bindestrichen bestehen.")?;
            Ok(Action::Transcribe {
                wav: PathBuf::from(wav),
                model_id: model_id.to_owned(),
            })
        }
        _ => Err("Aufruf: transcribe_file <wav> <modell-id>; Hilfe: --help."),
    }
}

fn read_wav(reader: impl Read + Seek) -> Result<Vec<f32>, &'static str> {
    let mut reader =
        hound::WavReader::new(reader).map_err(|_| "Die Eingabe ist keine lesbare WAV-Datei.")?;
    let spec = reader.spec();
    if spec.channels != 1 || spec.sample_rate != 16_000 {
        return Err("Die WAV-Datei muss 16 kHz mono verwenden.");
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
            "Die WAV-Datei muss normalisierte, endliche Samples enthalten und darf nicht leer sein.",
        );
    }
    Ok(samples)
}

fn main() {
    if let Err(message) = run() {
        eprintln!("Fehler: {message}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), &'static str> {
    let action = parse_args(&std::env::args_os().skip(1).collect::<Vec<_>>())?;
    let Action::Transcribe { wav, model_id } = action else {
        println!(
            "Lokale Spracherkennung: transcribe_file <wav> <modell-id>\n\
             WAV-Format: 16 kHz mono, Float32 oder PCM16/24/32.\n\
             ASR-Modell und silero-vad müssen vorher mit models-cli installiert sein.\n\
             Ausgabe: testdata/private/transcribe_file.txt im Projektordner (UTF-8).\n\
             Eine vorhandene Ergebnisdatei wird ersetzt. Transkript und Audio erscheinen nicht im Terminal."
        );
        return Ok(());
    };
    let file = File::open(wav).map_err(|_| "Die WAV-Datei konnte nicht geöffnet werden.")?;
    let samples = read_wav(BufReader::new(file))?;
    let manager = Manager::new().map_err(|_| "Der lokale Modellordner ist nicht verfügbar.")?;
    let mut engine = create_engine(manager, &model_id, EngineConfig::default()).map_err(
        |_| "Die Engine konnte nicht erstellt werden. Modell-ID und Installation prüfen.",
    )?;
    println!("Die Aufnahme wird lokal erkannt; eine vorhandene Ergebnisdatei wird ersetzt.");
    let transcript = engine
        .transcribe(&samples, 16_000, Some("de"))
        .map_err(|_| "Die Spracherkennung ist fehlgeschlagen. ASR-Modell und silero-vad müssen vollständig installiert sein.")?;
    let output_directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/private");
    fs::create_dir_all(&output_directory)
        .map_err(|_| "Der private Ausgabeordner konnte nicht erstellt werden.")?;
    fs::write(
        output_directory.join("transcribe_file.txt"),
        transcript.text.as_bytes(),
    )
    .map_err(|_| "Das private Transkript konnte nicht gespeichert werden.")?;
    println!(
        "Ergebnis: testdata/private/transcribe_file.txt (UTF-8). Verarbeitung: {:.3} Sekunden.",
        transcript.processing_duration.as_secs_f64()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, io::Cursor};

    use super::{Action, parse_args, read_wav};

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

    #[test]
    fn parses_a_unicode_path_and_an_explicit_model() {
        let args = [
            OsString::from("Aufnahmen mit Soße/privat.wav"),
            OsString::from("parakeet-v3-int8"),
        ];
        assert!(matches!(
            parse_args(&args),
            Ok(Action::Transcribe { wav, model_id })
                if wav == std::path::Path::new("Aufnahmen mit Soße/privat.wav")
                    && model_id == "parakeet-v3-int8"
        ));
        assert!(matches!(parse_args(&["--help".into()]), Ok(Action::Help)));
    }

    #[test]
    fn rejects_missing_extra_and_empty_arguments() {
        assert!(parse_args(&[]).is_err());
        assert!(parse_args(&["a.wav".into()]).is_err());
        assert!(parse_args(&["a.wav".into(), "model".into(), "extra".into()]).is_err());
        assert!(parse_args(&["".into(), "model".into()]).is_err());
        assert!(parse_args(&["a.wav".into(), "".into()]).is_err());
        assert!(parse_args(&["a.wav".into(), "Modell mit Umlaut ä".into()]).is_err());
    }

    #[test]
    fn reads_normalized_float_audio() {
        let bytes = wave(
            spec(hound::SampleFormat::Float, 32),
            &[-1.0f32, 0.0, 0.5, 1.0],
        );
        assert_eq!(read_wav(Cursor::new(bytes)).unwrap(), [-1.0, 0.0, 0.5, 1.0]);
    }

    #[test]
    fn reads_pcm16_pcm24_and_pcm32_with_the_correct_scale() {
        for bits in [16, 24, 32] {
            let magnitude = 1i64 << (bits - 1);
            let bytes = wave(
                spec(hound::SampleFormat::Int, bits),
                &[-magnitude as i32, 0, (magnitude / 2) as i32],
            );
            assert_eq!(read_wav(Cursor::new(bytes)).unwrap(), [-1.0, 0.0, 0.5]);
        }
    }

    #[test]
    fn rejects_unsupported_channels_rates_and_bit_depth() {
        let mut stereo = spec(hound::SampleFormat::Int, 16);
        stereo.channels = 2;
        let mut wrong_rate = spec(hound::SampleFormat::Int, 16);
        wrong_rate.sample_rate = 48_000;
        for invalid_spec in [stereo, wrong_rate, spec(hound::SampleFormat::Int, 8)] {
            assert!(read_wav(Cursor::new(wave(invalid_spec, &[0i16, 0]))).is_err());
        }
    }

    #[test]
    fn rejects_non_finite_out_of_range_empty_and_truncated_audio() {
        for invalid_sample in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.001, -1.001] {
            let bytes = wave(spec(hound::SampleFormat::Float, 32), &[invalid_sample]);
            assert!(read_wav(Cursor::new(bytes)).is_err());
        }
        let empty = wave::<i16>(spec(hound::SampleFormat::Int, 16), &[]);
        assert!(read_wav(Cursor::new(empty)).is_err());
        let mut truncated = wave(spec(hound::SampleFormat::Int, 16), &[1i16, 2]);
        truncated.pop();
        assert!(read_wav(Cursor::new(truncated)).is_err());
        assert!(read_wav(Cursor::new(b"invalid".to_vec())).is_err());
    }
}
