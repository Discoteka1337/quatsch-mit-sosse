use quatsch_mit_sosse_models::{Error, Manager, ProgressPhase};
use std::io::{self, Write};
use std::process::ExitCode;
use std::sync::{Arc, atomic::AtomicBool, atomic::Ordering};
use std::time::{Duration, Instant};

const USAGE: &str = "Modell-Manager für Quatsch mit Soße\n\n\
Verwendung: models-cli list | install <id> | remove <id>\n\
Abbruch mit Strg+C; derselbe Installationsbefehl setzt den Download fort.";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("Fehler: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let arguments: Vec<String> = std::env::args_os()
        .skip(1)
        .map(std::ffi::OsString::into_string)
        .collect::<Result<_, _>>()
        .map_err(|_| USAGE.to_owned())?;
    let command: Vec<&str> = arguments.iter().map(String::as_str).collect();
    if matches!(command.as_slice(), ["--help" | "-h"]) {
        println!("{USAGE}");
        return Ok(());
    }
    if !matches!(command.as_slice(), ["list"] | ["install" | "remove", _]) {
        return Err(USAGE.to_owned());
    }
    let manager = Manager::new().map_err(|error| error.to_string())?;
    match command.as_slice() {
        ["list"] => {
            for model in manager.list() {
                let installed = manager
                    .is_installed(&model.id)
                    .map_err(|error| error.to_string())?;
                println!(
                    "{} | {} | {} | {:.1} MB | {}",
                    model.id,
                    model.display_name,
                    if installed {
                        "installiert"
                    } else {
                        "nicht installiert"
                    },
                    model.download_size() as f64 / 1_000_000.0,
                    model.license,
                );
            }
        }
        ["install", id] => {
            let cancel = Arc::new(AtomicBool::new(false));
            let signal = Arc::clone(&cancel);
            ctrlc::set_handler(move || signal.store(true, Ordering::Relaxed)).map_err(|_| {
                "Die Abbruchbehandlung konnte nicht eingerichtet werden.".to_owned()
            })?;
            let mut last_update = Instant::now() - Duration::from_secs(1);
            let mut last_phase = None;
            let result = manager.install(id, &cancel, |progress| {
                let phase = match progress.phase {
                    ProgressPhase::Downloading => "Download",
                    ProgressPhase::Verifying => "Prüfsumme prüfen",
                    ProgressPhase::Extracting => "Dateien bereitstellen",
                    ProgressPhase::Complete => "Fertig",
                };
                if last_phase != Some(phase) || last_update.elapsed() >= Duration::from_millis(250)
                {
                    eprint!(
                        "\r{phase}: {:.1} / {:.1} MB                    ",
                        progress.downloaded_bytes as f64 / 1_000_000.0,
                        progress.total_bytes as f64 / 1_000_000.0,
                    );
                    let _ = io::stderr().flush();
                    last_update = Instant::now();
                    last_phase = Some(phase);
                }
            });
            eprintln!();
            match result {
                Ok(path) => println!("Installiert: {}", path.display()),
                Err(Error::Cancelled) => {
                    return Err(
                        "Abgebrochen. Derselbe Installationsbefehl setzt den Download fort."
                            .to_owned(),
                    );
                }
                Err(error) => return Err(error.to_string()),
            }
        }
        ["remove", id] => {
            manager.remove(id).map_err(|error| error.to_string())?;
            println!("Entfernt: {id}");
        }
        _ => unreachable!(),
    }
    Ok(())
}
