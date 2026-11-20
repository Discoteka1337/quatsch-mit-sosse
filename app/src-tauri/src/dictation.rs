use crate::{OverlayState, messages};

mod runtime;
pub(super) use runtime::{Controller, report_error};

trait Ports {
    type Target: Copy;
    fn installed(&mut self) -> Result<Option<&'static str>, String>;
    fn models(&mut self) -> Result<(), String>;
    fn begin(&mut self) -> Result<(), String>;
    fn finish(&mut self) -> Result<Vec<f32>, String>;
    fn suspend(&mut self) -> Result<(), String>;
    fn recognize(&mut self, model: &'static str, samples: Vec<f32>) -> Result<(), String>;
    fn insert(&mut self, target: Self::Target, text: &str) -> Result<(), String>;
    fn show(&mut self, state: OverlayState);
}

enum Phase<T> {
    Idle,
    Recording { model: &'static str, target: T },
    Recognizing { target: Option<T> },
    Inserting,
}

struct Session<P: Ports> {
    ports: P,
    phase: Phase<P::Target>,
    last: String,
    administrator_hint: bool,
}

impl<P: Ports> Session<P> {
    fn new(ports: P) -> Self {
        Self {
            ports,
            phase: Phase::Idle,
            last: String::new(),
            administrator_hint: false,
        }
    }

    fn start(&mut self, target: Option<P::Target>) -> bool {
        if !matches!(self.phase, Phase::Idle) {
            return false;
        }
        let result = (|| {
            let Some(model) = self.ports.installed()? else {
                return self.ports.models();
            };
            let target = target.ok_or_else(|| messages().target_unavailable.clone())?;
            if let Err(message) = self.ports.begin() {
                self.ports.suspend()?;
                return Err(message);
            }
            self.phase = Phase::Recording { model, target };
            self.level(0);
            Ok(())
        })();
        if let Err(message) = result {
            self.error(message);
        }
        matches!(self.phase, Phase::Recording { .. })
    }

    fn level(&mut self, level: u8) {
        if matches!(self.phase, Phase::Recording { .. }) {
            self.show(OverlayState::Listening { level });
        }
    }

    fn stop(&mut self) {
        let Phase::Recording { model, target } = self.phase else {
            return;
        };
        self.phase = Phase::Idle;
        let samples = self.ports.finish();
        let result = self.ports.suspend().and(samples).and_then(|samples| {
            self.show(OverlayState::Processing);
            self.ports.recognize(model, samples)
        });
        match result {
            Ok(()) => {
                self.phase = Phase::Recognizing {
                    target: Some(target),
                }
            }
            Err(message) => self.error(message),
        }
    }

    fn cancel(&mut self) {
        if matches!(self.phase, Phase::Recording { .. }) {
            self.phase = Phase::Idle;
            match self.ports.suspend() {
                Ok(()) => self.show(OverlayState::Ready),
                Err(message) => self.error(message),
            }
        }
    }

    fn elevated(&mut self) {
        self.cancel();
        if let Phase::Recognizing { target } = &mut self.phase {
            *target = None;
        }
        self.show(OverlayState::Administrator);
    }

    fn target_available(&mut self) {
        if self.administrator_hint {
            let state = match self.phase {
                Phase::Recognizing { .. } => OverlayState::Processing,
                Phase::Inserting => OverlayState::Inserting,
                _ => OverlayState::Ready,
            };
            self.show(state);
        }
    }

    fn recognized(&mut self, result: Result<String, String>) {
        let Phase::Recognizing { target } = self.phase else {
            return;
        };
        self.phase = Phase::Idle;
        match result {
            Ok(text) if !text.trim().is_empty() => {
                self.last = text;
                self.repeat(target);
            }
            Ok(_) => self.error(messages().no_speech.clone()),
            Err(message) => self.error(message),
        }
    }

    fn repeat(&mut self, target: Option<P::Target>) {
        if !matches!(self.phase, Phase::Idle) {
            return;
        }
        if self.last.is_empty() {
            self.error(messages().no_last_text.clone());
        } else if let Some(target) = target {
            self.show(OverlayState::Inserting);
            match self.ports.insert(target, &self.last) {
                Ok(()) => self.phase = Phase::Inserting,
                Err(message) => self.error(message),
            }
        } else {
            self.error(messages().target_unavailable.clone());
        }
    }

    fn inserted(&mut self, result: Result<(), String>) {
        self.phase = Phase::Idle;
        match result {
            Ok(()) => self.show(OverlayState::Ready),
            Err(message) => self.error(message),
        }
    }

    fn error(&mut self, message: String) {
        self.show(OverlayState::Error {
            message,
            text: self.last.clone(),
        });
    }

    fn show(&mut self, state: OverlayState) {
        self.administrator_hint = matches!(state, OverlayState::Administrator);
        self.ports.show(state);
    }
}

#[cfg(test)]
mod tests;
