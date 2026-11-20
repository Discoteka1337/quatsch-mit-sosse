#[cfg(windows)]
use quatsch_mit_sosse_hotkey::{HotkeyConfig, HotkeyEvent, HotkeyHook, HotkeyMode};
use std::process::ExitCode;

#[cfg(windows)]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let mut config = HotkeyConfig::default();
    let usage =
        "Aufruf: hotkey_test [halten|umschalten] [Diktatkürzel] [Kürzel für erneutes Einfügen]";
    if let Some(mode) = args.next() {
        config.mode = match mode.to_str() {
            Some("halten") => HotkeyMode::Hold,
            Some("umschalten") => HotkeyMode::Toggle,
            Some("--help" | "-h") => {
                println!("{usage}");
                return Ok(());
            }
            _ => return Err(usage.into()),
        };
    }
    for binding in [&mut config.dictation, &mut config.reinsert] {
        if let Some(value) = args.next() {
            *binding = value.to_str().ok_or(usage)?.parse()?;
        }
    }
    if args.next().is_some() {
        return Err(usage.into());
    }
    let (_hook, events) = HotkeyHook::start(config)?;
    let mode = match config.mode {
        HotkeyMode::Hold => "halten",
        HotkeyMode::Toggle => "umschalten",
    };
    println!(
        "Diktat ({mode}): {}. Erneut einfügen: {}.",
        config.dictation, config.reinsert
    );
    for binding in [config.dictation, config.reinsert] {
        for warning in binding.warnings() {
            println!("Hinweis zu {binding}: {warning}");
        }
    }
    println!("Esc bricht das Diktat ab. Strg+C im Konsolenfenster beendet das Beispiel.");
    for event in events {
        println!(
            "{}",
            match event {
                HotkeyEvent::Start => "Start",
                HotkeyEvent::Stop => "Stopp",
                HotkeyEvent::Cancel => "Abbruch",
                HotkeyEvent::ReinsertLast => "Letzten Text erneut einfügen",
                HotkeyEvent::ElevatedTarget =>
                    "ElevatedTarget – Das Vordergrundfenster hat höhere Rechte.",
            }
        );
    }
    Err("Der Hotkey-Thread wurde beendet.".into())
}

#[cfg(windows)]
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(windows))]
fn main() -> ExitCode {
    eprintln!("Das Hotkey-Beispiel benötigt Windows.");
    ExitCode::FAILURE
}
