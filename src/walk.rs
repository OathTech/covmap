//! File discovery and covering-input selection for `covmap cover`.
//!
//! Discovery is repository-wide and policy-independent: dot-prefixed path
//! components and symbolic links are skipped. The effective policy for a
//! covering then applies positive include patterns and repository plus
//! covering-specific ignore vetoes.

use crate::repo::Repo;
use crate::segment::Segment;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const DEFAULT_LABEL: &str = "unknown";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    pub text: String,
    pub file: PathBuf,
    pub line: usize,
}

#[derive(Clone, Debug)]
enum Include {
    All,
    Patterns(Vec<Pattern>),
}

#[derive(Clone, Debug)]
pub struct SelectionPolicy {
    include: Include,
    ignores: Vec<Pattern>,
}

#[derive(Clone, Debug)]
pub struct SelectionResult {
    pub files: Vec<PathBuf>,
    pub unmatched_include: Vec<Pattern>,
}

impl SelectionPolicy {
    pub fn load(repo: &Repo, covering: &str) -> io::Result<SelectionPolicy> {
        let selection_dir = repo.selection_dir(covering)?;
        require_optional_directory(&selection_dir)?;

        let global_include = load_pattern_file(&repo.dot().join("include"))?;
        let global_ignore = load_pattern_file(&repo.dot().join("ignore"))?.unwrap_or_default();
        let covering_include = load_pattern_file(&selection_dir.join("include"))?;
        let covering_ignore = load_pattern_file(&selection_dir.join("ignore"))?.unwrap_or_default();

        let include = match covering_include.or(global_include) {
            Some(patterns) => Include::Patterns(patterns),
            None => Include::All,
        };
        let mut ignores = global_ignore;
        ignores.extend(covering_ignore);
        Ok(SelectionPolicy { include, ignores })
    }

    pub fn admitted(&self, rel: &str) -> bool {
        self.included(rel) && !self.ignores.iter().any(|p| file_match(&p.text, rel))
    }

    pub fn select(&self, root: &Path) -> io::Result<SelectionResult> {
        let discoverable = discoverable_files(root)?;
        let mut files = Vec::new();
        let mut match_counts = match &self.include {
            Include::All => Vec::new(),
            Include::Patterns(patterns) => vec![0usize; patterns.len()],
        };

        for rel in discoverable {
            let rel_str = rel_string(&rel);
            let included = match &self.include {
                Include::All => true,
                Include::Patterns(patterns) => {
                    let mut matched = false;
                    for (index, pattern) in patterns.iter().enumerate() {
                        if file_match(&pattern.text, &rel_str) {
                            match_counts[index] += 1;
                            matched = true;
                        }
                    }
                    matched
                }
            };
            if included && !self.ignores.iter().any(|p| file_match(&p.text, &rel_str)) {
                files.push(rel);
            }
        }

        let unmatched_include = match &self.include {
            Include::All => Vec::new(),
            Include::Patterns(patterns) => patterns
                .iter()
                .zip(match_counts)
                .filter(|(_, count)| *count == 0)
                .map(|(pattern, _)| pattern.clone())
                .collect(),
        };
        Ok(SelectionResult {
            files,
            unmatched_include,
        })
    }

    fn included(&self, rel: &str) -> bool {
        match &self.include {
            Include::All => true,
            Include::Patterns(patterns) => patterns.iter().any(|p| file_match(&p.text, rel)),
        }
    }
}

/// Walk all discoverable regular files under `root` in stable order.
pub fn discoverable_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    walk(root, root, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            walk(root, &path, out)?;
        } else if file_type.is_file() {
            out.push(path.strip_prefix(root).unwrap().to_path_buf());
        }
    }
    Ok(())
}

fn require_optional_directory(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "selection path is not a regular directory: {}",
                    path.display()
                ),
            ))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn load_pattern_file(path: &Path) -> io::Result<Option<Vec<Pattern>>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("selection policy is not a regular file: {}", path.display()),
        ));
    }
    let raw = fs::read_to_string(path).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("cannot read selection policy {}: {error}", path.display()),
        )
    })?;
    let patterns = raw
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let text = line.trim();
            (!text.is_empty() && !text.starts_with('#')).then(|| Pattern {
                text: text.to_string(),
                file: path.to_path_buf(),
                line: index + 1,
            })
        })
        .collect();
    Ok(Some(patterns))
}

/// Validate global and covering-specific policy layout without requiring a
/// covering ref for each selection directory.
pub fn layout_issues(repo: &Repo) -> Vec<String> {
    let mut issues = Vec::new();
    for name in ["include", "ignore"] {
        validate_policy_file(&repo.dot().join(name), &mut issues);
    }

    let root = repo.selections_dir();
    let metadata = match fs::symlink_metadata(&root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return issues,
        Err(error) => {
            issues.push(format!("cannot inspect {}: {error}", root.display()));
            return issues;
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        issues.push(format!(
            "selection root is not a regular directory: {}",
            root.display()
        ));
        return issues;
    }
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) => {
            issues.push(format!("cannot read {}: {error}", root.display()));
            return issues;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                issues.push(format!("cannot read entry in {}: {error}", root.display()));
                continue;
            }
        };
        let name = match entry.file_name().into_string() {
            Ok(name) => name,
            Err(_) => {
                issues.push(format!(
                    "selection directory name is not valid UTF-8: {}",
                    entry.path().display()
                ));
                continue;
            }
        };
        if !crate::repo::valid_covering_name(&name) {
            issues.push(format!("invalid selection covering name '{name}'"));
            continue;
        }
        let metadata = match fs::symlink_metadata(entry.path()) {
            Ok(metadata) => metadata,
            Err(error) => {
                issues.push(format!(
                    "cannot inspect {}: {error}",
                    entry.path().display()
                ));
                continue;
            }
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            issues.push(format!(
                "selection entry is not a regular directory: {}",
                entry.path().display()
            ));
            continue;
        }
        let children = match fs::read_dir(entry.path()) {
            Ok(children) => children,
            Err(error) => {
                issues.push(format!("cannot read {}: {error}", entry.path().display()));
                continue;
            }
        };
        for child in children {
            let child = match child {
                Ok(child) => child,
                Err(error) => {
                    issues.push(format!(
                        "cannot read entry in {}: {error}",
                        entry.path().display()
                    ));
                    continue;
                }
            };
            let child_name = child.file_name();
            if !matches!(child_name.to_str(), Some("include" | "ignore")) {
                issues.push(format!(
                    "unexpected selection policy entry: {}",
                    child.path().display()
                ));
                continue;
            }
            validate_policy_file(&child.path(), &mut issues);
        }
    }
    issues
}

fn validate_policy_file(path: &Path, issues: &mut Vec<String>) {
    match load_pattern_file(path) {
        Ok(_) => {}
        Err(error) => issues.push(error.to_string()),
    }
}

/// Match a pattern against a file or any of its repository-relative ancestors.
pub fn file_match(pattern: &str, file: &str) -> bool {
    if entry_match(pattern, file) {
        return true;
    }
    let mut ancestor = file;
    while let Some((parent, _)) = ancestor.rsplit_once('/') {
        if entry_match(pattern, parent) {
            return true;
        }
        ancestor = parent;
    }
    false
}

fn entry_match(pat: &str, rel: &str) -> bool {
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
    let pattern: Vec<char> = pat.chars().collect();
    let text: Vec<char> = s.chars().collect();
    glob_rec(&pattern, 0, &text, 0)
}

fn glob_rec(pattern: &[char], pi: usize, text: &[char], ti: usize) -> bool {
    if pi == pattern.len() {
        return ti == text.len();
    }
    match pattern[pi] {
        '*' => {
            if glob_rec(pattern, pi + 1, text, ti) {
                return true;
            }
            ti < text.len() && text[ti] != '/' && glob_rec(pattern, pi, text, ti + 1)
        }
        '?' => ti < text.len() && text[ti] != '/' && glob_rec(pattern, pi + 1, text, ti + 1),
        c => ti < text.len() && text[ti] == c && glob_rec(pattern, pi + 1, text, ti + 1),
    }
}

/// Read a UTF-8 text file as lines retaining trailing newlines. Binary files
/// return `Ok(None)` and are skipped by `cover`.
pub fn read_text_lines(path: &Path) -> io::Result<Option<Vec<String>>> {
    let raw = fs::read(path)?;
    let text = match std::str::from_utf8(&raw) {
        Ok(text) => text,
        Err(_) => return Ok(None),
    };
    let mut lines = Vec::new();
    let mut start = 0usize;
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            lines.push(text[start..=index].to_string());
            start = index + 1;
        }
    }
    if start < text.len() {
        lines.push(text[start..].to_string());
    }
    Ok(Some(lines))
}

pub fn rel_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub fn whole_file_segment(file: &str, lines: &[String]) -> Segment {
    Segment {
        file: file.to_string(),
        a: 1,
        b: lines.len(),
        label: DEFAULT_LABEL.to_string(),
        content: lines.concat(),
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
        let root =
            std::env::temp_dir().join(format!("covmap-walk-{tag}-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        Repo::init(&root).unwrap()
    }

    #[test]
    fn glob_does_not_cross_components() {
        assert!(glob_match("*.rs", "main.rs"));
        assert!(!glob_match("*.rs", "src/main.rs"));
        assert!(glob_match("?at", "cat"));
        assert!(!glob_match("?", "/"));
    }

    #[test]
    fn file_match_includes_descendants_of_exact_directory_patterns() {
        assert!(file_match("schema/test", "a/schema/test/x.json"));
        assert!(file_match("/src", "src/lib.rs"));
        assert!(!file_match("/src", "nested/src/lib.rs"));
        assert!(file_match("*.go", "nested/main.go"));
    }

    #[test]
    fn discovery_skips_dot_prefixed_paths() {
        let repo = temp_repo("dot-paths");
        fs::create_dir_all(repo.root.join(".hidden")).unwrap();
        fs::create_dir_all(repo.root.join("visible/.nested")).unwrap();
        fs::write(repo.root.join("visible.txt"), "visible\n").unwrap();
        fs::write(repo.root.join(".hidden.txt"), "hidden\n").unwrap();
        fs::write(repo.root.join(".hidden/file.txt"), "hidden\n").unwrap();
        fs::write(repo.root.join("visible/.nested/file.txt"), "hidden\n").unwrap();

        assert_eq!(
            discoverable_files(&repo.root).unwrap(),
            vec![PathBuf::from("visible.txt")]
        );
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn discovery_skips_symbolic_links() {
        use std::os::unix::fs::symlink;

        let repo = temp_repo("symlinks");
        fs::create_dir_all(repo.root.join("real")).unwrap();
        fs::write(repo.root.join("real/file.txt"), "content\n").unwrap();
        symlink(
            repo.root.join("real/file.txt"),
            repo.root.join("linked-file.txt"),
        )
        .unwrap();
        symlink(repo.root.join("real"), repo.root.join("linked-dir")).unwrap();

        assert_eq!(
            discoverable_files(&repo.root).unwrap(),
            vec![PathBuf::from("real/file.txt")]
        );
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn covering_include_replaces_global_and_ignore_is_a_veto() {
        let repo = temp_repo("precedence");
        fs::create_dir_all(repo.root.join("src")).unwrap();
        fs::write(repo.root.join("README.md"), "readme\n").unwrap();
        fs::write(repo.root.join("src/lib.rs"), "lib\n").unwrap();
        fs::write(repo.root.join("src/generated.rs"), "generated\n").unwrap();
        fs::write(repo.dot().join("include"), "*.md\n").unwrap();
        fs::write(repo.dot().join("ignore"), "generated.rs\n").unwrap();
        let selection = repo.selection_dir("main").unwrap();
        fs::create_dir_all(&selection).unwrap();
        fs::write(selection.join("include"), "src\n").unwrap();

        let selected = SelectionPolicy::load(&repo, "main")
            .unwrap()
            .select(&repo.root)
            .unwrap();

        assert_eq!(selected.files, vec![PathBuf::from("src/lib.rs")]);
        assert!(selected.unmatched_include.is_empty());
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn present_empty_include_selects_none() {
        let repo = temp_repo("empty");
        fs::write(repo.root.join("file.txt"), "content\n").unwrap();
        fs::write(repo.dot().join("include"), "# deliberately empty\n").unwrap();

        let selected = SelectionPolicy::load(&repo, "main")
            .unwrap()
            .select(&repo.root)
            .unwrap();

        assert!(selected.files.is_empty());
        assert!(selected.unmatched_include.is_empty());
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn unmatched_counts_are_computed_before_ignore_veto() {
        let repo = temp_repo("diagnostics");
        fs::write(repo.root.join("file.rs"), "content\n").unwrap();
        fs::write(repo.dot().join("include"), "*.rs\n*.go\n").unwrap();
        fs::write(repo.dot().join("ignore"), "*.rs\n").unwrap();

        let selected = SelectionPolicy::load(&repo, "main")
            .unwrap()
            .select(&repo.root)
            .unwrap();

        assert!(selected.files.is_empty());
        assert_eq!(selected.unmatched_include.len(), 1);
        assert_eq!(selected.unmatched_include[0].text, "*.go");
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn layout_accepts_predeclared_covering_and_rejects_extras() {
        let repo = temp_repo("layout");
        let selection = repo.selection_dir("future").unwrap();
        fs::create_dir_all(&selection).unwrap();
        fs::write(selection.join("include"), "src\n").unwrap();
        assert!(layout_issues(&repo).is_empty());

        fs::write(selection.join("notes"), "unexpected\n").unwrap();
        assert_eq!(layout_issues(&repo).len(), 1);
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn rel_string_normalizes_separators() {
        let path: PathBuf = ["a", "b", "c.txt"].iter().collect();
        assert_eq!(rel_string(&path), "a/b/c.txt");
    }
}
