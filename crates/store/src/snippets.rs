use std::{
    collections::HashSet,
    io::{Read, Write},
};

use quatsch_mit_sosse_textfx::snippet_trigger_key;
use rusqlite::{Connection, Row, Transaction, TransactionBehavior, params};
use serde::Deserialize;

use crate::{Error, Result, Store, check_changed, csv_io, key};

const HEADERS: [&str; 3] = ["trigger", "text", "created_at"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnippetEntry {
    pub id: i64,
    pub trigger: String,
    pub text: String,
    /// UTC seconds since the Unix epoch.
    pub created_at: i64,
}

impl SnippetEntry {
    fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            trigger: row.get(1)?,
            text: row.get(2)?,
            created_at: row.get(3)?,
        })
    }

    pub fn matches(&self, query: &str) -> bool {
        self.matches_key(&key(query.trim()))
    }

    fn matches_key(&self, query: &str) -> bool {
        query.is_empty() || key(&self.trigger).contains(query) || key(&self.text).contains(query)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnippetDraft {
    pub trigger: String,
    pub text: String,
}

impl SnippetDraft {
    fn checked_key(&self) -> Result<String> {
        let key = snippet_trigger_key(&self.trigger);
        if key.is_empty() || self.trigger.contains(['\r', '\n', '\0']) || self.text.contains('\0') {
            return Err(Error::InvalidSnippet);
        }
        Ok(key)
    }

    fn save(
        &self,
        db: &Connection,
        id: Option<i64>,
        key: &str,
        created_at: Option<i64>,
    ) -> Result<i64> {
        let trigger = self.trigger.trim();
        if let Some(id) = id {
            check_changed(db.execute(
                "UPDATE snippets SET trigger=?1,trigger_key=?2,text=?3,created_at=coalesce(?4,created_at) WHERE id=?5",
                params![trigger, key, self.text, created_at, id],
            )?)?;
            Ok(id)
        } else {
            db.execute(
                "INSERT INTO snippets (trigger,trigger_key,text,created_at) VALUES (?1,?2,?3,coalesce(?4,unixepoch()))",
                params![trigger, key, self.text, created_at],
            )?;
            Ok(db.last_insert_rowid())
        }
    }
}

fn find_trigger(
    db: &Connection,
    key: &str,
    trigger: &str,
    exact_only: bool,
) -> Result<Option<i64>> {
    let mut statement = db.prepare_cached(
        "SELECT id, trigger=?2 AS exact FROM snippets
         WHERE trigger_key=?1 AND (NOT ?3 OR trigger=?2) ORDER BY exact DESC, id LIMIT 2",
    )?;
    let mut rows = statement.query(params![key, trigger.trim(), exact_only])?;
    let Some(first) = rows.next()? else {
        return Ok(None);
    };
    let id = first.get(0)?;
    if !first.get::<_, bool>(1)? && rows.next()?.is_some() {
        return Err(Error::DuplicateTrigger);
    }
    Ok(Some(id))
}

impl Store {
    pub fn list_snippets(&self, query: &str) -> Result<Vec<SnippetEntry>> {
        let query = key(query.trim());
        let mut statement = self.db.prepare(
            "SELECT id, trigger, text, created_at FROM snippets ORDER BY trigger_key, id",
        )?;
        let mut entries = Vec::new();
        for row in statement.query_map([], SnippetEntry::read)? {
            let entry = row?;
            if entry.matches_key(&query) {
                entries.push(entry);
            }
        }
        Ok(entries)
    }

    pub fn add_snippet(&self, draft: &SnippetDraft) -> Result<i64> {
        self.save_snippet(None, draft)
    }

    pub fn update_snippet(&self, id: i64, draft: &SnippetDraft) -> Result<()> {
        self.save_snippet(Some(id), draft).map(|_| ())
    }

    fn save_snippet(&self, id: Option<i64>, draft: &SnippetDraft) -> Result<i64> {
        let key = draft.checked_key()?;
        let tx = Transaction::new_unchecked(&self.db, TransactionBehavior::Immediate)?;
        if let Some(existing) = find_trigger(&tx, &key, &draft.trigger, false)?
            && Some(existing) != id
        {
            return Err(Error::DuplicateTrigger);
        }
        let id = draft.save(&tx, id, &key, None)?;
        tx.commit()?;
        Ok(id)
    }

    pub fn delete_snippet(&self, id: i64) -> Result<()> {
        check_changed(self.db.execute("DELETE FROM snippets WHERE id=?1", [id])?)
    }

    pub fn import_snippets_csv(&mut self, reader: impl Read) -> Result<usize> {
        #[derive(Deserialize)]
        struct CsvSnippet {
            trigger: String,
            text: String,
            created_at: Option<i64>,
        }

        let mut reader = csv::Reader::from_reader(reader);
        let headers = reader.headers().map_err(|_| Error::InvalidCsv)?;
        csv_io::validate_headers(headers, &["trigger", "text"], &HEADERS)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut imported = 0;
        let mut seen = HashSet::new();
        for row in reader.deserialize() {
            let entry: CsvSnippet = row.map_err(|_| Error::InvalidCsv)?;
            let draft = SnippetDraft {
                trigger: entry.trigger,
                text: entry.text,
            };
            let key = draft.checked_key()?;
            // If a file explicitly contains distinct spellings of one spoken
            // trigger, keep both: databases migrated from version 1 can contain them.
            let repeated = !seen.insert(key.clone());
            let id = find_trigger(&tx, &key, &draft.trigger, repeated)?;
            draft.save(&tx, id, &key, entry.created_at)?;
            imported += 1;
        }
        tx.commit()?;
        Ok(imported)
    }

    pub fn export_snippets_csv(&self, writer: impl Write) -> Result<()> {
        let mut csv = csv_io::export_writer(writer, &HEADERS)?;
        for entry in self.list_snippets("")? {
            csv.serialize((entry.trigger, entry.text, entry.created_at))
                .map_err(|_| Error::File)?;
        }
        csv.flush().map_err(|_| Error::File)
    }
}
