use std::process::{Command, Output};

fn run(arguments: &[&str]) -> (tempfile::TempDir, Output) {
    let local_data = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_models-cli"))
        .args(arguments)
        .env("LOCALAPPDATA", local_data.path())
        .output()
        .unwrap();
    (local_data, output)
}

#[test]
fn list_shows_all_models_without_creating_downloads() {
    let (local_data, output) = run(&["list"]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    for id in [
        "parakeet-v3-int8",
        "parakeet-primeline-de-int8",
        "whisper-large-v3-turbo",
        "silero-vad",
    ] {
        assert!(text.contains(id), "{id} fehlt in {text}");
    }
    assert!(text.contains("CC-BY-4.0"));
    assert!(!local_data.path().join("QuatschMitSosse/models").exists());
}

#[test]
fn invalid_commands_fail_without_creating_downloads() {
    for arguments in [
        vec![],
        vec!["install"],
        vec!["remove"],
        vec!["list", "extra"],
        vec!["unbekannt"],
        vec!["install", "../fremd"],
        vec!["remove", "nicht-vorhanden"],
    ] {
        let (local_data, output) = run(&arguments);
        assert!(!output.status.success(), "{arguments:?}");
        assert!(!output.stderr.is_empty());
        assert!(!local_data.path().join("QuatschMitSosse/models").exists());
    }
}

#[test]
fn help_is_available_without_local_app_data() {
    let output = Command::new(env!("CARGO_BIN_EXE_models-cli"))
        .arg("--help")
        .env_remove("LOCALAPPDATA")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("install <id>")
    );
}
