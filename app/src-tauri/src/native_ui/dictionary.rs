use super::*;
use quatsch_mit_sosse_store::{DictionaryDraft, DictionaryEntry, Source, Store};
use std::{fs::File, path::PathBuf, sync::mpsc, thread};

mod files;

pub(super) enum Action {
    Load,
    Save(Option<i64>, DictionaryDraft),
    Delete(i64),
    Import(PathBuf),
    Export(PathBuf),
}

pub(super) struct Update {
    entries: Option<Vec<DictionaryEntry>>,
    select: Option<i64>,
    message: String,
}

impl Action {
    pub(super) fn run(self, store: &mut Store) -> io::Result<Update> {
        let mut select = None;
        let message = match self {
            Self::Load => String::new(),
            Self::Save(id, draft) => {
                select = Some(match id {
                    Some(id) => {
                        store.update(id, &draft).map_err(io::Error::other)?;
                        id
                    }
                    None => store.add(&draft).map_err(io::Error::other)?,
                });
                LABELS.dictionary_saved.clone()
            }
            Self::Delete(id) => {
                store.delete(id).map_err(io::Error::other)?;
                LABELS.dictionary_deleted.clone()
            }
            Self::Import(path) => {
                let file = File::open(path).map_err(|_| files::file_error())?;
                let count = store.import_csv(file).map_err(io::Error::other)?;
                LABELS
                    .dictionary_imported
                    .replace("{count}", &count.to_string())
            }
            Self::Export(path) => {
                files::export(store, &path)?;
                return Ok(Update {
                    entries: None,
                    select,
                    message: LABELS.dictionary_exported.clone(),
                });
            }
        };
        Ok(Update {
            entries: Some(store.list("").map_err(io::Error::other)?),
            select,
            message,
        })
    }
}

#[derive(Default)]
pub(super) struct Page {
    entries: RefCell<Vec<DictionaryEntry>>,
    selected: Cell<Option<usize>>,
    pending: RefCell<Option<mpsc::Receiver<io::Result<Update>>>>,
    ready: Cell<bool>,
}

fn windows_lines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\n', "\r\n")
}

impl Form {
    fn show_dictionary(&self, show: bool) -> io::Result<()> {
        self.cancel_capture();
        self.end_probe();
        for field in FIELDS.iter() {
            let visibility = if on_dictionary_page(field.id) == show {
                SW_SHOWNA
            } else {
                SW_HIDE
            };
            unsafe {
                let _ = ShowWindow(self.control(field.id), visibility);
            }
        }
        self.scroll.set(0);
        layout::layout(self)?;
        let focus = if show { SEARCH } else { OPEN_DICTIONARY };
        unsafe {
            let _ = SetFocus(Some(self.control(focus)));
        }
        if show && self.dictionary.pending.borrow().is_none() {
            self.dictionary.ready.set(false);
            self.dictionary.entries.borrow_mut().clear();
            self.edit_entry(None);
            self.set_text(SEARCH, "");
            self.filter_entries()?;
            self.start_dictionary(Action::Load)?;
        }
        Ok(())
    }

    fn edit_text(&self, id: u16) -> String {
        let control = self.control(id);
        let length = unsafe { GetWindowTextLengthW(control) }.max(0) as usize;
        let mut text = vec![0u16; length + 1];
        let count = unsafe { GetWindowTextW(control, &mut text) } as usize;
        String::from_utf16_lossy(&text[..count])
    }

    // A row holds the index into this window's current database snapshot.
    // Filtering never changes the database ID used by edit and delete.
    fn filter_entries(&self) -> io::Result<()> {
        let query = self.edit_text(SEARCH);
        let list = self.control(ENTRIES);
        send(list, LB_RESETCONTENT, 0, 0);
        let selected = self.dictionary.selected.get();
        let mut selection_visible = false;
        for (index, entry) in self.dictionary.entries.borrow().iter().enumerate() {
            if entry.matches(&query) {
                let label = HSTRING::from(&entry.term);
                let row = send(list, LB_ADDSTRING, 0, label.as_ptr() as isize).0;
                if row < 0 || send(list, LB_SETITEMDATA, row as usize, index as isize).0 < 0 {
                    return Err(io::Error::other(&LABELS.dictionary_store_error[..]));
                }
                if selected == Some(index) {
                    send(list, LB_SETCURSEL, row as usize, 0);
                    selection_visible = true;
                }
            }
        }
        if selected.is_some() && !selection_visible {
            self.edit_entry(None);
        }
        Ok(())
    }

    fn edit_entry(&self, index: Option<usize>) {
        let rows = self.dictionary.entries.borrow();
        let entry = index.and_then(|i| rows.get(i));
        self.dictionary
            .selected
            .set(index.filter(|_| entry.is_some()));
        self.set_text(TERM, entry.map_or("", |e| e.term.as_str()));
        self.set_text(
            REPLACEMENT,
            &windows_lines(entry.and_then(|e| e.replacement.as_deref()).unwrap_or("")),
        );
        self.set_text(
            ALIASES,
            &entry.map_or_else(String::new, |e| e.aliases.join("\r\n")),
        );
        self.set_text(
            SAVE,
            if entry.is_some() {
                &LABELS.dictionary_save
            } else {
                &LABELS.dictionary_add
            },
        );
        self.enable_editor();
    }

    fn enable_editor(&self) {
        let enabled = self.dictionary.ready.get() && self.dictionary.pending.borrow().is_none();
        for id in [
            ENTRIES,
            TERM,
            REPLACEMENT,
            ALIASES,
            SAVE,
            NEW,
            DELETE,
            EXPORT,
            IMPORT,
        ] {
            unsafe {
                let _ = EnableWindow(
                    self.control(id),
                    enabled && (id != DELETE || self.dictionary.selected.get().is_some()),
                );
            }
        }
    }

    fn start_dictionary(&self, action: Action) -> io::Result<()> {
        if unsafe { SetTimer(Some(self.hwnd), DICTIONARY_TIMER, 40, None) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let (done, pending) = mpsc::channel();
        thread::Builder::new()
            .name("dictionary-storage".into())
            .spawn(move || {
                let outcome = Store::open_default()
                    .map_err(io::Error::other)
                    .and_then(|mut store| action.run(&mut store));
                let _ = done.send(outcome);
            })?;
        self.dictionary.pending.replace(Some(pending));
        self.set_text(DICTIONARY_STATUS, &LABELS.dictionary_busy);
        self.enable_editor();
        Ok(())
    }

    pub(super) fn poll_dictionary(&self) -> io::Result<()> {
        let received = self
            .dictionary
            .pending
            .borrow()
            .as_ref()
            .map(|rx| rx.try_recv());
        if matches!(received, Some(Err(mpsc::TryRecvError::Empty))) {
            return Ok(());
        }
        unsafe {
            let _ = KillTimer(Some(self.hwnd), DICTIONARY_TIMER);
        }
        self.dictionary.pending.take();
        self.enable_editor();
        let Some(result) = received else {
            return Ok(());
        };
        let update = result.map_err(|_| io::Error::other(&LABELS.dictionary_store_error[..]))??;
        if let Some(rows) = update.entries {
            let selected = update
                .select
                .and_then(|id| rows.iter().position(|e| e.id == id));
            let query = self.edit_text(SEARCH);
            let hidden = selected.is_some_and(|i| !rows[i].matches(&query));
            self.dictionary.entries.replace(rows);
            self.dictionary.ready.set(true);
            if hidden {
                self.set_text(SEARCH, "");
            }
            self.edit_entry(selected);
            self.filter_entries()?;
        }
        self.set_text(DICTIONARY_STATUS, &update.message);
        Ok(())
    }

    pub(super) fn dictionary_command(&self, id: u16, notification: u32) -> io::Result<()> {
        match (id, notification) {
            (OPEN_DICTIONARY, BN_CLICKED) => return self.show_dictionary(true),
            (GENERAL, BN_CLICKED) => return self.show_dictionary(false),
            (SEARCH, EN_CHANGE) => return self.filter_entries(),
            _ => {}
        }
        if !self.dictionary.ready.get() || self.dictionary.pending.borrow().is_some() {
            return Ok(());
        }
        match (id, notification) {
            (ENTRIES, LBN_SELCHANGE) => {
                let list = self.control(ENTRIES);
                let row = send(list, LB_GETCURSEL, 0, 0).0;
                let index = send(list, LB_GETITEMDATA, row as usize, 0).0;
                self.edit_entry(usize::try_from(index).ok());
            }
            (NEW, BN_CLICKED) => {
                send(self.control(ENTRIES), LB_SETCURSEL, usize::MAX, 0);
                self.edit_entry(None);
                self.set_text(DICTIONARY_STATUS, "");
                unsafe {
                    let _ = SetFocus(Some(self.control(TERM)));
                }
            }
            (SAVE, BN_CLICKED) => {
                let replacement = self.edit_text(REPLACEMENT);
                let entries = self.dictionary.entries.borrow();
                let selected = self.dictionary.selected.get().and_then(|i| entries.get(i));
                let draft = DictionaryDraft {
                    term: self.edit_text(TERM),
                    replacement: if let Some(original) =
                        selected.and_then(|e| e.replacement.as_ref())
                        && windows_lines(original) == replacement
                    {
                        Some(original.clone())
                    } else {
                        (!replacement.is_empty()).then(|| replacement.replace("\r\n", "\n"))
                    },
                    aliases: self
                        .edit_text(ALIASES)
                        .lines()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect(),
                    source: selected.map_or(Source::Manual, |e| e.source),
                };
                self.start_dictionary(Action::Save(selected.map(|e| e.id), draft))?;
            }
            (DELETE, BN_CLICKED) => {
                let selected = self.dictionary.selected.get().and_then(|i| {
                    self.dictionary
                        .entries
                        .borrow()
                        .get(i)
                        .map(|e| (e.id, e.term.clone()))
                });
                if let Some((id, term)) = selected {
                    let prompt = LABELS.dictionary_delete_prompt.replace("{term}", &term);
                    if self.confirm(&prompt, &LABELS.dictionary_title) {
                        self.start_dictionary(Action::Delete(id))?;
                    }
                }
            }
            (IMPORT | EXPORT, BN_CLICKED) => {
                if let Some(path) = files::choose(self.hwnd, id == EXPORT)? {
                    self.start_dictionary(if id == EXPORT {
                        Action::Export(path)
                    } else {
                        Action::Import(path)
                    })?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}
