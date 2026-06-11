//! Covering refs. A covering is stored at `.covmap/coverings/<name>`.
//!
//! Format:
//!   `# covered=<file>`              (one per file the covering covers)
//!   `<hash>\t<file>\t<a>\t<b>\t<label>`
//!   ...
//!
//! The `# covered=` lines are derived from the segments and rewritten
//! on every save. They make it easy to see, by reading the ref alone,
//! which files this covering claims to partition.

use crate::repo::Repo;
use crate::segment::{self, Segment};
use crate::store::atomic_write;
use std::collections::BTreeSet;
use std::fs;
use std::io;

#[derive(Clone, Debug)]
pub struct CoveringEntry {
    pub hash: String,
    pub file: String,
    pub a: usize,
    pub b: usize,
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct Covering {
    pub name: String,
    pub entries: Vec<CoveringEntry>,
}

impl Covering {
    pub fn write(&self, repo: &Repo) -> io::Result<()> {
        let path = repo.coverings_dir().join(&self.name);
        let mut buf = String::new();
        let files: BTreeSet<&str> = self.entries.iter().map(|e| e.file.as_str()).collect();
        for f in &files {
            buf.push_str(&format!("# covered={}\n", f));
        }
        for e in &self.entries {
            buf.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\n",
                e.hash, e.file, e.a, e.b, e.label
            ));
        }
        atomic_write(&path, &buf)
    }

    pub fn read(repo: &Repo, name: &str) -> io::Result<Covering> {
        let path = repo.coverings_dir().join(name);
        let raw = fs::read_to_string(&path).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!("covering '{name}' not found at {}", path.display()),
            )
        })?;
        let mut entries = Vec::new();
        for line in raw.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.splitn(5, '\t').collect();
            if parts.len() < 5 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("malformed covering line: {line}"),
                ));
            }
            entries.push(CoveringEntry {
                hash: parts[0].to_string(),
                file: parts[1].to_string(),
                a: parts[2].parse().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "bad line number")
                })?,
                b: parts[3].parse().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "bad line number")
                })?,
                label: parts[4].to_string(),
            });
        }
        Ok(Covering {
            name: name.to_string(),
            entries,
        })
    }

    pub fn list(repo: &Repo) -> io::Result<Vec<String>> {
        let mut out = Vec::new();
        let dir = repo.coverings_dir();
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

    /// Files that this covering claims to partition.
    pub fn covered_files(&self) -> BTreeSet<String> {
        self.entries.iter().map(|e| e.file.clone()).collect()
    }
}

pub struct Stats {
    pub cardinality: usize,
    pub total_lines: usize,
    pub entropy_bits: f64,
}

pub fn stats(c: &Covering) -> Stats {
    let total_lines: usize = c.entries.iter().map(|e| e.b - e.a + 1).sum();
    let mut h = 0.0f64;
    if total_lines > 0 {
        for e in &c.entries {
            let len = (e.b - e.a + 1) as f64;
            let p = len / total_lines as f64;
            if p > 0.0 {
                h -= p * p.log2();
            }
        }
    }
    Stats {
        cardinality: c.entries.len(),
        total_lines,
        entropy_bits: h,
    }
}

/// Resolve a segment hash prefix inside a single covering's hashes.
pub fn resolve_in_covering(c: &Covering, prefix: &str) -> io::Result<String> {
    let mut matches: Vec<&str> = c
        .entries
        .iter()
        .map(|e| e.hash.as_str())
        .filter(|h| h.starts_with(prefix))
        .collect();
    matches.sort();
    matches.dedup();
    match matches.len() {
        0 => Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no segment matches '{prefix}' in covering '{}'", c.name),
        )),
        1 => Ok(matches.remove(0).to_string()),
        n => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "ambiguous: {n} segments match '{prefix}' in covering '{}'",
                c.name
            ),
        )),
    }
}

pub fn hashes(c: &Covering) -> BTreeSet<String> {
    c.entries.iter().map(|e| e.hash.clone()).collect()
}

pub fn entry_by_hash<'a>(c: &'a Covering, hash: &str) -> Option<&'a CoveringEntry> {
    c.entries.iter().find(|e| e.hash == hash)
}

/// Replace an entry's label, writing a new object. The old hash is
/// preserved in the object store so any connection that referenced it
/// keeps resolving.
pub fn relabel(
    repo: &Repo,
    c: &mut Covering,
    old_hash: &str,
    new_label: &str,
) -> io::Result<String> {
    let idx = c
        .entries
        .iter()
        .position(|e| e.hash == old_hash)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("hash '{old_hash}' not in covering '{}'", c.name),
            )
        })?;
    let (_label_old, content) = segment::read_object(&repo.objects_dir(), old_hash)?;
    let new_hash = segment::hash_of(new_label, &content);
    let new_seg = Segment {
        file: c.entries[idx].file.clone(),
        a: c.entries[idx].a,
        b: c.entries[idx].b,
        label: new_label.to_string(),
        content,
    };
    segment::write_object(&repo.objects_dir(), &new_seg)?;
    c.entries[idx].hash = new_hash.clone();
    c.entries[idx].label = new_label.to_string();
    Ok(new_hash)
}

/// Refresh a segment's content from current disk bytes. Keeps the
/// same (file, a, b, label); recomputes the hash from the file as it
/// is now. Used after a file edit that changed bytes inside the
/// segment but did not move its line boundaries. Returns the new hash.
pub fn recut(
    repo: &Repo,
    c: &mut Covering,
    old_hash: &str,
) -> io::Result<String> {
    let idx = c
        .entries
        .iter()
        .position(|e| e.hash == old_hash)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("hash '{old_hash}' not in covering '{}'", c.name),
            )
        })?;
    let entry = c.entries[idx].clone();
    let abs = repo.root.join(&entry.file);
    let raw = fs::read_to_string(&abs)?;
    let lines: Vec<&str> = raw.split_inclusive('\n').collect();
    if entry.b > lines.len() || entry.a == 0 || entry.a > entry.b {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "segment {}:{}-{} no longer fits the file ({} lines on disk) — use merge/split",
                entry.file, entry.a, entry.b, lines.len()
            ),
        ));
    }
    let content: String = lines[entry.a - 1..entry.b].concat();
    let seg = Segment {
        file: entry.file.clone(),
        a: entry.a,
        b: entry.b,
        label: entry.label.clone(),
        content,
    };
    let new_hash = segment::write_object(&repo.objects_dir(), &seg)?;
    c.entries[idx].hash = new_hash.clone();
    Ok(new_hash)
}

/// Split the segment with hash `old_hash` at line `cut` (1-indexed
/// within the file). The line `cut` becomes the first line of the
/// bottom segment, so `cut` must satisfy `a < cut <= b`.
pub fn split(
    repo: &Repo,
    c: &mut Covering,
    old_hash: &str,
    cut: usize,
    label_top: &str,
    label_bot: &str,
) -> io::Result<(String, String)> {
    let idx = c
        .entries
        .iter()
        .position(|e| e.hash == old_hash)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("hash '{old_hash}' not in covering '{}'", c.name),
            )
        })?;
    let entry = c.entries[idx].clone();
    if cut <= entry.a || cut > entry.b {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "split cut={} out of range ({}, {}] for segment {}:{}-{}",
                cut, entry.a, entry.b, entry.file, entry.a, entry.b
            ),
        ));
    }
    let (_label_old, content) = segment::read_object(&repo.objects_dir(), old_hash)?;
    let lines: Vec<&str> = content.split_inclusive('\n').collect();
    let local_cut = cut - entry.a; // index of first line of bottom
    let top_content: String = lines[..local_cut].concat();
    let bot_content: String = lines[local_cut..].concat();
    let top = Segment {
        file: entry.file.clone(),
        a: entry.a,
        b: cut - 1,
        label: label_top.to_string(),
        content: top_content,
    };
    let bot = Segment {
        file: entry.file.clone(),
        a: cut,
        b: entry.b,
        label: label_bot.to_string(),
        content: bot_content,
    };
    let top_hash = segment::write_object(&repo.objects_dir(), &top)?;
    let bot_hash = segment::write_object(&repo.objects_dir(), &bot)?;
    c.entries[idx] = CoveringEntry {
        hash: top_hash.clone(),
        file: top.file,
        a: top.a,
        b: top.b,
        label: top.label,
    };
    c.entries.insert(
        idx + 1,
        CoveringEntry {
            hash: bot_hash.clone(),
            file: bot.file,
            a: bot.a,
            b: bot.b,
            label: bot.label,
        },
    );
    Ok((top_hash, bot_hash))
}

/// Merge two adjacent segments in the same file into one. The
/// segments must share a file and be contiguous (`h1.b + 1 == h2.a`).
pub fn merge(
    repo: &Repo,
    c: &mut Covering,
    h1: &str,
    h2: &str,
    label: &str,
) -> io::Result<String> {
    let i1 = c
        .entries
        .iter()
        .position(|e| e.hash == h1)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("hash '{h1}' not in covering")))?;
    let i2 = c
        .entries
        .iter()
        .position(|e| e.hash == h2)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("hash '{h2}' not in covering")))?;
    let (lo, hi) = if i1 < i2 { (i1, i2) } else { (i2, i1) };
    let (e1, e2) = (c.entries[lo].clone(), c.entries[hi].clone());
    if e1.file != e2.file || e1.b + 1 != e2.a {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "segments are not adjacent: {}:{}-{} and {}:{}-{}",
                e1.file, e1.a, e1.b, e2.file, e2.a, e2.b
            ),
        ));
    }
    let (_l1, c1) = segment::read_object(&repo.objects_dir(), &e1.hash)?;
    let (_l2, c2) = segment::read_object(&repo.objects_dir(), &e2.hash)?;
    let merged_content = format!("{c1}{c2}");
    let merged = Segment {
        file: e1.file.clone(),
        a: e1.a,
        b: e2.b,
        label: label.to_string(),
        content: merged_content,
    };
    let h = segment::write_object(&repo.objects_dir(), &merged)?;
    c.entries[lo] = CoveringEntry {
        hash: h.clone(),
        file: merged.file,
        a: merged.a,
        b: merged.b,
        label: merged.label,
    };
    c.entries.remove(hi);
    Ok(h)
}
