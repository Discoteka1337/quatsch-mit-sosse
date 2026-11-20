use bench::{
    corpus::Recording,
    measurement::measure,
    report::{ReportContext, protection_checks, render_report},
};
use quatsch_mit_sosse_stt::{SpeechEngine, Transcript};
use std::time::Duration;

struct FixtureEngine<'a> {
    silence_ids: &'a [u16],
    overrides: &'a [(u16, &'a str)],
    calls: usize,
}

impl SpeechEngine for FixtureEngine<'_> {
    fn transcribe(
        &mut self,
        samples: &[f32],
        _: u32,
        _: Option<&str>,
    ) -> quatsch_mit_sosse_stt::Result<Transcript> {
        self.calls += 1;
        let id = samples.len() as u16;
        let text = match self.overrides.iter().find(|(number, _)| *number == id) {
            Some((_, text)) => *text,
            None if self.silence_ids.contains(&id) => "",
            None => "„A\u{308}PFEL“ genügen.",
        };
        Ok(Transcript {
            text: text.into(),
            processing_duration: Duration::ZERO,
        })
    }
}

fn benchmark_report(
    speech_ids: &[u16],
    silence_ids: &[u16],
    overrides: &[(u16, &str)],
    expected_pass: bool,
) -> String {
    let recordings: Vec<_> = silence_ids
        .iter()
        .chain(speech_ids)
        .map(|&id| Recording {
            id,
            samples: vec![0.0; id.into()],
            reference: if silence_ids.contains(&id) {
                String::new()
            } else {
                "Äpfel sind synthetische Testdaten.".into()
            },
        })
        .collect();
    let mut engine = FixtureEngine {
        silence_ids,
        overrides,
        calls: 0,
    };
    let model = measure("testmodell", &recordings, &mut engine).unwrap();
    assert_eq!(engine.calls, model.warmup_attempts + recordings.len());
    assert_eq!(protection_checks(&model).passed(), expected_pass);
    let context = ReportContext {
        date: "2026-11-20".into(),
        threads: 2,
        logical_processors: 1,
        processor: "Testprozessor".into(),
        build: "Test".into(),
        revision: "abc".into(),
        uninstalled_models: vec![],
    };
    let report = render_report(&context, &[model]);
    assert!(!report.contains("Testdaten") && !report.contains("genügen"));
    report
}

#[test]
fn counts_exactly_twenty_by_id_and_tests_every_silence_for_exact_emptiness() {
    let speech: Vec<_> = (1..=23).rev().collect();
    let report = benchmark_report(
        &speech,
        &[50, 41, 45],
        &[
            (1, "Vorspann Äpfel"),
            (2, ""),
            (5, "Apfel"),
            (12, "…"),
            (21, "anderer Anfang"),
            (45, "\t"),
            (50, "!"),
        ],
        false,
    );
    assert!(report.contains("## Schritt 1.6"));
    assert!(report.contains(
        "| testmodell | 2 / 3 | 045, 050 | nicht bestanden | 16 / 20 | 001, 002, 005, 012 | nicht bestanden |"
    ));
}

#[test]
fn requires_twenty_matching_beginnings_and_at_least_one_silence() {
    let speech: Vec<_> = (1..=20).collect();
    let complete = benchmark_report(&speech, &[45], &[], true);
    assert!(complete.contains("| testmodell | 0 / 1 | — | bestanden | 20 / 20 | — | bestanden |"));

    let partial = benchmark_report(&speech[..19], &[45], &[], false);
    assert!(
        partial.contains("| testmodell | 0 / 1 | — | bestanden | 19 / 19 | — | unvollständig |")
    );

    let no_silence = benchmark_report(&speech, &[], &[], false);
    assert!(
        no_silence.contains("| testmodell | 0 / 0 | — | unvollständig | 20 / 20 | — | bestanden |")
    );
}
