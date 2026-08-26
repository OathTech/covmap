//! Resolving the link relation for inspection.
//!
//! covmap's value to a reader is the *relation* between segments, but a
//! connection stores each link as a pair of opaque endpoints (`@name` or
//! hash) into two coverings. To *understand* a segment you want its links
//! with the far side already resolved to `covering:label loc` — the
//! "combined view" that cip 0003 (bounded drill-down) is built on.
//!
//! This module walks the locally stored connections visible from a covering
//! and resolves every endpoint. Both the CLI (`show`, `ls`) and relationship
//! views consume it, so they agree about the relation.
//!
//! A source segment's **outgoing** links point at target segments; a target
//! segment's **incoming** links point back at source segments. Same-repository
//! incoming links are discoverable because their connection ref is local.
//! Cross-repository incoming links remain discoverable only from the source
//! repository that stores the connection (see cip 0003 "Known limit").

use crate::connection::Connection;
use crate::covering::{self, Covering};
use crate::peer;
use crate::repo::Repo;
use std::hash::{Hash, Hasher};
use std::io;

/// One end of a resolved link — the segment on the *far* side, described
/// well enough to read and to address, or marked broken when it no longer
/// resolves.
#[derive(Clone, Debug)]
pub struct ResolvedEnd {
    /// Connection this link belongs to.
    pub conn: String,
    /// Covering the far endpoint lives in.
    pub covering: String,
    /// The far segment's label, when the endpoint resolved.
    pub label: Option<String>,
    /// The far segment's location (`file:a..b`), when it resolved.
    pub loc: Option<String>,
    /// The far segment's `@name`, when it has one.
    pub name: Option<String>,
    /// The far endpoint's hash (resolved current hash, or the raw endpoint
    /// string when broken).
    pub hash: String,
    /// The link note.
    pub note: String,
    /// Did the far endpoint resolve in its covering?
    pub resolved: bool,
    /// Was the far covering available for endpoint verification?
    pub checked: bool,
}

/// The resolved links of one segment: outgoing (this segment is a source)
/// and incoming (this segment is a target).
#[derive(Clone, Debug, Default)]
pub struct SegLinks {
    pub outgoing: Vec<ResolvedEnd>,
    pub incoming: Vec<ResolvedEnd>,
}

#[derive(Clone, Debug, Eq)]
struct OccurrenceKey {
    file: String,
    a: usize,
    a_col: Option<usize>,
    b: usize,
    b_col: Option<usize>,
}

impl PartialEq for OccurrenceKey {
    fn eq(&self, other: &Self) -> bool {
        self.file == other.file
            && self.a == other.a
            && self.a_col == other.a_col
            && self.b == other.b
            && self.b_col == other.b_col
    }
}

impl Hash for OccurrenceKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.file.hash(state);
        self.a.hash(state);
        self.a_col.hash(state);
        self.b.hash(state);
        self.b_col.hash(state);
    }
}

fn occurrence_key(entry: &covering::CoveringEntry) -> OccurrenceKey {
    OccurrenceKey {
        file: entry.file.clone(),
        a: entry.a,
        a_col: entry.a_col,
        b: entry.b,
        b_col: entry.b_col,
    }
}

/// The resolved relation visible for one covering in `repo`. It includes
/// outgoing links from locally stored connections that use the covering as
/// their source and incoming links from same-repository connections that use
/// it as their target. Lookups are keyed by concrete covering occurrence, so
/// distinct named segments with equal content remain distinguishable.
pub struct Relation {
    outgoing: std::collections::HashMap<OccurrenceKey, Vec<ResolvedEnd>>,
    incoming: std::collections::HashMap<OccurrenceKey, Vec<ResolvedEnd>>,
}

impl Relation {
    pub fn build(repo: &Repo, focus_name: &str, focus: &Covering) -> io::Result<Relation> {
        let mut outgoing: std::collections::HashMap<OccurrenceKey, Vec<ResolvedEnd>> =
            std::collections::HashMap::new();
        let mut incoming: std::collections::HashMap<OccurrenceKey, Vec<ResolvedEnd>> =
            std::collections::HashMap::new();

        for cn in Connection::list(repo)? {
            let k = Connection::read(repo, &cn)?;
            let source_focus = k.source_covering == focus_name;
            let target_focus = k.target_peer == peer::SELF && k.target_covering == focus_name;
            if !source_focus && !target_focus {
                continue;
            }

            let source_owned = if source_focus {
                None
            } else {
                Covering::read(repo, &k.source_covering).ok()
            };
            let source = if source_focus {
                Some(focus)
            } else {
                source_owned.as_ref()
            };
            let source_checked = source.is_some();

            let target_owned = if target_focus {
                None
            } else {
                peer::resolve(repo, &k.target_peer)
                    .ok()
                    .and_then(|target| Covering::read(&target, &k.target_covering).ok())
            };
            let tgt_cov = if target_focus {
                Some(focus)
            } else {
                target_owned.as_ref()
            };
            let target_checked = tgt_cov.is_some();

            for p in &k.links {
                let a_index =
                    source.and_then(|covering| covering::endpoint_to_index(covering, &p.a));
                let b_index = tgt_cov.and_then(|target| covering::endpoint_to_index(target, &p.b));
                let resolved = a_index.is_some() && b_index.is_some();

                if source_focus {
                    if let Some(index) = a_index {
                        let target_entry = b_index
                            .and_then(|target_index| tgt_cov.map(|c| &c.entries[target_index]));
                        let target_end = resolved_end(
                            &k.name,
                            &k.target_covering,
                            target_entry,
                            &p.b,
                            &p.note,
                            resolved,
                            target_checked,
                        );
                        outgoing
                            .entry(occurrence_key(&focus.entries[index]))
                            .or_default()
                            .push(target_end);
                    }
                }

                if target_focus {
                    if let Some(index) = b_index {
                        let source_entry = a_index
                            .and_then(|source_index| source.map(|c| &c.entries[source_index]));
                        let source_end = resolved_end(
                            &k.name,
                            &k.source_covering,
                            source_entry,
                            &p.a,
                            &p.note,
                            resolved,
                            source_checked,
                        );
                        incoming
                            .entry(occurrence_key(&focus.entries[index]))
                            .or_default()
                            .push(source_end);
                    }
                }
            }
        }
        Ok(Relation { outgoing, incoming })
    }

    pub fn links_of(&self, entry: &covering::CoveringEntry) -> SegLinks {
        let key = occurrence_key(entry);
        SegLinks {
            outgoing: self.outgoing.get(&key).cloned().unwrap_or_default(),
            incoming: self.incoming.get(&key).cloned().unwrap_or_default(),
        }
    }

    pub fn is_linked(&self, entry: &covering::CoveringEntry) -> bool {
        let key = occurrence_key(entry);
        self.outgoing.contains_key(&key) || self.incoming.contains_key(&key)
    }
}

fn resolved_end(
    connection: &str,
    covering: &str,
    entry: Option<&covering::CoveringEntry>,
    raw_endpoint: &str,
    note: &str,
    resolved: bool,
    checked: bool,
) -> ResolvedEnd {
    ResolvedEnd {
        conn: connection.to_string(),
        covering: covering.to_string(),
        label: entry.map(|e| {
            if e.label.is_empty() {
                "-".to_string()
            } else {
                e.label.clone()
            }
        }),
        loc: entry.map(covering::CoveringEntry::loc),
        name: entry.and_then(|e| e.name.clone()),
        hash: entry
            .map(|e| e.hash.clone())
            .unwrap_or_else(|| raw_endpoint.to_string()),
        note: note.to_string(),
        resolved,
        checked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::Link;
    use crate::covering::CoveringEntry;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_repo(tag: &str) -> Repo {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "covmap-resolve-{tag}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join(".covmap/coverings")).unwrap();
        fs::create_dir_all(root.join(".covmap/connections")).unwrap();
        fs::create_dir_all(root.join(".covmap/objects")).unwrap();
        Repo { root }
    }

    fn entry(hash: &str, file: &str, line: usize, name: &str) -> CoveringEntry {
        CoveringEntry {
            hash: hash.to_string(),
            file: file.to_string(),
            a: line,
            a_col: None,
            b: line,
            b_col: None,
            label: "mapped".to_string(),
            name: Some(name.to_string()),
            message: None,
        }
    }

    #[test]
    fn target_only_same_repo_covering_discovers_incoming_links() {
        let repo = temp_repo("incoming");
        let source = Covering {
            name: "source".into(),
            entries: vec![entry("1111111111111111", "source.txt", 1, "source")],
        };
        let target = Covering {
            name: "target".into(),
            entries: vec![entry("2222222222222222", "target.txt", 1, "target")],
        };
        source.write(&repo).unwrap();
        target.write(&repo).unwrap();
        Connection {
            name: "map".into(),
            source_covering: "source".into(),
            target_peer: peer::SELF.into(),
            target_covering: "target".into(),
            links: vec![Link {
                a: "@source".into(),
                b: "@target".into(),
                note: "rationale".into(),
            }],
        }
        .write(&repo)
        .unwrap();

        let relation = Relation::build(&repo, "target", &target).unwrap();
        let links = relation.links_of(&target.entries[0]);
        assert!(links.outgoing.is_empty());
        assert_eq!(links.incoming.len(), 1);
        assert_eq!(links.incoming[0].name.as_deref(), Some("source"));
        assert!(relation.is_linked(&target.entries[0]));
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn target_relation_survives_a_missing_source_covering() {
        let repo = temp_repo("missing-source");
        let source = Covering {
            name: "source".into(),
            entries: vec![entry("1111111111111111", "source.txt", 1, "source")],
        };
        let target = Covering {
            name: "target".into(),
            entries: vec![entry("2222222222222222", "target.txt", 1, "target")],
        };
        source.write(&repo).unwrap();
        target.write(&repo).unwrap();
        Connection {
            name: "map".into(),
            source_covering: "source".into(),
            target_peer: peer::SELF.into(),
            target_covering: "target".into(),
            links: vec![Link {
                a: "@source".into(),
                b: "@target".into(),
                note: "rationale".into(),
            }],
        }
        .write(&repo)
        .unwrap();
        fs::remove_file(repo.coverings_dir().join("source")).unwrap();

        let relation = Relation::build(&repo, "target", &target).unwrap();
        let links = relation.links_of(&target.entries[0]);
        assert_eq!(links.incoming.len(), 1);
        assert_eq!(links.incoming[0].hash, "@source");
        assert!(!links.incoming[0].resolved);
        assert!(!links.incoming[0].checked);
        assert!(links.incoming[0].name.is_none());
        assert!(relation.is_linked(&target.entries[0]));
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn relation_distinguishes_named_duplicate_hash_occurrences() {
        let repo = temp_repo("duplicate");
        let source = Covering {
            name: "source".into(),
            entries: vec![entry("1111111111111111", "source.txt", 1, "source")],
        };
        let target = Covering {
            name: "target".into(),
            entries: vec![
                entry("aaaaaaaaaaaaaaaa", "target.txt", 1, "first"),
                entry("aaaaaaaaaaaaaaaa", "target.txt", 2, "second"),
            ],
        };
        source.write(&repo).unwrap();
        target.write(&repo).unwrap();
        Connection {
            name: "map".into(),
            source_covering: "source".into(),
            target_peer: peer::SELF.into(),
            target_covering: "target".into(),
            links: vec![Link {
                a: "@source".into(),
                b: "@second".into(),
                note: String::new(),
            }],
        }
        .write(&repo)
        .unwrap();

        let relation = Relation::build(&repo, "target", &target).unwrap();
        assert!(!relation.is_linked(&target.entries[0]));
        assert!(relation.is_linked(&target.entries[1]));
        assert!(relation.links_of(&target.entries[0]).incoming.is_empty());
        assert_eq!(relation.links_of(&target.entries[1]).incoming.len(), 1);
        fs::remove_dir_all(repo.root).unwrap();
    }
}
