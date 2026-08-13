//! Command dispatch.
//!
//! See covmap/theory/main.tex §5 for the model. Output is line-oriented
//! and pipe-friendly.

use crate::connection::{Connection, Link};
use crate::covering::{self, Covering, CoveringEntry};
use crate::iterator::{self, Iter, Order, Task, TaskState};
use crate::repo::Repo;
use crate::segment::{self, Segment, HASH_LEN};
use crate::walk::{self, DEFAULT_LABEL};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

pub fn dispatch(args: Vec<String>) -> io::Result<u8> {
    if args.len() < 2 {
        print_usage();
        return Ok(2);
    }
    let cmd = args[1].as_str();
    let rest: Vec<String> = args[2..].to_vec();

    // Top-level --version / -V.
    if matches!(cmd, "--version" | "-V") {
        println!("covmap {}", env!("CARGO_PKG_VERSION"));
        return Ok(0);
    }

    // If the user typed `covmap <cmd> -h|--help`, print the usage line
    // for that command and exit 0 without invoking it.
    if rest.iter().any(|a| a == "-h" || a == "--help") {
        if let Some(line) = command_help_line(cmd) {
            println!("{line}");
            return Ok(0);
        }
    }

    match cmd {
        "init" => cmd_init(&rest),
        "cover" => cmd_cover(&rest),
        "status" => cmd_status(&rest),
        "show" => cmd_show(&rest),
        "ls" => cmd_ls(&rest),
        "grep" => cmd_grep(&rest),
        "diff" => cmd_diff(&rest),
        "label" => cmd_label(&rest),
        "name" => cmd_name(&rest),
        "annotate" => cmd_annotate(&rest),
        "split" => cmd_split(&rest),
        "merge" => cmd_merge(&rest),
        "recut" => cmd_recut(&rest),
        "link" => cmd_link(&rest),
        "unlink" => cmd_unlink(&rest),
        "connect" => cmd_connect(&rest),
        "prune" => cmd_prune(&rest),
        "iter" => cmd_iter(&rest),
        "html" => cmd_html(&rest),
        "migrate" => cmd_migrate(&rest),
        // plumbing
        "hash-segment" => cmd_hash_segment(&rest),
        "cat-segment" => cmd_cat_segment(&rest),
        "ls-segments" => cmd_ls_segments(&rest),
        "ls-links" => cmd_ls_links(&rest),
        "rev-parse" => cmd_rev_parse(&rest),
        "fsck" => cmd_fsck(&rest),
        "help" | "-h" | "--help" => {
            // `covmap help <cmd>` prints that command's focused synopsis
            // (like `covmap <cmd> --help`); bare `covmap help` prints the
            // full usage page.
            match rest.first() {
                Some(sub) => match command_help_line(sub) {
                    Some(line) => {
                        println!("{line}");
                        Ok(0)
                    }
                    None => {
                        eprintln!("unknown command: {sub}");
                        print_usage();
                        Ok(2)
                    }
                },
                None => {
                    print_usage();
                    Ok(0)
                }
            }
        }
        _ => {
            eprintln!("unknown command: {cmd}");
            print_usage();
            Ok(2)
        }
    }
}

/// Per-command synopsis printed by `covmap <cmd> --help`. Returns
/// `None` for unknown commands (so dispatch can fall through).
fn command_help_line(cmd: &str) -> Option<&'static str> {
    Some(match cmd {
        "init" => "covmap init                                  create .covmap/ in the cwd",
        "cover" => "covmap cover [<covering>]                    add one whole-file `unknown` segment per missing file",
        "status" => "covmap status [<covering>]                   cardinality, lines, drift, broken links",
        "show" => "covmap show <addr>                           segment | covering | connection at <addr>",
        "ls" => "covmap ls [-l] [<covering>]                  list segments",
        "grep" => "covmap grep [-l] [-c <C>] <pattern>          segments whose label or content matches",
        "diff" => "covmap diff <C1> <C2>                        added / removed / shared",
        "label" => "covmap label <C>:<h> <label>                 relabel a segment",
        "name" => "covmap name <C>:<h|@name> <name|->         set, rename, or clear (-) a segment's name",
        "annotate" => "covmap annotate <C>:<addr> [-m <msg>...] | -   set, replace, clear (-), or print a segment's message",
        "split" => "covmap split <C>:<h> <line[:col]> [<top>] [<bot>] split one segment at a line or character position",
        "merge" => "covmap merge <C>:<h1> <C>:<h2> [<label>]     merge two adjacent segments",
        "recut" => "covmap recut <C>:<h|@name> | recut --remap [<C>[:<a>]]   refresh a segment, or diff-remap a covering's segments onto the edited file",
        "link" => {
            "covmap link <K> <A> <B> [<note>]            add a link to connection K\n  <A> and <B>: @name, hash prefix, or file:line[:col] in that side's covering"
        }
        "unlink" => {
            "covmap unlink <K> <A> <B>                   remove a link\n  <A> and <B>: @name, hash prefix, or file:line[:col] in that side's covering"
        }
        "connect" => {
            "covmap connect <C_A> [<repo>:]<C_B> [<K>]   create an empty connection; omit <repo>: for a same-repository target"
        }
        "prune" => "covmap prune <K>                             drop links whose endpoints are no longer in their coverings",
        "iter" => {
            "covmap iter [<name> <verb> ...]              freeze a covering into an ordered worklist (bare `iter` lists all; `iter <name>` with no verb prints the verb list)"
        }
        "html" => "covmap html [-o <path>] [--covering <C>] [--connection <K>]   render a static HTML browser (one connection; auto-selected if only one exists)",
        "migrate" => "covmap migrate                              upgrade .covmap/ on-disk format (1 → 2: content-only segment hash)",
        "hash-segment" => "covmap hash-segment <file> <a[:col]> <b[:col]> [-w]",
        "cat-segment" => "covmap cat-segment <hash>",
        "ls-segments" => "covmap ls-segments <covering>",
        "ls-links" => "covmap ls-links <connection>",
        "rev-parse" => "covmap rev-parse <addr>",
        "fsck" => "covmap fsck [--broken-links]",
        _ => return None,
    })
}

fn print_usage() {
    eprintln!(
        "covmap {} — coverings and connections between repositories

Run `covmap <command> --help` for the synopsis of a single command.
Run `covmap --version` to print the version.

PORCELAIN
  init                                create .covmap/ in the cwd
  cover [<covering>]                  add one whole-file `unknown` segment per missing file
  status [<covering>]                 cardinality, lines, drift, broken links
  show <addr>                         segment | covering | connection at <addr>
  ls [-l] [<covering>]                list segments
  grep [-l] [-c <C>] <pattern>        segments whose label or content matches
  diff <C1> <C2>                      added / removed / shared
  label <C>:<h> <label>               relabel a segment
  name <C>:<h|@name> <name|->         set, rename, or clear (-) a segment's name
  annotate <C>:<addr> [-m <msg>...]   set/replace/clear (-)/print a segment's free-form message
  split <C>:<h> <line[:col]> [<lbl_top>] [<lbl_bot>]   split one segment at a line or character position
  merge <C>:<h1> <C>:<h2> [<label>]   merge two adjacent segments
  recut <C>:<h|@name>                 refresh a segment's hash from current disk bytes
  recut --remap [<C>[:<addr>]] [--threshold <f>]   diff-remap; for unresolved segments, print fingerprint relocation proposals (propose-only)
  link <K> <A> <B> [<note>]           add a link; endpoints accept @name, hash prefix, or position
  unlink <K> <A> <B>                  remove a link by @name, hash prefix, or position
  connect <C_A> [<repo>:]<C_B> [<K>]  create empty connection; omit <repo>: for same-repo
  prune <K>                           drop links whose endpoints are no longer in their coverings
  iter [<name> <verb> ...]            freeze a covering into an ordered, resumable worklist
  html [-o <path>] [--covering <C>] [--connection <K>]   render a static HTML browser
  migrate                            upgrade .covmap/ on-disk format (1 → 2)

ITERATORS (covmap iter <name> <verb>)
  <name> new <C> [--order file|label] [--label <l>]   freeze covering C into a worklist
  <name> next [--content] | current [--content]       advance to / reprint the current task
  <name> done [-m <msg>] | skip [-m <msg>]            resolve the current task (does not advance)
  <name> choose <addr> | rewind [<addr>]              jump the cursor / reset a task to pending
  <name> peek [--all] [--content] | status | finished  look ahead / summarize / test exhaustion
  <name> rm                                           delete the iterator (never touches the covering)
  (bare `covmap iter` lists all iterators)

PLUMBING
  hash-segment <file> <a[:col]> <b[:col]> [-w]
  cat-segment <hash>
  ls-segments <covering>
  ls-links <connection>
  rev-parse <addr>
  fsck [--broken-links]
",
        env!("CARGO_PKG_VERSION")
    );
}

// ---- helpers --------------------------------------------------------------

fn current_repo() -> io::Result<Repo> {
    Repo::discover(&env::current_dir()?)
}

fn resolve_covering(repo: &Repo, name: &str) -> io::Result<Covering> {
    let n = if name == "HEAD" {
        repo.read_head()?
    } else {
        name.to_string()
    };
    Covering::read(repo, &n)
}

/// Address forms for `show` / `rev-parse`:
///   <hash>            -> segment via objects/
///   <C>:<h>           -> segment whose hash starts with <h> in covering <C>
fn parse_addr(s: &str) -> (Option<String>, String) {
    if let Some((c, h)) = s.split_once(':') {
        (Some(c.to_string()), h.to_string())
    } else {
        (None, s.to_string())
    }
}

/// Parse a `<line>` or `<line>:<col>` argument (both 1-indexed). A bare
/// line number yields `col = None` (the first character of the line).
fn parse_line_col(s: &str) -> io::Result<(usize, Option<usize>)> {
    let bad = || {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "<line> must be a positive integer, optionally <line>:<col>",
        )
    };
    match s.split_once(':') {
        Some((l, c)) => {
            let line: usize = l.parse().map_err(|_| bad())?;
            let col: usize = c.parse().map_err(|_| bad())?;
            if line == 0 || col == 0 {
                return Err(bad());
            }
            Ok((line, Some(col)))
        }
        None => {
            let line: usize = s.parse().map_err(|_| bad())?;
            if line == 0 {
                return Err(bad());
            }
            Ok((line, None))
        }
    }
}

fn short(h: &str) -> &str {
    let n = h.len().min(HASH_LEN);
    &h[..n]
}

// ---- init -----------------------------------------------------------------

fn cmd_init(args: &[String]) -> io::Result<u8> {
    if !args.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("init: unexpected arg '{}'", args[0]),
        ));
    }
    let cwd = env::current_dir()?;
    let _ = Repo::init(&cwd)?;
    println!("Initialized covmap in {}", cwd.join(".covmap").display());
    println!("  HEAD -> main (run `covmap cover` to populate)");
    Ok(0)
}

// ---- cover ----------------------------------------------------------------

/// `cover` is *additive and idempotent*: it walks the repo, and for
/// every readable UTF-8 file that the covering doesn't already cover,
/// it adds a single whole-file `unknown` segment. Existing segments
/// are never touched. Files already partitioned by the covering are
/// left alone (the user has presumably refined them with `split`).
fn cmd_cover(args: &[String]) -> io::Result<u8> {
    let mut name_override: Option<String> = None;
    for a in args {
        if a.starts_with('-') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("cover: unexpected flag '{a}'"),
            ));
        }
        if name_override.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cover accepts a single covering name",
            ));
        }
        name_override = Some(a.clone());
    }
    let repo = current_repo()?;
    let name = match name_override {
        Some(n) => n,
        None => repo.read_head()?,
    };
    let mut cov = match Covering::read(&repo, &name) {
        Ok(c) => c,
        Err(_) => Covering {
            name: name.clone(),
            entries: Vec::new(),
        },
    };
    let already = cov.covered_files();
    let mut added = 0usize;
    let mut skipped_binary = 0usize;
    for rel in walk::walk_files(&repo.root)? {
        let rel_str = walk::rel_string(&rel);
        if already.contains(&rel_str) {
            continue;
        }
        let abs = repo.root.join(&rel);
        let lines = match walk::read_text_lines(&abs)? {
            Some(l) => l,
            None => {
                skipped_binary += 1;
                eprintln!("(skipping non-UTF-8 file: {rel_str})");
                continue;
            }
        };
        if lines.is_empty() {
            continue;
        }
        let seg = walk::whole_file_segment(&rel_str, &lines);
        segment::write_object(&repo.objects_dir(), &seg)?;
        cov.entries.push(CoveringEntry {
            hash: seg.hash(),
            file: seg.file,
            a: seg.a,
            a_col: None,
            b: seg.b,
            b_col: None,
            label: seg.label,
            name: None,
            message: None,
        });
        added += 1;
    }
    cov.entries
        .sort_by(|a, b| a.file.cmp(&b.file).then(a.a.cmp(&b.a)));
    cov.write(&repo)?;
    let st = covering::stats(&cov);
    println!(
        "Covering '{}': {} segments, {} lines ({} added{})",
        cov.name,
        st.cardinality,
        st.total_lines,
        added,
        if skipped_binary > 0 {
            format!(", {skipped_binary} binary file(s) skipped")
        } else {
            String::new()
        }
    );
    Ok(0)
}

// ---- status ---------------------------------------------------------------

/// Render a connection's two coverings without exposing the repository's
/// absolute path for a same-repository target.
fn connection_route(repo: &Repo, k: &Connection) -> String {
    if k.target_repo == repo.root.to_string_lossy() {
        format!("{} -> {} (self-map)", k.source_covering, k.target_covering)
    } else {
        format!(
            "{} -> {}/{}",
            k.source_covering, k.target_repo, k.target_covering
        )
    }
}

/// "Drift" for a hand-curated covering means: a file the covering
/// claims to partition has been edited so that the recorded segments
/// no longer match its current bytes. We detect this without needing
/// a rule, by reconstructing each file from the covering and comparing
/// it to what's on disk.
fn cmd_status(args: &[String]) -> io::Result<u8> {
    let repo = current_repo()?;
    let names = if args.is_empty() {
        Covering::list(&repo)?
    } else {
        args.to_vec()
    };
    if names.is_empty() {
        println!("(no coverings yet — run `covmap cover`)");
        return Ok(0);
    }
    println!("repo: {}", repo.root.display());
    for n in &names {
        let c = match Covering::read(&repo, n) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("  {n}: {e}");
                continue;
            }
        };
        let st = covering::stats(&c);
        println!(
            "covering {}: segments={} lines={}",
            c.name, st.cardinality, st.total_lines
        );
        // Per-file drift: are the bytes on disk still consistent with
        // the recorded segments?
        let mut by_file: std::collections::BTreeMap<&str, Vec<&CoveringEntry>> =
            std::collections::BTreeMap::new();
        for e in &c.entries {
            by_file.entry(e.file.as_str()).or_default().push(e);
        }
        let mut drifted = 0usize;
        let mut missing = 0usize;
        for (file, mut entries) in by_file {
            entries.sort_by_key(|e| e.a);
            let abs = repo.root.join(file);
            let lines = match walk::read_text_lines(&abs) {
                Ok(Some(l)) => l,
                Ok(None) | Err(_) => {
                    missing += 1;
                    continue;
                }
            };
            if !file_matches_segments(&lines, &entries, &repo)? {
                drifted += 1;
            }
        }
        if drifted == 0 && missing == 0 {
            println!("  drift: clean");
        } else {
            println!("  drift: {drifted} file(s) edited, {missing} file(s) missing/binary");
        }
    }

    let conns = Connection::list(&repo)?;
    if !conns.is_empty() {
        println!("connections:");
        for cn in conns {
            let k = Connection::read(&repo, &cn)?;
            let src = Covering::read(&repo, &k.source_covering)?;
            let tgt_repo = Repo {
                root: PathBuf::from(&k.target_repo),
            };
            let tgt = Covering::read(&tgt_repo, &k.target_covering).ok();
            let mut broken = 0usize;
            for p in &k.links {
                let a_ok = covering::endpoint_resolves(&src, &p.a);
                let b_ok = tgt
                    .as_ref()
                    .is_some_and(|t| covering::endpoint_resolves(t, &p.b));
                if !a_ok || !b_ok {
                    broken += 1;
                }
            }
            // Coverage: distinct resolving endpoints over covering size, so
            // the reader sees how much of each side is mapped, not just a
            // link count (the theory-defined source/target coverage).
            let (src_cov, src_tot) = coverage(&src, k.links.iter().map(|p| p.a.as_str()));
            let cov_note = match &tgt {
                Some(t) => {
                    let (tc, tt) = coverage(t, k.links.iter().map(|p| p.b.as_str()));
                    format!("source {src_cov}/{src_tot}, target {tc}/{tt}")
                }
                None => format!("source {src_cov}/{src_tot}, target (covering unavailable)"),
            };
            println!(
                "  {}: {} : {} links, {} broken · coverage {}",
                cn,
                connection_route(&repo, &k),
                k.links.len(),
                broken,
                cov_note
            );
        }
    }
    Ok(0)
}

/// Check that the recorded segments for `file` reconstruct its current
/// content byte-for-byte (and that each segment's hash still matches
/// the object store).
fn file_matches_segments(
    lines: &[String],
    entries: &[&CoveringEntry],
    repo: &Repo,
) -> io::Result<bool> {
    let mut sorted: Vec<&&CoveringEntry> = entries.iter().collect();
    sorted.sort_by_key(|e| e.sort_key());
    // The next start position the partition must supply, beginning at
    // the first character of the file.
    let mut expected = covering::Pos { line: 1, col: None };
    for e in &sorted {
        if !covering::same_start(expected, e.start_pos()) {
            return Ok(false);
        }
        let on_disk = match segment::extract_content(lines, e.a, e.a_col, e.b, e.b_col) {
            Some(c) => c,
            None => return Ok(false),
        };
        let recorded = match segment::read_object(&repo.objects_dir(), &e.hash) {
            Ok(c) => c,
            Err(_) => return Ok(false),
        };
        if on_disk != recorded {
            return Ok(false);
        }
        expected = covering::successor_of_end(e.end_pos());
    }
    // The covering must reach exactly one past the end of the file.
    Ok(covering::same_start(
        expected,
        covering::file_end(lines.len()),
    ))
}

fn object_exists(repo: &Repo, hash: &str) -> bool {
    segment::object_path(&repo.objects_dir(), hash).exists()
}

// ---- show -----------------------------------------------------------------

fn cmd_show(args: &[String]) -> io::Result<u8> {
    // Split off flags (--label <l>, --content) from positionals.
    let mut label_lens: Option<String> = None;
    let mut with_content = false;
    let mut with_all = false;
    let mut pos: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--label" => {
                label_lens = Some(args.get(i + 1).cloned().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "--label needs a value")
                })?);
                i += 2;
            }
            "--content" => {
                with_content = true;
                i += 1;
            }
            "--all" => {
                with_all = true;
                i += 1;
            }
            other => {
                pos.push(other.to_string());
                i += 1;
            }
        }
    }
    let repo = current_repo()?;

    // --label <l> [<C>] — the cross-cutting "understand" call.
    if let Some(l) = label_lens {
        let cname = pos.first().cloned().unwrap_or_else(|| "HEAD".to_string());
        let c = resolve_covering(&repo, &cname)?;
        return show_label(&repo, &c, &l);
    }

    let addr = pos
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "show <addr>"))?;

    // Connection selectors: `<K>` (list links) or `<K> <a> <b>` (one link).
    if repo.connections_dir().join(addr).is_file() {
        let k = Connection::read(&repo, addr)?;
        if pos.len() >= 3 {
            return show_link(&repo, &k, &pos[1], &pos[2]);
        }
        return show_connection(&repo, &k, with_all);
    }

    let (covering, prefix) = parse_addr(addr);

    // Covering selector: `<C>` — either a region within it, or the whole
    // covering listing.
    if let Some(cname) = &covering {
        if repo.coverings_dir().join(cname).is_file() {
            let c = Covering::read(&repo, cname)?;
            if let Some((file, range)) = parse_region(&c, &prefix) {
                return show_region(&repo, &c, &file, range);
            }
        }
    } else if repo.coverings_dir().join(addr).is_file() {
        let c = Covering::read(&repo, addr)?;
        return show_covering(&c);
    }

    // Otherwise a single segment — DETAIL.
    show_segment(&repo, covering.as_deref(), &prefix, with_content)
}

/// Parse the part after `<C>:` as a region — a covered file, optionally
/// with a `:a..b` line range — or `None` if it is a segment address.
fn parse_region(c: &Covering, rest: &str) -> Option<(String, Option<(usize, usize)>)> {
    if let Some((file, rng)) = rest.rsplit_once(':') {
        if let Some((a, b)) = rng.split_once("..") {
            if let (Ok(a), Ok(b)) = (a.parse::<usize>(), b.parse::<usize>()) {
                if c.entries.iter().any(|e| e.file == file) {
                    return Some((file.to_string(), Some((a, b))));
                }
            }
        }
    }
    if c.entries.iter().any(|e| e.file == rest) {
        return Some((rest.to_string(), None));
    }
    None
}

/// Whether a segment (line range `[a,b]`) overlaps a region `[ra,rb]`.
fn overlaps(a: usize, b: usize, ra: usize, rb: usize) -> bool {
    a <= rb && ra <= b
}

// ---- show sub-renderers ---------------------------------------------------

fn show_covering(c: &Covering) -> io::Result<u8> {
    let st = covering::stats(c);
    println!(
        "covering {}: {} segments, {} lines",
        c.name, st.cardinality, st.total_lines
    );
    for e in &c.entries {
        let name = e
            .name
            .as_deref()
            .map(|n| format!("\t@{n}"))
            .unwrap_or_default();
        println!(
            "  {} {}\t{}{}",
            short(&e.hash),
            e.loc(),
            if e.label.is_empty() { "-" } else { &e.label },
            name
        );
    }
    Ok(0)
}

fn show_segment(
    repo: &Repo,
    covering: Option<&str>,
    prefix: &str,
    with_content: bool,
) -> io::Result<u8> {
    // Label, name and message are covering-local, so they are available
    // only when addressed through a covering; a bare hash names content
    // across every covering that shares it.
    let (full, cname, entry) = match covering {
        Some(cname) => {
            let c = resolve_covering(repo, cname)?;
            let idx = covering::resolve_segment(&c, prefix)?;
            (
                c.entries[idx].hash.clone(),
                Some(cname.to_string()),
                Some(c.entries[idx].clone()),
            )
        }
        None => (
            segment::resolve_prefix(&repo.objects_dir(), prefix)?,
            None,
            None,
        ),
    };
    let content = segment::read_object(&repo.objects_dir(), &full)?;
    println!("segment {full}");
    if let Some(cn) = &cname {
        println!("  covering  {cn}");
    }
    if let Some(e) = &entry {
        println!("  loc       {}   ({} lines)", e.loc(), e.b - e.a + 1);
        println!(
            "  label     {}",
            if e.label.is_empty() { "-" } else { &e.label }
        );
        println!("  name      {}", e.name.as_deref().unwrap_or("-"));
        println!(
            "  message   {}",
            match &e.message {
                Some(_) => "(below)",
                None => "-",
            }
        );
    }
    println!("  bytes     {}", content.len());

    // Drift state (only meaningful when we know the covering + location).
    if let (Some(cn), Some(e)) = (&cname, &entry) {
        let state = segment_drift_state(repo, cn, e, &content);
        println!("  state     {state}");
        // Resolved links, both directions.
        let c = resolve_covering(repo, cn)?;
        let rel = crate::resolve::Relation::build(repo, cn, &c)?;
        print_seg_links(&rel.links_of(&full));
    }

    if let Some(e) = &entry {
        if let Some(m) = &e.message {
            println!("  message:");
            for line in m.split('\n') {
                println!("    {line}");
            }
        }
    }

    if with_content || entry.is_none() {
        // A bare-hash address has no covering context, so content is all we
        // can show; through a covering, content is opt-in (`--content`).
        println!("--- content ---");
        print!("{content}");
        if !content.ends_with('\n') {
            println!();
        }
    }
    Ok(0)
}

/// Compare a segment's recorded content against the current file on disk.
fn segment_drift_state(repo: &Repo, cname: &str, e: &CoveringEntry, recorded: &str) -> String {
    let abs = repo.root.join(&e.file);
    let lines = match walk::read_text_lines(&abs) {
        Ok(Some(l)) => l,
        _ => return "unreadable (file missing/binary)".to_string(),
    };
    let addr = e
        .name
        .as_deref()
        .map(|n| format!("@{n}"))
        .unwrap_or_else(|| short(&e.hash).to_string());
    match segment::extract_content(&lines, e.a, e.a_col, e.b, e.b_col) {
        Some(on_disk) if on_disk == recorded => {
            "current            (recorded bytes match disk)".to_string()
        }
        Some(_) => {
            format!("drifted            (recorded ≠ disk; run: covmap recut {cname}:{addr})")
        }
        None => {
            "drifted            (boundary no longer fits; run: covmap recut --remap)".to_string()
        }
    }
}

fn print_seg_links(links: &crate::resolve::SegLinks) {
    let out = links.outgoing.len();
    let inc = links.incoming.len();
    if out == 0 && inc == 0 {
        println!("  links     none");
        return;
    }
    println!("  links     {out} outgoing · {inc} incoming");
    for l in &links.outgoing {
        print_resolved_end("->", l);
    }
    for l in &links.incoming {
        print_resolved_end("<-", l);
    }
}

fn print_resolved_end(arrow: &str, l: &crate::resolve::ResolvedEnd) {
    let far = far_label(l);
    println!(
        "    {arrow} [{}] {}:{}   {}",
        l.conn,
        l.covering,
        far,
        resolved_loc(l)
    );
    if !l.note.is_empty() {
        println!("                {}", l.note);
    }
}

/// The far endpoint's display handle: `@name` if named, else `label`, else
/// a short hash; suffixed `(broken)` when it no longer resolves.
fn far_label(l: &crate::resolve::ResolvedEnd) -> String {
    let base = l
        .name
        .as_ref()
        .map(|n| format!("@{n}"))
        .or_else(|| l.label.clone())
        .unwrap_or_else(|| short(&l.hash).to_string());
    if l.resolved {
        base
    } else {
        format!("{base} (broken)")
    }
}

fn resolved_loc(l: &crate::resolve::ResolvedEnd) -> String {
    l.loc.clone().unwrap_or_else(|| "(unresolved)".to_string())
}

/// Above this many links, `show <connection>` summarizes (coverage +
/// fan-in hubs) instead of dumping every link; `--all` forces the full list.
const CONN_LIST_THRESHOLD: usize = 30;

fn show_connection(repo: &Repo, k: &Connection, all: bool) -> io::Result<u8> {
    let src = Covering::read(repo, &k.source_covering)?;
    let tgt_repo = Repo {
        root: PathBuf::from(&k.target_repo),
    };
    let tgt = Covering::read(&tgt_repo, &k.target_covering).ok();

    // Coverage: distinct resolving endpoints over covering cardinality.
    let (src_cov, src_tot) = coverage(&src, k.links.iter().map(|p| p.a.as_str()));
    let (tgt_cov, tgt_tot) = match &tgt {
        Some(t) => coverage(t, k.links.iter().map(|p| p.b.as_str())),
        None => (0, 0),
    };

    println!("connection {}   {}", k.name, connection_route(repo, k));
    println!(
        "  {} links · source {}/{} segments · target {}/{} segments",
        k.links.len(),
        src_cov,
        src_tot,
        tgt_cov,
        tgt_tot
    );

    // Large connections summarize by default: show the top fan-in targets
    // (the highest-signal rows for triage — a catch-all hub stands out),
    // and require --all for the full per-link dump.
    if k.links.len() > CONN_LIST_THRESHOLD && !all {
        let mut degree: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for p in &k.links {
            *degree.entry(p.b.as_str()).or_insert(0) += 1;
        }
        let mut ranked: Vec<(&str, usize)> = degree.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        println!("  top targets by in-degree:");
        for (ep, n) in ranked.iter().take(10) {
            let disp = tgt
                .as_ref()
                .map(|t| resolve_display(t, ep))
                .unwrap_or_else(|| ep.to_string());
            println!("    {n:>4}  <- {disp}");
        }
        println!(
            "  ({} links total — `covmap show {} --all` to list each)",
            k.links.len(),
            k.name
        );
        return Ok(0);
    }

    for p in &k.links {
        let a = resolve_display(&src, &p.a);
        let b = tgt
            .as_ref()
            .map(|t| resolve_display(t, &p.b))
            .unwrap_or_else(|| p.b.clone());
        if p.note.is_empty() {
            println!("  {a}   ->   {b}");
        } else {
            println!("  {a}   ->   {b}   {}", p.note);
        }
    }
    Ok(0)
}

/// `label loc` for an endpoint, or the raw endpoint + `(broken)`.
fn resolve_display(c: &Covering, ep: &str) -> String {
    match covering::endpoint_to_hash(c, ep) {
        Some(h) => c
            .entries
            .iter()
            .find(|e| e.hash == h)
            .map(|e| {
                let lbl = if e.label.is_empty() { "-" } else { &e.label };
                format!("{lbl} {}", e.loc())
            })
            .unwrap_or_else(|| ep.to_string()),
        None => format!("{ep} (broken)"),
    }
}

/// (distinct resolving endpoints, covering cardinality).
fn coverage<'a>(c: &Covering, eps: impl Iterator<Item = &'a str>) -> (usize, usize) {
    let mut seen = std::collections::BTreeSet::new();
    for ep in eps {
        if let Some(h) = covering::endpoint_to_hash(c, ep) {
            seen.insert(h);
        }
    }
    (seen.len(), c.entries.len())
}

fn show_link(repo: &Repo, k: &Connection, a: &str, b: &str) -> io::Result<u8> {
    let link = k
        .links
        .iter()
        .find(|p| endpoints_match(&p.a, a) && endpoints_match(&p.b, b))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("no link {a} -> {b} in connection '{}'", k.name),
            )
        })?;
    let src = Covering::read(repo, &k.source_covering)?;
    let tgt_repo = Repo {
        root: PathBuf::from(&k.target_repo),
    };
    let tgt = Covering::read(&tgt_repo, &k.target_covering).ok();
    let a_ok = covering::endpoint_resolves(&src, &link.a);
    let b_ok = tgt
        .as_ref()
        .is_some_and(|t| covering::endpoint_resolves(t, &link.b));

    println!("link in {}", k.name);
    println!(
        "  note    {}",
        if link.note.is_empty() {
            "-"
        } else {
            &link.note
        }
    );
    let status = match (a_ok, b_ok) {
        (true, true) => "resolved   (both endpoints present in their coverings)".to_string(),
        (false, true) => format!(
            "broken (source endpoint '{}' no longer in '{}')",
            link.a, k.source_covering
        ),
        (true, false) => format!(
            "broken (target endpoint '{}' no longer in '{}')",
            link.b, k.target_covering
        ),
        (false, false) => "broken (neither endpoint resolves)".to_string(),
    };
    println!("  status  {status}");
    println!(
        "  source  {} : {}",
        k.source_covering,
        resolve_display(&src, &link.a)
    );
    match &tgt {
        Some(t) => println!(
            "  target  {} : {}",
            k.target_covering,
            resolve_display(t, &link.b)
        ),
        None => println!("  target  {} : (covering unavailable)", k.target_covering),
    }
    Ok(0)
}

fn endpoints_match(stored: &str, given: &str) -> bool {
    // A stored endpoint matches if it equals the given string, or (for a
    // hash endpoint) the given string is a prefix of it.
    stored == given
        || (!stored.starts_with('@') && !given.starts_with('@') && stored.starts_with(given))
        || format!("@{given}") == *stored
}

fn show_region(
    repo: &Repo,
    c: &Covering,
    file: &str,
    range: Option<(usize, usize)>,
) -> io::Result<u8> {
    let rel = crate::resolve::Relation::build(repo, &c.name, c)?;
    let mut segs: Vec<&CoveringEntry> = c
        .entries
        .iter()
        .filter(|e| {
            e.file == file
                && range
                    .map(|(ra, rb)| overlaps(e.a, e.b, ra, rb))
                    .unwrap_or(true)
        })
        .collect();
    segs.sort_by_key(|e| e.sort_key());
    let where_ = match range {
        Some((a, b)) => format!("{file}:{a}..{b}"),
        None => file.to_string(),
    };
    println!("region {} {} · {} segment(s)", c.name, where_, segs.len());
    print_region_segs(&rel, &segs);
    Ok(0)
}

fn show_label(repo: &Repo, c: &Covering, label: &str) -> io::Result<u8> {
    let rel = crate::resolve::Relation::build(repo, &c.name, c)?;
    let mut segs: Vec<&CoveringEntry> = c.entries.iter().filter(|e| e.label == label).collect();
    segs.sort_by_key(|e| e.sort_key());
    println!(
        "label \"{}\" in {} · {} segment(s)",
        label,
        c.name,
        segs.len()
    );
    print_region_segs(&rel, &segs);
    Ok(0)
}

/// Shared relationship-view body: each segment then its resolved
/// counterparts (the compare view, in text).
fn print_region_segs(rel: &crate::resolve::Relation, segs: &[&CoveringEntry]) {
    for e in segs {
        let handle = e
            .name
            .as_deref()
            .map(|n| format!("@{n}"))
            .unwrap_or_else(|| short(&e.hash).to_string());
        let lbl = if e.label.is_empty() { "-" } else { &e.label };
        println!("  {handle}  {lbl}  {}", e.loc());
        let links = rel.links_of(&e.hash);
        if links.outgoing.is_empty() && links.incoming.is_empty() {
            println!("    (unlinked)");
        }
        for l in &links.outgoing {
            println!("    -> {}   {}", far_label(l), resolved_loc(l));
            if !l.note.is_empty() {
                println!("       {}", l.note);
            }
        }
        for l in &links.incoming {
            println!("    <- {}   {}", far_label(l), resolved_loc(l));
            if !l.note.is_empty() {
                println!("       {}", l.note);
            }
        }
    }
}

// ---- ls -------------------------------------------------------------------

fn cmd_ls(args: &[String]) -> io::Result<u8> {
    let mut long = false;
    let mut label_filter: Option<String> = None;
    let mut link_filter: Option<bool> = None; // Some(true)=linked, Some(false)=unlinked
    let mut positionals: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-l" => {
                long = true;
                i += 1;
            }
            "--label" => {
                label_filter = Some(args.get(i + 1).cloned().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "--label needs a value")
                })?);
                i += 2;
            }
            "--linked" => {
                link_filter = Some(true);
                i += 1;
            }
            "--unlinked" => {
                link_filter = Some(false);
                i += 1;
            }
            other => {
                positionals.push(other.to_string());
                i += 1;
            }
        }
    }
    // Positionals: an optional covering name and/or a region (a covered
    // file, or `<file>:a..b`). `ls [<C>] [<file>[:a..b]]`.
    if positionals.len() > 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "ls [-l] [<covering>] [<file>[:a..b]] [--label <l>] [--linked|--unlinked]",
        ));
    }
    let repo = current_repo()?;
    // Decide which positional is the covering and which is the region.
    let (cov_name, region) = split_ls_positionals(&repo, &positionals)?;
    let c = resolve_covering(&repo, &cov_name)?;

    // Region filter (file + optional line range).
    let region_parsed: Option<(String, Option<(usize, usize)>)> = match &region {
        Some(r) => match r.split_once(':') {
            Some((f, rng)) => match rng.split_once("..") {
                Some((a, b)) => {
                    let a: usize = a.parse().map_err(|_| {
                        io::Error::new(io::ErrorKind::InvalidInput, "region range must be a..b")
                    })?;
                    let b: usize = b.parse().map_err(|_| {
                        io::Error::new(io::ErrorKind::InvalidInput, "region range must be a..b")
                    })?;
                    Some((f.to_string(), Some((a, b))))
                }
                None => Some((r.clone(), None)),
            },
            None => Some((r.clone(), None)),
        },
        None => None,
    };

    // Build the link relation only when a link filter is requested.
    let rel = match link_filter {
        Some(_) => Some(crate::resolve::Relation::build(&repo, &cov_name, &c)?),
        None => None,
    };

    for e in &c.entries {
        if let Some(l) = &label_filter {
            if &e.label != l {
                continue;
            }
        }
        if let Some((f, rng)) = &region_parsed {
            if &e.file != f {
                continue;
            }
            if let Some((ra, rb)) = rng {
                if !overlaps(e.a, e.b, *ra, *rb) {
                    continue;
                }
            }
        }
        if let (Some(want_linked), Some(rel)) = (link_filter, &rel) {
            if rel.is_linked(&e.hash) != want_linked {
                continue;
            }
        }
        if long {
            // Columns: hash, label, loc, lines, name, [message]. The name
            // keeps its historical position; the message's FIRST line is a
            // further trailing column (present only when set), so the
            // listing stays one row per segment — `show` prints the whole
            // (possibly multi-line) message.
            let name = e
                .name
                .as_deref()
                .map(|n| format!("@{n}"))
                .unwrap_or_else(|| "-".to_string());
            let msg_col = match &e.message {
                Some(m) => format!("\t{}", m.split('\n').next().unwrap_or("")),
                None => String::new(),
            };
            println!(
                "{}\t{}\t{}\t{}\t{}{}",
                e.hash,
                if e.label.is_empty() { "-" } else { &e.label },
                e.loc(),
                e.b - e.a + 1,
                name,
                msg_col
            );
        } else {
            println!("{}\t{}", e.hash, e.loc());
        }
    }
    Ok(0)
}

/// Split `ls` positionals into (covering, region). A positional that names
/// an existing covering is the covering; anything else is a region (a
/// covered file, optionally `file:a..b`). Defaults the covering to HEAD.
fn split_ls_positionals(
    repo: &Repo,
    positionals: &[String],
) -> io::Result<(String, Option<String>)> {
    let is_cov = |s: &str| repo.coverings_dir().join(s).is_file() || s == "HEAD";
    match positionals {
        [] => Ok(("HEAD".to_string(), None)),
        [one] => {
            if is_cov(one) {
                Ok((one.clone(), None))
            } else {
                Ok(("HEAD".to_string(), Some(one.clone())))
            }
        }
        [a, b] => {
            // Two positionals: <covering> <region>.
            Ok((a.clone(), Some(b.clone())))
        }
        _ => unreachable!("caller guards length"),
    }
}

// ---- grep -----------------------------------------------------------------

fn cmd_grep(args: &[String]) -> io::Result<u8> {
    let mut hashes_only = false;
    let mut covering_name = String::from("HEAD");
    let mut pattern: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-l" => {
                hashes_only = true;
                i += 1;
            }
            "-c" | "--covering" => {
                covering_name = args
                    .get(i + 1)
                    .cloned()
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "-c needs arg"))?;
                i += 2;
            }
            other => {
                if pattern.is_some() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "grep accepts a single pattern",
                    ));
                }
                pattern = Some(other.to_string());
                i += 1;
            }
        }
    }
    let pattern =
        pattern.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "grep <pattern>"))?;
    let repo = current_repo()?;
    let c = resolve_covering(&repo, &covering_name)?;
    for e in &c.entries {
        let content = segment::read_object(&repo.objects_dir(), &e.hash)?;
        if !(e.label.contains(&pattern) || content.contains(&pattern)) {
            continue;
        }
        if hashes_only {
            println!("{}", e.hash);
        } else {
            println!(
                "{}\t{}\t{}",
                e.hash,
                if e.label.is_empty() { "-" } else { &e.label },
                e.loc()
            );
        }
    }
    Ok(0)
}

// ---- diff -----------------------------------------------------------------

fn cmd_diff(args: &[String]) -> io::Result<u8> {
    if args.len() < 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "diff <covering1> <covering2>",
        ));
    }
    let repo = current_repo()?;
    let c1 = resolve_covering(&repo, &args[0])?;
    let c2 = resolve_covering(&repo, &args[1])?;
    let h1 = covering::hashes(&c1);
    let h2 = covering::hashes(&c2);
    for h in h1.difference(&h2) {
        if let Some(e) = covering::entry_by_hash(&c1, h) {
            println!("- {}\t{}\t{}", h, e.loc(), e.label);
        }
    }
    for h in h2.difference(&h1) {
        if let Some(e) = covering::entry_by_hash(&c2, h) {
            println!("+ {}\t{}\t{}", h, e.loc(), e.label);
        }
    }
    for h in h1.intersection(&h2) {
        if let Some(e) = covering::entry_by_hash(&c1, h) {
            println!("= {}\t{}\t{}", h, e.loc(), e.label);
        }
    }
    Ok(0)
}

// ---- label ----------------------------------------------------------------

fn cmd_label(args: &[String]) -> io::Result<u8> {
    if args.len() < 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "label <covering>:<hash> <label>",
        ));
    }
    let (covname, prefix) = parse_addr(&args[0]);
    let covname = covname.unwrap_or_else(|| "HEAD".to_string());
    let new_label = args[1].clone();
    let repo = current_repo()?;
    let mut c = resolve_covering(&repo, &covname)?;
    let old = covering::resolve_segment_hash(&c, &prefix)?;
    let new_hash = covering::relabel(&repo, &mut c, &old, &new_label)?;
    c.write(&repo)?;
    println!(
        "{} -> {}  (label '{}' in covering '{}')",
        old, new_hash, new_label, c.name
    );
    Ok(0)
}

// ---- name -----------------------------------------------------------------

/// `name <C>:<h|@name> <name|->` — set, rename, or clear a segment's
/// name. The address may itself be a hash prefix or an existing
/// `@name`. A `<name>` of `-` clears it (segment becomes anonymous).
fn cmd_name(args: &[String]) -> io::Result<u8> {
    if args.len() < 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "name <covering>:<hash|@name> <name|->",
        ));
    }
    let (covname, addr) = parse_addr(&args[0]);
    let covname = covname.unwrap_or_else(|| "HEAD".to_string());
    let tag = args[1].as_str();
    let repo = current_repo()?;
    let mut c = resolve_covering(&repo, &covname)?;
    let idx = covering::resolve_segment(&c, &addr)?;
    let old = c.entries[idx].name.clone();
    let new_name = if tag == "-" { None } else { Some(tag) };
    covering::set_name(&mut c, idx, new_name)?;
    c.write(&repo)?;
    let h = covering::short_hash(&c.entries[idx].hash);
    match (old, new_name) {
        (None, Some(n)) => println!("named {h} -> @{n}"),
        (Some(o), Some(n)) if o != n => {
            println!("renamed @{o} -> @{n}  (re-point connections that used '@{o}')")
        }
        (Some(o), Some(_)) => println!("@{o} (unchanged)"),
        (Some(o), None) => {
            println!("cleared @{o} from {h}  (connections to '@{o}' are now broken)")
        }
        (None, None) => println!("{h} (already anonymous)"),
    }
    Ok(0)
}

// ---- annotate -------------------------------------------------------------

/// `annotate <C>:<addr> [-m <msg>]... | -` — set, replace, clear, or print a
/// segment's free-form message. With no `-m` (and no `-`), prints the current
/// message. Repeated `-m` join as blank-line-separated paragraphs (git
/// convention). A lone `-` clears the message. `<addr>` is any segment
/// address (`@name`, hash prefix, or `file:line[:col]`).
fn cmd_annotate(args: &[String]) -> io::Result<u8> {
    let addr_arg = args.first().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "annotate <covering>:<addr> [-m <msg>]... | -",
        )
    })?;
    // Parse the rest: -m/--message (repeatable) sets; a lone `-` clears;
    // nothing means read.
    let mut paragraphs: Vec<String> = Vec::new();
    let mut clear = false;
    let mut set = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-m" | "--message" => {
                let v = args.get(i + 1).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "-m needs a message")
                })?;
                paragraphs.push(v.clone());
                set = true;
                i += 2;
            }
            "-" => {
                clear = true;
                i += 1;
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("annotate: unexpected argument '{other}' (use -m <msg> or -)"),
                ));
            }
        }
    }
    if set && clear {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "annotate: give either -m <msg> or - (clear), not both",
        ));
    }

    let (covname, addr) = parse_addr(addr_arg);
    let covname = covname.unwrap_or_else(|| "HEAD".to_string());
    let repo = current_repo()?;
    let mut c = resolve_covering(&repo, &covname)?;
    let idx = covering::resolve_segment(&c, &addr)?;
    let h = covering::short_hash(&c.entries[idx].hash).to_string();

    // Read mode: no -m and no `-`.
    if !set && !clear {
        match &c.entries[idx].message {
            Some(m) => {
                println!("{m}");
                return Ok(0);
            }
            None => {
                eprintln!("{h} (no message)");
                return Ok(1);
            }
        }
    }

    let new_message = if clear {
        None
    } else {
        Some(paragraphs.join("\n\n"))
    };
    covering::set_message(&mut c, idx, new_message.as_deref());
    c.write(&repo)?;
    match &c.entries[idx].message {
        Some(_) => println!("annotated {h}"),
        None => println!("cleared message from {h}"),
    }
    Ok(0)
}

// ---- split ----------------------------------------------------------------

fn cmd_split(args: &[String]) -> io::Result<u8> {
    if args.len() < 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "split <covering>:<hash> <line> [<label_top>] [<label_bot>]",
        ));
    }
    let (covname, prefix) = parse_addr(&args[0]);
    let covname = covname.unwrap_or_else(|| "HEAD".to_string());
    let (cut_line, cut_col) = parse_line_col(&args[1])?;
    let label_top = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| DEFAULT_LABEL.to_string());
    let label_bot = args
        .get(3)
        .cloned()
        .unwrap_or_else(|| DEFAULT_LABEL.to_string());
    let repo = current_repo()?;
    let mut c = resolve_covering(&repo, &covname)?;
    let old = covering::resolve_segment_hash(&c, &prefix)?;
    let (top, bot) = covering::split(
        &repo, &mut c, &old, cut_line, cut_col, &label_top, &label_bot,
    )?;
    c.write(&repo)?;
    println!("split {} -> {} + {}", old, top, bot);
    Ok(0)
}

// ---- recut ----------------------------------------------------------------

fn cmd_recut(args: &[String]) -> io::Result<u8> {
    let mut remap = false;
    let mut addr: Option<String> = None;
    let mut threshold = 0.4f64;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--remap" => remap = true,
            "--threshold" => {
                let v = args.get(i + 1).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "--threshold needs a value")
                })?;
                threshold = v.parse().map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "--threshold must be a number in [0,1]",
                    )
                })?;
                i += 1;
            }
            other if other.starts_with('-') => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("recut: unexpected flag '{other}'"),
                ));
            }
            other => {
                if addr.is_some() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "recut accepts a single address",
                    ));
                }
                addr = Some(other.to_string());
            }
        }
        i += 1;
    }
    if remap {
        return cmd_recut_remap(addr.as_deref(), threshold);
    }
    let addr = addr.ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "recut <covering>:<hash|@name>")
    })?;
    let (covname, prefix) = parse_addr(&addr);
    let covname = covname.unwrap_or_else(|| "HEAD".to_string());
    let repo = current_repo()?;
    let mut c = resolve_covering(&repo, &covname)?;
    // Accept either a hash prefix or an `@name` segment address.
    let idx = covering::resolve_segment(&c, &prefix)?;
    let old = c.entries[idx].hash.clone();
    let new_hash = covering::recut(&repo, &mut c, &old)?;
    c.write(&repo)?;
    if old == new_hash {
        println!("{} (unchanged)", old);
    } else {
        println!("{} -> {}", old, new_hash);
    }
    Ok(0)
}

/// `recut --remap [<C>[:<addr>]]` — diff-remap. Reconstruct each covered
/// file from the covering, diff against the bytes on disk, and re-fit
/// every segment's boundaries through the unchanged regions, refreshing
/// content and preserving names so connections carry forward. Per-file
/// all-or-nothing: a file whose boundaries fall in changed regions is
/// left untouched and its segments reported as unresolved. With an address
/// `<C>:<addr>` only that segment's file is remapped; otherwise every
/// drifted file in the covering is.
fn cmd_recut_remap(addr: Option<&str>, threshold: f64) -> io::Result<u8> {
    let repo = current_repo()?;
    let (covname, only_file) = match addr {
        Some(a) => {
            let (cn, seg) = parse_addr(a);
            let covname = cn.unwrap_or_else(|| "HEAD".to_string());
            let c = resolve_covering(&repo, &covname)?;
            let idx = covering::resolve_segment(&c, &seg)?;
            (covname, Some(c.entries[idx].file.clone()))
        }
        None => ("HEAD".to_string(), None),
    };
    let mut c = resolve_covering(&repo, &covname)?;

    // Group entry indices by file.
    let mut files: std::collections::BTreeMap<String, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (i, e) in c.entries.iter().enumerate() {
        files.entry(e.file.clone()).or_default().push(i);
    }

    let mut healed_files = 0usize;
    let mut total_refreshed = 0usize;
    let mut unresolved_count = 0usize;
    // Accumulate the rebuilt entry list; replace whole files atomically.
    let mut new_entries: Vec<CoveringEntry> = Vec::with_capacity(c.entries.len());
    // Process files in covering order so the rebuilt list stays ordered.
    let mut handled: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for e in &c.entries {
        if handled.contains(&e.file) {
            continue;
        }
        handled.insert(e.file.clone());
        let file = &e.file;
        let idxs = &files[file];
        let old_entries: Vec<CoveringEntry> = idxs.iter().map(|&i| c.entries[i].clone()).collect();

        if let Some(target) = &only_file {
            if file != target {
                new_entries.extend(old_entries);
                continue;
            }
        }

        // Reconstruct O from the covering; read N from disk.
        let mut ordered = old_entries.clone();
        ordered.sort_by_key(|e| e.sort_key());
        let mut o_lines: Vec<String> = Vec::new();
        let mut reconstruct_ok = true;
        for oe in &ordered {
            match segment::read_object(&repo.objects_dir(), &oe.hash) {
                Ok(content) => {
                    for piece in content.split_inclusive('\n') {
                        o_lines.push(piece.to_string());
                    }
                }
                Err(_) => {
                    reconstruct_ok = false;
                    break;
                }
            }
        }
        let abs = repo.root.join(file);
        let n_lines = match walk::read_text_lines(&abs) {
            Ok(Some(l)) => l,
            _ => {
                eprintln!("{file}: cannot read as UTF-8 — skipped");
                new_entries.extend(old_entries);
                continue;
            }
        };
        if !reconstruct_ok {
            eprintln!("{file}: object store incomplete — skipped");
            new_entries.extend(old_entries);
            continue;
        }

        match covering::remap_file(&old_entries, &o_lines, &n_lines) {
            Ok(fr) => {
                for (entry, content) in &fr.entries {
                    let seg = Segment {
                        file: entry.file.clone(),
                        a: entry.a,
                        b: entry.b,
                        label: entry.label.clone(),
                        content: content.clone(),
                    };
                    segment::write_object(&repo.objects_dir(), &seg)?;
                }
                total_refreshed += fr.refreshed;
                if fr.refreshed > 0 {
                    healed_files += 1;
                    println!(
                        "{file}: remapped {} segment(s) ({} refreshed, {} unchanged)",
                        fr.entries.len(),
                        fr.refreshed,
                        fr.unchanged
                    );
                    // Name the segments whose line range actually moved
                    // (old..b -> new..b), so a refresh is easy to confirm
                    // without re-reading `ls -l`. Pair old vs new by position:
                    // remap_file emits entries sorted by start, so sort the
                    // old ones the same way.
                    let mut old_sorted = old_entries.clone();
                    old_sorted.sort_by_key(|e| e.sort_key());
                    for (old, (new, _)) in old_sorted.iter().zip(fr.entries.iter()) {
                        if (old.a, old.b) != (new.a, new.b) {
                            let who = new
                                .name
                                .as_deref()
                                .map(|n| format!("@{n}"))
                                .unwrap_or_else(|| covering::short_hash(&new.hash).to_string());
                            println!("    {who}: {}..{} -> {}..{}", old.a, old.b, new.a, new.b);
                        }
                    }
                }
                new_entries.extend(fr.entries.into_iter().map(|(e, _)| e));
            }
            Err(covering::RemapError::NotDrifted) => {
                new_entries.extend(old_entries);
            }
            Err(covering::RemapError::CharLevel(locs)) => {
                eprintln!(
                    "{file}: has character-level segment(s) ({}) — diff-remap handles \
                     line-aligned segments only; left untouched",
                    locs.join(", ")
                );
                unresolved_count += locs.len();
                new_entries.extend(old_entries);
            }
            Err(covering::RemapError::Unresolved(items)) => {
                eprintln!(
                    "{file}: {} segment boundary(ies) fall in changed regions — left \
                     untouched. Fingerprint proposals (confirm manually; nothing \
                     was changed):",
                    items.len()
                );
                unresolved_count += items.len();
                for it in &items {
                    let who = it
                        .name
                        .as_deref()
                        .map(|n| format!("@{n}"))
                        .unwrap_or_else(|| {
                            format!("{} ({})", covering::short_hash(&it.hash), it.loc())
                        });
                    // Read the segment's recorded content and rank candidate
                    // locations in the new file. Propose-only.
                    let target = match segment::read_object(&repo.objects_dir(), &it.hash) {
                        Ok(c) => c,
                        Err(_) => {
                            eprintln!("  {who}: (content unavailable)");
                            continue;
                        }
                    };
                    let cands = crate::fingerprint::locate(&target, &n_lines, 3);
                    let shown: Vec<_> = cands.iter().filter(|c| c.score >= threshold).collect();
                    if shown.is_empty() {
                        eprintln!("  {who}: no candidate ≥ {:.2}", threshold);
                    } else {
                        for c in shown {
                            eprintln!(
                                "  {who}: {}:{}..{}  ({:.0}% match)  — covmap unlink/link to relocate",
                                file,
                                c.a,
                                c.b,
                                c.score * 100.0
                            );
                        }
                    }
                }
                new_entries.extend(old_entries);
            }
        }
    }

    c.entries = new_entries;
    c.write(&repo)?;
    println!(
        "remap: {healed_files} file(s) healed, {total_refreshed} segment(s) refreshed, \
         {unresolved_count} unresolved"
    );
    Ok(if unresolved_count > 0 { 1 } else { 0 })
}

// ---- merge ----------------------------------------------------------------

fn cmd_merge(args: &[String]) -> io::Result<u8> {
    if args.len() < 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "merge <covering>:<h1> <covering>:<h2> [<label>]",
        ));
    }
    let (cn1, p1) = parse_addr(&args[0]);
    let (cn2, p2) = parse_addr(&args[1]);
    let cn1 = cn1.unwrap_or_else(|| "HEAD".to_string());
    let cn2 = cn2.unwrap_or_else(|| "HEAD".to_string());
    if cn1 != cn2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "merge: both segments must be in the same covering",
        ));
    }
    let label = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| DEFAULT_LABEL.to_string());
    let repo = current_repo()?;
    let mut c = resolve_covering(&repo, &cn1)?;
    let h1 = covering::resolve_segment_hash(&c, &p1)?;
    let h2 = covering::resolve_segment_hash(&c, &p2)?;
    let h = covering::merge(&repo, &mut c, &h1, &h2, &label)?;
    c.write(&repo)?;
    println!("merged {} + {} -> {}", h1, h2, h);
    Ok(0)
}

// ---- link / unlink --------------------------------------------------------

/// A link endpoint is resolved against the connection's own source/target
/// covering, so a `<C>:` covering prefix is redundant there. For consistency
/// with every other segment-address command, we accept it and strip it —
/// but *only* when `<C>` names this side's covering. Anything else (a bare
/// `@name`/hash, or a positional `file:line[:col]` address, which also
/// contains `:`) is returned untouched so it resolves as before.
fn strip_side_covering<'a>(arg: &'a str, side_covering: &str) -> &'a str {
    match arg.split_once(':') {
        Some((c, rest)) if c == side_covering => rest,
        _ => arg,
    }
}

fn cmd_link(args: &[String]) -> io::Result<u8> {
    if args.len() < 3 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "link <connection> <A> <B> [<note>]",
        ));
    }
    let (kname, ha_in, hb_in) = (&args[0], &args[1], &args[2]);
    // `None` = no note argument (leave an existing note alone); `Some` = a
    // note was given (set or replace it), even if it is the empty string.
    let note = args.get(3).cloned();
    let repo = current_repo()?;
    let mut k = Connection::read(&repo, kname)?;
    let src = Covering::read(&repo, &k.source_covering)?;
    // Resolve each endpoint to a segment. The stored endpoint is the segment's
    // name (`@name`) when it is named — so the link survives content
    // edits — otherwise its segment hash (legacy / anonymous segments).
    // A redundant `<C>:` prefix naming this side's own covering is accepted
    // and stripped, so `link` reads like every other segment-address command.
    let ia = covering::resolve_segment(&src, strip_side_covering(ha_in, &k.source_covering))?;
    let ha = src.entries[ia].endpoint();

    let tgt_repo = Repo {
        root: PathBuf::from(&k.target_repo),
    };
    let tgt = Covering::read(&tgt_repo, &k.target_covering)?;
    let ib = covering::resolve_segment(&tgt, strip_side_covering(hb_in, &k.target_covering))?;
    let hb = tgt.entries[ib].endpoint();

    // Re-linking an existing pair is not an error: it updates the note when
    // one is given (a bare re-link is a harmless idempotence check that keeps
    // the existing note). A brand-new pair is appended.
    if let Some(p) = k.links.iter_mut().find(|p| p.a == ha && p.b == hb) {
        match note {
            Some(new) if new != p.note => {
                p.note = new;
                k.write(&repo)?;
                println!("updated note on {} -> {}", ha, hb);
            }
            _ => eprintln!("(link already present)"),
        }
        return Ok(0);
    }
    k.links.push(Link {
        a: ha.clone(),
        b: hb.clone(),
        note: note.unwrap_or_default(),
    });
    k.write(&repo)?;
    println!("linked {} -> {}", ha, hb);
    Ok(0)
}

fn cmd_unlink(args: &[String]) -> io::Result<u8> {
    if args.len() < 3 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unlink <connection> <A> <B>",
        ));
    }
    let (kname, ha_in, hb_in) = (&args[0], &args[1], &args[2]);
    let repo = current_repo()?;
    let mut k = Connection::read(&repo, kname)?;
    // Accept either a segment address (hash prefix / `@name`, resolved
    // against the covering to the segment's stored endpoint) or a literal
    // link-endpoint prefix (covers links whose segment has since vanished).
    let ha = resolve_endpoint_arg(&repo, &k, ha_in, true)?;
    let hb = resolve_endpoint_arg(&repo, &k, hb_in, false)?;
    let before = k.links.len();
    k.links.retain(|p| !(p.a == ha && p.b == hb));
    if k.links.len() == before {
        eprintln!("(no such link)");
        return Ok(1);
    }
    k.write(&repo)?;
    println!("unlinked {} -> {}", ha, hb);
    Ok(0)
}

/// Resolve an `unlink` endpoint argument to the exact string stored in
/// a link. Tries, in order: (1) `@name` literal already present in a
/// link; (2) resolving the arg against the relevant covering to a segment
/// endpoint; (3) prefix-matching the raw link endpoints (legacy hashes,
/// or segments that have since disappeared). `source_side` picks the A or
/// B column of each link.
fn resolve_endpoint_arg(
    repo: &Repo,
    k: &Connection,
    arg: &str,
    source_side: bool,
) -> io::Result<String> {
    // (1) exact @name endpoint already present?
    if arg.starts_with('@')
        && k.links.iter().any(|p| {
            let ep = if source_side { &p.a } else { &p.b };
            ep == arg
        })
    {
        return Ok(arg.to_string());
    }
    // (2) resolve against the covering on the relevant side. Accept and strip
    // a redundant `<C>:` prefix naming that side's own covering (as `link` does).
    let side_covering = if source_side {
        &k.source_covering
    } else {
        &k.target_covering
    };
    let addr = strip_side_covering(arg, side_covering);
    let cov = if source_side {
        Covering::read(repo, &k.source_covering)
    } else {
        let tgt_repo = Repo {
            root: PathBuf::from(&k.target_repo),
        };
        Covering::read(&tgt_repo, &k.target_covering)
    };
    if let Ok(c) = cov {
        if let Ok(idx) = covering::resolve_segment(&c, addr) {
            return Ok(c.entries[idx].endpoint());
        }
    }
    // (3) fall back to raw prefix match against link endpoints.
    resolve_link_hash(k, arg, source_side)
}

fn resolve_link_hash(k: &Connection, prefix: &str, source_side: bool) -> io::Result<String> {
    let mut matches: Vec<&str> = k
        .links
        .iter()
        .map(|p| {
            if source_side {
                p.a.as_str()
            } else {
                p.b.as_str()
            }
        })
        .filter(|h| h.starts_with(prefix))
        .collect();
    matches.sort();
    matches.dedup();
    match matches.len() {
        0 => Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "no link endpoint matches '{prefix}' in connection '{}'",
                k.name
            ),
        )),
        1 => Ok(matches.remove(0).to_string()),
        n => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("ambiguous: {n} link endpoints match '{prefix}'"),
        )),
    }
}

// ---- connect --------------------------------------------------------------

fn cmd_connect(args: &[String]) -> io::Result<u8> {
    if args.len() < 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "connect <C_A> [<repo>:]<C_B> [<connection-name>]",
        ));
    }
    let src_cov = args[0].clone();
    let target = &args[1];
    let (repo_b, cov_b) = match target.split_once(':') {
        Some((r, c)) => (r.to_string(), c.to_string()),
        None => (".".to_string(), target.to_string()),
    };
    let kname = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| format!("{}__{}", src_cov, cov_b));

    let repo = current_repo()?;
    Covering::read(&repo, &src_cov)?;
    let target_root = if repo_b == "." {
        repo.root.clone()
    } else {
        fs::canonicalize(&repo_b)?
    };
    let tgt_repo = Repo::discover(&target_root).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!(
                "target repo not initialized: {}: {e}",
                target_root.display()
            ),
        )
    })?;
    Covering::read(&tgt_repo, &cov_b)?;

    let k = Connection {
        name: kname.clone(),
        source_covering: src_cov,
        target_repo: tgt_repo.root.to_string_lossy().to_string(),
        target_covering: cov_b,
        links: Vec::new(),
    };
    if repo.connections_dir().join(&kname).exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("connection '{kname}' already exists"),
        ));
    }
    k.write(&repo)?;
    println!("created connection '{kname}': {}", connection_route(&repo, &k));
    Ok(0)
}

// ---- prune ----------------------------------------------------------------

fn cmd_prune(args: &[String]) -> io::Result<u8> {
    let kname = args
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "prune <connection>"))?;
    let repo = current_repo()?;
    let mut k = Connection::read(&repo, kname)?;
    let src = Covering::read(&repo, &k.source_covering)?;
    let tgt_repo = Repo {
        root: PathBuf::from(&k.target_repo),
    };
    let tgt = Covering::read(&tgt_repo, &k.target_covering)?;

    let mut kept: Vec<Link> = Vec::new();
    let mut dropped = 0usize;
    for p in &k.links {
        let a_ok = covering::endpoint_resolves(&src, &p.a);
        let b_ok = covering::endpoint_resolves(&tgt, &p.b);
        if a_ok && b_ok {
            kept.push(p.clone());
        } else {
            dropped += 1;
            println!(
                "broken: {} ({}) -> {} ({}): dropped",
                p.a,
                if a_ok {
                    "ok"
                } else {
                    "missing in source covering"
                },
                p.b,
                if b_ok {
                    "ok"
                } else {
                    "missing in target covering"
                }
            );
        }
    }
    k.links = kept;
    k.write(&repo)?;
    println!("prune: {} link(s) dropped, {} kept", dropped, k.links.len());
    Ok(0)
}

// ---- iter -----------------------------------------------------------------

/// Two-level dispatch for `covmap iter`. Grammar is *name-first*:
/// `covmap iter <name> <verb> [args]`, so `covmap iter <name>` is a stable
/// prefix. Bare `covmap iter` lists all iterators; there is no implicit
/// verb (`covmap iter <name>` with no verb is an error that prints the
/// available verbs).
fn cmd_iter(args: &[String]) -> io::Result<u8> {
    // Bare `covmap iter` → list every iterator.
    if args.is_empty() {
        let repo = current_repo()?;
        let names = Iter::list(&repo)?;
        if names.is_empty() {
            println!("(no iterators yet — run `covmap iter <name> new <covering>`)");
            return Ok(0);
        }
        // Gather the rows first so we can align the name and covering
        // columns to their widest entry (a plain `\t` leaves them ragged).
        let mut rows: Vec<(String, String, String)> = Vec::new();
        for n in &names {
            let it = Iter::read(&repo, n)?;
            rows.push((n.clone(), it.covering.clone(), it.summary()));
        }
        let name_w = rows.iter().map(|(n, _, _)| n.len()).max().unwrap_or(0);
        let cov_w = rows.iter().map(|(_, c, _)| c.len()).max().unwrap_or(0);
        for (n, cov, summary) in &rows {
            println!("{n:<name_w$}  {cov:<cov_w$}  {summary}");
        }
        return Ok(0);
    }

    let name = args[0].as_str();
    let verb = match args.get(1) {
        Some(v) => v.as_str(),
        None => {
            eprintln!(
                "covmap iter {name} <verb> — no verb given. Verbs: new, next, current, \
                 done, skip, choose, rewind, peek, status, finished, rm"
            );
            return Ok(2);
        }
    };
    let rest = &args[2..];

    match verb {
        "new" => cmd_iter_new(name, rest),
        "next" => cmd_iter_next(name, rest),
        "current" => cmd_iter_current(name, rest),
        "done" => cmd_iter_resolve(name, rest, TaskState::Done),
        "skip" => cmd_iter_resolve(name, rest, TaskState::Skipped),
        "choose" => cmd_iter_choose(name, rest),
        "rewind" => cmd_iter_rewind(name, rest),
        "peek" => cmd_iter_peek(name, rest),
        "status" => no_args(verb, rest).and_then(|_| cmd_iter_status(name)),
        "finished" => no_args(verb, rest).and_then(|_| cmd_iter_finished(name)),
        "rm" => no_args(verb, rest).and_then(|_| cmd_iter_rm(name)),
        other => {
            eprintln!(
                "covmap iter {name}: unknown verb '{other}'. Verbs: new, next, current, \
                 done, skip, choose, rewind, peek, status, finished, rm"
            );
            Ok(2)
        }
    }
}

/// Print the frozen content of a task to stdout (used by `--content`).
fn print_task_content(repo: &Repo, t: &Task) -> io::Result<()> {
    match segment::read_object(&repo.objects_dir(), &t.hash) {
        Ok(content) => {
            let mut out = io::stdout().lock();
            out.write_all(content.as_bytes())?;
            if !content.ends_with('\n') {
                out.write_all(b"\n")?;
            }
            Ok(())
        }
        Err(_) => {
            eprintln!("(content for {} unavailable in object store)", t.handle());
            Ok(())
        }
    }
}

fn cmd_iter_new(name: &str, args: &[String]) -> io::Result<u8> {
    let mut covering: Option<String> = None;
    let mut order = Order::File;
    let mut label: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--order" => {
                let v = args.get(i + 1).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "--order needs a value")
                })?;
                order = Order::parse(v)?;
                i += 2;
            }
            "--label" => {
                let v = args.get(i + 1).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "--label needs a value")
                })?;
                label = Some(v.clone());
                i += 2;
            }
            other if other.starts_with('-') => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("iter new: unexpected flag '{other}'"),
                ));
            }
            other => {
                if covering.is_some() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "iter new accepts a single covering name",
                    ));
                }
                covering = Some(other.to_string());
                i += 1;
            }
        }
    }
    let covering = covering.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "iter <name> new <covering> [--order file|label] [--label <l>]",
        )
    })?;
    let repo = current_repo()?;
    if repo.iterators_dir().join(name).exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("iterator '{name}' already exists (rm it first to re-freeze)"),
        ));
    }
    // Resolve the covering now (accepting HEAD), but store its given name so
    // a later reader sees what the user asked for.
    let cov = resolve_covering(&repo, &covering)?;
    let tasks = iterator::freeze(&cov, order, label.as_deref());
    if tasks.is_empty() {
        eprintln!(
            "(warning: iterator '{name}' froze 0 tasks{})",
            match &label {
                Some(l) => format!(" — no segment labelled '{l}' in covering '{}'", cov.name),
                None => String::new(),
            }
        );
    }
    let it = Iter {
        name: name.to_string(),
        covering: cov.name.clone(),
        order,
        label,
        cursor: None,
        tasks,
    };
    it.write(&repo)?;
    let (_, _, _, total) = it.counts();
    println!(
        "froze covering '{}' into iterator '{}': {} task(s), order={}{}",
        it.covering,
        name,
        total,
        it.order.as_str(),
        match &it.label {
            Some(l) => format!(", label={l}"),
            None => String::new(),
        }
    );
    Ok(0)
}

/// Parse the optional `--content` flag accepted by `next`/`current`/`peek`.
/// (`peek` also takes `--all`, so it parses its own args.)
fn parse_content_flag(verb: &str, args: &[String]) -> io::Result<bool> {
    let mut content = false;
    for a in args {
        match a.as_str() {
            "--content" => content = true,
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("iter {verb}: unexpected arg '{other}' (only --content is accepted)"),
                ));
            }
        }
    }
    Ok(content)
}

fn cmd_iter_next(name: &str, args: &[String]) -> io::Result<u8> {
    let content = parse_content_flag("next", args)?;
    let repo = current_repo()?;
    let mut it = Iter::read(&repo, name)?;
    // Enumeration guarantee: refuse to advance off a still-pending task.
    if it.current_is_pending() {
        let cur = &it.tasks[it.cursor.unwrap()];
        eprintln!(
            "current task is still pending: {} — resolve it with `covmap iter {name} done` \
             or `covmap iter {name} skip` before advancing",
            cur.handle()
        );
        return Ok(1);
    }
    match it.first_pending() {
        Some(idx) => {
            it.cursor = Some(idx);
            it.write(&repo)?;
            println!("{}", it.tasks[idx].print_line());
            if content {
                print_task_content(&repo, &it.tasks[idx])?;
            }
            Ok(0)
        }
        None => {
            // Exhausted: print nothing, exit non-zero so a shell loop stops.
            Ok(1)
        }
    }
}

fn cmd_iter_current(name: &str, args: &[String]) -> io::Result<u8> {
    let content = parse_content_flag("current", args)?;
    let repo = current_repo()?;
    let it = Iter::read(&repo, name)?;
    match it.cursor {
        Some(idx) => {
            println!("{}", it.tasks[idx].print_line());
            if content {
                print_task_content(&repo, &it.tasks[idx])?;
            }
            Ok(0)
        }
        None => {
            eprintln!("(cursor unset — run `covmap iter {name} next` to start)");
            Ok(1)
        }
    }
}

fn cmd_iter_resolve(name: &str, args: &[String], state: TaskState) -> io::Result<u8> {
    let note = parse_message(args)?;
    let repo = current_repo()?;
    let mut it = Iter::read(&repo, name)?;
    // A resolution replaces the note, so re-resolving without -m clears a
    // stale one (e.g. `done -m "verified"` then `skip` leaves no note).
    let idx = it.resolve_current(state, note)?;
    it.write(&repo)?;
    let verb = match state {
        TaskState::Done => "done",
        TaskState::Skipped => "skipped",
        TaskState::Pending => "pending",
    };
    println!("{}\t{verb}", it.tasks[idx].handle());
    Ok(0)
}

fn cmd_iter_choose(name: &str, args: &[String]) -> io::Result<u8> {
    let addr = args
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "iter <name> choose <addr>"))?;
    let repo = current_repo()?;
    let mut it = Iter::read(&repo, name)?;
    let idx = it.resolve_task(addr)?;
    it.cursor = Some(idx);
    it.write(&repo)?;
    println!("{}", it.tasks[idx].print_line());
    Ok(0)
}

fn cmd_iter_rewind(name: &str, args: &[String]) -> io::Result<u8> {
    let repo = current_repo()?;
    let mut it = Iter::read(&repo, name)?;
    let idx = match args.first() {
        Some(addr) => it.resolve_task(addr)?,
        None => it.cursor.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "iter <name> rewind [<addr>] — no cursor set, pass an address",
            )
        })?,
    };
    if it.tasks[idx].state == TaskState::Pending {
        println!("{}\t(already pending)", it.tasks[idx].handle());
        return Ok(0);
    }
    it.tasks[idx].state = TaskState::Pending;
    it.tasks[idx].note.clear();
    it.write(&repo)?;
    println!("{}\trewound to pending", it.tasks[idx].handle());
    Ok(0)
}

fn cmd_iter_peek(name: &str, args: &[String]) -> io::Result<u8> {
    let mut all = false;
    let mut content = false;
    for a in args {
        match a.as_str() {
            "--all" => all = true,
            "--content" => content = true,
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("iter peek: unexpected arg '{other}'"),
                ));
            }
        }
    }
    let repo = current_repo()?;
    let it = Iter::read(&repo, name)?;
    if all {
        // The whole worklist in frozen order, each line tagged with its
        // state (and note) — an audit view including done/skipped tasks.
        for t in &it.tasks {
            println!("{}", t.print_line_stateful());
            if content {
                print_task_content(&repo, t)?;
            }
        }
        return Ok(0);
    }
    // Default: every *pending* task, in frozen order — the set you can
    // `choose` among (includes the active one). This is a look-ahead, so it
    // does not move the cursor.
    let pending = it.pending_indices();
    if pending.is_empty() {
        eprintln!("(no pending tasks remain)");
        return Ok(1);
    }
    for &idx in &pending {
        println!("{}", it.tasks[idx].print_line());
        if content {
            print_task_content(&repo, &it.tasks[idx])?;
        }
    }
    Ok(0)
}

fn cmd_iter_status(name: &str) -> io::Result<u8> {
    let repo = current_repo()?;
    let it = Iter::read(&repo, name)?;
    println!("{}", it.summary());
    Ok(0)
}

fn cmd_iter_finished(name: &str) -> io::Result<u8> {
    let repo = current_repo()?;
    let it = Iter::read(&repo, name)?;
    Ok(if it.is_finished() { 0 } else { 1 })
}

fn cmd_iter_rm(name: &str) -> io::Result<u8> {
    let repo = current_repo()?;
    let path = repo.iterators_dir().join(name);
    if !path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("iterator '{name}' not found"),
        ));
    }
    fs::remove_file(&path)?;
    println!("removed iterator '{name}' (covering untouched)");
    Ok(0)
}

/// Reject any argument to a verb that takes none. This makes a stray token
/// fail loudly instead of being silently ignored — notably a `#` comment
/// pasted into an interactive zsh, where `#` is not a comment and arrives
/// as an argument (which otherwise leaves the task unresolved and a
/// `while ! finished` loop spinning).
fn no_args(verb: &str, args: &[String]) -> io::Result<()> {
    match args.first() {
        None => Ok(()),
        Some(a) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("iter {verb} takes no arguments (got '{a}')"),
        )),
    }
}

/// Parse an optional `-m <msg>` / `--message <msg>` note argument. Any other
/// argument is an error.
fn parse_message(args: &[String]) -> io::Result<Option<String>> {
    let mut msg: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-m" | "--message" => {
                let v = args.get(i + 1).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "-m needs a message")
                })?;
                msg = Some(v.clone());
                i += 2;
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unexpected argument '{other}' (only -m <msg> is accepted)"),
                ));
            }
        }
    }
    Ok(msg)
}

// ---- html -----------------------------------------------------------------

fn cmd_html(args: &[String]) -> io::Result<u8> {
    let mut out_path: Option<String> = None;
    let mut covering: Option<String> = None;
    let mut connection: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                out_path = Some(args.get(i + 1).cloned().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "-o needs a path")
                })?);
                i += 2;
            }
            "--covering" => {
                covering = Some(args.get(i + 1).cloned().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "--covering needs a name")
                })?);
                i += 2;
            }
            "--connection" => {
                connection = Some(args.get(i + 1).cloned().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "--connection needs a name")
                })?);
                i += 2;
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("html: unexpected arg '{other}'"),
                ));
            }
        }
    }
    let repo = current_repo()?;
    let path = match out_path {
        Some(p) => PathBuf::from(p),
        None => repo.dot().join("report.html"),
    };
    // The report renders a single connection at a time, so its Target/Compare
    // views show one coherent B covering (mixing connections that target
    // different coverings of the same file is what made the old all-at-once
    // report confusing). When `--connection` is omitted, resolve it:
    //   - 0 connections  -> None: List/Source views of the A covering only.
    //   - exactly 1       -> use it automatically (the unambiguous case).
    //   - 2 or more       -> error, listing names, requiring `--connection`.
    if connection.is_none() {
        let all = crate::connection::Connection::list(&repo)?;
        match all.len() {
            0 => {}
            1 => connection = Some(all.into_iter().next().unwrap()),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "html: {} connections exist; pass --connection <K> to choose one:\n  {}",
                        all.len(),
                        all.join("\n  ")
                    ),
                ));
            }
        }
    }
    // `--covering`/`--connection` explicitly select what the report shows,
    // independent of HEAD. Selecting a connection also fixes the source
    // covering to that connection's source covering (its Target/Compare
    // views then render the connection's target side), so pointing the
    // report at a specific map no longer depends on cwd or HEAD.
    let html = crate::html::render(&repo, covering.as_deref(), connection.as_deref())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, html)?;
    println!("wrote {}", path.display());
    Ok(0)
}

// ---- migrate --------------------------------------------------------------

/// Upgrade a `.covmap/` from on-disk format 1 → 2: recompute every segment
/// hash from content alone (dropping the label from the hash and from the
/// object body), then rewrite every hash reference in coverings,
/// connections, and iterators. Idempotent: a version-2 repo is left alone.
fn cmd_migrate(_args: &[String]) -> io::Result<u8> {
    let repo = Repo::discover_unchecked(&env::current_dir()?)?;
    let version = repo.read_version().unwrap_or_default();
    match version.as_str() {
        "2" => {
            println!("already at format 2 — nothing to migrate");
            return Ok(0);
        }
        "1" => {}
        other => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("cannot migrate from format '{other}' (expected '1')"),
            ));
        }
    }

    // old segment-hash -> new content-only hash, learned from the coverings
    // (the only place we can recover a segment's content + old label).
    let mut remap: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let objects = repo.objects_dir();
    let mut objects_written = 0usize;

    for cname in Covering::list(&repo)? {
        let mut c = Covering::read(&repo, &cname)?;
        for e in &mut c.entries {
            // Read the v1 object body (`label \0 content`) and recover the
            // pure content, then re-key it under the content-only hash.
            let content = read_v1_content(&objects, &e.hash)?;
            let new_hash = segment::hash_of(&content);
            if new_hash != e.hash {
                remap.insert(e.hash.clone(), new_hash.clone());
            }
            let seg = crate::segment::Segment {
                file: e.file.clone(),
                a: e.a,
                b: e.b,
                label: e.label.clone(),
                content,
            };
            segment::write_object(&objects, &seg)?;
            objects_written += 1;
            e.hash = new_hash;
        }
        c.write(&repo)?;
    }

    // Rewrite bare-hash endpoints in connections. `@name` endpoints and
    // endpoints into a not-yet-migrated target repo are left untouched
    // (the latter resolve again once that repo is migrated).
    let mut conn_eps = 0usize;
    for kname in Connection::list(&repo)? {
        let mut k = Connection::read(&repo, &kname)?;
        for l in &mut k.links {
            if let Some(n) = remap.get(&l.a) {
                l.a = n.clone();
                conn_eps += 1;
            }
            if let Some(n) = remap.get(&l.b) {
                l.b = n.clone();
                conn_eps += 1;
            }
        }
        k.write(&repo)?;
    }

    // Rewrite frozen task hashes in iterators (the cursor is an index, not
    // a hash, so it needs no rewrite).
    let mut iter_tasks = 0usize;
    for iname in crate::iterator::Iter::list(&repo)? {
        let mut it = crate::iterator::Iter::read(&repo, &iname)?;
        for t in &mut it.tasks {
            if let Some(n) = remap.get(&t.hash) {
                t.hash = n.clone();
                iter_tasks += 1;
            }
        }
        it.write(&repo)?;
    }

    fs::write(repo.dot().join("version"), "2\n")?;
    println!(
        "migrated to format 2: {} object(s) re-keyed ({} hash change(s)), \
         {} connection endpoint(s) and {} iterator task(s) rewritten",
        objects_written,
        remap.len(),
        conn_eps,
        iter_tasks
    );
    println!("run `covmap fsck` to confirm every partition and endpoint resolves");
    Ok(0)
}

/// Read a format-1 object body and return its content, stripping the
/// leading `label \0` prefix. Format-1 objects always contain a NUL; if
/// one is somehow absent, treat the whole body as content.
fn read_v1_content(objects_dir: &std::path::Path, hash: &str) -> io::Result<String> {
    let path = segment::object_path(objects_dir, hash);
    let buf = fs::read(&path)?;
    let content = match buf.iter().position(|&b| b == 0) {
        Some(nul) => &buf[nul + 1..],
        None => &buf[..],
    };
    Ok(String::from_utf8_lossy(content).to_string())
}

// ---- plumbing -------------------------------------------------------------

fn cmd_hash_segment(args: &[String]) -> io::Result<u8> {
    if args.len() < 3 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "hash-segment <file> <a> <b> [-w]",
        ));
    }
    let file = &args[0];
    let (a, a_col) = parse_line_col(&args[1])?;
    let (b, b_col) = parse_line_col(&args[2])?;
    // The hash is content only, so hash-segment takes no label; a stray
    // non-flag argument is a mistake (likely an old-style label).
    let mut write = false;
    for a in &args[3..] {
        if a == "-w" {
            write = true;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("hash-segment: unexpected argument '{a}' (labels are not hashed; the hash is content only)"),
            ));
        }
    }
    let raw = fs::read_to_string(file)?;
    let lines: Vec<String> = raw.split_inclusive('\n').map(|s| s.to_string()).collect();
    let content = segment::extract_content(&lines, a, a_col, b, b_col).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "range {}..{} out of bounds for {file} ({} lines)",
                args[1],
                args[2],
                lines.len()
            ),
        )
    })?;
    let seg = Segment {
        file: file.to_string(),
        a,
        b,
        label: String::new(),
        content,
    };
    let h = seg.hash();
    println!("{h}");
    if write {
        let repo = current_repo()?;
        segment::write_object(&repo.objects_dir(), &seg)?;
    }
    Ok(0)
}

fn cmd_cat_segment(args: &[String]) -> io::Result<u8> {
    let prefix = args
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "cat-segment <hash>"))?;
    let repo = current_repo()?;
    let full = segment::resolve_prefix(&repo.objects_dir(), prefix)?;
    let content = segment::read_object(&repo.objects_dir(), &full)?;
    let mut out = io::stdout().lock();
    out.write_all(content.as_bytes())?;
    Ok(0)
}

fn cmd_ls_segments(args: &[String]) -> io::Result<u8> {
    let mut long = false;
    let mut name: Option<String> = None;
    for a in args {
        match a.as_str() {
            "-l" => long = true,
            other => name = Some(other.to_string()),
        }
    }
    let repo = current_repo()?;
    let c = resolve_covering(&repo, &name.unwrap_or_else(|| "HEAD".to_string()))?;
    for e in &c.entries {
        if long {
            // Full per-segment record, TAB-delimited, empty fields for
            // absent name/message — the machine substrate `ls -l` renders.
            // Columns: hash file a b label name message(first line).
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                e.hash,
                e.file,
                e.a,
                e.b,
                e.label,
                e.name.as_deref().unwrap_or(""),
                e.message
                    .as_deref()
                    .map(|m| m.split('\n').next().unwrap_or(""))
                    .unwrap_or("")
            );
        } else {
            println!("{}", e.hash);
        }
    }
    Ok(0)
}

fn cmd_ls_links(args: &[String]) -> io::Result<u8> {
    let mut resolve = false;
    let mut name: Option<String> = None;
    for a in args {
        match a.as_str() {
            "--resolve" => resolve = true,
            other => name = Some(other.to_string()),
        }
    }
    let n = name.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "ls-links [--resolve] <connection>",
        )
    })?;
    let repo = current_repo()?;
    let k = Connection::read(&repo, &n)?;
    if resolve {
        // Resolved form: a-label a-loc b-label b-loc note (TAB), so a
        // consumer never has to join by hand. Endpoints that no longer
        // resolve emit empty label/loc fields.
        let src = Covering::read(&repo, &k.source_covering)?;
        let tgt_repo = Repo {
            root: PathBuf::from(&k.target_repo),
        };
        let tgt = Covering::read(&tgt_repo, &k.target_covering).ok();
        for p in &k.links {
            let (al, aloc) = resolve_fields(Some(&src), &p.a);
            let (bl, bloc) = resolve_fields(tgt.as_ref(), &p.b);
            println!("{}\t{}\t{}\t{}\t{}", al, aloc, bl, bloc, p.note);
        }
    } else {
        for p in &k.links {
            if p.note.is_empty() {
                println!("{}\t{}", p.a, p.b);
            } else {
                println!("{}\t{}\t{}", p.a, p.b, p.note);
            }
        }
    }
    Ok(0)
}

/// (label, loc) for an endpoint, or empty strings when it does not resolve.
fn resolve_fields(c: Option<&Covering>, ep: &str) -> (String, String) {
    let hash = c.and_then(|c| covering::endpoint_to_hash(c, ep));
    match (c, hash) {
        (Some(c), Some(h)) => c
            .entries
            .iter()
            .find(|e| e.hash == h)
            .map(|e| {
                let l = if e.label.is_empty() {
                    String::new()
                } else {
                    e.label.clone()
                };
                (l, e.loc())
            })
            .unwrap_or_default(),
        _ => (String::new(), String::new()),
    }
}

fn cmd_rev_parse(args: &[String]) -> io::Result<u8> {
    let addr = args
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "rev-parse <addr>"))?;
    let repo = current_repo()?;
    let (covering, prefix) = parse_addr(addr);
    let full = match covering {
        Some(cn) => {
            let c = resolve_covering(&repo, &cn)?;
            covering::resolve_segment_hash(&c, &prefix)?
        }
        None => segment::resolve_prefix(&repo.objects_dir(), &prefix)?,
    };
    println!("{full}");
    Ok(0)
}

fn cmd_fsck(args: &[String]) -> io::Result<u8> {
    let mut broken_only = false;
    for a in args {
        if a == "--broken-links" {
            broken_only = true;
        }
    }
    let repo = current_repo()?;
    let mut errs = 0usize;
    for cn in Covering::list(&repo)? {
        let c = Covering::read(&repo, &cn)?;
        let mut by_file: std::collections::BTreeMap<&str, Vec<&CoveringEntry>> =
            std::collections::BTreeMap::new();
        for e in &c.entries {
            by_file.entry(e.file.as_str()).or_default().push(e);
        }
        for (f, mut entries) in by_file {
            entries.sort_by_key(|e| e.sort_key());
            // The partition must begin at the first character of the
            // file and proceed with each segment abutting the previous
            // one (no gap, no overlap), down to the character.
            let mut expected = covering::Pos { line: 1, col: None };
            for e in &entries {
                if !covering::same_start(expected, e.start_pos()) {
                    if !broken_only {
                        eprintln!(
                            "fsck: covering {} file {} has gap or overlap before {} (expected start {})",
                            cn,
                            f,
                            e.range(),
                            covering::fmt_pos(expected)
                        );
                    }
                    errs += 1;
                }
                expected = covering::successor_of_end(e.end_pos());
            }
        }
        for e in &c.entries {
            if !object_exists(&repo, &e.hash) {
                if !broken_only {
                    eprintln!(
                        "fsck: covering {} hash {} missing in object store",
                        cn, e.hash
                    );
                }
                errs += 1;
            }
        }
    }
    for kn in Connection::list(&repo)? {
        let k = Connection::read(&repo, &kn)?;
        let src = Covering::read(&repo, &k.source_covering)?;
        let tgt_repo = Repo {
            root: PathBuf::from(&k.target_repo),
        };
        let tgt = Covering::read(&tgt_repo, &k.target_covering)?;
        for p in &k.links {
            let a_ok = covering::endpoint_resolves(&src, &p.a);
            let b_ok = covering::endpoint_resolves(&tgt, &p.b);
            if !a_ok || !b_ok {
                println!("{}\t{}\t{}", kn, p.a, p.b);
                errs += 1;
            }
        }
    }
    if !broken_only {
        if errs == 0 {
            println!("fsck: ok");
        } else {
            eprintln!("fsck: {errs} issue(s)");
        }
    }
    Ok(if errs > 0 { 1 } else { 0 })
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
            std::env::temp_dir().join(format!("covmap-cli-{tag}-{}-{nonce}", std::process::id()));
        fs::create_dir_all(root.join(".covmap")).unwrap();
        Repo { root }
    }

    #[test]
    fn connection_route_marks_same_repository_targets() {
        let repo = temp_repo("route");
        let mut k = Connection {
            name: "map".to_string(),
            source_covering: "left".to_string(),
            target_repo: repo.root.to_string_lossy().to_string(),
            target_covering: "right".to_string(),
            links: Vec::new(),
        };

        assert_eq!(connection_route(&repo, &k), "left -> right (self-map)");
        k.target_repo = "/tmp/elsewhere".to_string();
        assert_eq!(connection_route(&repo, &k), "left -> /tmp/elsewhere/right");
        fs::remove_dir_all(repo.root).unwrap();
    }
}
