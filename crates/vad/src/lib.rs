use std::{ffi::CString, path::PathBuf, time::Duration};

mod native;
mod runtime;

pub use runtime::{RuntimeError, initialize_runtime};

pub const SAMPLE_RATE: u32 = 16_000;
const WINDOW_SIZE: usize = 512;
const ANALYSIS_SAMPLES: usize = WINDOW_SIZE * 1_000;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    #[error("Die Sprachaktivitätserkennung benötigt Mono-Audio mit 16 kHz.")]
    SampleRate,
    #[error("Die Aufnahme enthält ungültige oder nicht normalisierte Audiodaten.")]
    InvalidSamples,
    #[error("Die Aufnahme ist für die Sprachaktivitätserkennung zu lang.")]
    RecordingTooLong,
    #[error("Die Einstellungen der Sprachaktivitätserkennung sind ungültig.")]
    InvalidConfig,
    #[error("Der Pfad zum Sprachaktivitätsmodell ist ungültig.")]
    InvalidModelPath,
    #[error("Das Sprachaktivitätsmodell ist nicht installiert oder nicht lesbar.")]
    ModelUnavailable,
    #[error("Das Sprachaktivitätsmodell konnte nicht geladen werden.")]
    ModelLoad,
    #[error("Die Sprachaktivitätserkennung hat ungültige Abschnitte geliefert.")]
    InvalidSegment,
}

#[derive(Debug, Clone)]
pub struct VadConfig {
    pub num_threads: u32,
    pub threshold: f32,
    pub min_silence_duration: f32,
    pub min_speech_duration: f32,
    pub speech_padding: Duration,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            num_threads: 1,
            threshold: 0.5,
            min_silence_duration: 0.5,
            min_speech_duration: 0.25,
            speech_padding: Duration::from_millis(100),
        }
    }
}

impl VadConfig {
    fn validate(&self) -> Result<()> {
        if self.num_threads == 0
            || self.num_threads > i32::MAX as u32
            || !self.threshold.is_finite()
            || self.threshold <= 0.0
            || self.threshold > 1.0
            || [self.min_silence_duration, self.min_speech_duration]
                .iter()
                .any(|duration| !duration.is_finite() || *duration <= 0.0 || *duration > 10.0)
            || self.speech_padding > Duration::from_secs(10)
        {
            return Err(Error::InvalidConfig);
        }
        Ok(())
    }
}

/// Half-open sample offsets into the original recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpeechSegment {
    pub start: usize,
    pub end: usize,
}

pub struct SileroVad {
    model_path: PathBuf,
    config: VadConfig,
    detector: Option<native::Detector>,
}

impl SileroVad {
    /// Stores configuration only; model loading is deferred until nonempty audio arrives.
    pub fn new(model_path: PathBuf, config: VadConfig) -> Result<Self> {
        config.validate()?;
        model_path_string(&model_path)?;
        Ok(Self {
            model_path,
            config,
            detector: None,
        })
    }

    pub fn is_loaded(&self) -> bool {
        self.detector.is_some()
    }

    /// Opens the native model while the caller can guarantee the files are available.
    pub fn load(&mut self) -> Result<()> {
        if self.detector.is_none() {
            if !self.model_path.is_file() || std::fs::File::open(&self.model_path).is_err() {
                return Err(Error::ModelUnavailable);
            }
            initialize_runtime()?;
            let path = model_path_string(&self.model_path)?;
            self.detector = Some(native::Detector::new(&path, &self.config)?);
        }
        Ok(())
    }

    pub fn unload(&mut self) {
        self.detector = None;
    }

    pub fn has_speech(&mut self, samples: &[f32], sample_rate: u32) -> Result<bool> {
        self.speech_segments(samples, sample_rate)
            .map(|segments| !segments.is_empty())
    }

    /// Detects speech and resets native stream state after each recording.
    pub fn speech_segments(
        &mut self,
        samples: &[f32],
        sample_rate: u32,
    ) -> Result<Vec<SpeechSegment>> {
        validate_audio(samples, sample_rate)?;
        if samples.is_empty() {
            return Ok(Vec::new());
        }
        self.load()?;
        let detector = self.detector.as_mut().ok_or(Error::ModelLoad)?;
        let padding =
            (self.config.speech_padding.as_secs_f64() * f64::from(SAMPLE_RATE)).round() as usize;
        detector.reset();
        let result = detect(detector, samples, padding, &self.config);
        detector.reset();
        result
    }
}

fn model_path_string(path: &std::path::Path) -> Result<CString> {
    let path = path
        .to_str()
        .filter(|path| !path.is_empty())
        .ok_or(Error::InvalidModelPath)?;
    CString::new(path).map_err(|_| Error::InvalidModelPath)
}

fn validate_audio(samples: &[f32], sample_rate: u32) -> Result<()> {
    if sample_rate != SAMPLE_RATE {
        return Err(Error::SampleRate);
    }
    // The bundled sherpa C API uses signed 32-bit sample positions.
    if samples.len() > i32::MAX as usize - WINDOW_SIZE {
        return Err(Error::RecordingTooLong);
    }
    if samples
        .iter()
        .any(|sample| !sample.is_finite() || sample.abs() > 1.0)
    {
        return Err(Error::InvalidSamples);
    }
    Ok(())
}

fn detect(
    detector: &mut native::Detector,
    samples: &[f32],
    padding: usize,
    config: &VadConfig,
) -> Result<Vec<SpeechSegment>> {
    let mut segments = Vec::new();
    for range in analysis_ranges(samples.len(), config) {
        // Overlap preserves speech near boundaries while bounding sherpa's audio
        // buffer. Reset avoids carrying hangover state across forced flushes.
        detector.reset();
        for chunk in samples[range.clone()].chunks(WINDOW_SIZE) {
            let mut window = [0.0; WINDOW_SIZE];
            window[..chunk.len()].copy_from_slice(chunk);
            detector.accept(&window);
            collect_segments(detector, &mut segments, &range, samples.len(), padding)?;
        }
        detector.flush();
        collect_segments(detector, &mut segments, &range, samples.len(), padding)?;
    }
    merge_segments(&mut segments);
    Ok(segments)
}

fn analysis_ranges(recording_len: usize, config: &VadConfig) -> Vec<std::ops::Range<usize>> {
    let context = ((config.min_speech_duration + config.min_silence_duration) * SAMPLE_RATE as f32)
        .ceil() as usize
        + 2 * WINDOW_SIZE;
    let overlap = context.div_ceil(WINDOW_SIZE) * WINDOW_SIZE;
    let stride = ANALYSIS_SAMPLES - overlap;
    let mut ranges = Vec::new();
    let mut start = 0;
    while start < recording_len {
        let end = start.saturating_add(ANALYSIS_SAMPLES).min(recording_len);
        ranges.push(start..end);
        if end == recording_len {
            break;
        }
        start += stride;
    }
    ranges
}

fn collect_segments(
    detector: &mut native::Detector,
    segments: &mut Vec<SpeechSegment>,
    range: &std::ops::Range<usize>,
    recording_len: usize,
    padding: usize,
) -> Result<()> {
    while let Some((start, len)) = detector.pop_segment()? {
        let start = usize::try_from(start).map_err(|_| Error::InvalidSegment)?;
        let len = usize::try_from(len).map_err(|_| Error::InvalidSegment)?;
        if start < range.len() {
            let len = len.min(range.len() - start);
            append_segment(
                segments,
                (range.start + start) as i64,
                len as i64,
                recording_len,
                padding,
            )?;
        }
    }
    Ok(())
}

fn append_segment(
    segments: &mut Vec<SpeechSegment>,
    start: i64,
    len: i64,
    recording_len: usize,
    padding: usize,
) -> Result<()> {
    let start = usize::try_from(start).map_err(|_| Error::InvalidSegment)?;
    let len = usize::try_from(len).map_err(|_| Error::InvalidSegment)?;
    if len == 0 || start >= recording_len {
        return Ok(());
    }
    let end = start.checked_add(len).ok_or(Error::InvalidSegment)?;
    let segment = SpeechSegment {
        start: start.saturating_sub(padding),
        end: end.saturating_add(padding).min(recording_len),
    };
    segments.push(segment);
    Ok(())
}

fn merge_segments(segments: &mut Vec<SpeechSegment>) {
    segments.sort_unstable_by_key(|segment| segment.start);
    let mut count = 0;
    for index in 0..segments.len() {
        let segment = segments[index];
        if count > 0 && segment.start <= segments[count - 1].end {
            segments[count - 1].end = segments[count - 1].end.max(segment.end);
        } else {
            segments[count] = segment;
            count += 1;
        }
    }
    segments.truncate(count);
}

#[cfg(test)]
mod tests;
