//! Resolving the link relation for inspection.
//!
//! covmap's value to a reader is the *relation* between segments, but a
//! connection stores each link as a pair of opaque endpoints (`@name` or
//! hash) into two coverings. To *understand* a segment you want its links
//! with the far side already resolved to `covering:label loc` — the
//! "combined view" that cip 0003 (bounded drill-down) is built on.
//!
//! This module is the single place that walks a source covering's
//! connections and resolves every endpoint. Both the CLI (`show`, `ls`,
//! `status`) and the HTML report consume it, so the two can never disagree
//! about the relation.
//!
//! Direction is from the perspective of the *source* repo (the one that
//! stores the connections): a source segment's **outgoing** links point at
//! target segments; a target segment's **incoming** links point back at
//! source segments. Incoming links are discoverable only for connections
//! this repo stores (see cip 0003 "Known limit").

use crate::connection::Connection;
use crate::covering::{self, Covering};
use crate::repo::Repo;
use std::io;
use std::path::PathBuf;

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
}

/// The resolved links of one segment: outgoing (this segment is a source)
/// and incoming (this segment is a target).
#[derive(Clone, Debug, Default)]
pub struct SegLinks {
    pub outgoing: Vec<ResolvedEnd>,
    pub incoming: Vec<ResolvedEnd>,
}

/// The whole resolved relation for a source covering `src` in `repo`:
/// every connection stored in `repo` whose source covering is `src`,
/// resolved against both sides. Look up a segment's links by its hash.
pub struct Relation {
    /// source segment hash -> its outgoing links.
    outgoing: std::collections::HashMap<String, Vec<ResolvedEnd>>,
    /// target segment hash (in the source covering, for self-maps, or in a
    /// target covering) -> incoming links pointing at it.
    incoming: std::collections::HashMap<String, Vec<ResolvedEnd>>,
}

impl Relation {
    /// Build the relation for source covering `src_name` in `repo`. Only
    /// connections whose `source_covering` is `src_name` are considered.
    pub fn build(repo: &Repo, src_name: &str, src: &Covering) -> io::Result<Relation> {
        let mut outgoing: std::collections::HashMap<String, Vec<ResolvedEnd>> =
            std::collections::HashMap::new();
        let mut incoming: std::collections::HashMap<String, Vec<ResolvedEnd>> =
            std::collections::HashMap::new();

        for cn in Connection::list(repo)? {
            let k = Connection::read(repo, &cn)?;
            if k.source_covering != src_name {
                continue;
            }
            let tgt_repo = Repo {
                root: PathBuf::from(&k.target_repo),
            };
            let tgt_cov = Covering::read(&tgt_repo, &k.target_covering).ok();

            for p in &k.links {
                // Resolve each endpoint to a current hash (so links follow
                // content edits); None => the endpoint is broken.
                let a_hash = covering::endpoint_to_hash(src, &p.a);
                let b_hash = tgt_cov
                    .as_ref()
                    .and_then(|t| covering::endpoint_to_hash(t, &p.b));
                let a_in = a_hash.is_some();
                let b_in = b_hash.is_some();
                let a_key = a_hash.unwrap_or_else(|| p.a.clone());
                let b_key = b_hash.unwrap_or_else(|| p.b.clone());

                // The target end (as seen from the source segment).
                let target_end = ResolvedEnd {
                    conn: k.name.clone(),
                    covering: k.target_covering.clone(),
                    label: describe_label(tgt_cov.as_ref(), &b_key, b_in),
                    loc: describe_loc(tgt_cov.as_ref(), &b_key, b_in),
                    name: describe_name(tgt_cov.as_ref(), &b_key, b_in),
                    hash: b_key.clone(),
                    note: p.note.clone(),
                    resolved: a_in && b_in,
                };
                outgoing.entry(a_key.clone()).or_default().push(target_end);

                // The source end (as seen from the target segment).
                let source_end = ResolvedEnd {
                    conn: k.name.clone(),
                    covering: k.source_covering.clone(),
                    label: describe_label(Some(src), &a_key, a_in),
                    loc: describe_loc(Some(src), &a_key, a_in),
                    name: describe_name(Some(src), &a_key, a_in),
                    hash: a_key.clone(),
                    note: p.note.clone(),
                    resolved: a_in && b_in,
                };
                incoming.entry(b_key.clone()).or_default().push(source_end);
            }
        }
        Ok(Relation { outgoing, incoming })
    }

    /// The resolved links of one segment (by its hash in the source
    /// covering / a target covering).
    pub fn links_of(&self, hash: &str) -> SegLinks {
        SegLinks {
            outgoing: self.outgoing.get(hash).cloned().unwrap_or_default(),
            incoming: self.incoming.get(hash).cloned().unwrap_or_default(),
        }
    }

    /// True if `hash` participates in any link (either direction).
    pub fn is_linked(&self, hash: &str) -> bool {
        self.outgoing.contains_key(hash) || self.incoming.contains_key(hash)
    }
}

fn find<'a>(
    cov: Option<&'a Covering>,
    hash: &str,
    resolved: bool,
) -> Option<&'a covering::CoveringEntry> {
    if !resolved {
        return None;
    }
    cov.and_then(|c| c.entries.iter().find(|e| e.hash == hash))
}

fn describe_label(cov: Option<&Covering>, hash: &str, resolved: bool) -> Option<String> {
    find(cov, hash, resolved).map(|e| {
        if e.label.is_empty() {
            "-".to_string()
        } else {
            e.label.clone()
        }
    })
}

fn describe_loc(cov: Option<&Covering>, hash: &str, resolved: bool) -> Option<String> {
    find(cov, hash, resolved).map(|e| e.loc())
}

fn describe_name(cov: Option<&Covering>, hash: &str, resolved: bool) -> Option<String> {
    find(cov, hash, resolved).and_then(|e| e.name.clone())
}
