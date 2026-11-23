use super::{Entry, discovery::Discovery};
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Default)]
pub(super) struct Filter {
    worker: Discovery<Vec<usize>>,
    cancel: Option<Arc<AtomicBool>>,
    queued: Option<(Arc<Vec<Entry>>, String)>,
}

impl Filter {
    pub(super) fn request(&mut self, entries: Arc<Vec<Entry>>, query: String) -> io::Result<()> {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Relaxed);
            self.queued = Some((entries, query));
            return Ok(());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        self.worker.request(move || {
            entries
                .iter()
                .enumerate()
                .take_while(|_| !stop.load(Ordering::Relaxed))
                .filter_map(|(index, entry)| entry.matches(&query).then_some(index))
                .collect()
        })?;
        self.cancel = Some(cancel);
        Ok(())
    }

    pub(super) fn poll(&mut self) -> io::Result<Option<Vec<usize>>> {
        let result = self.worker.poll();
        if matches!(result, Ok(None)) {
            return Ok(None);
        }
        self.cancel = None;
        if let Some((entries, query)) = self.queued.take() {
            self.request(entries, query)?;
            return Ok(None);
        }
        result.map(|ready| ready.map(|(rows, _)| rows))
    }

    pub(super) fn busy(&self) -> bool {
        self.cancel.is_some()
    }
}

impl Drop for Filter {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

pub(super) const ROWS_PER_TICK: usize = 128;
const TIME_PER_TICK: Duration = Duration::from_millis(4);

pub(super) enum RowChange {
    Remove(usize),
    Append(usize),
}

pub(super) struct ListUpdate {
    remove: usize,
    append: std::vec::IntoIter<usize>,
}

impl ListUpdate {
    pub(super) fn new(visible: usize, matches: Vec<usize>) -> Self {
        Self {
            remove: visible,
            append: matches.into_iter(),
        }
    }

    pub(super) fn tick(
        &mut self,
        mut apply: impl FnMut(RowChange) -> io::Result<()>,
    ) -> io::Result<()> {
        let start = Instant::now();
        for _ in 0..ROWS_PER_TICK {
            if self.remove > 0 {
                self.remove -= 1;
                apply(RowChange::Remove(self.remove))?;
            } else if let Some(index) = self.append.next() {
                apply(RowChange::Append(index))?;
            } else {
                break;
            }
            if start.elapsed() >= TIME_PER_TICK {
                break;
            }
        }
        Ok(())
    }

    pub(super) fn finished(&self) -> bool {
        self.remove == 0 && self.append.len() == 0
    }
}
