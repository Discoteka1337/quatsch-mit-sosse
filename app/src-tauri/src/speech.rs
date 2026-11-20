use quatsch_mit_sosse_audio::{AudioCapture, AudioError, AudioEvent, DeviceSelection, SAMPLE_RATE};
use quatsch_mit_sosse_models::Manager;
use quatsch_mit_sosse_stt::{EngineConfig, Error as SttError, SpeechEngine, create_engine};
use std::cell::OnceCell;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use unicode_segmentation::UnicodeSegmentation;

const PRIMELINE_MODEL: &str = "parakeet-primeline-de-int8";
pub(super) const DEFAULT_MODEL: &str = PRIMELINE_MODEL;
pub(super) const FALLBACK_MODEL: &str = "parakeet-v3-int8";
pub(super) const WHISPER_MODEL: &str = "whisper-large-v3-turbo";
pub(super) const VAD_MODEL: &str = "silero-vad";

fn select_asr(
    preferred: Option<&str>,
    mut installed: impl FnMut(&str) -> Result<bool, String>,
) -> Result<Option<&'static str>, String> {
    if let Some(preferred) = preferred {
        let id = asr_id(preferred).ok_or_else(|| SttError::UnsupportedModel.to_string())?;
        return Ok(installed(id)?.then_some(id));
    }
    for id in [DEFAULT_MODEL, FALLBACK_MODEL] {
        if installed(id)? {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

pub(super) fn asr_id(id: &str) -> Option<&'static str> {
    [DEFAULT_MODEL, FALLBACK_MODEL, WHISPER_MODEL]
        .into_iter()
        .find(|model| *model == id)
}

pub(super) fn installed_asr(manager: &Manager) -> Result<Option<&'static str>, String> {
    select_asr(None, |id| {
        manager.is_installed(id).map_err(|error| error.to_string())
    })
}

pub(super) fn installed_model(
    manager: &Manager,
    preferred: Option<&str>,
) -> Result<Option<&'static str>, String> {
    if manager
        .is_installed(VAD_MODEL)
        .map_err(|error| error.to_string())?
    {
        select_asr(preferred, |id| {
            manager.is_installed(id).map_err(|error| error.to_string())
        })
    } else {
        Ok(None)
    }
}

pub(super) struct MicrophoneFault {
    pub(super) message: String,
    generation: u64,
}

struct Health {
    generation: u64,
    device_error: Option<String>,
    recording: bool,
    recording_error: Option<String>,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            generation: 0,
            device_error: Some(AudioError::NoInputDevice.to_string()),
            recording: false,
            recording_error: None,
        }
    }
}

impl Health {
    fn advance(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    fn begin(&mut self) -> Result<(), String> {
        if self.recording {
            return Err(AudioError::AlreadyRecording.to_string());
        }
        if let Some(reason) = &self.device_error {
            return Err(reason.clone());
        }
        self.advance();
        self.recording = true;
        Ok(())
    }

    fn finish(&mut self) -> Result<(), String> {
        if !self.recording {
            return Err(AudioError::NotRecording.to_string());
        }
        self.advance();
        self.recording = false;
        match self.recording_error.take() {
            Some(reason) => Err(reason),
            None => Ok(()),
        }
    }

    fn listening(&self) -> bool {
        self.recording && self.recording_error.is_none()
    }

    fn current_fault(&self, fault: &MicrophoneFault) -> bool {
        self.generation == fault.generation
    }

    fn event(&mut self, event: AudioEvent) -> Option<MicrophoneFault> {
        let message = match event {
            AudioEvent::DeviceChanged { .. } => {
                self.device_error = None;
                if !self.recording {
                    self.advance();
                }
                return None;
            }
            AudioEvent::Stopped => {
                self.advance();
                self.device_error = Some(AudioError::NotReady.to_string());
                self.recording = false;
                self.recording_error = None;
                return None;
            }
            AudioEvent::RealtimeUnavailable => return None,
            AudioEvent::DeviceLost { error, .. } | AudioEvent::DeviceUnavailable { error } => {
                let reason = error.to_string();
                self.device_error = Some(reason.clone());
                reason
            }
            AudioEvent::InputOverflow => crate::messages().audio_overflow.clone(),
            AudioEvent::InputDiscontinuity => AudioError::StreamGlitch.to_string(),
        };
        if self.recording {
            if self.recording_error.is_some() {
                return None;
            }
            self.recording_error = Some(message.clone());
        }
        self.advance();
        Some(MicrophoneFault {
            message,
            generation: self.generation,
        })
    }
}

fn locked_health(health: &Mutex<Health>) -> MutexGuard<'_, Health> {
    health.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(super) struct Microphone {
    capture: Option<AudioCapture>,
    health: Arc<Mutex<Health>>,
    on_level: Arc<dyn Fn(u8) + Send + Sync>,
    on_fault: Arc<dyn Fn(MicrophoneFault) + Send + Sync>,
}

impl Microphone {
    pub(super) fn new(
        on_level: impl Fn(u8) + Send + Sync + 'static,
        on_fault: impl Fn(MicrophoneFault) + Send + Sync + 'static,
    ) -> Self {
        Self {
            capture: None,
            health: Arc::default(),
            on_level: Arc::new(on_level),
            on_fault: Arc::new(on_fault),
        }
    }

    fn open(&mut self, device: DeviceSelection) -> Result<(), String> {
        if self.capture.is_none() {
            let level_health = Arc::clone(&self.health);
            let event_health = Arc::clone(&self.health);
            let on_level = Arc::clone(&self.on_level);
            let on_fault = Arc::clone(&self.on_fault);
            self.capture = Some(
                AudioCapture::start(
                    device,
                    move |level| {
                        if locked_health(&level_health).listening() {
                            on_level((level * 100.0).round() as u8);
                        }
                    },
                    move |event| {
                        let fault = locked_health(&event_health).event(event);
                        if let Some(fault) = fault {
                            on_fault(fault);
                        }
                    },
                )
                .map_err(|error| error.to_string())?,
            );
        }
        Ok(())
    }

    pub(super) fn begin(&mut self, name: Option<String>) -> Result<(), String> {
        self.open(name.map_or(DeviceSelection::Default, DeviceSelection::Named))?;
        locked_health(&self.health).begin()?;
        let capture = self.capture.as_ref().expect("open creates the capture");
        capture.start_recording().map_err(|error| error.to_string())
    }

    pub(super) fn finish(&mut self) -> Result<Vec<f32>, String> {
        let capture = self
            .capture
            .as_ref()
            .ok_or_else(|| AudioError::NotReady.to_string())?;
        // The audio worker reports pending driver faults before replying to finish_recording.
        let recording = capture.finish_recording();
        let health = locked_health(&self.health).finish();
        let recording = recording.map_err(|error| error.to_string())?;
        health?;
        if recording.samples().is_empty() {
            return Err(crate::messages().audio_empty.clone());
        }
        Ok(recording.into_samples())
    }

    pub(super) fn current_fault(&self, fault: &MicrophoneFault) -> bool {
        locked_health(&self.health).current_fault(fault)
    }

    pub(super) fn suspend(&mut self) -> Result<(), String> {
        let result = self
            .capture
            .take()
            .map(|mut capture| capture.stop_ready().map(drop));
        locked_health(&self.health).event(AudioEvent::Stopped);
        result.unwrap_or(Ok(())).map_err(|error| error.to_string())
    }
}

#[derive(Default)]
pub(super) struct Recognizer {
    engine: Option<(&'static str, Box<dyn SpeechEngine>)>,
}

impl Recognizer {
    pub(super) fn remove_model(&mut self, manager: &Manager, id: &str) -> Result<(), String> {
        if self
            .engine
            .as_ref()
            .is_some_and(|(loaded, _)| *loaded == id)
        {
            // The engine may still own a shared file lease during its idle timeout.
            self.engine = None;
        }
        manager.remove(id).map_err(|e| e.to_string())
    }

    pub(super) fn transcribe(
        &mut self,
        model: &'static str,
        samples: &[f32],
        on_fallback: impl FnMut(String),
    ) -> Result<String, String> {
        let manager = OnceCell::new();
        transcribe_with_fallback(
            model,
            samples,
            |id, audio| self.recognize(id, audio),
            |id| {
                let manager = manager
                    .get_or_init(Manager::new)
                    .as_ref()
                    .map_err(ToString::to_string)?;
                manager.is_installed(id).map_err(|error| error.to_string())
            },
            on_fallback,
        )
    }

    fn recognize(&mut self, model: &'static str, samples: &[f32]) -> Result<String, SttError> {
        if !matches!(&self.engine, Some((id, _)) if *id == model) {
            self.engine = None;
            let manager = Manager::new().map_err(|_| SttError::ModelAccess)?;
            let engine = create_engine(manager, model, EngineConfig::default())?;
            self.engine = Some((model, engine));
        }
        let (_, engine) = self.engine.as_mut().expect("engine created above");
        // SpeechEngine runs Silero first and loads ASR only when it finds speech.
        let text = engine.transcribe(samples, SAMPLE_RATE, Some("de"))?.text;
        Ok(if model == PRIMELINE_MODEL && text.contains("ss") {
            restore_primeline_sharp_s(&text)
        } else {
            text
        })
    }
}

// Keep these whole-word spellings tied to observed German dictation.
// Ambiguous words such as "Masse" and "Busse" require context.
fn restore_primeline_sharp_s(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for token in text.split_inclusive(char::is_whitespace) {
        // Preserve technical spellings in email addresses, URLs and paths.
        if !token.contains("ss") || token.contains(['@', '/', '\\']) {
            result.push_str(token);
            continue;
        }
        result.extend(token.split_word_bounds().map(|word| match word {
            "Übermass" => "Übermaß",
            "übermass" => "übermaß",
            "Grüsse" => "Grüße",
            "grüsse" => "grüße",
            "Strasse" => "Straße",
            "strasse" => "straße",
            "Draussen" => "Draußen",
            "draussen" => "draußen",
            "Grossen" => "Großen",
            "grossen" => "großen",
            "Fuss" => "Fuß",
            "fuss" => "fuß",
            _ => word,
        }));
    }
    result
}

fn recognition_message(error: &SttError) -> String {
    match error {
        SttError::ModelNotInstalled => crate::messages().recognizer_model_missing.clone(),
        other => other.to_string(),
    }
}

fn transcribe_with_fallback(
    model: &'static str,
    samples: &[f32],
    mut recognize: impl FnMut(&'static str, &[f32]) -> Result<String, SttError>,
    mut installed: impl FnMut(&str) -> Result<bool, String>,
    mut on_fallback: impl FnMut(String),
) -> Result<String, String> {
    let error = match recognize(model, samples) {
        Ok(text) => return Ok(text),
        Err(error) => error,
    };
    let reason = recognition_message(&error);
    if model != DEFAULT_MODEL
        || !matches!(
            error,
            SttError::Recognition | SttError::InvalidPath | SttError::ModelNotInstalled
        )
    {
        return Err(reason);
    }
    let mut available = |id| installed(id).map_err(|message| format!("{reason} {message}"));
    // ModelNotInstalled is shared by the VAD and ASR loaders. Retry only if
    // primeLine actually disappeared between the hotkey and model loading.
    if !available(VAD_MODEL)?
        || !available(FALLBACK_MODEL)?
        || (matches!(error, SttError::ModelNotInstalled) && available(DEFAULT_MODEL)?)
    {
        return Err(reason);
    }
    on_fallback(crate::messages().recognizer_fallback_notice.clone());
    recognize(FALLBACK_MODEL, samples).map_err(|error| {
        crate::messages()
            .recognizer_fallback_failed
            .replace("{reason}", &recognition_message(&error))
    })
}

#[cfg(test)]
mod tests;
