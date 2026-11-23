use super::*;
use crate::settings::{CleanupSettings, FillerMode, Settings};
use quatsch_mit_sosse_hotkey::{Hotkey, HotkeyMode};
use windows::Win32::UI::{Controls::EM_SETLIMITTEXT, Input::KeyboardAndMouse::GetFocus};

pub(super) fn cleanup_switch(settings: &mut CleanupSettings, id: u16) -> Option<&mut bool> {
    Some(match id {
        VOICE_COMMANDS => &mut settings.voice_commands,
        USE_DICTIONARY => &mut settings.dictionary,
        SELF_CORRECTIONS => &mut settings.self_corrections,
        WRITE_NUMBERS => &mut settings.numbers,
        PUNCTUATION => &mut settings.punctuation,
        USE_SNIPPETS => &mut settings.snippets,
        _ => return None,
    })
}

pub(super) fn digit_threshold(text: &str) -> Option<u32> {
    (text.len() <= 10)
        .then(|| text.trim().parse().ok())
        .flatten()
}

const FILLER_MODES: [FillerMode; 3] = [FillerMode::Off, FillerMode::Standard, FillerMode::Strong];

fn fill(hwnd: HWND, entries: &[String], selected: usize) {
    send(hwnd, CB_RESETCONTENT, 0, 0);
    for text in entries {
        let text = HSTRING::from(text);
        send(hwnd, CB_ADDSTRING, 0, text.as_ptr() as isize);
    }
    send(hwnd, CB_SETCURSEL, selected, 0);
}

impl Form {
    pub(super) fn comparison_shortcut(&self) {
        let text = self
            .app
            .try_state::<crate::input::Shortcuts>()
            .and_then(|shortcuts| shortcuts.comparison_shortcut())
            .map(|key| {
                LABELS
                    .general_comparison_key
                    .replace("{key}", &key.to_string())
            });
        self.set_text(
            COMPARISON_SHORTCUT,
            text.as_deref()
                .unwrap_or(&LABELS.general_comparison_unavailable),
        );
    }

    pub(super) fn basics(&self, settings: &Settings) {
        self.comparison_shortcut();
        let mut cleanup = settings.cleanup;
        for id in [
            VOICE_COMMANDS,
            USE_DICTIONARY,
            SELF_CORRECTIONS,
            WRITE_NUMBERS,
            PUNCTUATION,
            USE_SNIPPETS,
        ] {
            let checked = *cleanup_switch(&mut cleanup, id).expect("known cleanup switch");
            send(self.control(id), BM_SETCHECK, usize::from(checked), 0);
        }
        let mode = FILLER_MODES
            .iter()
            .position(|&item| item == cleanup.filler_words)
            .expect("known filler mode");
        send(self.control(FILLER_MODE), CB_SETCURSEL, mode, 0);
        if unsafe { GetFocus() } != self.control(DIGITS_FROM) {
            self.set_text(DIGITS_FROM, &cleanup.digits_from.to_string());
        }
        if self.capturing.get() != DICTATION {
            self.set_text(DICTATION, &settings.dictation_hotkey);
        }
        if self.capturing.get() != REINSERT {
            self.set_text(REINSERT, &settings.reinsert_hotkey);
        }
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
        fill(
            self.control(FILLER_MODE),
            &[
                LABELS.cleanup_off.clone(),
                LABELS.cleanup_standard.clone(),
                LABELS.cleanup_strong.clone(),
            ],
            1,
        );
        // Keep an eleventh digit so an overlong paste cannot become a valid prefix.
        send(self.control(DIGITS_FROM), EM_SETLIMITTEXT, 11, 0);
        let volumes = (0..=100).map(|p| format!("{p} %")).collect::<Vec<_>>();
        fill(self.control(VOLUME), &volumes, 20);
        let settings = crate::preferences::display_settings(&self.app)?;
        self.basics(&settings);
        self.microphones(Vec::new(), settings.microphone.as_deref());
        fill(
            self.control(MODEL),
            std::slice::from_ref(&LABELS.general_auto_model),
            0,
        );
        unsafe {
            let _ = EnableWindow(self.control(REMOVE), false);
        }
        Ok(())
    }

    pub(super) fn refresh(&self) -> io::Result<()> {
        let settings = crate::preferences::display_settings(&self.app)?;
        self.basics(&settings);
        self.request_discovery(None)
    }

    fn request_discovery(&self, field: Option<u16>) -> io::Result<()> {
        if unsafe { SetTimer(Some(self.hwnd), DISCOVERY_TIMER, 80, None) } == 0 {
            return Err(io::Error::last_os_error());
        }
        self.app
            .state::<InventoryScan>()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .request(
                (field != Some(MODEL)).then_some(discovery::read_devices),
                (field != Some(MICROPHONE)).then_some(discovery::read_models),
            )
    }

    pub(super) fn poll_discovery(&self) -> io::Result<()> {
        // Keep the rows stable under the pointer while the user chooses an entry.
        let closed = |field| send(self.control(field), CB_GETDROPPEDSTATE, 0, 0).0 == 0;
        let (devices_closed, models_closed) = (closed(MICROPHONE), closed(MODEL));
        let (updates, pending) = {
            let state = self.app.state::<InventoryScan>();
            let mut scans = state.lock().unwrap_or_else(|e| e.into_inner());
            (scans.poll(devices_closed, models_closed), scans.pending())
        };
        if !pending {
            unsafe {
                let _ = KillTimer(Some(self.hwnd), DISCOVERY_TIMER);
            }
        }
        if matches!(updates.devices, Ok(None)) && matches!(updates.models, Ok(None)) {
            return Ok(());
        }
        let settings = crate::preferences::display_settings(&self.app)?;
        let microphones = updates.devices.map(|devices| {
            if let Some(devices) = devices {
                self.microphones(devices, settings.microphone.as_deref());
            }
        });
        let models = updates.models.map(|models| {
            if let Some(models) = models {
                self.show_models(models, settings.model.as_deref());
            }
        });
        microphones.and(models)
    }

    fn microphones(&self, mut ids: Vec<String>, selected: Option<&str>) {
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
    }

    fn show_models(&self, models: Vec<discovery::ModelChoice>, selected: Option<&str>) {
        let mut names = vec![LABELS.general_auto_model.clone()];
        for model in &models {
            let state = if model.installed {
                &LABELS.general_installed
            } else {
                &LABELS.general_not_installed
            };
            names.push(format!("{} · {state}", model.name));
        }
        self.models.replace(models);
        fill(self.control(MODEL), &names, 0);
        self.select_model(selected);
    }

    fn select_model(&self, selected: Option<&str>) {
        let selected = self
            .models
            .borrow()
            .iter()
            .enumerate()
            .find(|(_, model)| selected == Some(model.id.as_str()))
            .map(|(index, model)| (index + 1, model.installed));
        let (index, removable) = selected.unwrap_or_default();
        send(self.control(MODEL), CB_SETCURSEL, index, 0);
        unsafe {
            let _ = EnableWindow(
                self.control(REMOVE),
                removable && self.removal.borrow().is_none(),
            );
        }
    }

    pub(super) fn show_preferences(&self) -> io::Result<()> {
        let settings = crate::preferences::display_settings(&self.app)?;
        self.basics(&settings);
        self.select_model(settings.model.as_deref());
        let index = self
            .devices
            .borrow()
            .iter()
            .position(|id| Some(id.as_str()) == settings.microphone.as_deref())
            .map_or(0, |i| i + 1);
        send(self.control(MICROPHONE), CB_SETCURSEL, index, 0);
        Ok(())
    }

    fn index(&self, id: u16) -> io::Result<usize> {
        usize::try_from(send(self.control(id), CB_GETCURSEL, 0, 0).0)
            .map_err(|_| io::Error::other(&LABELS.general_failure[..]))
    }

    pub(super) fn command(&self, id: u16, notification: u32) -> io::Result<()> {
        if id == DIGITS_FROM && notification == EN_KILLFOCUS {
            let mut buffer = [0u16; 12];
            let len = unsafe { GetWindowTextW(self.control(id), &mut buffer) } as usize;
            let text = String::from_utf16_lossy(&buffer[..len]);
            let previous = crate::preferences::display_settings(&self.app)?
                .cleanup
                .digits_from;
            let value = digit_threshold(&text).ok_or_else(|| {
                self.set_text(DIGITS_FROM, &previous.to_string());
                io::Error::other(&LABELS.cleanup_invalid_threshold[..])
            })?;
            if previous != value {
                crate::apply_settings(&self.app, move |s| s.cleanup.digits_from = value)?;
            }
        } else if notification == CBN_DROPDOWN {
            if matches!(id, MICROPHONE | MODEL) {
                self.request_discovery(Some(id))?;
            }
        } else if notification == CBN_SELCHANGE {
            let index = self.index(id)?;
            match id {
                FILLER_MODE => {
                    let mode = *FILLER_MODES
                        .get(index)
                        .ok_or_else(|| io::Error::other(&LABELS.general_failure[..]))?;
                    crate::apply_settings(&self.app, move |s| s.cleanup.filler_words = mode)?;
                }
                MODE => crate::apply_settings(&self.app, move |s| {
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
                    crate::apply_settings(&self.app, move |s| s.microphone.clone_from(&value))?;
                }
                MODEL => {
                    let value = index
                        .checked_sub(1)
                        .and_then(|i| self.models.borrow().get(i).map(|m| m.id.clone()));
                    crate::apply_settings(&self.app, move |s| s.model.clone_from(&value))?;
                    self.select_model(
                        crate::preferences::display_settings(&self.app)?
                            .model
                            .as_deref(),
                    );
                }
                VOLUME => {
                    let percent = u8::try_from(index).map_err(io::Error::other)?;
                    crate::apply_settings(&self.app, move |s| s.volume_percent = percent)?;
                }
                _ => {}
            }
        } else if notification == BN_CLICKED {
            match id {
                VOICE_COMMANDS | USE_DICTIONARY | SELF_CORRECTIONS | WRITE_NUMBERS
                | PUNCTUATION | USE_SNIPPETS => {
                    let checked = send(self.control(id), BM_GETCHECK, 0, 0).0 == 0;
                    crate::apply_settings(&self.app, move |s| {
                        *cleanup_switch(&mut s.cleanup, id).expect("known cleanup switch") =
                            checked;
                    })?;
                    self.basics(&crate::preferences::display_settings(&self.app)?);
                }
                TONES | AUTOSTART | SHOW_READY | HIDE_FROM_CAPTURE => {
                    let checked = send(self.control(id), BM_GETCHECK, 0, 0).0 == 0;
                    crate::apply_settings(&self.app, move |s| {
                        let value = match id {
                            TONES => &mut s.tones,
                            AUTOSTART => &mut s.autostart,
                            SHOW_READY => &mut s.overlay.show_ready,
                            _ => &mut s.overlay.hide_from_capture,
                        };
                        *value = checked;
                    })?;
                    self.basics(&crate::preferences::display_settings(&self.app)?);
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
                        let device = crate::preferences::display_settings(&self.app)?.microphone;
                        let probe = probe::Probe::start(device)?;
                        if unsafe { SetTimer(Some(self.hwnd), PROBE_TIMER, 80, None) } == 0 {
                            return Err(io::Error::last_os_error());
                        }
                        self.probe.replace(Some(probe));
                        self.set_text(TEST_MIC, &LABELS.general_stop_test);
                        self.set_text(STATUS, &LABELS.general_test_active);
                    }
                }
                DOWNLOAD => crate::model_window::show_for(
                    &self.app,
                    crate::preferences::display_settings(&self.app)?.model,
                )
                .map_err(io::Error::other)?,
                REMOVE => self.remove_model()?,
                _ => {}
            }
        }
        Ok(())
    }

    fn remove_model(&self) -> io::Result<()> {
        let Some(id) = crate::preferences::display_settings(&self.app)?.model else {
            return Ok(());
        };
        let name = self
            .models
            .borrow()
            .iter()
            .find(|m| m.id == id)
            .map(|m| m.name.clone())
            .ok_or_else(|| io::Error::other(&LABELS.general_failure[..]))?;
        let prompt = LABELS.general_confirm_remove.replace("{name}", &name);
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
        self.request_discovery(Some(MODEL))?;
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
            if let Ok(settings) = crate::preferences::display_settings(&self.app) {
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
        let mut candidate = crate::preferences::display_settings(&self.app)?;
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
        crate::apply_settings(&self.app, move |settings| {
            if id == DICTATION {
                settings.dictation_hotkey.clone_from(&value);
            } else {
                settings.reinsert_hotkey.clone_from(&value);
            }
        })?;
        self.basics(&crate::preferences::display_settings(&self.app)?);
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
