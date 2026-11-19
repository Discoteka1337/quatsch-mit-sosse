use std::{collections::VecDeque, fs, io};

use corpus_recorder::{
    CorpusStore, Interface, Key, PromptKind, Recorder, build_items, run_session,
};

fn sentences() -> String {
    (1..=40)
        .map(|id| format!("Synthetischer Prüfsatz Nummer {id} mit Umlauten äöü und ß."))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn plan_has_exact_categories_stable_ids_and_ten_empty_references() {
    let items = build_items(&sentences()).unwrap();
    assert_eq!(items.len(), 50);
    let groups = [
        (0..15, PromptKind::AiPrompt),
        (15..23, PromptKind::Everyday),
        (23..29, PromptKind::FormalEmail),
        (29..34, PromptKind::Numbers),
        (34..40, PromptKind::Corrections),
        (40..45, PromptKind::Silence),
        (45..50, PromptKind::Keyboard),
    ];
    for (range, kind) in groups {
        for index in range {
            assert_eq!(items[index].id, index + 1);
            assert_eq!(items[index].kind, kind);
            assert_eq!(items[index].reference.is_empty(), index >= 40);
        }
    }
}

#[test]
fn windows_utf8_bom_and_crlf_preserve_the_reference() {
    let input = format!("\u{feff}{}\r\n", sentences().replace('\n', "\r\n"));
    let items = build_items(&input).unwrap();
    assert_eq!(items[0].reference, sentences().lines().next().unwrap());
}

#[test]
fn malformed_lists_fail_without_echoing_content() {
    for input in [
        String::new(),
        sentences().lines().take(39).collect::<Vec<_>>().join("\n"),
        format!("{}\nZusatz", sentences()),
        sentences().replacen("Nummer 1", "\x1b[31m", 1),
        sentences().replacen(
            "Synthetischer Prüfsatz Nummer 1 mit Umlauten äöü und ß.",
            "",
            1,
        ),
    ] {
        let error = build_items(&input).err().expect("Invalid list accepted");
        assert!(!error.to_string().contains("Synthetischer"));
        assert!(!error.to_string().contains('\x1b'));
    }
}

#[test]
fn wav_and_utf8_reference_round_trip_in_path_with_spaces_and_sharp_s() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("Ordner mit Soße");
    let store = CorpusStore::new(directory.clone());
    let items = build_items(&sentences()).unwrap();
    let samples = vec![0.0, 0.25, -0.75, 1.0, -1.0];
    store.save(&items[0], &samples).unwrap();
    assert_eq!(
        fs::read_to_string(directory.join("001.txt")).unwrap(),
        items[0].reference
    );
    let mut reader = hound::WavReader::open(directory.join("001.wav")).unwrap();
    assert_eq!(reader.spec().sample_rate, 16_000);
    assert_eq!(reader.spec().channels, 1);
    assert_eq!(reader.spec().bits_per_sample, 32);
    assert_eq!(reader.spec().sample_format, hound::SampleFormat::Float);
    assert_eq!(
        reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        samples
    );
    assert!(store.contains(&items[0]).unwrap());
    store.save(&items[40], &[0.0; 16]).unwrap();
    assert!(fs::read(directory.join("041.txt")).unwrap().is_empty());
    assert!(store.contains(&items[40]).unwrap());
}

#[test]
fn existing_or_incomplete_pairs_are_never_overwritten() {
    let temp = tempfile::tempdir().unwrap();
    let store = CorpusStore::new(temp.path().to_owned());
    let items = build_items(&sentences()).unwrap();
    store.save(&items[0], &[0.5]).unwrap();
    let original = fs::read(temp.path().join("001.wav")).unwrap();
    assert!(store.save(&items[0], &[0.1]).is_err());
    assert_eq!(fs::read(temp.path().join("001.wav")).unwrap(), original);
    fs::write(temp.path().join("002.txt"), "Vorhandene Referenz").unwrap();
    assert!(store.contains(&items[1]).is_err());
    assert!(store.save(&items[1], &[0.2]).is_err());
    assert!(!temp.path().join("002.wav").exists());
    assert_eq!(
        fs::read_to_string(temp.path().join("002.txt")).unwrap(),
        "Vorhandene Referenz"
    );
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 3);
}

#[test]
fn invalid_samples_and_ids_create_no_pair() {
    let temp = tempfile::tempdir().unwrap();
    let store = CorpusStore::new(temp.path().to_owned());
    let mut items = build_items(&sentences()).unwrap();
    for samples in [vec![], vec![f32::NAN], vec![f32::INFINITY]] {
        assert!(store.save(&items[0], &samples).is_err());
    }
    items[0].id = 0;
    assert!(store.save(&items[0], &[0.0]).is_err());
    items[0].id = 1000;
    assert!(store.save(&items[0], &[0.0]).is_err());
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[derive(Default)]
struct FakeRecorder {
    calls: Vec<&'static str>,
    takes: usize,
    fail_finish: bool,
}

impl Recorder for FakeRecorder {
    fn prepare(&mut self) -> io::Result<()> {
        self.calls.push("prepare");
        Ok(())
    }
    fn start(&mut self) -> io::Result<()> {
        self.calls.push("start");
        Ok(())
    }
    fn finish(&mut self) -> io::Result<Vec<f32>> {
        self.calls.push("finish");
        self.takes += 1;
        if self.fail_finish {
            return Err(io::Error::other("Synthetische Audiounterbrechung"));
        }
        // The prefix represents audio already buffered before the start key.
        let mut samples = vec![-0.25; 8_000];
        samples.extend(vec![self.takes as f32 / 10.0; 160_000]);
        Ok(samples)
    }
}

struct FakeInterface {
    keys: VecDeque<Key>,
    output: String,
}

impl FakeInterface {
    fn new(keys: impl IntoIterator<Item = Key>) -> Self {
        Self {
            keys: keys.into_iter().collect(),
            output: String::new(),
        }
    }
}

impl Interface for FakeInterface {
    fn write(&mut self, message: &str) -> io::Result<()> {
        self.output.push_str(message);
        Ok(())
    }
    fn read_key(&mut self) -> io::Result<Key> {
        self.keys
            .pop_front()
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "Testeingabe beendet"))
    }
}

#[test]
fn enter_records_repeat_discards_first_take_then_enter_saves_with_preroll() {
    let temp = tempfile::tempdir().unwrap();
    let store = CorpusStore::new(temp.path().to_owned());
    let items = build_items(&sentences()).unwrap();
    let mut recorder = FakeRecorder::default();
    let mut ui = FakeInterface::new([
        Key::Enter,
        Key::Enter,
        Key::Repeat,
        Key::Enter,
        Key::Enter,
        Key::Enter,
    ]);
    let result = run_session(&items[..1], &mut recorder, &mut ui, &store).unwrap();
    assert_eq!(result.saved, 1);
    assert!(!result.aborted);
    assert_eq!(
        recorder.calls,
        ["prepare", "start", "finish", "prepare", "start", "finish"]
    );
    let mut reader = hound::WavReader::open(temp.path().join("001.wav")).unwrap();
    let samples = reader
        .samples::<f32>()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(samples.len(), 168_000);
    assert!(samples[..8_000].iter().all(|sample| *sample == -0.25));
    assert!(samples[8_000..].iter().all(|sample| *sample == 0.2));
}

#[test]
fn skip_before_during_and_after_recording_keeps_numbering_and_writes_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let store = CorpusStore::new(temp.path().to_owned());
    let items = build_items(&sentences()).unwrap();
    let mut recorder = FakeRecorder::default();
    let mut ui = FakeInterface::new([
        Key::Skip,
        Key::Enter,
        Key::Skip,
        Key::Enter,
        Key::Enter,
        Key::Skip,
        Key::Enter,
        Key::Enter,
        Key::Enter,
    ]);
    let result = run_session(&items[..4], &mut recorder, &mut ui, &store).unwrap();
    assert_eq!(result.skipped, 3);
    assert_eq!(result.saved, 1);
    assert!(!temp.path().join("001.wav").exists());
    assert!(!temp.path().join("002.wav").exists());
    assert!(!temp.path().join("003.wav").exists());
    assert!(temp.path().join("004.wav").exists());
}

#[test]
fn resume_skips_saved_pairs_without_opening_microphone() {
    let temp = tempfile::tempdir().unwrap();
    let store = CorpusStore::new(temp.path().to_owned());
    let items = build_items(&sentences()).unwrap();
    store.save(&items[0], &[0.0]).unwrap();
    let mut recorder = FakeRecorder::default();
    let mut ui = FakeInterface::new([]);
    let result = run_session(&items[..1], &mut recorder, &mut ui, &store).unwrap();
    assert_eq!(result.existing, 1);
    assert!(recorder.calls.is_empty());
}

#[test]
fn quit_or_input_failure_never_saves_an_unconfirmed_take() {
    for keys in [
        vec![Key::Quit],
        vec![Key::Enter, Key::Quit],
        vec![Key::Enter, Key::Enter, Key::Quit],
        vec![Key::Enter],
    ] {
        let temp = tempfile::tempdir().unwrap();
        let store = CorpusStore::new(temp.path().to_owned());
        let items = build_items(&sentences()).unwrap();
        let mut recorder = FakeRecorder::default();
        let mut ui = FakeInterface::new(keys);
        if let Ok(result) = run_session(&items[..1], &mut recorder, &mut ui, &store) {
            assert!(result.aborted);
        }
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}

#[test]
fn audio_failure_cannot_become_a_saved_take() {
    let temp = tempfile::tempdir().unwrap();
    let store = CorpusStore::new(temp.path().to_owned());
    let items = build_items(&sentences()).unwrap();
    let mut recorder = FakeRecorder {
        fail_finish: true,
        ..Default::default()
    };
    let mut ui = FakeInterface::new([Key::Enter, Key::Enter, Key::Enter]);
    assert!(run_session(&items[..1], &mut recorder, &mut ui, &store).is_err());
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn silence_and_keyboard_have_separate_prompts_and_empty_reference_files() {
    let temp = tempfile::tempdir().unwrap();
    let store = CorpusStore::new(temp.path().to_owned());
    let items = build_items(&sentences()).unwrap();
    let mut recorder = FakeRecorder::default();
    let mut ui = FakeInterface::new([Key::Enter, Key::Enter, Key::Enter].repeat(10));
    let result = run_session(&items[40..], &mut recorder, &mut ui, &store).unwrap();
    assert_eq!(result.saved, 10);
    assert!(ui.output.contains("Stille"));
    assert!(ui.output.contains("Tastaturgeräusche"));
    for id in 41..=50 {
        assert!(
            fs::read(temp.path().join(format!("{id:03}.txt")))
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn typing_w_and_s_during_keyboard_take_does_not_repeat_or_skip() {
    let temp = tempfile::tempdir().unwrap();
    let store = CorpusStore::new(temp.path().to_owned());
    let items = build_items(&sentences()).unwrap();
    let mut recorder = FakeRecorder::default();
    let mut ui = FakeInterface::new([
        Key::Enter,
        Key::Repeat,
        Key::Skip,
        Key::Other,
        Key::Enter,
        Key::Enter,
    ]);
    let result = run_session(&items[45..46], &mut recorder, &mut ui, &store).unwrap();
    assert_eq!(result.saved, 1);
    assert_eq!(recorder.takes, 1);
    assert!(temp.path().join("046.wav").exists());
}
