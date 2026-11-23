use crate::{OverlayState, comparison::DictationRecord, messages};
use std::sync::Arc;

mod dictionary;
mod review;
mod runtime;
use dictionary::Transcript;
pub(super) use runtime::{Controller, report_error};

fn with_warning(
    state: OverlayState,
    warning: &mut Option<String>,
    ready_text: &str,
) -> OverlayState {
    if !matches!(state, OverlayState::Ready | OverlayState::Error { .. }) {
        return state;
    }
    let Some(warning) = warning.take() else {
        return state;
    };
    match state {
        OverlayState::Error { message, text } => OverlayState::Error {
            message: format!("{message} {warning}"),
            text,
        },
        _ => OverlayState::Error {
            message: warning,
            text: ready_text.to_owned(),
        },
    }
}

trait Ports {
    type Target: Copy;
    fn installed(&mut self) -> Result<Option<&'static str>, String>;
    fn models(&mut self) -> Result<(), String>;
    fn begin(&mut self) -> Result<(), String>;
    fn finish(&mut self) -> Result<Vec<f32>, String>;
    fn suspend(&mut self) -> Result<(), String>;
    fn recognize(&mut self, model: &'static str, samples: Vec<f32>) -> Result<(), String>;
    fn insert(
        &mut self,
        target: Self::Target,
        text: &str,
        cursor: Option<usize>,
    ) -> Result<(), String>;
    fn compare(&mut self, target: Self::Target, record: Arc<DictationRecord>)
    -> Result<(), String>;
    fn show(&mut self, state: OverlayState);
}

enum Phase<T> {
    Idle,
    Recording { model: &'static str, target: T },
    Recognizing { target: Option<T> },
    Inserting { text: String },
}

struct Session<P: Ports> {
    ports: P,
    phase: Phase<P::Target>,
    history: Vec<Arc<DictationRecord>>,
    last_insertable: Option<Arc<DictationRecord>>,
    warning: Option<String>,
    administrator_hint: bool,
}

impl<P: Ports> Session<P> {
    fn new(ports: P) -> Self {
        Self {
            ports,
            phase: Phase::Idle,
            history: Vec::new(),
            last_insertable: None,
            warning: None,
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

    fn discard_target(&mut self) {
        self.cancel();
        if let Phase::Recognizing { target } = &mut self.phase {
            *target = None;
        }
    }

    fn unavailable(&mut self, message: String) {
        self.discard_target();
        self.error(message);
    }

    fn elevated(&mut self) {
        self.discard_target();
        self.show(OverlayState::Administrator);
    }

    fn target_available(&mut self) {
        if self.administrator_hint {
            let state = match self.phase {
                Phase::Recognizing { .. } => OverlayState::Processing,
                Phase::Inserting { .. } => OverlayState::Inserting,
                _ => OverlayState::Ready,
            };
            self.show(state);
        }
    }

    fn recognized(&mut self, result: Result<Transcript, String>) {
        let Phase::Recognizing { target } = self.phase else {
            return;
        };
        self.phase = Phase::Idle;
        match result {
            Ok(transcript) if !transcript.record.original.trim().is_empty() => {
                self.warning = transcript.warning;
                let record = Arc::new(transcript.record);
                self.history.push(record.clone());
                if record.finished.is_empty() {
                    self.show(OverlayState::Ready);
                } else {
                    self.last_insertable = Some(record);
                    self.repeat(target);
                }
            }
            Ok(_) => self.error(messages().no_speech.clone()),
            Err(message) => self.error(message),
        }
    }

    fn repeat(&mut self, target: Option<P::Target>) {
        if !matches!(self.phase, Phase::Idle) {
            return;
        }
        let Some(record) = self.last_insertable.clone() else {
            self.error(messages().no_last_text.clone());
            return;
        };
        if let Some(target) = target {
            self.insert(target, &record.finished, record.cursor);
        } else {
            self.error(messages().target_unavailable.clone());
        }
    }

    fn compare(&mut self, target: Option<P::Target>) {
        if !matches!(self.phase, Phase::Idle) {
            return;
        }
        let Some(record) = self.history.last().cloned() else {
            self.error(messages().no_comparison.clone());
            return;
        };
        let Some(target) = target else {
            self.error(messages().target_unavailable.clone());
            return;
        };
        if let Err(message) = self.ports.compare(target, record) {
            self.error(message);
        }
    }

    fn insert_original(&mut self, target: P::Target, original: &str) {
        if matches!(self.phase, Phase::Idle) {
            self.insert(target, original, None);
        } else {
            self.show(OverlayState::Error {
                message: messages().original_busy.clone(),
                text: original.to_owned(),
            });
        }
    }

    fn insert(&mut self, target: P::Target, text: &str, cursor: Option<usize>) {
        self.show(OverlayState::Inserting);
        match self.ports.insert(target, text, cursor) {
            Ok(()) => {
                self.phase = Phase::Inserting {
                    text: text.to_owned(),
                }
            }
            Err(message) => self.show(OverlayState::Error {
                message,
                text: text.to_owned(),
            }),
        }
    }

    fn inserted(&mut self, result: Result<(), String>) {
        let Phase::Inserting { text } = &mut self.phase else {
            return;
        };
        let text = std::mem::take(text);
        self.phase = Phase::Idle;
        match result {
            Ok(()) => self.show(OverlayState::Ready),
            Err(message) => self.show(OverlayState::Error { message, text }),
        }
    }

    fn error(&mut self, message: String) {
        self.show(OverlayState::Error {
            message,
            text: self
                .last_insertable
                .as_ref()
                .map(|record| record.finished.clone())
                .unwrap_or_default(),
        });
    }

    fn show(&mut self, state: OverlayState) {
        self.administrator_hint = matches!(state, OverlayState::Administrator);
        let text = self
            .last_insertable
            .as_ref()
            .map_or("", |record| record.finished.as_str());
        self.ports
            .show(with_warning(state, &mut self.warning, text));
    }
}

#[cfg(test)]
mod tests;
