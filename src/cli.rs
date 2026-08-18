//! Command dispatch.
//!
//! See covmap/theory/main.tex §5 for the model. Output is line-oriented
//! and pipe-friendly.

use crate::cache::{self, HydrateOutcome};
use crate::connection::{Connection, LegacyConnection, Link};
use crate::covering::{self, Covering, CoveringEntry};
use crate::iterator::{self, Iter, Order, Task, TaskState};
use crate::peer;
use crate::repo::Repo;
use crate::segment::{self, Segment, HASH_LEN};
use crate::walk::{self, DEFAULT_LABEL};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

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
        "hydrate" => cmd_hydrate(&rest),
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
        "peer" => cmd_peer(&rest),
        "disconnect" => cmd_disconnect(&rest),
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
        "hydrate" => {
            "covmap hydrate [<covering>]                  reconstruct missing live objects from matching source"
        }
        "status" => "covmap status [<covering>]                   cardinality, lines, drift, broken links",
        "show" => {
            "covmap show <addr> [--content] [--all]   inspect a segment, covering, region, link, or connection"
        }
        "ls" => {
            "covmap ls [-l] [<covering>] [<region> ...] [--label <l>] [--linked|--unlinked]   list segments, optionally narrowed to regions"
        }
        "grep" => "covmap grep [-l] [-c <C>] <pattern>          segments whose label or content matches",
        "diff" => "covmap diff <C1> <C2>                        added / removed / shared",
        "label" => "covmap label <C>:<h> <label>                 relabel a segment",
        "name" => "covmap name <C>:<h|@name> <name|->         set, rename, or clear (-) a segment's name",
        "annotate" => "covmap annotate <C>:<addr> [-m <msg>...] | -   set, replace, clear (-), or print a segment's message",
        "split" => {
            "covmap split <C>:<segment|file> <cut> [<cut> ...]   atomically insert line[:column] cuts; apply metadata separately"
        }
        "merge" => "covmap merge <C>:<h1> <C>:<h2> [<label>]     merge two adjacent segments",
        "recut" => "covmap recut <C>:<h|@name> | recut --remap [<C>[:<a>]]   refresh a segment, or diff-remap a covering's segments onto the edited file",
        "link" => {
            "covmap link <K> <A> <B> [<note>]            add a link to connection K\n  <A> and <B>: @name, hash prefix, or file:line[:col] in that side's covering"
        }
        "unlink" => {
            "covmap unlink <K> <A> <B>                   remove a link\n  <A> and <B>: @name, hash prefix, or file:line[:col] in that side's covering"
        }
        "connect" => {
            "covmap connect <C_A> [<peer>:]<C_B> [<K>]   create an empty connection; omit <peer>: for a same-repository target"
        }
        "peer" => {
            "covmap peer bind <alias> <path> | ls | rename <old> <new> | unbind <alias>"
        }
        "disconnect" => {
            "covmap disconnect <K>                       delete only the named connection ref"
        }
        "prune" => "covmap prune <K>                             drop links whose endpoints are no longer in their coverings",
        "iter" => {
            "covmap iter [<name> <verb> ...]              freeze a covering into an ordered worklist\n  peek          list pending tasks\n  peek --all    list the complete frozen worklist with state and notes"
        }
        "html" => "covmap html [-o <path>] [--covering <C>] [--connection <K>]   render a static HTML browser (one connection; auto-selected if only one exists)",
        "migrate" => "covmap migrate                              upgrade .covmap/ on-disk format (1/2 → 3)",
        "hash-segment" => "covmap hash-segment <file> <a[:col]> <b[:col]> [-w]",
        "cat-segment" => "covmap cat-segment <hash>",
        "ls-segments" => "covmap ls-segments <covering>",
        "ls-links" => "covmap ls-links <connection>",
        "rev-parse" => "covmap rev-parse <addr>",
        "fsck" => "covmap fsck [--broken-links] [--offline [--hydrate]]",
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
  hydrate [<covering>]                reconstruct missing live objects from matching source
  status [<covering>]                 cardinality, lines, drift, broken links
  show <addr> [--content] [--all]     inspect a segment, covering, region, link, or connection
  ls [-l] [<covering>] [<region> ...] list segments, optionally narrowed to regions
  grep [-l] [-c <C>] <pattern>        segments whose label or content matches
  diff <C1> <C2>                      added / removed / shared
  label <C>:<h> <label>               relabel a segment
  name <C>:<h|@name> <name|->         set, rename, or clear (-) a segment's name
  annotate <C>:<addr> [-m <msg>...]   set/replace/clear (-)/print a segment's free-form message
  split <C>:<segment|file> <cut> [<cut> ...]   atomically insert line[:column] cuts; apply metadata separately
  merge <C>:<h1> <C>:<h2> [<label>]   merge two adjacent segments
  recut <C>:<h|@name>                 refresh a segment's hash from current disk bytes
  recut --remap [<C>[:<addr>]] [--threshold <f>]   diff-remap; for unresolved segments, print fingerprint relocation proposals (propose-only)
  link <K> <A> <B> [<note>]           add a link; endpoints accept @name, hash prefix, or position
  unlink <K> <A> <B>                  remove a link by @name, hash prefix, or position
  connect <C_A> [<peer>:]<C_B> [<K>]  create empty connection; omit <peer>: for same-repo
  peer bind <alias> <path> | ls | rename <old> <new> | unbind <alias>
                                      manage checkout-local peer bindings
  disconnect <K>                      delete only the named connection ref
  prune <K>                           drop links whose endpoints are no longer in their coverings
  iter [<name> <verb> ...]            freeze a covering into an ordered, resumable worklist
  html [-o <path>] [--covering <C>] [--connection <K>]   render a static HTML browser
  migrate                            upgrade .covmap/ on-disk format (1/2 → 3)

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
  fsck [--broken-links] [--offline [--hydrate]]
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
    println!("  Project artifacts: .covmap/coverings, .covmap/connections, and covmap metadata");
    println!("  Local state:       .covmap/objects, .covmap/iterators, .covmap/local");
    println!("  Next:              git add .covmap");
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

// ---- hydrate --------------------------------------------------------------

fn cmd_hydrate(args: &[String]) -> io::Result<u8> {
    if args.len() > 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "hydrate [<covering>]",
        ));
    }
    let repo = current_repo()?;
    let coverings = match args.first() {
        Some(name) => vec![resolve_covering(&repo, name)?],
        None => Covering::list(&repo)?
            .into_iter()
            .map(|name| Covering::read(&repo, &name))
            .collect::<io::Result<Vec<_>>>()?,
    };
    if coverings.is_empty() {
        println!("hydrate: no coverings");
        return Ok(0);
    }

    let mut present = 0usize;
    let mut written = 0usize;
    let mut errors = 0usize;
    for covering in coverings {
        for issue in covering::partition_issues(&covering) {
            eprintln!("hydrate: covering {} {issue}", covering.name);
            errors += 1;
        }
        let mut by_file: BTreeMap<&str, Vec<&CoveringEntry>> = BTreeMap::new();
        for entry in &covering.entries {
            by_file.entry(entry.file.as_str()).or_default().push(entry);
        }
        for (file, entries) in by_file {
            let lines = match cache::source_lines(&repo, file) {
                Ok(lines) => lines,
                Err(error) => {
                    eprintln!("hydrate: covering {}: {error}", covering.name);
                    errors += 1;
                    continue;
                }
            };
            for entry in entries {
                match cache::hydrate_entry_from_lines(&repo, entry, &lines) {
                    Ok(HydrateOutcome::Present) => present += 1,
                    Ok(HydrateOutcome::Written) => written += 1,
                    Err(error) => {
                        let hint = hydrate_recovery_hint(&error);
                        eprintln!(
                            "hydrate: covering {} {}: {error}{hint}",
                            covering.name,
                            entry.loc()
                        );
                        errors += 1;
                    }
                }
            }
        }
    }
    if errors == 0 {
        println!("hydrate: {written} written, {present} already present");
        Ok(0)
    } else {
        eprintln!("hydrate: {errors} issue(s)");
        Ok(1)
    }
}

fn hydrate_recovery_hint(error: &io::Error) -> &'static str {
    let message = error.to_string();
    if message.contains("source drift") || message.contains("source range") {
        "; restore matching source, or repair the covering with `covmap recut` or `covmap recut --remap`"
    } else if message.contains("cannot read source") || message.contains("is not UTF-8") {
        "; restore the covered source, or deliberately remove it from the covering"
    } else {
        ""
    }
}

fn print_cache_warnings(covering: &str, warnings: Vec<String>) {
    for warning in warnings {
        eprintln!("warning: covering {covering}: {warning}");
    }
}

// ---- status ---------------------------------------------------------------

/// Render a connection's two coverings without exposing the repository's
/// absolute path for a same-repository target.
fn connection_route(repo: &Repo, k: &Connection) -> String {
    if k.target_peer == peer::SELF {
        format!("{} -> {} (self-map)", k.source_covering, k.target_covering)
    } else {
        let suffix = match peer::resolve(repo, &k.target_peer) {
            Ok(_) => String::new(),
            Err(error) => format!(" (unavailable: {error})"),
        };
        format!(
            "{} -> {}:{}{}",
            k.source_covering, k.target_peer, k.target_covering, suffix
        )
    }
}

fn resolve_target_repo(repo: &Repo, k: &Connection) -> io::Result<Repo> {
    peer::resolve(repo, &k.target_peer)
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
        print_cache_warnings(
            &c.name,
            cache::warm_entries_best_effort(&repo, c.entries.iter()),
        );
        // Per-file drift: are the bytes on disk still consistent with
        // the recorded segments?
        let mut by_file: std::collections::BTreeMap<&str, Vec<&CoveringEntry>> =
            std::collections::BTreeMap::new();
        for e in &c.entries {
            by_file.entry(e.file.as_str()).or_default().push(e);
        }
        let mut drifted = Vec::new();
        let mut missing = Vec::new();
        for (file, mut entries) in by_file {
            entries.sort_by_key(|e| e.a);
            let abs = repo.root.join(file);
            let lines = match walk::read_text_lines(&abs) {
                Ok(Some(l)) => l,
                Ok(None) | Err(_) => {
                    missing.push(file);
                    continue;
                }
            };
            if !file_matches_segments(&lines, &entries) {
                drifted.push(file);
            }
        }
        if drifted.is_empty() && missing.is_empty() {
            println!("  drift: clean");
        } else {
            println!(
                "  drift: {} file(s) edited, {} file(s) missing/binary",
                drifted.len(),
                missing.len()
            );
            for file in drifted {
                println!("    edited: {file}");
            }
            for file in missing {
                println!("    missing/binary: {file}");
            }
            println!("  details: run `covmap fsck --offline`");
        }
    }

    let conns = Connection::list(&repo)?;
    if !conns.is_empty() {
        println!("connections:");
        for cn in conns {
            let k = Connection::read(&repo, &cn)?;
            let src = Covering::read(&repo, &k.source_covering)?;
            let tgt = resolve_target_repo(&repo, &k)
                .ok()
                .and_then(|target| Covering::read(&target, &k.target_covering).ok());
            let mut source_broken = 0usize;
            let mut target_broken = 0usize;
            for p in &k.links {
                let a_ok = covering::endpoint_resolves(&src, &p.a);
                if !a_ok {
                    source_broken += 1;
                }
                if let Some(target) = &tgt {
                    if !covering::endpoint_resolves(target, &p.b) {
                        target_broken += 1;
                    }
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
            let integrity = if tgt.is_some() {
                format!("{} broken", source_broken + target_broken)
            } else {
                format!("{source_broken} source-broken, target unchecked")
            };
            println!(
                "  {}: {} : {} links, {} · coverage {}",
                cn,
                connection_route(&repo, &k),
                k.links.len(),
                integrity,
                cov_note
            );
        }
    }
    Ok(0)
}

/// Check that the recorded segments for `file` reconstruct its current
/// content byte-for-byte using the hashes in the covering ref. Cache objects
/// are not required for drift detection.
fn file_matches_segments(lines: &[String], entries: &[&CoveringEntry]) -> bool {
    let mut sorted: Vec<&&CoveringEntry> = entries.iter().collect();
    sorted.sort_by_key(|e| e.sort_key());
    // The next start position the partition must supply, beginning at
    // the first character of the file.
    let mut expected = covering::Pos { line: 1, col: None };
    for e in &sorted {
        if !covering::same_start(expected, e.start_pos()) {
            return false;
        }
        let on_disk = match segment::extract_content(lines, e.a, e.a_col, e.b, e.b_col) {
            Some(c) => c,
            None => return false,
        };
        if segment::hash_of(&on_disk) != e.hash {
            return false;
        }
        expected = covering::successor_of_end(e.end_pos());
    }
    // The covering must reach exactly one past the end of the file.
    covering::same_start(expected, covering::file_end(lines.len()))
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
            if let Some(region) = parse_region(&c, &prefix) {
                return show_region(&repo, &c, &region?);
            }
        }
    } else if repo.coverings_dir().join(addr).is_file() {
        let c = Covering::read(&repo, addr)?;
        return show_covering(&repo, &c);
    }

    // Otherwise a single segment — DETAIL.
    show_segment(&repo, covering.as_deref(), &prefix, with_content)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Region {
    file: String,
    range: Option<(usize, usize)>,
}

impl Region {
    fn contains(&self, entry: &CoveringEntry) -> bool {
        entry.file == self.file
            && self
                .range
                .map(|(a, b)| overlaps(entry.a, entry.b, a, b))
                .unwrap_or(true)
    }
}

/// Parse a covered file or `file:a..b` region. Exact covered filenames win,
/// including filenames containing `:`. `None` means the token may instead be
/// a segment address; a range-shaped token returns a specific error.
fn parse_region(c: &Covering, input: &str) -> Option<io::Result<Region>> {
    let covered = |file: &str| c.entries.iter().any(|entry| entry.file == file);
    if covered(input) {
        return Some(Ok(Region {
            file: input.to_string(),
            range: None,
        }));
    }

    let (file, bounds) = input.rsplit_once(':')?;
    let invalid = |detail: &str| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "invalid region '{input}': {detail}; expected covered-file or covered-file:a..b"
            ),
        )
    };
    if !covered(file) {
        return Some(Err(invalid(
            "the prefix before the final ':' is not a covered file",
        )));
    }
    let (a, b) = match bounds.split_once("..") {
        Some(bounds) => bounds,
        None => return Some(Err(invalid("a line range must use a..b"))),
    };
    let a = match a.parse::<usize>() {
        Ok(a) if a > 0 => a,
        _ => return Some(Err(invalid("range bounds must be positive integers"))),
    };
    let b = match b.parse::<usize>() {
        Ok(b) if b > 0 => b,
        _ => return Some(Err(invalid("range bounds must be positive integers"))),
    };
    if a > b {
        return Some(Err(invalid("range start must not exceed its end")));
    }
    Some(Ok(Region {
        file: file.to_string(),
        range: Some((a, b)),
    }))
}

/// Whether a segment (line range `[a,b]`) overlaps a region `[ra,rb]`.
fn overlaps(a: usize, b: usize, ra: usize, rb: usize) -> bool {
    a <= rb && ra <= b
}

// ---- show sub-renderers ---------------------------------------------------

fn show_covering(repo: &Repo, c: &Covering) -> io::Result<u8> {
    print_cache_warnings(
        &c.name,
        cache::warm_entries_best_effort(repo, c.entries.iter()),
    );
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
    let (full, cname, entry, content) = match covering {
        Some(cname) => {
            let c = resolve_covering(repo, cname)?;
            let idx = covering::resolve_segment(&c, prefix)?;
            let entry = c.entries[idx].clone();
            (
                entry.hash.clone(),
                Some(cname.to_string()),
                Some(entry.clone()),
                cache::read_live_content(repo, &entry)?,
            )
        }
        None => {
            let resolved = cache::resolve_local_hash(repo, prefix)?;
            let content = cache::read_resolved_content(repo, &resolved)?;
            (resolved.hash, None, None, content)
        }
    };
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
/// a short hash; suffixed `(broken)` when checked and unresolved, or
/// `(unchecked)` when its covering is unavailable.
fn far_label(l: &crate::resolve::ResolvedEnd) -> String {
    let base = l
        .name
        .as_ref()
        .map(|n| format!("@{n}"))
        .or_else(|| l.label.clone())
        .unwrap_or_else(|| short(&l.hash).to_string());
    if !l.checked {
        format!("{base} (unchecked)")
    } else if l.resolved {
        base
    } else {
        format!("{base} (broken)")
    }
}

fn resolved_loc(l: &crate::resolve::ResolvedEnd) -> String {
    if !l.checked {
        "(covering unavailable)".to_string()
    } else {
        l.loc.clone().unwrap_or_else(|| "(unresolved)".to_string())
    }
}

/// Above this many links, `show <connection>` summarizes instead of dumping
/// every link; degree maxima and bounded hubs are always shown when available.
const CONN_LIST_THRESHOLD: usize = 30;

#[derive(Clone, Debug)]
struct ResolvedConnectionLink {
    source_hash: Option<String>,
    target_hash: Option<String>,
    source_display: String,
    target_display: String,
    note: String,
}

fn resolve_connection_endpoint(c: Option<&Covering>, endpoint: &str) -> (Option<String>, String) {
    match c.and_then(|covering| covering::endpoint_to_hash(covering, endpoint)) {
        Some(hash) => {
            let display = c
                .and_then(|covering| covering.entries.iter().find(|entry| entry.hash == hash))
                .map(display_entry)
                .unwrap_or_else(|| endpoint.to_string());
            (Some(hash), display)
        }
        None if c.is_some() => (None, format!("{endpoint} (broken)")),
        None => (None, endpoint.to_string()),
    }
}

fn display_entry(entry: &CoveringEntry) -> String {
    let label = if entry.label.is_empty() {
        "-"
    } else {
        &entry.label
    };
    format!("{label} {}", entry.loc())
}

fn resolve_connection_links(
    source: &Covering,
    target: Option<&Covering>,
    connection: &Connection,
) -> Vec<ResolvedConnectionLink> {
    connection
        .links
        .iter()
        .map(|link| {
            let (source_hash, source_display) = resolve_connection_endpoint(Some(source), &link.a);
            let (target_hash, mut target_display) = resolve_connection_endpoint(target, &link.b);
            if target.is_none() {
                target_display.push_str(" (target unchecked)");
            }
            ResolvedConnectionLink {
                source_hash,
                target_hash,
                source_display,
                target_display,
                note: link.note.clone(),
            }
        })
        .collect()
}

fn ranked_hubs(
    covering: &Covering,
    degree: &std::collections::HashMap<String, usize>,
) -> Vec<(usize, String)> {
    let mut seen = std::collections::HashSet::new();
    let mut hubs = Vec::new();
    for (order, entry) in covering.entries.iter().enumerate() {
        if !seen.insert(entry.hash.as_str()) {
            continue;
        }
        let count = degree.get(&entry.hash).copied().unwrap_or(0);
        if count > 1 {
            hubs.push((count, order, display_entry(entry)));
        }
    }
    hubs.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    hubs.into_iter()
        .map(|(count, _, display)| (count, display))
        .collect()
}

fn append_hubs(lines: &mut Vec<String>, kind: &str, arrow: &str, hubs: &[(usize, String)]) {
    if hubs.is_empty() {
        return;
    }
    lines.push(format!("  {kind}:"));
    for (degree, display) in hubs.iter().take(10) {
        lines.push(format!("    {degree:>4} {arrow} {display}"));
    }
    if hubs.len() > 10 {
        lines.push(format!("    ... {} more {kind} hubs", hubs.len() - 10));
    }
}

const CONNECTION_NOTE_WIDTH: usize = 79;
const CONNECTION_NOTE_INDENT: &str = "    ";

fn wrap_connection_note(note: &str) -> Vec<String> {
    let content_width = CONNECTION_NOTE_WIDTH - CONNECTION_NOTE_INDENT.chars().count();
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in note.split_whitespace() {
        let next_len =
            current.chars().count() + usize::from(!current.is_empty()) + word.chars().count();
        if !current.is_empty() && next_len > content_width {
            lines.push(format!("{CONNECTION_NOTE_INDENT}{current}"));
            current.clear();
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(format!("{CONNECTION_NOTE_INDENT}{current}"));
    }
    lines
}

fn connection_lines(repo: &Repo, k: &Connection, all: bool) -> io::Result<Vec<String>> {
    let src = Covering::read(repo, &k.source_covering)?;
    let tgt = resolve_target_repo(repo, k)
        .ok()
        .and_then(|target| Covering::read(&target, &k.target_covering).ok());
    let resolved = resolve_connection_links(&src, tgt.as_ref(), k);

    // Coverage: distinct resolving endpoints over covering cardinality.
    let src_cov = resolved
        .iter()
        .filter_map(|link| link.source_hash.as_ref())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let src_tot = src.entries.len();
    let (tgt_cov, tgt_tot) = match &tgt {
        Some(t) => (
            resolved
                .iter()
                .filter_map(|link| link.target_hash.as_ref())
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            t.entries.len(),
        ),
        None => (0, 0),
    };

    let mut lines = vec![format!(
        "connection {}   {}",
        k.name,
        connection_route(repo, k)
    )];
    if tgt.is_some() {
        lines.push(format!(
            "  {} links · source {}/{} segments · target {}/{} segments",
            k.links.len(),
            src_cov,
            src_tot,
            tgt_cov,
            tgt_tot
        ));
    } else {
        lines.push(format!(
            "  {} links · source {}/{} segments · target unchecked",
            k.links.len(),
            src_cov,
            src_tot
        ));
    }

    if let Some(target) = &tgt {
        let pairs: std::collections::BTreeSet<(String, String)> = resolved
            .iter()
            .filter_map(|link| {
                Some((
                    link.source_hash.as_ref()?.clone(),
                    link.target_hash.as_ref()?.clone(),
                ))
            })
            .collect();
        let mut out_degree = std::collections::HashMap::new();
        let mut in_degree = std::collections::HashMap::new();
        for (source, target) in pairs {
            *out_degree.entry(source).or_insert(0) += 1;
            *in_degree.entry(target).or_insert(0) += 1;
        }
        let max_out = out_degree.values().copied().max().unwrap_or(0);
        let max_in = in_degree.values().copied().max().unwrap_or(0);
        lines.push(format!("  degree max-out {max_out}, max-in {max_in}"));
        append_hubs(&mut lines, "fan-out", "->", &ranked_hubs(&src, &out_degree));
        append_hubs(&mut lines, "fan-in", "<-", &ranked_hubs(target, &in_degree));
    } else {
        lines.push("  degree unavailable (target covering unavailable)".to_string());
    }

    // Large connections summarize by default and require --all for the full
    // per-link dump.
    if k.links.len() > CONN_LIST_THRESHOLD && !all {
        lines.push(format!(
            "  ({} links total — `covmap show {} --all` to list each)",
            k.links.len(),
            k.name
        ));
        return Ok(lines);
    }

    for link in resolved {
        if link.note.is_empty() {
            lines.push(format!(
                "  {}   ->   {}",
                link.source_display, link.target_display
            ));
        } else {
            lines.push(format!(
                "  {}   ->   {}",
                link.source_display, link.target_display
            ));
            lines.extend(wrap_connection_note(&link.note));
        }
    }
    Ok(lines)
}

fn show_connection(repo: &Repo, k: &Connection, all: bool) -> io::Result<u8> {
    for line in connection_lines(repo, k, all)? {
        println!("{line}");
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
            .map(display_entry)
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
    let tgt = resolve_target_repo(repo, k)
        .ok()
        .and_then(|target| Covering::read(&target, &k.target_covering).ok());
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
    let status = match (&tgt, a_ok, b_ok) {
        (None, true, _) => {
            "target unchecked (covering unavailable; source endpoint resolves)".to_string()
        }
        (None, false, _) => format!(
            "broken source (endpoint '{}' no longer in '{}'; target unchecked)",
            link.a, k.source_covering
        ),
        (Some(_), true, true) => {
            "resolved   (both endpoints present in their coverings)".to_string()
        }
        (Some(_), false, true) => format!(
            "broken (source endpoint '{}' no longer in '{}')",
            link.a, k.source_covering
        ),
        (Some(_), true, false) => format!(
            "broken (target endpoint '{}' no longer in '{}')",
            link.b, k.target_covering
        ),
        (Some(_), false, false) => "broken (neither endpoint resolves)".to_string(),
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

fn show_region(repo: &Repo, c: &Covering, region: &Region) -> io::Result<u8> {
    let mut segs: Vec<&CoveringEntry> = c
        .entries
        .iter()
        .filter(|entry| region.contains(entry))
        .collect();
    segs.sort_by_key(|e| e.sort_key());
    print_cache_warnings(
        &c.name,
        cache::warm_entries_best_effort(repo, segs.iter().copied()),
    );
    let rel = crate::resolve::Relation::build(repo, &c.name, c)?;
    let where_ = match region.range {
        Some((a, b)) => format!("{}:{a}..{b}", region.file),
        None => region.file.clone(),
    };
    println!("region {} {} · {} segment(s)", c.name, where_, segs.len());
    print_region_segs(&rel, &segs);
    Ok(0)
}

fn show_label(repo: &Repo, c: &Covering, label: &str) -> io::Result<u8> {
    let mut segs: Vec<&CoveringEntry> = c.entries.iter().filter(|e| e.label == label).collect();
    segs.sort_by_key(|e| e.sort_key());
    print_cache_warnings(
        &c.name,
        cache::warm_entries_best_effort(repo, segs.iter().copied()),
    );
    let rel = crate::resolve::Relation::build(repo, &c.name, c)?;
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
    let repo = current_repo()?;
    // Decide whether the first positional is a covering; all remaining
    // positionals are regions, or every positional is a HEAD region.
    let (cov_name, region_args, implicit_head) = split_ls_positionals(&repo, &positionals);
    let c = resolve_covering(&repo, &cov_name)?;
    let regions: Vec<Region> = region_args
        .iter()
        .enumerate()
        .map(|(index, arg)| match parse_region(&c, arg) {
            Some(region) => region,
            None if implicit_head && index == 0 && !arg.contains(':') => {
                Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "'{arg}' is neither an existing covering nor a covered file; expected [<covering>] [<covered-file>[:a..b] ...]"
                    ),
                ))
            }
            None => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "invalid region '{arg}': not a covered file or covered-file:a..b range"
                ),
            )),
        })
        .collect::<io::Result<_>>()?;

    // Build the link relation only when a link filter is requested.
    let rel = match link_filter {
        Some(_) => Some(crate::resolve::Relation::build(&repo, &c.name, &c)?),
        None => None,
    };

    let selected: Vec<&CoveringEntry> = c
        .entries
        .iter()
        .filter(|e| match &label_filter {
            Some(label) => &e.label == label,
            None => true,
        })
        .filter(|e| regions.is_empty() || regions.iter().any(|region| region.contains(e)))
        .filter(|e| match (link_filter, rel.as_ref()) {
            (Some(want_linked), Some(rel)) => rel.is_linked(&e.hash) == want_linked,
            _ => true,
        })
        .collect();
    print_cache_warnings(
        &c.name,
        cache::warm_entries_best_effort(&repo, selected.iter().copied()),
    );

    for e in selected {
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

/// Split `ls` positionals into (covering, regions). The first positional is
/// the covering only when it names an existing covering or HEAD; otherwise
/// every positional is a region under HEAD.
fn split_ls_positionals(repo: &Repo, positionals: &[String]) -> (String, Vec<String>, bool) {
    let is_cov = |s: &str| repo.coverings_dir().join(s).is_file() || s == "HEAD";
    match positionals.first() {
        Some(first) if is_cov(first) => (first.clone(), positionals[1..].to_vec(), false),
        _ => ("HEAD".to_string(), positionals.to_vec(), true),
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
        let content = cache::read_live_content(&repo, e)?;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NameImpactSide {
    Source,
    Target,
    Both,
}

impl NameImpactSide {
    fn description(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Target => "target",
            Self::Both => "source and target",
        }
    }
}

#[derive(Clone, Debug)]
struct NameImpact {
    connection: String,
    side: NameImpactSide,
    old_a: String,
    old_b: String,
    note: String,
    repair: NameRepair,
}

#[derive(Clone, Debug)]
enum NameRepair {
    Ready { a: String, b: String },
    BrokenFar,
    Collision { a: String, b: String },
    NoReplacement,
}

/// Find exact `@old` endpoints in connections stored in this repository.
/// A connection stored elsewhere and pointing into this covering is
/// intentionally undiscoverable from local state.
fn discover_name_impacts(
    repo: &Repo,
    covering: &Covering,
    old_name: &str,
    new_name: Option<&str>,
) -> io::Result<Vec<NameImpact>> {
    let old_endpoint = format!("@{old_name}");
    let new_endpoint = new_name.map(|n| format!("@{n}"));
    let mut impacts = Vec::new();

    for connection_name in Connection::list(repo)? {
        let k = Connection::read(repo, &connection_name)?;
        let source_is_covering = k.source_covering == covering.name;
        let target_is_covering = k.target_peer == peer::SELF && k.target_covering == covering.name;
        let affected: Vec<_> = k
            .links
            .iter()
            .filter_map(|link| {
                let source_affected = source_is_covering && link.a == old_endpoint;
                let target_affected = target_is_covering && link.b == old_endpoint;
                (source_affected || target_affected).then_some((
                    link,
                    source_affected,
                    target_affected,
                ))
            })
            .collect();
        if affected.is_empty() {
            continue;
        }

        // Load each covering at most once for this connection. The covering
        // being renamed is already available in its pre-rename state.
        let source_covering = if affected.iter().any(|(_, source, _)| !source) {
            if source_is_covering {
                Some(covering.clone())
            } else {
                Covering::read(repo, &k.source_covering).ok()
            }
        } else {
            None
        };
        let target_covering = if affected.iter().any(|(_, _, target)| !target) {
            if target_is_covering {
                Some(covering.clone())
            } else {
                resolve_target_repo(repo, &k)
                    .ok()
                    .and_then(|target| Covering::read(&target, &k.target_covering).ok())
            }
        } else {
            None
        };

        for (link, source_affected, target_affected) in affected {
            let side = if source_affected {
                if target_affected {
                    NameImpactSide::Both
                } else {
                    NameImpactSide::Source
                }
            } else {
                NameImpactSide::Target
            };
            let repair = match &new_endpoint {
                None => NameRepair::NoReplacement,
                Some(endpoint) => {
                    let a = if source_affected {
                        endpoint.clone()
                    } else {
                        link.a.clone()
                    };
                    let b = if target_affected {
                        endpoint.clone()
                    } else {
                        link.b.clone()
                    };
                    let source_resolves = source_affected
                        || source_covering
                            .as_ref()
                            .is_some_and(|c| covering::endpoint_resolves(c, &link.a));
                    let target_resolves = target_affected
                        || target_covering
                            .as_ref()
                            .is_some_and(|c| covering::endpoint_resolves(c, &link.b));

                    if !source_resolves || !target_resolves {
                        NameRepair::BrokenFar
                    } else if k
                        .links
                        .iter()
                        .any(|existing| existing.a == a && existing.b == b)
                    {
                        NameRepair::Collision { a, b }
                    } else {
                        NameRepair::Ready { a, b }
                    }
                }
            };

            impacts.push(NameImpact {
                connection: k.name.clone(),
                side,
                old_a: link.a.clone(),
                old_b: link.b.clone(),
                note: link.note.clone(),
                repair,
            });
        }
    }
    Ok(impacts)
}

fn posix_shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn link_command(connection: &str, a: &str, b: &str, note: &str) -> String {
    let mut command = format!(
        "covmap link {} {} {}",
        posix_shell_quote(connection),
        posix_shell_quote(a),
        posix_shell_quote(b)
    );
    if !note.is_empty() {
        command.push(' ');
        command.push_str(&posix_shell_quote(note));
    }
    command
}

fn unlink_command(connection: &str, a: &str, b: &str) -> String {
    format!(
        "covmap unlink {} {} {}",
        posix_shell_quote(connection),
        posix_shell_quote(a),
        posix_shell_quote(b)
    )
}

fn render_name_impacts(impacts: &[NameImpact], old_name: &str) -> String {
    if impacts.is_empty() {
        return format!(
            "no connection refs stored in this repository use exact endpoint '@{old_name}'"
        );
    }
    let mut lines = Vec::new();
    let mut link_commands = Vec::new();
    let mut unlink_commands = Vec::new();
    let mut reported_ready = Vec::new();

    for impact in impacts {
        let ready_key = (impact.connection.as_str(), impact.side);
        if !matches!(impact.repair, NameRepair::Ready { .. })
            || !reported_ready.contains(&ready_key)
        {
            lines.push(format!(
                "connection '{}': stored {} endpoint '@{}' is now broken",
                impact.connection,
                impact.side.description(),
                old_name
            ));
        }
        match &impact.repair {
            NameRepair::Ready { a, b } => {
                reported_ready.push(ready_key);
                link_commands.push(link_command(&impact.connection, a, b, &impact.note));
                unlink_commands.push(unlink_command(
                    &impact.connection,
                    &impact.old_a,
                    &impact.old_b,
                ));
            }
            NameRepair::BrokenFar => {
                lines.push(
                    "  no repair block: the unchanged far endpoint is already broken".to_string(),
                );
            }
            NameRepair::Collision { a, b } => {
                lines.push(format!(
                    "  no repair block: replacement pair {} -> {} already exists",
                    a, b
                ));
            }
            NameRepair::NoReplacement => {
                lines.push(format!(
                    "  remove the stale link with: {}",
                    unlink_command(&impact.connection, &impact.old_a, &impact.old_b)
                ));
            }
        }
    }

    if !link_commands.is_empty() {
        lines.push("--- covmap repair begin ---".to_string());
        lines.extend(link_commands);
        lines.extend(unlink_commands);
        lines.push("--- covmap repair end ---".to_string());
    }
    lines.push(
        "note: inbound connections stored in another repository cannot be discovered here"
            .to_string(),
    );
    lines.join("\n")
}

fn print_name_impacts(impacts: &[NameImpact], old_name: &str) {
    println!("{}", render_name_impacts(impacts, old_name));
}

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
    let before = c.clone();
    let new_name = if tag == "-" { None } else { Some(tag) };
    covering::set_name(&mut c, idx, new_name)?;
    let impacts = match old.as_deref() {
        Some(old_name) if new_name != Some(old_name) => {
            discover_name_impacts(&repo, &before, old_name, new_name)?
        }
        _ => Vec::new(),
    };
    c.write(&repo)?;
    let h = covering::short_hash(&c.entries[idx].hash);
    match (old.as_deref(), new_name) {
        (None, Some(n)) => println!("named {h} -> @{n}"),
        (Some(o), Some(n)) if o != n => {
            println!("renamed @{o} -> @{n} in covering '{}'", c.name);
            print_name_impacts(&impacts, o);
        }
        (Some(o), Some(_)) => println!("@{o} (unchanged)"),
        (Some(o), None) => {
            println!("cleared @{o} from {h} in covering '{}'", c.name);
            print_name_impacts(&impacts, o);
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

fn resolve_split_selector(repo: &Repo, raw: &str) -> io::Result<(Covering, covering::SplitScope)> {
    let covering_names = Covering::list(repo)?;
    let (covering_name, selector) = match raw.split_once(':') {
        Some((candidate, rest))
            if candidate == "HEAD" || covering_names.iter().any(|name| name == candidate) =>
        {
            (candidate.to_string(), rest.to_string())
        }
        _ => ("HEAD".to_string(), raw.to_string()),
    };
    let covering = resolve_covering(repo, &covering_name)?;
    let scope = if selector.starts_with('@') || selector.contains(':') {
        covering::SplitScope::Segment(covering::resolve_segment(&covering, &selector)?)
    } else if covering.covered_files().contains(&selector) {
        covering::SplitScope::File(selector)
    } else {
        covering::SplitScope::Segment(covering::resolve_segment(&covering, &selector)?)
    };
    Ok((covering, scope))
}

fn split_impact_warning(
    connection: &str,
    side: NameImpactSide,
    endpoint: &str,
    group: &covering::SplitGroup,
) -> Option<String> {
    if let Some(name) = &group.original.name {
        let named = format!("@{name}");
        if endpoint == named {
            return Some(format!(
                "warning: connection '{connection}': {} name endpoint '{named}' will narrow from {} to {}",
                side.description(),
                group.original.range(),
                group.fragments[0].range()
            ));
        }
    }
    (endpoint == group.original.hash).then(|| {
        format!(
            "warning: connection '{connection}': {} hash endpoint '{}' will become broken after subdividing {}",
            side.description(),
            group.original.hash,
            group.original.range()
        )
    })
}

fn discover_split_warnings(
    repo: &Repo,
    covering: &Covering,
    plan: &covering::SplitPlan,
) -> io::Result<Vec<String>> {
    let mut warnings = Vec::new();
    for connection_name in Connection::list(repo)? {
        let connection = Connection::read(repo, &connection_name)?;
        let source_local = connection.source_covering == covering.name;
        let target_local =
            connection.target_peer == peer::SELF && connection.target_covering == covering.name;
        if !source_local && !target_local {
            continue;
        }
        for link in &connection.links {
            let local_endpoints = [
                (NameImpactSide::Source, source_local, &link.a),
                (NameImpactSide::Target, target_local, &link.b),
            ];
            for (side, is_local, endpoint) in local_endpoints {
                if !is_local {
                    continue;
                }
                for group in &plan.groups {
                    if let Some(warning) =
                        split_impact_warning(&connection.name, side, endpoint, group)
                    {
                        if !warnings.contains(&warning) {
                            warnings.push(warning);
                        }
                    }
                }
            }
        }
    }
    Ok(warnings)
}

fn print_split_warnings(warnings: &[String]) {
    for warning in warnings {
        println!("{warning}");
    }
    if !warnings.is_empty() {
        println!(
            "note: inbound connections stored in another repository cannot be discovered here"
        );
    }
}

fn print_split_plan(plan: &covering::SplitPlan) {
    for group in &plan.groups {
        println!(
            "split {} {} ->",
            covering::short_hash(&group.original.hash),
            group.original.loc()
        );
        for (index, fragment) in group.fragments.iter().enumerate() {
            let mut inherited = Vec::new();
            if index == 0 {
                if let Some(name) = &fragment.name {
                    inherited.push(format!("@{name}"));
                }
                if fragment.message.is_some() {
                    inherited.push("message".to_string());
                }
            }
            let suffix = if inherited.is_empty() {
                String::new()
            } else {
                format!("  inherited {}", inherited.join(", "))
            };
            println!(
                "  {}  {}  {}{}",
                covering::short_hash(&fragment.hash),
                fragment.label,
                fragment.loc(),
                suffix
            );
        }
    }
    for existing in &plan.existing_cuts {
        println!(
            "existing cut: {}:{} (already satisfied)",
            plan.file,
            covering::fmt_pos(*existing)
        );
    }
}

fn cmd_split(args: &[String]) -> io::Result<u8> {
    if args.len() < 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "split <covering>:<segment|file> <cut> [<cut> ...]",
        ));
    }
    let repo = current_repo()?;
    let (mut c, scope) = resolve_split_selector(&repo, &args[0])?;
    let cuts = args[1..]
        .iter()
        .map(|raw| {
            parse_line_col(raw)
                .map(|(line, col)| covering::Pos { line, col })
                .map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!(
                            "split cut '{raw}' is not line[:column]; split accepts cuts only. \
                             Apply metadata afterward with `covmap label`, `covmap name`, or \
                             `covmap annotate`"
                        ),
                    )
                })
        })
        .collect::<io::Result<Vec<_>>>()?;
    let plan = covering::plan_split_many(&repo, &c, scope, &cuts)?;
    let warnings = discover_split_warnings(&repo, &c, &plan)?;
    print_split_warnings(&warnings);
    if plan.is_noop() {
        print_split_plan(&plan);
        println!("split: no changes");
        return Ok(0);
    }
    covering::apply_split_plan(&repo, &mut c, &plan)?;
    c.write(&repo)?;
    print_split_plan(&plan);
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
        let mut reconstruct_error: Option<io::Error> = None;
        for oe in &ordered {
            match cache::read_recorded_content(&repo, oe) {
                Ok(content) => {
                    for piece in content.split_inclusive('\n') {
                        o_lines.push(piece.to_string());
                    }
                }
                Err(error) => {
                    reconstruct_error = Some(error);
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
        if let Some(error) = reconstruct_error {
            eprintln!("{file}: {error} — skipped");
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
                    let target = match cache::read_recorded_content(&repo, it) {
                        Ok(c) => c,
                        Err(error) => {
                            eprintln!("  {who}: {error}");
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

    let tgt_repo = resolve_target_repo(&repo, &k)?;
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
        let tgt_repo = resolve_target_repo(repo, k)?;
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

// ---- peer -----------------------------------------------------------------

fn cmd_peer(args: &[String]) -> io::Result<u8> {
    let verb = args.first().map(String::as_str).unwrap_or("ls");
    let repo = current_repo()?;
    match verb {
        "bind" => {
            if args.len() != 3 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "peer bind <alias> <path>",
                ));
            }
            let target = peer::bind(&repo, &args[1], Path::new(&args[2]))?;
            println!("bound peer '{}' -> {}", args[1], target.root.display());
            Ok(0)
        }
        "ls" => {
            if args.len() != 1 {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "peer ls"));
            }
            let bindings = peer::read_bindings(&repo)?;
            let mut aliases = peer::referenced_aliases(&repo)?;
            aliases.extend(bindings.entries.keys().cloned());
            for alias in aliases {
                if alias == peer::SELF {
                    println!(".\tself\t{}", repo.root.display());
                    continue;
                }
                match bindings.entries.get(&alias) {
                    None => println!("{alias}\tunbound\t-"),
                    Some(path) => match peer::resolve(&repo, &alias) {
                        Ok(target) => {
                            println!("{alias}\tbound\t{}", target.root.display());
                        }
                        Err(error) if error.kind() == io::ErrorKind::NotFound => {
                            println!("{alias}\tmissing\t{path}");
                        }
                        Err(error) => {
                            println!("{alias}\tinvalid\t{path}");
                            eprintln!("peer {alias}: {error}");
                        }
                    },
                }
            }
            for issue in &bindings.issues {
                println!("-\tinvalid\t{issue}");
            }
            Ok(u8::from(!bindings.issues.is_empty()))
        }
        "rename" => {
            if args.len() != 3 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "peer rename <old> <new>",
                ));
            }
            let rewritten = peer::rename(&repo, &args[1], &args[2])?;
            println!(
                "renamed peer '{}' -> '{}': {} connection ref(s) rewritten",
                args[1], args[2], rewritten
            );
            Ok(0)
        }
        "unbind" => {
            if args.len() != 2 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "peer unbind <alias>",
                ));
            }
            if peer::unbind(&repo, &args[1])? {
                println!("unbound peer '{}'", args[1]);
                Ok(0)
            } else {
                eprintln!("(peer '{}' was not bound)", args[1]);
                Ok(1)
            }
        }
        other => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("peer: unknown verb '{other}' (expected bind, ls, rename, or unbind)"),
        )),
    }
}

// ---- connect --------------------------------------------------------------

fn cmd_connect(args: &[String]) -> io::Result<u8> {
    if args.len() < 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "connect <C_A> [<peer>:]<C_B> [<connection-name>]",
        ));
    }
    let src_cov = args[0].clone();
    let target = &args[1];
    let (target_peer, cov_b) = match target.split_once(':') {
        Some((r, c)) => (r.to_string(), c.to_string()),
        None => (".".to_string(), target.to_string()),
    };
    let kname = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| format!("{}__{}", src_cov, cov_b));

    let repo = current_repo()?;
    Covering::read(&repo, &src_cov)?;
    if target_peer != peer::SELF {
        peer::validate_external_alias(&target_peer)?;
    }
    let tgt_repo = peer::resolve(&repo, &target_peer)?;
    Covering::read(&tgt_repo, &cov_b)?;
    let target_peer = if tgt_repo.root == repo.root {
        peer::SELF.to_string()
    } else {
        target_peer
    };

    let k = Connection {
        name: kname.clone(),
        source_covering: src_cov,
        target_peer,
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
    println!(
        "created connection '{kname}': {}",
        connection_route(&repo, &k)
    );
    Ok(0)
}

// ---- disconnect -----------------------------------------------------------

fn cmd_disconnect(args: &[String]) -> io::Result<u8> {
    if args.len() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "disconnect <connection>",
        ));
    }
    let repo = current_repo()?;
    let k = Connection::remove(&repo, &args[0])?;
    println!(
        "disconnected '{}': removed {} link(s) from {}",
        k.name,
        k.links.len(),
        connection_route(&repo, &k)
    );
    if repo.dot().join("report.html").is_file() {
        eprintln!("warning: .covmap/report.html may be stale; regenerate it with `covmap html`");
    }
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
    let tgt_repo = resolve_target_repo(&repo, &k)?;
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
                 done, skip, choose, rewind, peek, status, finished, rm\n  \
                 peek          list pending tasks\n  \
                 peek --all    list the complete frozen worklist with state and notes"
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
    let selected: Vec<&CoveringEntry> = cov
        .entries
        .iter()
        .filter(|entry| {
            label
                .as_deref()
                .map(|value| entry.label == value)
                .unwrap_or(true)
        })
        .collect();
    cache::ensure_recorded_entries(&repo, &selected)?;
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

fn cmd_migrate(_args: &[String]) -> io::Result<u8> {
    let repo = Repo::discover_unchecked(&env::current_dir()?)?;
    let mut version = repo.read_version().unwrap_or_default();
    match version.as_str() {
        "3" => {
            if cleanup_published_migration(&repo)? {
                println!("format 3 migration was already published; removed stale staging state");
            } else {
                println!("already at format 3 — nothing to migrate");
            }
            return Ok(0);
        }
        "1" => {
            migrate_v1_to_v2(&repo)?;
            version = "2".to_string();
        }
        "2" => {}
        other => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("cannot migrate from format '{other}' (expected '1' or '2')"),
            ));
        }
    }
    debug_assert_eq!(version, "2");
    migrate_v2_to_v3(&repo)?;
    println!("run `covmap fsck` to confirm every partition and endpoint resolves");
    Ok(0)
}

fn cleanup_published_migration(repo: &Repo) -> io::Result<bool> {
    let path = migration_dir(repo);
    if path.exists() {
        fs::remove_dir_all(path)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Upgrade format 1 to the legacy format-2 state expected by the portable
/// artifact migration.
fn migrate_v1_to_v2(repo: &Repo) -> io::Result<()> {
    // old segment-hash -> new content-only hash, learned from the coverings
    // (the only place we can recover a segment's content + old label).
    let mut remap: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let objects = repo.objects_dir();
    let mut objects_written = 0usize;

    for cname in Covering::list(repo)? {
        let mut c = Covering::read(repo, &cname)?;
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
        c.write(repo)?;
    }

    // Rewrite bare-hash endpoints in connections. `@name` endpoints and
    // endpoints into a not-yet-migrated target repo are left untouched
    // (the latter resolve again once that repo is migrated).
    let mut conn_eps = 0usize;
    for kname in Connection::list(repo)? {
        let mut k = LegacyConnection::read(repo, &kname)?;
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
        k.write(repo)?;
    }

    // Rewrite frozen task hashes in iterators (the cursor is an index, not
    // a hash, so it needs no rewrite).
    let mut iter_tasks = 0usize;
    for iname in crate::iterator::Iter::list(repo)? {
        let mut it = crate::iterator::Iter::read(repo, &iname)?;
        for t in &mut it.tasks {
            if let Some(n) = remap.get(&t.hash) {
                t.hash = n.clone();
                iter_tasks += 1;
            }
        }
        it.write(repo)?;
    }

    crate::store::atomic_write(&repo.dot().join("version"), "2\n")?;
    println!(
        "migrated to format 2: {} object(s) re-keyed ({} hash change(s)), \
         {} connection endpoint(s) and {} iterator task(s) rewritten",
        objects_written,
        remap.len(),
        conn_eps,
        iter_tasks
    );
    Ok(())
}

fn ensure_default_gitignore(repo: &Repo) -> io::Result<()> {
    let path = repo.dot().join(".gitignore");
    match fs::read_to_string(&path) {
        Ok(existing) => {
            for required in ["/local/", "/objects/", "/iterators/", "/*.html", "*.tmp"] {
                if !existing.lines().any(|line| line.trim() == required) {
                    eprintln!("warning: existing .covmap/.gitignore does not exclude '{required}'");
                }
            }
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            crate::store::atomic_write(&path, crate::repo::DEFAULT_GITIGNORE)
        }
        Err(error) => Err(error),
    }
}

fn legacy_target_is_self(repo: &Repo, target: &str) -> bool {
    let current = fs::canonicalize(&repo.root).unwrap_or_else(|_| repo.root.clone());
    fs::canonicalize(target)
        .ok()
        .is_some_and(|path| path == current)
        || Repo::discover_unchecked(Path::new(target))
            .ok()
            .is_some_and(|target_repo| target_repo.root == current)
}

fn alias_seed(target: &str) -> String {
    let basename = Path::new(target)
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "peer".to_string());
    let mut alias: String = basename
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect();
    while !alias.is_empty()
        && !alias
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
    {
        alias.remove(0);
    }
    if alias.is_empty() || alias == peer::SELF {
        "peer".to_string()
    } else {
        alias
    }
}

fn unique_alias(seed: &str, used: &mut std::collections::BTreeSet<String>) -> String {
    if used.insert(seed.to_string()) {
        return seed.to_string();
    }
    let mut suffix = 2usize;
    loop {
        let candidate = format!("{seed}-{suffix}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        suffix += 1;
    }
}

fn migration_dir(repo: &Repo) -> PathBuf {
    repo.local_dir().join("migrate-2-to-3")
}

fn migration_manifest(repo: &Repo) -> PathBuf {
    migration_dir(repo).join("manifest")
}

fn read_migration_manifest(repo: &Repo) -> io::Result<Option<Vec<(String, String)>>> {
    let path = migration_manifest(repo);
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut lines = raw.lines();
    if lines.next() != Some("ready") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid migration manifest at {}", path.display()),
        ));
    }
    let mut aliases = Vec::new();
    for line in lines {
        let (alias, path) = line.split_once('\t').ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid migration manifest record '{line}'"),
            )
        })?;
        aliases.push((alias.to_string(), path.to_string()));
    }
    Ok(Some(aliases))
}

fn stage_v2_to_v3(repo: &Repo) -> io::Result<Vec<(String, String)>> {
    ensure_default_gitignore(repo)?;
    let root = migration_dir(repo);
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    let staged = root.join("staged");
    fs::create_dir_all(staged.join("coverings"))?;
    fs::create_dir_all(staged.join("connections"))?;
    fs::create_dir_all(staged.join("local"))?;

    let mut legacy = Vec::new();
    let mut targets = std::collections::BTreeSet::new();
    let mut unavailable_targets = std::collections::BTreeSet::new();
    for name in Connection::list(repo)? {
        let connection = LegacyConnection::read(repo, &name)?;
        if !legacy_target_is_self(repo, &connection.target_repo) {
            if connection.target_repo.contains('\t') || connection.target_repo.contains('\n') {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "legacy target path for connection '{}' contains a tab or newline",
                        connection.name
                    ),
                ));
            }
            if Repo::discover_unchecked(Path::new(&connection.target_repo)).is_err() {
                unavailable_targets.insert(connection.target_repo.clone());
            }
            targets.insert(connection.target_repo.clone());
        }
        legacy.push(connection);
    }

    let existing = peer::read_bindings(repo)?;
    if let Some(issue) = existing.issues.first() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid existing peer bindings: {issue}"),
        ));
    }
    let mut bindings = existing.entries;
    let mut used: std::collections::BTreeSet<String> = bindings.keys().cloned().collect();
    let mut by_target = std::collections::BTreeMap::new();
    let mut generated = Vec::new();
    for target in targets {
        let alias = unique_alias(&alias_seed(&target), &mut used);
        bindings.insert(alias.clone(), target.clone());
        by_target.insert(target.clone(), alias.clone());
        if unavailable_targets.contains(&target) {
            eprintln!(
                "warning: legacy target '{}' is unavailable; migrated it as peer '{}'. \
                 If it denotes this repository, run `covmap peer bind {} .` then \
                 `covmap peer rename {} .`",
                target, alias, alias, alias
            );
        }
        generated.push((alias, target));
    }

    for name in Covering::list(repo)? {
        let covering = Covering::read(repo, &name)?;
        crate::store::atomic_write(&staged.join("coverings").join(name), &covering.serialize())?;
    }
    for connection in legacy {
        let target_peer = if legacy_target_is_self(repo, &connection.target_repo) {
            peer::SELF.to_string()
        } else {
            by_target[&connection.target_repo].clone()
        };
        let connection = Connection {
            name: connection.name,
            source_covering: connection.source_covering,
            target_peer,
            target_covering: connection.target_covering,
            links: connection.links,
        };
        crate::store::atomic_write(
            &staged.join("connections").join(&connection.name),
            &connection.serialize(),
        )?;
    }
    crate::store::atomic_write(
        &staged.join("local/peers"),
        &peer::serialize_bindings(&bindings),
    )?;

    let mut manifest = String::from("ready\n");
    for (alias, path) in &generated {
        manifest.push_str(&format!("{alias}\t{path}\n"));
    }
    crate::store::atomic_write(&migration_manifest(repo), &manifest)?;
    Ok(generated)
}

fn publish_staged_directory(from: &Path, to: &Path) -> io::Result<()> {
    let mut names = Vec::new();
    if from.is_dir() {
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                names.push(entry.file_name());
            }
        }
    }
    names.sort();
    for name in names {
        let content = fs::read_to_string(from.join(&name))?;
        crate::store::atomic_write(&to.join(name), &content)?;
    }
    Ok(())
}

fn publish_v2_to_v3(repo: &Repo) -> io::Result<()> {
    let staged = migration_dir(repo).join("staged");
    publish_staged_directory(&staged.join("coverings"), &repo.coverings_dir())?;
    publish_staged_directory(&staged.join("connections"), &repo.connections_dir())?;
    let peers = fs::read_to_string(staged.join("local/peers"))?;
    crate::store::atomic_write(&repo.peers_file(), &peers)?;
    crate::store::atomic_write(&repo.dot().join("version"), "3\n")?;
    fs::remove_dir_all(migration_dir(repo))?;
    Ok(())
}

fn migrate_v2_to_v3(repo: &Repo) -> io::Result<()> {
    let generated = match read_migration_manifest(repo)? {
        Some(generated) => {
            println!("resuming staged format 2 -> 3 migration");
            generated
        }
        None => stage_v2_to_v3(repo)?,
    };
    publish_v2_to_v3(repo)?;
    println!("migrated to format 3: portable peer refs and canonical Git-trackable artifacts");
    for (alias, path) in generated {
        println!("  peer {alias} -> {path}");
    }
    println!(
        "note: ignore rules do not untrack indexed files; if needed run:\n  \
         git rm -r --cached --ignore-unmatch .covmap/objects .covmap/iterators .covmap/local"
    );
    Ok(())
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
    let resolved = cache::resolve_local_hash(&repo, prefix)?;
    let content = cache::read_resolved_content(&repo, &resolved)?;
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
        let tgt = resolve_target_repo(&repo, &k)
            .ok()
            .and_then(|target| Covering::read(&target, &k.target_covering).ok());
        if tgt.is_none() && k.target_peer != peer::SELF {
            eprintln!(
                "warning: peer '{}' is unavailable; target fields are empty",
                k.target_peer
            );
        }
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
        None => cache::resolve_local_hash(&repo, &prefix)?.hash,
    };
    println!("{full}");
    Ok(0)
}

fn cmd_fsck(args: &[String]) -> io::Result<u8> {
    let mut broken_only = false;
    let mut offline = false;
    let mut hydrate = false;
    for a in args {
        match a.as_str() {
            "--broken-links" => broken_only = true,
            "--offline" => offline = true,
            "--hydrate" => hydrate = true,
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("fsck: unexpected argument '{other}'"),
                ))
            }
        }
    }
    if hydrate && !offline {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "fsck --hydrate requires --offline",
        ));
    }
    if hydrate && broken_only {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "fsck --hydrate cannot be combined with --broken-links",
        ));
    }

    let repo = current_repo()?;
    let mut errs = 0usize;
    let bindings = peer::read_bindings(&repo)?;
    for issue in &bindings.issues {
        eprintln!("fsck: peer bindings: {issue}");
        errs += 1;
    }
    if let Some((old, new)) = peer::read_rename_journal(&repo)? {
        eprintln!(
            "fsck: unfinished peer rename {old} -> {new}; \
             run `covmap peer rename {old} {new}` to resume"
        );
        errs += 1;
    }
    if !broken_only {
        for cn in Covering::list(&repo)? {
            let c = Covering::read(&repo, &cn)?;
            for issue in covering::partition_issues(&c) {
                eprintln!("fsck: covering {cn} {issue}");
                errs += 1;
            }

            let mut by_file: BTreeMap<&str, Vec<&CoveringEntry>> = BTreeMap::new();
            for entry in &c.entries {
                by_file.entry(entry.file.as_str()).or_default().push(entry);
            }
            for (file, entries) in by_file {
                let lines = match cache::source_lines(&repo, file) {
                    Ok(lines) => Some(lines),
                    Err(error) => {
                        eprintln!("fsck: covering {cn}: {error}");
                        errs += 1;
                        None
                    }
                };
                for entry in entries {
                    let object = segment::read_object(&repo.objects_dir(), &entry.hash);
                    let missing =
                        matches!(&object, Err(error) if error.kind() == io::ErrorKind::NotFound);

                    if let Some(lines) = &lines {
                        let source_result = if hydrate && missing {
                            cache::hydrate_entry_from_lines(&repo, entry, lines).map(|_| ())
                        } else {
                            cache::source_content_from_lines(lines, entry).map(|_| ())
                        };
                        if let Err(error) = source_result {
                            if hydrate && missing {
                                eprintln!(
                                    "fsck: covering {cn} {}: cannot hydrate: {error}",
                                    entry.loc()
                                );
                            } else {
                                eprintln!("fsck: covering {cn} {}: {error}", entry.loc());
                            }
                            errs += 1;
                        }
                    }

                    match object {
                        Ok(_) => {}
                        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                        Err(error) => {
                            eprintln!("fsck: covering {cn} {}: {error}", entry.loc());
                            errs += 1;
                        }
                    }
                }
            }
        }
    }

    let mut unchecked = 0usize;
    for kn in Connection::list(&repo)? {
        let k = Connection::read(&repo, &kn)?;
        let src = Covering::read(&repo, &k.source_covering)?;
        if k.target_peer != peer::SELF {
            if let Ok(target) = peer::resolve(&repo, &k.target_peer) {
                if target.root == repo.root {
                    eprintln!(
                        "fsck: connection {kn} uses noncanonical self peer '{}'; \
                         run `covmap peer rename {} .`",
                        k.target_peer, k.target_peer
                    );
                    errs += 1;
                }
            }
        }
        if offline && k.target_peer != peer::SELF {
            for link in &k.links {
                if !covering::endpoint_resolves(&src, &link.a) {
                    println!("{}\t{}\t{}", kn, link.a, link.b);
                    errs += 1;
                }
            }
            eprintln!(
                "fsck: connection {kn} target unchecked in --offline mode: {}:{}",
                k.target_peer, k.target_covering
            );
            unchecked += 1;
            continue;
        }
        let tgt_repo = resolve_target_repo(&repo, &k)?;
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
            if unchecked == 0 {
                println!("fsck: ok");
            } else {
                println!("fsck: ok ({unchecked} external connection(s) unchecked)");
            }
        } else {
            eprintln!("fsck: {errs} issue(s)");
        }
    }
    Ok(if errs > 0 { 1 } else { 0 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
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

    fn named_entry(hash: &str, line: usize, name: &str) -> CoveringEntry {
        CoveringEntry {
            hash: hash.to_string(),
            file: "file.txt".to_string(),
            a: line,
            a_col: None,
            b: line,
            b_col: None,
            label: "mapped".to_string(),
            name: Some(name.to_string()),
            message: None,
        }
    }

    fn split_entry(hash: &str, file: &str, end_line: usize, name: &str) -> CoveringEntry {
        CoveringEntry {
            hash: hash.to_string(),
            file: file.to_string(),
            a: 1,
            a_col: None,
            b: end_line,
            b_col: None,
            label: "unknown".to_string(),
            name: Some(name.to_string()),
            message: None,
        }
    }

    fn initialize_test_repo(repo: &Repo) {
        fs::create_dir_all(repo.objects_dir()).unwrap();
        fs::create_dir_all(repo.coverings_dir()).unwrap();
        fs::create_dir_all(repo.connections_dir()).unwrap();
        fs::write(repo.head_file(), "main\n").unwrap();
    }

    #[test]
    fn drift_check_uses_covering_hashes_without_cache_objects() {
        let lines = vec!["alpha\n".to_string(), "beta\n".to_string()];
        let entry = CoveringEntry {
            hash: segment::hash_of("alpha\nbeta\n"),
            file: "file.txt".to_string(),
            a: 1,
            a_col: None,
            b: 2,
            b_col: None,
            label: "unknown".to_string(),
            name: None,
            message: None,
        };

        assert!(file_matches_segments(&lines, &[&entry]));

        let changed = vec!["changed\n".to_string(), "beta\n".to_string()];
        assert!(!file_matches_segments(&changed, &[&entry]));
    }

    #[test]
    fn connection_route_marks_same_repository_targets() {
        let repo = temp_repo("route");
        let mut k = Connection {
            name: "map".to_string(),
            source_covering: "left".to_string(),
            target_peer: ".".to_string(),
            target_covering: "right".to_string(),
            links: Vec::new(),
        };

        assert_eq!(connection_route(&repo, &k), "left -> right (self-map)");
        k.target_peer = "elsewhere".to_string();
        assert_eq!(
            connection_route(&repo, &k),
            "left -> elsewhere:right (unavailable: peer 'elsewhere' is not bound; \
             run covmap peer bind elsewhere <path>)"
        );
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn region_parser_prefers_exact_colon_filenames_and_validates_ranges() {
        let covering = Covering {
            name: "main".to_string(),
            entries: vec![
                CoveringEntry {
                    hash: "1111111111111111".to_string(),
                    file: "api:v1.md".to_string(),
                    a: 1,
                    a_col: None,
                    b: 10,
                    b_col: None,
                    label: "mapped".to_string(),
                    name: None,
                    message: None,
                },
                CoveringEntry {
                    hash: "2222222222222222".to_string(),
                    file: "api:v1.md".to_string(),
                    a: 11,
                    a_col: None,
                    b: 20,
                    b_col: None,
                    label: "reviewed".to_string(),
                    name: None,
                    message: None,
                },
                CoveringEntry {
                    hash: "3333333333333333".to_string(),
                    file: "other.md".to_string(),
                    a: 1,
                    a_col: None,
                    b: 20,
                    b_col: None,
                    label: "mapped".to_string(),
                    name: None,
                    message: None,
                },
            ],
        };

        assert_eq!(
            parse_region(&covering, "api:v1.md").unwrap().unwrap(),
            Region {
                file: "api:v1.md".to_string(),
                range: None,
            }
        );
        assert_eq!(
            parse_region(&covering, "api:v1.md:4..9").unwrap().unwrap(),
            Region {
                file: "api:v1.md".to_string(),
                range: Some((4, 9)),
            }
        );
        let selected = parse_region(&covering, "api:v1.md:4..12").unwrap().unwrap();
        assert!(selected.contains(&covering.entries[0]));
        assert!(selected.contains(&covering.entries[1]));
        assert!(!selected.contains(&covering.entries[2]));
        let first_only = parse_region(&covering, "api:v1.md:1..3").unwrap().unwrap();
        assert!(first_only.contains(&covering.entries[0]));
        assert!(!first_only.contains(&covering.entries[1]));
        for invalid in ["api:v1.md:0..3", "api:v1.md:8..3", "missing.md:1..3"] {
            let error = parse_region(&covering, invalid)
                .unwrap()
                .unwrap_err()
                .to_string();
            assert!(error.contains(invalid));
        }
        assert!(parse_region(&covering, "api:v1.md:5")
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("a line range must use a..b"));
        assert!(parse_region(&covering, "@segment").is_none());
    }

    #[test]
    fn ls_positionals_treat_only_an_existing_first_token_as_covering() {
        let repo = temp_repo("ls-positionals");
        initialize_test_repo(&repo);
        fs::write(repo.coverings_dir().join("review"), "# covered=x\n").unwrap();

        let two_regions = vec!["one.md".to_string(), "two.md:4..8".to_string()];
        assert_eq!(
            split_ls_positionals(&repo, &two_regions),
            ("HEAD".to_string(), two_regions, true)
        );

        let explicit = vec![
            "review".to_string(),
            "one.md".to_string(),
            "two.md".to_string(),
        ];
        assert_eq!(
            split_ls_positionals(&repo, &explicit),
            (
                "review".to_string(),
                vec!["one.md".to_string(), "two.md".to_string()],
                false
            )
        );
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn connection_degree_combines_hash_and_name_endpoint_spellings() {
        let repo = temp_repo("degree-identity");
        initialize_test_repo(&repo);
        let source = Covering {
            name: "main".to_string(),
            entries: vec![
                named_entry("1111111111111111", 1, "one"),
                named_entry("2222222222222222", 2, "two"),
                named_entry("3333333333333333", 3, "three"),
            ],
        };
        let target = Covering {
            name: "target".to_string(),
            entries: vec![CoveringEntry {
                hash: "aaaaaaaaaaaaaaaa".to_string(),
                file: "target.txt".to_string(),
                a: 1,
                a_col: None,
                b: 1,
                b_col: None,
                label: "mapped".to_string(),
                name: Some("hub".to_string()),
                message: None,
            }],
        };
        source.write(&repo).unwrap();
        target.write(&repo).unwrap();
        let connection = Connection {
            name: "map".to_string(),
            source_covering: "main".to_string(),
            target_peer: ".".to_string(),
            target_covering: "target".to_string(),
            links: vec![
                Link {
                    a: "@one".to_string(),
                    b: "aaaaaaaaaaaaaaaa".to_string(),
                    note: String::new(),
                },
                Link {
                    a: "@two".to_string(),
                    b: "@hub".to_string(),
                    note: String::new(),
                },
                Link {
                    a: "@three".to_string(),
                    b: "@hub".to_string(),
                    note: String::new(),
                },
            ],
        };

        let output = connection_lines(&repo, &connection, false)
            .unwrap()
            .join("\n");

        assert!(output.contains("target 1/1 segments"));
        assert!(output.contains("degree max-out 1, max-in 3"));
        assert_eq!(output.matches("mapped target.txt:1..1").count(), 4);
        assert_eq!(output.matches("fan-in:").count(), 1);
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn hub_rendering_is_capped_and_reports_the_omitted_count() {
        let covering = Covering {
            name: "target".to_string(),
            entries: (0..12)
                .map(|i| CoveringEntry {
                    hash: format!("{i:016x}"),
                    file: "target.txt".to_string(),
                    a: i + 1,
                    a_col: None,
                    b: i + 1,
                    b_col: None,
                    label: format!("target-{i}"),
                    name: None,
                    message: None,
                })
                .collect(),
        };
        let degree = covering
            .entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let count = if i == 2 { 19 } else { 20 - i };
                (entry.hash.clone(), count)
            })
            .collect();
        let hubs = ranked_hubs(&covering, &degree);
        let mut lines = Vec::new();

        append_hubs(&mut lines, "fan-in", "<-", &hubs);

        assert_eq!(lines.len(), 12);
        assert_eq!(lines[0], "  fan-in:");
        assert!(lines[1].contains("20 <- target-0"));
        assert!(lines[2].contains("19 <- target-1"));
        assert!(lines[3].contains("19 <- target-2"));
        assert_eq!(lines[11], "    ... 2 more fan-in hubs");
    }

    #[test]
    fn shared_content_hash_prints_one_hub_row() {
        let repo = temp_repo("shared-hub-hash");
        initialize_test_repo(&repo);
        let source = Covering {
            name: "main".to_string(),
            entries: vec![
                named_entry("1111111111111111", 1, "one"),
                named_entry("2222222222222222", 2, "two"),
            ],
        };
        let target = Covering {
            name: "target".to_string(),
            entries: vec![
                CoveringEntry {
                    hash: "aaaaaaaaaaaaaaaa".to_string(),
                    file: "x.txt".to_string(),
                    a: 1,
                    a_col: None,
                    b: 1,
                    b_col: None,
                    label: "first".to_string(),
                    name: None,
                    message: None,
                },
                CoveringEntry {
                    hash: "aaaaaaaaaaaaaaaa".to_string(),
                    file: "y.txt".to_string(),
                    a: 1,
                    a_col: None,
                    b: 1,
                    b_col: None,
                    label: "second".to_string(),
                    name: None,
                    message: None,
                },
            ],
        };
        source.write(&repo).unwrap();
        target.write(&repo).unwrap();
        let connection = Connection {
            name: "map".to_string(),
            source_covering: "main".to_string(),
            target_peer: ".".to_string(),
            target_covering: "target".to_string(),
            links: vec![
                Link {
                    a: "@one".to_string(),
                    b: "aaaaaaaaaaaaaaaa".to_string(),
                    note: String::new(),
                },
                Link {
                    a: "@two".to_string(),
                    b: "aaaaaaaaaaaaaaaa".to_string(),
                    note: String::new(),
                },
            ],
        };

        let output = connection_lines(&repo, &connection, false)
            .unwrap()
            .join("\n");

        assert!(output.contains("degree max-out 1, max-in 2"));
        assert_eq!(output.matches("2 <- first x.txt:1..1").count(), 1);
        assert!(!output.contains("2 <- second y.txt:1..1"));
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn degree_excludes_broken_links_and_reports_fan_out() {
        let repo = temp_repo("degree-broken-fanout");
        initialize_test_repo(&repo);
        let source = Covering {
            name: "main".to_string(),
            entries: vec![named_entry("1111111111111111", 1, "source")],
        };
        let target = Covering {
            name: "target".to_string(),
            entries: vec![
                named_entry("aaaaaaaaaaaaaaaa", 1, "one"),
                named_entry("bbbbbbbbbbbbbbbb", 2, "two"),
            ],
        };
        source.write(&repo).unwrap();
        target.write(&repo).unwrap();
        let connection = Connection {
            name: "map".to_string(),
            source_covering: "main".to_string(),
            target_peer: ".".to_string(),
            target_covering: "target".to_string(),
            links: vec![
                Link {
                    a: "@source".to_string(),
                    b: "@one".to_string(),
                    note: String::new(),
                },
                Link {
                    a: "@source".to_string(),
                    b: "@two".to_string(),
                    note: String::new(),
                },
                Link {
                    a: "@source".to_string(),
                    b: "@missing".to_string(),
                    note: String::new(),
                },
            ],
        };

        let output = connection_lines(&repo, &connection, false)
            .unwrap()
            .join("\n");

        assert!(output.contains("degree max-out 2, max-in 1"));
        assert!(output.contains("fan-out:"));
        assert!(output.contains("2 -> mapped file.txt:1..1"));
        assert!(output.contains("@missing (broken)"));
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn unavailable_target_reports_unknown_degree_instead_of_zero() {
        let repo = temp_repo("degree-unavailable");
        initialize_test_repo(&repo);
        Covering {
            name: "main".to_string(),
            entries: vec![named_entry("1111111111111111", 1, "source")],
        }
        .write(&repo)
        .unwrap();
        let connection = Connection {
            name: "map".to_string(),
            source_covering: "main".to_string(),
            target_peer: "missing".to_string(),
            target_covering: "target".to_string(),
            links: vec![Link {
                a: "@source".to_string(),
                b: "@target".to_string(),
                note: String::new(),
            }],
        };

        let output = connection_lines(&repo, &connection, false)
            .unwrap()
            .join("\n");

        assert!(output.contains("degree unavailable (target covering unavailable)"));
        assert!(output.contains("target unchecked"));
        assert!(output.contains("@target (target unchecked)"));
        assert!(!output.contains("@target (broken)"));
        assert!(!output.contains("degree max-out"));
        assert!(!output.contains("fan-in:"));
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn connection_note_wrapping_is_deterministic_and_bounded() {
        let note = "The parser normalizes escaped fields before the reader validates the decoded value, preserving the mapping rationale without stretching the resolved endpoint row across the terminal.";

        let lines = wrap_connection_note(note);

        assert!(lines.len() > 1);
        assert!(lines
            .iter()
            .all(|line| line.chars().count() <= CONNECTION_NOTE_WIDTH));
        assert_eq!(
            lines
                .iter()
                .flat_map(|line| line.split_whitespace())
                .collect::<Vec<_>>()
                .join(" "),
            note
        );
    }

    #[test]
    fn large_connection_wraps_notes_only_when_all_is_requested() {
        let repo = temp_repo("connection-all-notes");
        initialize_test_repo(&repo);
        let source = Covering {
            name: "main".to_string(),
            entries: (0..31)
                .map(|i| named_entry(&format!("{:016x}", i + 1), i + 1, &format!("source-{i}")))
                .collect(),
        };
        let target = Covering {
            name: "target".to_string(),
            entries: vec![named_entry("aaaaaaaaaaaaaaaa", 1, "target")],
        };
        source.write(&repo).unwrap();
        target.write(&repo).unwrap();
        let note = "This long note must appear as wrapped porcelain only when the caller explicitly expands the large connection with show --all.";
        let connection = Connection {
            name: "map".to_string(),
            source_covering: "main".to_string(),
            target_peer: ".".to_string(),
            target_covering: "target".to_string(),
            links: (0..31)
                .map(|i| Link {
                    a: format!("@source-{i}"),
                    b: "@target".to_string(),
                    note: if i == 0 { note } else { "" }.to_string(),
                })
                .collect(),
        };

        let summary = connection_lines(&repo, &connection, false)
            .unwrap()
            .join("\n");
        let expanded = connection_lines(&repo, &connection, true)
            .unwrap()
            .join("\n");

        assert!(summary.contains("31 links total"));
        assert!(!summary.contains("This long note"));
        assert!(expanded.contains("    This long note must appear as wrapped porcelain"));
        assert!(!expanded.contains("31 links total"));
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn split_selector_prefers_exact_files_and_accepts_all_segment_forms() {
        let repo = temp_repo("split-selector");
        initialize_test_repo(&repo);
        let covering = Covering {
            name: "main".into(),
            entries: vec![
                split_entry("abcdef0123456789", "ab", 1, "hex-file"),
                split_entry("1234567890abcdef", "other.txt", 2, "other"),
            ],
        };
        covering.write(&repo).unwrap();

        let (_, file) = resolve_split_selector(&repo, "ab").unwrap();
        assert!(matches!(file, covering::SplitScope::File(ref f) if f == "ab"));
        let (_, named) = resolve_split_selector(&repo, "main:@other").unwrap();
        assert!(matches!(named, covering::SplitScope::Segment(1)));
        let (_, positioned) = resolve_split_selector(&repo, "other.txt:2").unwrap();
        assert!(matches!(positioned, covering::SplitScope::Segment(1)));
        let (_, hashed) = resolve_split_selector(&repo, "main:1234").unwrap();
        assert!(matches!(hashed, covering::SplitScope::Segment(1)));

        let malformed = resolve_split_selector(&repo, "main:other.txt:nope").unwrap_err();
        assert!(malformed.to_string().contains("not a valid file:line"));
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn split_impact_discovery_covers_name_hash_source_and_target() {
        let repo = temp_repo("split-impacts");
        initialize_test_repo(&repo);
        let content = "a\nb\n";
        let segment = Segment {
            file: "file.txt".into(),
            a: 1,
            b: 2,
            label: "mapped".into(),
            content: content.into(),
        };
        let hash = segment::write_object(&repo.objects_dir(), &segment).unwrap();
        let covering = Covering {
            name: "main".into(),
            entries: vec![split_entry(&hash, "file.txt", 2, "whole")],
        };
        covering.write(&repo).unwrap();
        Connection {
            name: "self".into(),
            source_covering: "main".into(),
            target_peer: ".".to_string(),
            target_covering: "main".into(),
            links: vec![
                Link {
                    a: "@whole".into(),
                    b: "@whole".into(),
                    note: String::new(),
                },
                Link {
                    a: hash.clone(),
                    b: "@whole".into(),
                    note: String::new(),
                },
            ],
        }
        .write(&repo)
        .unwrap();
        let plan = covering::plan_split_many(
            &repo,
            &covering,
            covering::SplitScope::Segment(0),
            &[covering::Pos { line: 2, col: None }],
        )
        .unwrap();

        let warnings = discover_split_warnings(&repo, &covering, &plan).unwrap();

        assert_eq!(warnings.len(), 3);
        assert_eq!(
            warnings
                .iter()
                .filter(|warning| warning.contains("name endpoint"))
                .count(),
            2
        );
        assert_eq!(
            warnings
                .iter()
                .filter(|warning| warning.contains("hash endpoint"))
                .count(),
            1
        );
        assert!(warnings
            .iter()
            .any(|warning| warning.contains("source hash endpoint")));
        assert!(warnings.iter().any(
            |warning| warning.contains("target name endpoint") && warning.ends_with("to 1..1")
        ));
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn shell_quote_round_trips_without_evaluating_note_syntax() {
        let value = "spaces ' \"$HOME\" `printf executed` $(printf executed) ; * ? !";
        let script = format!("printf '%s' {}", posix_shell_quote(value));
        let output = Command::new("sh").args(["-c", &script]).output().unwrap();

        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap(), value);
    }

    #[test]
    fn rename_discovery_handles_source_target_and_both_sides() {
        let repo = temp_repo("rename-sides");
        let covering = Covering {
            name: "main".to_string(),
            entries: vec![
                named_entry("1111111111111111", 1, "old"),
                named_entry("2222222222222222", 2, "other"),
            ],
        };
        covering.write(&repo).unwrap();
        Connection {
            name: "self".to_string(),
            source_covering: "main".to_string(),
            target_peer: ".".to_string(),
            target_covering: "main".to_string(),
            links: vec![
                Link {
                    a: "@old".to_string(),
                    b: "@other".to_string(),
                    note: "source".to_string(),
                },
                Link {
                    a: "@other".to_string(),
                    b: "@old".to_string(),
                    note: "target".to_string(),
                },
                Link {
                    a: "@old".to_string(),
                    b: "@old".to_string(),
                    note: "both".to_string(),
                },
            ],
        }
        .write(&repo)
        .unwrap();

        let impacts = discover_name_impacts(&repo, &covering, "old", Some("new")).unwrap();

        assert_eq!(impacts.len(), 3);
        assert!(impacts
            .iter()
            .any(|impact| impact.side == NameImpactSide::Source));
        assert!(impacts
            .iter()
            .any(|impact| impact.side == NameImpactSide::Target));
        let both = impacts
            .iter()
            .find(|impact| impact.side == NameImpactSide::Both)
            .unwrap();
        assert!(matches!(
            &both.repair,
            NameRepair::Ready { a, b } if a == "@new" && b == "@new"
        ));
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn rename_discovery_blocks_collisions_and_broken_far_endpoints() {
        let repo = temp_repo("rename-conflicts");
        let covering = Covering {
            name: "main".to_string(),
            entries: vec![
                named_entry("1111111111111111", 1, "old"),
                named_entry("2222222222222222", 2, "other"),
            ],
        };
        covering.write(&repo).unwrap();
        Connection {
            name: "self".to_string(),
            source_covering: "main".to_string(),
            target_peer: ".".to_string(),
            target_covering: "main".to_string(),
            links: vec![
                Link {
                    a: "@old".to_string(),
                    b: "@other".to_string(),
                    note: String::new(),
                },
                Link {
                    a: "@new".to_string(),
                    b: "@other".to_string(),
                    note: String::new(),
                },
                Link {
                    a: "@old".to_string(),
                    b: "@missing".to_string(),
                    note: String::new(),
                },
            ],
        }
        .write(&repo)
        .unwrap();

        let impacts = discover_name_impacts(&repo, &covering, "old", Some("new")).unwrap();

        assert_eq!(impacts.len(), 2);
        assert!(impacts.iter().any(|impact| impact.old_b == "@other"
            && matches!(impact.repair, NameRepair::Collision { .. })));
        assert!(impacts
            .iter()
            .any(|impact| impact.old_b == "@missing"
                && matches!(impact.repair, NameRepair::BrokenFar)));
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn rename_rendering_groups_fanout_into_one_executable_block() {
        let no_impacts = render_name_impacts(&[], "unused");
        assert!(!no_impacts.contains("inbound connections"));

        let impacts = vec![
            NameImpact {
                connection: "map".to_string(),
                side: NameImpactSide::Source,
                old_a: "@old".to_string(),
                old_b: "@one".to_string(),
                note: "first".to_string(),
                repair: NameRepair::Ready {
                    a: "@new".to_string(),
                    b: "@one".to_string(),
                },
            },
            NameImpact {
                connection: "map".to_string(),
                side: NameImpactSide::Source,
                old_a: "@old".to_string(),
                old_b: "@two".to_string(),
                note: "second".to_string(),
                repair: NameRepair::Ready {
                    a: "@new".to_string(),
                    b: "@two".to_string(),
                },
            },
        ];

        let rendered = render_name_impacts(&impacts, "old");
        assert_eq!(rendered.matches("--- covmap repair begin ---").count(), 1);
        assert_eq!(rendered.matches("--- covmap repair end ---").count(), 1);
        assert_eq!(
            rendered
                .matches("stored source endpoint '@old' is now broken")
                .count(),
            1
        );

        let script = rendered
            .lines()
            .skip_while(|line| *line != "--- covmap repair begin ---")
            .skip(1)
            .take_while(|line| *line != "--- covmap repair end ---")
            .collect::<Vec<_>>()
            .join("\n");
        let commands: Vec<_> = script.lines().collect();
        assert_eq!(commands.len(), 4);
        assert!(commands[..2]
            .iter()
            .all(|line| line.starts_with("covmap link ")));
        assert!(commands[2..]
            .iter()
            .all(|line| line.starts_with("covmap unlink ")));

        let shell = format!("covmap() {{ :; }}\n{script}");
        let status = Command::new("sh")
            .args(["-eu", "-c", &shell])
            .status()
            .unwrap();
        assert!(status.success());
    }

    fn format2_repo(tag: &str) -> Repo {
        let repo = temp_repo(tag);
        initialize_test_repo(&repo);
        fs::create_dir_all(repo.iterators_dir()).unwrap();
        fs::write(repo.dot().join("version"), "2\n").unwrap();
        fs::write(repo.root.join("file.txt"), "alpha\n").unwrap();
        let entry = CoveringEntry {
            hash: segment::hash_of("alpha\n"),
            file: "file.txt".to_string(),
            a: 1,
            a_col: None,
            b: 1,
            b_col: None,
            label: "mapped".to_string(),
            name: Some("alpha".to_string()),
            message: None,
        };
        Covering {
            name: "main".to_string(),
            entries: vec![entry],
        }
        .write(&repo)
        .unwrap();
        repo
    }

    #[test]
    fn format2_migration_is_resumable_and_canonicalizes_self() {
        let repo = format2_repo("migrate-resume");
        LegacyConnection {
            name: "self".to_string(),
            source_covering: "main".to_string(),
            target_repo: repo.root.to_string_lossy().to_string(),
            target_covering: "main".to_string(),
            links: vec![Link {
                a: "@alpha".to_string(),
                b: "@alpha".to_string(),
                note: "self".to_string(),
            }],
        }
        .write(&repo)
        .unwrap();

        stage_v2_to_v3(&repo).unwrap();
        let staged =
            fs::read_to_string(migration_dir(&repo).join("staged/connections/self")).unwrap();
        fs::write(repo.connections_dir().join("self"), staged).unwrap();
        migrate_v2_to_v3(&repo).unwrap();

        assert_eq!(repo.read_version().unwrap(), "3");
        assert_eq!(Connection::read(&repo, "self").unwrap().target_peer, ".");
        assert_eq!(
            fs::read_to_string(repo.dot().join(".gitignore")).unwrap(),
            crate::repo::DEFAULT_GITIGNORE
        );
        assert!(!migration_dir(&repo).exists());
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn format2_migration_assigns_deterministic_alias_collisions() {
        let repo = format2_repo("migrate-aliases");
        for (name, target) in [("first", "/tmp/a/api"), ("second", "/tmp/b/api")] {
            LegacyConnection {
                name: name.to_string(),
                source_covering: "main".to_string(),
                target_repo: target.to_string(),
                target_covering: "main".to_string(),
                links: Vec::new(),
            }
            .write(&repo)
            .unwrap();
        }

        migrate_v2_to_v3(&repo).unwrap();

        assert_eq!(Connection::read(&repo, "first").unwrap().target_peer, "api");
        assert_eq!(
            Connection::read(&repo, "second").unwrap().target_peer,
            "api-2"
        );
        let bindings = peer::read_bindings(&repo).unwrap();
        assert_eq!(
            bindings.entries.get("api").map(String::as_str),
            Some("/tmp/a/api")
        );
        assert_eq!(
            bindings.entries.get("api-2").map(String::as_str),
            Some("/tmp/b/api")
        );
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn format2_migration_preserves_custom_gitignore() {
        let repo = format2_repo("migrate-ignore");
        let custom = "# project policy\n/objects/\n";
        fs::write(repo.dot().join(".gitignore"), custom).unwrap();

        migrate_v2_to_v3(&repo).unwrap();

        assert_eq!(
            fs::read_to_string(repo.dot().join(".gitignore")).unwrap(),
            custom
        );
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn format1_migration_chains_through_portable_format3() {
        use sha2::{Digest, Sha256};

        let repo = temp_repo("migrate-chain");
        initialize_test_repo(&repo);
        fs::create_dir_all(repo.iterators_dir()).unwrap();
        fs::write(repo.dot().join("version"), "1\n").unwrap();
        fs::write(repo.root.join("file.txt"), "alpha\n").unwrap();
        let label = "mapped";
        let content = "alpha\n";
        let mut digest = Sha256::new();
        digest.update(label.as_bytes());
        digest.update([0]);
        digest.update(content.as_bytes());
        let old_hash = digest
            .finalize()
            .iter()
            .take(HASH_LEN / 2)
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let old_path = segment::object_path(&repo.objects_dir(), &old_hash);
        fs::create_dir_all(old_path.parent().unwrap()).unwrap();
        fs::write(&old_path, format!("{label}\0{content}")).unwrap();
        Covering {
            name: "main".to_string(),
            entries: vec![CoveringEntry {
                hash: old_hash.clone(),
                file: "file.txt".to_string(),
                a: 1,
                a_col: None,
                b: 1,
                b_col: None,
                label: label.to_string(),
                name: None,
                message: None,
            }],
        }
        .write(&repo)
        .unwrap();
        LegacyConnection {
            name: "self".to_string(),
            source_covering: "main".to_string(),
            target_repo: repo.root.to_string_lossy().to_string(),
            target_covering: "main".to_string(),
            links: vec![Link {
                a: old_hash.clone(),
                b: old_hash,
                note: String::new(),
            }],
        }
        .write(&repo)
        .unwrap();

        migrate_v1_to_v2(&repo).unwrap();
        migrate_v2_to_v3(&repo).unwrap();

        let new_hash = segment::hash_of(content);
        assert_eq!(repo.read_version().unwrap(), "3");
        assert_eq!(
            Covering::read(&repo, "main").unwrap().entries[0].hash,
            new_hash
        );
        assert_eq!(
            segment::read_object(&repo.objects_dir(), &new_hash).unwrap(),
            content
        );
        let connection = Connection::read(&repo, "self").unwrap();
        assert_eq!(connection.target_peer, ".");
        assert_eq!(connection.links[0].a, new_hash);
        assert_eq!(connection.links[0].b, new_hash);
        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn published_format3_migration_cleanup_is_idempotent() {
        let repo = format2_repo("migrate-cleanup");
        fs::write(repo.dot().join("version"), "3\n").unwrap();
        fs::create_dir_all(migration_dir(&repo).join("staged")).unwrap();
        fs::write(migration_manifest(&repo), "ready\n").unwrap();

        assert!(cleanup_published_migration(&repo).unwrap());
        assert!(!migration_dir(&repo).exists());
        assert!(!cleanup_published_migration(&repo).unwrap());
        fs::remove_dir_all(repo.root).unwrap();
    }
}
