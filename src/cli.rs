//! Command dispatch.
//!
//! See covmap/theory/main.tex §5 for the model. Output is line-oriented
//! and pipe-friendly.

use crate::connection::{Connection, Pair};
use crate::covering::{self, Covering, CoveringEntry};
use crate::repo::Repo;
use crate::segment::{self, Segment, HASH_LEN};
use crate::walk::{self, DEFAULT_LABEL};
use std::collections::BTreeSet;
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
        "split" => cmd_split(&rest),
        "merge" => cmd_merge(&rest),
        "recut" => cmd_recut(&rest),
        "link" => cmd_link(&rest),
        "unlink" => cmd_unlink(&rest),
        "connect" => cmd_connect(&rest),
        "repair" => cmd_repair(&rest),
        "html" => cmd_html(&rest),
        // plumbing
        "hash-segment" => cmd_hash_segment(&rest),
        "cat-segment" => cmd_cat_segment(&rest),
        "ls-segments" => cmd_ls_segments(&rest),
        "ls-pairs" => cmd_ls_pairs(&rest),
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
        "status" => "covmap status [<covering>]                   cardinality, entropy, drift, broken pairs",
        "show" => "covmap show <addr>                           segment | covering | connection at <addr>",
        "ls" => "covmap ls [-l] [<covering>]                  list segments",
        "grep" => "covmap grep [-l] [-c <C>] <pattern>          segments whose label or content matches",
        "diff" => "covmap diff <C1> <C2>                        added / removed / shared",
        "label" => "covmap label <C>:<h> <label>                 relabel a segment",
        "split" => "covmap split <C>:<h> <line> [<top>] [<bot>] split one segment at <line>",
        "merge" => "covmap merge <C>:<h1> <C>:<h2> [<label>]     merge two adjacent segments",
        "recut" => "covmap recut <C>:<h>                         refresh a segment's hash from current disk bytes",
        "link" => "covmap link <K> <h_A> <h_B> [<note>]         add a pair to connection K",
        "unlink" => "covmap unlink <K> <h_A> <h_B>                remove a pair",
        "connect" => "covmap connect <C_A> <repoB:C_B> [<K>]       create empty connection",
        "repair" => "covmap repair <K>                            drop pairs whose endpoints are no longer in their coverings",
        "html" => "covmap html [-o <path>]                      render a static HTML browser",
        "hash-segment" => "covmap hash-segment <file> <a> <b> [<lbl>] [-w]",
        "cat-segment" => "covmap cat-segment <hash>",
        "ls-segments" => "covmap ls-segments <covering>",
        "ls-pairs" => "covmap ls-pairs <connection>",
        "rev-parse" => "covmap rev-parse <addr>",
        "fsck" => "covmap fsck [--broken-pairs]",
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
  status [<covering>]                 cardinality, entropy, drift, broken pairs
  show <addr>                         segment | covering | connection at <addr>
  ls [-l] [<covering>]                list segments
  grep [-l] [-c <C>] <pattern>        segments whose label or content matches
  diff <C1> <C2>                      added / removed / shared
  label <C>:<h> <label>               relabel a segment
  split <C>:<h> <line> [<lbl_top>] [<lbl_bot>]   split one segment at <line>
  merge <C>:<h1> <C>:<h2> [<label>]   merge two adjacent segments
  recut <C>:<h>                       refresh a segment's hash from current disk bytes
  link <K> <h_A> <h_B> [<note>]       add a pair to connection K
  unlink <K> <h_A> <h_B>              remove a pair
  connect <C_A> <repoB:C_B> [<K>]     create empty connection
  repair <K>                          drop pairs whose endpoints are no longer in their coverings
  html [-o <path>]                    render a static HTML browser

PLUMBING
  hash-segment <file> <a> <b> [<lbl>] [-w]
  cat-segment <hash>
  ls-segments <covering>
  ls-pairs <connection>
  rev-parse <addr>
  fsck [--broken-pairs]
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
            b: seg.b,
            label: seg.label,
        });
        added += 1;
    }
    cov.entries
        .sort_by(|a, b| a.file.cmp(&b.file).then(a.a.cmp(&b.a)));
    cov.write(&repo)?;
    let st = covering::stats(&cov);
    println!(
        "Covering '{}': {} segments, {} lines, entropy {:.3} bits ({} added{})",
        cov.name,
        st.cardinality,
        st.total_lines,
        st.entropy_bits,
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
            "covering {}: segments={} lines={} entropy={:.3}",
            c.name, st.cardinality, st.total_lines, st.entropy_bits
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
            let src_set = covering::hashes(&src);
            let tgt_repo = Repo {
                root: PathBuf::from(&k.target_repo),
            };
            let tgt_set = match Covering::read(&tgt_repo, &k.target_covering) {
                Ok(t) => covering::hashes(&t),
                Err(_) => BTreeSet::new(),
            };
            let mut broken = 0usize;
            for p in &k.pairs {
                if !src_set.contains(&p.a) || !tgt_set.contains(&p.b) {
                    broken += 1;
                }
            }
            println!(
                "  {} -> {}/{} : {} pairs, {} broken",
                cn,
                k.target_repo,
                k.target_covering,
                k.pairs.len(),
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
    let mut expected = 1usize;
    for e in entries {
        if e.a != expected {
            return Ok(false);
        }
        if e.b > lines.len() {
            return Ok(false);
        }
        let on_disk: String = lines[e.a - 1..e.b].concat();
        let recorded = match segment::read_object(&repo.objects_dir(), &e.hash) {
            Ok((_, c)) => c,
            Err(_) => return Ok(false),
        };
        if on_disk != recorded {
            return Ok(false);
        }
        expected = e.b + 1;
    }
    Ok(expected == lines.len() + 1)
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
        println!("  pairs:           {}", k.pairs.len());
        for p in &k.pairs {
            if p.note.is_empty() {
                println!("    {} {}", short(&p.a), short(&p.b));
            } else {
                println!("    {} {}  ({})", short(&p.a), short(&p.b), p.note);
            }
        }
        return Ok(0);
    }

    let cov_path = repo.coverings_dir().join(addr);
    if cov_path.is_file() {
        let c = Covering::read(&repo, addr)?;
        let st = covering::stats(&c);
        println!(
            "covering {}: {} segments, {} lines, entropy {:.3}",
            c.name, st.cardinality, st.total_lines, st.entropy_bits
        );
        for e in &c.entries {
            println!(
                "  {} {}:{}-{}\t{}",
                short(&e.hash),
                e.file,
                e.a,
                e.b,
                if e.label.is_empty() { "-" } else { &e.label }
            );
        }
        return Ok(0);
    }

    let full = match covering {
        Some(cname) => {
            let c = resolve_covering(&repo, &cname)?;
            covering::resolve_in_covering(&c, &prefix)?
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
            println!(
                "{}\t{}\t{}:{}-{}\t{}",
                e.hash,
                if e.label.is_empty() { "-" } else { &e.label },
                e.file,
                e.a,
                e.b,
                e.b - e.a + 1
            );
        } else {
            println!("{}\t{}:{}-{}", e.hash, e.file, e.a, e.b);
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
                "{}\t{}\t{}:{}-{}",
                e.hash,
                if e.label.is_empty() { "-" } else { &e.label },
                e.file,
                e.a,
                e.b
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
            println!("- {}\t{}:{}-{}\t{}", h, e.file, e.a, e.b, e.label);
        }
    }
    for h in h2.difference(&h1) {
        if let Some(e) = covering::entry_by_hash(&c2, h) {
            println!("+ {}\t{}:{}-{}\t{}", h, e.file, e.a, e.b, e.label);
        }
    }
    for h in h1.intersection(&h2) {
        if let Some(e) = covering::entry_by_hash(&c1, h) {
            println!("= {}\t{}:{}-{}\t{}", h, e.file, e.a, e.b, e.label);
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
    let old = covering::resolve_in_covering(&c, &prefix)?;
    let new_hash = covering::relabel(&repo, &mut c, &old, &new_label)?;
    c.write(&repo)?;
    println!(
        "{} -> {}  (label '{}' in covering '{}')",
        old, new_hash, new_label, c.name
    );
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
    let cut: usize = args[1].parse().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "<line> must be a positive integer",
        )
    })?;
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
    let old = covering::resolve_in_covering(&c, &prefix)?;
    let (top, bot) = covering::split(&repo, &mut c, &old, cut, &label_top, &label_bot)?;
    c.write(&repo)?;
    println!("split {} -> {} + {}", old, top, bot);
    Ok(0)
}

// ---- recut ----------------------------------------------------------------

fn cmd_recut(args: &[String]) -> io::Result<u8> {
    let addr = args
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "recut <covering>:<hash>"))?;
    let (covname, prefix) = parse_addr(addr);
    let covname = covname.unwrap_or_else(|| "HEAD".to_string());
    let repo = current_repo()?;
    let mut c = resolve_covering(&repo, &covname)?;
    let old = covering::resolve_in_covering(&c, &prefix)?;
    let new_hash = covering::recut(&repo, &mut c, &old)?;
    c.write(&repo)?;
    if old == new_hash {
        println!("{} (unchanged)", old);
    } else {
        println!("{} -> {}", old, new_hash);
    }
    Ok(0)
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
    let h1 = covering::resolve_in_covering(&c, &p1)?;
    let h2 = covering::resolve_in_covering(&c, &p2)?;
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
    let ha = covering::resolve_in_covering(&src, ha_in)?;

    let tgt_repo = Repo {
        root: PathBuf::from(&k.target_repo),
    };
    let tgt = Covering::read(&tgt_repo, &k.target_covering)?;
    let hb = covering::resolve_in_covering(&tgt, hb_in)?;

    if k.pairs.iter().any(|p| p.a == ha && p.b == hb) {
        eprintln!("(pair already present)");
        return Ok(0);
    }
    k.pairs.push(Pair {
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
    let ha = resolve_pair_hash(&k, ha_in, true)?;
    let hb = resolve_pair_hash(&k, hb_in, false)?;
    let before = k.pairs.len();
    k.pairs.retain(|p| !(p.a == ha && p.b == hb));
    if k.pairs.len() == before {
        eprintln!("(no such pair)");
        return Ok(1);
    }
    k.write(&repo)?;
    println!("unlinked {} -> {}", ha, hb);
    Ok(0)
}

fn resolve_pair_hash(k: &Connection, prefix: &str, source_side: bool) -> io::Result<String> {
    let mut matches: Vec<&str> = k
        .pairs
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
                "no pair endpoint matches '{prefix}' in connection '{}'",
                k.name
            ),
        )),
        1 => Ok(matches.remove(0).to_string()),
        n => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("ambiguous: {n} pair endpoints match '{prefix}'"),
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
        pairs: Vec::new(),
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

// ---- repair ---------------------------------------------------------------

fn cmd_repair(args: &[String]) -> io::Result<u8> {
    let kname = args
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "repair <connection>"))?;
    let repo = current_repo()?;
    let mut k = Connection::read(&repo, kname)?;
    let src = Covering::read(&repo, &k.source_covering)?;
    let tgt_repo = Repo {
        root: PathBuf::from(&k.target_repo),
    };
    let tgt = Covering::read(&tgt_repo, &k.target_covering)?;
    let src_set = covering::hashes(&src);
    let tgt_set = covering::hashes(&tgt);

    let mut kept: Vec<Pair> = Vec::new();
    let mut dropped = 0usize;
    for p in &k.pairs {
        let a_ok = src_set.contains(&p.a);
        let b_ok = tgt_set.contains(&p.b);
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
    k.pairs = kept;
    k.write(&repo)?;
    println!(
        "repair: {} pair(s) dropped, {} kept",
        dropped,
        k.pairs.len()
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
    let a: usize = args[1]
        .parse()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "a must be a positive integer"))?;
    let b: usize = args[2]
        .parse()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "b must be a positive integer"))?;
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
    let lines: Vec<&str> = raw.split_inclusive('\n').collect();
    if a == 0 || b < a || b > lines.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "line range {a}:{b} out of bounds for {file} ({} lines)",
                lines.len()
            ),
        ));
    }
    let content: String = lines[a - 1..b].concat();
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

fn cmd_ls_pairs(args: &[String]) -> io::Result<u8> {
    let n = args
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "ls-pairs <connection>"))?;
    let repo = current_repo()?;
    let k = Connection::read(&repo, n)?;
    for p in &k.pairs {
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
            covering::resolve_in_covering(&c, &prefix)?
        }
        None => segment::resolve_prefix(&repo.objects_dir(), &prefix)?,
    };
    println!("{full}");
    Ok(0)
}

fn cmd_fsck(args: &[String]) -> io::Result<u8> {
    let mut broken_only = false;
    for a in args {
        if a == "--broken-pairs" {
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
            entries.sort_by_key(|e| e.a);
            let mut expected = 1usize;
            for e in &entries {
                if e.a != expected {
                    if !broken_only {
                        eprintln!(
                            "fsck: covering {} file {} has gap or overlap at line {} (got {})",
                            cn, f, expected, e.a
                        );
                    }
                    errs += 1;
                }
                expected = e.b + 1;
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
        let src_set = covering::hashes(&src);
        let tgt_set = covering::hashes(&tgt);
        for p in &k.pairs {
            let a_ok = src_set.contains(&p.a);
            let b_ok = tgt_set.contains(&p.b);
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
