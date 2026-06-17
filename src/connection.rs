//! Connections between two coverings.
//!
//! A connection is stored in the SOURCE repo's `.covmap/connections/<name>`
//! and references the target repo by absolute path. File format:
//!
//! ```text
//! # source_covering=<C_A>
//! # target_repo=<abs/path>
//! # target_covering=<C_B>
//! <hash_A>\t<hash_B>[\t<note>]
//! ...
//! ```

use crate::repo::Repo;
use crate::store::atomic_write;
use std::fs;
use std::io;

#[derive(Clone, Debug)]
pub struct Pair {
    pub a: String,
    pub b: String,
    pub note: String,
}

#[derive(Clone, Debug)]
pub struct Connection {
    pub name: String,
    pub source_covering: String,
    pub target_repo: String,
    pub target_covering: String,
    pub pairs: Vec<Pair>,
}

impl Connection {
    pub fn write(&self, repo: &Repo) -> io::Result<()> {
        let path = repo.connections_dir().join(&self.name);
        let mut buf = String::new();
        buf.push_str(&format!("# source_covering={}\n", self.source_covering));
        buf.push_str(&format!("# target_repo={}\n", self.target_repo));
        buf.push_str(&format!("# target_covering={}\n", self.target_covering));
        for p in &self.pairs {
            if p.note.is_empty() {
                buf.push_str(&format!("{}\t{}\n", p.a, p.b));
            } else {
                buf.push_str(&format!("{}\t{}\t{}\n", p.a, p.b, p.note));
            }
        }
        atomic_write(&path, &buf)
    }

    pub fn read(repo: &Repo, name: &str) -> io::Result<Connection> {
        let path = repo.connections_dir().join(name);
        let raw = fs::read_to_string(&path).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!("connection '{name}' not found at {}", path.display()),
            )
        })?;
        let mut source_covering = String::new();
        let mut target_repo = String::new();
        let mut target_covering = String::new();
        let mut pairs = Vec::new();
        for line in raw.lines() {
            if let Some(v) = line.strip_prefix("# source_covering=") {
                source_covering = v.to_string();
            } else if let Some(v) = line.strip_prefix("# target_repo=") {
                target_repo = v.to_string();
            } else if let Some(v) = line.strip_prefix("# target_covering=") {
                target_covering = v.to_string();
            } else if line.is_empty() || line.starts_with('#') {
                continue;
            } else {
                let parts: Vec<&str> = line.splitn(3, '\t').collect();
                if parts.len() < 2 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("malformed connection line: {line}"),
                    ));
                }
                pairs.push(Pair {
                    a: parts[0].to_string(),
                    b: parts[1].to_string(),
                    note: parts.get(2).copied().unwrap_or("").to_string(),
                });
            }
        }
        Ok(Connection {
            name: name.to_string(),
            source_covering,
            target_repo,
            target_covering,
            pairs,
        })
    }

    pub fn list(repo: &Repo) -> io::Result<Vec<String>> {
        let mut out = Vec::new();
        let dir = repo.connections_dir();
        if !dir.is_dir() {
            return Ok(out);
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with('.') && !name.ends_with(".tmp") {
                out.push(name);
            }
        }
        out.sort();
        Ok(out)
    }
}
