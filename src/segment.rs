//! Segments and their content-addressed storage.
//!
//! A segment is a range (file, a, b) with a covering-local `label`. Its
//! hash is the first 16 hex chars (64 bits) of sha256(content) — content
//! only: the label is *not* an input to the hash (it is ref-local status
//! metadata, like the segment name and message), so re-labelling a segment
//! never changes its identity. With a few thousand segments the collision
//! probability is around 2^-44, and the shorter hash is what every command
//! prints and accepts. The object body is the raw content bytes.

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
        hash_of(&self.content)
    }
}

/// Number of Unicode scalar values (characters) in a line.
pub fn char_len(line: &str) -> usize {
    line.chars().count()
}

/// Extract the content of a segment range from a file's `lines` (each
/// line retains its trailing `'\n'` when present, as produced by
/// `walk::read_text_lines`).
///
/// Line numbers `a`/`b` are 1-indexed inclusive. Columns `a_col`/`b_col`
/// are 1-indexed inclusive character positions within their line:
/// `a_col = None` means "from the first character of line `a`" and
/// `b_col = None` means "through the last character of line `b`"
/// (including its trailing newline, if any). A single-line segment has
/// `a == b`, and its characters run from `a_col` through `b_col`
/// inclusive.
///
/// Returns `None` when the range does not fit the file (boundary drift)
/// or the columns are out of range — callers treat that as an error.
pub fn extract_content(
    lines: &[String],
    a: usize,
    a_col: Option<usize>,
    b: usize,
    b_col: Option<usize>,
) -> Option<String> {
    if a == 0 || a > b || b > lines.len() {
        return None;
    }
    let mut out = String::new();
    if a == b {
        let cs: Vec<char> = lines[a - 1].chars().collect();
        let len = cs.len();
        let start = a_col.unwrap_or(1);
        let end = b_col.unwrap_or(len);
        if start < 1 || start > len || end > len || start > end {
            return None;
        }
        out.extend(&cs[start - 1..end]);
    } else {
        // First line: from a_col through end of line (incl. newline).
        let first: Vec<char> = lines[a - 1].chars().collect();
        let flen = first.len();
        let start = a_col.unwrap_or(1);
        if start < 1 || start > flen {
            return None;
        }
        out.extend(&first[start - 1..]);
        // Middle lines (file lines a+1 ..= b-1): whole.
        for line in &lines[a..b - 1] {
            out.push_str(line);
        }
        // Last line: from its first character through b_col.
        let last: Vec<char> = lines[b - 1].chars().collect();
        let llen = last.len();
        let end = b_col.unwrap_or(llen);
        if end < 1 || end > llen {
            return None;
        }
        out.extend(&last[..end]);
    }
    Some(out)
}

/// The segment hash: `trunc16(sha256(content))`. Content only — the label
/// is deliberately excluded (see the module docs), so identical content is
/// always one shared object regardless of label.
pub fn hash_of(content: &str) -> String {
    let mut h = Sha256::new();
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
    // Object body is the raw content — the label is ref-local metadata and
    // is not stored in (or hashed into) the object.
    fs::write(path, &seg.content)?;
    Ok(h)
}

/// Read an object's content from the store. The object body is exactly the
/// segment's content (the label lives in the covering ref, not here).
pub fn read_object(objects_dir: &std::path::Path, hash: &str) -> std::io::Result<String> {
    let path = object_path(objects_dir, hash);
    let mut f = fs::File::open(path)?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        // Per-process counter avoids same-nanosecond path collisions
        // when tests run in parallel.
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        p.push(format!("covmap-segment-test-{nanos}-{seq}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn hash_is_deterministic_and_truncated() {
        let h = hash_of("hello\n");
        assert_eq!(h.len(), HASH_LEN);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(h, hash_of("hello\n"));
    }

    #[test]
    fn hash_excludes_label() {
        // Same content, any label → same hash. The label is not an input.
        let a = Segment {
            file: "x".into(),
            a: 1,
            b: 1,
            label: "unknown".into(),
            content: "x\n".into(),
        };
        let b = Segment {
            file: "x".into(),
            a: 1,
            b: 1,
            label: "mapped".into(),
            content: "x\n".into(),
        };
        assert_eq!(a.hash(), b.hash());
        assert_eq!(a.hash(), hash_of("x\n"));
    }

    #[test]
    fn different_content_differs() {
        assert_ne!(hash_of("a"), hash_of("b"));
    }

    #[test]
    fn object_path_splits_at_two() {
        let dir = std::path::Path::new("/tmp/objs");
        let p = object_path(dir, "abcdef0123456789");
        assert_eq!(p, std::path::Path::new("/tmp/objs/ab/cdef0123456789"));
    }

    #[test]
    fn write_then_read_roundtrip() {
        let dir = tmpdir();
        let seg = Segment {
            file: "x.md".into(),
            a: 1,
            b: 2,
            label: "Intro".into(),
            content: "line one\nline two\n".into(),
        };
        let h = write_object(&dir, &seg).unwrap();
        let content = read_object(&dir, &h).unwrap();
        assert_eq!(content, "line one\nline two\n");
        // The object body is content only; the hash is content only.
        assert_eq!(h, hash_of("line one\nline two\n"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_object_is_idempotent() {
        let dir = tmpdir();
        let seg = Segment {
            file: "x.md".into(),
            a: 1,
            b: 1,
            label: "".into(),
            content: "x\n".into(),
        };
        let h1 = write_object(&dir, &seg).unwrap();
        let h2 = write_object(&dir, &seg).unwrap();
        assert_eq!(h1, h2);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_prefix_rejects_short_input() {
        let dir = tmpdir();
        let err = resolve_prefix(&dir, "a").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_prefix_unique_match() {
        let dir = tmpdir();
        let seg = Segment {
            file: "x".into(),
            a: 1,
            b: 1,
            label: "".into(),
            content: "abc".into(),
        };
        let h = write_object(&dir, &seg).unwrap();
        let prefix = &h[..6];
        let resolved = resolve_prefix(&dir, prefix).unwrap();
        assert_eq!(resolved, h);
        std::fs::remove_dir_all(&dir).ok();
    }

    fn ls(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn extract_whole_lines_when_cols_none() {
        let lines = ls(&["alpha\n", "beta\n", "gamma\n"]);
        assert_eq!(
            extract_content(&lines, 1, None, 3, None).unwrap(),
            "alpha\nbeta\ngamma\n"
        );
        assert_eq!(extract_content(&lines, 2, None, 2, None).unwrap(), "beta\n");
    }

    #[test]
    fn extract_single_line_columns_inclusive() {
        // "hello world\n" — cols 1..5 inclusive = "hello".
        let lines = ls(&["hello world\n"]);
        assert_eq!(
            extract_content(&lines, 1, Some(1), 1, Some(5)).unwrap(),
            "hello"
        );
        // col 7 to end-of-line includes the newline.
        assert_eq!(
            extract_content(&lines, 1, Some(7), 1, None).unwrap(),
            "world\n"
        );
    }

    #[test]
    fn extract_multi_line_with_columns() {
        let lines = ls(&["hello world\n", "second\n", "third line\n"]);
        // From line 1 col 7 through line 3 col 5: "world\n" + "second\n" + "third".
        assert_eq!(
            extract_content(&lines, 1, Some(7), 3, Some(5)).unwrap(),
            "world\nsecond\nthird"
        );
    }

    #[test]
    fn extract_rejects_out_of_range() {
        let lines = ls(&["abc\n"]);
        assert!(extract_content(&lines, 1, Some(1), 1, Some(5)).is_none()); // col past end
        assert!(extract_content(&lines, 1, Some(3), 1, Some(2)).is_none()); // start > end
        assert!(extract_content(&lines, 1, None, 2, None).is_none()); // line past eof
    }

    #[test]
    fn extract_handles_no_trailing_newline() {
        let lines = ls(&["abc\n", "xyz"]); // last line has no '\n'
        assert_eq!(extract_content(&lines, 2, None, 2, None).unwrap(), "xyz");
        assert_eq!(
            extract_content(&lines, 2, Some(1), 2, Some(3)).unwrap(),
            "xyz"
        );
    }

    #[test]
    fn extract_multibyte_chars_are_one_column_each() {
        // "héllo\n": é is one char but two bytes; cols are char positions.
        let lines = ls(&["héllo\n"]);
        assert_eq!(
            extract_content(&lines, 1, Some(1), 1, Some(2)).unwrap(),
            "hé"
        );
    }

    #[test]
    fn resolve_prefix_no_match() {
        let dir = tmpdir();
        // No objects written.
        let err = resolve_prefix(&dir, "deadbeef").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        std::fs::remove_dir_all(&dir).ok();
    }
}
