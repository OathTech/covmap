//! Atomic helpers for writing files in `.covmap/`. Uses a tmpfile +
//! rename so concurrent readers always see a consistent state.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

pub const ATOMIC_TEMP_PREFIX: &str = "#covmap-tmp-";

pub fn is_atomic_temp_name(name: &str) -> bool {
    name.starts_with(ATOMIC_TEMP_PREFIX) && name.ends_with(".tmp")
}

pub fn atomic_write(path: &Path, content: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("atomic write path has no file name: {}", path.display()),
        )
    })?;
    let mut temp_name = OsString::from(ATOMIC_TEMP_PREFIX);
    temp_name.push(file_name);
    temp_name.push(".tmp");
    let tmp = path.with_file_name(temp_name);
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(content.as_bytes())?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}
