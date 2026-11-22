use crate::{
    corpus::Recording,
    metrics::{WordErrors, compare_words, word_errors},
};
use quatsch_mit_sosse_stt::SpeechEngine;
use quatsch_mit_sosse_textfx::{Pipeline, software_dictation_pipeline};
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleMetrics {
    pub id: u16,
    pub audio_seconds: f64,
    pub latency_seconds: f64,
    pub reference_words: usize,
    pub substitutions: usize,
    pub deletions: usize,
    pub insertions: usize,
    pub silence: bool,
    pub phantom: bool,
    pub first_word_matches: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Probe {
    pub id: u16,
    pub seconds: [f64; 3],
    pub empty_outputs: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelResult {
    pub model_id: String,
    pub cold_id: u16,
    pub cold_seconds: f64,
    pub warmup_attempts: usize,
    pub warmup_succeeded: bool,
    pub peak_ram_bytes: u64,
    pub samples: Vec<SampleMetrics>,
    pub probes: Vec<Probe>,
    pub cleanup: CleanupSummary,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CleanupSummary {
    pub speech_errors: WordErrors,
    pub improved: Vec<u16>,
    pub worsened: Vec<u16>,
    pub phantoms: usize,
    pub speech_cleanup_seconds: f64,
}

impl CleanupSummary {
    fn score(
        &mut self,
        pipeline: &Pipeline,
        recording: &Recording,
        raw: &str,
        raw_errors: WordErrors,
    ) {
        let started = Instant::now();
        let cleaned = pipeline.apply(raw);
        let elapsed = started.elapsed().as_secs_f64();
        if recording.reference.trim().is_empty() {
            self.phantoms += usize::from(!cleaned.is_empty());
            return;
        }
        self.speech_cleanup_seconds += elapsed;
        let errors = word_errors(&recording.reference, &cleaned);
        self.speech_errors.reference_words += errors.reference_words;
        self.speech_errors.substitutions += errors.substitutions;
        self.speech_errors.deletions += errors.deletions;
        self.speech_errors.insertions += errors.insertions;
        match errors.total().cmp(&raw_errors.total()) {
            std::cmp::Ordering::Less => self.improved.push(recording.id),
            std::cmp::Ordering::Greater => self.worsened.push(recording.id),
            std::cmp::Ordering::Equal => {}
        }
    }
}

pub fn measure(
    model_id: &str,
    recordings: &[Recording],
    engine: &mut dyn SpeechEngine,
) -> Result<ModelResult, &'static str> {
    let first = recordings
        .iter()
        .find(|recording| !recording.reference.trim().is_empty())
        .ok_or("Der Korpus enthält keine Sprachreferenz für den Kaltstart.")?;
    let started = Instant::now();
    let warmup = engine
        .transcribe(&first.samples, 16_000, Some("de"))
        .map_err(|_| "Die lokale Erkennung ist beim Kaltstart fehlgeschlagen.")?;
    let cold_seconds = started.elapsed().as_secs_f64();
    let mut warmup_succeeded = !warmup.text.trim().is_empty();
    let mut warmup_attempts = 1;
    drop(warmup);
    for recording in recordings
        .iter()
        .filter(|recording| !recording.reference.trim().is_empty())
        .skip(1)
    {
        if warmup_succeeded {
            break;
        }
        let transcript = engine
            .transcribe(&recording.samples, 16_000, Some("de"))
            .map_err(|_| "Die lokale Erkennung ist beim Aufwärmen fehlgeschlagen.")?;
        warmup_attempts += 1;
        warmup_succeeded = !transcript.text.trim().is_empty();
    }
    let mut samples = Vec::with_capacity(recordings.len());
    let pipeline = software_dictation_pipeline();
    let mut cleanup = CleanupSummary::default();
    for recording in recordings {
        let started = Instant::now();
        let transcript = engine
            .transcribe(&recording.samples, 16_000, Some("de"))
            .map_err(|_| "Die lokale Erkennung einer Korpusaufnahme ist fehlgeschlagen.")?;
        let latency_seconds = started.elapsed().as_secs_f64();
        let (errors, first_word_matches) = compare_words(&recording.reference, &transcript.text);
        cleanup.score(&pipeline, recording, &transcript.text, errors);
        let silence = recording.reference.trim().is_empty();
        samples.push(SampleMetrics {
            id: recording.id,
            audio_seconds: recording.samples.len() as f64 / 16_000.0,
            latency_seconds,
            reference_words: errors.reference_words,
            substitutions: errors.substitutions,
            deletions: errors.deletions,
            insertions: errors.insertions,
            silence,
            phantom: silence && !transcript.text.is_empty(),
            first_word_matches,
        });
    }
    let mut probes = Vec::new();
    for recording in recordings.iter().filter(|recording| {
        !recording.reference.trim().is_empty() && recording.samples.len() >= 160_000
    }) {
        let start = (recording.samples.len() - 160_000) / 2;
        let clip = &recording.samples[start..start + 160_000];
        let mut seconds = [0.0; 3];
        let mut empty_outputs = 0;
        for time in &mut seconds {
            let started = Instant::now();
            let transcript = engine.transcribe(clip, 16_000, Some("de")).map_err(
                |_| "Die lokale Erkennung eines Zehnsekunden-Ausschnitts ist fehlgeschlagen.",
            )?;
            *time = started.elapsed().as_secs_f64();
            empty_outputs += usize::from(transcript.text.trim().is_empty());
        }
        probes.push(Probe {
            id: recording.id,
            seconds,
            empty_outputs,
        });
    }
    Ok(ModelResult {
        model_id: model_id.to_owned(),
        cold_id: first.id,
        cold_seconds,
        warmup_attempts,
        warmup_succeeded,
        peak_ram_bytes: peak_ram_bytes()?,
        samples,
        probes,
        cleanup,
    })
}

#[cfg(windows)]
pub fn peak_ram_bytes() -> Result<u64, &'static str> {
    use windows::Win32::System::{
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::GetCurrentProcess,
    };
    let mut counters = PROCESS_MEMORY_COUNTERS::default();
    let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    counters.cb = size;
    // Windows maintains this high-water mark from process creation, including model loading.
    unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, size) }
        .map_err(|_| "Der Prozess-Spitzenspeicher konnte nicht gelesen werden.")?;
    Ok(counters.PeakWorkingSetSize as u64)
}

#[cfg(not(windows))]
pub fn peak_ram_bytes() -> Result<u64, &'static str> {
    Err("Die Spitzen-RAM-Messung ist nur unter Windows vorgesehen.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use quatsch_mit_sosse_stt::{Error, Transcript};
    use std::time::Duration;

    struct FakeEngine {
        calls: Vec<usize>,
        fail: bool,
    }
    impl SpeechEngine for FakeEngine {
        fn transcribe(
            &mut self,
            samples: &[f32],
            rate: u32,
            hint: Option<&str>,
        ) -> quatsch_mit_sosse_stt::Result<Transcript> {
            assert_eq!(rate, 16_000);
            assert_eq!(hint, Some("de"));
            self.calls.push(samples.len());
            if self.fail {
                return Err(Error::Recognition);
            }
            Ok(Transcript {
                text: if samples[0] == 0.0 {
                    "!".into()
                } else {
                    "Grüße, Straße!".into()
                },
                processing_duration: Duration::ZERO,
            })
        }
    }

    #[test]
    fn measures_every_pair_and_real_ten_second_probes_after_speech_warmup() {
        let recordings = vec![
            Recording {
                id: 1,
                samples: vec![0.0; 16_000],
                reference: String::new(),
            },
            Recording {
                id: 2,
                samples: vec![0.1; 160_000],
                reference: "GRÜẞE Straße".into(),
            },
            Recording {
                id: 3,
                samples: vec![0.1; 32_000],
                reference: "Grüße".into(),
            },
        ];
        let mut engine = FakeEngine {
            calls: Vec::new(),
            fail: false,
        };
        let result = measure("test-model", &recordings, &mut engine).unwrap();
        assert_eq!(
            engine.calls,
            [160_000, 16_000, 160_000, 32_000, 160_000, 160_000, 160_000]
        );
        assert_eq!(result.cold_id, 2);
        assert_eq!(result.samples.len(), 3);
        assert!(result.samples[0].silence && result.samples[0].phantom);
        assert_eq!(result.samples[1].substitutions, 0);
        assert_eq!(result.samples[1].reference_words, 2);
        assert_eq!(result.samples[2].insertions, 1);
        assert_eq!(result.probes.len(), 1);
        assert_eq!(result.probes[0].id, 2);
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(!serialized.contains("Straße"));
        assert!(!serialized.contains("GRÜẞE"));
    }

    #[test]
    fn errors_never_return_partial_or_private_results() {
        let mut engine = FakeEngine {
            calls: Vec::new(),
            fail: true,
        };
        let speech = [Recording {
            id: 1,
            samples: vec![0.1; 160_000],
            reference: "Synthetic private reference".into(),
        }];
        assert!(measure("test-model", &speech, &mut engine).is_err());
        assert!(measure("test-model", &[], &mut engine).is_err());
        let silence = [Recording {
            id: 1,
            samples: vec![0.0; 160_000],
            reference: String::new(),
        }];
        assert!(measure("test-model", &silence, &mut engine).is_err());
    }

    #[test]
    fn empty_recognition_is_measured_as_deletions_instead_of_aborting() {
        struct EmptyEngine;
        impl SpeechEngine for EmptyEngine {
            fn transcribe(
                &mut self,
                _: &[f32],
                _: u32,
                _: Option<&str>,
            ) -> quatsch_mit_sosse_stt::Result<Transcript> {
                Ok(Transcript {
                    text: String::new(),
                    processing_duration: Duration::ZERO,
                })
            }
        }
        let recordings = [Recording {
            id: 1,
            samples: vec![0.0; 160_000],
            reference: "eins zwei".into(),
        }];
        let result = measure("empty-model", &recordings, &mut EmptyEngine).unwrap();
        assert_eq!(result.samples[0].deletions, 2);
        assert_eq!(result.probes[0].empty_outputs, 3);
        assert!(crate::report::select_models(&[result]).0.is_none());
    }

    #[test]
    #[cfg(windows)]
    fn reads_process_lifetime_peak_memory() {
        assert!(peak_ram_bytes().unwrap() > 0);
    }
}
