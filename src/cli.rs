//! Command dispatch.
//!
//! See covmap/theory/main.tex §5 for the model. Output is line-oriented
//! and pipe-friendly.

mod content;
mod inspect;

use crate::cache::{self, HydrateOutcome};
use crate::connection::{Connection, Link};
use crate::covering::{self, Covering, CoveringEntry};
use crate::iterator::{self, Iter, Order, Task, TaskState};
use crate::peer;
use crate::repo::Repo;
use crate::segment::{self, Segment, HASH_LEN};
use crate::walk::{self, DEFAULT_LABEL};
use content::{ContentFormat, ContentRange, SourceContext};
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
        "status" => inspect::cmd_status(&rest),
        "show" => inspect::cmd_show(&rest),
        "ls" => inspect::cmd_ls(&rest),
        "grep" => inspect::cmd_grep(&rest),
        "diff" => inspect::cmd_diff(&rest),
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
        "migrate" => crate::migrate::run(&rest),
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
            "covmap show <addr> [--content|--content-plain] [--all]   inspect a segment, covering, region, link, or connection"
        }
        "ls" => {
            "covmap ls [-l] [<covering>] [<region> ...] [--label <l>] [--linked|--unlinked]   list segments, optionally narrowed to regions"
        }
        "grep" => "covmap grep [-l] [-c <C>] <pattern>          segments whose label or content matches",
        "diff" => "covmap diff <C1> <C2>                        added / removed / shared",
        "label" => "covmap label <C>:<h> <label>                 relabel a segment",
        "name" => "covmap name <C>:<h|@name> <name|->         set, rename, or clear (-) a segment's name",
        "annotate" => "covmap annotate <C>:<addr> [-m <msg>]... | -   set, replace, clear (-), or print a segment's message",
        "split" => {
            "covmap split <C>:<segment|file> <cut> [<cut> ...]   atomically insert line[:column] cuts; apply metadata separately"
        }
        "merge" => "covmap merge <C>:<h1> <C>:<h2> [<label>]     merge two adjacent segments",
        "recut" => "covmap recut <C>:<h|@name> | recut --remap [<C>[:<a>]]   refresh a segment, or diff-remap a covering's segments onto the edited file",
        "link" => {
            "covmap link <K> <A> <B> [-m <msg>]...       add a link to connection K\n  <A> and <B>: @name, hash prefix, or file:line[:col] in that side's covering\n  repeated messages form blank-line-separated paragraphs\n  on re-link, -m '' clears the note and no message flag preserves it"
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
            "covmap iter [<name> <verb> ...]              freeze a covering into an ordered worklist\n  done/skip [-m <msg>]...  resolve current task; repeated messages form paragraphs; no flag clears its prior note\n  peek                     list pending tasks\n  peek --all               list the complete frozen worklist with state and escaped notes"
        }
        "html" => "covmap html [-o <path>] [--covering <C>] [--connection <K>]   render a static HTML browser (one connection; auto-selected if only one exists)",
        "migrate" => {
            "covmap migrate [--recover-covering <legacy>]...  upgrade .covmap/ format (1/2/3/4 -> 5)"
        }
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
  cover [<covering>]                  add selected missing files; report policy diagnostics
  hydrate [<covering>]                reconstruct missing live objects from matching source
  status [<covering>]                 cardinality, lines, drift, broken links
  show <addr> [--content|--content-plain] [--all]
                                      inspect a segment, covering, region, link, or connection
                                      numbered content, or raw content with --content-plain
  ls [-l] [<covering>] [<region> ...] list segments, optionally narrowed to regions
  grep [-l] [-c <C>] <pattern>        segments whose label or content matches
  diff <C1> <C2>                      added / removed / shared
  label <C>:<h> <label>               relabel a segment
  name <C>:<h|@name> <name|->         set, rename, or clear (-) a segment's name
  annotate <C>:<addr> [-m <msg>]...   set/replace/clear (-)/print a segment's free-form message
  split <C>:<segment|file> <cut> [<cut> ...]   atomically insert line[:column] cuts; apply metadata separately
  merge <C>:<h1> <C>:<h2> [<label>]   merge two adjacent segments
  recut <C>:<h|@name>                 refresh a segment's hash from current disk bytes
  recut --remap [<C>[:<addr>]] [--threshold <f>]   diff-remap; for unresolved segments, print fingerprint relocation proposals (propose-only)
  link <K> <A> <B> [-m <msg>]...      add a link; repeated messages form paragraphs
  unlink <K> <A> <B>                  remove a link by @name, hash prefix, or position
  connect <C_A> [<peer>:]<C_B> [<K>]  create empty connection; omit <peer>: for same-repo
  peer bind <alias> <path> | ls | rename <old> <new> | unbind <alias>
                                      manage checkout-local peer bindings
  disconnect <K>                      delete only the named connection ref
  prune <K>                           drop links whose endpoints are no longer in their coverings
  iter [<name> <verb> ...]            freeze a covering into an ordered, resumable worklist
  html [-o <path>] [--covering <C>] [--connection <K>]   render a static HTML browser
  migrate [--recover-covering <legacy>]...
                                     upgrade .covmap/ format (1/2/3/4 -> 5)

ITERATORS (covmap iter <name> <verb>)
  <name> new <C> [--order file|label] [--label <l>]   freeze covering C into a worklist
  <name> next [--content|--content-plain] | current [--content|--content-plain]
                                                      advance to / reprint the current task
  <name> done [-m <msg>]... | skip [-m <msg>]...      resolve current task (does not advance)
  <name> choose <addr> | rewind [<addr>]              jump the cursor / reset a task to pending
  <name> peek [--all] [--content|--content-plain] | status | finished
                                                      look ahead / summarize / test exhaustion
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
    println!(
        "  Project artifacts: selection policy, .covmap/coverings, \
         .covmap/connections, and covmap metadata"
    );
    println!("  Local state:       .covmap/objects, .covmap/iterators, .covmap/local");
    println!("  Next:              git add .covmap");
    Ok(0)
}

// ---- cover ----------------------------------------------------------------

/// `cover` is *additive and idempotent*: it walks the repo, and for
/// every selected readable UTF-8 file that the covering doesn't already
/// cover, it adds a single whole-file `unknown` segment. Existing segments
/// are never touched.
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
        Some(n) if n == "HEAD" => repo.read_head()?,
        Some(n) => {
            crate::repo::validate_covering_name(&n)?;
            n
        }
        None => repo.read_head()?,
    };
    let mut cov = match Covering::read(&repo, &name) {
        Ok(c) => c,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Covering {
            name: name.clone(),
            entries: Vec::new(),
        },
        Err(error) => return Err(error),
    };
    let policy = walk::SelectionPolicy::load(&repo, &name)?;
    let selection = policy.select(&repo.root)?;
    for pattern in &selection.unmatched_include {
        let source = pattern
            .file
            .strip_prefix(&repo.root)
            .unwrap_or(&pattern.file);
        eprintln!(
            "warning: {}:{}: include pattern '{}' matches no discoverable file",
            source.display(),
            pattern.line,
            pattern.text
        );
    }
    let already = cov.covered_files();
    for file in &already {
        if !policy.admitted(file) {
            eprintln!(
                "warning: covering '{name}' keeps '{file}', which current selection policy does not admit"
            );
        }
    }
    let mut added = 0usize;
    let mut skipped_binary = 0usize;
    for rel in selection.files {
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

// ---- shared connection helpers --------------------------------------------

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
        command.push_str(" -m ");
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
    // Repeated -m/--message values are paragraphs; exactly one bare `-`
    // clears; no message arguments means read.
    let message_args = &args[1..];
    let clear = matches!(message_args, [arg] if arg == "-");
    let message = if clear {
        MessageInput::Explicit(String::new())
    } else {
        parse_message_input(message_args)
            .map_err(|error| io::Error::new(error.kind(), format!("annotate: {error}")))?
    };

    let (covname, addr) = parse_addr(addr_arg);
    let covname = covname.unwrap_or_else(|| "HEAD".to_string());
    let repo = current_repo()?;
    let mut c = resolve_covering(&repo, &covname)?;
    let idx = covering::resolve_segment(&c, &addr)?;
    let h = covering::short_hash(&c.entries[idx].hash).to_string();

    // Read mode: no -m and no `-`.
    if message == MessageInput::Absent {
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

    let new_message = message.into_option().unwrap_or_default();
    covering::set_message(&mut c, idx, Some(&new_message));
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
fn remap_scope(repo: &Repo, addr: Option<&str>) -> io::Result<(String, Option<String>)> {
    match addr {
        Some(a) if a == "HEAD" || repo.covering_exists(a)? => Ok((a.to_string(), None)),
        Some(a) => {
            let (cn, seg) = parse_addr(a);
            let covname = cn.unwrap_or_else(|| "HEAD".to_string());
            let c = resolve_covering(repo, &covname)?;
            let idx = covering::resolve_segment(&c, &seg)?;
            Ok((covname, Some(c.entries[idx].file.clone())))
        }
        None => Ok(("HEAD".to_string(), None)),
    }
}

fn cmd_recut_remap(addr: Option<&str>, threshold: f64) -> io::Result<u8> {
    let repo = current_repo()?;
    let (covname, only_file) = remap_scope(&repo, addr)?;
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

fn parse_link_args(args: &[String]) -> io::Result<(&str, &str, &str, Option<String>)> {
    // Keep parsing separate from repository mutation so message-flag behavior
    // can be tested without constructing a repository fixture.
    if args.len() < 3 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "link <connection> <A> <B> [-m <msg>]...",
        ));
    }
    let note = parse_message_input(&args[3..])
        .map_err(|error| io::Error::new(error.kind(), format!("link: {error}")))?;
    Ok((&args[0], &args[1], &args[2], note.into_option()))
}

fn cmd_link(args: &[String]) -> io::Result<u8> {
    let (kname, ha_in, hb_in, note) = parse_link_args(args)?;
    // `None` = no -m/--message flag (leave an existing note alone); `Some` =
    // a note was given (set or replace it), even if it is the empty string.
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

/// Print the frozen content of a task using the same renderer as `show`.
fn print_task_content(
    repo: &Repo,
    t: &Task,
    format: ContentFormat,
    source_context: &mut SourceContext,
) -> io::Result<()> {
    match segment::read_object(&repo.objects_dir(), &t.hash) {
        Ok(content) => {
            let range = ContentRange {
                first_line: t.a,
                first_col: t.a_col,
                last_line: t.b,
                last_col: t.b_col,
            };
            let source_widths = (format == ContentFormat::Numbered)
                .then(|| source_context.matching_widths(repo, &t.file, range, &content))
                .flatten();
            let handle = t.handle();
            let mut out = io::stdout().lock();
            content::write_content_block(
                &mut out,
                Some(&handle),
                &content,
                range,
                format,
                source_widths.as_deref(),
            )
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

/// Parse the optional content flag accepted by `next`/`current`/`peek`.
/// (`peek` also takes `--all`, so it parses its own args.)
fn parse_content_flag(verb: &str, args: &[String]) -> io::Result<ContentFormat> {
    let mut format = ContentFormat::None;
    for a in args {
        match a.as_str() {
            "--content" => {
                set_iter_content_format(verb, &mut format, ContentFormat::Numbered, "--content")?
            }
            "--content-plain" => {
                set_iter_content_format(verb, &mut format, ContentFormat::Plain, "--content-plain")?
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "iter {verb}: unexpected arg '{other}' \
                         (only --content or --content-plain is accepted)"
                    ),
                ));
            }
        }
    }
    Ok(format)
}

fn set_iter_content_format(
    verb: &str,
    current: &mut ContentFormat,
    requested: ContentFormat,
    flag: &str,
) -> io::Result<()> {
    if *current != ContentFormat::None {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("iter {verb}: content format specified more than once (at '{flag}')"),
        ));
    }
    *current = requested;
    Ok(())
}

fn cmd_iter_next(name: &str, args: &[String]) -> io::Result<u8> {
    let content_format = parse_content_flag("next", args)?;
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
            if content_format != ContentFormat::None {
                let mut source_context = SourceContext::default();
                print_task_content(&repo, &it.tasks[idx], content_format, &mut source_context)?;
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
    let content_format = parse_content_flag("current", args)?;
    let repo = current_repo()?;
    let it = Iter::read(&repo, name)?;
    match it.cursor {
        Some(idx) => {
            println!("{}", it.tasks[idx].print_line());
            if content_format != ContentFormat::None {
                let mut source_context = SourceContext::default();
                print_task_content(&repo, &it.tasks[idx], content_format, &mut source_context)?;
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
    let note = parse_message_input(args)?.into_option();
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
    let mut content_format = ContentFormat::None;
    for a in args {
        match a.as_str() {
            "--all" => all = true,
            "--content" => set_iter_content_format(
                "peek",
                &mut content_format,
                ContentFormat::Numbered,
                "--content",
            )?,
            "--content-plain" => set_iter_content_format(
                "peek",
                &mut content_format,
                ContentFormat::Plain,
                "--content-plain",
            )?,
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
    let mut source_context = SourceContext::default();
    if all {
        // The whole worklist in frozen order, each line tagged with its
        // state (and note) — an audit view including done/skipped tasks.
        for t in &it.tasks {
            println!("{}", t.print_line_stateful());
            if content_format != ContentFormat::None {
                print_task_content(&repo, t, content_format, &mut source_context)?;
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
        if content_format != ContentFormat::None {
            print_task_content(&repo, &it.tasks[idx], content_format, &mut source_context)?;
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

#[derive(Debug, PartialEq, Eq)]
enum MessageInput {
    Absent,
    Explicit(String),
}

impl MessageInput {
    fn into_option(self) -> Option<String> {
        match self {
            MessageInput::Absent => None,
            MessageInput::Explicit(value) => Some(value),
        }
    }
}

/// Parse and normalize repeated `-m <msg>` / `--message <msg>` arguments.
/// Any other argument is an error.
fn parse_message_input(args: &[String]) -> io::Result<MessageInput> {
    let mut messages = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-m" | "--message" => {
                let v = args.get(i + 1).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "-m needs a message")
                })?;
                messages.push(v.clone());
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
    if messages.is_empty() {
        Ok(MessageInput::Absent)
    } else {
        Ok(MessageInput::Explicit(crate::message::normalize_values(
            &messages,
        )))
    }
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
            println!(
                "{}\t{}\t{}\t{}\t{}",
                al,
                aloc,
                bl,
                bloc,
                crate::message::escape_field(&p.note)
            );
        }
    } else {
        for p in &k.links {
            if p.note.is_empty() {
                println!("{}\t{}", p.a, p.b);
            } else {
                println!(
                    "{}\t{}\t{}",
                    p.a,
                    p.b,
                    crate::message::escape_field(&p.note)
                );
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
    if let Err(error) = repo.read_head() {
        eprintln!("fsck: HEAD: {error}");
        errs += 1;
    }
    for issue in walk::layout_issues(&repo) {
        eprintln!("fsck: selection policy: {issue}");
        errs += 1;
    }
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
    for name in Iter::list(&repo)? {
        if let Err(error) = Iter::read(&repo, &name) {
            eprintln!("fsck: iterator {name}: {error}");
            errs += 1;
        }
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
    fn remap_scope_accepts_a_bare_covering_name() {
        let repo = temp_repo("remap-covering");
        initialize_test_repo(&repo);
        Covering {
            name: "theory".to_string(),
            entries: Vec::new(),
        }
        .write(&repo)
        .unwrap();

        assert_eq!(
            remap_scope(&repo, Some("theory")).unwrap(),
            ("theory".to_string(), None)
        );
        assert_eq!(
            remap_scope(&repo, Some("HEAD")).unwrap(),
            ("HEAD".to_string(), None)
        );
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
    fn link_messages_require_a_flag_and_repairs_emit_it() {
        let short = ["map", "@source", "@target", "-m", "why"]
            .map(String::from)
            .to_vec();
        let long = ["map", "@source", "@target", "--message", "why"]
            .map(String::from)
            .to_vec();
        let positional = ["map", "@source", "@target", "why"]
            .map(String::from)
            .to_vec();
        let repeated = ["map", "@source", "@target", "-m", "one", "-m", "two"]
            .map(String::from)
            .to_vec();

        assert_eq!(parse_link_args(&short).unwrap().3.as_deref(), Some("why"));
        assert_eq!(parse_link_args(&long).unwrap().3.as_deref(), Some("why"));
        assert!(parse_link_args(&positional)
            .unwrap_err()
            .to_string()
            .contains("only -m <msg> is accepted"));
        assert_eq!(
            parse_link_args(&repeated).unwrap().3.as_deref(),
            Some("one\n\ntwo")
        );
        assert_eq!(
            link_command("map", "@source", "@target", "why"),
            "covmap link 'map' '@source' '@target' -m 'why'"
        );
        assert_eq!(
            link_command("map", "@source", "@target", ""),
            "covmap link 'map' '@source' '@target'"
        );
    }

    #[test]
    fn shared_message_parser_normalizes_repeated_values() {
        let repeated = ["-m", "first", "--message", "second"]
            .map(String::from)
            .to_vec();
        let single = ["--message", "only"].map(String::from).to_vec();

        assert_eq!(
            parse_message_input(&repeated).unwrap(),
            MessageInput::Explicit("first\n\nsecond".to_string())
        );
        assert_eq!(
            parse_message_input(&single).unwrap(),
            MessageInput::Explicit("only".to_string())
        );
        assert_eq!(parse_message_input(&[]).unwrap(), MessageInput::Absent);
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
}
