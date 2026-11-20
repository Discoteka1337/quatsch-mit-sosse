#[cfg(windows)]
fn main() -> std::process::ExitCode {
    use quatsch_mit_sosse_inject::{InjectConfig, insert};
    use std::{process::ExitCode, thread, time::Duration};

    let result = InjectConfig::load().and_then(|config| {
        println!("In drei Sekunden wird eingefügt. Jetzt das gewünschte Textfeld fokussieren.");
        thread::sleep(Duration::from_secs(3));
        insert("Grüße aus Überlingen – 3,5 %", &config)
    });
    match result {
        Ok(method) => {
            println!("Verwendeter Einfügeweg: {method}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("Einfügen fehlgeschlagen: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Dieses Beispiel benötigt Windows.");
}
