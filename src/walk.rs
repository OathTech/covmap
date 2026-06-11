//! File discovery for `covmap cover`.
//!
//! There are no segmentation rules. `cover` walks the repository and,
//! for every readable UTF-8 file it finds, emits one whole-file segment
//! with the label `unknown`. The user (or an agent) refines that
//! covering with `covmap split` and `covmap merge`.

use crate::segment::Segment;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const DEFAULT_LABEL: &str = "unknown";

/// Walk all files under `root`, returning paths relative to `root`,
/// in stable (sorted) order. Skips dot-directories (e.g. `.covmap`,
/// `.git`) and any path matched by `.covmap/ignore`.
pub fn walk_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let ignore = load_ignore(root)?;
    let mut out = Vec::new();
    walk(root, root, &ignore, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk(
    root: &Path,
    dir: &Path,
    ignore: &[String],
    out: &mut Vec<PathBuf>,
) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let s = name.to_string_lossy();
        if s.starts_with('.') {
            continue;
        }
        let p = entry.path();
        let rel = p.strip_prefix(root).unwrap().to_path_buf();
        let rel_str = rel_string(&rel);
        let ft = entry.file_type()?;
        if ft.is_dir() {
            if is_ignored(&rel_str, true, ignore) {
                continue;
            }
            walk(root, &p, ignore, out)?;
        } else if ft.is_file() {
            if is_ignored(&rel_str, false, ignore) {
                continue;
            }
            out.push(rel);
        }
    }
    Ok(())
}

/// Load `.covmap/ignore` (one pattern per line; `#` comments and blank
/// lines skipped). Missing file → no patterns.
fn load_ignore(root: &Path) -> io::Result<Vec<String>> {
    let p = root.join(".covmap").join("ignore");
    let raw = match fs::read_to_string(&p) {
        Ok(s) => s,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        out.push(t.to_string());
    }
    Ok(out)
}

fn is_ignored(rel: &str, is_dir: bool, patterns: &[String]) -> bool {
    for pat in patterns {
        if matches_pattern(rel, is_dir, pat) {
            return true;
        }
    }
    false
}

fn matches_pattern(rel: &str, _is_dir: bool, pat: &str) -> bool {
    let pat = pat.strip_suffix('/').unwrap_or(pat);
    let (pat, anchored) = match pat.strip_prefix('/') {
        Some(rest) => (rest, true),
        None => (pat, false),
    };
    if anchored {
        return glob_match(pat, rel);
    }
    if glob_match(pat, rel) {
        return true;
    }
    for comp in rel.split('/') {
        if glob_match(pat, comp) {
            return true;
        }
    }
    if pat.contains('/') {
        let mut start = 0;
        while start < rel.len() {
            if glob_match(pat, &rel[start..]) {
                return true;
            }
            match rel[start..].find('/') {
                Some(i) => start += i + 1,
                None => break,
            }
        }
    }
    false
}

fn glob_match(pat: &str, s: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    let t: Vec<char> = s.chars().collect();
    glob_rec(&p, 0, &t, 0)
}

fn glob_rec(p: &[char], pi: usize, t: &[char], ti: usize) -> bool {
    if pi == p.len() {
        return ti == t.len();
    }
    match p[pi] {
        '*' => {
            if glob_rec(p, pi + 1, t, ti) {
                return true;
            }
            if ti < t.len() && t[ti] != '/' {
                return glob_rec(p, pi, t, ti + 1);
            }
            false
        }
        '?' => {
            if ti < t.len() && t[ti] != '/' {
                glob_rec(p, pi + 1, t, ti + 1)
            } else {
                false
            }
        }
        c => {
            if ti < t.len() && t[ti] == c {
                glob_rec(p, pi + 1, t, ti + 1)
            } else {
                false
            }
        }
    }
}

/// Read a UTF-8 text file as a vector of lines, each retaining its
/// trailing '\n' when present. Returns `Ok(None)` for files that are
/// not valid UTF-8 (binaries are skipped silently).
pub fn read_text_lines(path: &Path) -> io::Result<Option<Vec<String>>> {
    let raw = match fs::read(path) {
        Ok(b) => b,
        Err(e) => return Err(e),
    };
    let s = match std::str::from_utf8(&raw) {
        Ok(s) => s.to_string(),
        Err(_) => return Ok(None),
    };
    let mut lines = Vec::new();
    let mut start = 0usize;
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' {
            lines.push(s[start..=i].to_string());
            start = i + 1;
        }
    }
    if start < s.len() {
        lines.push(s[start..].to_string());
    }
    Ok(Some(lines))
}

pub fn rel_string(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// Build a single whole-file segment with the default label.
pub fn whole_file_segment(file: &str, lines: &[String]) -> Segment {
    let content: String = lines.concat();
    Segment {
        file: file.to_string(),
        a: 1,
        b: lines.len(),
        label: DEFAULT_LABEL.to_string(),
        content,
    }
}
