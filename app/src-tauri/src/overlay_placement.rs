use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
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
    preferences: OverlayPreferences,
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

pub(crate) struct OverlayStore {
    path: PathBuf,
    saved: SavedOverlay,
}

impl OverlayStore {
    pub fn open() -> io::Result<Self> {
        let local = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .ok_or_else(|| io::Error::other("Local data directory unavailable"))?;
        Self::load(
            &local
                .join("QuatschMitSosse")
                .join("data")
                .join("overlay.json"),
        )
    }

    fn load(path: &Path) -> io::Result<Self> {
        let saved = match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::from)?,
            Err(err) if err.kind() == io::ErrorKind::NotFound => SavedOverlay::default(),
            Err(err) => return Err(err),
        };
        Ok(Self {
            path: path.into(),
            saved,
        })
    }

    pub fn preferences(&self) -> OverlayPreferences {
        self.saved.preferences
    }

    pub fn set_preferences(&mut self, preferences: OverlayPreferences) -> io::Result<()> {
        self.update(|next| next.preferences = preferences)
    }

    pub fn placement(&self, width_dip: i32, height_dip: i32) -> io::Result<Bounds> {
        self.saved
            .restored(&monitors::connected()?, width_dip, height_dip)
            .ok_or_else(|| io::Error::other("No monitor work area available"))
    }

    pub fn remember(&mut self, bounds: Bounds) -> io::Result<()> {
        self.remember_on(&monitors::connected()?, bounds)
    }

    fn remember_on(&mut self, monitors: &[Monitor], bounds: Bounds) -> io::Result<()> {
        self.update(|next| next.record(monitors, bounds))
    }

    fn update(&mut self, change: impl FnOnce(&mut SavedOverlay)) -> io::Result<()> {
        let mut next = self.saved.clone();
        change(&mut next);
        if next == self.saved {
            return Ok(());
        }
        let parent = self
            .path
            .parent()
            .ok_or_else(|| io::Error::other("Missing data directory"))?;
        fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer_pretty(file.as_file_mut(), &next)?;
        file.as_file().sync_all()?;
        file.persist(&self.path)?;
        self.saved = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
