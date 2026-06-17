//! Atomic helpers for writing files in `.covmap/`. Uses a tmpfile +
//! rename so concurrent readers always see a consistent state.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

pub fn atomic_write(path: &Path, content: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(content.as_bytes())?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}
