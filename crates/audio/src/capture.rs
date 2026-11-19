use crate::{
    AudioError, Recording,
    dsp::{PRE_ROLL_SAMPLES, RingBuffer},
};

pub(crate) struct CaptureBuffer {
    preroll: RingBuffer,
    recording: Option<Vec<f32>>,
}

impl CaptureBuffer {
    pub(crate) fn new() -> Self {
        Self {
            preroll: RingBuffer::new(PRE_ROLL_SAMPLES),
            recording: None,
        }
    }

    pub(crate) fn accept(&mut self, samples: &[f32]) {
        if let Some(recording) = &mut self.recording {
            recording.extend_from_slice(samples);
        }
        for sample in samples {
            self.preroll.push(*sample);
        }
    }

    pub(crate) fn begin(&mut self) -> Result<(), AudioError> {
        if self.recording.is_some() {
            return Err(AudioError::AlreadyRecording);
        }
        self.recording = Some(self.preroll.snapshot());
        Ok(())
    }

    pub(crate) fn finish(&mut self) -> Option<Recording> {
        self.recording.take().map(Recording::new)
    }

    pub(crate) fn reset_preroll(&mut self) {
        self.preroll.clear();
    }
}
