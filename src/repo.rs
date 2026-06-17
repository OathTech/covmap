//! On-disk layout for a covmap-managed repository.
//!
//! ```text
//! <repo>/
//!   .covmap/
//!     version             # on-disk format version (currently "1")
//!     HEAD                # default covering name
//!     ignore              # optional gitignore-lite patterns
//!     objects/<hh>/<rest> # segment object: "<label>\0<content>"
//!     coverings/<name>    # covered files + segment list
//!     connections/<name>  # connection metadata + pairs
//! ```

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Current on-disk format version. Bumped on incompatible changes to
/// the segment hash, object encoding, covering refs, or connection
/// refs. covmap refuses to operate on a `.covmap/` whose version
/// disagrees.
pub const FORMAT_VERSION: &str = "1";

pub struct Repo {
    pub root: PathBuf,
}

impl Repo {
    /// Walk up from `start` looking for a `.covmap` directory, then
    /// verify its format version.
    pub fn discover(start: &Path) -> io::Result<Repo> {
        let mut p = fs::canonicalize(start)?;
        loop {
            if p.join(".covmap").is_dir() {
                let repo = Repo { root: p };
                repo.check_format_version()?;
                return Ok(repo);
            }
            match p.parent() {
                Some(parent) => p = parent.to_path_buf(),
                None => {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        "no .covmap directory found in this directory or any parent",
                    ))
                }
            }
        }
    }

    pub fn init(root: &Path) -> io::Result<Repo> {
        let dot = root.join(".covmap");
        if dot.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(".covmap already exists at {}", dot.display()),
            ));
        }
        fs::create_dir_all(dot.join("objects"))?;
        fs::create_dir_all(dot.join("coverings"))?;
        fs::create_dir_all(dot.join("connections"))?;
        fs::write(dot.join("version"), format!("{FORMAT_VERSION}\n"))?;
        let mut head = fs::File::create(dot.join("HEAD"))?;
        writeln!(head, "main")?;
        Ok(Repo {
            root: fs::canonicalize(root)?,
        })
    }

    /// Verify `.covmap/version` matches the version this binary
    /// supports. A missing file is treated as an older format
    /// (pre-versioning) and rejected with a clear migration message.
    fn check_format_version(&self) -> io::Result<()> {
        let p = self.dot().join("version");
        let raw = match fs::read_to_string(&p) {
            Ok(s) => s,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        ".covmap is missing version file ({}). \
                         This repo was created by an older covmap; \
                         to upgrade, write `{}` to that file or \
                         re-run `covmap init` on a fresh directory.",
                        p.display(),
                        FORMAT_VERSION
                    ),
                ));
            }
            Err(e) => return Err(e),
        };
        let on_disk = raw.trim();
        if on_disk != FORMAT_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "covmap on-disk format version mismatch: \
                     repo has '{on_disk}', this binary supports '{FORMAT_VERSION}'."
                ),
            ));
        }
        Ok(())
    }

    pub fn dot(&self) -> PathBuf {
        self.root.join(".covmap")
    }
    pub fn objects_dir(&self) -> PathBuf {
        self.dot().join("objects")
    }
    pub fn coverings_dir(&self) -> PathBuf {
        self.dot().join("coverings")
    }
    pub fn connections_dir(&self) -> PathBuf {
        self.dot().join("connections")
    }
    pub fn head_file(&self) -> PathBuf {
        self.dot().join("HEAD")
    }

    pub fn read_head(&self) -> io::Result<String> {
        Ok(fs::read_to_string(self.head_file())?.trim().to_string())
    }
}
