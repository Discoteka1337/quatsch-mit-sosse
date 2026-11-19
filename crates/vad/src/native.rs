use std::{ffi::CStr, ptr::NonNull};

use sherpa_rs::sherpa_rs_sys as sys;

use crate::{Error, Result, SAMPLE_RATE, VadConfig, WINDOW_SIZE};

pub(crate) struct Detector(NonNull<sys::SherpaOnnxVoiceActivityDetector>);

// The handle is exclusively owned and every native mutation requires &mut self.
unsafe impl Send for Detector {}

impl Detector {
    pub(crate) fn new(model: &CStr, options: &VadConfig) -> Result<Self> {
        // Zero-initialization is the documented C API default for unused models.
        let mut config: sys::SherpaOnnxVadModelConfig = unsafe { std::mem::zeroed() };
        config.silero_vad.model = model.as_ptr();
        config.silero_vad.threshold = options.threshold;
        config.silero_vad.min_silence_duration = options.min_silence_duration;
        config.silero_vad.min_speech_duration = options.min_speech_duration;
        config.silero_vad.max_speech_duration = 3600.0;
        config.silero_vad.window_size = WINDOW_SIZE as i32;
        config.sample_rate = SAMPLE_RATE as i32;
        config.num_threads = options.num_threads as i32;
        config.provider = c"cpu".as_ptr();
        config.debug = 0;
        // Strings remain alive until creation completes; sherpa copies the config.
        // Overlapping 32-second analysis windows bound the audio ring buffer.
        let handle = unsafe { sys::SherpaOnnxCreateVoiceActivityDetector(&config, 45.0) };
        NonNull::new(handle.cast_mut())
            .map(Self)
            .ok_or(Error::ModelLoad)
    }

    pub(crate) fn reset(&mut self) {
        // The constructor rejects null handles and this method has exclusive access.
        unsafe { sys::SherpaOnnxVoiceActivityDetectorReset(self.0.as_ptr()) };
    }

    pub(crate) fn accept(&mut self, window: &[f32; WINDOW_SIZE]) {
        // The native call borrows exactly one full, finite input window synchronously.
        unsafe {
            sys::SherpaOnnxVoiceActivityDetectorAcceptWaveform(
                self.0.as_ptr(),
                window.as_ptr(),
                WINDOW_SIZE as i32,
            )
        };
    }

    pub(crate) fn flush(&mut self) {
        // The handle is valid for the lifetime of this owner.
        unsafe { sys::SherpaOnnxVoiceActivityDetectorFlush(self.0.as_ptr()) };
    }

    pub(crate) fn pop_segment(&mut self) -> Result<Option<(i64, i64)>> {
        // Queue access is exclusive; the copied result is destroyed before popping.
        unsafe {
            if sys::SherpaOnnxVoiceActivityDetectorEmpty(self.0.as_ptr()) == 1 {
                return Ok(None);
            }
            let segment = sys::SherpaOnnxVoiceActivityDetectorFront(self.0.as_ptr());
            let Some(raw) = segment.as_ref() else {
                return Err(Error::InvalidSegment);
            };
            let result = (i64::from(raw.start), i64::from(raw.n));
            sys::SherpaOnnxDestroySpeechSegment(segment);
            sys::SherpaOnnxVoiceActivityDetectorPop(self.0.as_ptr());
            Ok(Some(result))
        }
    }
}

impl Drop for Detector {
    fn drop(&mut self) {
        // This owner releases the non-null native handle exactly once.
        unsafe { sys::SherpaOnnxDestroyVoiceActivityDetector(self.0.as_ptr()) };
    }
}
