use super::{Phase, Ports, Session, Transcript, with_warning};
use crate::{
    OverlayState, current_settings,
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
    time::Duration,
};
use tauri::{AppHandle, Manager};
use windows::{
    Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW},
    core::HSTRING,
};

type ModelResult = Box<dyn FnOnce(Result<(), String>) + Send>;

enum Command {
    Input(Action),
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
    Recognize(&'static str, Vec<f32>),
    Insert(Target, String, Option<usize>),
    RemoveModel(String, ModelResult),
}

struct Desktop {
    app: AppHandle,
    mic: Microphone,
    recording_ticket: Option<RecordingTicket>,
    effects: Effects,
    mixer_warning: Option<String>,
    jobs: Sender<Job>,
    level_pending: Arc<AtomicBool>,
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
        self.jobs
            .send(Job::Recognize(model, samples))
            .map_err(|_| messages().worker_error.clone())
    }
    fn insert(&mut self, target: Target, text: &str, cursor: Option<usize>) -> Result<(), String> {
        self.jobs
            .send(Job::Insert(target, text.to_owned(), cursor))
            .map_err(|_| messages().worker_error.clone())
    }
    fn show(&mut self, state: OverlayState) {
        let state = with_warning(state, &mut self.mixer_warning, "");
        let listening = matches!(state, OverlayState::Listening { .. });
        if listening && self.level_pending.swap(true, Ordering::AcqRel) {
            return;
        }
        let pending = self.level_pending.clone();
        let app = self.app.clone();
        // The user's key-up must finish audio even when the window thread is busy.
        if self
            .app
            .run_on_main_thread(move || {
                if listening {
                    pending.store(false, Ordering::Release);
                }
                if set_overlay_state(&app, state).is_err() {
                    overlay_failure();
                }
            })
            .is_err()
        {
            self.level_pending.store(false, Ordering::Release);
            overlay_failure();
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
                        Job::Recognize(model, samples) => {
                            let transcript = recognizer
                                .transcribe(model, &samples, |message| {
                                    let _ = replies.send(Command::Error(message));
                                })
                                .map(|(model, text)| {
                                    Transcript::from_dictation(model, text, Store::open_default)
                                });
                            Command::Recognized(transcript)
                        }
                        Job::Insert(target, text, cursor) => Command::Inserted(
                            target.insert(&text, cursor).map_err(|e| e.to_string()),
                        ),
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
                    recording_ticket: None,
                    effects: Effects::new(data_dir),
                    mixer_warning: None,
                    jobs,
                    level_pending: Arc::default(),
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
                    app.exit(code);
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
            app.exit(code);
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
        let command = if session.ports.effects.needs_recovery() {
            match rx.recv_timeout(Duration::from_secs(5)) {
                Ok(command) => command,
                Err(RecvTimeoutError::Disconnected) => return None,
                Err(RecvTimeoutError::Timeout) => {
                    let _ = session.ports.effects.recover();
                    continue;
                }
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
                Action::Elevated => session.elevated(),
                Action::TargetAvailable => session.target_available(),
                Action::TargetUnavailable => {
                    session.cancel();
                    session.error(messages().target_unavailable.clone());
                }
                Action::Unavailable => {
                    session.cancel();
                    session.error(messages().shortcut_error.clone());
                }
            },
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
