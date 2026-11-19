use corpus_recorder::Recorder;
use quatsch_mit_sosse_audio::{AudioCapture, AudioEvent, DeviceSelection};
use std::{
    io,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

#[derive(Default)]
struct Health {
    levels: usize,
    generation: u64,
    last_level: Option<Instant>,
}

impl Health {
    fn on_level(&mut self) {
        self.levels = self.levels.saturating_add(1);
        self.last_level = Some(Instant::now());
    }

    fn on_event(&mut self, event: AudioEvent) {
        if !matches!(event, AudioEvent::RealtimeUnavailable) {
            self.levels = 0;
            self.last_level = None;
            self.generation = self.generation.wrapping_add(1);
        }
    }

    fn ready(&self) -> bool {
        // The audio crate emits 30 RMS reports per second of received samples.
        self.levels >= 16
            && self
                .last_level
                .is_some_and(|last| last.elapsed() < Duration::from_millis(250))
    }
}

pub struct Microphone {
    selection: DeviceSelection,
    capture: Option<AudioCapture>,
    health: Arc<Mutex<Health>>,
    take_generation: Option<u64>,
}

impl Microphone {
    pub fn new(selection: DeviceSelection) -> Self {
        Self {
            selection,
            capture: None,
            health: Arc::new(Mutex::new(Health::default())),
            take_generation: None,
        }
    }

    fn health(&self) -> io::Result<std::sync::MutexGuard<'_, Health>> {
        self.health
            .lock()
            .map_err(|_| io::Error::other("Der Mikrofonzustand ist nicht mehr verfügbar."))
    }
}

impl Recorder for Microphone {
    fn prepare(&mut self) -> io::Result<()> {
        if self.capture.is_none() {
            let level_health = Arc::clone(&self.health);
            let event_health = Arc::clone(&self.health);
            self.capture = Some(
                AudioCapture::start(
                    self.selection.clone(),
                    move |_| {
                        if let Ok(mut health) = level_health.lock() {
                            health.on_level();
                        }
                    },
                    move |event| {
                        if let Ok(mut health) = event_health.lock() {
                            health.on_event(event);
                        }
                    },
                )
                .map_err(io::Error::other)?,
            );
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while !self.health()?.ready() {
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Innerhalb von zehn Sekunden kam kein vollständiger Mikrofonvorlauf an. \
                     Bitte Standardmikrofon und Windows-Mikrofonberechtigung prüfen.",
                ));
            }
            thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    fn start(&mut self) -> io::Result<()> {
        if self.take_generation.is_some() {
            return Err(io::Error::other("Es läuft bereits eine Aufnahme."));
        }
        self.prepare()?;
        let generation = self.health()?.generation;
        self.capture
            .as_ref()
            .expect("prepare creates the capture")
            .start_recording()
            .map_err(io::Error::other)?;
        self.take_generation = Some(generation);
        Ok(())
    }

    fn finish(&mut self) -> io::Result<Vec<f32>> {
        let generation = self
            .take_generation
            .take()
            .ok_or_else(|| io::Error::other("Es läuft keine Aufnahme."))?;
        let recording = self
            .capture
            .as_ref()
            .expect("start creates the capture")
            .finish_recording()
            .map_err(io::Error::other)?;
        let health = self.health()?;
        validate_take(recording.samples(), generation, health.generation)?;
        if !health.ready() {
            return Err(io::Error::other(
                "Der Mikrofonstrom wurde unterbrochen. Bitte die Aufnahme wiederholen.",
            ));
        }
        Ok(recording.into_samples())
    }
}

fn validate_take(samples: &[f32], started: u64, finished: u64) -> io::Result<()> {
    if samples.is_empty() || started != finished {
        return Err(io::Error::other(
            "Die Aufnahme ist leer oder das Mikrofon wurde unterbrochen beziehungsweise gewechselt. \
             Bitte die Aufnahme wiederholen.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use quatsch_mit_sosse_audio::{AudioError, DeviceInfo};

    fn connected_event() -> AudioEvent {
        AudioEvent::DeviceChanged {
            device: DeviceInfo {
                name: "Testmikrofon".into(),
            },
            fallback: false,
        }
    }

    #[test]
    fn warmup_requires_sixteen_received_rms_reports() {
        let mut health = Health::default();
        health.on_event(connected_event());
        for _ in 0..15 {
            health.on_level();
        }
        assert!(!health.ready());
        health.on_level();
        assert!(health.ready());
    }

    #[test]
    fn old_reports_cannot_leave_a_stalled_microphone_ready() {
        let mut health = Health::default();
        for _ in 0..16 {
            health.on_level();
        }
        health.last_level = Some(Instant::now() - Duration::from_secs(1));
        assert!(!health.ready());
    }

    #[test]
    fn device_changes_and_glitches_invalidate_takes_and_reset_preroll() {
        for event in [
            connected_event(),
            AudioEvent::DeviceLost {
                device: DeviceInfo {
                    name: "Testmikrofon".into(),
                },
                error: AudioError::StreamGlitch,
            },
            AudioEvent::DeviceUnavailable {
                error: AudioError::NoInputDevice,
            },
            AudioEvent::InputOverflow,
            AudioEvent::InputDiscontinuity,
            AudioEvent::Stopped,
        ] {
            let mut health = Health::default();
            for _ in 0..16 {
                health.on_level();
            }
            let generation = health.generation;
            health.on_event(event);
            assert!(!health.ready());
            assert_ne!(health.generation, generation);
        }
    }

    #[test]
    fn realtime_warning_does_not_discard_valid_audio() {
        let mut health = Health::default();
        for _ in 0..16 {
            health.on_level();
        }
        let generation = health.generation;
        health.on_event(AudioEvent::RealtimeUnavailable);
        assert!(health.ready());
        assert_eq!(health.generation, generation);
    }

    #[test]
    fn empty_audio_and_changed_generation_are_rejected() {
        assert!(validate_take(&[], 1, 1).is_err());
        assert!(validate_take(&[0.0], 1, 2).is_err());
        assert!(validate_take(&[0.0, 0.5, -0.5], 1, 1).is_ok());
    }
}
