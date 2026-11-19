use std::process::Command;

#[test]
fn help_works_without_console_or_microphone_from_another_directory() {
    let directory = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_corpus-recorder"))
        .arg("--help")
        .current_dir(directory.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("500 ms"));
    assert!(text.contains("--device"));
    assert!(text.contains("testdata/private/corpus"));
    assert!(text.contains("W"));
    assert!(text.contains("S"));
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn unknown_arguments_fail_without_echoing_argument_content() {
    let output = Command::new(env!("CARGO_BIN_EXE_corpus-recorder"))
        .arg("--synthetisches-geheimnis")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("synthetisches-geheimnis"));
}

#[test]
fn redirected_recording_is_rejected_before_private_prompts_or_audio() {
    let output = Command::new(env!("CARGO_BIN_EXE_corpus-recorder"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Terminal"));
}
