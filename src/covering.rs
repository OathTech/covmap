//! Covering refs. A covering is stored at `.covmap/coverings/<name>`.
//!
//! Format:
//!   `# covered=<file>`              (one per file the covering covers)
//!   `<hash>\t<file>\t<a>\t<b>\t<label>`
//!   ...
//!
//! Each of the `<a>` / `<b>` fields is either a bare line number (e.g.
//! `7`) or a line number with a 1-indexed inclusive character column
//! (`7:3`). A bare line number on the start field means "from the first
//! character of the line"; on the end field it means "through the last
//! character of the line" (including its trailing newline). This makes
//! the format backward compatible: refs written by older covmap (line
//! numbers only) parse unchanged and denote whole-line boundaries.
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

/// A character-granular position inside a file: 1-indexed line plus an
/// optional 1-indexed inclusive column. `col == None` is the canonical
/// "line boundary" — the start of the line for a segment's start, or
/// the end of the line (through the newline) for a segment's end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pos {
    pub line: usize,
    pub col: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct CoveringEntry {
    pub hash: String,
    pub file: String,
    pub a: usize,
    pub a_col: Option<usize>,
    pub b: usize,
    pub b_col: Option<usize>,
    pub label: String,
}

/// Parse a `line` or `line:col` field into `(line, Option<col>)`.
fn parse_pos_field(s: &str) -> io::Result<(usize, Option<usize>)> {
    let bad = |_| io::Error::new(io::ErrorKind::InvalidData, format!("bad position '{s}'"));
    match s.split_once(':') {
        Some((l, c)) => {
            let line = l.parse().map_err(bad)?;
            let col = c.parse().map_err(bad)?;
            Ok((line, Some(col)))
        }
        None => Ok((s.parse().map_err(bad)?, None)),
    }
}

/// Render a `(line, Option<col>)` back to `line` or `line:col`.
fn fmt_pos_field(line: usize, col: Option<usize>) -> String {
    match col {
        Some(c) => format!("{line}:{c}"),
        None => line.to_string(),
    }
}

/// Render a `Pos` for diagnostics.
pub fn fmt_pos(p: Pos) -> String {
    fmt_pos_field(p.line, p.col)
}

impl CoveringEntry {
    /// Human-readable location: `file:a..b` for a line-level segment,
    /// `file:a:ac..b:bc` when columns are present. Columns are omitted
    /// individually when `None` (line boundary).
    pub fn loc(&self) -> String {
        format!(
            "{}:{}..{}",
            self.file,
            fmt_pos_field(self.a, self.a_col),
            fmt_pos_field(self.b, self.b_col)
        )
    }

    /// Location without the file prefix: `a..b` (with columns when set).
    pub fn range(&self) -> String {
        format!(
            "{}..{}",
            fmt_pos_field(self.a, self.a_col),
            fmt_pos_field(self.b, self.b_col)
        )
    }

    pub fn start_pos(&self) -> Pos {
        Pos {
            line: self.a,
            col: self.a_col,
        }
    }

    pub fn end_pos(&self) -> Pos {
        Pos {
            line: self.b,
            col: self.b_col,
        }
    }

    /// Sort key over positions within a file. A `None` start column
    /// sorts before any explicit column on the same line; a `None` end
    /// column sorts after any explicit column.
    pub fn sort_key(&self) -> (usize, usize) {
        (self.a, self.a_col.unwrap_or(0))
    }

    /// Whether this segment spans only whole lines: it starts at the
    /// first character of its first line and ends at the end of its
    /// last line. Such a segment occupies its own visual row when
    /// rendered. A segment with a mid-line start or end is a
    /// character-level fragment that must flow inline with its
    /// neighbours so they share the same source line.
    pub fn is_line_aligned(&self) -> bool {
        matches!(self.a_col, None | Some(1)) && self.b_col.is_none()
    }
}

/// The position that immediately follows `end` — i.e. where the next
/// adjacent segment must start. If the end column is `None` (through
/// end of line), the successor is the start of the next line. Otherwise
/// it is the next column on the same line.
pub fn successor_of_end(end: Pos) -> Pos {
    match end.col {
        None => Pos {
            line: end.line + 1,
            col: None,
        },
        Some(c) => Pos {
            line: end.line,
            col: Some(c + 1),
        },
    }
}

/// Whether `start` is the canonical successor position of `end`, i.e.
/// the two segments are adjacent with no gap and no overlap. A start of
/// `col = Some(1)` is equivalent to `col = None` (both denote the first
/// character of the line).
pub fn positions_adjacent(end: Pos, start: Pos) -> bool {
    same_start(successor_of_end(end), start)
}

/// Equality of two *start* positions, treating `col = None` and
/// `col = Some(1)` as the same point (first character of the line).
pub fn same_start(x: Pos, y: Pos) -> bool {
    let norm = |p: Pos| (p.line, p.col.unwrap_or(1));
    norm(x) == norm(y)
}

/// The position one past the end of a file with `n_lines` lines — what
/// the final segment of a complete covering must abut.
pub fn file_end(n_lines: usize) -> Pos {
    Pos {
        line: n_lines + 1,
        col: None,
    }
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
                e.hash,
                e.file,
                fmt_pos_field(e.a, e.a_col),
                fmt_pos_field(e.b, e.b_col),
                e.label
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
            let (a, a_col) = parse_pos_field(parts[2])?;
            let (b, b_col) = parse_pos_field(parts[3])?;
            entries.push(CoveringEntry {
                hash: parts[0].to_string(),
                file: parts[1].to_string(),
                a,
                a_col,
                b,
                b_col,
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
pub fn recut(repo: &Repo, c: &mut Covering, old_hash: &str) -> io::Result<String> {
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
    let lines: Vec<String> = raw.split_inclusive('\n').map(|s| s.to_string()).collect();
    let content =
        segment::extract_content(&lines, entry.a, entry.a_col, entry.b, entry.b_col).ok_or_else(
            || {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "segment {} no longer fits the file ({} lines on disk) — use merge/split",
                        entry.loc(),
                        lines.len()
                    ),
                )
            },
        )?;
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

/// Split the segment with hash `old_hash` at position `(cut_line,
/// cut_col)`. That position becomes the first character of the bottom
/// segment; the top segment ends at the character immediately before
/// it. `cut_col = None` means "the first character of `cut_line`", so
/// the line-level split `a < cut_line <= b` is the special case of a
/// `None` column. Both resulting segments must be non-empty.
///
/// The split is performed on the stored object content (not the file on
/// disk), so `top_content || bot_content == content` exactly and the
/// operation is well-defined even if the file has since drifted.
pub fn split(
    repo: &Repo,
    c: &mut Covering,
    old_hash: &str,
    cut_line: usize,
    cut_col: Option<usize>,
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
    let oob = || {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "split cut={} out of range for segment {}",
                fmt_pos_field(cut_line, cut_col),
                entry.loc()
            ),
        )
    };
    if cut_line < entry.a || cut_line > entry.b {
        return Err(oob());
    }
    let (_label_old, content) = segment::read_object(&repo.objects_dir(), old_hash)?;
    let clines: Vec<&str> = content.split_inclusive('\n').collect();
    let li = cut_line - entry.a; // index of the cut line within content
    if li >= clines.len() {
        return Err(oob());
    }
    // File column at which the bottom segment begins (1 = first char).
    let bcol = cut_col.unwrap_or(1);
    if bcol < 1 {
        return Err(oob());
    }
    // File column at which content line `li` begins: the first content
    // line is offset by the segment's own start column; the rest start
    // at column 1.
    let line_start_col = if li == 0 { entry.a_col.unwrap_or(1) } else { 1 };
    if bcol < line_start_col {
        return Err(oob());
    }
    let skip = bcol - line_start_col; // chars to skip within content line `li`
    let line_chars = segment::char_len(clines[li]);
    if skip > line_chars {
        return Err(oob());
    }
    let offset: usize = clines[..li].iter().map(|l| segment::char_len(l)).sum::<usize>() + skip;
    let total = segment::char_len(&content);
    if offset == 0 || offset >= total {
        // Empty top or empty bottom — nothing to split.
        return Err(oob());
    }
    let top_content: String = content.chars().take(offset).collect();
    let bot_content: String = content.chars().skip(offset).collect();

    // Boundary positions of the two halves. Bottom starts at the cut;
    // the top ends just before it. A cut at column 1 means the top ends
    // at the end of the previous line (canonical `None` end column).
    let (bot_a, bot_a_col) = if bcol == 1 {
        (cut_line, None)
    } else {
        (cut_line, Some(bcol))
    };
    let (top_b, top_b_col) = if bcol == 1 {
        (cut_line - 1, None)
    } else {
        (cut_line, Some(bcol - 1))
    };

    let top = Segment {
        file: entry.file.clone(),
        a: entry.a,
        b: top_b,
        label: label_top.to_string(),
        content: top_content,
    };
    let bot = Segment {
        file: entry.file.clone(),
        a: bot_a,
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
        a_col: entry.a_col,
        b: top_b,
        b_col: top_b_col,
        label: top.label,
    };
    c.entries.insert(
        idx + 1,
        CoveringEntry {
            hash: bot_hash.clone(),
            file: bot.file,
            a: bot_a,
            a_col: bot_a_col,
            b: entry.b,
            b_col: entry.b_col,
            label: bot.label,
        },
    );
    Ok((top_hash, bot_hash))
}

/// Merge two adjacent segments in the same file into one. The
/// segments must share a file and be contiguous (`h1.b + 1 == h2.a`).
pub fn merge(repo: &Repo, c: &mut Covering, h1: &str, h2: &str, label: &str) -> io::Result<String> {
    let i1 = c.entries.iter().position(|e| e.hash == h1).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("hash '{h1}' not in covering"),
        )
    })?;
    let i2 = c.entries.iter().position(|e| e.hash == h2).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("hash '{h2}' not in covering"),
        )
    })?;
    // Order the two by position so the "left" segment's end abuts the
    // "right" segment's start, regardless of the order they were passed.
    let (lo, hi) = if i1 < i2 { (i1, i2) } else { (i2, i1) };
    let (mut e1, mut e2) = (c.entries[lo].clone(), c.entries[hi].clone());
    let (mut lo, mut hi) = (lo, hi);
    if e1.sort_key() > e2.sort_key() {
        std::mem::swap(&mut e1, &mut e2);
        std::mem::swap(&mut lo, &mut hi);
    }
    if e1.file != e2.file || !positions_adjacent(e1.end_pos(), e2.start_pos()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "segments are not adjacent: {} and {}",
                e1.loc(),
                e2.loc()
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
    let keep = lo.min(hi);
    let drop = lo.max(hi);
    c.entries[keep] = CoveringEntry {
        hash: h.clone(),
        file: merged.file,
        a: e1.a,
        a_col: e1.a_col,
        b: e2.b,
        b_col: e2.b_col,
        label: merged.label,
    };
    c.entries.remove(drop);
    Ok(h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_repo() -> Repo {
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!("covmap-covering-test-{nanos}"));
        std::fs::create_dir_all(p.join("objects")).unwrap();
        Repo { root: p }
    }

    fn whole(repo: &Repo, file: &str, content: &str, lines: usize) -> CoveringEntry {
        let seg = Segment {
            file: file.into(),
            a: 1,
            b: lines,
            label: "unknown".into(),
            content: content.into(),
        };
        let hash = segment::write_object(&repo.objects_dir(), &seg).unwrap();
        CoveringEntry {
            hash,
            file: file.into(),
            a: 1,
            a_col: None,
            b: lines,
            b_col: None,
            label: "unknown".into(),
        }
    }

    #[test]
    fn pos_field_roundtrip() {
        assert_eq!(parse_pos_field("7").unwrap(), (7, None));
        assert_eq!(parse_pos_field("7:3").unwrap(), (7, Some(3)));
        assert_eq!(fmt_pos_field(7, None), "7");
        assert_eq!(fmt_pos_field(7, Some(3)), "7:3");
    }

    #[test]
    fn split_at_line_boundary() {
        let repo = tmp_repo();
        let mut c = Covering {
            name: "main".into(),
            entries: vec![whole(&repo, "f.txt", "a\nb\nc\n", 3)],
        };
        let old = c.entries[0].hash.clone();
        split(&repo, &mut c, &old, 2, None, "top", "bot").unwrap();
        assert_eq!(c.entries.len(), 2);
        assert_eq!(c.entries[0].range(), "1..1");
        assert_eq!(c.entries[1].range(), "2..3");
        let (_, top) = segment::read_object(&repo.objects_dir(), &c.entries[0].hash).unwrap();
        let (_, bot) = segment::read_object(&repo.objects_dir(), &c.entries[1].hash).unwrap();
        assert_eq!(top, "a\n");
        assert_eq!(bot, "b\nc\n");
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn split_mid_line_by_column() {
        let repo = tmp_repo();
        let mut c = Covering {
            name: "main".into(),
            entries: vec![whole(&repo, "f.txt", "hello world\n", 1)],
        };
        let old = c.entries[0].hash.clone();
        // Bottom starts at col 7 ("world"); top ends at col 6.
        split(&repo, &mut c, &old, 1, Some(7), "top", "bot").unwrap();
        assert_eq!(c.entries[0].range(), "1..1:6");
        assert_eq!(c.entries[1].range(), "1:7..1");
        let (_, top) = segment::read_object(&repo.objects_dir(), &c.entries[0].hash).unwrap();
        let (_, bot) = segment::read_object(&repo.objects_dir(), &c.entries[1].hash).unwrap();
        assert_eq!(top, "hello ");
        assert_eq!(bot, "world\n");
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn split_then_merge_is_identity() {
        let repo = tmp_repo();
        let mut c = Covering {
            name: "main".into(),
            entries: vec![whole(&repo, "f.txt", "hello world\n", 1)],
        };
        let old = c.entries[0].hash.clone();
        let (top, bot) = split(&repo, &mut c, &old, 1, Some(7), "t", "b").unwrap();
        let merged = merge(&repo, &mut c, &top, &bot, "unknown").unwrap();
        assert_eq!(c.entries.len(), 1);
        // Merging the two halves with the original label restores the
        // original content+label hash.
        assert_eq!(merged, old);
        assert_eq!(c.entries[0].range(), "1..1");
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn split_rejects_empty_halves() {
        let repo = tmp_repo();
        let mut c = Covering {
            name: "main".into(),
            entries: vec![whole(&repo, "f.txt", "abc\n", 1)],
        };
        let old = c.entries[0].hash.clone();
        // Column 1 of the first line would make the top empty.
        assert!(split(&repo, &mut c, &old, 1, Some(1), "t", "b").is_err());
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn position_adjacency_rules() {
        // End-of-line abuts start of next line.
        assert!(positions_adjacent(
            Pos { line: 2, col: None },
            Pos { line: 3, col: None }
        ));
        // Column end abuts next column same line.
        assert!(positions_adjacent(
            Pos { line: 2, col: Some(5) },
            Pos { line: 2, col: Some(6) }
        ));
        // col None and col Some(1) denote the same start point.
        assert!(positions_adjacent(
            Pos { line: 2, col: None },
            Pos { line: 3, col: Some(1) }
        ));
        // A gap is not adjacency.
        assert!(!positions_adjacent(
            Pos { line: 2, col: Some(5) },
            Pos { line: 2, col: Some(7) }
        ));
    }
}
