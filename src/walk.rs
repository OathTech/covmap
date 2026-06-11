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

fn walk(root: &Path, dir: &Path, ignore: &[String], out: &mut Vec<PathBuf>) -> io::Result<()> {
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
    let raw = fs::read(path)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn pats(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    // ---- glob_match -------------------------------------------------------

    #[test]
    fn glob_literal() {
        assert!(glob_match("foo", "foo"));
        assert!(!glob_match("foo", "bar"));
        assert!(!glob_match("foo", "foobar"));
    }

    #[test]
    fn glob_star_matches_within_component() {
        assert!(glob_match("*.go", "main.go"));
        assert!(glob_match("*.go", ".go")); // empty match before .go is ok
        assert!(!glob_match("*.go", "src/main.go")); // * must not cross /
    }

    #[test]
    fn glob_star_does_not_cross_slash() {
        assert!(!glob_match("*", "a/b"));
        assert!(glob_match("*", "abc"));
    }

    #[test]
    fn glob_question_mark() {
        assert!(glob_match("?at", "cat"));
        assert!(!glob_match("?at", "scat"));
        assert!(!glob_match("?", "/")); // ? cannot match /
    }

    // ---- matches_pattern --------------------------------------------------

    #[test]
    fn pattern_extension_unanchored() {
        // *.go matches at any depth.
        assert!(matches_pattern("main.go", false, "*.go"));
        assert!(matches_pattern("src/main.go", false, "*.go"));
        assert!(!matches_pattern("main.rs", false, "*.go"));
    }

    #[test]
    fn pattern_directory_with_trailing_slash() {
        assert!(matches_pattern("schema", true, "schema/"));
        assert!(matches_pattern("schema/foo.json", false, "schema/"));
        assert!(matches_pattern("a/schema/x", false, "schema/"));
    }

    #[test]
    fn pattern_anchored_with_leading_slash() {
        assert!(matches_pattern("CODEOWNERS", false, "/CODEOWNERS"));
        // Anchored: must equal the full relative path.
        assert!(!matches_pattern("a/CODEOWNERS", false, "/CODEOWNERS"));
    }

    #[test]
    fn pattern_exact_filename() {
        assert!(matches_pattern("LICENSE", false, "LICENSE"));
        assert!(matches_pattern("a/LICENSE", false, "LICENSE"));
        assert!(!matches_pattern("LICENSE.md", false, "LICENSE"));
    }

    #[test]
    fn pattern_multi_segment_unanchored() {
        // Multi-segment patterns match on a full tail starting at a
        // component boundary.
        assert!(matches_pattern("schema/test", true, "schema/test"));
        assert!(matches_pattern("a/schema/test", true, "schema/test"));
        // Does not match a longer tail (use a trailing slash to ignore
        // a whole subtree; see the test below).
        assert!(!matches_pattern(
            "a/schema/test/x.json",
            false,
            "schema/test"
        ));
        // Does not straddle a non-boundary.
        assert!(!matches_pattern("aschema/testb", false, "schema/test"));
    }

    #[test]
    fn pattern_directory_form_matches_directory_entry() {
        // `schema/` matches the directory entry `schema`. The walker
        // then prunes the whole subtree without re-checking each file.
        assert!(matches_pattern("schema", true, "schema/"));
        // The matcher itself is component-by-component: a file path
        // under an ignored directory only gets filtered because the
        // walker stopped descending — we don't try to match the
        // file path against `schema/` directly.
    }

    // ---- is_ignored -------------------------------------------------------

    #[test]
    fn is_ignored_combines_patterns() {
        let p = pats(&["*.go", "schema/", "/CODEOWNERS"]);
        assert!(is_ignored("main.go", false, &p));
        assert!(is_ignored("schema/x.json", false, &p));
        assert!(is_ignored("CODEOWNERS", false, &p));
        assert!(!is_ignored("README.md", false, &p));
        assert!(!is_ignored("a/CODEOWNERS", false, &p));
    }

    // ---- read_text_lines / rel_string -------------------------------------

    #[test]
    fn rel_string_normalizes_separators() {
        let p: PathBuf = ["a", "b", "c.txt"].iter().collect();
        // On unix this is already "a/b/c.txt"; on windows the helper
        // converts "\\" -> "/".
        assert_eq!(rel_string(&p), "a/b/c.txt");
    }
}
