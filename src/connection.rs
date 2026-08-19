//! Connections between two coverings.
//!
//! A connection is stored in the SOURCE repo's `.covmap/connections/<name>`
//! and references the target repo by a durable peer alias. File format:
//!
//! ```text
//! # source_covering=<C_A>
//! # target_peer=<alias-or-.>
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
    pub target_peer: String,
    pub target_covering: String,
    pub links: Vec<Link>,
}

#[derive(Clone, Debug)]
pub struct LegacyConnection {
    pub name: String,
    pub source_covering: String,
    pub target_repo: String,
    pub target_covering: String,
    pub links: Vec<Link>,
}

impl Connection {
    pub fn write(&self, repo: &Repo) -> io::Result<()> {
        crate::repo::validate_covering_name(&self.source_covering)?;
        crate::repo::validate_covering_name(&self.target_covering)?;
        let path = repo.connections_dir().join(&self.name);
        atomic_write(&path, &self.serialize())
    }

    pub fn serialize(&self) -> String {
        let mut buf = String::new();
        buf.push_str(&format!("# source_covering={}\n", self.source_covering));
        buf.push_str(&format!("# target_peer={}\n", self.target_peer));
        buf.push_str(&format!("# target_covering={}\n", self.target_covering));
        let mut links = self.links.clone();
        links.sort_by(|a, b| (&a.a, &a.b, &a.note).cmp(&(&b.a, &b.b, &b.note)));
        for p in &links {
            if p.note.is_empty() {
                buf.push_str(&format!("{}\t{}\n", p.a, p.b));
            } else {
                buf.push_str(&format!("{}\t{}\t{}\n", p.a, p.b, p.note));
            }
        }
        buf
    }

    pub fn read(repo: &Repo, name: &str) -> io::Result<Connection> {
        let path = repo.connections_dir().join(name);
        let raw = fs::read_to_string(&path).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!("connection '{name}' not found at {}", path.display()),
            )
        })?;
        Self::parse(name, &raw)
    }

    pub(crate) fn parse(name: &str, raw: &str) -> io::Result<Connection> {
        let mut source_covering = String::new();
        let mut target_peer = String::new();
        let mut target_covering = String::new();
        let mut links = Vec::new();
        for line in raw.lines() {
            if let Some(v) = line.strip_prefix("# source_covering=") {
                source_covering = v.to_string();
            } else if let Some(v) = line.strip_prefix("# target_peer=") {
                target_peer = v.to_string();
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
            ("target_peer", target_peer.as_str()),
            ("target_covering", target_covering.as_str()),
        ] {
            if value.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("connection '{name}' is missing required field '{field}'"),
                ));
            }
        }
        if !crate::peer::valid_alias(&target_peer) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("connection '{name}' has invalid target_peer '{target_peer}'"),
            ));
        }
        for (field, value) in [
            ("source_covering", source_covering.as_str()),
            ("target_covering", target_covering.as_str()),
        ] {
            if !crate::repo::valid_covering_name(value) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("connection '{name}' has invalid {field} '{value}'"),
                ));
            }
        }
        Ok(Connection {
            name: name.to_string(),
            source_covering,
            target_peer,
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
            if !name.starts_with('.') && !crate::store::is_atomic_temp_name(&name) {
                out.push(name);
            }
        }
        out.sort();
        Ok(out)
    }
}

impl LegacyConnection {
    pub fn read(repo: &Repo, name: &str) -> io::Result<LegacyConnection> {
        let path = repo.connections_dir().join(name);
        let raw = fs::read_to_string(&path).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("connection '{name}' not found at {}", path.display()),
            )
        })?;
        Self::parse(name, &raw)
    }

    pub(crate) fn parse(name: &str, raw: &str) -> io::Result<LegacyConnection> {
        let mut source_covering = String::new();
        let mut target_repo = String::new();
        let mut target_covering = String::new();
        let mut links = Vec::new();
        for line in raw.lines() {
            if let Some(value) = line.strip_prefix("# source_covering=") {
                source_covering = value.to_string();
            } else if let Some(value) = line.strip_prefix("# target_repo=") {
                target_repo = value.to_string();
            } else if let Some(value) = line.strip_prefix("# target_covering=") {
                target_covering = value.to_string();
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
        for (field, value) in [
            ("source_covering", source_covering.as_str()),
            ("target_covering", target_covering.as_str()),
        ] {
            if !crate::repo::valid_covering_name(value) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("connection '{name}' has invalid {field} '{value}'"),
                ));
            }
        }
        Ok(LegacyConnection {
            name: name.to_string(),
            source_covering,
            target_repo,
            target_covering,
            links,
        })
    }

    pub fn write(&self, repo: &Repo) -> io::Result<()> {
        atomic_write(&repo.connections_dir().join(&self.name), &self.serialize())
    }

    pub fn serialize(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("# source_covering={}\n", self.source_covering));
        out.push_str(&format!("# target_repo={}\n", self.target_repo));
        out.push_str(&format!("# target_covering={}\n", self.target_covering));
        let mut links = self.links.clone();
        links.sort_by(|a, b| (&a.a, &a.b, &a.note).cmp(&(&b.a, &b.b, &b.note)));
        for link in links {
            if link.note.is_empty() {
                out.push_str(&format!("{}\t{}\n", link.a, link.b));
            } else {
                out.push_str(&format!("{}\t{}\t{}\n", link.a, link.b, link.note));
            }
        }
        out
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
            target_peer: ".".to_string(),
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
        assert!(err.to_string().contains("target_peer"));
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn read_rejects_an_invalid_peer_alias() {
        let repo = temp_repo("peer-alias");
        let path = repo.connections_dir().join("broken");
        fs::write(
            &path,
            "# source_covering=main\n\
             # target_peer=bad alias\n\
             # target_covering=main\n",
        )
        .unwrap();

        let err = Connection::read(&repo, "broken").unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("invalid target_peer"));
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn format3_connection_serialization_is_canonical() {
        let repo = temp_repo("canonical");
        let connection = Connection {
            name: "map".to_string(),
            source_covering: "source".to_string(),
            target_peer: "impl".to_string(),
            target_covering: "target".to_string(),
            links: vec![
                Link {
                    a: "@z".to_string(),
                    b: "@b".to_string(),
                    note: "second".to_string(),
                },
                Link {
                    a: "@a".to_string(),
                    b: "@c".to_string(),
                    note: "first".to_string(),
                },
            ],
        };

        connection.write(&repo).unwrap();
        let raw = fs::read_to_string(repo.connections_dir().join("map")).unwrap();

        assert_eq!(
            raw,
            "# source_covering=source\n\
             # target_peer=impl\n\
             # target_covering=target\n\
             @a\t@c\tfirst\n\
             @z\t@b\tsecond\n"
        );
        let read = Connection::read(&repo, "map").unwrap();
        assert_eq!(read.target_peer, "impl");
        fs::remove_dir_all(repo.root).unwrap();
    }
}
