use bench::{
    corpus::load_corpus,
    measurement::{ModelResult, measure},
    report::{ReportContext, protection_checks, render_report},
};
use quatsch_mit_sosse_models::{Manager, ModelKind};
use quatsch_mit_sosse_stt::{EngineConfig, create_engine};
use std::{
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Debug, PartialEq)]
struct Options {
    date: String,
    threads: u32,
    corpus: PathBuf,
    output: PathBuf,
    check_protection: bool,
}

fn parse_options(args: &[OsString]) -> Result<Options, &'static str> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut options = Options {
        date: chrono::Local::now().format("%Y-%m-%d").to_string(),
        threads: EngineConfig::default().num_threads,
        corpus: root.join("testdata/private/corpus"),
        output: root.join("docs/benchmarks"),
        check_protection: false,
    };
    let mut seen = std::collections::HashSet::new();
    let mut args = args.iter();
    while let Some(flag) = args.next() {
        if !seen.insert(flag) {
            return Err("Eine Option wurde mehrfach angegeben.");
        }
        if flag == "--check-protection" {
            options.check_protection = true;
            continue;
        }
        let value = args
            .next()
            .filter(|value| !value.is_empty() && *value != "--check-protection")
            .ok_or("Für eine Option fehlt der Wert.")?;
        match flag.to_str() {
            Some("--date") => {
                let date = value
                    .to_str()
                    .ok_or("Das Datum muss JJJJ-MM-TT entsprechen.")?;
                let parsed = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
                    .map_err(|_| "Das Datum ist ungültig.")?;
                if parsed.format("%Y-%m-%d").to_string() != date || date.len() != 10 {
                    return Err("Das Datum muss JJJJ-MM-TT entsprechen.");
                }
                options.date = date.to_owned();
            }
            Some("--threads") => {
                options.threads = value
                    .to_str()
                    .and_then(|s| s.parse().ok())
                    .filter(|&n| n > 0 && n <= i32::MAX as u32)
                    .ok_or("Die Thread-Anzahl ist ungültig.")?;
            }
            Some("--corpus") => options.corpus = PathBuf::from(value),
            Some("--output") => options.output = PathBuf::from(value),
            _ => return Err("Unbekannte Option; Hilfe: --help."),
        }
    }
    Ok(options)
}

fn main() {
    // Panic payloads from dependencies must not expose audio, tokens or references.
    std::panic::set_hook(Box::new(|_| {
        eprintln!("Fehler: Der lokale Benchmark wurde unerwartet abgebrochen.")
    }));
    if let Err(message) = run() {
        eprintln!("Fehler: {message}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), &'static str> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--worker") {
        return worker(&args[1..]);
    }
    if args == [OsString::from("--help")] || args == [OsString::from("-h")] {
        println!(
            "Lokaler ASR-Benchmark\n\
            Aufruf: cargo run --release -p bench -- [--date JJJJ-MM-TT] [--threads N] [--corpus ORDNER] [--output ORDNER] [--check-protection]\n\
            Standard: privater Projektkorpus, docs/benchmarks, lokales Tagesdatum und 2 Threads.\n\
            Misst alle vollständig installierten ASR-Modelle mit Silero-VAD, ohne Downloads.\n\
            Der Tagesbericht wird atomar ersetzt. Satztexte und Audio werden nie ausgegeben.\n\
            --check-protection meldet nach dem Schreiben des Berichts Exitcode 1, wenn ein gemessenes Modell die Ziele für Stille oder Satzanfänge verfehlt oder die Stichprobe unvollständig ist."
        );
        return Ok(());
    }
    let options = parse_options(&args)?;
    let recordings = load_corpus(&options.corpus)?;
    println!(
        "Korpus geprüft: {} Paare, davon {} mit leerer Referenz.",
        recordings.len(),
        recordings
            .iter()
            .filter(|r| r.reference.trim().is_empty())
            .count()
    );
    drop(recordings);
    let manager = Manager::new().map_err(|_| "Der lokale Modellordner ist nicht verfügbar.")?;
    let mut installed = Vec::new();
    let mut uninstalled_models = Vec::new();
    for model in manager.list().iter().filter(|m| m.kind == ModelKind::Asr) {
        if manager
            .is_installed(&model.id)
            .map_err(|_| "Der Installationsstatus konnte nicht geprüft werden.")?
        {
            installed.push(model.id.clone());
        } else {
            uninstalled_models.push(model.id.clone());
        }
    }
    if installed.is_empty() {
        return Err("Kein vollständig installiertes ASR-Modell gefunden.");
    }
    if !manager
        .is_installed("silero-vad")
        .map_err(|_| "Der VAD-Installationsstatus konnte nicht geprüft werden.")?
    {
        return Err("Silero-VAD muss vor dem Benchmark vollständig installiert sein.");
    }
    let executable = std::env::current_exe()
        .map_err(|_| "Das Benchmark-Programm konnte nicht gefunden werden.")?;
    let mut results = Vec::new();
    for model_id in installed {
        println!(
            "Messung startet: {model_id} (Kaltstart, gesamter Korpus, Zehnsekunden-Ausschnitte). "
        );
        let output = tempfile::NamedTempFile::new()
            .map_err(|_| "Die temporäre Messdatei konnte nicht erstellt werden.")?;
        let status = Command::new(&executable)
            .arg("--worker")
            .arg(&model_id)
            .arg(options.threads.to_string())
            .arg(&options.corpus)
            .arg(output.path())
            // Native runtime diagnostics never reach the console or a log file.
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|_| "Der isolierte Messprozess konnte nicht gestartet werden.")?;
        if !status.success() {
            return Err(
                "Ein Modell-Messprozess ist fehlgeschlagen; es wird kein unvollständiger Bericht veröffentlicht.",
            );
        }
        let data =
            fs::read(output.path()).map_err(|_| "Die Messdaten konnten nicht gelesen werden.")?;
        let result: ModelResult =
            serde_json::from_slice(&data).map_err(|_| "Die Messdaten sind ungültig.")?;
        if result.model_id != model_id {
            return Err("Die Messdaten gehören nicht zum gestarteten Modell.");
        }
        println!("Messung abgeschlossen: {model_id}.");
        results.push(result);
    }
    let context = ReportContext {
        date: options.date.clone(),
        threads: options.threads,
        logical_processors: std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1),
        processor: std::env::var("PROCESSOR_IDENTIFIER")
            .unwrap_or_else(|_| std::env::consts::ARCH.into()),
        build: if cfg!(debug_assertions) {
            "Debug"
        } else {
            "Release"
        }
        .into(),
        revision: revision(),
        uninstalled_models,
    };
    let report = render_report(&context, &results);
    fs::create_dir_all(&options.output)
        .map_err(|_| "Der Berichtsordner konnte nicht erstellt werden.")?;
    let path = options.output.join(format!("asr-{}.md", options.date));
    let mut output = tempfile::NamedTempFile::new_in(&options.output)
        .map_err(|_| "Die Berichtsdatei konnte nicht erstellt werden.")?;
    output
        .write_all(report.as_bytes())
        .map_err(|_| "Der Bericht konnte nicht geschrieben werden.")?;
    output
        .as_file()
        .sync_all()
        .map_err(|_| "Der Bericht konnte nicht gesichert werden.")?;
    output
        .persist(path)
        .map_err(|_| "Der Bericht konnte nicht bereitgestellt werden.")?;
    println!("Benchmark abgeschlossen. Bericht: asr-{}.md", options.date);
    if options.check_protection
        && results
            .iter()
            .any(|model| !protection_checks(model).passed())
    {
        return Err(
            "Die Prüfung von Stille und Satzanfängen ist im Benchmark-Bericht ausgewertet. Mindestens ein Modell hat die Abnahme verfehlt.",
        );
    }
    Ok(())
}

fn worker(args: &[OsString]) -> Result<(), &'static str> {
    let [model, threads, corpus, output] = args else {
        return Err("Ungültiger interner Messaufruf.");
    };
    let model_id = model.to_str().ok_or("Ungültiges Modell.")?;
    let num_threads: u32 = threads
        .to_str()
        .and_then(|t| t.parse().ok())
        .ok_or("Ungültige Thread-Anzahl.")?;
    let recordings = load_corpus(Path::new(corpus))?;
    let manager = Manager::new().map_err(|_| "Der lokale Modellordner ist nicht verfügbar.")?;
    let mut engine = create_engine(
        manager,
        model_id,
        EngineConfig {
            num_threads,
            ..EngineConfig::default()
        },
    )
    .map_err(|_| "Die lokale Engine konnte nicht erstellt werden.")?;
    let result = measure(model_id, &recordings, engine.as_mut())?;
    let data = serde_json::to_vec(&result)
        .map_err(|_| "Die Messdaten konnten nicht serialisiert werden.")?;
    fs::write(Path::new(output), data)
        .map_err(|_| "Die Messdaten konnten nicht gespeichert werden.")
}

fn revision() -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| s.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(|s| format!("{s} + Arbeitskopie"))
        .unwrap_or_else(|| "unbekannt".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn protection_check_is_an_optional_flag_without_a_value() {
        for flags in [
            vec!["--check-protection"],
            vec!["--check-protection", "--threads", "2"],
            vec!["--threads", "2", "--check-protection"],
        ] {
            assert!(parse_options(&args(&flags)).unwrap().check_protection);
        }
        assert!(parse_options(&args(&["--check-protection", "--check-protection"])).is_err());
        assert!(parse_options(&args(&["--output", "--check-protection"])).is_err());
        assert!(!parse_options(&[]).unwrap().check_protection);
    }

    #[test]
    fn accepts_explicit_date_threads_and_unicode_paths() {
        let options = parse_options(&args(&[
            "--date",
            "2026-11-20",
            "--threads",
            "2",
            "--corpus",
            "Aufnahmen mit ß",
            "--output",
            "Berichte mit ß",
        ]))
        .unwrap();
        assert_eq!(options.date, "2026-11-20");
        assert_eq!(options.threads, 2);
        assert_eq!(options.corpus, PathBuf::from("Aufnahmen mit ß"));
        assert_eq!(options.output, PathBuf::from("Berichte mit ß"));
    }

    #[test]
    fn rejects_invalid_or_ambiguous_options_without_echoing_values() {
        for values in [
            vec!["--date", "2026-02-30"],
            vec!["--date", "../../private"],
            vec!["--date", "2026-1-2"],
            vec!["--threads", "0"],
            vec!["--threads", "2147483648"],
            vec!["--threads", "2", "--threads", "3"],
            vec!["--corpus"],
            vec!["--corpus", ""],
            vec!["--unknown", "private input"],
        ] {
            let error = parse_options(&args(&values)).unwrap_err();
            assert!(!error.contains("private"));
        }
    }

    #[test]
    fn defaults_use_repository_paths_and_engine_thread_count() {
        let options = parse_options(&[]).unwrap();
        assert!(options.corpus.ends_with("testdata/private/corpus"));
        assert!(options.output.ends_with("docs/benchmarks"));
        assert_eq!(
            options.threads,
            quatsch_mit_sosse_stt::EngineConfig::default().num_threads
        );
    }
}
