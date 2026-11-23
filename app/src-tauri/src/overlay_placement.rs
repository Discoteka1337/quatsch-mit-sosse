use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs, io,
    path::Path,
    sync::{Arc, Mutex, mpsc},
    thread,
};

mod monitors;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct OverlayPreferences {
    pub show_ready: bool,
    pub hide_from_capture: bool,
}

/// Physical desktop pixels, including negative coordinates on secondary displays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Bounds {
    fn overlap(self, other: Self) -> i64 {
        let span = |a: i32, size_a: i32, b: i32, size_b: i32| {
            let end = (i64::from(a) + i64::from(size_a)).min(i64::from(b) + i64::from(size_b));
            (end - i64::from(a.max(b))).max(0)
        };
        span(self.x, self.width, other.x, other.width)
            * span(self.y, self.height, other.y, other.height)
    }
}

struct Monitor {
    id: String,
    work: Bounds,
    primary: bool,
    dpi: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Offset {
    x: i32,
    y: i32,
}

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct SavedOverlay {
    last_monitor: Option<String>,
    positions: BTreeMap<String, Offset>,
}

impl SavedOverlay {
    fn restored(&self, monitors: &[Monitor], width_dip: i32, height_dip: i32) -> Option<Bounds> {
        let last = monitors
            .iter()
            .find(|m| Some(m.id.as_str()) == self.last_monitor.as_deref());
        let monitor = last
            .or_else(|| monitors.iter().find(|m| m.primary))
            .or_else(|| monitors.first())?;
        let work = monitor.work;
        let scale = |value: i32| {
            ((i64::from(value) * i64::from(monitor.dpi) + 48) / 96).clamp(1, i64::from(i32::MAX))
                as i32
        };
        let width = scale(width_dip).min(work.width);
        let height = scale(height_dip).min(work.height);
        let max_x = work.x.saturating_add(work.width - width);
        let max_y = work.y.saturating_add(work.height - height);
        // A missing last ID means the user dropped outside all work areas.
        // Otherwise even a fallback display can have a previously saved position.
        if self.last_monitor.is_some()
            && let Some(pos) = self.positions.get(&monitor.id)
        {
            let candidate = Bounds {
                x: work.x.saturating_add(pos.x),
                y: work.y.saturating_add(pos.y),
                width,
                height,
            };
            if work.overlap(candidate) > 0 {
                return Some(Bounds {
                    x: candidate.x.clamp(work.x, max_x),
                    y: candidate.y.clamp(work.y, max_y),
                    ..candidate
                });
            }
        }
        Some(Bounds {
            x: work.x.saturating_add((work.width - width) / 2),
            y: max_y.saturating_sub(scale(16)).max(work.y),
            width,
            height,
        })
    }

    fn record(&mut self, monitors: &[Monitor], bounds: Bounds) {
        let monitor = monitors
            .iter()
            .map(|m| (m.work.overlap(bounds), m))
            .filter(|(area, _)| *area > 0)
            .max_by_key(|(area, m)| (*area, m.primary));
        self.last_monitor = monitor.map(|(_, m)| {
            self.positions.insert(
                m.id.clone(),
                Offset {
                    x: bounds.x.saturating_sub(m.work.x),
                    y: bounds.y.saturating_sub(m.work.y),
                },
            );
            m.id.clone()
        });
    }
}

type WriteResult = Box<dyn FnOnce(io::Result<()>) + Send>;

enum Write {
    Save(Arc<SavedOverlay>, WriteResult),
    Flush(mpsc::Sender<()>),
}

pub(crate) struct OverlayStore {
    current: Arc<Mutex<Arc<SavedOverlay>>>,
    writer: mpsc::Sender<Write>,
}

impl OverlayStore {
    pub fn open() -> io::Result<Self> {
        Self::load(&crate::settings::data_dir()?.join("overlay.json"))
    }

    fn load(path: &Path) -> io::Result<Self> {
        let saved = match crate::config_file::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::from)?,
            Err(err) if err.kind() == io::ErrorKind::NotFound => SavedOverlay::default(),
            Err(err) => return Err(err),
        };
        let path = path.to_owned();
        Self::with_writer(saved, move |next| persist(&path, next))
    }

    fn with_writer(
        saved: SavedOverlay,
        mut persist: impl FnMut(&SavedOverlay) -> io::Result<()> + Send + 'static,
    ) -> io::Result<Self> {
        let mut committed = Arc::new(saved);
        let current = Arc::new(Mutex::new(committed.clone()));
        let latest = current.clone();
        let (writer, requests) = mpsc::channel();
        thread::Builder::new()
            .name("qms-overlay-position".into())
            .spawn(move || {
                for request in requests {
                    match request {
                        Write::Save(next, done) => {
                            let result = persist(&next);
                            if result.is_ok() {
                                committed = next;
                            } else {
                                let mut current = latest.lock().unwrap_or_else(|e| e.into_inner());
                                // Only roll back this drag if no later drag has superseded it.
                                if Arc::ptr_eq(&current, &next) {
                                    *current = committed.clone();
                                }
                            }
                            done(result);
                        }
                        Write::Flush(done) => {
                            let _ = done.send(());
                        }
                    }
                }
            })?;
        Ok(Self { current, writer })
    }

    fn snapshot(&self) -> Arc<SavedOverlay> {
        self.current
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub(super) fn pending_writes(&self) -> io::Result<mpsc::Receiver<()>> {
        let (done, completion) = mpsc::channel();
        self.writer
            .send(Write::Flush(done))
            .map_err(|_| io::Error::other("Overlay writer stopped"))?;
        Ok(completion)
    }

    pub fn placement(&self, width_dip: i32, height_dip: i32) -> io::Result<Bounds> {
        self.snapshot()
            .restored(&monitors::connected()?, width_dip, height_dip)
            .ok_or_else(|| io::Error::other("No monitor work area available"))
    }

    pub fn remember(
        &mut self,
        bounds: Bounds,
        done: impl FnOnce(io::Result<()>) + Send + 'static,
    ) -> io::Result<()> {
        self.remember_on(&monitors::connected()?, bounds, done)
    }

    fn remember_on(
        &mut self,
        monitors: &[Monitor],
        bounds: Bounds,
        done: impl FnOnce(io::Result<()>) + Send + 'static,
    ) -> io::Result<()> {
        let mut current = self.current.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = (**current).clone();
        next.record(monitors, bounds);
        if next == **current {
            return Ok(());
        }
        let next = Arc::new(next);
        self.writer
            .send(Write::Save(next.clone(), Box::new(done)))
            .map_err(|_| io::Error::other("Overlay writer stopped"))?;
        *current = next;
        Ok(())
    }
}

fn persist(path: &Path, next: &SavedOverlay) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Missing data directory"))?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(file.as_file_mut(), next)?;
    crate::config_file::sync(file.as_file())?;
    file.persist(path)?;
    Ok(())
}

#[cfg(test)]
mod tests;
