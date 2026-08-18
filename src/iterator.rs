//! Iterators: coverings frozen into ordered, stateful worklists.
//!
//! See covmap/theory/main.tex §Iterators for the model. An iterator is a
//! thin layer *over* a covering: it never mutates segments, coverings, or
//! connections. It records, at freeze time, one **task** per included
//! segment — plus a per-task state and a cursor — so an external consumer
//! (an agent, a human, a script) can pull one task at a time, work it, and
//! mark it resolved. Iterator creation ensures every selected object is
//! cached; retaining those task objects append-only keeps their content
//! retrievable (`cat-segment`) no matter how the files or covering later
//! change.
//!
//! Stored at `.covmap/iterators/<name>`:
//!
//! ```text
//! # covering=<C>
//! # order=file|label
//! # label=<l>              (present only for a label-filtered freeze)
//! # cursor=<index>         (the current task, 0-based; absent after `new`)
//! <hash>\t<file>\t<a>\t<b>\t<label>\t<name>\t<state>[\t<note>]
//! ...
//! ```
//!
//! The `<a>`/`<b>` fields use the same `line[:col]` encoding as covering
//! refs, so a char-level segment round-trips. `<name>` is empty for an
//! anonymous segment. `<state>` is `pending`, `done`, or `skipped`.
//!
//! The task list is built once, in the chosen `--order`, at `new` time, and
//! that order is frozen for the life of the iterator (the theory's total
//! order `<` over the tasks). No later command reorders it — `choose` moves
//! the *cursor* to another task, never the tasks themselves — so a task's
//! position is a stable name for it.
//!
//! The cursor is therefore stored as the 0-based task index into that frozen
//! list rather than as a hash: two byte-identical, equally-labelled segments
//! share a hash, so a hash cursor would be ambiguous within one worklist,
//! whereas an index names the task unambiguously, matching the theory's
//! cursor `γ ∈ τ ∪ {⊥}`.
//!
//! `iterators/` is a purely additive directory an older covmap never reads,
//! so introducing it needs no `FORMAT_VERSION` bump (like the char-column
//! and name fields before it).

use crate::covering::{self, Covering, CoveringEntry, Pos};
use crate::repo::Repo;
use crate::segment::HASH_LEN;
use crate::store::atomic_write;
use std::fs;
use std::io;

/// The state of a task in a worklist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState {
    Pending,
    Done,
    Skipped,
}

impl TaskState {
    fn as_str(self) -> &'static str {
        match self {
            TaskState::Pending => "pending",
            TaskState::Done => "done",
            TaskState::Skipped => "skipped",
        }
    }

    fn parse(s: &str) -> io::Result<TaskState> {
        match s {
            "pending" => Ok(TaskState::Pending),
            "done" => Ok(TaskState::Done),
            "skipped" => Ok(TaskState::Skipped),
            other => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown task state '{other}'"),
            )),
        }
    }
}

/// The enumeration order chosen at freeze time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// By path, then by start position — natural reading order (default).
    File,
    /// By label, then path, then start position — groups a concern.
    Label,
}

impl Order {
    pub fn as_str(self) -> &'static str {
        match self {
            Order::File => "file",
            Order::Label => "label",
        }
    }

    pub fn parse(s: &str) -> io::Result<Order> {
        match s {
            "file" => Ok(Order::File),
            "label" => Ok(Order::Label),
            other => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unknown order '{other}' (want 'file' or 'label')"),
            )),
        }
    }
}

/// One frozen segment plus its worklist bookkeeping.
#[derive(Clone, Debug)]
pub struct Task {
    pub hash: String,
    pub file: String,
    pub a: usize,
    pub a_col: Option<usize>,
    pub b: usize,
    pub b_col: Option<usize>,
    pub label: String,
    pub name: Option<String>,
    pub state: TaskState,
    pub note: String,
}

impl Task {
    /// `file:a..b` (with columns when set), mirroring `CoveringEntry::loc`.
    pub fn loc(&self) -> String {
        format!(
            "{}:{}..{}",
            self.file,
            covering::fmt_pos(Pos {
                line: self.a,
                col: self.a_col
            }),
            covering::fmt_pos(Pos {
                line: self.b,
                col: self.b_col
            }),
        )
    }

    /// The stable handle a consumer keys off: `@name` when named, else the
    /// short hash.
    pub fn handle(&self) -> String {
        match &self.name {
            Some(n) => format!("@{n}"),
            None => {
                let n = self.hash.len().min(HASH_LEN);
                self.hash[..n].to_string()
            }
        }
    }

    /// The one-line, parseable print form: `<@name|hash>\t<label>\t<loc>`.
    pub fn print_line(&self) -> String {
        format!(
            "{}\t{}\t{}",
            self.handle(),
            if self.label.is_empty() {
                "-"
            } else {
                &self.label
            },
            self.loc()
        )
    }

    /// `print_line` plus the task's state (and note, when present) — the
    /// audit form used by `peek --all`, which lists resolved tasks too.
    pub fn print_line_stateful(&self) -> String {
        let base = self.print_line();
        if self.note.is_empty() {
            format!("{base}\t{}", self.state.as_str())
        } else {
            format!("{base}\t{}\t{}", self.state.as_str(), self.note)
        }
    }
}

/// A covering frozen into an ordered, stateful worklist.
#[derive(Clone, Debug)]
pub struct Iter {
    pub name: String,
    pub covering: String,
    pub order: Order,
    pub label: Option<String>,
    /// The task the consumer is on, as a 0-based index into `tasks`.
    /// `None` before the first `next` (the theory's `⊥`).
    pub cursor: Option<usize>,
    pub tasks: Vec<Task>,
}

/// Freeze a covering's (optionally label-filtered) segments into an ordered
/// list of `pending` tasks. Pure over the covering — a snapshot.
pub fn freeze(cov: &Covering, order: Order, label: Option<&str>) -> Vec<Task> {
    let mut entries: Vec<&CoveringEntry> = cov
        .entries
        .iter()
        .filter(|e| label.map(|l| e.label == l).unwrap_or(true))
        .collect();
    match order {
        Order::File => {
            entries.sort_by(|x, y| x.file.cmp(&y.file).then(x.sort_key().cmp(&y.sort_key())))
        }
        Order::Label => entries.sort_by(|x, y| {
            x.label
                .cmp(&y.label)
                .then(x.file.cmp(&y.file))
                .then(x.sort_key().cmp(&y.sort_key()))
        }),
    }
    entries
        .into_iter()
        .map(|e| Task {
            hash: e.hash.clone(),
            file: e.file.clone(),
            a: e.a,
            a_col: e.a_col,
            b: e.b,
            b_col: e.b_col,
            label: e.label.clone(),
            name: e.name.clone(),
            state: TaskState::Pending,
            note: String::new(),
        })
        .collect()
}

impl Iter {
    pub fn write(&self, repo: &Repo) -> io::Result<()> {
        let path = repo.iterators_dir().join(&self.name);
        let mut buf = String::new();
        buf.push_str(&format!("# covering={}\n", self.covering));
        buf.push_str(&format!("# order={}\n", self.order.as_str()));
        if let Some(l) = &self.label {
            buf.push_str(&format!("# label={l}\n"));
        }
        if let Some(i) = self.cursor {
            buf.push_str(&format!("# cursor={i}\n"));
        }
        for t in &self.tasks {
            let a = covering::fmt_pos(Pos {
                line: t.a,
                col: t.a_col,
            });
            let b = covering::fmt_pos(Pos {
                line: t.b,
                col: t.b_col,
            });
            let name = t.name.as_deref().unwrap_or("");
            let base = format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                t.hash,
                t.file,
                a,
                b,
                t.label,
                name,
                t.state.as_str()
            );
            if t.note.is_empty() {
                buf.push_str(&base);
                buf.push('\n');
            } else {
                buf.push_str(&format!("{base}\t{}\n", t.note));
            }
        }
        atomic_write(&path, &buf)
    }

    pub fn read(repo: &Repo, name: &str) -> io::Result<Iter> {
        let path = repo.iterators_dir().join(name);
        let raw = fs::read_to_string(&path).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!("iterator '{name}' not found at {}", path.display()),
            )
        })?;
        let mut covering = String::new();
        let mut order = Order::File;
        let mut label: Option<String> = None;
        let mut cursor: Option<usize> = None;
        let mut tasks = Vec::new();
        for line in raw.lines() {
            if let Some(v) = line.strip_prefix("# covering=") {
                covering = v.to_string();
            } else if let Some(v) = line.strip_prefix("# order=") {
                order = Order::parse(v)?;
            } else if let Some(v) = line.strip_prefix("# label=") {
                label = Some(v.to_string());
            } else if let Some(v) = line.strip_prefix("# cursor=") {
                cursor = Some(v.trim().parse().map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("bad cursor index '{v}'"),
                    )
                })?);
            } else if line.is_empty() || line.starts_with('#') {
                continue;
            } else {
                let parts: Vec<&str> = line.splitn(8, '\t').collect();
                if parts.len() < 7 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("malformed iterator line: {line}"),
                    ));
                }
                let (a, a_col) = covering::parse_pos_field(parts[2])?;
                let (b, b_col) = covering::parse_pos_field(parts[3])?;
                let name = if parts[5].is_empty() {
                    None
                } else {
                    Some(parts[5].to_string())
                };
                tasks.push(Task {
                    hash: parts[0].to_string(),
                    file: parts[1].to_string(),
                    a,
                    a_col,
                    b,
                    b_col,
                    label: parts[4].to_string(),
                    name,
                    state: TaskState::parse(parts[6])?,
                    note: parts.get(7).copied().unwrap_or("").to_string(),
                });
            }
        }
        // A stored cursor must index an existing task; drop a stale one
        // defensively rather than panicking on later access.
        if let Some(i) = cursor {
            if i >= tasks.len() {
                cursor = None;
            }
        }
        Ok(Iter {
            name: name.to_string(),
            covering,
            order,
            label,
            cursor,
            tasks,
        })
    }

    pub fn list(repo: &Repo) -> io::Result<Vec<String>> {
        let mut out = Vec::new();
        let dir = repo.iterators_dir();
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

    /// Index of the `<`-least pending task, if any.
    pub fn first_pending(&self) -> Option<usize> {
        self.tasks
            .iter()
            .position(|t| t.state == TaskState::Pending)
    }

    /// Whether the cursor points at a task that is still pending — i.e. the
    /// current task is *active* and must be resolved before `next`.
    pub fn current_is_pending(&self) -> bool {
        matches!(self.cursor, Some(i) if self.tasks[i].state == TaskState::Pending)
    }

    /// Index of the active task (the cursor's task, when it is pending), if
    /// any. There is at most one — a single cursor — so `status` names it
    /// rather than counting it.
    pub fn active_index(&self) -> Option<usize> {
        match self.cursor {
            Some(i) if self.tasks[i].state == TaskState::Pending => Some(i),
            _ => None,
        }
    }

    /// No pending task remains — the complete-enumeration exhaustion state.
    pub fn is_finished(&self) -> bool {
        self.first_pending().is_none()
    }

    /// One-line human summary shared by `status` and the `iter` listing.
    ///
    /// The fraction counts *resolved* tasks (done + skipped), so it reaches
    /// `total/total` exactly when the sweep is finished — `done/total` would
    /// read as incomplete forever on any sweep that skips a task. The tail
    /// states completion: `— finished` when nothing is pending, otherwise the
    /// active task (or the pending count when the cursor is unset).
    pub fn summary(&self) -> String {
        let (done, _active, skipped, total) = self.counts();
        let resolved = done + skipped;
        let head = format!("{resolved}/{total} resolved ({done} done, {skipped} skipped)");
        let tail = if self.is_finished() {
            "finished".to_string()
        } else if let Some(i) = self.active_index() {
            format!("active {}", self.tasks[i].handle())
        } else {
            let pending = total - resolved;
            format!("{pending} pending, no active task")
        };
        format!("{head} — {tail}")
    }

    /// Every pending task's index, in frozen order — the remaining worklist
    /// `peek` shows. Includes the active task (the one the cursor is on), so
    /// `peek --content` can read the task currently in hand.
    pub fn pending_indices(&self) -> Vec<usize> {
        self.tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.state == TaskState::Pending)
            .map(|(i, _)| i)
            .collect()
    }

    /// Resolve the cursor's task to `state`, replacing its note with `note`
    /// (so re-resolving without a message clears a stale one). Returns the
    /// resolved task's index, or an error if the cursor is unset.
    pub fn resolve_current(&mut self, state: TaskState, note: Option<String>) -> io::Result<usize> {
        let idx = self.cursor.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("cursor unset — run `covmap iter {} next` first", self.name),
            )
        })?;
        self.tasks[idx].state = state;
        self.tasks[idx].note = note.unwrap_or_default();
        Ok(idx)
    }

    /// `(done, active, skipped, total)` counts for `status`.
    pub fn counts(&self) -> (usize, usize, usize, usize) {
        let done = self
            .tasks
            .iter()
            .filter(|t| t.state == TaskState::Done)
            .count();
        let skipped = self
            .tasks
            .iter()
            .filter(|t| t.state == TaskState::Skipped)
            .count();
        let active = usize::from(self.current_is_pending());
        (done, active, skipped, self.tasks.len())
    }

    /// Resolve a task address (`@name` or a hash prefix ≥2 chars) to its
    /// index in the frozen order.
    pub fn resolve_task(&self, addr: &str) -> io::Result<usize> {
        if let Some(name) = addr.strip_prefix('@') {
            return self
                .tasks
                .iter()
                .position(|t| t.name.as_deref() == Some(name))
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("no task named '@{name}' in iterator '{}'", self.name),
                    )
                });
        }
        if addr.len() < 2 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "task address must be '@name' or a hash prefix of ≥2 chars",
            ));
        }
        let matches: Vec<usize> = self
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.hash.starts_with(addr))
            .map(|(i, _)| i)
            .collect();
        match matches.len() {
            0 => Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("no task matches '{addr}' in iterator '{}'", self.name),
            )),
            1 => Ok(matches[0]),
            n => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("ambiguous: {n} tasks match '{addr}' (use '@name')"),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segment::{self, Segment};

    fn tmp_repo() -> Repo {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        p.push(format!("covmap-iterator-test-{nanos}-{seq}"));
        std::fs::create_dir_all(p.join("objects")).unwrap();
        std::fs::create_dir_all(p.join("iterators")).unwrap();
        Repo { root: p }
    }

    fn seg_entry(
        repo: &Repo,
        file: &str,
        content: &str,
        label: &str,
        name: Option<&str>,
    ) -> CoveringEntry {
        let n = content.split_inclusive('\n').count().max(1);
        let seg = Segment {
            file: file.into(),
            a: 1,
            b: n,
            label: label.into(),
            content: content.into(),
        };
        let hash = segment::write_object(&repo.objects_dir(), &seg).unwrap();
        CoveringEntry {
            hash,
            file: file.into(),
            a: 1,
            a_col: None,
            b: n,
            b_col: None,
            label: label.into(),
            name: name.map(|s| s.to_string()),
            message: None,
        }
    }

    fn covering_of(entries: Vec<CoveringEntry>) -> Covering {
        Covering {
            name: "main".into(),
            entries,
        }
    }

    #[test]
    fn freeze_file_order_sorts_by_path_then_position() {
        let repo = tmp_repo();
        let mut e_b = seg_entry(&repo, "b.md", "x\n", "unknown", None);
        e_b.a = 3;
        e_b.b = 3;
        let e_a2 = {
            let mut e = seg_entry(&repo, "a.md", "y\n", "unknown", None);
            e.a = 10;
            e.b = 10;
            e
        };
        let e_a1 = {
            let mut e = seg_entry(&repo, "a.md", "z\n", "unknown", None);
            e.a = 1;
            e.b = 1;
            e
        };
        let cov = covering_of(vec![e_b, e_a2, e_a1]);
        let tasks = freeze(&cov, Order::File, None);
        // a.md:1, a.md:10, b.md:3
        assert_eq!(tasks[0].file, "a.md");
        assert_eq!(tasks[0].a, 1);
        assert_eq!(tasks[1].file, "a.md");
        assert_eq!(tasks[1].a, 10);
        assert_eq!(tasks[2].file, "b.md");
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn freeze_label_filter_narrows_the_set() {
        let repo = tmp_repo();
        let cov = covering_of(vec![
            seg_entry(&repo, "a.md", "1\n", "unknown", None),
            seg_entry(&repo, "b.md", "2\n", "reviewed", None),
            seg_entry(&repo, "c.md", "3\n", "unknown", None),
        ]);
        let tasks = freeze(&cov, Order::File, Some("unknown"));
        assert_eq!(tasks.len(), 2);
        assert!(tasks.iter().all(|t| t.label == "unknown"));
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn roundtrip_through_ref_preserves_everything() {
        let repo = tmp_repo();
        let cov = covering_of(vec![
            seg_entry(&repo, "a.md", "1\n", "unknown", Some("intro")),
            seg_entry(&repo, "b.md", "2\n", "reviewed", None),
        ]);
        let mut it = Iter {
            name: "sweep".into(),
            covering: "main".into(),
            order: Order::File,
            label: None,
            cursor: Some(1),
            tasks: freeze(&cov, Order::File, None),
        };
        it.tasks[0].state = TaskState::Done;
        it.tasks[0].note = "looks good".into();
        it.tasks[1].state = TaskState::Skipped;
        it.write(&repo).unwrap();
        let back = Iter::read(&repo, "sweep").unwrap();
        assert_eq!(back.covering, "main");
        assert_eq!(back.cursor, Some(1));
        assert_eq!(back.tasks.len(), 2);
        assert_eq!(back.tasks[0].name.as_deref(), Some("intro"));
        assert_eq!(back.tasks[0].state, TaskState::Done);
        assert_eq!(back.tasks[0].note, "looks good");
        assert_eq!(back.tasks[1].state, TaskState::Skipped);
        assert_eq!(back.tasks[1].name, None);
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn enumeration_counts_and_finished() {
        let repo = tmp_repo();
        let cov = covering_of(vec![
            seg_entry(&repo, "a.md", "1\n", "unknown", None),
            seg_entry(&repo, "b.md", "2\n", "unknown", None),
            seg_entry(&repo, "c.md", "3\n", "unknown", None),
        ]);
        let mut it = Iter {
            name: "sweep".into(),
            covering: "main".into(),
            order: Order::File,
            label: None,
            cursor: None,
            tasks: freeze(&cov, Order::File, None),
        };
        assert!(!it.is_finished());
        // next lands on task 0.
        assert_eq!(it.first_pending(), Some(0));
        it.cursor = Some(0);
        assert!(it.current_is_pending());
        // resolve 0, next lands on 1.
        it.tasks[0].state = TaskState::Done;
        assert!(!it.current_is_pending());
        assert_eq!(it.first_pending(), Some(1));
        it.tasks[1].state = TaskState::Skipped;
        it.tasks[2].state = TaskState::Done;
        assert!(it.is_finished());
        let (done, active, skipped, total) = it.counts();
        assert_eq!((done, active, skipped, total), (2, 0, 1, 3));
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn summary_counts_resolved_and_marks_state() {
        let repo = tmp_repo();
        let cov = covering_of(vec![
            seg_entry(&repo, "a.md", "1\n", "unknown", None),
            seg_entry(&repo, "b.md", "2\n", "unknown", None),
            seg_entry(&repo, "c.md", "3\n", "unknown", None),
        ]);
        let mut it = Iter {
            name: "sweep".into(),
            covering: "main".into(),
            order: Order::File,
            label: None,
            cursor: None,
            tasks: freeze(&cov, Order::File, None),
        };
        // Fresh: nothing resolved, no cursor -> pending count.
        assert_eq!(
            it.summary(),
            "0/3 resolved (0 done, 0 skipped) — 3 pending, no active task"
        );
        // Cursor on a pending task -> named active.
        it.cursor = Some(0);
        let handle = it.tasks[0].handle();
        assert_eq!(
            it.summary(),
            format!("0/3 resolved (0 done, 0 skipped) — active {handle}")
        );
        // Finished with a skip: fraction reaches total/total, marked finished.
        it.tasks[0].state = TaskState::Done;
        it.tasks[1].state = TaskState::Skipped;
        it.tasks[2].state = TaskState::Done;
        assert_eq!(it.summary(), "3/3 resolved (2 done, 1 skipped) — finished");
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn peek_includes_the_active_task() {
        let repo = tmp_repo();
        let cov = covering_of(vec![
            seg_entry(&repo, "a.md", "1\n", "unknown", None),
            seg_entry(&repo, "b.md", "2\n", "unknown", None),
            seg_entry(&repo, "c.md", "3\n", "unknown", None),
        ]);
        let mut it = Iter {
            name: "sweep".into(),
            covering: "main".into(),
            order: Order::File,
            label: None,
            cursor: Some(0), // active on task 0
            tasks: freeze(&cov, Order::File, None),
        };
        // All three are pending; peek lists every pending task in order,
        // including the active one (index 0).
        assert_eq!(it.pending_indices(), vec![0, 1, 2]);
        // Resolving the active task drops it from the pending list.
        it.tasks[0].state = TaskState::Done;
        assert_eq!(it.pending_indices(), vec![1, 2]);
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn active_index_is_the_pending_cursor_or_none() {
        let repo = tmp_repo();
        let cov = covering_of(vec![
            seg_entry(&repo, "a.md", "1\n", "unknown", None),
            seg_entry(&repo, "b.md", "2\n", "unknown", None),
        ]);
        let mut it = Iter {
            name: "sweep".into(),
            covering: "main".into(),
            order: Order::File,
            label: None,
            cursor: None,
            tasks: freeze(&cov, Order::File, None),
        };
        // No cursor → no active task.
        assert_eq!(it.active_index(), None);
        // Cursor on a pending task → that task is active.
        it.cursor = Some(0);
        assert_eq!(it.active_index(), Some(0));
        // Cursor on a resolved task → no active task (it is not pending).
        it.tasks[0].state = TaskState::Done;
        assert_eq!(it.active_index(), None);
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn print_line_stateful_appends_state_and_note() {
        let repo = tmp_repo();
        let cov = covering_of(vec![seg_entry(
            &repo,
            "a.md",
            "1\n",
            "unknown",
            Some("intro"),
        )]);
        let mut it = Iter {
            name: "sweep".into(),
            covering: "main".into(),
            order: Order::File,
            label: None,
            cursor: None,
            tasks: freeze(&cov, Order::File, None),
        };
        // Pending, no note: state tacked on, no trailing note field.
        assert!(it.tasks[0].print_line_stateful().ends_with("\tpending"));
        // Done with a note: both appended.
        it.tasks[0].state = TaskState::Done;
        it.tasks[0].note = "looks good".into();
        assert!(it.tasks[0]
            .print_line_stateful()
            .ends_with("\tdone\tlooks good"));
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn resolving_replaces_note_and_clears_when_absent() {
        let repo = tmp_repo();
        let cov = covering_of(vec![seg_entry(&repo, "a.md", "1\n", "unknown", None)]);
        let mut it = Iter {
            name: "sweep".into(),
            covering: "main".into(),
            order: Order::File,
            label: None,
            cursor: Some(0),
            tasks: freeze(&cov, Order::File, None),
        };
        // done -m "verified" records the note.
        let idx = it
            .resolve_current(TaskState::Done, Some("verified".into()))
            .unwrap();
        assert_eq!(idx, 0);
        assert_eq!(it.tasks[0].state, TaskState::Done);
        assert_eq!(it.tasks[0].note, "verified");
        // skip (no message) flips the state AND clears the stale note.
        it.resolve_current(TaskState::Skipped, None).unwrap();
        assert_eq!(it.tasks[0].state, TaskState::Skipped);
        assert_eq!(it.tasks[0].note, "");
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn resolve_current_errors_when_cursor_unset() {
        let repo = tmp_repo();
        let cov = covering_of(vec![seg_entry(&repo, "a.md", "1\n", "unknown", None)]);
        let mut it = Iter {
            name: "sweep".into(),
            covering: "main".into(),
            order: Order::File,
            label: None,
            cursor: None,
            tasks: freeze(&cov, Order::File, None),
        };
        assert!(it.resolve_current(TaskState::Done, None).is_err());
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn resolve_task_by_name_and_hash_prefix() {
        let repo = tmp_repo();
        let cov = covering_of(vec![
            seg_entry(&repo, "a.md", "hello\n", "unknown", Some("intro")),
            seg_entry(&repo, "b.md", "world\n", "unknown", None),
        ]);
        let it = Iter {
            name: "sweep".into(),
            covering: "main".into(),
            order: Order::File,
            label: None,
            cursor: None,
            tasks: freeze(&cov, Order::File, None),
        };
        assert_eq!(it.resolve_task("@intro").unwrap(), 0);
        let h = it.tasks[1].hash.clone();
        assert_eq!(it.resolve_task(&h[..6]).unwrap(), 1);
        assert!(it.resolve_task("@nope").is_err());
        std::fs::remove_dir_all(&repo.root).ok();
    }

    #[test]
    fn read_drops_out_of_range_cursor() {
        let repo = tmp_repo();
        let cov = covering_of(vec![seg_entry(&repo, "a.md", "1\n", "unknown", None)]);
        let it = Iter {
            name: "sweep".into(),
            covering: "main".into(),
            order: Order::File,
            label: None,
            cursor: Some(0),
            tasks: freeze(&cov, Order::File, None),
        };
        it.write(&repo).unwrap();
        // Hand-corrupt the cursor to an out-of-range index.
        let p = repo.iterators_dir().join("sweep");
        let raw = std::fs::read_to_string(&p).unwrap();
        let raw = raw.replace("# cursor=0", "# cursor=9");
        std::fs::write(&p, raw).unwrap();
        let back = Iter::read(&repo, "sweep").unwrap();
        assert_eq!(back.cursor, None);
        std::fs::remove_dir_all(&repo.root).ok();
    }
}
