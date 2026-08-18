//! On-disk layout for a covmap-managed repository.
//!
//! ```text
//! <repo>/
//!   .covmap/
//!     .gitignore          # tracked default publication policy
//!     version             # on-disk format version (currently "3")
//!     HEAD                # default covering name
//!     ignore              # optional gitignore-lite patterns
//!     objects/<hh>/<rest> # local segment-content cache
//!     coverings/<name>    # covered files + segment list
//!     connections/<name>  # connection metadata + links
//!     local/peers         # checkout-local peer bindings
//!     iterators/<name>    # frozen worklist + per-task state + cursor
//! ```

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Current on-disk format version. Bumped on incompatible changes to
/// the segment hash, object encoding, covering refs, or connection
/// refs. covmap refuses to operate on a `.covmap/` whose version
/// disagrees.
pub const FORMAT_VERSION: &str = "3";

#[derive(Clone, Debug)]
pub struct Repo {
    pub root: PathBuf,
}

impl Repo {
    /// Walk up from `start` looking for a `.covmap` directory, then
    /// verify its format version.
    pub fn discover(start: &Path) -> io::Result<Repo> {
        let repo = Repo::discover_unchecked(start)?;
        repo.check_format_version()?;
        Ok(repo)
    }

    /// Like `discover`, but without the format-version check. Used by
    /// `covmap migrate`, which must open a repo of an *older* format in
    /// order to upgrade it.
    pub fn discover_unchecked(start: &Path) -> io::Result<Repo> {
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

    /// The repo's on-disk format version string (trimmed), or an error if
    /// the `version` file is missing/unreadable.
    pub fn read_version(&self) -> io::Result<String> {
        let p = self.dot().join("version");
        Ok(fs::read_to_string(p)?.trim().to_string())
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
        fs::create_dir_all(dot.join("iterators"))?;
        fs::create_dir_all(dot.join("local"))?;
        fs::write(dot.join(".gitignore"), crate::repo::DEFAULT_GITIGNORE)?;
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
            let hint = if matches!(on_disk, "1" | "2") {
                " Run `covmap migrate` to upgrade it to format 3."
            } else {
                ""
            };
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "covmap on-disk format version mismatch: \
                     repo has '{on_disk}', this binary supports '{FORMAT_VERSION}'.{hint}"
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
    /// Directory of iterator refs. Purely additive (an older covmap never
    /// reads it), so it needs no `FORMAT_VERSION` bump. A repo initialized
    /// before iterators existed simply lacks the directory; the iterator
    /// commands create it on first write via `atomic_write`.
    pub fn iterators_dir(&self) -> PathBuf {
        self.dot().join("iterators")
    }
    pub fn local_dir(&self) -> PathBuf {
        self.dot().join("local")
    }
    pub fn peers_file(&self) -> PathBuf {
        self.local_dir().join("peers")
    }
    pub fn peer_rename_file(&self) -> PathBuf {
        self.local_dir().join("peer-rename")
    }
    pub fn head_file(&self) -> PathBuf {
        self.dot().join("HEAD")
    }

    pub fn read_head(&self) -> io::Result<String> {
        Ok(fs::read_to_string(self.head_file())?.trim().to_string())
    }
}

pub const DEFAULT_GITIGNORE: &str = "\
# Local and reconstructible covmap state.
# Commit .covmap/coverings, .covmap/connections, and covmap metadata.
/local/
/objects/
/iterators/
/*.html
*.tmp
";

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(tag: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("covmap-repo-{tag}-{}-{nonce}", std::process::id()))
    }

    #[test]
    fn init_creates_format3_publication_policy() {
        let root = temp_root("init");
        fs::create_dir_all(&root).unwrap();

        let repo = Repo::init(&root).unwrap();

        assert_eq!(repo.read_version().unwrap(), "3");
        assert_eq!(
            fs::read_to_string(repo.dot().join(".gitignore")).unwrap(),
            DEFAULT_GITIGNORE
        );
        assert!(repo.local_dir().is_dir());
        fs::remove_dir_all(root).unwrap();
    }
}
