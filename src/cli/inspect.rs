//! Read-only CLI inspection commands and their text renderers.

use super::{
    connection_route,
    content::{
        content_line_count, write_content_block, ContentFormat, ContentRange, SourceContext,
    },
    current_repo, parse_addr, print_cache_warnings, resolve_covering, resolve_target_repo, short,
};

use crate::cache;
use crate::connection::Connection;
use crate::covering::{self, Covering, CoveringEntry};
use crate::repo::Repo;
use crate::segment;
use crate::walk;
use std::io::{self, Write};

/// "Drift" for a hand-curated covering means: a file the covering
/// claims to partition has been edited so that the recorded segments
/// no longer match its current bytes. We detect this without needing
/// a rule, by reconstructing each file from the covering and comparing
/// it to what's on disk.
pub(super) fn cmd_status(args: &[String]) -> io::Result<u8> {
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ShowOptions {
    /// `None` means no CLI override. Covering views then omit content, while
    /// bare hashes retain their content-first behavior.
    content: Option<ContentFormat>,
}

pub(super) fn cmd_show(args: &[String]) -> io::Result<u8> {
    // Split off flags (--label <l>, --content) from positionals.
    let mut label_lens: Option<String> = None;
    let mut options = ShowOptions::default();
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
                set_content_format(&mut options, ContentFormat::Numbered, "--content")?;
                i += 1;
            }
            "--content-plain" => {
                set_content_format(&mut options, ContentFormat::Plain, "--content-plain")?;
                i += 1;
            }
            "--all" => {
                with_all = true;
                i += 1;
            }
            other if other.starts_with('-') => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("show: unexpected flag '{other}'"),
                ));
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
        reject_extra_show_args(&pos, 1, options)?;
        let cname = pos.first().cloned().unwrap_or_else(|| "HEAD".to_string());
        let c = resolve_covering(&repo, &cname)?;
        return show_label(
            &repo,
            &c,
            &l,
            options.content.unwrap_or(ContentFormat::None),
        );
    }

    let addr = pos
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "show <addr>"))?;

    // Connection selectors: `<K>` (list links) or `<K> <a> <b>` (one link).
    if repo.connections_dir().join(addr).is_file() {
        let k = Connection::read(&repo, addr)?;
        match pos.len() {
            1 => return show_connection(&repo, &k, with_all),
            3 => return show_link(&repo, &k, &pos[1], &pos[2]),
            count => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "show connection '{addr}': expected no endpoints or exactly <A> <B>; \
                         got {} endpoint argument(s)",
                        count - 1
                    ),
                ))
            }
        }
    }
    reject_extra_show_args(&pos, 1, options)?;

    let (covering, prefix) = parse_addr(addr);

    // Covering selector: `<C>` — either a region within it, or the whole
    // covering listing.
    if let Some(cname) = &covering {
        if repo.covering_exists(cname).unwrap_or(false) {
            let c = Covering::read(&repo, cname)?;
            if let Some(region) = parse_region(&c, &prefix) {
                return show_region(
                    &repo,
                    &c,
                    &region?,
                    options.content.unwrap_or(ContentFormat::None),
                );
            }
        }
    } else if repo.covering_exists(addr).unwrap_or(false) {
        let c = Covering::read(&repo, addr)?;
        return show_covering(&repo, &c);
    }

    // Otherwise a single segment — DETAIL.
    show_segment(&repo, covering.as_deref(), &prefix, options)
}

fn set_content_format(
    options: &mut ShowOptions,
    format: ContentFormat,
    flag: &str,
) -> io::Result<()> {
    if options.content.replace(format).is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("show: content format specified more than once (at '{flag}')"),
        ));
    }
    Ok(())
}

fn reject_extra_show_args(
    positionals: &[String],
    allowed: usize,
    options: ShowOptions,
) -> io::Result<()> {
    if positionals.len() <= allowed {
        return Ok(());
    }
    let extra = &positionals[allowed];
    let detail = if options.content == Some(ContentFormat::Numbered) && extra == "plain" {
        "; use --content-plain for plain content"
    } else {
        ""
    };
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("show: unexpected argument '{extra}'{detail}"),
    ))
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
    options: ShowOptions,
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

    let content_format = options.content.unwrap_or_else(|| {
        if entry.is_none() {
            ContentFormat::Numbered
        } else {
            ContentFormat::None
        }
    });
    if content_format != ContentFormat::None {
        // A bare-hash address has no covering context, so content is all we
        // can show; through a covering, content is opt-in.
        let range = entry
            .as_ref()
            .map(|e| ContentRange {
                first_line: e.a,
                first_col: e.a_col,
                last_line: e.b,
                last_col: e.b_col,
            })
            .unwrap_or_else(|| ContentRange::whole_lines(1, content_line_count(&content)));
        let mut source_context = SourceContext::default();
        let source_widths = (content_format == ContentFormat::Numbered)
            .then(|| {
                entry
                    .as_ref()
                    .and_then(|e| source_context.matching_widths(repo, &e.file, range, &content))
            })
            .flatten();
        let handle = entry
            .as_ref()
            .and_then(|e| e.name.as_deref().map(|name| format!("@{name}")))
            .unwrap_or_else(|| short(&full).to_string());
        let stdout = io::stdout();
        let mut out = stdout.lock();
        write_content_block(
            &mut out,
            Some(&handle),
            &content,
            range,
            content_format,
            source_widths.as_deref(),
        )?;
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

fn show_region(
    repo: &Repo,
    c: &Covering,
    region: &Region,
    content_format: ContentFormat,
) -> io::Result<u8> {
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
    let segments = prepare_region_segments(repo, segs, content_format)?;
    print_region_segs(&rel, &segments, content_format)?;
    Ok(0)
}

fn show_label(
    repo: &Repo,
    c: &Covering,
    label: &str,
    content_format: ContentFormat,
) -> io::Result<u8> {
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
    let segments = prepare_region_segments(repo, segs, content_format)?;
    print_region_segs(&rel, &segments, content_format)?;
    Ok(0)
}

struct RegionSegmentView<'a> {
    entry: &'a CoveringEntry,
    content: Option<String>,
    source_widths: Option<Vec<usize>>,
}

fn prepare_region_segments<'a>(
    repo: &Repo,
    segs: Vec<&'a CoveringEntry>,
    content_format: ContentFormat,
) -> io::Result<Vec<RegionSegmentView<'a>>> {
    let mut source_context = SourceContext::default();
    segs.into_iter()
        .map(|entry| {
            let content = match content_format {
                ContentFormat::None => None,
                ContentFormat::Plain | ContentFormat::Numbered => {
                    Some(cache::read_live_content(repo, entry)?)
                }
            };
            let source_widths = (content_format == ContentFormat::Numbered)
                .then(|| {
                    content.as_ref().and_then(|recorded| {
                        source_context.matching_widths(
                            repo,
                            &entry.file,
                            ContentRange {
                                first_line: entry.a,
                                first_col: entry.a_col,
                                last_line: entry.b,
                                last_col: entry.b_col,
                            },
                            recorded,
                        )
                    })
                })
                .flatten();
            Ok(RegionSegmentView {
                entry,
                content,
                source_widths,
            })
        })
        .collect()
}

/// Shared relationship-view body: each segment then its resolved
/// counterparts (the compare view, in text).
fn print_region_segs(
    rel: &crate::resolve::Relation,
    segments: &[RegionSegmentView<'_>],
    content_format: ContentFormat,
) -> io::Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_region_segs(&mut out, rel, segments, content_format)
}

fn write_region_segs(
    out: &mut impl Write,
    rel: &crate::resolve::Relation,
    segments: &[RegionSegmentView<'_>],
    content_format: ContentFormat,
) -> io::Result<()> {
    for segment in segments {
        let e = segment.entry;
        let handle = e
            .name
            .as_deref()
            .map(|n| format!("@{n}"))
            .unwrap_or_else(|| short(&e.hash).to_string());
        let lbl = if e.label.is_empty() { "-" } else { &e.label };
        writeln!(out, "  {handle}  {lbl}  {}", e.loc())?;
        let links = rel.links_of(&e.hash);
        if links.outgoing.is_empty() && links.incoming.is_empty() {
            writeln!(out, "    (unlinked)")?;
        }
        for l in &links.outgoing {
            writeln!(out, "    -> {}   {}", far_label(l), resolved_loc(l))?;
            if !l.note.is_empty() {
                writeln!(out, "       {}", l.note)?;
            }
        }
        for l in &links.incoming {
            writeln!(out, "    <- {}   {}", far_label(l), resolved_loc(l))?;
            if !l.note.is_empty() {
                writeln!(out, "       {}", l.note)?;
            }
        }
        if let Some(content) = &segment.content {
            write_content_block(
                out,
                Some(&handle),
                content,
                ContentRange {
                    first_line: e.a,
                    first_col: e.a_col,
                    last_line: e.b,
                    last_col: e.b_col,
                },
                content_format,
                segment.source_widths.as_deref(),
            )?;
        }
    }
    Ok(())
}

// ---- ls -------------------------------------------------------------------

pub(super) fn cmd_ls(args: &[String]) -> io::Result<u8> {
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
    let is_cov = |s: &str| repo.covering_exists(s).unwrap_or(false) || s == "HEAD";
    match positionals.first() {
        Some(first) if is_cov(first) => (first.clone(), positionals[1..].to_vec(), false),
        _ => ("HEAD".to_string(), positionals.to_vec(), true),
    }
}

// ---- grep -----------------------------------------------------------------

pub(super) fn cmd_grep(args: &[String]) -> io::Result<u8> {
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

pub(super) fn cmd_diff(args: &[String]) -> io::Result<u8> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::Link;
    use std::fs;
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
    fn region_renderer_includes_content_for_each_selected_segment() {
        let repo = temp_repo("region-content");
        initialize_test_repo(&repo);
        fs::write(repo.root.join("file.txt"), "alpha\nbeta").unwrap();
        let first_hash = segment::hash_of("alpha\n");
        let second_hash = segment::hash_of("beta");
        let covering = Covering {
            name: "main".to_string(),
            entries: vec![
                CoveringEntry {
                    hash: first_hash,
                    file: "file.txt".to_string(),
                    a: 1,
                    a_col: None,
                    b: 1,
                    b_col: None,
                    label: "first".to_string(),
                    name: Some("alpha".to_string()),
                    message: None,
                },
                CoveringEntry {
                    hash: second_hash.clone(),
                    file: "file.txt".to_string(),
                    a: 2,
                    a_col: None,
                    b: 2,
                    b_col: None,
                    label: "second".to_string(),
                    name: None,
                    message: None,
                },
            ],
        };
        covering.write(&repo).unwrap();
        let rel = crate::resolve::Relation::build(&repo, "main", &covering).unwrap();

        let segments = prepare_region_segments(
            &repo,
            covering.entries.iter().collect(),
            ContentFormat::None,
        )
        .unwrap();
        let mut metadata_only = Vec::new();
        write_region_segs(&mut metadata_only, &rel, &segments, ContentFormat::None).unwrap();
        assert!(!String::from_utf8(metadata_only)
            .unwrap()
            .contains("--- content"));

        let segments = prepare_region_segments(
            &repo,
            covering.entries.iter().collect(),
            ContentFormat::Numbered,
        )
        .unwrap();
        let mut numbered = Vec::new();
        write_region_segs(&mut numbered, &rel, &segments, ContentFormat::Numbered).unwrap();
        let numbered = String::from_utf8(numbered).unwrap();
        assert_eq!(numbered.matches("--- content ").count(), 2);
        assert!(numbered.contains("--- content @alpha ---\n1 | alpha\n"));
        assert!(numbered.contains(&format!("--- content {second_hash} ---\n2 | beta\n")));

        let segments = prepare_region_segments(
            &repo,
            covering.entries.iter().collect(),
            ContentFormat::Plain,
        )
        .unwrap();
        let mut plain = Vec::new();
        write_region_segs(&mut plain, &rel, &segments, ContentFormat::Plain).unwrap();
        let plain = String::from_utf8(plain).unwrap();
        assert!(plain.contains("--- content @alpha ---\nalpha\n"));
        assert!(plain.contains(&format!("--- content {second_hash} ---\nbeta\n")));
        assert!(!plain.contains("1 | alpha"));

        fs::remove_dir_all(repo.root).unwrap();
    }

    #[test]
    fn show_rejects_plain_as_a_trailing_content_value() {
        let options = ShowOptions {
            content: Some(ContentFormat::Numbered),
        };
        let error = reject_extra_show_args(
            &["main:file.txt".to_string(), "plain".to_string()],
            1,
            options,
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "show: unexpected argument 'plain'; use --content-plain for plain content"
        );
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
}
