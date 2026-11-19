use std::{
    io,
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub(crate) struct IdleModel<T: Send + 'static> {
    shared: Arc<Shared<T>>,
    worker: Option<JoinHandle<()>>,
}

struct Shared<T> {
    state: Mutex<State<T>>,
    changed: Condvar,
    timeout: Duration,
}

struct State<T> {
    model: Option<T>,
    last_used: Option<Instant>,
    stopping: bool,
}

impl<T> Default for State<T> {
    fn default() -> Self {
        Self {
            model: None,
            last_used: None,
            stopping: false,
        }
    }
}

impl<T> State<T> {
    fn finish_use(&mut self, now: Instant) {
        self.last_used = self.model.as_ref().map(|_| now);
    }

    fn remaining_idle(&self, now: Instant, timeout: Duration) -> Option<Duration> {
        self.model.as_ref()?;
        self.last_used
            .map(|last_used| timeout.saturating_sub(now.saturating_duration_since(last_used)))
    }

    fn evict_if_idle(&mut self, now: Instant, timeout: Duration) -> bool {
        if self.remaining_idle(now, timeout) != Some(Duration::ZERO) {
            return false;
        }
        self.model = None;
        self.last_used = None;
        true
    }
}

impl<T> Shared<T> {
    fn lock(&self) -> MutexGuard<'_, State<T>> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

// Finishing a failed or unwinding operation also starts the idle timer.
struct Usage<'a, T> {
    shared: &'a Shared<T>,
    state: MutexGuard<'a, State<T>>,
}

impl<T> Drop for Usage<'_, T> {
    fn drop(&mut self) {
        let now = Instant::now();
        self.state.finish_use(now);
        self.state.evict_if_idle(now, self.shared.timeout);
        self.shared.changed.notify_one();
    }
}

impl<T: Send + 'static> IdleModel<T> {
    pub(crate) fn new(timeout: Duration) -> io::Result<Self> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
            timeout,
        });
        let timer = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("qms-model-idle".into())
            .spawn(move || {
                let mut state = timer.lock();
                loop {
                    if state.stopping {
                        state.model = None;
                        return;
                    }
                    let now = Instant::now();
                    state.evict_if_idle(now, timer.timeout);
                    state = match state.remaining_idle(now, timer.timeout) {
                        Some(remaining) => {
                            timer
                                .changed
                                .wait_timeout(state, remaining)
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .0
                        }
                        None => timer
                            .changed
                            .wait(state)
                            .unwrap_or_else(std::sync::PoisonError::into_inner),
                    };
                }
            })?;
        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }

    pub(crate) fn with<R, E>(
        &self,
        load: impl FnOnce() -> Result<T, E>,
        use_model: impl FnOnce(&mut T) -> Result<R, E>,
    ) -> Result<R, E> {
        let mut state = self.shared.lock();
        // Enforce expiration even if the timer thread has not been scheduled yet.
        state.evict_if_idle(Instant::now(), self.shared.timeout);
        if state.model.is_none() {
            state.model = Some(load()?);
        }
        let mut usage = Usage {
            shared: &self.shared,
            state,
        };
        use_model(usage.state.model.as_mut().expect("model loaded above"))
    }

    pub(crate) fn is_loaded(&self) -> bool {
        self.shared.lock().model.is_some()
    }
}

impl<T: Send + 'static> Drop for IdleModel<T> {
    fn drop(&mut self) {
        self.shared.lock().stopping = true;
        self.shared.changed.notify_one();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
            mpsc,
        },
        time::{Duration, Instant},
    };

    use super::{IdleModel, State};

    struct Tracked(Arc<AtomicUsize>);

    impl Drop for Tracked {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn construction_is_lazy_and_repeated_use_reuses_the_model() {
        let loads = AtomicUsize::new(0);
        let cache = IdleModel::new(Duration::from_secs(300)).unwrap();
        assert!(!cache.is_loaded());
        for expected in 1..=2 {
            let result: Result<usize, ()> = cache.with(
                || {
                    loads.fetch_add(1, Ordering::SeqCst);
                    Ok(0usize)
                },
                |model| {
                    *model += 1;
                    Ok(*model)
                },
            );
            assert_eq!(result, Ok(expected));
        }
        assert!(cache.is_loaded());
        assert_eq!(loads.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_load_error_does_not_cache_a_model_and_the_next_use_retries() {
        let cache = IdleModel::new(Duration::from_secs(300)).unwrap();
        let failed: Result<(), &str> = cache.with(|| Err("load"), |_: &mut usize| Ok(()));
        assert_eq!(failed, Err("load"));
        assert!(!cache.is_loaded());
        let retried: Result<usize, &str> = cache.with(|| Ok(7), |model| Ok(*model));
        assert_eq!(retried, Ok(7));
    }

    #[test]
    fn timeout_boundary_and_last_use_are_deterministic() {
        let dropped = Arc::new(AtomicUsize::new(0));
        let start = Instant::now();
        let timeout = Duration::from_secs(300);
        let mut state = State::default();
        assert!(!state.evict_if_idle(start + timeout, timeout));
        state.model = Some(Tracked(Arc::clone(&dropped)));
        state.finish_use(start);
        assert!(!state.evict_if_idle(start + timeout - Duration::from_nanos(1), timeout));
        assert_eq!(dropped.load(Ordering::SeqCst), 0);

        state.finish_use(start + Duration::from_secs(100));
        assert!(!state.evict_if_idle(start + timeout, timeout));
        assert!(state.evict_if_idle(start + Duration::from_secs(400), timeout));
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
        assert!(!state.evict_if_idle(start + Duration::from_secs(500), timeout));
    }

    #[test]
    fn zero_timeout_unloads_even_after_an_inference_error() {
        let dropped = Arc::new(AtomicUsize::new(0));
        let cache = IdleModel::new(Duration::ZERO).unwrap();
        let result: Result<(), &str> =
            cache.with(|| Ok(Tracked(Arc::clone(&dropped))), |_| Err("inference"));
        assert_eq!(result, Err("inference"));
        assert!(!cache.is_loaded());
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn the_timer_unloads_without_a_followup_api_call() {
        struct SignalDrop(mpsc::Sender<()>);
        impl Drop for SignalDrop {
            fn drop(&mut self) {
                let _ = self.0.send(());
            }
        }
        let (sender, receiver) = mpsc::channel();
        let cache = IdleModel::new(Duration::from_millis(20)).unwrap();
        let result: Result<(), ()> = cache.with(|| Ok(SignalDrop(sender)), |_| Ok(()));
        assert!(result.is_ok());
        receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(!cache.is_loaded());
    }

    #[test]
    fn a_model_stays_loaded_for_the_entire_operation() {
        let dropped = Arc::new(AtomicUsize::new(0));
        let cache = IdleModel::new(Duration::from_millis(10)).unwrap();
        let result: Result<(), ()> = cache.with(
            || Ok(Tracked(Arc::clone(&dropped))),
            |_| {
                std::thread::sleep(Duration::from_millis(30));
                assert_eq!(dropped.load(Ordering::SeqCst), 0);
                Ok(())
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn dropping_the_cache_unloads_a_model_without_waiting_for_its_timeout() {
        let dropped = Arc::new(AtomicUsize::new(0));
        let cache = IdleModel::new(Duration::from_secs(300)).unwrap();
        let result: Result<(), ()> = cache.with(|| Ok(Tracked(Arc::clone(&dropped))), |_| Ok(()));
        assert!(result.is_ok());
        drop(cache);
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
    }
}
