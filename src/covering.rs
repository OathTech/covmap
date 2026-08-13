//! Covering refs. A covering is stored at `.covmap/coverings/<name>`.
//!
//! Format:
//!   `# covered=<file>`              (one per file the covering covers)
//!   `<hash>\t<file>\t<a>\t<b>\t<label>[\t<name>]`
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
//! The optional sixth field `<name>` is a *name*: a stable, semantic
//! segment identity that connections can reference instead of the content
//! hash, so a link survives an edit that changes the segment's bytes. A
//! line without a sixth field is an anonymous segment (the common case for
//! `unknown` segments). Older refs that never wrote the field parse
//! unchanged, so this is a backward-compatible extension (no format
//! version bump).
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
    /// Optional name (segment identity). `None` = anonymous segment.
    /// Connections may reference a named segment by `@name`, which survives
    /// edits to the segment's content; an anonymous segment can only be
    /// referenced by its segment hash.
    pub name: Option<String>,
    /// Optional free-form *message*: durable, possibly multi-line text
    /// recording why the segment was cut / what it represents / why it is
    /// (not) mapped. Covering-local metadata like `name`: it is **not** part
    /// of the segment hash, so editing it never touches the object store or
    /// causes drift, and it travels with the segment across split/merge/
    /// recut/remap. Serialized as a `# message` stanza (see `write`).
    pub message: Option<String>,
}

/// Validate a proposed name. Names must be non-empty and avoid the
/// characters covmap uses as separators / sigils so they round-trip
/// through refs and address parsing unambiguously.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('\t')
        && !name.contains('\n')
        && !name.contains(' ')
        && !name.contains(':')
        && !name.starts_with('@')
        && !name.starts_with('#')
}

/// Parse a `line` or `line:col` field into `(line, Option<col>)`.
pub(crate) fn parse_pos_field(s: &str) -> io::Result<(usize, Option<usize>)> {
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

/// Error for a `# message <index> <line-count>` header that does not parse.
fn bad_message_header(line: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("malformed message stanza header: {line}"),
    )
}

/// First `HASH_LEN` chars of a (possibly already short) hash, for
/// display. Safe on shorter strings.
pub fn short_hash(h: &str) -> &str {
    let n = h.len().min(segment::HASH_LEN);
    &h[..n]
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

    /// The endpoint a connection should store for this segment: the
    /// name (`@name`) when the segment is named — so the link survives
    /// content edits — otherwise the bare segment hash (legacy).
    pub fn endpoint(&self) -> String {
        match &self.name {
            Some(n) => format!("@{n}"),
            None => self.hash.clone(),
        }
    }
}

/// A connection-link endpoint, parsed from its stored string. `@name`
/// references a named segment (resilient to content edits); a bare hex
/// string references a segment hash (legacy / anonymous segments).
pub enum Endpoint {
    Name(String),
    Hash(String),
}

pub fn parse_endpoint(s: &str) -> Endpoint {
    match s.strip_prefix('@') {
        Some(n) => Endpoint::Name(n.to_string()),
        None => Endpoint::Hash(s.to_string()),
    }
}

/// Does endpoint `ep` still resolve against covering `c`? A name
/// endpoint resolves iff a segment with that name still exists (it is
/// broken only when the segment is *deleted*, not when its bytes change).
/// A hash endpoint resolves iff some segment still carries that hash
/// (legacy behaviour: byte changes break it).
pub fn endpoint_resolves(c: &Covering, ep: &str) -> bool {
    endpoint_to_hash(c, ep).is_some()
}

/// Resolve an endpoint string to the current segment hash of the segment
/// it denotes, or `None` if it no longer resolves. A `@name` endpoint
/// maps to whatever hash the named segment currently carries (so the link
/// follows content edits); a hash endpoint maps to itself if present.
pub fn endpoint_to_hash(c: &Covering, ep: &str) -> Option<String> {
    match parse_endpoint(ep) {
        Endpoint::Name(n) => c
            .entries
            .iter()
            .find(|e| e.name.as_deref() == Some(n.as_str()))
            .map(|e| e.hash.clone()),
        Endpoint::Hash(h) => c
            .entries
            .iter()
            .find(|e| e.hash == h)
            .map(|e| e.hash.clone()),
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

/// Total order on a position used as a *start*: `col = None` and
/// `col = Some(1)` collapse to the first character of the line.
fn start_key(p: Pos) -> (usize, usize) {
    (p.line, p.col.unwrap_or(1))
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
            // The sixth (name) field is written only when present, so a
            // covering with no named segments is byte-identical to one
            // written by an older covmap.
            match &e.name {
                Some(n) => buf.push_str(&format!(
                    "{}\t{}\t{}\t{}\t{}\t{}\n",
                    e.hash,
                    e.file,
                    fmt_pos_field(e.a, e.a_col),
                    fmt_pos_field(e.b, e.b_col),
                    e.label,
                    n
                )),
                None => buf.push_str(&format!(
                    "{}\t{}\t{}\t{}\t{}\n",
                    e.hash,
                    e.file,
                    fmt_pos_field(e.a, e.a_col),
                    fmt_pos_field(e.b, e.b_col),
                    e.label
                )),
            }
        }
        // Segment messages, appended after all segment lines as
        // length-prefixed stanzas. `<index>` is the 0-based position of the
        // segment among the lines just written (i.e. in `self.entries`
        // order); `<line-count>` fixes exactly how many following lines form
        // the message, so a body containing blank lines — or lines starting
        // with '#' — is read back verbatim. A segment with no message emits
        // nothing, so a message-free covering is byte-identical to an older
        // covmap's ref (backward compatible, no FORMAT_VERSION bump).
        for (i, e) in self.entries.iter().enumerate() {
            if let Some(msg) = &e.message {
                let lines: Vec<&str> = msg.split('\n').collect();
                buf.push_str(&format!("# message {} {}\n", i, lines.len()));
                for l in lines {
                    buf.push_str(l);
                    buf.push('\n');
                }
            }
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
        // Scan line by line. Segment lines come first; `# message` stanzas
        // (each a header plus a fixed number of body lines) come after, and
        // are attached to the segment at the stanza's index. We consume the
        // body lines explicitly so a body containing blank or `#`-prefixed
        // lines is read verbatim rather than skipped as a comment.
        let all: Vec<&str> = raw.lines().collect();
        let mut i = 0;
        while i < all.len() {
            let line = all[i];
            if let Some(rest) = line.strip_prefix("# message ") {
                let mut it = rest.split_whitespace();
                let idx: usize = it
                    .next()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| bad_message_header(line))?;
                let count: usize = it
                    .next()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| bad_message_header(line))?;
                let body_start = i + 1;
                let body_end = body_start + count;
                if body_end > all.len() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("message stanza claims {count} lines past end of ref"),
                    ));
                }
                let msg = all[body_start..body_end].join("\n");
                if let Some(e) = entries.get_mut(idx) {
                    let e: &mut CoveringEntry = e;
                    e.message = Some(msg);
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("message stanza index {idx} has no segment"),
                    ));
                }
                i = body_end;
                continue;
            }
            if line.is_empty() || line.starts_with('#') {
                i += 1;
                continue;
            }
            let parts: Vec<&str> = line.splitn(6, '\t').collect();
            if parts.len() < 5 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("malformed covering line: {line}"),
                ));
            }
            let (a, a_col) = parse_pos_field(parts[2])?;
            let (b, b_col) = parse_pos_field(parts[3])?;
            // Optional sixth field: the segment name. Absent in refs
            // written by older covmap (and for anonymous segments).
            let name = parts
                .get(5)
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty());
            entries.push(CoveringEntry {
                hash: parts[0].to_string(),
                file: parts[1].to_string(),
                a,
                a_col,
                b,
                b_col,
                label: parts[4].to_string(),
                name,
                message: None,
            });
            i += 1;
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
    /// Partition entropy (theory/main.tex §Complexity). Computed and kept
    /// as a defined covering measure, but intentionally not surfaced in
    /// any command output — it is not actionable on its own. Available
    /// for future use (e.g. an evenness heuristic); `allow(dead_code)`
    /// until a consumer lands.
    #[allow(dead_code)]
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

/// Replace an entry's label. The label is ref-local metadata and not a
/// hash input, so this updates only the covering row: the segment's hash
/// (its content identity) is unchanged, the object store is untouched, and
/// every connection endpoint that referenced the segment — by hash or by
/// name — keeps resolving. Returns the (unchanged) hash.
pub fn relabel(
    _repo: &Repo,
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
    c.entries[idx].label = new_label.to_string();
    Ok(c.entries[idx].hash.clone())
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
    let content = segment::extract_content(&lines, entry.a, entry.a_col, entry.b, entry.b_col)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "segment {} no longer fits the file ({} lines on disk) — use merge/split",
                    entry.loc(),
                    lines.len()
                ),
            )
        })?;
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
    let content = segment::read_object(&repo.objects_dir(), old_hash)?;
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
    let offset: usize = clines[..li]
        .iter()
        .map(|l| segment::char_len(l))
        .sum::<usize>()
        + skip;
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
    // Name/message propagation: the top keeps the original segment's name
    // and message; the bottom is a fresh anonymous, un-messaged segment. (A
    // connection to the named segment therefore follows the top half.)
    c.entries[idx] = CoveringEntry {
        hash: top_hash.clone(),
        file: top.file,
        a: top.a,
        a_col: entry.a_col,
        b: top_b,
        b_col: top_b_col,
        label: top.label,
        name: entry.name.clone(),
        message: entry.message.clone(),
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
            name: None,
            message: None,
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
            format!("segments are not adjacent: {} and {}", e1.loc(), e2.loc()),
        ));
    }
    let c1 = segment::read_object(&repo.objects_dir(), &e1.hash)?;
    let c2 = segment::read_object(&repo.objects_dir(), &e2.hash)?;
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
    // Name propagation: the merged segment inherits the surviving name. If
    // only one side is named, that name carries over (its connection
    // follows the merge). If both are named, the left (earlier) side's
    // name wins and the right's is dropped — warn so the user can re-tag
    // or re-link if that was not intended.
    let merged_name = match (&e1.name, &e2.name) {
        (Some(n1), Some(n2)) => {
            eprintln!(
                "(merge: both segments are named; keeping '@{n1}', dropping '@{n2}' — \
                 any connection to '@{n2}' is now broken)"
            );
            Some(n1.clone())
        }
        (Some(n1), None) => Some(n1.clone()),
        (None, Some(n2)) => Some(n2.clone()),
        (None, None) => None,
    };
    // Message propagation mirrors the name rule: the merged segment keeps the
    // surviving side's message (the earlier/left side's if both have one).
    // Warn on a dropped message so it can be re-recorded if that was not
    // intended.
    let merged_message = match (&e1.message, &e2.message) {
        (Some(m1), Some(_)) => {
            eprintln!("(merge: both segments have a message; keeping the earlier one)");
            Some(m1.clone())
        }
        (Some(m1), None) => Some(m1.clone()),
        (None, Some(m2)) => Some(m2.clone()),
        (None, None) => None,
    };
    c.entries[keep] = CoveringEntry {
        hash: h.clone(),
        file: merged.file,
        a: e1.a,
        a_col: e1.a_col,
        b: e2.b,
        b_col: e2.b_col,
        label: merged.label,
        name: merged_name,
        message: merged_message,
    };
    c.entries.remove(drop);
    Ok(h)
}

/// Outcome of remapping one file's segments through a diff against the
/// current bytes on disk.
pub struct FileRemap {
    /// Replacement entries for the file, in order, each paired with the
    /// content to persist to the object store.
    pub entries: Vec<(CoveringEntry, String)>,
    /// Segments whose recorded content changed (their hash was refreshed).
    pub refreshed: usize,
    /// Segments that mapped to identical content (no hash change).
    pub unchanged: usize,
}

/// Why a file could not be remapped.
pub enum RemapError {
    /// The file's recorded segments already reconstruct the bytes on disk:
    /// there is nothing to remap.
    NotDrifted,
    /// Diff-remap only handles line-aligned segments; these mid-line segments
    /// block it (listed by `loc`).
    CharLevel(Vec<String>),
    /// These segment boundaries fell in changed regions, so their new
    /// positions are unknown. The file is left untouched (all-or-nothing)
    /// and these segments are the unresolved ones to relocate (e.g. by
    /// fingerprint proposal). The whole list of the file's segments is returned for
    /// reporting context.
    Unresolved(Vec<CoveringEntry>),
}

/// Remap one file's segments from the old covering onto the new bytes `n`.
///
/// `old` are the file's current covering entries (any order); `n` is the
/// file's lines on disk (each retaining its trailing newline). Returns
/// replacement entries that re-fit the partition to `n`, carrying every
/// segment's name and label forward, or a `RemapError` describing why it
/// could not (in which case the caller must leave the file untouched).
///
/// Strategy: a line-aligned covering is a sequence of cut lines; each
/// segment's start line is mapped through `diff::line_map`. If every
/// interior boundary lands on a preserved line, the new cut sequence is
/// determined and each segment is re-extracted from `n` (refreshing its
/// content, like `recut`, while its boundaries follow surrounding
/// inserts/deletes).
pub fn remap_file(
    old: &[CoveringEntry],
    o_lines: &[String],
    n_lines: &[String],
) -> Result<FileRemap, RemapError> {
    // Order segments by position.
    let mut segments: Vec<&CoveringEntry> = old.iter().collect();
    segments.sort_by_key(|e| start_key(e.start_pos()));

    // If the recorded segments already reconstruct disk, there is nothing
    // to remap. Check this first so an undrifted file is silently
    // skipped regardless of its segment shape (no spurious char-level
    // warning for files the user never touched).
    let reconstructed: String = o_lines.concat();
    let on_disk: String = n_lines.concat();
    if reconstructed == on_disk {
        return Err(RemapError::NotDrifted);
    }

    // Restriction: only line-aligned segments can be remapped through a
    // line diff. Bail (listing the offenders) if any is mid-line.
    let char_level: Vec<String> = segments
        .iter()
        .filter(|e| !e.is_line_aligned())
        .map(|e| e.loc())
        .collect();
    if !char_level.is_empty() {
        return Err(RemapError::CharLevel(char_level));
    }

    let o_refs: Vec<&str> = o_lines.iter().map(|s| s.as_str()).collect();
    let n_refs: Vec<&str> = n_lines.iter().map(|s| s.as_str()).collect();
    let map = crate::diff::line_map(&o_refs, &n_refs);

    // Map each segment's start line (1-indexed) to a new start line.
    // The first segment always starts at new line 1.
    let mut new_starts: Vec<usize> = Vec::with_capacity(segments.len());
    let mut unresolved: Vec<CoveringEntry> = Vec::new();
    for (i, e) in segments.iter().enumerate() {
        if i == 0 {
            new_starts.push(1);
            continue;
        }
        // Old start line is `e.a` (line-aligned ⇒ a_col is line start).
        match map.get(e.a - 1).copied().flatten() {
            Some(j0) => new_starts.push(j0 + 1), // 0-indexed → 1-indexed
            None => unresolved.push((*e).clone()),
        }
    }
    if !unresolved.is_empty() {
        return Err(RemapError::Unresolved(unresolved));
    }

    // New cut sequence: starts + (file end). Must be strictly increasing
    // and within the new file; LCS monotonicity guarantees this, but we
    // validate defensively. On a defensive failure, report every segment as
    // unresolved (the file is left untouched).
    let n_total = n_lines.len();
    let bail = || RemapError::Unresolved(segments.iter().map(|e| (*e).clone()).collect());
    for w in new_starts.windows(2) {
        if w[1] <= w[0] {
            return Err(bail());
        }
    }
    if new_starts.last().copied().unwrap_or(1) > n_total {
        return Err(bail());
    }

    // Build replacement entries: segment i spans new lines
    // [new_starts[i] .. (new_starts[i+1]-1 or n_total)].
    let mut entries = Vec::with_capacity(segments.len());
    let mut refreshed = 0usize;
    let mut unchanged = 0usize;
    for (i, e) in segments.iter().enumerate() {
        let a = new_starts[i];
        let b = if i + 1 < new_starts.len() {
            new_starts[i + 1] - 1
        } else {
            n_total
        };
        let content = match segment::extract_content(n_lines, a, None, b, None) {
            Some(c) => c,
            None => return Err(bail()),
        };
        let new_hash = segment::hash_of(&content);
        if new_hash == e.hash {
            unchanged += 1;
        } else {
            refreshed += 1;
        }
        entries.push((
            CoveringEntry {
                hash: new_hash,
                file: e.file.clone(),
                a,
                a_col: None,
                b,
                b_col: None,
                label: e.label.clone(),
                name: e.name.clone(),
                message: e.message.clone(),
            },
            content,
        ));
    }
    Ok(FileRemap {
        entries,
        refreshed,
        unchanged,
    })
}

/// Resolve an in-covering address to the index of a segment. The address
/// is either `@name` (exact segment name) or a content-hash prefix
/// (≥2 hex chars, git-style unique-prefix match).
pub fn resolve_segment(c: &Covering, addr: &str) -> io::Result<usize> {
    if let Some(name) = addr.strip_prefix('@') {
        return c
            .entries
            .iter()
            .position(|e| e.name.as_deref() == Some(name))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("no segment named '@{name}' in covering '{}'", c.name),
                )
            });
    }
    // A positional address `file:line[:col]` selects the (unique) segment
    // whose range contains that position — a stable handle that never
    // depends on a segment hash. It contains a `:`; a hash prefix never
    // does, so the presence of `:` disambiguates the two.
    if addr.contains(':') {
        return resolve_position(c, addr);
    }
    let full = resolve_in_covering(c, addr)?;
    c.entries
        .iter()
        .position(|e| e.hash == full)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("hash '{full}' not in covering '{}'", c.name),
            )
        })
}

/// Like `resolve_segment`, but returns the matched segment's current hash.
/// A drop-in for `resolve_in_covering` that additionally accepts `@name`
/// and `file:line[:col]` addresses.
pub fn resolve_segment_hash(c: &Covering, addr: &str) -> io::Result<String> {
    Ok(c.entries[resolve_segment(c, addr)?].hash.clone())
}

/// Resolve a positional address `file:line[:col]` to the index of the
/// segment whose range contains that position. By covering completeness
/// (Segment Lookup) at most one segment qualifies. `col` defaults to 1
/// (the first character of the line).
fn resolve_position(c: &Covering, addr: &str) -> io::Result<usize> {
    let bad = |m: &str| io::Error::new(io::ErrorKind::InvalidInput, m.to_string());
    // Split off line[:col] from the right; the remainder is the file
    // (which may itself contain ':', so we split from the end).
    let parts: Vec<&str> = addr.rsplitn(3, ':').collect(); // reversed
                                                           // parts[0] is the rightmost field.
    let (file, line, col): (String, usize, usize) = if parts.len() >= 3
        && parts[0].chars().all(|ch| ch.is_ascii_digit())
        && parts[1].chars().all(|ch| ch.is_ascii_digit())
        && !parts[0].is_empty()
        && !parts[1].is_empty()
    {
        // file:line:col
        (
            parts[2].to_string(),
            parts[1].parse().map_err(|_| bad("bad line in address"))?,
            parts[0].parse().map_err(|_| bad("bad column in address"))?,
        )
    } else if parts.len() >= 2
        && parts[0].chars().all(|ch| ch.is_ascii_digit())
        && !parts[0].is_empty()
    {
        // file:line  (col defaults to 1)
        let file = addr[..addr.len() - parts[0].len() - 1].to_string();
        (
            file,
            parts[0].parse().map_err(|_| bad("bad line in address"))?,
            1,
        )
    } else {
        return Err(bad(&format!(
            "'{addr}' is not a valid file:line[:col] address"
        )));
    };

    let query = Pos {
        line,
        col: Some(col),
    };
    let hit = c
        .entries
        .iter()
        .position(|e| e.file == file && pos_in_segment(query, e));
    hit.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "no segment of '{file}' covers {}:{} in covering '{}'",
                line, col, c.name
            ),
        )
    })
}

/// Whether character position `p` lies within segment `e`'s range,
/// inclusive, in within-file character order. A `None` start column is
/// the first character of the line; a `None` end column is the end of
/// the line (so any column on the end line qualifies).
fn pos_in_segment(p: Pos, e: &CoveringEntry) -> bool {
    let after_start = p.line > e.a || (p.line == e.a && p.col.unwrap_or(1) >= e.a_col.unwrap_or(1));
    let before_end =
        p.line < e.b || (p.line == e.b && e.b_col.map_or(true, |bc| p.col.unwrap_or(1) <= bc));
    after_start && before_end
}

/// Set, change, or clear a segment's name. `new_name = None` clears it
/// (the segment becomes anonymous). Names are unique within the covering.
/// Renaming is a first-class operation: it does not touch content or
/// hash, so a connection that references the segment by `@name` must be
/// re-pointed with `link`/`unlink` only if the name string itself
/// changes — which is exactly what this records.
pub fn set_name(c: &mut Covering, idx: usize, new_name: Option<&str>) -> io::Result<()> {
    if let Some(n) = new_name {
        if !valid_name(n) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "invalid segment name '{n}': must be non-empty and contain no \
                     space, tab, ':' and not start with '@' or '#'"
                ),
            ));
        }
        if c.entries
            .iter()
            .enumerate()
            .any(|(i, e)| i != idx && e.name.as_deref() == Some(n))
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "segment name '{n}' is already used in covering '{}'",
                    c.name
                ),
            ));
        }
    }
    c.entries[idx].name = new_name.map(|s| s.to_string());
    Ok(())
}

/// Set, change, or clear a segment's free-form message. `new_message =
/// None` (or an empty string) clears it. The message is covering-local
/// metadata that is not part of the segment hash, so this never touches the
/// object store or any connection. Unlike a name it has no uniqueness or
/// character constraints — it is arbitrary (possibly multi-line) text; only
/// a trailing newline is trimmed so the stored form is canonical.
pub fn set_message(c: &mut Covering, idx: usize, new_message: Option<&str>) {
    let normalized = new_message
        .map(|m| m.strip_suffix('\n').unwrap_or(m).to_string())
        .filter(|m| !m.is_empty());
    c.entries[idx].message = normalized;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_repo() -> Repo {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        // A per-process counter makes the path unique even when several
        // tests run in the same nanosecond under the parallel runner.
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        p.push(format!("covmap-covering-test-{nanos}-{seq}"));
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
            name: None,
            message: None,
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
        let top = segment::read_object(&repo.objects_dir(), &c.entries[0].hash).unwrap();
        let bot = segment::read_object(&repo.objects_dir(), &c.entries[1].hash).unwrap();
        assert_eq!(top, "a\n");
        assert_eq!(bot, "b\nc\n");
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn relabel_preserves_hash_and_object() {
        let repo = tmp_repo();
        let mut c = Covering {
            name: "main".into(),
            entries: vec![whole(&repo, "f.txt", "content\n", 1)],
        };
        let old = c.entries[0].hash.clone();
        let object = segment::object_path(&repo.objects_dir(), &old);

        let new = relabel(&repo, &mut c, &old, "reviewed").unwrap();

        assert_eq!(new, old);
        assert_eq!(c.entries[0].hash, old);
        assert_eq!(c.entries[0].label, "reviewed");
        assert_eq!(std::fs::read_to_string(object).unwrap(), "content\n");
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
        let top = segment::read_object(&repo.objects_dir(), &c.entries[0].hash).unwrap();
        let bot = segment::read_object(&repo.objects_dir(), &c.entries[1].hash).unwrap();
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
        // Merging the two halves restores the original content hash,
        // regardless of the labels used while they were split.
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

    // Build a covering entry directly (no object store needed) for the
    // pure remap_file tests. The hash is just a placeholder string.
    fn entry(file: &str, a: usize, b: usize, name: Option<&str>) -> CoveringEntry {
        CoveringEntry {
            hash: format!("h{a}-{b}"),
            file: file.into(),
            a,
            a_col: None,
            b,
            b_col: None,
            label: "unknown".into(),
            name: name.map(|s| s.to_string()),
            message: None,
        }
    }

    fn slines(s: &str) -> Vec<String> {
        s.split_inclusive('\n').map(|l| l.to_string()).collect()
    }

    #[test]
    fn remap_skips_undrifted_file_even_with_char_level_segments() {
        // Regression: an undrifted file must be reported NotDrifted and
        // never trigger the char-level restriction. (A whole-covering
        // `recut --remap` would otherwise emit a spurious warning for an
        // untouched file that happens to have character-level cuts.)
        let lines = slines("alpha beta\ngamma\n");
        // Two segments on line 1, split mid-line at column 6 — char-level.
        let mut e1 = entry("f.txt", 1, 1, None);
        e1.b_col = Some(5); // "alpha"
        let mut e2 = entry("f.txt", 1, 2, None);
        e2.a_col = Some(6); // "beta\n" + "gamma\n"
        let old = vec![e1, e2];
        // o == n (no edit): must be NotDrifted, not CharLevel.
        let r = remap_file(&old, &lines, &lines);
        assert!(matches!(r, Err(RemapError::NotDrifted)));
    }

    #[test]
    fn remap_refuses_char_level_only_when_drifted() {
        let o = slines("alpha beta\ngamma\n");
        let n = slines("alpha beta\ngamma\ndelta\n"); // appended a line → drifted
        let mut e1 = entry("f.txt", 1, 1, None);
        e1.b_col = Some(5);
        let mut e2 = entry("f.txt", 1, 2, None);
        e2.a_col = Some(6);
        let r = remap_file(&[e1, e2], &o, &n);
        assert!(matches!(r, Err(RemapError::CharLevel(_))));
    }

    #[test]
    fn remap_refits_line_aligned_segments_and_keeps_names() {
        // Insert two lines into the first segment; the second segment must
        // shift down and keep its name.
        let o = slines("a\nb\nc\nd\n");
        let n = slines("a\nb\nX\nY\nc\nd\n");
        let s1 = entry("f.txt", 1, 2, Some("top")); // a,b
        let s2 = entry("f.txt", 3, 4, Some("bot")); // c,d
        let fr = match remap_file(&[s1, s2], &o, &n) {
            Ok(fr) => fr,
            Err(_) => panic!("expected a successful remap"),
        };
        assert_eq!(fr.entries.len(), 2);
        // top grew to a..4 (a,b,X,Y), bot shifted to 5..6 (c,d).
        assert_eq!((fr.entries[0].0.a, fr.entries[0].0.b), (1, 4));
        assert_eq!((fr.entries[1].0.a, fr.entries[1].0.b), (5, 6));
        assert_eq!(fr.entries[0].0.name.as_deref(), Some("top"));
        assert_eq!(fr.entries[1].0.name.as_deref(), Some("bot"));
        // Reconstruction equals N.
        let recon: String = fr.entries.iter().map(|(_, c)| c.clone()).collect();
        assert_eq!(recon, n.concat());
    }

    #[test]
    fn remap_reports_moved_segment_as_unresolved() {
        // Move the 2nd segment's content above the 1st: its start line can't
        // map monotonically, so it is unresolved (fingerprint territory).
        let o = slines("aaa\nbbb\nccc\nddd\n");
        let n = slines("ccc\nddd\naaa\nbbb\n");
        let s1 = entry("f.txt", 1, 2, Some("first"));
        let s2 = entry("f.txt", 3, 4, Some("second"));
        let r = remap_file(&[s1, s2], &o, &n);
        assert!(matches!(r, Err(RemapError::Unresolved(_))));
    }

    #[test]
    fn split_keeps_name_on_top_bottom_anonymous() {
        let repo = tmp_repo();
        let mut e = whole(&repo, "f.txt", "a\nb\nc\n", 3);
        e.name = Some("intro".into());
        let mut c = Covering {
            name: "main".into(),
            entries: vec![e],
        };
        let old = c.entries[0].hash.clone();
        split(&repo, &mut c, &old, 2, None, "top", "bot").unwrap();
        assert_eq!(c.entries[0].name.as_deref(), Some("intro"));
        assert_eq!(c.entries[1].name, None);
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn merge_inherits_single_name() {
        let repo = tmp_repo();
        let mut e = whole(&repo, "f.txt", "a\nb\n", 2);
        e.name = Some("whole".into());
        let mut c = Covering {
            name: "main".into(),
            entries: vec![e],
        };
        let old = c.entries[0].hash.clone();
        // split drops the name to the top; clear it, name the bottom, merge.
        let (top, bot) = split(&repo, &mut c, &old, 2, None, "t", "b").unwrap();
        c.entries[0].name = None;
        c.entries[1].name = Some("tail".into());
        let _ = merge(&repo, &mut c, &top, &bot, "m").unwrap();
        assert_eq!(c.entries.len(), 1);
        assert_eq!(c.entries[0].name.as_deref(), Some("tail"));
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn name_roundtrips_through_ref_and_legacy_parses() {
        let repo = tmp_repo();
        std::fs::create_dir_all(repo.coverings_dir()).unwrap();
        let mut named = whole(&repo, "f.txt", "x\n", 1);
        named.name = Some("req-1".into());
        let c = Covering {
            name: "main".into(),
            entries: vec![named],
        };
        c.write(&repo).unwrap();
        // The named segment writes a 6th tab field.
        let raw = std::fs::read_to_string(repo.coverings_dir().join("main")).unwrap();
        assert!(raw.lines().any(|l| l.ends_with("\treq-1")));
        let back = Covering::read(&repo, "main").unwrap();
        assert_eq!(back.entries[0].name.as_deref(), Some("req-1"));

        // A legacy 5-field line (no name) parses as anonymous.
        let legacy = format!("{}\tf.txt\t1\t1\tunknown\n", back.entries[0].hash);
        std::fs::write(repo.coverings_dir().join("legacy"), legacy).unwrap();
        let leg = Covering::read(&repo, "legacy").unwrap();
        assert_eq!(leg.entries[0].name, None);
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn message_roundtrips_through_ref_with_tricky_body() {
        let repo = tmp_repo();
        std::fs::create_dir_all(repo.coverings_dir()).unwrap();
        let mut a = whole(&repo, "a.txt", "x\n", 1);
        a.name = Some("seg-a".into());
        let b = whole(&repo, "b.txt", "y\n", 1);
        let mut c = Covering {
            name: "main".into(),
            entries: vec![a, b],
        };
        // A multi-line body containing a blank line AND a line that begins
        // with '#' — both must survive verbatim thanks to the line-count.
        set_message(&mut c, 0, Some("first line\n\n# looks like a comment"));
        // The second segment stays un-messaged.
        c.write(&repo).unwrap();
        let raw = std::fs::read_to_string(repo.coverings_dir().join("main")).unwrap();
        assert!(raw.contains("# message 0 3"));
        assert!(!raw.contains("# message 1"));
        let back = Covering::read(&repo, "main").unwrap();
        assert_eq!(
            back.entries[0].message.as_deref(),
            Some("first line\n\n# looks like a comment")
        );
        assert_eq!(back.entries[1].message, None);
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn legacy_ref_parses_with_no_messages() {
        let repo = tmp_repo();
        std::fs::create_dir_all(repo.coverings_dir()).unwrap();
        // A ref written by older covmap: only segment lines, no stanzas.
        let legacy = "abc123\tf.txt\t1\t2\tunknown\n";
        std::fs::write(repo.coverings_dir().join("old"), legacy).unwrap();
        let c = Covering::read(&repo, "old").unwrap();
        assert_eq!(c.entries.len(), 1);
        assert_eq!(c.entries[0].message, None);
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn set_message_normalizes_and_clears() {
        let repo = tmp_repo();
        let mut c = Covering {
            name: "main".into(),
            entries: vec![whole(&repo, "f.txt", "x\n", 1)],
        };
        // A trailing newline is trimmed to a canonical form.
        set_message(&mut c, 0, Some("why\n"));
        assert_eq!(c.entries[0].message.as_deref(), Some("why"));
        // Empty string clears (treated as None).
        set_message(&mut c, 0, Some(""));
        assert_eq!(c.entries[0].message, None);
        // Explicit None clears.
        set_message(&mut c, 0, Some("again"));
        set_message(&mut c, 0, None);
        assert_eq!(c.entries[0].message, None);
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn split_keeps_message_on_top_and_merge_keeps_earlier() {
        let repo = tmp_repo();
        let mut e = whole(&repo, "f.txt", "a\nb\nc\nd\n", 4);
        e.message = Some("whole-file rationale".into());
        let mut c = Covering {
            name: "main".into(),
            entries: vec![e],
        };
        let old = c.entries[0].hash.clone();
        // Split: the top keeps the message; the bottom is un-messaged.
        split(&repo, &mut c, &old, 3, None, "top", "bot").unwrap();
        assert_eq!(
            c.entries[0].message.as_deref(),
            Some("whole-file rationale")
        );
        assert_eq!(c.entries[1].message, None);
        // Give the bottom a message, then merge: the earlier (top) wins.
        set_message(&mut c, 1, Some("bottom rationale"));
        let (h1, h2) = (c.entries[0].hash.clone(), c.entries[1].hash.clone());
        merge(&repo, &mut c, &h1, &h2, "merged").unwrap();
        assert_eq!(c.entries.len(), 1);
        assert_eq!(
            c.entries[0].message.as_deref(),
            Some("whole-file rationale")
        );
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn remap_preserves_message() {
        // Insert two lines above a messaged segment; diff-remap must carry
        // the message through the boundary shift (like it carries the name).
        let o = slines("a\nb\nc\nd\n");
        let n = slines("a\nb\nX\nY\nc\nd\n");
        let mut s1 = entry("f.txt", 1, 2, Some("top"));
        s1.message = Some("keep me".into());
        let s2 = entry("f.txt", 3, 4, Some("bot"));
        let fr = match remap_file(&[s1, s2], &o, &n) {
            Ok(fr) => fr,
            Err(_) => panic!("expected a successful remap"),
        };
        assert_eq!(fr.entries[0].0.message.as_deref(), Some("keep me"));
        assert_eq!(fr.entries[1].0.message, None);
        std::fs::remove_dir_all("/tmp/nonexistent-covmap-noop").ok();
    }

    #[test]
    fn endpoint_and_resolution_semantics() {
        let repo = tmp_repo();
        let mut named = whole(&repo, "f.txt", "hi\n", 1);
        named.name = Some("greeting".into());
        let anon = whole(&repo, "g.txt", "bye\n", 1);
        let anon_hash = anon.hash.clone();
        let c = Covering {
            name: "main".into(),
            entries: vec![named, anon],
        };
        // Named segment's endpoint is @name; anonymous segment's is its hash.
        assert_eq!(c.entries[0].endpoint(), "@greeting");
        assert_eq!(c.entries[1].endpoint(), anon_hash);
        // Both resolve now.
        assert!(endpoint_resolves(&c, "@greeting"));
        assert!(endpoint_resolves(&c, &anon_hash));
        // A name endpoint resolves even if the segment's hash changes.
        let mut c2 = c.clone();
        c2.entries[0].hash = "deadbeefdeadbeef".into();
        assert!(endpoint_resolves(&c2, "@greeting"));
        // A missing name / hash does not resolve.
        assert!(!endpoint_resolves(&c, "@nope"));
        assert!(!endpoint_resolves(&c, "0000000000000000"));
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn set_name_enforces_uniqueness_and_validity() {
        let repo = tmp_repo();
        let mut c = Covering {
            name: "main".into(),
            entries: vec![
                whole(&repo, "f.txt", "a\n", 1),
                whole(&repo, "g.txt", "b\n", 1),
            ],
        };
        set_name(&mut c, 0, Some("dup")).unwrap();
        // Duplicate name rejected.
        assert!(set_name(&mut c, 1, Some("dup")).is_err());
        // Invalid names rejected.
        assert!(set_name(&mut c, 1, Some("has space")).is_err());
        assert!(set_name(&mut c, 1, Some("@at")).is_err());
        // Clearing works.
        set_name(&mut c, 0, None).unwrap();
        assert_eq!(c.entries[0].name, None);
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn resolve_segment_accepts_positional_address() {
        let repo = tmp_repo();
        // queue.md split into 1..4, 5..9, 10..13, 14..16; errors mid-line.
        let mk = |a, ac, b, bc, file: &str| CoveringEntry {
            hash: format!("h{file}{a}"),
            file: file.into(),
            a,
            a_col: ac,
            b,
            b_col: bc,
            label: "x".into(),
            name: None,
            message: None,
        };
        let c = Covering {
            name: "main".into(),
            entries: vec![
                mk(1, None, 4, None, "queue.md"),
                mk(5, None, 9, None, "queue.md"),
                mk(10, None, 13, None, "queue.md"),
                mk(14, None, 16, None, "queue.md"),
                // a char-level pair on errors.md line 3
                mk(3, None, 3, Some(16), "errors.md"),
                mk(3, Some(17), 4, None, "errors.md"),
            ],
        };
        // file:line picks the containing segment.
        assert_eq!(resolve_segment(&c, "queue.md:1").unwrap(), 0);
        assert_eq!(resolve_segment(&c, "queue.md:7").unwrap(), 1); // inside 5..9
        assert_eq!(resolve_segment(&c, "queue.md:10").unwrap(), 2);
        assert_eq!(resolve_segment(&c, "queue.md:16").unwrap(), 3);
        // file:line:col picks the right char-level fragment on line 3.
        assert_eq!(resolve_segment(&c, "errors.md:3:5").unwrap(), 4); // cols 1..16
        assert_eq!(resolve_segment(&c, "errors.md:3:20").unwrap(), 5); // cols 17..
                                                                       // out-of-range / unknown file errors out.
        assert!(resolve_segment(&c, "queue.md:99").is_err());
        assert!(resolve_segment(&c, "nope.md:1").is_err());
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn resolve_segment_accepts_name_and_hash() {
        let repo = tmp_repo();
        let mut named = whole(&repo, "f.txt", "a\n", 1);
        named.name = Some("intro".into());
        let anon = whole(&repo, "g.txt", "b\n", 1);
        let anon_hash = anon.hash.clone();
        let c = Covering {
            name: "main".into(),
            entries: vec![named, anon],
        };
        // By @name.
        assert_eq!(resolve_segment(&c, "@intro").unwrap(), 0);
        // By hash prefix (≥2 chars) — what plain `recut` relied on before.
        assert_eq!(resolve_segment(&c, &anon_hash[..6]).unwrap(), 1);
        // Misses error out.
        assert!(resolve_segment(&c, "@nope").is_err());
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
            Pos {
                line: 2,
                col: Some(5)
            },
            Pos {
                line: 2,
                col: Some(6)
            }
        ));
        // col None and col Some(1) denote the same start point.
        assert!(positions_adjacent(
            Pos { line: 2, col: None },
            Pos {
                line: 3,
                col: Some(1)
            }
        ));
        // A gap is not adjacency.
        assert!(!positions_adjacent(
            Pos {
                line: 2,
                col: Some(5)
            },
            Pos {
                line: 2,
                col: Some(7)
            }
        ));
    }
}
