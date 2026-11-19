mod input;
mod microphone;

use std::{fs, io, path::Path};

use corpus_recorder::{CorpusStore, Interface, build_items, run_session};
use quatsch_mit_sosse_audio::DeviceSelection;

fn main() {
    if let Err(error) = run() {
        eprintln!("Fehler: {error}");
        std::process::exit(1);
    }
}

fn run() -> io::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let selection = match args.as_slice() {
        [] => DeviceSelection::Default,
        [flag] if flag == "--help" || flag == "-h" => {
            println!(
                "Korpus-Recorder – Quatsch mit Soße\n\
                 40 deutsche Sätze, danach 5 Stille- und 5 Tastaturaufnahmen.\n\
                 Start im interaktiven Windows-Terminal, ohne Pipe/Umleitung.\n\
                 Enter: starten / stoppen / speichern und weiter (drei einzelne Tastendrücke).\n\
                 W: ungespeicherte Aufnahme wiederholen; S: überspringen.\n\
                 Bei laufenden Tastaturaufnahmen sind W und S normale Buchstaben.\n\
                 Escape oder Strg+C: beenden; die unbestätigte Aufnahme wird verworfen.\n\
                 Vorlauf: 500 ms aus crates/audio; WAV: 16 kHz mono, Float32.\n\
                 --device \"Exakter Mikrofonname\": Gerät wählen, sonst Windows-Standard.\n\
                 Satzliste: testdata/private/sentences.txt (UTF-8, genau 40 Zeilen).\n\
                 Ausgabe: testdata/private/corpus/NNN.wav und NNN.txt im Projektordner.\n\
                 Vorhandene Paare bleiben erhalten; ein Neustart ergänzt fehlende Aufnahmen."
            );
            return Ok(());
        }
        [flag, name] if flag == "--device" && !name.is_empty() => DeviceSelection::Named(
            name.to_str()
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Der Mikrofonname ist kein gültiger Unicode-Text.",
                    )
                })?
                .to_owned(),
        ),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Ungültige Argumente. Erlaubt: --help oder --device \"Exakter Mikrofonname\".",
            ));
        }
    };
    // Validate the console before reading or displaying any private references.
    let mut terminal = input::Terminal::open()?;
    let private = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("The recorder is under tools/corpus-recorder")
        .join("testdata")
        .join("private");
    let sentences = fs::read_to_string(private.join("sentences.txt")).map_err(|_| io::Error::other(
        "Die private Satzliste konnte nicht als UTF-8 gelesen werden. Bitte testdata/private/sentences.txt mit genau 40 Zeilen bereitstellen."
    ))?;
    let items = build_items(&sentences)?;
    let store = CorpusStore::new(private.join("corpus"));
    // Fail before opening the microphone if an interrupted save needs attention.
    for item in &items {
        store.contains(item)?;
    }
    terminal.write(
        "Korpus-Recorder – Quatsch mit Soße\n\
         40 Sprachaufnahmen und 10 Aufnahmen ohne Sprache; Zieldauer je 10 bis 40 Sekunden.\n\
         Das Mikrofon bleibt während der Sitzung für den 500-ms-Vorlauf geöffnet.\n\
         Audio bleibt bis zur Bestätigung im Arbeitsspeicher. Speicherung ausschließlich im privaten Korpus.\n\
         Enter startet, Enter stoppt, ein weiteres Enter speichert und geht weiter.\n\
         W wiederholt, S überspringt; Escape oder Strg+C beendet ohne den unbestätigten Take.\n"
    )?;
    let mut recorder = microphone::Microphone::new(selection);
    let result = run_session(&items, &mut recorder, &mut terminal, &store);
    drop(recorder);
    terminal.write("\nMikrofon freigegeben.\n")?;
    let summary = result?;
    terminal.write(&format!(
        "{} Neu gespeichert: {} · bereits vorhanden: {} · übersprungen: {}.\n",
        if summary.aborted {
            "Sitzung beendet."
        } else {
            "Durchlauf beendet."
        },
        summary.saved,
        summary.existing,
        summary.skipped
    ))?;
    if summary.saved + summary.existing < 50 {
        terminal.write(
            "Zum Ergänzen denselben Befehl erneut starten; vorhandene Paare bleiben erhalten.\n",
        )?;
    }
    Ok(())
}
