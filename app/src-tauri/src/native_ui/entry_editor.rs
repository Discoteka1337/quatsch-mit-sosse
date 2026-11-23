use super::*;
use quatsch_mit_sosse_store::{
    DictionaryDraft, DictionaryEntry, SnippetDraft, SnippetEntry, Source, Store,
};
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    thread,
};

pub(super) mod clipboard;
mod filter;
use filter::{Filter, ListUpdate, RowChange};

#[cfg(test)]
mod clipboard_tests;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Collection {
    Dictionary,
    Snippets,
}

impl Collection {
    fn offset(self) -> u16 {
        match self {
            Self::Dictionary => 0,
            Self::Snippets => SNIPPET_OFFSET,
        }
    }

    pub(super) fn id(self, base: u16) -> u16 {
        base + self.offset()
    }

    pub(super) fn for_field(id: u16) -> Option<Self> {
        match id {
            300..400 => Some(Self::Dictionary),
            400..500 => Some(Self::Snippets),
            _ => None,
        }
    }

    pub(super) fn title(self) -> &'static str {
        match self {
            Self::Dictionary => &LABELS.dictionary_title,
            Self::Snippets => &LABELS.snippets_title,
        }
    }

    pub(super) fn search_label(self) -> &'static str {
        match self {
            Self::Dictionary => &LABELS.dictionary_search,
            Self::Snippets => &LABELS.snippets_search,
        }
    }

    fn timer(self) -> usize {
        match self {
            Self::Dictionary => DICTIONARY_TIMER,
            Self::Snippets => SNIPPETS_TIMER,
        }
    }

    fn store_error(self) -> io::Error {
        io::Error::other(match self {
            Self::Dictionary => LABELS.dictionary_store_error.as_str(),
            Self::Snippets => LABELS.snippets_store_error.as_str(),
        })
    }
}

enum Entry {
    Term(DictionaryEntry),
    Snippet(SnippetEntry),
}

impl Entry {
    fn id(&self) -> i64 {
        match self {
            Self::Term(e) => e.id,
            Self::Snippet(e) => e.id,
        }
    }

    fn label(&self) -> &str {
        match self {
            Self::Term(e) => &e.term,
            Self::Snippet(e) => &e.trigger,
        }
    }

    fn text(&self) -> Option<&str> {
        match self {
            Self::Term(e) => e.replacement.as_deref(),
            Self::Snippet(e) => Some(&e.text),
        }
    }

    fn matches(&self, query: &str) -> bool {
        match self {
            Self::Term(e) => e.matches(query),
            Self::Snippet(e) => e.matches(query),
        }
    }
}

pub(super) enum Action {
    Load,
    SaveTerm(Option<i64>, DictionaryDraft),
    SaveSnippet(Option<i64>, SnippetDraft),
    Delete(i64),
    Import(PathBuf),
    Export(PathBuf),
}

pub(super) struct Update {
    entries: Option<Vec<Entry>>,
    select: Option<usize>,
    message: String,
}

impl Action {
    pub(super) fn run(self, collection: Collection, store: &mut Store) -> io::Result<Update> {
        let mut select = None;
        let message = match self {
            Self::Load => String::new(),
            Self::SaveTerm(id, draft) => {
                select = Some(match id {
                    Some(id) => {
                        store.update(id, &draft).map_err(io::Error::other)?;
                        id
                    }
                    None => store.add(&draft).map_err(io::Error::other)?,
                });
                LABELS.dictionary_saved.clone()
            }
            Self::SaveSnippet(id, draft) => {
                select = Some(match id {
                    Some(id) => {
                        store.update_snippet(id, &draft).map_err(io::Error::other)?;
                        id
                    }
                    None => store.add_snippet(&draft).map_err(io::Error::other)?,
                });
                LABELS.dictionary_saved.clone()
            }
            Self::Delete(id) => {
                match collection {
                    Collection::Dictionary => store.delete(id),
                    Collection::Snippets => store.delete_snippet(id),
                }
                .map_err(io::Error::other)?;
                LABELS.dictionary_deleted.clone()
            }
            Self::Import(path) => {
                let file = csv_files::open(&path)?;
                let count = match collection {
                    Collection::Dictionary => store.import_csv(file),
                    Collection::Snippets => store.import_snippets_csv(file),
                }
                .map_err(io::Error::other)?;
                LABELS
                    .dictionary_imported
                    .replace("{count}", &count.to_string())
            }
            Self::Export(path) => {
                csv_files::export(&path, |file| match collection {
                    Collection::Dictionary => store.export_csv(file),
                    Collection::Snippets => store.export_snippets_csv(file),
                })?;
                return Ok(Update {
                    entries: None,
                    select: None,
                    message: LABELS.dictionary_exported.clone(),
                });
            }
        };
        let entries: Vec<_> = match collection {
            Collection::Dictionary => store
                .list("")
                .map_err(io::Error::other)?
                .into_iter()
                .map(Entry::Term)
                .collect(),
            Collection::Snippets => store
                .list_snippets("")
                .map_err(io::Error::other)?
                .into_iter()
                .map(Entry::Snippet)
                .collect(),
        };
        Ok(Update {
            select: select.and_then(|id| entries.iter().position(|entry| entry.id() == id)),
            entries: Some(entries),
            message,
        })
    }
}

#[derive(Default)]
pub(super) struct Page {
    entries: RefCell<Arc<Vec<Entry>>>,
    selected: Cell<Option<usize>>,
    pending: RefCell<Option<mpsc::Receiver<io::Result<Update>>>>,
    filter: RefCell<Filter>,
    render: RefCell<Option<ListUpdate>>,
    reveal_saved: Cell<bool>,
    list_ready: Cell<bool>,
    ready: Cell<bool>,
}

pub(super) fn windows_lines(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\r\n")
}

pub(super) fn edited_text(original: Option<&str>, displayed: &str) -> String {
    match original {
        Some(original) if windows_lines(original) == displayed => original.to_owned(),
        _ => displayed.replace("\r\n", "\n"),
    }
}

pub(super) struct Editor<'a> {
    form: &'a Form,
    page: &'a Page,
    collection: Collection,
}

impl Form {
    pub(super) fn editor(&self, collection: Collection) -> Editor<'_> {
        let page = match collection {
            Collection::Dictionary => &self.dictionary,
            Collection::Snippets => &self.snippets,
        };
        Editor {
            form: self,
            page,
            collection,
        }
    }
}

impl Editor<'_> {
    fn control(&self, id: u16) -> HWND {
        self.form.control(self.collection.id(id))
    }

    pub(super) fn set_status(&self, text: &str) {
        self.set_text(EDITOR_STATUS, text);
    }

    fn set_text(&self, id: u16, text: &str) {
        self.form.set_text(self.collection.id(id), text);
    }

    fn show(&self, show: bool) -> io::Result<()> {
        self.form.cancel_capture();
        self.form.end_probe();
        let page = show.then_some(self.collection);
        for field in FIELDS.iter() {
            let visibility = if Collection::for_field(field.id) == page {
                SW_SHOWNA
            } else {
                SW_HIDE
            };
            unsafe {
                let _ = ShowWindow(self.form.control(field.id), visibility);
            }
        }
        self.form.scroll.set(0);
        layout::layout(self.form)?;
        let focus = if show {
            self.collection.id(SEARCH)
        } else {
            match self.collection {
                Collection::Dictionary => OPEN_DICTIONARY,
                Collection::Snippets => OPEN_SNIPPETS,
            }
        };
        unsafe {
            let _ = SetFocus(Some(self.form.control(focus)));
        }
        if show && self.page.pending.borrow().is_none() {
            self.page.ready.set(false);
            self.page.list_ready.set(false);
            self.page.filter.take();
            self.page.render.take();
            self.page.entries.take();
            self.edit_entry(None);
            self.set_text(SEARCH, "");
            self.start(Action::Load)?;
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
        if !self.page.ready.get() {
            return Ok(());
        }
        self.page.list_ready.set(false);
        self.enable_editor();
        self.start_timer()?;
        self.page.render.take();
        self.page
            .filter
            .borrow_mut()
            .request(self.page.entries.borrow().clone(), self.edit_text(SEARCH))?;
        Ok(())
    }

    fn poll_filter(&self) -> io::Result<()> {
        let list = self.control(ENTRIES);
        let found = self.page.filter.borrow_mut().poll()?;
        if let Some(indices) = found {
            let reveal_saved = self.page.reveal_saved.replace(false);
            if self
                .page
                .selected
                .get()
                .is_some_and(|i| indices.binary_search(&i).is_err())
            {
                if reveal_saved {
                    self.set_text(SEARCH, "");
                    return Ok(());
                }
                self.edit_entry(None);
            }
            let visible = usize::try_from(send(list, LB_GETCOUNT, 0, 0).0)
                .map_err(|_| self.collection.store_error())?;
            self.page
                .render
                .replace(Some(ListUpdate::new(visible, indices)));
        }
        if let Some(mut update) = self.page.render.take() {
            update.tick(|change| {
                match change {
                    RowChange::Remove(row) => {
                        if send(list, LB_DELETESTRING, row, 0).0 < 0 {
                            return Err(self.collection.store_error());
                        }
                    }
                    RowChange::Append(index) => {
                        let entries = self.page.entries.borrow();
                        let label = HSTRING::from(entries[index].label());
                        let row = send(list, LB_ADDSTRING, 0, label.as_ptr() as isize).0;
                        if row < 0 || send(list, LB_SETITEMDATA, row as usize, index as isize).0 < 0
                        {
                            return Err(self.collection.store_error());
                        }
                        if self.page.selected.get() == Some(index) {
                            send(list, LB_SETCURSEL, row as usize, 0);
                        }
                    }
                }
                Ok(())
            })?;
            if !update.finished() {
                self.page.render.replace(Some(update));
            } else {
                self.page.list_ready.set(true);
                self.enable_editor();
            }
        }
        Ok(())
    }

    fn edit_entry(&self, index: Option<usize>) {
        let rows = self.page.entries.borrow();
        let entry = index.and_then(|i| rows.get(i));
        self.page.selected.set(index.filter(|_| entry.is_some()));
        self.set_text(TERM, entry.map_or("", Entry::label));
        self.set_text(
            REPLACEMENT,
            &windows_lines(entry.and_then(Entry::text).unwrap_or("")),
        );
        if self.collection == Collection::Dictionary {
            let aliases = match entry {
                Some(Entry::Term(term)) => term.aliases.join("\r\n"),
                _ => String::new(),
            };
            self.set_text(ALIASES, &aliases);
        }
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
        let enabled = self.can_edit();
        for id in [
            ENTRIES,
            TERM,
            REPLACEMENT,
            SAVE,
            NEW,
            DELETE,
            EXPORT,
            IMPORT,
        ]
        .into_iter()
        .chain((self.collection == Collection::Dictionary).then_some(ALIASES))
        {
            unsafe {
                let _ = EnableWindow(
                    self.control(id),
                    enabled && (id != DELETE || self.page.selected.get().is_some()),
                );
            }
        }
    }

    fn can_edit(&self) -> bool {
        self.page.ready.get() && self.page.pending.borrow().is_none() && self.page.list_ready.get()
    }

    fn start_timer(&self) -> io::Result<()> {
        if unsafe { SetTimer(Some(self.form.hwnd), self.collection.timer(), 16, None) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn start(&self, action: Action) -> io::Result<()> {
        let collection = self.collection;
        self.start_timer()?;
        let (done, pending) = mpsc::channel();
        thread::Builder::new()
            .name("entry-storage".into())
            .spawn(move || {
                let outcome = Store::open_default()
                    .map_err(io::Error::other)
                    .and_then(|mut store| action.run(collection, &mut store));
                let _ = done.send(outcome);
            })?;
        self.page.pending.replace(Some(pending));
        self.set_status(match collection {
            Collection::Dictionary => &LABELS.dictionary_busy,
            Collection::Snippets => &LABELS.snippets_busy,
        });
        self.enable_editor();
        Ok(())
    }

    pub(super) fn poll(&self) -> io::Result<()> {
        let result = self.poll_storage().and_then(|()| self.poll_filter());
        if self.page.pending.borrow().is_none()
            && !self.page.filter.borrow().busy()
            && self.page.render.borrow().is_none()
        {
            unsafe {
                let _ = KillTimer(Some(self.form.hwnd), self.collection.timer());
            }
        }
        result
    }

    fn poll_storage(&self) -> io::Result<()> {
        let received = self.page.pending.borrow().as_ref().map(|rx| rx.try_recv());
        let Some(result) = received else {
            return Ok(());
        };
        if matches!(result, Err(mpsc::TryRecvError::Empty)) {
            return Ok(());
        }
        self.page.pending.take();
        self.enable_editor();
        let update = result.map_err(|_| self.collection.store_error())??;
        if let Some(rows) = update.entries {
            self.page.list_ready.set(false);
            self.page.entries.replace(Arc::new(rows));
            self.page.ready.set(true);
            self.page.reveal_saved.set(update.select.is_some());
            self.edit_entry(update.select);
            self.filter_entries()?;
        }
        self.set_text(EDITOR_STATUS, &update.message);
        Ok(())
    }

    pub(super) fn command(&self, id: u16, notification: u32) -> io::Result<()> {
        if matches!(id, OPEN_DICTIONARY | OPEN_SNIPPETS) {
            return if notification == BN_CLICKED {
                self.show(true)
            } else {
                Ok(())
            };
        }
        let id = id - self.collection.offset();
        match (id, notification) {
            (GENERAL, BN_CLICKED) => return self.show(false),
            (SEARCH, EN_CHANGE) => return self.filter_entries(),
            _ => {}
        }
        if !self.can_edit() {
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
                self.set_text(EDITOR_STATUS, "");
                unsafe {
                    let _ = SetFocus(Some(self.control(TERM)));
                }
            }
            (SAVE, BN_CLICKED) => {
                let replacement = self.edit_text(REPLACEMENT);
                let entries = self.page.entries.borrow();
                let selected = self.page.selected.get().and_then(|i| entries.get(i));
                let text = edited_text(selected.and_then(Entry::text), &replacement);
                let id = selected.map(Entry::id);
                let term = self.edit_text(TERM);
                let action = match self.collection {
                    Collection::Dictionary => Action::SaveTerm(
                        id,
                        DictionaryDraft {
                            term,
                            replacement: (!text.is_empty()).then_some(text),
                            aliases: self
                                .edit_text(ALIASES)
                                .lines()
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                                .map(str::to_owned)
                                .collect(),
                            source: match selected {
                                Some(Entry::Term(entry)) => entry.source,
                                _ => Source::Manual,
                            },
                        },
                    ),
                    Collection::Snippets => Action::SaveSnippet(
                        id,
                        SnippetDraft {
                            trigger: term,
                            text,
                        },
                    ),
                };
                self.start(action)?;
            }
            (DELETE, BN_CLICKED) => {
                let selected = self.page.selected.get().and_then(|i| {
                    self.page
                        .entries
                        .borrow()
                        .get(i)
                        .map(|e| (e.id(), e.label().to_owned()))
                });
                if let Some((id, term)) = selected {
                    let prompt = match self.collection {
                        Collection::Dictionary => &LABELS.dictionary_delete_prompt,
                        Collection::Snippets => &LABELS.snippets_delete_prompt,
                    }
                    .replace("{term}", &term);
                    if self.form.confirm(&prompt, self.collection.title()) {
                        self.start(Action::Delete(id))?;
                    }
                }
            }
            (IMPORT | EXPORT, BN_CLICKED) => {
                if let Some(path) = csv_files::choose(self.form.hwnd, id == EXPORT)? {
                    self.start(if id == EXPORT {
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
