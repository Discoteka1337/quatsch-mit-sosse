use super::*;
use std::cell::{Cell, RefCell};
use tempfile::tempdir;

struct Slider {
    level: Cell<f32>,
    fail: Cell<bool>,
    writes: RefCell<Vec<f32>>,
}

impl Slider {
    fn new(level: f32) -> Self {
        Self {
            level: Cell::new(level),
            fail: Cell::new(false),
            writes: RefCell::default(),
        }
    }
}

impl Volume for Slider {
    fn level(&self) -> io::Result<f32> {
        Ok(self.level.get())
    }
    fn set(&self, level: f32) -> io::Result<()> {
        if self.fail.get() {
            return Err(io::Error::other("device unavailable"));
        }
        self.writes.borrow_mut().push(level);
        self.level.set(level);
        Ok(())
    }
}

fn mixer(levels: &[(&str, f32)]) -> Mixer<Slider> {
    let mut mixer = Mixer::default();
    for &(id, level) in levels {
        mixer.sessions.insert(
            id.into(),
            Session {
                device: "speaker".into(),
                group: id.into(),
                volume: Slider::new(level),
            },
        );
    }
    mixer
}

#[test]
fn crash_recovery_restores_each_original_level_and_never_raises_quiet_sessions() {
    let dir = tempdir().unwrap();
    let journal = Journal(dir.path().join("levels.json"));
    let mixer = mixer(&[
        ("music", 0.83),
        ("browser", 0.57),
        ("quiet", 0.09),
        ("silent", 0.0),
    ]);
    journal.lower(&mixer, 0.2).unwrap();
    assert_eq!(mixer.sessions["music"].volume.level.get(), 0.2);
    assert_eq!(mixer.sessions["browser"].volume.level.get(), 0.2);
    assert!(mixer.sessions["quiet"].volume.writes.borrow().is_empty());
    assert!(mixer.sessions["silent"].volume.writes.borrow().is_empty());

    let restarted = Journal(dir.path().join("levels.json"));
    restarted.restore(&mixer).unwrap();
    assert_eq!(mixer.sessions["music"].volume.level.get(), 0.83);
    assert_eq!(mixer.sessions["browser"].volume.level.get(), 0.57);
    assert!(!restarted.0.exists());
    restarted.restore(&mixer).unwrap();
}

#[test]
fn refresh_adds_new_sessions_without_overwriting_originals_or_manual_adjustments() {
    let dir = tempdir().unwrap();
    let journal = Journal(dir.path().join("levels.json"));
    let mut current = mixer(&[("early", 0.8), ("manual", 0.7)]);
    journal.lower(&current, 0.2).unwrap();
    current.sessions["manual"].volume.level.set(0.45);
    current.sessions.extend(mixer(&[("late", 0.9)]).sessions);
    journal.lower(&current, 0.2).unwrap();
    assert_eq!(current.sessions["manual"].volume.level.get(), 0.45);
    assert_eq!(current.sessions["late"].volume.level.get(), 0.2);
    journal.restore(&current).unwrap();
    assert_eq!(current.sessions["early"].volume.level.get(), 0.8);
    assert_eq!(current.sessions["manual"].volume.level.get(), 0.45);
    assert_eq!(current.sessions["late"].volume.level.get(), 0.9);
}

#[test]
fn partial_restoration_and_disconnected_devices_are_retried() {
    let dir = tempdir().unwrap();
    let journal = Journal(dir.path().join("levels.json"));
    let current = mixer(&[("a", 1.0), ("b", 0.8)]);
    journal.lower(&current, 0.2).unwrap();
    journal.restore(&Mixer::<Slider>::default()).unwrap();
    assert!(journal.0.exists());
    current.sessions["a"].volume.fail.set(true);
    assert!(journal.restore(&current).is_err());
    assert_eq!(current.sessions["b"].volume.level.get(), 0.8);
    assert!(journal.0.exists());
    current.sessions["a"].volume.fail.set(false);
    journal.restore(&current).unwrap();
    assert_eq!(current.sessions["a"].volume.level.get(), 1.0);
    assert!(!journal.0.exists());
}

#[test]
fn failure_to_save_or_read_journal_leaves_all_sliders_alone() {
    let dir = tempdir().unwrap();
    let mixer = mixer(&[("a", 1.0)]);
    let blocked = dir.path().join("not-a-directory");
    fs::write(&blocked, b"invalid").unwrap();
    for path in [blocked.join("levels.json"), blocked] {
        let journal = Journal(path);
        assert!(journal.lower(&mixer, 0.2).is_err());
        assert!(mixer.sessions["a"].volume.writes.borrow().is_empty());
    }
}

#[test]
fn disabled_effects_can_start_refresh_and_stop_without_an_audio_endpoint_or_journal() {
    let dir = tempdir().unwrap();
    let mut effects = Effects::new(dir.path().into());
    effects.recover().unwrap();
    assert!(!effects.needs_recovery());
    effects.begin(false, 100).unwrap();
    effects.refresh().unwrap();
    effects.end().unwrap();
    effects.end().unwrap();
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn stop_after_partial_ducking_does_not_touch_the_unmodified_session() {
    let dir = tempdir().unwrap();
    let journal = Journal(dir.path().join("levels.json"));
    let mixer = mixer(&[("a", 1.0), ("b", 0.8)]);
    mixer.sessions["b"].volume.fail.set(true);
    assert!(journal.lower(&mixer, 0.2).is_err());
    assert!(journal.0.is_file());
    journal.restore(&mixer).unwrap();
    assert_eq!(mixer.sessions["a"].volume.level.get(), 1.0);
    assert!(mixer.sessions["b"].volume.writes.borrow().is_empty());
    assert!(!journal.0.exists());
}

#[test]
fn reinstantiated_external_session_recovers_its_persisted_mixer_setting() {
    let dir = tempdir().unwrap();
    let journal = Journal(dir.path().join("levels.json"));
    let mut current = mixer(&[("old instance", 0.9)]);
    journal.lower(&current, 0.2).unwrap();
    let session = current.sessions.remove("old instance").unwrap();
    current.sessions.insert("new instance".into(), session);
    journal.restore(&current).unwrap();
    assert_eq!(current.sessions["new instance"].volume.level.get(), 0.9);
    assert!(!journal.0.exists());
}

#[test]
fn absent_session_is_kept_for_later_recovery_even_when_its_device_is_online() {
    let dir = tempdir().unwrap();
    let journal = Journal(dir.path().join("volume-recovery.json"));
    let previous = mixer(&[("radio", 0.85)]);
    journal.lower(&previous, 0.2).unwrap();
    journal.restore(&mixer(&[])).unwrap();
    assert!(journal.0.exists());
    journal.restore(&previous).unwrap();
    assert_eq!(previous.sessions["radio"].volume.level.get(), 0.85);
    assert!(!journal.0.exists());
}

#[test]
fn new_recording_with_a_different_cap_retains_the_original_for_a_returning_app() {
    let dir = tempdir().unwrap();
    let journal = Journal(dir.path().join("volume-recovery.json"));
    let mut previous = mixer(&[("old", 0.75)]);
    journal.lower(&previous, 0.2).unwrap();
    let mut current = mixer(&[("other", 0.62)]);
    journal.restore(&current).unwrap();
    journal.lower(&current, 0.1).unwrap();
    current.sessions.insert(
        "relaunched".into(),
        previous.sessions.remove("old").unwrap(),
    );
    journal.lower(&current, 0.1).unwrap();
    assert_eq!(current.sessions["relaunched"].volume.level.get(), 0.1);
    journal.restore(&current).unwrap();
    assert_eq!(current.sessions["relaunched"].volume.level.get(), 0.75);
    assert_eq!(current.sessions["other"].volume.level.get(), 0.62);
}

#[test]
fn ambiguous_instances_never_restore_another_instances_slider() {
    let dir = tempdir().unwrap();
    let journal = Journal(dir.path().join("volume-recovery.json"));
    let mut previous = mixer(&[("first", 0.85), ("second", 0.65)]);
    previous.sessions.get_mut("first").unwrap().group = "shared id".into();
    previous.sessions.get_mut("second").unwrap().group = "shared id".into();
    journal.lower(&previous, 0.2).unwrap();
    let first = previous.sessions.remove("first").unwrap();
    journal.restore(&previous).unwrap();
    assert_eq!(previous.sessions["second"].volume.level.get(), 0.65);
    journal.restore(&previous).unwrap();
    assert!(journal.0.exists());
    previous.sessions.insert("first".into(), first);
    journal.restore(&previous).unwrap();
    assert_eq!(previous.sessions["first"].volume.level.get(), 0.85);
    assert!(!journal.0.exists());
}

#[test]
fn reconnected_device_respects_the_new_cap_on_its_original_session() {
    let dir = tempdir().unwrap();
    let journal = Journal(dir.path().join("volume-recovery.json"));
    let previous = mixer(&[("same instance", 0.74)]);
    journal.lower(&previous, 0.2).unwrap();
    journal.restore(&mixer(&[])).unwrap();
    journal.lower(&previous, 0.12).unwrap();
    assert_eq!(previous.sessions["same instance"].volume.level.get(), 0.12);
    journal.restore(&previous).unwrap();
    assert_eq!(previous.sessions["same instance"].volume.level.get(), 0.74);
}

#[test]
fn cues_are_distinct_short_pcm_waves_with_silent_edges() {
    let start = tone::wave(true);
    let end = tone::wave(false);
    assert_ne!(start, end);
    for cue in [start, end] {
        assert_eq!(&cue[..4], b"RIFF");
        assert_eq!(&cue[8..16], b"WAVEfmt ");
        assert_eq!(&cue[36..40], b"data");
        let bytes = u32::from_le_bytes(cue[40..44].try_into().unwrap()) as usize;
        assert_eq!(bytes + 44, cue.len());
        assert!(bytes > 1_000 && bytes < 8_000);
        assert_eq!(&cue[44..46], &[0, 0]);
        assert_eq!(&cue[cue.len() - 2..], &[0, 0]);
    }
}

#[test]
fn core_audio_identifiers_are_stable_digests_in_the_persisted_journal() {
    let private = r"C:\Programs\Example\player.exe";
    let identity = native::opaque_id(&private.encode_utf16().collect::<Vec<_>>());
    assert_eq!(
        native::opaque_id(&[65]),
        "e61c21ca716b3b1aefb7d1198f83679c4ca4d596e5792275dd6203b49216237d"
    );
    assert_ne!(identity, native::opaque_id(&[65]));
    let dir = tempdir().unwrap();
    let journal = Journal(dir.path().join("volume-recovery.json"));
    journal.lower(&mixer(&[(&identity, 0.6)]), 0.2).unwrap();
    let saved = fs::read_to_string(&journal.0).unwrap();
    assert!(saved.contains(&identity));
    assert!(!saved.contains("player.exe"));
}
