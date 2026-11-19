use std::{path::Path, time::Instant};

use quatsch_mit_sosse_models::{Manager, Model};
use quatsch_mit_sosse_vad::{SileroVad, VadConfig};
use sherpa_rs::{
    transducer::{TransducerConfig, TransducerRecognizer},
    whisper::{WhisperConfig, WhisperRecognizer},
};

use crate::{
    EngineConfig, Error, Result, SpeechEngine, Transcript, idle::IdleModel, recognize_chunks,
    split_at_pauses,
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
        if sample_rate != 16_000
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
                let vad_root = self
                    .manager
                    .path_of("silero-vad")
                    .map_err(|_| Error::ModelAccess)?
                    .ok_or(Error::ModelNotInstalled)?;
                let vad = SileroVad::new(vad_root.join("silero_vad.onnx"), VadConfig::default())
                    .map_err(|_| Error::Vad)?;
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
                let chunks = split_at_pauses(&segments, samples.len(), sample_rate)?;
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

    fn load_recognizer(&self, language: &str) -> Result<Recognizer> {
        let root = self
            .manager
            .path_of(&self.model.id)
            .map_err(|_| Error::ModelAccess)?
            .ok_or(Error::ModelNotInstalled)?;
        let threads = self.config.num_threads as i32;
        match self.kind {
            Kind::Transducer => TransducerRecognizer::new(TransducerConfig {
                encoder: model_path(&root, "encoder.int8.onnx")?,
                decoder: model_path(&root, "decoder.int8.onnx")?,
                joiner: model_path(&root, "joiner.int8.onnx")?,
                tokens: model_path(&root, "tokens.txt")?,
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
                encoder: model_path(&root, "turbo-encoder.int8.onnx")?,
                decoder: model_path(&root, "turbo-decoder.int8.onnx")?,
                tokens: model_path(&root, "turbo-tokens.txt")?,
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

    pub fn is_loaded(&self) -> bool {
        self.0.session.is_loaded()
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

    pub fn is_loaded(&self) -> bool {
        self.0.session.is_loaded()
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
