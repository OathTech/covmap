//! Segments and their content-addressed storage.
//!
//! A segment is a quadruple (file, a, b, label). Its hash is the first
//! 16 hex chars (64 bits) of sha256(label || NUL || content). With a
//! few thousand segments the collision probability is around 2^-44,
//! and the shorter hash is what every command prints and accepts.

use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;

pub const HASH_LEN: usize = 16;

#[derive(Clone, Debug)]
pub struct Segment {
    pub file: String,    // path relative to repo root
    pub a: usize,        // 1-indexed inclusive
    pub b: usize,        // 1-indexed inclusive
    pub label: String,   // empty = no label
    pub content: String, // raw text of lines a..=b, including line breaks
}

impl Segment {
    pub fn hash(&self) -> String {
        hash_of(&self.label, &self.content)
    }
}

pub fn hash_of(label: &str, content: &str) -> String {
    let mut h = Sha256::new();
    h.update(label.as_bytes());
    h.update([0u8]);
    h.update(content.as_bytes());
    let out = h.finalize();
    let mut s = String::with_capacity(HASH_LEN);
    for b in &out[..HASH_LEN / 2] {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Object store path for a hash: `objects/<hh>/<rest>`.
pub fn object_path(objects_dir: &std::path::Path, hash: &str) -> std::path::PathBuf {
    objects_dir.join(&hash[..2]).join(&hash[2..])
}

/// Persist a segment as an object file. Idempotent.
pub fn write_object(objects_dir: &std::path::Path, seg: &Segment) -> std::io::Result<String> {
    let h = seg.hash();
    let path = object_path(objects_dir, &h);
    if path.exists() {
        return Ok(h);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = format!("{}\0{}", seg.label, seg.content);
    fs::write(path, body)?;
    Ok(h)
}

/// Read raw object content (label, content) from the store.
pub fn read_object(objects_dir: &std::path::Path, hash: &str) -> std::io::Result<(String, String)> {
    let path = object_path(objects_dir, hash);
    let mut f = fs::File::open(path)?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    let nul = buf.iter().position(|&b| b == 0).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "malformed object: no NUL")
    })?;
    let label = String::from_utf8_lossy(&buf[..nul]).to_string();
    let content = String::from_utf8_lossy(&buf[nul + 1..]).to_string();
    Ok((label, content))
}

/// Resolve a hash prefix to a unique full hash. Errors on ambiguity / no match.
pub fn resolve_prefix(objects_dir: &std::path::Path, prefix: &str) -> std::io::Result<String> {
    if prefix.len() < 2 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "hash prefix must be at least 2 hex chars",
        ));
    }
    let head = &prefix[..2];
    let tail = &prefix[2..];
    let dir = objects_dir.join(head);
    if !dir.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no segment matches '{prefix}'"),
        ));
    }
    let mut matches = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(tail) {
            matches.push(format!("{head}{name}"));
        }
    }
    match matches.len() {
        0 => Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no segment matches '{prefix}'"),
        )),
        1 => Ok(matches.into_iter().next().unwrap()),
        n => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("ambiguous: {n} segments match '{prefix}'"),
        )),
    }
}
