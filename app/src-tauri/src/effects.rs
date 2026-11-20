use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs, io,
    path::PathBuf,
    time::{Duration, Instant},
};

mod native;
mod tone;

trait Volume {
    fn level(&self) -> io::Result<f32>;
    fn set(&self, level: f32) -> io::Result<()>;
}

struct Session<V> {
    device: String,
    group: String,
    volume: V,
}

struct Mixer<V> {
    sessions: BTreeMap<String, Session<V>>,
}

impl<V> Default for Mixer<V> {
    fn default() -> Self {
        Self {
            sessions: BTreeMap::new(),
        }
    }
}

impl<V> Mixer<V> {
    fn group(&self, device: &str, group: &str) -> impl Iterator<Item = &Session<V>> {
        self.sessions
            .values()
            .filter(move |s| s.device == device && s.group == group)
    }
}

#[derive(Serialize, Deserialize)]
struct Saved {
    device: String,
    group: String,
    original: f32,
    lowered: f32,
    instance_only: bool,
}

impl Saved {
    fn restore(&self, volume: &impl Volume) -> io::Result<()> {
        // Respect a manual mixer change. A crash before the actual write also lands here.
        if (volume.level()? - self.lowered).abs() < 0.00001 {
            volume.set(self.original)?;
        }
        Ok(())
    }
}

type Entries = BTreeMap<String, Saved>;
struct Journal(PathBuf);

impl Journal {
    fn read(&self) -> io::Result<Entries> {
        let entries: Entries = match fs::read(&self.0) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Entries::new()),
            Err(error) => return Err(error),
        };
        if entries
            .values()
            .any(|e| !(0.0..=1.0).contains(&e.original) || !(0.0..e.original).contains(&e.lowered))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid recovery volume",
            ));
        }
        Ok(entries)
    }

    fn save(&self, entries: &Entries) -> io::Result<()> {
        if entries.is_empty() {
            return match fs::remove_file(&self.0) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                result => result,
            };
        }
        let dir = self
            .0
            .parent()
            .ok_or_else(|| io::Error::other("Missing data directory"))?;
        fs::create_dir_all(dir)?;
        let mut temp = tempfile::NamedTempFile::new_in(dir)?;
        serde_json::to_writer(temp.as_file_mut(), entries)?;
        temp.as_file().sync_all()?;
        temp.persist(&self.0)?;
        Ok(())
    }

    fn lower<V: Volume>(&self, mixer: &Mixer<V>, target: f32) -> io::Result<()> {
        let mut entries = self.read()?;
        // A returning app may still have the cap from a previous recording. Complete
        // its old transaction before using the slider as the baseline for a new cap.
        self.restore_entries(mixer, &mut entries, |id, saved| {
            !mixer.sessions.contains_key(id) || saved.lowered != target
        })?;
        let mut changes = Vec::new();
        for (id, session) in &mixer.sessions {
            let unresolved_sibling = entries.iter().any(|(old, saved)| {
                !mixer.sessions.contains_key(old)
                    && saved.group == session.group
                    && saved.device == session.device
            });
            if entries.contains_key(id) || unresolved_sibling {
                continue;
            }
            let original = session.volume.level()?;
            if original > target {
                entries.insert(
                    id.clone(),
                    Saved {
                        device: session.device.clone(),
                        group: session.group.clone(),
                        original,
                        lowered: target,
                        instance_only: mixer.group(&session.device, &session.group).count() > 1,
                    },
                );
                changes.push(&session.volume);
            }
        }
        if !changes.is_empty() {
            // Every affected session is recoverable even if the process dies inside SetMasterVolume.
            self.save(&entries)?;
            for volume in changes {
                volume.set(target)?;
            }
        }
        Ok(())
    }

    fn restore<V: Volume>(&self, mixer: &Mixer<V>) -> io::Result<()> {
        self.restore_entries(mixer, &mut self.read()?, |_, _| true)
    }

    fn restore_entries<V: Volume>(
        &self,
        mixer: &Mixer<V>,
        entries: &mut Entries,
        eligible: impl Fn(&str, &Saved) -> bool,
    ) -> io::Result<()> {
        let mut failures = None;
        let mut changed = false;
        entries.retain(|id, saved| {
            let mut siblings = mixer.group(&saved.device, &saved.group);
            let replacement = siblings.next();
            if siblings.next().is_some() && !saved.instance_only {
                // Remember this even after one sibling disappears or has been restored.
                saved.instance_only = true;
                changed = true;
            }
            if !eligible(id, saved) {
                return true;
            }
            let session = mixer.sessions.get(id).or(if saved.instance_only {
                None
            } else {
                replacement
            });
            let Some(session) = session else { return true };
            if let Err(error) = saved.restore(&session.volume) {
                failures = Some(error);
                return true;
            }
            changed = true;
            false
        });
        if changed {
            self.save(entries)?;
        }
        failures.map_or(Ok(()), Err)
    }
}

pub(crate) struct Effects {
    journal: Journal,
    recording: Option<Recording>,
    recovery_pending: bool,
}

struct Recording {
    tones: bool,
    target: f32,
    checked: Instant,
}

impl Effects {
    pub(crate) fn new(data_dir: PathBuf) -> Self {
        Self {
            journal: Journal(data_dir.join("volume-recovery.json")),
            recording: None,
            recovery_pending: false,
        }
    }

    pub(crate) fn recover(&mut self) -> Result<(), String> {
        let restore = || -> io::Result<bool> {
            if self.journal.0.try_exists()? {
                native::with_mixer(|m| self.journal.restore(m))?;
                return self.journal.0.try_exists();
            }
            Ok(false)
        };
        let result = restore();
        self.recovery_pending = result.as_ref().copied().unwrap_or(true);
        result.map(drop).map_err(|_| {
            "Die Lautstärke anderer Apps konnte nicht vollständig wiederhergestellt werden.".into()
        })
    }

    pub(crate) fn needs_recovery(&self) -> bool {
        self.recovery_pending && self.recording.is_none()
    }

    pub(crate) fn begin(&mut self, tones: bool, volume_percent: u8) -> Result<(), String> {
        if self.recording.is_some() {
            return Ok(());
        }
        self.recover()?;
        let target = f32::from(volume_percent.min(100)) / 100.0;
        if target < 1.0 && native::with_mixer(|m| self.journal.lower(m, target)).is_err() {
            self.recover()?;
            return Err("Die Lautstärke anderer Apps konnte nicht abgesenkt werden.".into());
        }
        if tones {
            tone::play(true);
        }
        self.recording = Some(Recording {
            tones,
            target,
            checked: Instant::now(),
        });
        Ok(())
    }

    pub(crate) fn refresh(&mut self) -> Result<(), String> {
        if let Some(active) = &mut self.recording
            && active.target < 1.0
            && active.checked.elapsed() >= Duration::from_secs(1)
        {
            active.checked = Instant::now();
            native::with_mixer(|m| self.journal.lower(m, active.target)).map_err(|_| {
                "Die Lautstärke neuer Audiositzungen konnte nicht abgesenkt werden.".to_owned()
            })?;
        }
        Ok(())
    }

    pub(crate) fn end(&mut self) -> Result<(), String> {
        if let Some(active) = self.recording.take() {
            if active.tones {
                tone::play(false);
            }
            self.recover()?;
        }
        Ok(())
    }
}

impl Drop for Effects {
    fn drop(&mut self) {
        let _ = self.end();
    }
}

#[cfg(test)]
mod tests;
