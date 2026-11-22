use std::{fs, path::Path};

use rusqlite::{Connection, MAIN_DB, OpenFlags, TransactionBehavior, backup::NO_PROGRESS};

use crate::{Error, Result};
use quatsch_mit_sosse_textfx::snippet_trigger_key;
use rusqlite::functions::FunctionFlags;

const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/001_dictionary.sql"),
    include_str!("../migrations/002_snippets.sql"),
];

fn version(db: &Connection) -> Result<usize> {
    let version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
    usize::try_from(version)
        .ok()
        .filter(|n| *n <= MIGRATIONS.len())
        .ok_or(Error::UnsupportedVersion)
}

pub(super) fn migrate(db: &mut Connection, path: &Path) -> Result<()> {
    while version(db)? < MIGRATIONS.len() {
        // Reserve the next write before choosing the version and its snapshot.
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = version(&tx)?;
        let Some(sql) = MIGRATIONS.get(current) else {
            break;
        };
        backup(path, current).map_err(|_| Error::Backup)?;
        if current == 1 {
            tx.create_scalar_function(
                "qms_trigger_key",
                1,
                FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
                |ctx| Ok(snippet_trigger_key(ctx.get_raw(0).as_str()?)),
            )?;
        }
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (current + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

fn backup(path: &Path, current: usize) -> Result<()> {
    let folder = path.with_file_name("backups");
    fs::create_dir_all(&folder).map_err(|_| Error::File)?;
    let name = path.file_stem().ok_or(Error::DataPath)?.to_string_lossy();
    let file = tempfile::Builder::new()
        .prefix(&format!("{name}-v{current}-"))
        .suffix(".tmp")
        .tempfile_in(folder)
        .map_err(|_| Error::File)?;
    // SQLite's online backup includes committed WAL pages. A second connection
    // can read the snapshot while the migration connection holds its write lock.
    let snapshot = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    snapshot.backup(MAIN_DB, file.path(), NO_PROGRESS)?;
    file.as_file().sync_all().map_err(|_| Error::File)?;
    let target = file.path().with_extension("db");
    file.persist_noclobber(target).map_err(|_| Error::File)?;
    Ok(())
}
