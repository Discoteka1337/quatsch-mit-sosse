use std::{path::Path, time::Instant};

use quatsch_mit_sosse_models::{Manager, Model, ModelLease};
use quatsch_mit_sosse_vad::{SAMPLE_RATE, SileroVad, VadConfig};
use sherpa_rs::{
    transducer::{TransducerConfig, TransducerRecognizer},
    whisper::{WhisperConfig, WhisperRecognizer},
};

use crate::{
    EngineConfig, Error, Result, SpeechEngine, Transcript, chunks::recognition_chunks,
    idle::IdleModel, recognize_chunks,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Transducer,
    Whisper,
}

fn kind(id: &str) -> Result<Kind> {
    match id {
        "parakeet-v3-int8" | "parakeet-primeline-de-int8" => Ok(Kind::Transducer),
        "whisper-large-v3-turbo" => Ok(Kind::Whisper),
        _ => Err(Error::UnsupportedModel),
    }
}

enum Recognizer {
    Transducer(Box<TransducerRecognizer>),
    Whisper(Box<WhisperRecognizer>),
}

impl Recognizer {
    fn transcribe(&mut self, samples: &[f32]) -> String {
        match self {
            Self::Transducer(recognizer) => recognizer.transcribe(16_000, samples),
            Self::Whisper(recognizer) => recognizer.transcribe(16_000, samples).text,
        }
    }
}

struct Session {
    vad: SileroVad,
    recognizer: Option<Recognizer>,
    language: String,
}

struct Engine {
    manager: Manager,
    model: Model,
    kind: Kind,
    config: EngineConfig,
    session: IdleModel<Session>,
}

impl Engine {
    fn new(manager: Manager, model_id: &str, expected: Kind, config: EngineConfig) -> Result<Self> {
        config.validate()?;
        if kind(model_id)? != expected {
            return Err(Error::UnsupportedModel);
        }
        let model = manager
            .list()
            .iter()
            .find(|m| m.id == model_id)
            .cloned()
            .ok_or(Error::UnsupportedModel)?;
        let session = IdleModel::new(config.idle_timeout).map_err(|_| Error::IdleWorker)?;
        Ok(Self {
            manager,
            model,
            kind: expected,
            config,
            session,
        })
    }

    fn transcribe(
        &self,
        samples: &[f32],
        sample_rate: u32,
        language_hint: Option<&str>,
    ) -> Result<Transcript> {
        let started = Instant::now();
        if sample_rate != SAMPLE_RATE
            || samples.len() > i32::MAX as usize
            || samples
                .iter()
                .any(|sample| !sample.is_finite() || sample.abs() > 1.0)
        {
            return Err(Error::InvalidAudio);
        }
        let language = language_hint.unwrap_or("");
        if language_hint.is_some()
            && !self
                .model
                .languages
                .iter()
                .any(|allowed| allowed == language)
        {
            return Err(Error::UnsupportedLanguage);
        }
        if samples.is_empty() {
            return Ok(Transcript {
                text: String::new(),
                processing_duration: started.elapsed(),
            });
        }
        let text = self.session.with(
            || {
                let lease = self.acquire("silero-vad")?;
                let mut vad =
                    SileroVad::new(lease.path().join("silero_vad.onnx"), VadConfig::default())
                        .map_err(|_| Error::Vad)?;
                vad.load().map_err(|_| Error::Vad)?;
                Ok(Session {
                    vad,
                    recognizer: None,
                    language: String::new(),
                })
            },
            |session| {
                let segments = session
                    .vad
                    .speech_segments(samples, sample_rate)
                    .map_err(|_| Error::Vad)?;
                let chunks = recognition_chunks(&segments, samples.len())?;
                if chunks.is_empty() {
                    return Ok(String::new());
                }
                if self.kind == Kind::Whisper && session.language != language {
                    // Drop first: avoid temporarily keeping two large model instances alive.
                    session.recognizer = None;
                }
                if session.recognizer.is_none() {
                    session.recognizer = Some(self.load_recognizer(language)?);
                    session.language = language.into();
                }

                let recognizer = session.recognizer.as_mut().ok_or(Error::Recognition)?;
                recognize_chunks(samples, &chunks, |chunk| Ok(recognizer.transcribe(chunk)))
            },
        )?;
        Ok(Transcript {
            text,
            processing_duration: started.elapsed(),
        })
    }

    fn acquire(&self, id: &str) -> Result<ModelLease> {
        self.manager
            .acquire(id)
            .map_err(|error| match error {
                quatsch_mit_sosse_models::Error::Busy => Error::ModelBusy,
                _ => Error::ModelAccess,
            })?
            .ok_or(Error::ModelNotInstalled)
    }

    fn load_recognizer(&self, language: &str) -> Result<Recognizer> {
        let lease = self.acquire(&self.model.id)?;
        let root = lease.path();

        let threads = self.config.num_threads as i32;
        let inline_encoder = if self.model.id == "parakeet-primeline-de-int8" {
            Some(crate::onnx::prepare_encoder(root)?)
        } else {
            None
        };
        let encoder = match &inline_encoder {
            Some(file) => file.to_str().ok_or(Error::InvalidPath)?.to_owned(),
            None => model_path(root, "encoder.int8.onnx")?,
        };
        match self.kind {
            Kind::Transducer => TransducerRecognizer::new(TransducerConfig {
                encoder,
                decoder: model_path(root, "decoder.int8.onnx")?,
                joiner: model_path(root, "joiner.int8.onnx")?,
                tokens: model_path(root, "tokens.txt")?,
                model_type: "nemo_transducer".into(),
                decoding_method: "greedy_search".into(),
                sample_rate: 16_000,
                feature_dim: 80,
                num_threads: threads,
                provider: Some("cpu".into()),
                debug: false,
                ..Default::default()
            })
            .map(|r| Recognizer::Transducer(Box::new(r)))
            .map_err(|_| Error::Recognition),
            Kind::Whisper => WhisperRecognizer::new(WhisperConfig {
                encoder: model_path(root, "turbo-encoder.int8.onnx")?,
                decoder: model_path(root, "turbo-decoder.int8.onnx")?,
                tokens: model_path(root, "turbo-tokens.txt")?,
                language: language.into(),
                num_threads: Some(threads),
                provider: Some("cpu".into()),
                debug: false,
                ..Default::default()
            })
            .map(|r| Recognizer::Whisper(Box::new(r)))
            .map_err(|_| Error::Recognition),
        }
    }
}

fn model_path(root: &Path, file: &str) -> Result<String> {
    root.join(file)
        .to_str()
        .filter(|path| !path.contains('\0'))
        .map(str::to_owned)
        .ok_or(Error::InvalidPath)
}

pub struct TransducerEngine(Engine);

impl TransducerEngine {
    pub fn new(model_id: &str, config: EngineConfig) -> Result<Self> {
        Self::with_manager(
            Manager::new().map_err(|_| Error::ModelAccess)?,
            model_id,
            config,
        )
    }

    pub fn with_manager(manager: Manager, model_id: &str, config: EngineConfig) -> Result<Self> {
        Engine::new(manager, model_id, Kind::Transducer, config).map(Self)
    }

    /// Returns whether the ASR model is resident. Silero has a separate lifetime.
    pub fn is_loaded(&self) -> bool {
        self.0
            .session
            .is_loaded(|session| session.recognizer.is_some())
    }
}

impl SpeechEngine for TransducerEngine {
    fn transcribe(
        &mut self,
        samples: &[f32],
        sample_rate: u32,
        language_hint: Option<&str>,
    ) -> Result<Transcript> {
        // Parakeet detects language itself; a supported hint is accepted without forcing it.
        self.0.transcribe(samples, sample_rate, language_hint)
    }
}

pub struct WhisperEngine(Engine);

impl WhisperEngine {
    pub fn new(model_id: &str, config: EngineConfig) -> Result<Self> {
        Self::with_manager(
            Manager::new().map_err(|_| Error::ModelAccess)?,
            model_id,
            config,
        )
    }

    pub fn with_manager(manager: Manager, model_id: &str, config: EngineConfig) -> Result<Self> {
        Engine::new(manager, model_id, Kind::Whisper, config).map(Self)
    }

    /// Returns whether the ASR model is resident. Silero has a separate lifetime.
    pub fn is_loaded(&self) -> bool {
        self.0
            .session
            .is_loaded(|session| session.recognizer.is_some())
    }
}

impl SpeechEngine for WhisperEngine {
    fn transcribe(
        &mut self,
        samples: &[f32],
        sample_rate: u32,
        language_hint: Option<&str>,
    ) -> Result<Transcript> {
        self.0.transcribe(samples, sample_rate, language_hint)
    }
}

pub fn create_engine(
    manager: Manager,
    model_id: &str,
    config: EngineConfig,
) -> Result<Box<dyn SpeechEngine>> {
    match kind(model_id)? {
        Kind::Transducer => Ok(Box::new(TransducerEngine::with_manager(
            manager, model_id, config,
        )?)),
        Kind::Whisper => Ok(Box::new(WhisperEngine::with_manager(
            manager, model_id, config,
        )?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn a_concurrent_install_or_removal_blocks_both_native_loaders() {
        let directory = tempfile::tempdir().unwrap();
        let manager = Manager::with_root(directory.path().into()).unwrap();
        let model = "parakeet-v3-int8";
        let engine =
            Engine::new(manager, model, Kind::Transducer, EngineConfig::default()).unwrap();
        let locks = directory.path().join(".locks");
        fs::create_dir(&locks).unwrap();
        for id in ["silero-vad", model] {
            let lock = fs::File::create(locks.join(format!("{id}.lock"))).unwrap();
            lock.try_lock().unwrap();
            let try_load = || {
                if id == model {
                    engine.load_recognizer("de").map(drop)
                } else {
                    engine
                        .transcribe(&[0.0; 512], SAMPLE_RATE, Some("de"))
                        .map(drop)
                }
            };
            assert!(matches!(try_load(), Err(Error::ModelBusy)));
            drop(lock);
            assert!(matches!(try_load(), Err(Error::ModelNotInstalled)));
        }
    }
}
