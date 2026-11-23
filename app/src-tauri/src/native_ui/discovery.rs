use super::LABELS;
use quatsch_mit_sosse_audio::input_devices;
use quatsch_mit_sosse_models::{Manager, ModelKind};
use std::{io, sync::mpsc, thread};

pub(super) struct ModelChoice {
    pub id: String,
    pub name: String,
    pub installed: bool,
}

pub(super) fn read_devices() -> io::Result<Vec<String>> {
    input_devices()
        .map(|devices| devices.into_iter().map(|device| device.name).collect())
        .map_err(io::Error::other)
}

pub(super) fn read_models() -> io::Result<Vec<ModelChoice>> {
    Manager::new()
        .and_then(|manager| {
            manager
                .list()
                .iter()
                .filter(|model| model.kind == ModelKind::Asr)
                .map(|model| {
                    Ok(ModelChoice {
                        id: model.id.clone(),
                        name: model.display_name.clone(),
                        installed: manager.is_installed(&model.id)?,
                    })
                })
                .collect()
        })
        .map_err(io::Error::other)
}

#[derive(Default)]
pub(crate) struct Inventory {
    devices: Discovery<io::Result<Vec<String>>>,
    models: Discovery<io::Result<Vec<ModelChoice>>>,
}

pub(super) struct Updates {
    pub devices: io::Result<Option<Vec<String>>>,
    pub models: io::Result<Option<Vec<ModelChoice>>>,
}

impl Inventory {
    pub(super) fn request(
        &mut self,
        devices: Option<impl FnOnce() -> io::Result<Vec<String>> + Send + 'static>,
        models: Option<impl FnOnce() -> io::Result<Vec<ModelChoice>> + Send + 'static>,
    ) -> io::Result<()> {
        let devices = devices.map_or(Ok(()), |read| self.devices.request(read));
        let models = models.map_or(Ok(()), |read| self.models.request(read));
        devices.and(models)
    }

    pub(super) fn poll(&mut self, allow_devices: bool, allow_models: bool) -> Updates {
        Updates {
            devices: self.devices.take_ready(allow_devices, read_devices),
            models: self.models.take_ready(allow_models, read_models),
        }
    }

    pub(super) fn pending(&self) -> bool {
        self.devices.pending.is_some() || self.models.pending.is_some()
    }
}

pub(crate) struct Discovery<T> {
    pending: Option<mpsc::Receiver<T>>,
    again: bool,
}

impl<T> Default for Discovery<T> {
    fn default() -> Self {
        Self {
            pending: None,
            again: false,
        }
    }
}

impl<T: Send + 'static> Discovery<T> {
    pub fn request(&mut self, scan: impl FnOnce() -> T + Send + 'static) -> io::Result<()> {
        if self.pending.is_some() {
            self.again = true;
            return Ok(());
        }
        let (sender, receiver) = mpsc::channel();
        thread::Builder::new()
            .name("qms-settings-inventory".into())
            .spawn(move || {
                let _ = sender.send(scan());
            })?;
        self.pending = Some(receiver);
        Ok(())
    }

    pub fn poll(&mut self) -> io::Result<Option<(T, bool)>> {
        let Some(pending) = &self.pending else {
            return Ok(None);
        };
        let result = match pending.try_recv() {
            Ok(value) => Ok(Some((value, self.again))),
            Err(mpsc::TryRecvError::Empty) => return Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                Err(io::Error::other(&LABELS.general_failure[..]))
            }
        };
        self.pending = None;
        self.again = false;
        result
    }
}

impl<T: Send + 'static> Discovery<io::Result<T>> {
    fn take_ready(
        &mut self,
        allow: bool,
        read: impl FnOnce() -> io::Result<T> + Send + 'static,
    ) -> io::Result<Option<T>> {
        if !allow {
            return Ok(None);
        }
        let Some((value, again)) = self.poll()? else {
            return Ok(None);
        };
        if again {
            self.request(read)?;
        }
        value.map(Some)
    }
}
