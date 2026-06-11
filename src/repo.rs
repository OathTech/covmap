//! On-disk layout for a covmap-managed repository.
//!
//! ```text
//! <repo>/
//!   .covmap/
//!     HEAD                # default covering name
//!     ignore              # optional gitignore-lite patterns
//!     objects/<hh>/<rest> # segment object: "<label>\0<content>"
//!     coverings/<name>    # covered files + segment list
//!     connections/<name>  # connection metadata + pairs
//! ```

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub struct Repo {
    pub root: PathBuf,
}

impl Repo {
    /// Walk up from `start` looking for a `.covmap` directory.
    pub fn discover(start: &Path) -> io::Result<Repo> {
        let mut p = fs::canonicalize(start)?;
        loop {
            if p.join(".covmap").is_dir() {
                return Ok(Repo { root: p });
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
        let mut head = fs::File::create(dot.join("HEAD"))?;
        writeln!(head, "main")?;
        Ok(Repo {
            root: fs::canonicalize(root)?,
        })
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
