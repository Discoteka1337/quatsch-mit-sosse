use std::io::{Read, Write};

use rusqlite::TransactionBehavior;
use serde::Deserialize;

use crate::{DictionaryDraft, Error, INSERT_ENTRY, Result, Source, Store};

const HEADERS: [&str; 5] = ["term", "replacement", "aliases", "source", "created_at"];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CsvEntry {
    term: String,
    replacement: Option<String>,
    aliases: Option<String>,
    source: Option<Source>,
    created_at: Option<i64>,
}

pub(super) fn validate_headers(
    headers: &csv::StringRecord,
    required: &[&str],
    allowed: &[&str],
) -> Result<()> {
    if required
        .iter()
        .any(|name| !headers.iter().any(|h| h == *name))
        || headers
            .iter()
            .enumerate()
            .any(|(i, h)| !allowed.contains(&h) || headers.iter().take(i).any(|p| p == h))
    {
        return Err(Error::InvalidCsv);
    }
    Ok(())
}

pub(super) fn export_writer<W: Write>(writer: W, headers: &[&str]) -> Result<csv::Writer<W>> {
    let mut csv = csv::WriterBuilder::new()
        .has_headers(false)
        .terminator(csv::Terminator::CRLF)
        .from_writer(writer);
    csv.write_record(headers).map_err(|_| Error::File)?;
    Ok(csv)
}

impl Store {
    pub fn import_csv(&mut self, reader: impl Read) -> Result<usize> {
        let mut reader = csv::Reader::from_reader(reader);
        let headers = reader.headers().map_err(|_| Error::InvalidCsv)?;
        validate_headers(headers, &["term"], &HEADERS)?;
        // Only fields present in the file may overwrite an existing entry.
        let mut sql = format!(
            "{INSERT_ENTRY} ON CONFLICT(term_key) DO UPDATE SET
            term=excluded.term, created_at=coalesce(?6, dictionary.created_at)"
        );
        for column in ["replacement", "aliases", "source"] {
            if headers.iter().any(|header| header == column) {
                sql.push_str(&format!(", {column}=excluded.{column}"));
            }
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut statement = tx.prepare(&sql)?;
        let mut imported = 0;
        for row in reader.deserialize() {
            let entry: CsvEntry = row.map_err(|_| Error::InvalidCsv)?;
            let draft = DictionaryDraft {
                term: entry.term,
                replacement: entry.replacement,
                aliases: serde_json::from_str(entry.aliases.as_deref().unwrap_or("[]"))
                    .map_err(|_| Error::InvalidCsv)?,
                source: entry.source.unwrap_or_default(),
            };
            draft.insert(&mut statement, entry.created_at)?;
            imported += 1;
        }
        drop(statement);
        tx.commit()?;
        Ok(imported)
    }

    pub fn export_csv(&self, writer: impl Write) -> Result<()> {
        let mut csv = export_writer(writer, &HEADERS)?;
        for entry in self.list("")? {
            let aliases = serde_json::to_string(&entry.aliases).map_err(|_| Error::InvalidEntry)?;
            csv.serialize((
                entry.term,
                entry.replacement,
                aliases,
                entry.source,
                entry.created_at,
            ))
            .map_err(|_| Error::File)?;
        }
        csv.flush().map_err(|_| Error::File)
    }
}
