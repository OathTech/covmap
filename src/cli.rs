//! Command dispatch.
//!
//! See covmap/theory/main.tex §5 for the model. Output is line-oriented
//! and pipe-friendly.

use crate::connection::{Connection, Link};
use crate::covering::{self, Covering, CoveringEntry};
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
        "split" => cmd_split(&rest),
        "merge" => cmd_merge(&rest),
        "recut" => cmd_recut(&rest),
        "link" => cmd_link(&rest),
        "unlink" => cmd_unlink(&rest),
        "connect" => cmd_connect(&rest),
        "prune" => cmd_prune(&rest),
        "html" => cmd_html(&rest),
        // plumbing
        "hash-segment" => cmd_hash_segment(&rest),
        "cat-segment" => cmd_cat_segment(&rest),
        "ls-segments" => cmd_ls_segments(&rest),
        "ls-links" => cmd_ls_links(&rest),
        "rev-parse" => cmd_rev_parse(&rest),
        "fsck" => cmd_fsck(&rest),
        "help" | "-h" | "--help" => {
            print_usage();
            Ok(0)
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
        "split" => "covmap split <C>:<h> <line[:col]> [<top>] [<bot>] split one segment at a line or character position",
        "merge" => "covmap merge <C>:<h1> <C>:<h2> [<label>]     merge two adjacent segments",
        "recut" => "covmap recut <C>:<h|@name> | recut --remap [<C>[:<a>]]   refresh a segment, or diff-remap a covering's segments onto the edited file",
        "link" => "covmap link <K> <h_A> <h_B> [<note>]         add a link to connection K",
        "unlink" => "covmap unlink <K> <h_A> <h_B>                remove a link",
        "connect" => "covmap connect <C_A> <repoB:C_B> [<K>]       create empty connection",
        "prune" => "covmap prune <K>                             drop links whose endpoints are no longer in their coverings",
        "html" => "covmap html [-o <path>]                      render a static HTML browser",
        "hash-segment" => "covmap hash-segment <file> <a[:col]> <b[:col]> [<lbl>] [-w]",
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
  split <C>:<h> <line[:col]> [<lbl_top>] [<lbl_bot>]   split one segment at a line or character position
  merge <C>:<h1> <C>:<h2> [<label>]   merge two adjacent segments
  recut <C>:<h|@name>                 refresh a segment's hash from current disk bytes
  recut --remap [<C>[:<addr>]] [--threshold <f>]   diff-remap; for unresolved segments, print fingerprint relocation proposals (propose-only)
  link <K> <h_A> <h_B> [<note>]       add a link to connection K
  unlink <K> <h_A> <h_B>              remove a link
  connect <C_A> <repoB:C_B> [<K>]     create empty connection
  prune <K>                           drop links whose endpoints are no longer in their coverings
  html [-o <path>]                    render a static HTML browser

PLUMBING
  hash-segment <file> <a[:col]> <b[:col]> [<lbl>] [-w]
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

/// Display a link endpoint: `@name` endpoints are shown verbatim, hash
/// endpoints are truncated to the short form.
fn short_endpoint(ep: &str) -> &str {
    if ep.starts_with('@') {
        ep
    } else {
        short(ep)
    }
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
                let b_ok = tgt.as_ref().map_or(false, |t| covering::endpoint_resolves(t, &p.b));
                if !a_ok || !b_ok {
                    broken += 1;
                }
            }
            println!(
                "  {} -> {}/{} : {} links, {} broken",
                cn,
                k.target_repo,
                k.target_covering,
                k.links.len(),
                broken
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
    let mut expected = covering::Pos {
        line: 1,
        col: None,
    };
    for e in &sorted {
        if !covering::same_start(expected, e.start_pos()) {
            return Ok(false);
        }
        let on_disk = match segment::extract_content(lines, e.a, e.a_col, e.b, e.b_col) {
            Some(c) => c,
            None => return Ok(false),
        };
        let recorded = match segment::read_object(&repo.objects_dir(), &e.hash) {
            Ok((_, c)) => c,
            Err(_) => return Ok(false),
        };
        if on_disk != recorded {
            return Ok(false);
        }
        expected = covering::successor_of_end(e.end_pos());
    }
    // The covering must reach exactly one past the end of the file.
    Ok(covering::same_start(expected, covering::file_end(lines.len())))
}

fn object_exists(repo: &Repo, hash: &str) -> bool {
    segment::object_path(&repo.objects_dir(), hash).exists()
}

// ---- show -----------------------------------------------------------------

fn cmd_show(args: &[String]) -> io::Result<u8> {
    let addr = args
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "show <addr>"))?;
    let repo = current_repo()?;
    let (covering, prefix) = parse_addr(addr);

    let conn_path = repo.connections_dir().join(addr);
    if conn_path.is_file() {
        let k = Connection::read(&repo, addr)?;
        println!("connection {}", k.name);
        println!("  source covering: {}", k.source_covering);
        println!("  target repo:     {}", k.target_repo);
        println!("  target covering: {}", k.target_covering);
        println!("  links:           {}", k.links.len());
        for p in &k.links {
            if p.note.is_empty() {
                println!("    {} {}", short_endpoint(&p.a), short_endpoint(&p.b));
            } else {
                println!(
                    "    {} {}  ({})",
                    short_endpoint(&p.a),
                    short_endpoint(&p.b),
                    p.note
                );
            }
        }
        return Ok(0);
    }

    let cov_path = repo.coverings_dir().join(addr);
    if cov_path.is_file() {
        let c = Covering::read(&repo, addr)?;
        let st = covering::stats(&c);
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
        return Ok(0);
    }

    let full = match covering {
        Some(cname) => {
            let c = resolve_covering(&repo, &cname)?;
            covering::resolve_segment_hash(&c, &prefix)?
        }
        None => segment::resolve_prefix(&repo.objects_dir(), &prefix)?,
    };
    let (label, content) = segment::read_object(&repo.objects_dir(), &full)?;
    println!("segment {full}");
    println!("  label:  {}", if label.is_empty() { "-" } else { &label });
    println!("  bytes:  {}", content.len());
    println!("---");
    print!("{content}");
    if !content.ends_with('\n') {
        println!();
    }
    Ok(0)
}

// ---- ls -------------------------------------------------------------------

fn cmd_ls(args: &[String]) -> io::Result<u8> {
    let mut long = false;
    let mut name: Option<String> = None;
    for a in args {
        match a.as_str() {
            "-l" => long = true,
            other => {
                if name.is_some() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "ls accepts a single covering name",
                    ));
                }
                name = Some(other.to_string());
            }
        }
    }
    let repo = current_repo()?;
    let n = name.unwrap_or_else(|| "HEAD".to_string());
    let c = resolve_covering(&repo, &n)?;
    for e in &c.entries {
        if long {
            // Columns: hash, label, loc, lines, name. The name is
            // appended last so the historical column order (hash, label,
            // loc, lines) is unchanged for existing scripts.
            let name = e
                .name
                .as_deref()
                .map(|n| format!("@{n}"))
                .unwrap_or_else(|| "-".to_string());
            println!(
                "{}\t{}\t{}\t{}\t{}",
                e.hash,
                if e.label.is_empty() { "-" } else { &e.label },
                e.loc(),
                e.b - e.a + 1,
                name
            );
        } else {
            println!("{}\t{}", e.hash, e.loc());
        }
    }
    Ok(0)
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
        let (_, content) = segment::read_object(&repo.objects_dir(), &e.hash)?;
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
        (Some(o), None) => println!("cleared @{o} from {h}  (connections to '@{o}' are now broken)"),
        (None, None) => println!("{h} (already anonymous)"),
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
/// left untouched and its segments reported as residue. With an address
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
    let mut residue = 0usize;
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
                Ok((_, content)) => {
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
                residue += locs.len();
                new_entries.extend(old_entries);
            }
            Err(covering::RemapError::Unresolved(items)) => {
                eprintln!(
                    "{file}: {} segment boundary(ies) fall in changed regions — left \
                     untouched. Fingerprint proposals (confirm manually; nothing \
                     was changed):",
                    items.len()
                );
                residue += items.len();
                for it in &items {
                    let who = it
                        .name
                        .as_deref()
                        .map(|n| format!("@{n}"))
                        .unwrap_or_else(|| format!("{} ({})", covering::short_hash(&it.hash), it.loc()));
                    // Read the segment's recorded content and rank candidate
                    // locations in the new file. Propose-only.
                    let target = match segment::read_object(&repo.objects_dir(), &it.hash) {
                        Ok((_, c)) => c,
                        Err(_) => {
                            eprintln!("  {who}: (content unavailable)");
                            continue;
                        }
                    };
                    let cands = crate::fingerprint::locate(&target, &n_lines, 3);
                    let shown: Vec<_> =
                        cands.iter().filter(|c| c.score >= threshold).collect();
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
         {residue} unresolved"
    );
    Ok(if residue > 0 { 1 } else { 0 })
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

fn cmd_link(args: &[String]) -> io::Result<u8> {
    if args.len() < 3 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "link <connection> <hash_A> <hash_B> [<note>]",
        ));
    }
    let (kname, ha_in, hb_in) = (&args[0], &args[1], &args[2]);
    let note = args.get(3).cloned().unwrap_or_default();
    let repo = current_repo()?;
    let mut k = Connection::read(&repo, kname)?;
    let src = Covering::read(&repo, &k.source_covering)?;
    // Resolve each endpoint to a segment. The stored endpoint is the segment's
    // name (`@name`) when it is named — so the link survives content
    // edits — otherwise its segment hash (legacy / anonymous segments).
    let ia = covering::resolve_segment(&src, ha_in)?;
    let ha = src.entries[ia].endpoint();

    let tgt_repo = Repo {
        root: PathBuf::from(&k.target_repo),
    };
    let tgt = Covering::read(&tgt_repo, &k.target_covering)?;
    let ib = covering::resolve_segment(&tgt, hb_in)?;
    let hb = tgt.entries[ib].endpoint();

    if k.links.iter().any(|p| p.a == ha && p.b == hb) {
        eprintln!("(link already present)");
        return Ok(0);
    }
    k.links.push(Link {
        a: ha.clone(),
        b: hb.clone(),
        note,
    });
    k.write(&repo)?;
    println!("linked {} -> {}", ha, hb);
    Ok(0)
}

fn cmd_unlink(args: &[String]) -> io::Result<u8> {
    if args.len() < 3 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unlink <connection> <hash_A> <hash_B>",
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
    // (2) resolve against the covering on the relevant side.
    let cov = if source_side {
        Covering::read(repo, &k.source_covering)
    } else {
        let tgt_repo = Repo {
            root: PathBuf::from(&k.target_repo),
        };
        Covering::read(&tgt_repo, &k.target_covering)
    };
    if let Ok(c) = cov {
        if let Ok(idx) = covering::resolve_segment(&c, arg) {
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
            "connect <C_A> <repoB:C_B> [<connection-name>]",
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
    println!(
        "Created connection '{}' from {} to {}/{}",
        kname, k.source_covering, k.target_repo, k.target_covering
    );
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
    println!(
        "prune: {} link(s) dropped, {} kept",
        dropped,
        k.links.len()
    );
    Ok(0)
}

// ---- html -----------------------------------------------------------------

fn cmd_html(args: &[String]) -> io::Result<u8> {
    let mut out_path: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                out_path = Some(args.get(i + 1).cloned().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "-o needs a path")
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
    let html = crate::html::render(&repo)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, html)?;
    println!("wrote {}", path.display());
    Ok(0)
}

// ---- plumbing -------------------------------------------------------------

fn cmd_hash_segment(args: &[String]) -> io::Result<u8> {
    if args.len() < 3 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "hash-segment <file> <a> <b> [<label>] [-w]",
        ));
    }
    let file = &args[0];
    let (a, a_col) = parse_line_col(&args[1])?;
    let (b, b_col) = parse_line_col(&args[2])?;
    let mut label = String::new();
    let mut write = false;
    for a in &args[3..] {
        if a == "-w" {
            write = true;
        } else {
            label = a.clone();
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
        label,
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
    let (_label, content) = segment::read_object(&repo.objects_dir(), &full)?;
    let mut out = io::stdout().lock();
    out.write_all(content.as_bytes())?;
    Ok(0)
}

fn cmd_ls_segments(args: &[String]) -> io::Result<u8> {
    let n = args.first().cloned().unwrap_or_else(|| "HEAD".to_string());
    let repo = current_repo()?;
    let c = resolve_covering(&repo, &n)?;
    for e in &c.entries {
        println!("{}", e.hash);
    }
    Ok(0)
}

fn cmd_ls_links(args: &[String]) -> io::Result<u8> {
    let n = args
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "ls-links <connection>"))?;
    let repo = current_repo()?;
    let k = Connection::read(&repo, n)?;
    for p in &k.links {
        if p.note.is_empty() {
            println!("{}\t{}", p.a, p.b);
        } else {
            println!("{}\t{}\t{}", p.a, p.b, p.note);
        }
    }
    Ok(0)
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
            let mut expected = covering::Pos {
                line: 1,
                col: None,
            };
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
