use super::{Phase, Ports, Session, Transcript, with_warning};
use crate::{
    OverlayState,
    comparison::DictationRecord,
    current_settings,
    effects::Effects,
    input::{Action, Target},
    messages, model_window, set_overlay_state,
    speech::{self, Microphone, MicrophoneFault, Recognizer},
};
use quatsch_mit_sosse_hotkey::RecordingTicket;
use quatsch_mit_sosse_models::Manager as ModelManager;
use quatsch_mit_sosse_store::Store;
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread,
    time::Instant,
};
use tauri::{AppHandle, Manager};
use windows::{
    Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW},
    core::HSTRING,
};

type ModelResult = Box<dyn FnOnce(Result<(), String>) + Send>;

enum Command {
    Input(Action),
    InsertOriginal(Target, String),
    Level(u8),
    AudioFault(MicrophoneFault),
    Recognized(Result<Transcript, String>),
    Inserted(Result<(), String>),
    RemoveModel(String, ModelResult),
    ModelRemoved,
    Error(String),
    Shutdown(i32),
}

enum Job {
    Recognize(&'static str, Vec<f32>, crate::settings::CleanupSettings),
    Insert(Target, String, Option<usize>),
    RemoveModel(String, ModelResult),
}

struct Desktop {
    app: AppHandle,
    mic: Microphone,
    reviews: super::review::Requests,
    recording_ticket: Option<RecordingTicket>,
    effects: Effects,
    mixer_warning: Option<String>,
    jobs: Sender<Job>,
    level_pending: Arc<AtomicBool>,
    overlay_failure: Arc<OverlayFailure>,
}

#[derive(Default)]
pub(super) struct OverlayFailure {
    notified: AtomicBool,
    open: AtomicBool,
}

impl OverlayFailure {
    pub(super) fn recovered(&self) {
        self.notified.store(false, Ordering::Release);
    }

    pub(super) fn report(
        self: &Arc<Self>,
        show: impl FnOnce() + Send + 'static,
    ) -> io::Result<Option<thread::JoinHandle<()>>> {
        if self.notified.swap(true, Ordering::AcqRel) || self.open.swap(true, Ordering::AcqRel) {
            return Ok(None);
        }
        let notice = self.clone();
        thread::Builder::new()
            .name("qms-overlay-warning".into())
            .spawn(move || {
                show();
                notice.open.store(false, Ordering::Release);
            })
            .map(Some)
            .inspect_err(|_| {
                self.open.store(false, Ordering::Release);
                self.recovered();
            })
    }

    fn warn(self: &Arc<Self>) {
        if self.report(overlay_failure).is_err() {
            eprintln!("{}", messages().overlay_unavailable);
        }
    }
}

impl Ports for Desktop {
    type Target = Target;

    fn installed(&mut self) -> Result<Option<&'static str>, String> {
        let manager = ModelManager::new().map_err(|e| e.to_string())?;
        let settings = current_settings(&self.app).map_err(|e| e.to_string())?;
        speech::installed_model(&manager, settings.model.as_deref())
    }
    fn models(&mut self) -> Result<(), String> {
        model_window::show(&self.app).map_err(|e| e.to_string())
    }
    fn begin(&mut self) -> Result<(), String> {
        let settings = current_settings(&self.app).map_err(|e| e.to_string())?;
        self.mic.begin(settings.microphone)?;
        if let Err(message) = self.effects.begin(settings.tones, settings.volume_percent) {
            self.mixer_warning = Some(message);
        }
        Ok(())
    }
    fn finish(&mut self) -> Result<Vec<f32>, String> {
        self.mic.finish()
    }
    fn suspend(&mut self) -> Result<(), String> {
        self.recording_ticket = None;
        let microphone = self.mic.suspend();
        if let Err(message) = self.effects.end() {
            self.mixer_warning = Some(message);
        }
        microphone
    }
    fn recognize(&mut self, model: &'static str, samples: Vec<f32>) -> Result<(), String> {
        let cleanup = current_settings(&self.app)
            .map_err(|e| e.to_string())?
            .cleanup;
        self.jobs
            .send(Job::Recognize(model, samples, cleanup))
            .map_err(|_| messages().worker_error.clone())
    }
    fn insert(&mut self, target: Target, text: &str, cursor: Option<usize>) -> Result<(), String> {
        self.jobs
            .send(Job::Insert(target, text.to_owned(), cursor))
            .map_err(|_| messages().worker_error.clone())
    }
    fn compare(&mut self, target: Target, record: Arc<DictationRecord>) -> Result<(), String> {
        let hide = current_settings(&self.app)
            .map_err(|_| messages().comparison_open_error.clone())?
            .overlay
            .hide_from_capture;
        let insert = self.app.state::<Controller>().tx.clone();
        self.reviews.open(
            self.app.clone(),
            target,
            record,
            hide,
            move |target, original| {
                let _ = insert.send(Command::InsertOriginal(target, original));
            },
        )
    }
    fn show(&mut self, state: OverlayState) {
        let state = with_warning(state, &mut self.mixer_warning, "");
        let listening = matches!(state, OverlayState::Listening { .. });
        if listening && self.level_pending.swap(true, Ordering::AcqRel) {
            return;
        }
        let pending = self.level_pending.clone();
        let notice = self.overlay_failure.clone();
        let app = self.app.clone();
        // The user's key-up must finish audio even when the window thread is busy.
        if self
            .app
            .run_on_main_thread(move || {
                if listening {
                    pending.store(false, Ordering::Release);
                }
                if set_overlay_state(&app, state).is_err() {
                    notice.warn();
                } else {
                    notice.recovered();
                }
            })
            .is_err()
        {
            self.level_pending.store(false, Ordering::Release);
            self.overlay_failure.warn();
        }
    }
}

fn overlay_failure() {
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(&messages().overlay_unavailable),
            &HSTRING::from(&messages().app_name),
            MB_ICONERROR | MB_OK,
        );
    }
}

pub(crate) struct Controller {
    tx: Sender<Command>,
    stopping: Arc<AtomicBool>,
}

impl Controller {
    pub(crate) fn start(app: &AppHandle) -> io::Result<Self> {
        let data_dir = crate::settings::data_dir()?;
        let (tx, rx) = mpsc::channel();
        let (jobs, work) = mpsc::channel();
        let stopping = Arc::new(AtomicBool::new(false));
        let stop = stopping.clone();
        let replies = tx.clone();
        let worker = thread::Builder::new()
            .name("qms-recognition-insertion".into())
            .spawn(move || {
                let mut recognizer = Recognizer::default();
                while let Ok(job) = work.recv() {
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    let reply = match job {
                        Job::Recognize(model, samples, cleanup) => {
                            let transcript = recognizer
                                .transcribe(model, &samples, |message| {
                                    let _ = replies.send(Command::Error(message));
                                })
                                .map(|(model, text)| {
                                    Transcript::from_dictation(
                                        model,
                                        text,
                                        cleanup,
                                        Store::open_default,
                                    )
                                });
                            Command::Recognized(transcript)
                        }
                        Job::Insert(target, text, cursor) => {
                            let late = replies.clone();
                            let (dispatched, first_reply) = mpsc::channel();
                            let result = target.insert(text, cursor, move |error| {
                                // A reader can release the clipboard the instant the
                                // pending report is sent. Keep UI notifications in order.
                                if first_reply.recv().is_ok() {
                                    let _ = late.send(Command::Error(error.to_string()));
                                }
                            });
                            if replies
                                .send(Command::Inserted(result.map_err(|e| e.to_string())))
                                .is_err()
                            {
                                break;
                            }
                            let _ = dispatched.send(());
                            continue;
                        }
                        Job::RemoveModel(id, done) => {
                            let result = ModelManager::new()
                                .map_err(|e| e.to_string())
                                .and_then(|manager| recognizer.remove_model(&manager, &id));
                            done(result);
                            Command::ModelRemoved
                        }
                    };
                    if replies.send(reply).is_err() {
                        break;
                    }
                }
            })?;
        let levels = tx.clone();
        let faults = tx.clone();
        let mic = Microphone::new(
            move |level| {
                let _ = levels.send(Command::Level(level));
            },
            move |fault| {
                let _ = faults.send(Command::AudioFault(fault));
            },
        );
        let app = app.clone();
        let stop = stopping.clone();
        thread::Builder::new()
            .name("qms-dictation".into())
            .spawn(move || {
                let mut session = Session::new(Desktop {
                    app: app.clone(),
                    mic,
                    reviews: Default::default(),
                    recording_ticket: None,
                    effects: Effects::new(data_dir),
                    mixer_warning: None,
                    jobs,
                    level_pending: Arc::default(),
                    overlay_failure: Arc::default(),
                });
                if let Err(message) = session.ports.effects.recover() {
                    session.error(message);
                }
                let code = run(&mut session, rx, &stop);
                session.cancel();
                // Release the microphone first. A running inference may still need to finish.
                drop(session);
                let _ = worker.join();
                if let Some(code) = code {
                    let flushed = crate::overlay::finish_position_writes(&app);
                    crate::preferences::exit_when_saved(
                        &app,
                        if flushed.is_err() && code == 0 {
                            1
                        } else {
                            code
                        },
                    );
                }
            })?;
        Ok(Self { tx, stopping })
    }

    pub(crate) fn input_handler(&self) -> impl Fn(Action) + Send + Sync + 'static {
        let tx = self.tx.clone();
        move |action| {
            let _ = tx.send(Command::Input(action));
        }
    }

    pub(crate) fn quit(&self, app: &AppHandle, code: i32) {
        self.stopping.store(true, Ordering::Release);
        if self.tx.send(Command::Shutdown(code)).is_err() {
            crate::preferences::exit_when_saved(app, code);
        }
    }

    pub(crate) fn remove_model(
        &self,
        id: String,
        on_done: impl FnOnce(Result<(), String>) + Send + 'static,
    ) -> io::Result<()> {
        self.tx
            .send(Command::RemoveModel(id, Box::new(on_done)))
            .map_err(|_| io::Error::other(&messages().worker_error[..]))
    }
}

pub(crate) fn report_error(app: &AppHandle, message: String) {
    if let Some(controller) = app.try_state::<Controller>() {
        let _ = controller.tx.send(Command::Error(message));
    }
}

fn run(
    session: &mut Session<Desktop>,
    rx: Receiver<Command>,
    stopping: &AtomicBool,
) -> Option<i32> {
    let mut removing_model = false;
    loop {
        let command = if !stopping.load(Ordering::Acquire)
            && let Some(wait) = session.ports.effects.recovery_wait(Instant::now())
        {
            // A stream of foreground notifications must not postpone a due recovery.
            if wait.is_zero() {
                let _ = session.ports.effects.recover();
                continue;
            }
            match rx.recv_timeout(wait) {
                Ok(command) => command,
                Err(RecvTimeoutError::Disconnected) => return None,
                Err(RecvTimeoutError::Timeout) => continue,
            }
        } else {
            rx.recv().ok()?
        };
        match command {
            Command::Shutdown(code) => return Some(code),
            _ if stopping.load(Ordering::Acquire) => continue,
            Command::RemoveModel(id, done) => {
                if removing_model || !matches!(session.phase, Phase::Idle) {
                    done(Err("Das Modell ist beschäftigt. Bitte nach dem laufenden Vorgang erneut versuchen.".into()));
                } else if let Err(mpsc::SendError(Job::RemoveModel(_, done))) =
                    session.ports.jobs.send(Job::RemoveModel(id, done))
                {
                    done(Err(messages().worker_error.clone()));
                } else {
                    removing_model = true;
                }
            }
            Command::ModelRemoved => removing_model = false,
            Command::Input(action) => match action {
                Action::Start(..) if removing_model => {
                    session.error("Das Sprachmodell wird gerade entfernt.".into())
                }
                Action::Start(target, ticket) => {
                    if session.start(target) {
                        session.ports.recording_ticket = Some(ticket);
                    }
                }
                Action::Stop => session.stop(),
                Action::Cancel => session.cancel(),
                Action::Reinsert(target) => session.repeat(target),
                Action::ShowChanges(target) => session.compare(target),
                Action::ComparisonShortcutUpdated => {
                    crate::native_ui::comparison_shortcut_changed(&session.ports.app)
                }
                Action::Elevated => session.elevated(),
                Action::TargetAvailable => session.target_available(),
                Action::ForegroundBlocked => session.discard_target(),
                Action::TargetUnavailable => {
                    session.unavailable(messages().target_unavailable.clone())
                }
                Action::Unavailable => session.unavailable(messages().shortcut_error.clone()),
            },
            Command::InsertOriginal(target, original) => session.insert_original(target, &original),
            Command::Level(level) => {
                if let Err(message) = session.ports.effects.refresh() {
                    session.ports.mixer_warning = Some(message);
                }
                session.level(level);
            }
            Command::AudioFault(fault) => {
                if session.ports.mic.current_fault(&fault) {
                    session.cancel();
                    session.error(fault.message);
                }
            }
            Command::Recognized(result) => session.recognized(result),
            Command::Inserted(result) => session.inserted(result),
            Command::Error(message) => session.error(message),
        }
    }
}
