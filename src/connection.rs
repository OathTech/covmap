//! Connections between two coverings.
//!
//! A connection is stored in the SOURCE repo's `.covmap/connections/<name>`
//! and references the target repo by absolute path. File format:
//!
//! ```text
//! # source_covering=<C_A>
//! # target_repo=<abs/path>
//! # target_covering=<C_B>
//! <endpoint_A>\t<endpoint_B>[\t<note>]
//! ...
//! ```

use crate::repo::Repo;
use crate::store::atomic_write;
use std::fs;
use std::io;

#[derive(Clone, Debug)]
pub struct Link {
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
    pub links: Vec<Link>,
}

impl Connection {
    pub fn write(&self, repo: &Repo) -> io::Result<()> {
        let path = repo.connections_dir().join(&self.name);
        let mut buf = String::new();
        buf.push_str(&format!("# source_covering={}\n", self.source_covering));
        buf.push_str(&format!("# target_repo={}\n", self.target_repo));
        buf.push_str(&format!("# target_covering={}\n", self.target_covering));
        for p in &self.links {
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
        let mut links = Vec::new();
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
                links.push(Link {
                    a: parts[0].to_string(),
                    b: parts[1].to_string(),
                    note: parts.get(2).copied().unwrap_or("").to_string(),
                });
            }
        }
        for (field, value) in [
            ("source_covering", source_covering.as_str()),
            ("target_repo", target_repo.as_str()),
            ("target_covering", target_covering.as_str()),
        ] {
            if value.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("connection '{name}' is missing required field '{field}'"),
                ));
            }
        }
        Ok(Connection {
            name: name.to_string(),
            source_covering,
            target_repo,
            target_covering,
            links,
        })
    }

    /// Remove one connection ref after parsing it successfully. Reading first
    /// prevents a malformed ref from being silently discarded.
    pub fn remove(repo: &Repo, name: &str) -> io::Result<Connection> {
        let connection = Self::read(repo, name)?;
        fs::remove_file(repo.connections_dir().join(name))?;
        Ok(connection)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_repo(tag: &str) -> Repo {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "covmap-connection-{tag}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join(".covmap/connections")).unwrap();
        Repo { root }
    }

    #[test]
    fn remove_reads_then_deletes_only_the_connection_ref() {
        let repo = temp_repo("remove");
        let keep = repo.dot().join("keep");
        fs::write(&keep, "untouched").unwrap();
        let connection = Connection {
            name: "map".to_string(),
            source_covering: "source".to_string(),
            target_repo: repo.root.to_string_lossy().to_string(),
            target_covering: "target".to_string(),
            links: vec![Link {
                a: "@left".to_string(),
                b: "@right".to_string(),
                note: "why".to_string(),
            }],
        };
        connection.write(&repo).unwrap();

        let removed = Connection::remove(&repo, "map").unwrap();

        assert_eq!(removed.links.len(), 1);
        assert!(!repo.connections_dir().join("map").exists());
        assert_eq!(fs::read_to_string(keep).unwrap(), "untouched");
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn remove_preserves_a_malformed_connection_ref() {
        let repo = temp_repo("malformed");
        let path = repo.connections_dir().join("broken");
        fs::write(&path, "# source_covering=main\nnot-a-link\n").unwrap();

        let err = Connection::remove(&repo, "broken").unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(path.exists());
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn read_rejects_missing_connection_metadata() {
        let repo = temp_repo("metadata");
        let path = repo.connections_dir().join("broken");
        fs::write(&path, "# source_covering=main\n# target_covering=main\n").unwrap();

        let err = Connection::read(&repo, "broken").unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("target_repo"));
        fs::remove_dir_all(repo.root).unwrap();
    }
}
