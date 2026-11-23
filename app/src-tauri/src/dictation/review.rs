use crate::{
    comparison::DictationRecord,
    comparison_window::{self, PreparedComparison},
    input::Target,
    messages,
};
use std::{
    io,
    sync::{Arc, Weak},
    thread,
};
use tauri::{AppHandle, Manager};

#[derive(Default)]
pub(super) struct Requests {
    pending: Weak<()>,
}

impl Requests {
    // Keep one ticket through preparation and queued UI delivery. A closed
    // dispatcher or a failed worker spawn releases it just like a completed open.
    pub(super) fn start(&mut self, job: impl FnOnce(Arc<()>) + Send + 'static) -> io::Result<bool> {
        if self.pending.upgrade().is_some() {
            return Ok(false);
        }
        let ticket = Arc::new(());
        self.pending = Arc::downgrade(&ticket);
        thread::Builder::new()
            .name("qms-comparison".into())
            .spawn(move || job(ticket))?;
        Ok(true)
    }

    pub(super) fn open(
        &mut self,
        app: AppHandle,
        target: Target,
        record: Arc<DictationRecord>,
        hide_from_capture: bool,
        insert: impl FnOnce(Target, String) + Send + 'static,
    ) -> Result<(), String> {
        self.start(move |ticket| {
            let view = PreparedComparison::new(record);
            let handle = app.clone();
            let queued = app.run_on_main_thread(move || {
                let _ticket = ticket;
                let notify = handle.clone();
                let opened = handle
                    .get_window("overlay")
                    .and_then(|window| window.hwnd().ok())
                    .is_some_and(|overlay| {
                        comparison_window::show(
                            overlay,
                            hide_from_capture,
                            view,
                            target,
                            Box::new(insert),
                            Box::new(move |message| {
                                super::report_error(&notify, message.to_owned())
                            }),
                        )
                        .is_ok()
                    });
                if !opened {
                    super::report_error(&handle, messages().comparison_open_error.clone());
                }
            });
            if queued.is_err() {
                super::report_error(&app, messages().comparison_open_error.clone());
            }
        })
        .map(|_| ())
        .map_err(|_| messages().comparison_open_error.clone())
    }
}
