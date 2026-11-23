use super::*;
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn blocked_disk_write_keeps_snapshots_and_later_edits_available_and_quit_drains_them() {
    let dir = tempfile::tempdir().unwrap();
    let store = SettingsStore::load(dir.path()).unwrap();
    let writer = Writer::start(store).unwrap();
    let (entered, started) = mpsc::channel();
    let (resume, blocked) = mpsc::channel();
    let (fail, failed) = mpsc::channel();
    writer
        .edit(
            |s| s.tones = false,
            move |_, _| {
                entered.send(()).unwrap();
                blocked.recv_timeout(Duration::from_secs(5)).unwrap();
                Err(io::Error::other("synthetic write failure"))
            },
            move |result| fail.send(result.is_err()).unwrap(),
        )
        .unwrap();
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(writer.committed().tones);
    assert!(!writer.display().tones);
    let (done, saved) = mpsc::channel();
    writer
        .edit(
            |s| s.volume_percent = 73,
            |store, next| store.save(next),
            move |result| {
                done.send(result.is_ok()).unwrap();
            },
        )
        .unwrap();
    assert_eq!(writer.display().volume_percent, 73);
    let (stop, stopped) = mpsc::channel();
    writer.stop_accepting();
    assert!(
        writer
            .edit(|s| s.volume_percent = 42, |_, _| Ok(()), |_| {})
            .is_err()
    );
    writer.finish(move || stop.send(()).unwrap()).unwrap();
    resume.send(()).unwrap();
    stopped.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(failed.recv().unwrap());
    assert!(saved.recv().unwrap());
    assert!(writer.committed().tones);
    assert_eq!(writer.display(), writer.committed());
    assert_eq!(
        SettingsStore::load(dir.path()).unwrap().current(),
        &writer.committed()
    );
}

#[test]
fn login_repair_failure_keeps_the_writer_available_for_other_preferences() {
    let dir = tempfile::tempdir().unwrap();
    let store = SettingsStore::load(dir.path()).unwrap();
    let writer = Writer::start(store).unwrap();
    let (warning, warned) = mpsc::channel();
    writer
        .repair_login(
            |enabled| {
                assert!(!enabled);
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "synthetic registry policy",
                ))
            },
            move || warning.send(()).unwrap(),
        )
        .unwrap();
    warned.recv_timeout(Duration::from_secs(5)).unwrap();
    let (done, saved) = mpsc::channel();
    writer
        .edit(
            |s| s.tones = false,
            |store, next| store.save(next),
            move |r| done.send(r).unwrap(),
        )
        .unwrap();
    saved.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    assert!(!writer.committed().tones);
}
