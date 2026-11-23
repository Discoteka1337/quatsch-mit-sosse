use std::{
    fs::File,
    io::{self, Read},
    path::Path,
};

const MAX_BYTES: u64 = 1024 * 1024;

fn check_size(bytes: u64) -> io::Result<()> {
    if bytes > MAX_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Die lokale Konfigurationsdatei überschreitet die Größenbegrenzung von 1 MiB.",
        ));
    }
    Ok(())
}

pub(super) fn read(path: &Path) -> io::Result<Vec<u8>> {
    let file = File::open(path)?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    check_size(bytes.len() as u64)?;
    Ok(bytes)
}

/// Check the completed temporary file before publishing a configuration that
/// needs to remain readable on the next launch.
pub(super) fn sync(file: &File) -> io::Result<()> {
    check_size(file.metadata()?.len())?;
    file.sync_all()
}
