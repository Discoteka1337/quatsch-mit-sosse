use super::*;
use crate::settings::Settings;
use quatsch_mit_sosse_audio::input_devices;
use quatsch_mit_sosse_hotkey::{Hotkey, HotkeyMode};
use quatsch_mit_sosse_models::{Manager as Models, ModelKind};

fn fill(hwnd: HWND, entries: &[String], selected: usize) {
    send(hwnd, CB_RESETCONTENT, 0, 0);
    for text in entries {
        let text = HSTRING::from(text);
        send(hwnd, CB_ADDSTRING, 0, text.as_ptr() as isize);
    }
    send(hwnd, CB_SETCURSEL, selected, 0);
}

impl Form {
    pub(super) fn basics(&self, settings: &Settings) {
        self.set_text(DICTATION, &settings.dictation_hotkey);
        self.set_text(REINSERT, &settings.reinsert_hotkey);
        send(
            self.control(MODE),
            CB_SETCURSEL,
            usize::from(settings.mode == HotkeyMode::Toggle),
            0,
        );
        send(
            self.control(VOLUME),
            CB_SETCURSEL,
            usize::from(settings.volume_percent),
            0,
        );
        for (id, checked) in [
            (TONES, settings.tones),
            (AUTOSTART, settings.autostart),
            (SHOW_READY, settings.overlay.show_ready),
            (HIDE_FROM_CAPTURE, settings.overlay.hide_from_capture),
        ] {
            send(self.control(id), BM_SETCHECK, usize::from(checked), 0);
        }
    }

    pub(super) fn initialize(&self) -> io::Result<()> {
        fill(
            self.control(MODE),
            &[LABELS.general_hold.clone(), LABELS.general_toggle.clone()],
            0,
        );
        let volumes = (0..=100).map(|p| format!("{p} %")).collect::<Vec<_>>();
        fill(self.control(VOLUME), &volumes, 20);
        self.basics(&crate::current_settings(&self.app)?);
        Ok(())
    }

    pub(super) fn refresh(&self) -> io::Result<()> {
        let settings = crate::current_settings(&self.app)?;
        self.basics(&settings);
        let microphones = self.microphones(settings.microphone.as_deref());
        let models = self.models(settings.model.as_deref());
        microphones.and(models)
    }

    fn microphones(&self, selected: Option<&str>) -> io::Result<()> {
        let devices = input_devices();
        let mut ids = devices
            .as_ref()
            .map(|devices| devices.iter().map(|d| d.name.clone()).collect::<Vec<_>>())
            .unwrap_or_default();
        let mut names = vec![LABELS.general_default_device.clone()];
        names.extend(ids.iter().cloned());
        if let Some(missing) = selected
            && !ids.iter().any(|id| id == missing)
        {
            ids.push(missing.to_owned());
            names.push(format!("{missing} · {}", LABELS.general_missing));
        }
        let index = ids
            .iter()
            .position(|id| Some(id.as_str()) == selected)
            .map_or(0, |i| i + 1);
        self.devices.replace(ids);
        fill(self.control(MICROPHONE), &names, index);
        devices.map(|_| ()).map_err(io::Error::other)
    }

    fn models(&self, selected: Option<&str>) -> io::Result<()> {
        let manager = Models::new().map_err(io::Error::other)?;
        let mut names = vec![LABELS.general_auto_model.clone()];
        let mut ids = Vec::new();
        let mut index = 0;
        let mut removable = false;
        for model in manager
            .list()
            .iter()
            .filter(|model| model.kind == ModelKind::Asr)
        {
            let installed = manager.is_installed(&model.id).map_err(io::Error::other)?;
            let state = if installed {
                &LABELS.general_installed
            } else {
                &LABELS.general_not_installed
            };
            if selected == Some(model.id.as_str()) {
                index = ids.len() + 1;
                removable = installed;
            }
            names.push(format!("{} · {state}", model.display_name));
            ids.push(model.id.clone());
        }
        self.model_ids.replace(ids);
        fill(self.control(MODEL), &names, index);
        unsafe {
            let _ = EnableWindow(
                self.control(REMOVE),
                removable && self.removal.borrow().is_none(),
            );
        }
        Ok(())
    }

    fn index(&self, id: u16) -> io::Result<usize> {
        usize::try_from(send(self.control(id), CB_GETCURSEL, 0, 0).0)
            .map_err(|_| io::Error::other(&LABELS.general_failure[..]))
    }

    pub(super) fn command(&self, id: u16, notification: u32) -> io::Result<()> {
        if notification == CBN_DROPDOWN {
            let settings = crate::current_settings(&self.app)?;
            match id {
                MICROPHONE => self.microphones(settings.microphone.as_deref())?,
                MODEL => self.models(settings.model.as_deref())?,
                _ => {}
            }
        } else if notification == CBN_SELCHANGE {
            let index = self.index(id)?;
            match id {
                MODE => crate::apply_settings(&self.app, |s| {
                    s.mode = if index == 0 {
                        HotkeyMode::Hold
                    } else {
                        HotkeyMode::Toggle
                    }
                })?,
                MICROPHONE => {
                    self.end_probe();
                    let value = index
                        .checked_sub(1)
                        .and_then(|i| self.devices.borrow().get(i).cloned());
                    crate::apply_settings(&self.app, |s| s.microphone = value)?;
                }
                MODEL => {
                    let value = index
                        .checked_sub(1)
                        .and_then(|i| self.model_ids.borrow().get(i).cloned());
                    crate::apply_settings(&self.app, |s| s.model = value.clone())?;
                    self.models(value.as_deref())?;
                }
                VOLUME => {
                    let percent = u8::try_from(index).map_err(io::Error::other)?;
                    crate::apply_settings(&self.app, |s| s.volume_percent = percent)?;
                }
                _ => {}
            }
        } else if notification == BN_CLICKED {
            match id {
                TONES | AUTOSTART | SHOW_READY | HIDE_FROM_CAPTURE => {
                    crate::apply_settings(&self.app, |s| {
                        let value = match id {
                            TONES => &mut s.tones,
                            AUTOSTART => &mut s.autostart,
                            SHOW_READY => &mut s.overlay.show_ready,
                            _ => &mut s.overlay.hide_from_capture,
                        };
                        *value = !*value;
                    })?;
                    self.basics(&crate::current_settings(&self.app)?);
                }
                DICTATION | REINSERT => {
                    self.cancel_capture();
                    crate::capture_shortcuts(&self.app, true);
                    self.capturing.set(id);
                    self.chord.replace(Chord::default());
                    self.set_text(id, &LABELS.general_record);
                    unsafe {
                        let _ = SetFocus(Some(self.control(id)));
                    }
                }
                TEST_MIC => {
                    if self.probe.borrow().is_some() {
                        self.end_probe();
                    } else {
                        let device = crate::current_settings(&self.app)?.microphone;
                        let probe = probe::Probe::start(device)?;
                        if unsafe { SetTimer(Some(self.hwnd), PROBE_TIMER, 80, None) } == 0 {
                            return Err(io::Error::last_os_error());
                        }
                        self.probe.replace(Some(probe));
                        self.set_text(TEST_MIC, &LABELS.general_stop_test);
                        self.set_text(STATUS, &LABELS.general_test_active);
                    }
                }
                DOWNLOAD => crate::model_window::show(&self.app).map_err(io::Error::other)?,
                REMOVE => self.remove_model()?,
                _ => {}
            }
        }
        Ok(())
    }

    fn remove_model(&self) -> io::Result<()> {
        let Some(id) = crate::current_settings(&self.app)?.model else {
            return Ok(());
        };
        let manager = Models::new().map_err(io::Error::other)?;
        let model = manager
            .list()
            .iter()
            .find(|m| m.id == id)
            .ok_or_else(|| io::Error::other(&LABELS.general_failure[..]))?;
        let prompt = LABELS
            .general_confirm_remove
            .replace("{name}", &model.display_name);
        if self.confirm(&prompt, &LABELS.general_model) {
            if unsafe { SetTimer(Some(self.hwnd), REMOVE_TIMER, 100, None) } == 0 {
                return Err(io::Error::last_os_error());
            }
            let (done, result) = std::sync::mpsc::channel();
            self.removal.replace(Some(result));
            unsafe {
                let _ = EnableWindow(self.control(REMOVE), false);
            }
            self.app
                .state::<crate::dictation::Controller>()
                .remove_model(id, move |outcome| {
                    let _ = done.send(outcome);
                })?;
        }
        Ok(())
    }

    pub(super) fn poll_removal(&self) -> io::Result<()> {
        let result = self
            .removal
            .borrow()
            .as_ref()
            .map(|receiver| receiver.try_recv());
        let result = match result {
            Some(Ok(outcome)) => outcome.map_err(io::Error::other),
            Some(Err(std::sync::mpsc::TryRecvError::Empty)) => return Ok(()),
            _ => Err(io::Error::other(&LABELS.general_failure[..])),
        };
        unsafe {
            let _ = KillTimer(Some(self.hwnd), REMOVE_TIMER);
        }
        self.removal.take();
        self.models(crate::current_settings(&self.app)?.model.as_deref())?;
        result?;
        self.set_text(STATUS, &LABELS.general_removed);
        Ok(())
    }

    pub(super) fn end_probe(&self) {
        if let Some(probe) = self.probe.take() {
            unsafe {
                let _ = KillTimer(Some(self.hwnd), PROBE_TIMER);
            }
            self.set_text(STATUS, probe.result());
            self.set_text(TEST_MIC, &LABELS.general_microphone_test);
            send(self.control(LEVEL), PBM_SETPOS, 0, 0);
        }
    }

    pub(super) fn cancel_capture(&self) {
        let id = self.capturing.replace(0);
        if id != 0 {
            crate::capture_shortcuts(&self.app, false);
            if let Ok(settings) = crate::current_settings(&self.app) {
                self.set_text(
                    id,
                    if id == DICTATION {
                        &settings.dictation_hotkey
                    } else {
                        &settings.reinsert_hotkey
                    },
                );
            }
        }
    }

    pub(super) fn capture_key(&self, key: u32, pressed: bool) -> io::Result<()> {
        if key == 0x1b && pressed {
            self.cancel_capture();
            return Ok(());
        }
        let result = self.chord.borrow_mut().key(key, pressed);
        if let Some(binding) = result {
            let id = self.capturing.get();
            self.cancel_capture();
            match binding {
                None => self.set_text(STATUS, &LABELS.general_unsupported),
                Some(binding) => self.apply_binding(id, binding)?,
            }
        }
        Ok(())
    }

    fn apply_binding(&self, id: u16, binding: Hotkey) -> io::Result<()> {
        let value = binding.to_string();
        let mut candidate = crate::current_settings(&self.app)?;
        if id == DICTATION {
            candidate.dictation_hotkey.clone_from(&value);
        } else {
            candidate.reinsert_hotkey.clone_from(&value);
        }
        candidate.hotkeys()?;
        let mut warnings = binding
            .warnings()
            .into_iter()
            .map(|w| w.to_string())
            .collect::<Vec<_>>();
        if !warnings.is_empty() {
            warnings.push(LABELS.general_keep.clone());
            if !self.confirm(&warnings.join("\n\n"), &LABELS.general_conflict) {
                return Ok(());
            }
        }
        crate::apply_settings(&self.app, |settings| {
            if id == DICTATION {
                settings.dictation_hotkey = value;
            } else {
                settings.reinsert_hotkey = value;
            }
        })?;
        self.basics(&crate::current_settings(&self.app)?);
        self.set_text(STATUS, "");
        Ok(())
    }

    pub(super) fn confirm(&self, prompt: &str, title: &str) -> bool {
        unsafe {
            MessageBoxW(
                Some(self.hwnd),
                &HSTRING::from(prompt),
                &HSTRING::from(title),
                MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2,
            ) == IDYES
        }
    }
}
