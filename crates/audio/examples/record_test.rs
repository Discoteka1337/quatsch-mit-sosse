use std::{
    error::Error,
    fs::{self, File},
    io::{self, BufWriter, Seek, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use quatsch_mit_sosse_audio::{
    AudioCapture, AudioEvent, DeviceInfo, DeviceSelection, SAMPLE_RATE, default_input_device,
    input_devices,
};

const TEST_SECONDS: u32 = 10;

enum Action {
    List,
    Help,
    Record(DeviceSelection),
}

fn parse_args(args: &[String]) -> Result<Action, &'static str> {
    match args {
        [] => Ok(Action::Record(DeviceSelection::Default)),
        [flag] if flag == "--list" => Ok(Action::List),
        [flag] if flag == "--help" || flag == "-h" => Ok(Action::Help),
        [flag, name] if flag == "--device" && !name.is_empty() => {
            Ok(Action::Record(DeviceSelection::Named(name.clone())))
        }
        _ => Err("Ungültige Argumente. Erlaubt: --list, --help oder --device \"Exakter Name\"."),
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Fehler: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let action = parse_args(&std::env::args().skip(1).collect::<Vec<_>>())
        .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?;
    match action {
        Action::List => {
            let default = default_input_device()?;
            let devices = input_devices()?;
            write_device_list(&mut io::stdout().lock(), &devices, default.as_ref())?;
            Ok(())
        }
        Action::Help => {
            println!(
                "Mikrofontest: 10 Sekunden Aufnahme plus 500 ms Vorlauf (bei ungestörtem Eingang etwa 10,5 Sekunden WAV).\n\
                 Ohne Argumente: Standardmikrofon. --device \"Exakter Name\": Gerätewahl.\n\
                 --list: Eingabegeräte anzeigen, ohne das Mikrofon zu öffnen.\n\
                 Ausgabe: testdata/private/record_test.wav im Projektordner, 16 kHz Mono, 32-Bit-Float.\n\
                 Eine vorhandene Testaufnahme an diesem Pfad wird ersetzt."
            );
            Ok(())
        }
        Action::Record(selection) => record(selection),
    }
}

fn write_device_list(
    writer: &mut impl Write,
    devices: &[DeviceInfo],
    default: Option<&DeviceInfo>,
) -> io::Result<()> {
    match default {
        Some(device) => writeln!(writer, "Standardmikrofon: {}", device.name)?,
        None => writeln!(writer, "Standardmikrofon: keines")?,
    }
    if devices.is_empty() {
        writeln!(writer, "Keine Eingabegeräte gefunden.")?;
    } else {
        writeln!(writer, "Eingabegeräte:")?;
        for device in devices {
            writeln!(writer, "- {}", device.name)?;
        }
    }
    Ok(())
}

fn record(selection: DeviceSelection) -> Result<(), Box<dyn Error>> {
    println!(
        "Bereitschaft wird aktiviert: Das Mikrofon ist ab jetzt geöffnet.\n\
         Der laufende 500-ms-Vorlauf bleibt zunächst nur im Arbeitsspeicher."
    );
    let levels = Arc::new(AtomicUsize::new(0));
    let level_count = Arc::clone(&levels);
    let event_count = Arc::clone(&levels);
    let mut capture = AudioCapture::start(
        selection,
        move |_| {
            level_count.fetch_add(1, Ordering::Relaxed);
        },
        move |event| report_event(event, &event_count),
    )?;
    wait_for_preroll(&levels)?;
    println!(
        "Mikrofon bereit. Enter startet 10 Sekunden Aufnahme; sprich direkt beim Start.\n\
         Die WAV enthält zusätzlich die letzten 500 ms davor (insgesamt etwa 10,5 Sekunden)."
    );
    io::stdout().flush()?;
    let mut input = String::new();
    if io::stdin().read_line(&mut input)? == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "Keine Eingabe verfügbar. Bitte den Mikrofontest in einem interaktiven Terminal starten.",
        )
        .into());
    }
    if levels.load(Ordering::Relaxed) < 16 {
        println!("Das Mikrofon wurde gewechselt. Der Vorlauf wird erneut gefüllt …");
        wait_for_preroll(&levels)?;
    }
    capture.start_recording()?;
    println!("Aufnahme läuft – bitte 10 Sekunden sprechen.");
    thread::sleep(Duration::from_secs(u64::from(TEST_SECONDS)));
    let recording = capture.stop_ready()?.ok_or_else(|| {
        io::Error::other("Die Aufnahme wurde unerwartet beendet; es wird keine WAV geschrieben.")
    })?;
    println!("Aufnahme beendet; das Mikrofon ist freigegeben.");

    let destination = destination_path();
    save_recording(&destination, recording.samples())?;
    println!(
        "Gespeichert: {} ({:.2} Sekunden, 16 kHz Mono, 32-Bit-Float).",
        destination.display(),
        recording.duration().as_secs_f64()
    );
    Ok(())
}

fn wait_for_preroll(levels: &AtomicUsize) -> Result<(), io::Error> {
    let deadline = Instant::now() + Duration::from_secs(10);
    // Sixteen RMS reports guarantee more than 500 ms of received audio.
    while levels.load(Ordering::Relaxed) < 16 {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Innerhalb von 10 Sekunden kamen nicht genügend Audiodaten vom Mikrofon an. \
                 Bitte Gerät, Windows-Standardmikrofon und Mikrofonberechtigung prüfen. \
                 Es wird keine WAV geschrieben.",
            ));
        }
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

fn report_event(event: AudioEvent, levels: &AtomicUsize) {
    match event {
        AudioEvent::DeviceChanged { device, fallback } => {
            levels.store(0, Ordering::Relaxed);
            if fallback {
                eprintln!("Rückfall auf Standardmikrofon: {}", device.name);
            } else {
                eprintln!("Aktives Mikrofon: {}", device.name);
            }
        }
        AudioEvent::DeviceLost { device, error } => {
            levels.store(0, Ordering::Relaxed);
            eprintln!("Mikrofon ausgefallen ({}): {error}", device.name);
        }
        AudioEvent::DeviceUnavailable { error } => {
            levels.store(0, Ordering::Relaxed);
            eprintln!("Mikrofon nicht verfügbar: {error}");
        }
        AudioEvent::InputOverflow => {
            levels.store(0, Ordering::Relaxed);
            eprintln!("Audiounterbrechung: Der Mikrofonstream wird neu aufgebaut.");
        }
        AudioEvent::InputDiscontinuity => {
            levels.store(0, Ordering::Relaxed);
            eprintln!("Audiounterbrechung: Das gewählte Mikrofon wird erneut geöffnet.");
        }
        AudioEvent::RealtimeUnavailable => {
            eprintln!("Hinweis: Die Aufnahme läuft ohne erhöhte Threadpriorität weiter.");
        }
        AudioEvent::Stopped => {}
    }
}

fn destination_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("Die Audio-Crate liegt unter crates/audio.")
        .join("testdata")
        .join("private")
        .join("record_test.wav")
}

fn write_wav<W: Write + Seek>(writer: W, samples: &[f32]) -> Result<(), hound::Error> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::new(writer, spec)?;
    for sample in samples {
        writer.write_sample(*sample)?;
    }
    writer.finalize()
}

fn publish_wav(path: &Path, write: impl FnOnce(&mut File) -> io::Result<()>) -> io::Result<()> {
    let directory = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
    fs::create_dir_all(directory)?;
    let mut wav = tempfile::NamedTempFile::new_in(directory)?;
    write(wav.as_file_mut())?;
    wav.as_file().sync_all()?;
    wav.persist(path)?;
    Ok(())
}

fn save_recording(path: &Path, samples: &[f32]) -> io::Result<()> {
    if samples.len() < (TEST_SECONDS * SAMPLE_RATE) as usize {
        return Err(io::Error::other(
            "Das Mikrofon hat weniger als zehn Sekunden Audio geliefert. Bitte die Testaufnahme wiederholen.",
        ));
    }
    publish_wav(path, |file| {
        write_wav(BufWriter::new(file), samples).map_err(io::Error::other)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn a_preroll_only_take_cannot_replace_the_ten_second_microphone_test() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("record_test.wav");
        save_recording(&path, &vec![0.25; 168_000]).unwrap();
        let previous = fs::read(&path).unwrap();
        assert!(save_recording(&path, &[0.0; 8_000]).is_err());
        assert!(fs::read(&path).unwrap() == previous);
    }

    #[test]
    fn failed_wav_replacement_preserves_the_previous_recording() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("record_test.wav");
        publish_wav(&path, |file| {
            write_wav(file, &[0.25]).map_err(io::Error::other)
        })
        .unwrap();
        let previous = fs::read(&path).unwrap();
        assert!(
            publish_wav(&path, |file| {
                file.write_all(b"partial")?;
                Err(io::Error::other("Synthetischer Schreibfehler"))
            })
            .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), previous);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
        publish_wav(&path, |file| {
            write_wav(file, &[0.5]).map_err(io::Error::other)
        })
        .unwrap();
        assert_ne!(fs::read(&path).unwrap(), previous);
    }

    #[test]
    fn duplicate_names_are_listed_without_claiming_both_are_default() {
        let device = DeviceInfo {
            name: "USB-Mikrofon".into(),
        };
        let mut output = Vec::new();
        write_device_list(
            &mut output,
            &[device.clone(), device.clone()],
            Some(&device),
        )
        .unwrap();
        let output = String::from_utf8(output).unwrap();
        assert_eq!(output.matches("Standardmikrofon:").count(), 1);
        assert_eq!(
            output
                .lines()
                .filter(|line| *line == "- USB-Mikrofon")
                .count(),
            2
        );
        assert!(!output.contains("(Standard)"));
    }

    #[test]
    fn discontinuity_resets_warmup_but_realtime_warning_keeps_it() {
        let levels = AtomicUsize::new(16);
        report_event(AudioEvent::RealtimeUnavailable, &levels);
        assert_eq!(levels.load(Ordering::Relaxed), 16);
        report_event(AudioEvent::InputDiscontinuity, &levels);
        assert_eq!(levels.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn wav_preserves_float_samples_format_and_ten_seconds_plus_preroll() {
        let samples: Vec<f32> = (0..168_000)
            .map(|index| (index % 32) as f32 / 16.0 - 1.0)
            .collect();
        let mut buffer = Cursor::new(Vec::new());
        write_wav(&mut buffer, &samples).unwrap();
        buffer.set_position(0);
        let mut reader = hound::WavReader::new(buffer).unwrap();
        let spec = reader.spec();
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.sample_rate, 16_000);
        assert_eq!(spec.bits_per_sample, 32);
        assert_eq!(spec.sample_format, hound::SampleFormat::Float);
        assert_eq!(reader.duration(), 168_000);
        assert_eq!(
            reader
                .samples::<f32>()
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            samples
        );
    }

    #[test]
    fn list_and_help_are_separate_from_recording() {
        assert!(matches!(parse_args(&["--list".into()]), Ok(Action::List)));
        assert!(matches!(parse_args(&["--help".into()]), Ok(Action::Help)));
        assert!(matches!(
            parse_args(&[]),
            Ok(Action::Record(DeviceSelection::Default))
        ));
        assert!(matches!(
            parse_args(&["--device".into(), "Mikrofon mit Umlaut ä".into()]),
            Ok(Action::Record(DeviceSelection::Named(name))) if name == "Mikrofon mit Umlaut ä"
        ));
        assert!(parse_args(&["--device".into()]).is_err());
        assert!(parse_args(&["--device".into(), String::new()]).is_err());
        assert!(parse_args(&["--list".into(), "--device".into(), "Name".into()]).is_err());
        assert!(parse_args(&["--unbekannt".into()]).is_err());
    }
}
