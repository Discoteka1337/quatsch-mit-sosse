use std::{
    fs,
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
};

use quatsch_mit_sosse_audio::{PRE_ROLL, SAMPLE_RATE};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Enter,
    Repeat,
    Skip,
    Quit,
    Other,
}

pub trait Interface {
    fn write(&mut self, message: &str) -> io::Result<()>;
    fn read_key(&mut self) -> io::Result<Key>;
}

pub trait Recorder {
    fn prepare(&mut self) -> io::Result<()>;
    fn start(&mut self) -> io::Result<()>;
    fn finish(&mut self) -> io::Result<Vec<f32>>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptKind {
    AiPrompt,
    Everyday,
    FormalEmail,
    Numbers,
    Corrections,
    Silence,
    Keyboard,
}

impl PromptKind {
    fn title(self) -> &'static str {
        match self {
            Self::AiPrompt => "KI-Prompt mit Fachwörtern",
            Self::Everyday => "Alltagsnachricht",
            Self::FormalEmail => "E-Mail im Sie-Ton",
            Self::Numbers => "Zahlen, Datum, Uhrzeit, Prozent und Euro",
            Self::Corrections => "Selbstkorrekturen und Füllwörter",
            Self::Silence => {
                "Stille: Bitte 10 bis 20 Sekunden völlig still bleiben und nicht tippen."
            }
            Self::Keyboard => {
                "Tastaturgeräusche: Bitte 10 bis 20 Sekunden nur tippen, nicht sprechen.\n\
                 Buchstaben und Leertaste verwenden; Enter stoppt, Escape/Strg+C beendet."
            }
        }
    }
}

// Deliberately no Debug implementation: references belong only in the prompt and private files.
pub struct Item {
    pub id: usize,
    pub kind: PromptKind,
    pub reference: String,
}

pub fn build_items(input: &str) -> io::Result<Vec<Item>> {
    let lines: Vec<_> = input.trim_start_matches('\u{feff}').lines().collect();
    if lines.len() != 40 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Die Satzliste muss genau 40 nicht leere Zeilen enthalten (UTF-8).",
        ));
    }
    let mut items = Vec::with_capacity(50);
    for (index, line) in lines.into_iter().enumerate() {
        if line.trim().is_empty() || line.chars().any(char::is_control) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Satz {} ist leer oder enthält Steuerzeichen.", index + 1),
            ));
        }
        items.push(Item {
            id: index + 1,
            kind: match index {
                0..15 => PromptKind::AiPrompt,
                15..23 => PromptKind::Everyday,
                23..29 => PromptKind::FormalEmail,
                29..34 => PromptKind::Numbers,
                _ => PromptKind::Corrections,
            },
            reference: line.to_owned(),
        });
    }
    items.extend((41..=50).map(|id| Item {
        id,
        kind: if id <= 45 {
            PromptKind::Silence
        } else {
            PromptKind::Keyboard
        },
        reference: String::new(),
    }));
    Ok(items)
}

pub struct CorpusStore {
    directory: PathBuf,
}

impl CorpusStore {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    fn paths(&self, item: &Item) -> io::Result<(PathBuf, PathBuf)> {
        if !(1..=50).contains(&item.id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Ungültige Aufnahmenummer.",
            ));
        }
        Ok((
            self.directory.join(format!("{:03}.wav", item.id)),
            self.directory.join(format!("{:03}.txt", item.id)),
        ))
    }

    pub fn contains(&self, item: &Item) -> io::Result<bool> {
        let (wav, reference) = self.paths(item)?;
        match (regular_file(&wav)?, regular_file(&reference)?) {
            (true, true) => Ok(true),
            (false, false) => Ok(false),
            _ => Err(io::Error::other(format!(
                "Aufnahme {:03} ist unvollständig. Bitte das vorhandene WAV/TXT-Paar im privaten Korpus prüfen und sichern, bevor Sie es erneut aufnehmen.",
                item.id
            ))),
        }
    }

    pub fn save(&self, item: &Item, samples: &[f32]) -> io::Result<()> {
        let (wav_path, reference_path) = self.paths(item)?;
        if samples.is_empty() || samples.iter().any(|sample| !sample.is_finite()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Keine gültigen Audiodaten zum Speichern.",
            ));
        }
        if self.contains(item)? {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Die Aufnahme ist bereits gespeichert und wird nicht überschrieben.",
            ));
        }
        fs::create_dir_all(&self.directory)?;
        let mut wav = tempfile::NamedTempFile::new_in(&self.directory)?;
        let mut reference = tempfile::NamedTempFile::new_in(&self.directory)?;
        {
            let mut writer = hound::WavWriter::new(
                BufWriter::new(wav.as_file_mut()),
                hound::WavSpec {
                    channels: 1,
                    sample_rate: SAMPLE_RATE,
                    bits_per_sample: 32,
                    sample_format: hound::SampleFormat::Float,
                },
            )
            .map_err(wav_error)?;
            for sample in samples {
                writer.write_sample(*sample).map_err(wav_error)?;
            }
            writer.finalize().map_err(wav_error)?;
        }
        reference.write_all(item.reference.as_bytes())?;
        wav.as_file().sync_all()?;
        reference.as_file().sync_all()?;
        // Publish only complete files, with no overwrite even if another recorder races us.
        wav.persist_noclobber(&wav_path)
            .map_err(|error| error.error)?;
        if let Err(error) = reference.persist_noclobber(&reference_path) {
            fs::remove_file(&wav_path).map_err(|_| io::Error::other(
                "Das Dateipaar konnte nicht vollständig gespeichert werden. Bitte den privaten Korpus prüfen."
            ))?;
            return Err(error.error);
        }
        Ok(())
    }
}

fn regular_file(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(true),
        Ok(_) => Err(io::Error::other("Ein Korpuspfad ist keine reguläre Datei.")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn wav_error(_: hound::Error) -> io::Error {
    io::Error::other("Die WAV-Datei konnte nicht geschrieben werden.")
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct Summary {
    pub saved: usize,
    pub skipped: usize,
    pub existing: usize,
    pub aborted: bool,
}

pub fn run_session(
    items: &[Item],
    recorder: &mut impl Recorder,
    ui: &mut impl Interface,
    store: &CorpusStore,
) -> io::Result<Summary> {
    let mut summary = Summary::default();
    'items: for item in items {
        if store.contains(item)? {
            ui.write(&format!(
                "{:03}: bereits gespeichert; bleibt erhalten.\n",
                item.id
            ))?;
            summary.existing += 1;
            continue;
        }
        loop {
            ui.write("\nMikrofonbereitschaft: 500-ms-Vorlauf wird gefüllt …\n")?;
            recorder.prepare()?;
            ui.write(&format!("\n{:03}/050 – {}\n", item.id, item.kind.title()))?;
            if !item.reference.is_empty() {
                ui.write(&format!("\n{}\n\n", item.reference))?;
            } else {
                ui.write("Referenz bleibt leer. Die Ansage bitte nicht mitsprechen.\n")?;
            }
            ui.write("Enter: Aufnahme starten · S: überspringen · Escape/Strg+C: beenden\n")?;
            loop {
                match ui.read_key()? {
                    Key::Enter => break,
                    Key::Skip => {
                        summary.skipped += 1;
                        continue 'items;
                    }
                    Key::Quit => {
                        summary.aborted = true;
                        return Ok(summary);
                    }
                    _ => {}
                }
            }
            recorder.start()?;
            let stop_key = read_stop_key(ui, item.kind);
            let take = recorder.finish();
            let stop_key = stop_key?;
            let samples = match take {
                Ok(samples) => Some(samples),
                Err(error) if error.kind() == io::ErrorKind::InvalidData => None,
                Err(error) => return Err(error),
            };
            match stop_key {
                Key::Repeat => continue,
                Key::Skip => {
                    summary.skipped += 1;
                    continue 'items;
                }
                Key::Quit => {
                    summary.aborted = true;
                    return Ok(summary);
                }
                _ => {}
            }
            let Some(samples) = samples else {
                ui.write("Die Aufnahme wurde unterbrochen oder ist leer. Bitte diesen Satz wiederholen.\n")?;
                continue;
            };
            let duration = samples.len() as f64 / f64::from(SAMPLE_RATE);
            ui.write(&format!(
                "Aufnahme gestoppt: {duration:.1} Sekunden einschließlich Vorlauf.\n"
            ))?;
            let spoken_duration = duration - PRE_ROLL.as_secs_f64();
            if !(10.0..=40.0).contains(&spoken_duration) {
                ui.write("Hinweis: Die Zielzeit beträgt 10 bis 40 Sekunden ohne Vorlauf. Bei Bedarf mit W wiederholen.\n")?;
            }
            ui.write("Enter: speichern und weiter · W: wiederholen · S: verwerfen und überspringen · Escape/Strg+C: beenden\n")?;
            loop {
                match ui.read_key()? {
                    Key::Enter => {
                        store.save(item, &samples)?;
                        ui.write(&format!(
                            "Gespeichert: {:03}.wav und {:03}.txt\n",
                            item.id, item.id
                        ))?;
                        summary.saved += 1;
                        continue 'items;
                    }
                    Key::Repeat => break,
                    Key::Skip => {
                        summary.skipped += 1;
                        continue 'items;
                    }
                    Key::Quit => {
                        summary.aborted = true;
                        return Ok(summary);
                    }
                    Key::Other => {}
                }
            }
        }
    }
    Ok(summary)
}

fn read_stop_key(ui: &mut impl Interface, kind: PromptKind) -> io::Result<Key> {
    ui.write(if kind == PromptKind::Keyboard {
        "Aufnahme läuft: Jetzt nur tippen. Enter: stoppen · Escape/Strg+C: beenden\n"
    } else {
        "Aufnahme läuft. Enter: stoppen · W: verwerfen/wiederholen · S: überspringen · Escape/Strg+C: beenden\n"
    })?;
    loop {
        match ui.read_key()? {
            Key::Other => {}
            Key::Repeat | Key::Skip if kind == PromptKind::Keyboard => {}
            key => return Ok(key),
        }
    }
}
