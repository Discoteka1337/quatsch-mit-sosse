mod csv_io;
mod migration;
mod snippets;

pub use snippets::{SnippetDraft, SnippetEntry};

use std::{
    fs,
    path::{Path, PathBuf},
};

use icu_normalizer::ComposingNormalizerBorrowed;
use rusqlite::{Connection, Row, Statement, params};
use serde::{Deserialize, Serialize};

pub type Result<T> = std::result::Result<T, Error>;

const INSERT_ENTRY: &str =
    "INSERT INTO dictionary (term, term_key, replacement, aliases, source, created_at)
    VALUES (?1, ?2, ?3, ?4, ?5, coalesce(?6, unixepoch()))";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("LOCALAPPDATA enthält keinen absoluten Datenpfad.")]
    DataPath,
    #[error("Die Datei konnte nicht gelesen oder geschrieben werden.")]
    File,
    #[error("Die lokale Datenbank konnte nicht gelesen oder geschrieben werden.")]
    Database,
    #[error("Die Sicherung vor der Datenbankmigration ist fehlgeschlagen.")]
    Backup,
    #[error("Diese Datenbankversion wird von der App nicht unterstützt.")]
    UnsupportedVersion,
    #[error(
        "Begriff und einzelne Varianten müssen Text auf jeweils einer Zeile enthalten. Nullzeichen sind unzulässig."
    )]
    InvalidEntry,
    #[error("Der Auslöser braucht Text auf einer Zeile. Nullzeichen sind unzulässig.")]
    InvalidSnippet,
    #[error("Diesen Auslöser gibt es bereits.")]
    DuplicateTrigger,
    #[error("Diesen Begriff gibt es bereits.")]
    Duplicate,
    #[error("Der Eintrag ist nicht mehr vorhanden.")]
    MissingEntry,
    #[error("Die CSV-Datei enthält ungültige Spalten oder Werte.")]
    InvalidCsv,
}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        if error
            .sqlite_error()
            .is_some_and(|e| e.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE)
        {
            Self::Duplicate
        } else {
            Self::Database
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    #[default]
    Manual,
    Auto,
}

impl Source {
    fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Auto => "auto",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DictionaryEntry {
    pub id: i64,
    pub term: String,
    pub replacement: Option<String>,
    pub aliases: Vec<String>,
    pub source: Source,
    /// UTC seconds since the Unix epoch.
    pub created_at: i64,
}

fn key(value: &str) -> String {
    ComposingNormalizerBorrowed::new_nfc()
        .normalize(value)
        .to_lowercase()
}

impl DictionaryEntry {
    pub fn matches(&self, query: &str) -> bool {
        self.matches_key(&key(query.trim()))
    }

    fn matches_key(&self, query: &str) -> bool {
        query.is_empty()
            || std::iter::once(&self.term)
                .chain(self.replacement.iter())
                .chain(&self.aliases)
                .any(|text| key(text).contains(query))
    }

    fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        let aliases: String = row.get(3)?;
        Ok(Self {
            id: row.get(0)?,
            term: row.get(1)?,
            replacement: row.get(2)?,
            aliases: serde_json::from_str(&aliases).map_err(|_| rusqlite::Error::InvalidQuery)?,
            source: match row.get_ref(4)?.as_str()? {
                "manual" => Source::Manual,
                "auto" => Source::Auto,
                _ => return Err(rusqlite::Error::InvalidQuery),
            },
            created_at: row.get(5)?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DictionaryDraft {
    pub term: String,
    pub replacement: Option<String>,
    pub aliases: Vec<String>,
    pub source: Source,
}

impl DictionaryDraft {
    fn encoded_aliases(&self) -> Result<String> {
        let invalid = |s: &str| s.trim().is_empty() || s.contains(['\0', '\r', '\n']);
        if invalid(&self.term)
            || self.aliases.iter().any(|s| invalid(s))
            || self.replacement.as_ref().is_some_and(|s| s.contains('\0'))
        {
            return Err(Error::InvalidEntry);
        }
        let aliases: Vec<_> = self.aliases.iter().map(|s| s.trim()).collect();
        serde_json::to_string(&aliases).map_err(|_| Error::InvalidEntry)
    }

    fn replacement(&self) -> Option<&str> {
        self.replacement.as_deref().filter(|s| !s.is_empty())
    }

    fn insert(&self, statement: &mut Statement<'_>, created_at: Option<i64>) -> Result<()> {
        statement.execute(params![
            self.term.trim(),
            key(self.term.trim()),
            self.replacement(),
            self.encoded_aliases()?,
            self.source.as_str(),
            created_at,
        ])?;
        Ok(())
    }
}

pub struct Store {
    db: Connection,
}

impl Store {
    pub fn open_default() -> Result<Self> {
        let root = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .ok_or(Error::DataPath)?;
        Self::open(
            root.join("QuatschMitSosse")
                .join("data")
                .join("quatsch-mit-sosse.db"),
        )
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let parent = path.parent().ok_or(Error::DataPath)?;
        fs::create_dir_all(parent).map_err(|_| Error::File)?;
        let mut db = Connection::open(path)?;
        migration::migrate(&mut db, path)?;
        Ok(Self { db })
    }

    pub fn list(&self, query: &str) -> Result<Vec<DictionaryEntry>> {
        let query = key(query.trim());
        let mut statement = self.db.prepare(
            "SELECT id, term, replacement, aliases, source, created_at FROM dictionary ORDER BY term_key"
        )?;
        let mut entries = Vec::new();
        for row in statement.query_map([], DictionaryEntry::read)? {
            let entry = row?;
            if entry.matches_key(&query) {
                entries.push(entry);
            }
        }
        Ok(entries)
    }

    pub fn add(&self, draft: &DictionaryDraft) -> Result<i64> {
        draft.insert(&mut self.db.prepare(INSERT_ENTRY)?, None)?;
        Ok(self.db.last_insert_rowid())
    }

    pub fn update(&self, id: i64, draft: &DictionaryDraft) -> Result<()> {
        check_changed(self.db.execute(
            "UPDATE dictionary SET term=?1, term_key=?2, replacement=?3, aliases=?4, source=?5 WHERE id=?6",
            params![
                draft.term.trim(),
                key(draft.term.trim()),
                draft.replacement(),
                draft.encoded_aliases()?,
                draft.source.as_str(),
                id,
            ],
        )?)
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        check_changed(
            self.db
                .execute("DELETE FROM dictionary WHERE id=?1", [id])?,
        )
    }
}

fn check_changed(count: usize) -> Result<()> {
    if count == 0 {
        Err(Error::MissingEntry)
    } else {
        Ok(())
    }
}
