#[cfg(any(windows, test))]
fn test_sentence(args: &[std::ffi::OsString]) -> Result<&'static str, &'static str> {
    match args {
        [] => Ok("Grüße aus Überlingen – 3,5 %"),
        [flag] if flag == "--matrix" => Ok("Äpfel, Öl & Übermaß – 3,5 % für 12,99 €"),
        _ => Err("Aufruf: inject_test [--matrix]"),
    }
}

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    use quatsch_mit_sosse_inject::{InjectConfig, insert};
    use std::{process::ExitCode, thread, time::Duration};

    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let sentence = match test_sentence(&args) {
        Ok(sentence) => sentence,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    let result = InjectConfig::load().and_then(|config| {
        println!("In drei Sekunden wird eingefügt. Jetzt das gewünschte Textfeld fokussieren.");
        thread::sleep(Duration::from_secs(3));
        insert(sentence, &config)
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

#[cfg(test)]
mod tests {
    use super::test_sentence;

    #[test]
    fn default_keeps_the_original_example() {
        assert_eq!(test_sentence(&[]), Ok("Grüße aus Überlingen – 3,5 %"));
    }

    #[test]
    fn matrix_selects_the_exact_reference_sentence() {
        assert_eq!(
            test_sentence(&["--matrix".into()]),
            Ok("Äpfel, Öl & Übermaß – 3,5 % für 12,99 €")
        );
    }

    #[test]
    fn unknown_option_returns_an_error() {
        assert!(test_sentence(&["--matrx".into()]).is_err());
    }
}
