//! HTML report — list / source / target views with hover tooltips and
//! click-through navigation between connections. Ported from the
//! specov design.

use crate::cache;
use crate::connection::Connection;
use crate::covering::{self, Covering, CoveringEntry};
use crate::peer;
use crate::repo::Repo;
use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;

/// Render the static HTML report.
///
/// `sel_covering` / `sel_connection`, when given, select what the report
/// shows instead of defaulting to HEAD + every connection:
/// - `sel_connection` limits the report to that one connection and, unless
///   `sel_covering` overrides, fixes the source covering to the connection's
///   own source covering — so pointing the report at a specific map does not
///   depend on the current working directory or `.covmap/HEAD`.
/// - `sel_covering` sets the source-side covering explicitly.
///
/// An unknown covering or connection name is an error.
pub fn render(
    repo: &Repo,
    sel_covering: Option<&str>,
    sel_connection: Option<&str>,
) -> io::Result<String> {
    let coverings = Covering::list(repo)?;

    // Which connections to show: one selected connection, or all of them.
    let connections = match sel_connection {
        Some(k) => {
            if !repo.connections_dir().join(k).is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("connection '{k}' not found in {}", repo.root.display()),
                ));
            }
            vec![k.to_string()]
        }
        None => Connection::list(repo)?,
    };

    // Choose the source-side covering. Precedence: --covering, else the
    // selected connection's source covering, else HEAD, else the first
    // covering present.
    let head_name = repo.read_head().unwrap_or_else(|_| "main".to_string());
    let conn_source = match sel_connection {
        Some(k) => Some(Connection::read(repo, k)?.source_covering),
        None => None,
    };
    let source_covering = match sel_covering {
        Some(c) => {
            if !repo.covering_exists(c)? {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("covering '{c}' not found in {}", repo.root.display()),
                ));
            }
            Some(c.to_string())
        }
        None => conn_source.or_else(|| {
            coverings
                .iter()
                .find(|n| n == &&head_name)
                .or_else(|| coverings.first())
                .cloned()
        }),
    };

    let src_cov = match source_covering {
        Some(n) => Some(Covering::read(repo, &n)?),
        None => None,
    };

    // Resolve all connection objects + their target coverings.
    let mut conn_data: Vec<ConnView> = Vec::new();
    for (view_id, cn) in connections.iter().enumerate() {
        let k = Connection::read(repo, cn)?;
        let tgt_repo = peer::resolve(repo, &k.target_peer)?;
        let tgt_cov = Some(Covering::read(&tgt_repo, &k.target_covering)?);
        conn_data.push(ConnView {
            view_id,
            k,
            tgt_repo,
            tgt_cov,
        });
    }

    // Build segment indices for the source covering.
    // For each segment hash in the source covering, list outgoing links.
    let mut outgoing: BTreeMap<String, Vec<OutgoingLink>> = BTreeMap::new();
    let mut incoming: BTreeMap<String, Vec<IncomingLink>> = BTreeMap::new();
    // Incoming links are keyed by target occurrence rather than content
    // hash. Distinct named segments may legitimately carry identical bytes.
    if let Some(src) = &src_cov {
        for cv in &conn_data {
            for p in &cv.k.links {
                // Endpoints may be `@name` segment references or bare hashes.
                // Resolve the source to its current content identity and the
                // target to its concrete covering occurrence.
                let a_hash = covering::endpoint_to_hash(src, &p.a);
                let b_entry = cv
                    .tgt_cov
                    .as_ref()
                    .and_then(|t| covering::endpoint_to_index(t, &p.b).map(|index| (t, index)));
                let b_hash = b_entry.map(|(t, index)| t.entries[index].hash.clone());
                let a_in = a_hash.is_some();
                let b_in = b_hash.is_some();
                let a_key = a_hash.unwrap_or_else(|| p.a.clone());
                let b_key = b_hash.unwrap_or_else(|| p.b.clone());
                let target_id = b_entry.map(|(_, index)| target_occurrence_id(cv, index));
                outgoing
                    .entry(a_key.clone())
                    .or_default()
                    .push(OutgoingLink {
                        conn_name: cv.k.name.clone(),
                        target_peer: cv.k.target_peer.clone(),
                        target_covering: cv.k.target_covering.clone(),
                        target_hash: b_key.clone(),
                        target_id: target_id.clone(),
                        note: p.note.clone(),
                        src_resolved: a_in,
                        tgt_resolved: b_in,
                    });
                if let Some(target_id) = target_id {
                    incoming.entry(target_id).or_default().push(IncomingLink {
                        conn_name: cv.k.name.clone(),
                        source_hash: a_key.clone(),
                        note: p.note.clone(),
                        src_resolved: a_in,
                    });
                }
            }
        }
    }

    let mut s = String::new();
    s.push_str(HEAD_TEMPLATE);

    // Header / summary.
    let summary = match &src_cov {
        Some(c) => {
            let st = covering::stats(c);
            format!(
                "{} segments · {} lines · {} connection(s)",
                st.cardinality,
                st.total_lines,
                conn_data.len()
            )
        }
        None => "(no coverings yet)".to_string(),
    };
    s.push_str(&format!(
        "<header>\
            <div>\
              <h1>covmap</h1>\
              <div class=\"sub\">{}</div>\
              <div class=\"sub\">repo: <code>{}</code> · covering: <code>{}</code></div>\
            </div>\
            <div class=\"view-toggle\">\
              <button data-view=\"list\" class=\"active\">List</button>\
              <button data-view=\"source\">Source</button>\
              <button data-view=\"target\">Target</button>\
              <button data-view=\"compare\">Compare</button>\
            </div>\
         </header>\n",
        esc(&summary),
        esc(&repo.root.to_string_lossy()),
        esc(src_cov.as_ref().map(|c| c.name.as_str()).unwrap_or("-"))
    ));

    // Filters bar.
    s.push_str(
        "<div class=\"filters\">\
            <span>Show:</span>\
            <label><span class=\"swatch sw-linked\"></span><input type=\"checkbox\" class=\"flt\" value=\"linked\" checked>linked</label>\
            <label><span class=\"swatch sw-unlinked\"></span><input type=\"checkbox\" class=\"flt\" value=\"unlinked\" checked>unlinked</label>\
            <label><span class=\"swatch sw-broken\"></span><input type=\"checkbox\" class=\"flt\" value=\"broken\" checked>broken</label>\
         </div>\n",
    );

    // Build per-file groupings of source segments.
    let by_file: BTreeMap<String, Vec<&CoveringEntry>> = match &src_cov {
        Some(c) => {
            let mut m: BTreeMap<String, Vec<&CoveringEntry>> = BTreeMap::new();
            for e in &c.entries {
                m.entry(e.file.clone()).or_default().push(e);
            }
            for v in m.values_mut() {
                v.sort_by_key(|e| e.sort_key());
            }
            m
        }
        None => BTreeMap::new(),
    };

    // Sidebar: source-side file list + target-side file list.
    s.push_str("<main>\n<aside id=\"sidebar\"><div id=\"sidebar-source\">");
    s.push_str(&render_source_sidebar(&by_file, &outgoing));
    s.push_str("</div><div id=\"sidebar-target\" class=\"hidden\">");
    s.push_str(&render_target_sidebar(&conn_data, &incoming));
    s.push_str("</div></aside>\n");

    // Section: three views.
    s.push_str("<section>\n");
    s.push_str("<div id=\"view-list\">");
    s.push_str(&render_list_view(&by_file, &outgoing));
    s.push_str("</div>\n");
    s.push_str("<div id=\"view-source\" class=\"hidden\">");
    s.push_str(&render_source_view(repo, &by_file, &outgoing)?);
    s.push_str("</div>\n");
    s.push_str("<div id=\"view-target\" class=\"hidden\">");
    s.push_str(&render_target_view(&conn_data, &incoming)?);
    s.push_str("</div>\n");
    s.push_str("<div id=\"view-compare\" class=\"hidden\">");
    s.push_str(&render_compare_view(
        repo, &by_file, &outgoing, &conn_data, &incoming,
    )?);
    s.push_str("</div>\n");
    s.push_str("</section>\n</main>\n");

    s.push_str("<div id=\"tooltip\" role=\"tooltip\"></div>\n");
    s.push_str(SCRIPT);
    s.push_str(TAIL_TEMPLATE);
    Ok(s)
}

// ----- helpers -------------------------------------------------------------

struct ConnView {
    view_id: usize,
    k: Connection,
    tgt_repo: Repo,
    tgt_cov: Option<Covering>,
}

#[derive(Clone)]
struct OutgoingLink {
    conn_name: String,
    target_peer: String,
    target_covering: String,
    target_hash: String,
    target_id: Option<String>,
    note: String,
    src_resolved: bool,
    tgt_resolved: bool,
}

#[derive(Clone)]
struct IncomingLink {
    conn_name: String,
    source_hash: String,
    note: String,
    src_resolved: bool,
}

fn target_occurrence_id(cv: &ConnView, entry_index: usize) -> String {
    format!("t{}-{entry_index}", cv.view_id)
}

/// Status for a source segment: linked / unlinked / broken.
fn seg_status(outgoing: &BTreeMap<String, Vec<OutgoingLink>>, hash: &str) -> &'static str {
    match outgoing.get(hash) {
        None => "unlinked",
        Some(v) => {
            if v.iter().any(|l| !l.src_resolved || !l.tgt_resolved) {
                "broken"
            } else {
                "linked"
            }
        }
    }
}

fn render_source_sidebar(
    by_file: &BTreeMap<String, Vec<&CoveringEntry>>,
    outgoing: &BTreeMap<String, Vec<OutgoingLink>>,
) -> String {
    let mut s = String::new();
    // total counts
    let mut totals = (0usize, 0usize, 0usize); // linked, unlinked, broken
    for entries in by_file.values() {
        for e in entries {
            match seg_status(outgoing, &e.hash) {
                "linked" => totals.0 += 1,
                "broken" => totals.2 += 1,
                _ => totals.1 += 1,
            }
        }
    }
    s.push_str(&file_sidebar_item("all files", "__all__", totals, true));
    for (fname, entries) in by_file {
        let mut counts = (0usize, 0usize, 0usize);
        for e in entries {
            match seg_status(outgoing, &e.hash) {
                "linked" => counts.0 += 1,
                "broken" => counts.2 += 1,
                _ => counts.1 += 1,
            }
        }
        s.push_str(&file_sidebar_item(fname, fname, counts, false));
    }
    s
}

fn file_sidebar_item(name: &str, key: &str, counts: (usize, usize, usize), active: bool) -> String {
    let total = counts.0 + counts.1 + counts.2;
    let denom = total.max(1) as f64;
    let pct_l = (counts.0 as f64 / denom) * 100.0;
    let pct_u = (counts.1 as f64 / denom) * 100.0;
    let pct_b = (counts.2 as f64 / denom) * 100.0;
    format!(
        "<div class=\"file{}\" data-file=\"{}\">\
           <div class=\"name\">{}</div>\
           <div class=\"bar\">\
             <span class=\"sw-linked\" style=\"width:{:.1}%\"></span>\
             <span class=\"sw-unlinked\" style=\"width:{:.1}%\"></span>\
             <span class=\"sw-broken\" style=\"width:{:.1}%\"></span>\
           </div>\
           <div class=\"counts\">{} segments · {} linked · {} unlinked · {} broken</div>\
         </div>",
        if active { " active" } else { "" },
        esc(key),
        esc(name),
        pct_l,
        pct_u,
        pct_b,
        total,
        counts.0,
        counts.1,
        counts.2
    )
}

fn render_target_sidebar(
    conns: &[ConnView],
    incoming: &BTreeMap<String, Vec<IncomingLink>>,
) -> String {
    if conns.is_empty() {
        return "<div class=\"file-empty\">No connections yet.</div>".to_string();
    }
    let mut s = String::new();
    // group target files by (repo, covering, file)
    let mut groups: BTreeMap<(String, String, String), usize> = BTreeMap::new();
    for cv in conns {
        if let Some(tc) = &cv.tgt_cov {
            let mut by_file: BTreeMap<String, usize> = BTreeMap::new();
            for (entry_index, e) in tc.entries.iter().enumerate() {
                if incoming.contains_key(&target_occurrence_id(cv, entry_index)) {
                    *by_file.entry(e.file.clone()).or_insert(0) += 1;
                }
            }
            for (f, n) in by_file {
                let key = (cv.k.target_peer.clone(), cv.k.target_covering.clone(), f);
                *groups.entry(key).or_insert(0) += n;
            }
        }
    }
    if groups.is_empty() {
        return "<div class=\"file-empty\">No target files have incoming links.</div>".to_string();
    }
    for ((repo, cov, file), n) in groups {
        let key = format!("{}::{}::{}", repo, cov, file);
        s.push_str(&format!(
            "<div class=\"file\" data-target-file=\"{}\">\
               <div class=\"name\">{}</div>\
               <div class=\"counts\">{} · {} incoming link(s)</div>\
             </div>",
            esc(&key),
            esc(&file),
            esc(&format!("{} / {}", short_repo_name(&repo), cov)),
            n
        ));
    }
    s
}

fn short_repo_name(p: &str) -> String {
    PathBuf::from(p)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| p.to_string())
}

// ----- list view ----------------------------------------------------------

fn render_list_view(
    by_file: &BTreeMap<String, Vec<&CoveringEntry>>,
    outgoing: &BTreeMap<String, Vec<OutgoingLink>>,
) -> String {
    let mut s = String::new();
    if by_file.is_empty() {
        s.push_str("<div class=\"empty\">No source covering yet. Run `covmap cover`.</div>");
        return s;
    }
    for entries in by_file.values() {
        for (i, e) in entries.iter().enumerate() {
            let status = seg_status(outgoing, &e.hash);
            let links = outgoing.get(&e.hash).cloned().unwrap_or_default();
            let label = if e.label.is_empty() {
                "&middot;"
            } else {
                &e.label
            };
            // A segment message (if any) renders as its own detail line under
            // the meta, newlines preserved.
            let message_block = match &e.message {
                Some(m) => format!("<div class=\"seg-message\">{}</div>", esc(m)),
                None => String::new(),
            };
            s.push_str(&format!(
                "<div class=\"segment clickable status-{} zebra-{}\" data-status=\"{}\" \
                       data-file=\"{}\" data-hash=\"{}\" data-tip='{}'>\
                   <h3>{} <span class=\"badge {}\">{}</span></h3>\
                   <div class=\"meta\"><code>{}</code> · {} · {} lines · label {}</div>\
                   {}{}\
                 </div>\n",
                status,
                i % 2,
                status,
                esc(&e.file),
                &e.hash,
                esc(&tip_json(e, &links)),
                esc(&heading_for(e)),
                status,
                status,
                &e.hash,
                esc(&e.loc()),
                e.b - e.a + 1,
                label_html(label),
                message_block,
                links_block(&links)
            ));
        }
    }
    s
}

fn heading_for(e: &CoveringEntry) -> String {
    // Use label if present, otherwise show file:lines.
    if !e.label.is_empty() {
        e.label.clone()
    } else {
        e.loc()
    }
}

fn label_html(label: &str) -> String {
    // Already either escaped placeholder or raw text.
    if label == "&middot;" {
        label.to_string()
    } else {
        format!("<code>{}</code>", esc(label))
    }
}

fn links_block(links: &[OutgoingLink]) -> String {
    if links.is_empty() {
        return String::new();
    }
    let mut s = String::from("<div class=\"links\">→ ");
    for (i, l) in links.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        let cls = if !l.src_resolved || !l.tgt_resolved {
            "broken"
        } else {
            "ok"
        };
        s.push_str(&format!(
            "<code class=\"{}\">{}</code> in <code>{}</code>",
            cls,
            &l.target_hash,
            esc(&format!(
                "{}:{}",
                short_repo_name(&l.target_peer),
                l.target_covering
            ))
        ));
        if !l.note.is_empty() {
            s.push_str(&format!(" <span class=\"note\">— {}</span>", esc(&l.note)));
        }
    }
    s.push_str("</div>");
    s
}

// ----- source view --------------------------------------------------------

fn render_source_view(
    repo: &Repo,
    by_file: &BTreeMap<String, Vec<&CoveringEntry>>,
    outgoing: &BTreeMap<String, Vec<OutgoingLink>>,
) -> io::Result<String> {
    let mut s = String::new();
    if by_file.is_empty() {
        s.push_str("<div class=\"empty\">No source covering yet.</div>");
        return Ok(s);
    }
    for (fname, entries) in by_file {
        s.push_str(&format!(
            "<div class=\"src-file\" data-file=\"{}\"><h2>{}</h2><pre class=\"src-pre\">",
            esc(fname),
            esc(fname)
        ));
        for (i, e) in entries.iter().enumerate() {
            let content =
                cache::read_live_content(repo, e).unwrap_or_else(|_| String::from("(missing)\n"));
            let status = seg_status(outgoing, &e.hash);
            let links = outgoing.get(&e.hash).cloned().unwrap_or_default();
            let inline = if e.is_line_aligned() { "" } else { " inline" };
            s.push_str(&format!(
                "<span class=\"src-seg status-{}{} zebra-{}\" data-status=\"{}\" \
                       data-segid=\"{}\" data-file=\"{}\" data-tip='{}'>",
                status,
                inline,
                i % 2,
                status,
                &e.hash,
                esc(fname),
                esc(&tip_json(e, &links))
            ));
            s.push_str(&esc(&content));
            s.push_str("</span>");
        }
        s.push_str("</pre></div>\n");
    }
    Ok(s)
}

// ----- target view --------------------------------------------------------

fn render_target_view(
    conns: &[ConnView],
    incoming: &BTreeMap<String, Vec<IncomingLink>>,
) -> io::Result<String> {
    let mut s = String::new();
    if conns.is_empty() {
        s.push_str("<div class=\"empty\">No connections yet.</div>");
        return Ok(s);
    }
    let mut printed = 0usize;
    for cv in conns {
        let tc = match &cv.tgt_cov {
            Some(t) => t,
            None => continue,
        };
        // Group target entries by file.
        let mut by_file: BTreeMap<String, Vec<(usize, &CoveringEntry)>> = BTreeMap::new();
        for (entry_index, e) in tc.entries.iter().enumerate() {
            by_file
                .entry(e.file.clone())
                .or_default()
                .push((entry_index, e));
        }
        for v in by_file.values_mut() {
            v.sort_by_key(|(_, e)| e.sort_key());
        }

        for (fname, entries) in &by_file {
            let has_incoming = entries.iter().any(|(entry_index, _)| {
                incoming.contains_key(&target_occurrence_id(cv, *entry_index))
            });
            if !has_incoming {
                continue;
            }
            printed += 1;

            let key = format!("{}::{}::{}", cv.k.target_peer, cv.k.target_covering, fname);
            s.push_str(&format!(
                "<div class=\"tgt-file\" data-target-file=\"{}\">\
                   <h2>{} <span class=\"sub\">[{} / {}]</span></h2>\
                   <pre class=\"tgt-pre\">",
                esc(&key),
                esc(fname),
                esc(&short_repo_name(&cv.k.target_peer)),
                esc(&cv.k.target_covering)
            ));
            for (i, (entry_index, e)) in entries.iter().enumerate() {
                let content = cache::read_live_content(&cv.tgt_repo, e)
                    .unwrap_or_else(|_| String::from("(missing)\n"));
                let target_id = target_occurrence_id(cv, *entry_index);
                let inc = incoming.get(&target_id).cloned().unwrap_or_default();
                let any_broken = inc.iter().any(|l| !l.src_resolved);
                let cov_class = if inc.is_empty() {
                    ""
                } else if any_broken {
                    " covered cov-broken"
                } else {
                    " covered cov-linked"
                };
                let inline = if e.is_line_aligned() { "" } else { " inline" };
                s.push_str(&format!(
                    "<span class=\"tgt-block{}{} zebra-{}\" data-tgt-id=\"{}\" data-tgt-hash=\"{}\">",
                    cov_class,
                    inline,
                    i % 2,
                    target_id,
                    &e.hash
                ));
                s.push_str(&esc(&content));
                if !inc.is_empty() {
                    s.push_str("<span class=\"tgt-markers\">");
                    for l in &inc {
                        let cls = if !l.src_resolved {
                            "tgt-marker tgt-broken"
                        } else {
                            "tgt-marker"
                        };
                        s.push_str(&format!(
                            "<span class=\"{} tgt-to-src\" data-segid=\"{}\">\
                                ← <code>{}</code> ({})\
                             </span>\
                             <span class=\"tgt-to-cmp\" title=\"show this link in Compare\" \
                                   data-a-hash=\"{}\" data-b-id=\"{}\">⇄</span>",
                            cls,
                            &l.source_hash,
                            &l.source_hash,
                            esc(&l.conn_name),
                            &l.source_hash,
                            target_id
                        ));
                        if !l.note.is_empty() {
                            s.push_str(&format!(" <span class=\"note\">{}</span>", esc(&l.note)));
                        }
                    }
                    s.push_str("</span>");
                }
                s.push_str("</span>");
            }
            s.push_str("</pre></div>\n");
        }
    }
    if printed == 0 {
        s.push_str("<div class=\"empty\">No target files have incoming links.</div>");
    }
    Ok(s)
}

// ----- compare view --------------------------------------------------------
//
// A self-contained 4-panel layout that shows the FULL A and B coverings at
// once, so they can be read side by side. Left to right:
//
//   [ file list A ] [ covering focus A ] [ covering focus B ] [ file list B ]
//
// All markup here uses `cmp-*` classes and `data-cmp-*` attributes so the
// existing List / Source / Target views (and their JS) are untouched. Source
// segments carry the target occurrence IDs they link to (`data-cmp-links`); target
// segments carry the source hashes that point at them (`data-cmp-back`). The
// compare JS uses those to highlight + scroll the counterpart panel on click.

fn render_compare_view(
    repo: &Repo,
    by_file: &BTreeMap<String, Vec<&CoveringEntry>>,
    outgoing: &BTreeMap<String, Vec<OutgoingLink>>,
    conns: &[ConnView],
    incoming: &BTreeMap<String, Vec<IncomingLink>>,
) -> io::Result<String> {
    let mut s = String::from("<div class=\"cmp-grid\">\n");

    // Hash -> source entry lookup, so the B-side reverse tooltip can name the
    // A segments (loc / label / file) that point at a target segment.
    let src_by_hash: BTreeMap<&str, &CoveringEntry> = by_file
        .values()
        .flatten()
        .map(|e| (e.hash.as_str(), *e))
        .collect();

    // --- panel 1: file list for A ------------------------------------------
    s.push_str(
        "<aside class=\"cmp-files cmp-files-a\"><div class=\"cmp-files-head\">A · source</div>",
    );
    if by_file.is_empty() {
        s.push_str("<div class=\"file-empty\">No source covering.</div>");
    } else {
        for (fname, entries) in by_file {
            let mut counts = (0usize, 0usize, 0usize); // linked, unlinked, broken
            for e in entries {
                match seg_status(outgoing, &e.hash) {
                    "linked" => counts.0 += 1,
                    "broken" => counts.2 += 1,
                    _ => counts.1 += 1,
                }
            }
            s.push_str(&cmp_file_item("a", fname, fname, counts, None));
        }
    }
    s.push_str("</aside>\n");

    // --- panel 2: covering focus for A -------------------------------------
    s.push_str("<div class=\"cmp-focus cmp-focus-a\">");
    if by_file.is_empty() {
        s.push_str("<div class=\"empty\">No source covering yet.</div>");
    } else {
        for (fname, entries) in by_file {
            s.push_str(&format!(
                "<div class=\"cmp-file-block\" data-cmp-file-a=\"{}\"><h3>{}</h3><pre class=\"cmp-pre\">",
                esc(fname),
                esc(fname)
            ));
            for (i, e) in entries.iter().enumerate() {
                let content = cache::read_live_content(repo, e)
                    .unwrap_or_else(|_| String::from("(missing)\n"));
                let status = seg_status(outgoing, &e.hash);
                let links = outgoing.get(&e.hash).cloned().unwrap_or_default();
                let link_targets: Vec<&str> = links
                    .iter()
                    .filter_map(|link| link.target_id.as_deref())
                    .collect();
                let inline = if e.is_line_aligned() { "" } else { " inline" };
                s.push_str(&format!(
                    "<span class=\"cmp-seg cmp-seg-a status-{}{} zebra-{}\" data-status=\"{}\" \
                           data-cmp-a-hash=\"{}\" data-cmp-links=\"{}\" data-tip='{}'>",
                    status,
                    inline,
                    i % 2,
                    status,
                    &e.hash,
                    esc(&link_targets.join(" ")),
                    esc(&tip_json(e, &links))
                ));
                s.push_str(&esc(&content));
                s.push_str("</span>");
            }
            s.push_str("</pre></div>\n");
        }
    }
    s.push_str("</div>\n");

    // --- panel 3: covering focus for B -------------------------------------
    // Show the full target covering(s). Each target segment records which
    // source hashes point at it (if any) for the back-highlight.
    s.push_str("<div class=\"cmp-focus cmp-focus-b\">");
    let mut any_b = false;
    for cv in conns {
        let tc = match &cv.tgt_cov {
            Some(t) => t,
            None => continue,
        };
        let mut tgt_by_file: BTreeMap<String, Vec<(usize, &CoveringEntry)>> = BTreeMap::new();
        for (entry_index, e) in tc.entries.iter().enumerate() {
            tgt_by_file
                .entry(e.file.clone())
                .or_default()
                .push((entry_index, e));
        }
        for v in tgt_by_file.values_mut() {
            v.sort_by_key(|(_, e)| e.sort_key());
        }
        for (fname, entries) in &tgt_by_file {
            any_b = true;
            let key = format!("{}::{}::{}", cv.k.target_peer, cv.k.target_covering, fname);
            s.push_str(&format!(
                "<div class=\"cmp-file-block\" data-cmp-file-b=\"{}\">\
                   <h3>{} <span class=\"sub\">[{} / {}]</span></h3><pre class=\"cmp-pre\">",
                esc(&key),
                esc(fname),
                esc(&short_repo_name(&cv.k.target_peer)),
                esc(&cv.k.target_covering)
            ));
            for (i, (entry_index, e)) in entries.iter().enumerate() {
                let content = cache::read_live_content(&cv.tgt_repo, e)
                    .unwrap_or_else(|_| String::from("(missing)\n"));
                let target_id = target_occurrence_id(cv, *entry_index);
                let inc = incoming.get(&target_id).cloned().unwrap_or_default();
                let back_hashes: Vec<String> = inc.iter().map(|l| l.source_hash.clone()).collect();
                let status = if inc.is_empty() {
                    "unlinked"
                } else if inc.iter().any(|l| !l.src_resolved) {
                    "broken"
                } else {
                    "linked"
                };
                let inline = if e.is_line_aligned() { "" } else { " inline" };
                s.push_str(&format!(
                    "<span class=\"cmp-seg cmp-seg-b status-{}{} zebra-{}\" data-status=\"{}\" \
                           data-cmp-b-id=\"{}\" data-cmp-b-hash=\"{}\" data-cmp-back=\"{}\" data-tip='{}'>",
                    status,
                    inline,
                    i % 2,
                    status,
                    target_id,
                    &e.hash,
                    esc(&back_hashes.join(" ")),
                    esc(&back_tip_json(
                        e,
                        status,
                        &inc,
                        &src_by_hash,
                        &target_id,
                    ))
                ));
                s.push_str(&esc(&content));
                s.push_str("</span>");
            }
            s.push_str("</pre></div>\n");
        }
    }
    if !any_b {
        s.push_str("<div class=\"empty\">No target covering. Run `covmap connect`.</div>");
    }
    s.push_str("</div>\n");

    // --- panel 4: file list for B ------------------------------------------
    s.push_str(
        "<aside class=\"cmp-files cmp-files-b\"><div class=\"cmp-files-head\">B · target</div>",
    );
    if !any_b {
        s.push_str("<div class=\"file-empty\">No target covering.</div>");
    } else {
        // group target files by (repo, covering, file), counting each
        // segment's status (linked / unlinked / broken) the same way the
        // focus-B panel does, so the bar matches the A side.
        let mut groups: BTreeMap<(String, String, String), (usize, usize, usize)> = BTreeMap::new();
        for cv in conns {
            if let Some(tc) = &cv.tgt_cov {
                for (entry_index, e) in tc.entries.iter().enumerate() {
                    let inc = incoming
                        .get(&target_occurrence_id(cv, entry_index))
                        .cloned()
                        .unwrap_or_default();
                    let key = (
                        cv.k.target_peer.clone(),
                        cv.k.target_covering.clone(),
                        e.file.clone(),
                    );
                    let g = groups.entry(key).or_insert((0, 0, 0));
                    if inc.is_empty() {
                        g.1 += 1; // unlinked
                    } else if inc.iter().any(|l| !l.src_resolved) {
                        g.2 += 1; // broken
                    } else {
                        g.0 += 1; // linked
                    }
                }
            }
        }
        for ((repo_p, cov, file), counts) in groups {
            let key = format!("{}::{}::{}", repo_p, cov, file);
            let sub = format!("{} / {}", short_repo_name(&repo_p), cov);
            s.push_str(&cmp_file_item("b", &file, &key, counts, Some(&sub)));
        }
    }
    s.push_str("</aside>\n");

    s.push_str("</div>\n");
    Ok(s)
}

fn cmp_file_item(
    side: &str,
    name: &str,
    key: &str,
    counts: (usize, usize, usize),
    sub: Option<&str>,
) -> String {
    let total = counts.0 + counts.1 + counts.2;
    let denom = total.max(1) as f64;
    let pct_l = (counts.0 as f64 / denom) * 100.0;
    let pct_u = (counts.1 as f64 / denom) * 100.0;
    let pct_b = (counts.2 as f64 / denom) * 100.0;
    let sub_html = match sub {
        Some(t) => format!("<div class=\"sub\">{}</div>", esc(t)),
        None => String::new(),
    };
    format!(
        "<div class=\"file\" data-cmp-file-{}=\"{}\">\
           <div class=\"name\">{}</div>\
           {}\
           <div class=\"bar\">\
             <span class=\"sw-linked\" style=\"width:{:.1}%\"></span>\
             <span class=\"sw-unlinked\" style=\"width:{:.1}%\"></span>\
             <span class=\"sw-broken\" style=\"width:{:.1}%\"></span>\
           </div>\
           <div class=\"counts\">{} segs · {} linked · {} unlinked · {} broken</div>\
         </div>",
        side,
        esc(key),
        esc(name),
        sub_html,
        pct_l,
        pct_u,
        pct_b,
        total,
        counts.0,
        counts.1,
        counts.2
    )
}

// ----- tooltip JSON --------------------------------------------------------

fn tip_json(e: &CoveringEntry, links: &[OutgoingLink]) -> String {
    let mut s = String::from("{");
    s.push_str(&format!("\"hash\":\"{}\",", &e.hash));
    s.push_str(&format!("\"loc\":\"{}\",", json_escape(&e.loc())));
    s.push_str(&format!("\"label\":\"{}\",", json_escape(&e.label)));
    if let Some(m) = &e.message {
        s.push_str(&format!("\"message\":\"{}\",", json_escape(m)));
    }
    let status = if links.is_empty() {
        "unlinked"
    } else if links.iter().any(|l| !l.src_resolved || !l.tgt_resolved) {
        "broken"
    } else {
        "linked"
    };
    s.push_str(&format!("\"status\":\"{}\",", status));
    s.push_str("\"links\":[");
    for (i, l) in links.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!(
            "{{\"conn\":\"{}\",\"target_peer\":\"{}\",\"target_covering\":\"{}\",\"target_hash\":\"{}\",\"target_id\":\"{}\",\"note\":\"{}\",\"resolved\":{}}}",
            json_escape(&l.conn_name),
            json_escape(&l.target_peer),
            json_escape(&l.target_covering),
            &l.target_hash,
            l.target_id.as_deref().unwrap_or(""),
            json_escape(&l.note),
            l.src_resolved && l.tgt_resolved
        ));
    }
    s.push_str("]}");
    s
}

/// Reverse tooltip for a B (target) segment: the A (source) segments that
/// point at it. Mirrors `tip_json` but lists incoming links instead of
/// outgoing ones. `dir:"back"` lets the shared JS pick the right renderer.
fn back_tip_json(
    e: &CoveringEntry,
    status: &str,
    incoming: &[IncomingLink],
    src_by_hash: &BTreeMap<&str, &CoveringEntry>,
    target_id: &str,
) -> String {
    let mut s = String::from("{\"dir\":\"back\",");
    s.push_str(&format!("\"hash\":\"{}\",", &e.hash));
    s.push_str(&format!("\"loc\":\"{}\",", json_escape(&e.loc())));
    s.push_str(&format!("\"label\":\"{}\",", json_escape(&e.label)));
    s.push_str(&format!("\"target_id\":\"{}\",", json_escape(target_id)));
    if let Some(m) = &e.message {
        s.push_str(&format!("\"message\":\"{}\",", json_escape(m)));
    }
    s.push_str(&format!("\"status\":\"{}\",", status));
    s.push_str("\"links\":[");
    for (i, l) in incoming.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let src = src_by_hash.get(l.source_hash.as_str());
        let src_loc = src.map(|e| e.loc()).unwrap_or_default();
        let src_label = src.map(|e| e.label.clone()).unwrap_or_default();
        let src_file = src.map(|e| e.file.clone()).unwrap_or_default();
        s.push_str(&format!(
            "{{\"conn\":\"{}\",\"source_hash\":\"{}\",\"source_loc\":\"{}\",\"source_label\":\"{}\",\"source_file\":\"{}\",\"note\":\"{}\",\"resolved\":{}}}",
            json_escape(&l.conn_name),
            &l.source_hash,
            json_escape(&src_loc),
            json_escape(&src_label),
            json_escape(&src_file),
            json_escape(&l.note),
            l.src_resolved
        ));
    }
    s.push_str("]}");
    s
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

// ----- HTML escaping -------------------------------------------------------

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

// ----- templates -----------------------------------------------------------

const HEAD_TEMPLATE: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>covmap report</title>
<style>
:root {
  --c-linked:   #22c55e;
  --c-unlinked: #cbd5e1;
  --c-broken:   #ef4444;
}
body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; margin: 0; color: #222; }
header { background: #1e293b; color: white; padding: 1rem 1.5rem; display: flex; align-items: center; justify-content: space-between; }
header h1 { margin: 0; font-size: 1.3rem; }
header .sub { opacity: 0.85; font-size: 0.85rem; margin-top: 0.25rem; }
header code { background: rgba(255,255,255,0.1); padding: 0 .3em; border-radius: 3px; font-family: ui-monospace, monospace; }
.view-toggle { display: flex; gap: 0; border: 1px solid #475569; border-radius: 6px; overflow: hidden; }
.view-toggle button { background: transparent; color: white; border: 0; padding: 0.4rem 0.85rem; cursor: pointer; font-size: 0.85rem; }
.view-toggle button.active { background: #475569; }
main { display: grid; grid-template-columns: 320px 1fr; min-height: calc(100vh - 110px); }
aside { border-right: 1px solid #e5e7eb; background: #f9fafb; overflow-y: auto; }
.file { padding: 0.75rem 1rem; border-bottom: 1px solid #e5e7eb; cursor: pointer; }
.file:hover { background: #eef2ff; }
.file.active { background: #e0e7ff; }
.file .name { font-weight: 600; font-size: 0.9rem; word-break: break-all; }
.file .bar { display: flex; height: 6px; margin-top: 0.35rem; border-radius: 3px; overflow: hidden; background: #e5e7eb; }
.file .bar span { display: block; height: 100%; }
.file .counts { font-size: 0.75rem; color: #64748b; margin-top: 0.25rem; }
section { padding: 1.5rem; overflow-y: auto; }

.segment { border: 1px solid #e5e7eb; border-radius: 6px; padding: 0.85rem 1rem; margin-bottom: 0.75rem; background: white; }
.segment.clickable { cursor: pointer; }
.segment.clickable:hover { border-color: #94a3b8; }
.segment h3 { margin: 0 0 0.25rem 0; font-size: 1rem; }
.segment .meta { font-size: 0.75rem; color: #64748b; margin-bottom: 0.5rem; font-family: ui-monospace, monospace; }
.segment .seg-message { font-size: 0.8rem; color: #475569; margin-bottom: 0.5rem; white-space: pre-wrap; border-left: 3px solid #cbd5e1; padding-left: 0.6rem; }
.segment .links { margin-top: 0.5rem; font-size: 0.82rem; }
.segment .links code { background: #f1f5f9; padding: 0.1rem 0.35rem; border-radius: 3px; }
.segment .note { font-style: italic; color: #4b5563; white-space: pre-wrap; }

.badge { display: inline-block; padding: 0.1rem 0.5rem; border-radius: 10px; font-size: 0.7rem; font-weight: 600; text-transform: uppercase; letter-spacing: 0.03em; }
.badge.linked   { background: #dcfce7; color: #166534; }
.badge.unlinked { background: #e5e7eb; color: #374151; }
.badge.broken   { background: #fecaca; color: #991b1b; }
.bar span.sw-linked, .swatch.sw-linked     { background: var(--c-linked); }
.bar span.sw-unlinked, .swatch.sw-unlinked { background: var(--c-unlinked); }
.bar span.sw-broken, .swatch.sw-broken     { background: var(--c-broken); }

.filters { padding: 0.65rem 1.5rem; background: #f1f5f9; border-bottom: 1px solid #e5e7eb; font-size: 0.85rem; display: flex; gap: 1.25rem; align-items: center; flex-wrap: wrap; }
.filters label { cursor: pointer; display: inline-flex; align-items: center; gap: 0.35rem; }
.filters .swatch { width: 10px; height: 10px; border-radius: 2px; display: inline-block; }

.hidden { display: none !important; }

.src-file { margin-bottom: 2rem; }
.src-file h2 { font-size: 1rem; font-family: ui-monospace, monospace; background: #f1f5f9; padding: 0.5rem 0.75rem; border-radius: 4px; margin: 0 0 0.5rem 0; }
.src-pre {
  margin: 0; font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
  font-size: 0.85rem; line-height: 1.55; white-space: pre-wrap; word-break: break-word;
  background: #fafafa; border: 1px solid #e5e7eb; border-radius: 4px; padding: 0.75rem 1rem;
}
.src-seg {
  display: block; border-left: 4px solid transparent; padding: 0.15rem 0.4rem 0.15rem 0.6rem;
  margin: 0.35rem -1rem 0.35rem -1rem; position: relative; cursor: help;
}
/* Character-level fragments flow inline so adjacent chunks share the
   same source line; the real newlines in the content (under pre-wrap)
   do the line breaking. The highlight hugs just those characters. */
.src-seg.inline {
  display: inline; border-left: none; padding: 0.05em 0.1em;
  margin: 0; border-radius: 3px; box-decoration-break: clone; -webkit-box-decoration-break: clone;
}
.src-seg:hover { filter: brightness(0.97); }
.src-seg.status-linked   { background: rgba(34, 197, 94, 0.12); border-left-color: var(--c-linked); }
.src-seg.status-unlinked { background: rgba(203, 213, 225, 0.25); border-left-color: var(--c-unlinked); }
.src-seg.status-broken   { background: rgba(239, 68, 68, 0.14); border-left-color: var(--c-broken); }
.src-seg.dimmed { opacity: 0.3; }
.src-seg.flash  { animation: flash 1.6s ease-out; }

#tooltip {
  position: fixed; z-index: 100; background: #0f172a; color: #f1f5f9;
  padding: 0.6rem 0.8rem; border-radius: 6px; box-shadow: 0 8px 24px rgba(0,0,0,0.25);
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  font-size: 0.8rem; line-height: 1.4; max-width: 380px; pointer-events: auto;
  opacity: 0; transition: opacity 0.08s ease-out;
}
#tooltip.visible { opacity: 1; }
#tooltip:not(.visible) { pointer-events: none; }
#tooltip .tt-title { font-weight: 600; margin-bottom: 0.3rem; display: flex; align-items: center; gap: 0.4rem; }
#tooltip .tt-meta { font-family: ui-monospace, monospace; font-size: 0.72rem; color: #94a3b8; margin-bottom: 0.35rem; }
#tooltip .tt-note { font-style: italic; color: #cbd5e1; margin: 0.35rem 0; white-space: pre-wrap; }
#tooltip .tt-links { margin: 0.35rem 0 0 0; padding-left: 1rem; }
#tooltip .tt-links li { margin: 0.1rem 0; }
#tooltip code { background: rgba(148,163,184,0.2); padding: 0.05rem 0.25rem; border-radius: 3px; font-size: 0.72rem; }
#tooltip .tt-link { pointer-events: auto; cursor: pointer; color: #93c5fd; text-decoration: underline; }
#tooltip .tt-link:hover { color: #bfdbfe; }
#tooltip .tt-open { pointer-events: auto; cursor: pointer; color: #94a3b8; margin-left: 0.15rem; text-decoration: none; }
#tooltip .tt-open:hover { color: #f1f5f9; }

@keyframes flash {
  0%, 30%  { box-shadow: 0 0 0 3px rgba(59,130,246,0.55); }
  100%     { box-shadow: 0 0 0 0 rgba(59,130,246,0); }
}

.tgt-file { margin-bottom: 2rem; }
.tgt-file h2 { font-size: 1rem; font-family: ui-monospace, monospace; background: #f1f5f9; padding: 0.5rem 0.75rem; border-radius: 4px; margin: 0 0 0.5rem 0; }
.tgt-file h2 .sub { font-size: 0.75rem; color: #64748b; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
.tgt-pre {
  margin: 0; font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
  font-size: 0.85rem; line-height: 1.55; white-space: pre-wrap; word-break: break-word;
  background: #fafafa; border: 1px solid #e5e7eb; border-radius: 4px; padding: 0.75rem 1rem;
}
.tgt-block { display: block; padding: 0.15rem 0.4rem; margin: 0.2rem -0.4rem; border-left: 4px solid transparent; }
.tgt-block.inline {
  display: inline; border-left: none; padding: 0.05em 0.1em; margin: 0; border-radius: 3px;
  box-decoration-break: clone; -webkit-box-decoration-break: clone;
}
.tgt-block.covered.cov-linked { background: rgba(34, 197, 94, 0.12); border-left-color: var(--c-linked); }
.tgt-block.covered.cov-broken { background: rgba(239, 68, 68, 0.14); border-left-color: var(--c-broken); }
.tgt-block.flash { animation: flash 1.6s ease-out; }
.tgt-markers { display: block; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; font-size: 0.72rem; margin: 0.15rem 0; }
.tgt-marker { display: inline-block; margin-right: 0.5rem; padding: 0.05rem 0.4rem 0.05rem 0.3rem; border-radius: 3px; background: #f1f5f9; color: #0f172a; cursor: pointer; }
.tgt-marker:hover { background: #dbeafe; }
.tgt-marker.tgt-broken { background: #fee2e2; color: #991b1b; }
.tgt-to-src { color: #1d4ed8; }
.tgt-to-cmp { display: inline-block; margin-right: 0.5rem; padding: 0.05rem 0.4rem; border-radius: 3px; background: #f1f5f9; color: #475569; cursor: pointer; }
.tgt-to-cmp:hover { background: #e0e7ff; color: #1e293b; }

code { background: #f1f5f9; padding: 0 .25em; border-radius: 3px;
       font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: .85rem; }
code.broken { color: #991b1b; }
code.ok { color: #166534; }
.note { color: #64748b; font-style: italic; white-space: pre-wrap; }
.empty, .file-empty { padding: 1rem; color: #64748b; font-size: 0.88rem; }

/* ---- compare view: 4 panels side by side ---- */
/* When compare is active, let the section span the whole width (the shared
   sidebar is hidden by the compare JS) and host its own 4-column grid. */
main.compare-active { grid-template-columns: 1fr; }
#view-compare { padding: 0; }
.cmp-grid {
  display: grid;
  grid-template-columns: 240px minmax(0, 1fr) minmax(0, 1fr) 240px;
  height: calc(100vh - 170px);
}
.cmp-files { background: #f9fafb; overflow-y: auto; font-size: 0.85rem; }
.cmp-files-a { border-right: 1px solid #e5e7eb; }
.cmp-files-b { border-left: 1px solid #e5e7eb; }
.cmp-files .file .sub { font-size: 0.7rem; color: #94a3b8; margin-top: 0.1rem; word-break: break-all; }
.cmp-files-head {
  position: sticky; top: 0; background: #eef2f7; padding: 0.5rem 0.85rem;
  font-weight: 600; font-size: 0.75rem; text-transform: uppercase;
  letter-spacing: 0.04em; color: #475569; border-bottom: 1px solid #e5e7eb;
}
.cmp-focus { overflow-y: auto; padding: 1rem; }
.cmp-focus-a { border-right: 1px solid #e5e7eb; background: #fcfcfd; }
.cmp-focus-b { background: #fcfcfd; }
.cmp-file-block { margin-bottom: 1.5rem; }
.cmp-file-block h3 {
  font-size: 0.85rem; font-family: ui-monospace, monospace; background: #f1f5f9;
  padding: 0.4rem 0.6rem; border-radius: 4px; margin: 0 0 0.4rem 0; position: sticky; top: 0;
}
.cmp-file-block h3 .sub { font-size: 0.7rem; color: #64748b; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
.cmp-pre {
  margin: 0; font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
  font-size: 0.8rem; line-height: 1.5; white-space: pre-wrap; word-break: break-word;
  background: #fafafa; border: 1px solid #e5e7eb; border-radius: 4px; padding: 0.6rem 0.8rem;
}
.cmp-seg {
  display: block; border-left: 4px solid transparent; padding: 0.12rem 0.35rem 0.12rem 0.5rem;
  margin: 0.3rem -0.8rem; cursor: pointer; position: relative;
}
.cmp-seg.inline {
  display: inline; border-left: none; padding: 0.05em 0.1em; margin: 0; border-radius: 3px;
  box-decoration-break: clone; -webkit-box-decoration-break: clone;
}
.cmp-seg.status-linked   { background: rgba(34, 197, 94, 0.12); border-left-color: var(--c-linked); }
.cmp-seg.status-unlinked { background: rgba(203, 213, 225, 0.25); border-left-color: var(--c-unlinked); }
.cmp-seg.status-broken   { background: rgba(239, 68, 68, 0.14); border-left-color: var(--c-broken); }
.cmp-seg.dimmed { opacity: 0.28; }
.cmp-seg.cmp-hot { box-shadow: 0 0 0 2px #3b82f6 inset; }
/* The one counterpart the cycle pointer is currently on: a stronger,
   filled-edge ring so it stands out among the other highlighted (cmp-hot)
   counterparts as you click through them. */
.cmp-seg.cmp-current { box-shadow: 0 0 0 2px #3b82f6 inset, -4px 0 0 0 #3b82f6; }
.cmp-seg.flash  { animation: flash 1.6s ease-out; }
/* Persistent "you came from Compare" ring on the segment a ↗ landed on, so
   it stays marked while you read the Source / Target view. */
.src-seg.cmp-origin, .tgt-block.cmp-origin { box-shadow: 0 0 0 2px #3b82f6 inset; }
/* Floating "↩ Back to Compare" pill. */
#cmp-return {
  position: fixed; bottom: 1.25rem; right: 1.25rem; z-index: 120;
  background: #3b82f6; color: white; padding: 0.5rem 0.9rem; border-radius: 999px;
  font-size: 0.85rem; font-weight: 600; cursor: pointer; box-shadow: 0 6px 18px rgba(0,0,0,0.25);
  opacity: 0; transform: translateY(6px); transition: opacity 0.12s, transform 0.12s; pointer-events: none;
}
#cmp-return.visible { opacity: 1; transform: none; pointer-events: auto; }
#cmp-return:hover { background: #2563eb; }

/* Zebra shading: alternate chunks get a faint neutral darken, so two
   contiguous chunks of the same status still show a visible seam. The
   darken is a translucent background-image layered ON TOP of each
   chunk's status background-color, so it composes with the linked /
   broken / unlinked tints instead of replacing them — and it never
   touches box-shadow, leaving the hover frame and flash untouched.
   `zebra-0` chunks are left untouched; only the odd ones darken. */
.segment.zebra-1,
.src-seg.zebra-1,
.tgt-block.zebra-1,
.cmp-seg.zebra-1 {
  background-image: linear-gradient(rgba(15, 23, 42, 0.16), rgba(15, 23, 42, 0.16));
}
</style>
</head>
<body>
"#;

const TAIL_TEMPLATE: &str = "</body></html>\n";

const SCRIPT: &str = r#"<script>
let activeFile = '__all__';
let currentView = 'list';
// Compare click-to-cycle state: repeated clicks on the same segment step
// through its counterparts one at a time (origin element + current index).
let cmpCycle = { origin: null, els: [], idx: 0 };
// The link a ↗ left Compare on, so "↩ Back to Compare" can restore it exactly.
let cmpReturn = null;

document.querySelectorAll('#sidebar-source .file').forEach(f => {
  f.addEventListener('click', () => {
    document.querySelectorAll('#sidebar-source .file').forEach(x => x.classList.remove('active'));
    f.classList.add('active');
    activeFile = f.dataset.file;
    applyAll();
    if (currentView === 'source' && activeFile !== '__all__') {
      const el = document.querySelector(`.src-file[data-file="${CSS.escape(activeFile)}"]`);
      if (el) el.scrollIntoView({behavior: 'smooth', block: 'start'});
    }
  });
});

document.querySelectorAll('#sidebar-target .file').forEach(f => {
  f.addEventListener('click', () => {
    document.querySelectorAll('#sidebar-target .file').forEach(x => x.classList.remove('active'));
    f.classList.add('active');
    const key = f.dataset.targetFile;
    const el = document.querySelector(`.tgt-file[data-target-file="${CSS.escape(key)}"]`);
    if (el) el.scrollIntoView({behavior: 'smooth', block: 'start'});
  });
});

document.querySelectorAll('.flt').forEach(c => c.addEventListener('change', applyAll));

document.querySelectorAll('.view-toggle button').forEach(b => b.addEventListener('click', () => {
  document.querySelectorAll('.view-toggle button').forEach(x => x.classList.remove('active'));
  b.classList.add('active');
  currentView = b.dataset.view;
  document.getElementById('view-list').classList.toggle('hidden', currentView !== 'list');
  document.getElementById('view-source').classList.toggle('hidden', currentView !== 'source');
  document.getElementById('view-target').classList.toggle('hidden', currentView !== 'target');
  document.getElementById('view-compare').classList.toggle('hidden', currentView !== 'compare');
  // Compare has its own 4-panel layout, so hide the shared sidebar entirely
  // and let the section span the full width.
  const cmp = currentView === 'compare';
  document.querySelector('main').classList.toggle('compare-active', cmp);
  document.getElementById('sidebar').classList.toggle('hidden', cmp);
  document.getElementById('sidebar-source').classList.toggle('hidden', !cmp && currentView === 'target');
  document.getElementById('sidebar-target').classList.toggle('hidden', cmp || currentView !== 'target');
  applyAll();
}));

// Drop any persistent "came from Compare" marker before setting a new one.
function clearOrigin() {
  document.querySelectorAll('.cmp-origin').forEach(x => x.classList.remove('cmp-origin'));
}
// `origin` (set when arriving via ↗) leaves a persistent blue ring on the
// landed segment, so it stays marked as "where you came from" while you read.
function jumpToSource(segid, file, origin) {
  document.querySelector('.view-toggle button[data-view="source"]').click();
  if (file) {
    const sf = document.querySelector(`.src-file[data-file="${CSS.escape(file)}"]`);
    if (sf) sf.scrollIntoView({behavior: 'smooth', block: 'start'});
  }
  const el = document.querySelector(`.src-seg[data-segid="${CSS.escape(segid)}"]`);
  if (el) {
    el.scrollIntoView({behavior: 'smooth', block: 'center'});
    el.classList.remove('flash'); void el.offsetWidth; el.classList.add('flash');
    clearOrigin();
    if (origin) el.classList.add('cmp-origin');
  }
}

function jumpToTarget(targetId, origin) {
  document.querySelector('.view-toggle button[data-view="target"]').click();
  if (targetId) {
    const block = document.querySelector(`.tgt-block[data-tgt-id="${CSS.escape(targetId)}"]`);
    if (block) {
      block.scrollIntoView({behavior: 'smooth', block: 'center'});
      block.classList.remove('flash'); void block.offsetWidth; block.classList.add('flash');
      clearOrigin();
      if (origin) block.classList.add('cmp-origin');
    }
  }
}

// Floating "↩ Back to Compare" pill, shown after a ↗ leaves Compare.
function showReturnPill() {
  let pill = document.getElementById('cmp-return');
  if (!pill) {
    pill = document.createElement('div');
    pill.id = 'cmp-return';
    pill.innerHTML = '↩ Back to Compare';
    document.body.appendChild(pill);
  }
  pill.classList.add('visible');
}
function hideReturnPill() {
  const pill = document.getElementById('cmp-return');
  if (pill) pill.classList.remove('visible');
}
// Leaving for Compare by any route (view toggle, ⇄) clears the origin marker
// and the pill, so they only mark a genuine ↗ round-trip.
document.querySelector('.view-toggle button[data-view="compare"]').addEventListener('click', () => {
  clearOrigin();
  hideReturnPill();
});

document.addEventListener('click', ev => {
  // "show link in Compare" (⇄): enter Compare and land on the link, both
  // endpoints highlighted. Offered by the Target-view markers and the
  // Source / List tooltip, so each single-segment view has a way back into
  // the side-by-side. Checked before the plainer jumps that share their row.
  const toCmp = ev.target.closest('.tgt-to-cmp');
  if (toCmp) {
    tooltip.classList.remove('visible');
    cmpJumpToLink(toCmp.dataset.aHash, toCmp.dataset.bId);
    return;
  }
  // target → source jumps
  const back = ev.target.closest('.tgt-to-src');
  if (back) {
    jumpToSource(back.dataset.segid, '');
    return;
  }
  // "open in full view" (↗): open THIS segment in its own dedicated view —
  // an A segment in the Source view, a B segment in the Target view. It does
  // not jump to the linked counterpart. Dismiss the tooltip so it doesn't
  // linger over the view we just switched to, and remember this segment so
  // "↩ Back to Compare" can bring it back highlighted in the Compare panel.
  const open = ev.target.closest('.tt-open');
  if (open) {
    tooltip.classList.remove('visible');
    if (open.dataset.openView === 'source') {
      cmpReturn = { a: open.dataset.segid, b: null };
      jumpToSource(open.dataset.segid, '', true);
    } else {
      cmpReturn = { a: null, b: open.dataset.targetId };
      jumpToTarget(open.dataset.targetId, true);
    }
    showReturnPill();
    return;
  }
  // "↩ Back to Compare": return to the segment a ↗ left on, highlighted.
  const ret = ev.target.closest('#cmp-return');
  if (ret) {
    hideReturnPill();
    if (cmpReturn) cmpJumpToLink(cmpReturn.a, cmpReturn.b);
    return;
  }
  // Forward tooltip link (List / Source views only): jump to the target.
  // In Compare the tooltip link rows are informational (navigation is by
  // clicking the segment in the panel), so they carry no jump target.
  const fwd = ev.target.closest('.tt-link[data-target-id]');
  if (fwd) {
    jumpToTarget(fwd.dataset.targetId);
    return;
  }
});

// Enter Compare and land on a single link: highlight + scroll BOTH endpoints
// (the A segment in its panel, the B segment in its). Used by the reciprocal
// control in the Source / Target views. A missing endpoint (broken link) is
// simply skipped, so a half-resolved link still shows the side that exists.
function cmpJumpToLink(aHash, bId) {
  document.querySelector('.view-toggle button[data-view="compare"]').click();
  cmpClearHot();
  const a = aHash ? document.querySelector(`.cmp-seg-a[data-cmp-a-hash="${CSS.escape(aHash)}"]`) : null;
  const b = bId ? document.querySelector(`.cmp-seg-b[data-cmp-b-id="${CSS.escape(bId)}"]`) : null;
  if (a) { a.scrollIntoView({behavior: 'smooth', block: 'center'}); cmpFlash(a); }
  if (b) { b.scrollIntoView({behavior: 'smooth', block: 'center'}); cmpFlash(b); }
}

document.querySelectorAll('.segment.clickable').forEach(card => {
  card.addEventListener('click', () => {
    const segid = card.dataset.hash;
    const file = card.dataset.file;
    document.querySelectorAll('#sidebar-source .file').forEach(x => {
      if (x.dataset.file === file) x.click();
    });
    jumpToSource(segid, file);
  });
});

function applyAll() {
  const enabled = new Set(Array.from(document.querySelectorAll('.flt:checked')).map(x => x.value));
  document.querySelectorAll('.segment').forEach(s => {
    const okStatus = enabled.has(s.dataset.status);
    const okFile = activeFile === '__all__' || s.dataset.file === activeFile;
    s.classList.toggle('hidden', !(okStatus && okFile));
  });
  document.querySelectorAll('.src-file').forEach(f => {
    f.classList.toggle('hidden', activeFile !== '__all__' && f.dataset.file !== activeFile);
  });
  document.querySelectorAll('.src-seg').forEach(s => {
    s.classList.toggle('dimmed', !enabled.has(s.dataset.status));
  });
  document.querySelectorAll('.cmp-seg').forEach(s => {
    s.classList.toggle('dimmed', !enabled.has(s.dataset.status));
  });
}
applyAll();

// ---- compare view: bidirectional A <-> B highlighting ----
function cmpClearHot() {
  document.querySelectorAll('.cmp-seg.cmp-hot').forEach(x => x.classList.remove('cmp-hot'));
  document.querySelectorAll('.cmp-seg.cmp-current').forEach(x => x.classList.remove('cmp-current'));
  // Any programmatic (re)highlight ends the click-to-cycle run, so the next
  // click on a segment starts a fresh cycle rather than advancing a stale one.
  cmpCycle = { origin: null, els: [], idx: 0 };
}
function cmpFlash(el) {
  el.classList.add('cmp-hot');
  el.classList.remove('flash'); void el.offsetWidth; el.classList.add('flash');
}
// Click a Compare segment -> highlight ALL its counterparts on the other
// side; repeated clicks on the SAME segment cycle through them one at a
// time (wrapping), scrolling each into view. `sideSel` picks the panel and
// `attr` the identity attribute of the counterparts; `linksAttr` is the
// clicked segment's list of counterpart identities.
function cmpWire(seg, linksAttr, sideSel, attr) {
  seg.addEventListener('click', () => {
    const ids = (seg.dataset[linksAttr] || '').split(/\s+/).filter(Boolean);
    const els = ids
      .map(id => document.querySelector(`${sideSel}[${attr}="${CSS.escape(id)}"]`))
      .filter(Boolean);
    if (cmpCycle.origin === seg && cmpCycle.els.length) {
      // Same segment clicked again: advance to the next counterpart.
      cmpCycle.idx = (cmpCycle.idx + 1) % cmpCycle.els.length;
    } else {
      cmpClearHot();
      cmpFlash(seg);
      els.forEach(t => t.classList.add('cmp-hot'));
      cmpCycle = { origin: seg, els, idx: 0 };
    }
    const cur = cmpCycle.els[cmpCycle.idx];
    if (cur) {
      document.querySelectorAll('.cmp-seg.cmp-current').forEach(x => x.classList.remove('cmp-current'));
      cur.classList.add('cmp-current');
      cur.scrollIntoView({behavior: 'smooth', block: 'center'});
    }
  });
}
document.querySelectorAll('.cmp-seg-a').forEach(seg =>
  cmpWire(seg, 'cmpLinks', '.cmp-seg-b', 'data-cmp-b-id'));
document.querySelectorAll('.cmp-seg-b').forEach(seg =>
  cmpWire(seg, 'cmpBack', '.cmp-seg-a', 'data-cmp-a-hash'));
// Compare file lists scroll their panel to the chosen file.
document.querySelectorAll('.cmp-files-a .file').forEach(f => {
  f.addEventListener('click', () => {
    const el = document.querySelector(`.cmp-file-block[data-cmp-file-a="${CSS.escape(f.dataset.cmpFileA)}"]`);
    if (el) el.scrollIntoView({behavior: 'smooth', block: 'start'});
  });
});
document.querySelectorAll('.cmp-files-b .file').forEach(f => {
  f.addEventListener('click', () => {
    const el = document.querySelector(`.cmp-file-block[data-cmp-file-b="${CSS.escape(f.dataset.cmpFileB)}"]`);
    if (el) el.scrollIntoView({behavior: 'smooth', block: 'start'});
  });
});

// Hover tooltips on source segments.
const tooltip = document.getElementById('tooltip');
function escHtml(s) {
  return s.replace(/[&<>"']/g, c =>
    ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
}
function renderTip(data) {
  if (data.dir === 'back') return renderBackTip(data);
  const inCompare = currentView === 'compare';
  // Header describes THIS segment. In Compare a single ↗ opens this segment
  // in its own Source view (an A segment lives in the source covering) — it
  // does not jump to the linked B counterpart.
  const openIcon = inCompare
    ? ` <span class="tt-open" title="open this segment in the Source view" data-open-view="source" data-segid="${data.hash}">↗</span>`
    : '';
  let html = `<div class="tt-title"><span class="badge ${data.status}">${data.status}</span>${escHtml(data.label || data.loc)}${openIcon}</div>`;
  html += `<div class="tt-meta">${escHtml(data.loc)} · <code>${data.hash}</code></div>`;
  if (data.message) {
    html += `<div class="tt-note">${escHtml(data.message)}</div>`;
  }
  // Links: what this segment maps to, shown for reading. Outside Compare the
  // link is clickable (jump to the target); in Compare, navigation is by
  // clicking the segment in the panel, so the rows are informational.
  if (data.links && data.links.length) {
    html += '<div class="tt-meta" style="margin-top:0.35rem">links:</div>';
    html += '<ul class="tt-links">';
    for (const l of data.links) {
      const cls = l.resolved ? '' : ' broken';
      const note = l.note ? ` <span class="note">— ${escHtml(l.note)}</span>` : '';
      if (inCompare) {
        html += `<li><code class="${cls.trim()}">${escHtml(l.conn)}</code> → <code>${l.target_hash.slice(0,12)}</code> in <code>${escHtml(l.target_covering)}</code>${note}</li>`;
      } else {
        html += `<li><span class="tt-link${cls}" data-target-id="${l.target_id}">${escHtml(l.conn)} → <code>${l.target_hash.slice(0,12)}</code> in <code>${escHtml(l.target_covering)}</code></span>${note}</li>`;
      }
    }
    html += '</ul>';
    if (!inCompare) {
      html += '<div class="tt-meta" style="margin-top:0.35rem">click → to jump to target</div>';
    }
  } else {
    html += '<div class="tt-note" style="color:#94a3b8">no outgoing links</div>';
  }
  return html;
}
// Tooltip for a B (target) segment: describes THIS segment, then lists the
// A segments pointing at it (informational). The header ↗ opens this B
// segment in its own Target view — not the counterpart A segment.
function renderBackTip(data) {
  const openIcon = ` <span class="tt-open" title="open this segment in the Target view" data-open-view="target" data-target-id="${data.target_id||''}">↗</span>`;
  let html = `<div class="tt-title"><span class="badge ${data.status}">${data.status}</span>${escHtml(data.label || data.loc)}${openIcon}</div>`;
  html += `<div class="tt-meta">${escHtml(data.loc)} · <code>${data.hash}</code></div>`;
  if (data.message) {
    html += `<div class="tt-note">${escHtml(data.message)}</div>`;
  }
  if (data.links && data.links.length) {
    html += `<div class="tt-meta" style="margin-top:0.35rem">links: ← ${data.links.length} incoming from A</div>`;
    html += '<ul class="tt-links">';
    for (const l of data.links) {
      const cls = l.resolved ? '' : ' broken';
      const note = l.note ? ` <span class="note">— ${escHtml(l.note)}</span>` : '';
      const name = l.source_label || l.source_loc || l.source_hash.slice(0,12);
      html += `<li><code class="${cls.trim()}">${escHtml(name)}</code> via ${escHtml(l.conn)}${note}</li>`;
    }
    html += '</ul>';
  } else {
    html += '<div class="tt-note" style="color:#94a3b8">no incoming links</div>';
  }
  return html;
}
function positionTip(ev) {
  const pad = 14;
  const w = tooltip.offsetWidth, h = tooltip.offsetHeight;
  let x = ev.clientX + pad, y = ev.clientY + pad;
  if (x + w > window.innerWidth - 8)  x = ev.clientX - w - pad;
  if (y + h > window.innerHeight - 8) y = ev.clientY - h - pad;
  tooltip.style.left = Math.max(8, x) + 'px';
  tooltip.style.top  = Math.max(8, y) + 'px';
}
let tipTimer = null;
function showTip(seg, ev) {
  try {
    const data = JSON.parse(seg.dataset.tip);
    tooltip.innerHTML = renderTip(data);
    positionTip(ev);
    tooltip.classList.add('visible');
  } catch (e) {}
}
function hideTipSoon() {
  if (tipTimer) clearTimeout(tipTimer);
  tipTimer = setTimeout(() => tooltip.classList.remove('visible'), 120);
}
function cancelHide() { if (tipTimer) { clearTimeout(tipTimer); tipTimer = null; } }
document.addEventListener('mouseover', ev => {
  const seg = ev.target.closest('.src-seg, .segment, .cmp-seg-a, .cmp-seg-b');
  if (seg && seg.dataset.tip) { cancelHide(); showTip(seg, ev); return; }
  if (ev.target.closest('#tooltip')) { cancelHide(); return; }
});
document.addEventListener('mouseout', ev => {
  const leftSeg = ev.target.closest('.src-seg, .segment, .cmp-seg-a, .cmp-seg-b');
  const wentToTooltip = ev.relatedTarget && ev.relatedTarget.closest && ev.relatedTarget.closest('#tooltip');
  const stillOnSeg = ev.relatedTarget && ev.relatedTarget.closest && ev.relatedTarget.closest('.src-seg, .segment, .cmp-seg-a, .cmp-seg-b');
  if (leftSeg && !wentToTooltip && !stillOnSeg) hideTipSoon();
  if (ev.target.closest('#tooltip') && (!ev.relatedTarget || (!ev.relatedTarget.closest('#tooltip') && !ev.relatedTarget.closest('.src-seg, .segment, .cmp-seg-a, .cmp-seg-b')))) {
    hideTipSoon();
  }
});
</script>
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::Link;
    use crate::segment;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_repo(tag: &str) -> Repo {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("covmap-html-{tag}-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        Repo::init(&root).unwrap()
    }

    fn entry(hash: &str, file: &str, line: usize, name: &str) -> CoveringEntry {
        CoveringEntry {
            hash: hash.to_string(),
            file: file.to_string(),
            a: line,
            a_col: None,
            b: line,
            b_col: None,
            label: name.to_string(),
            name: Some(name.to_string()),
            message: None,
        }
    }

    fn opening_span<'a>(html: &'a str, attribute: &str) -> &'a str {
        let attribute_start = html.find(attribute).unwrap();
        let tag_start = html[..attribute_start].rfind("<span").unwrap();
        let tag_end = attribute_start + html[attribute_start..].find('>').unwrap() + 1;
        &html[tag_start..tag_end]
    }

    #[test]
    fn multiline_notes_remain_literal_text_for_pre_wrapped_rendering() {
        assert_eq!(esc("first\n<second>"), "first\n&lt;second&gt;");
        assert!(HEAD_TEMPLATE.contains(".note {"));
        assert!(HEAD_TEMPLATE.contains("white-space: pre-wrap"));
    }

    #[test]
    fn duplicate_target_hashes_keep_distinct_navigation_occurrences() {
        let repo = temp_repo("duplicate-targets");
        fs::write(repo.root.join("help.txt"), "status help\n").unwrap();
        fs::write(repo.root.join("tutorial.md"), "covmap status\n".repeat(3)).unwrap();

        let source_hash = segment::hash_of("status help\n");
        let target_hash = segment::hash_of("covmap status\n");
        Covering {
            name: "source".to_string(),
            entries: vec![entry(&source_hash, "help.txt", 1, "status")],
        }
        .write(&repo)
        .unwrap();
        Covering {
            name: "target".to_string(),
            entries: (1..=3)
                .map(|line| entry(&target_hash, "tutorial.md", line, &format!("status-{line}")))
                .collect(),
        }
        .write(&repo)
        .unwrap();
        Connection {
            name: "map".to_string(),
            source_covering: "source".to_string(),
            target_peer: ".".to_string(),
            target_covering: "target".to_string(),
            links: (1..=3)
                .map(|line| Link {
                    a: "@status".to_string(),
                    b: format!("@status-{line}"),
                    note: format!("occurrence-{line}"),
                })
                .collect(),
        }
        .write(&repo)
        .unwrap();

        let html = render(&repo, Some("source"), Some("map")).unwrap();

        assert!(html.contains("data-cmp-links=\"t0-0 t0-1 t0-2\""));
        assert!(html.contains("3 incoming link(s)"));
        for line in 1..=3 {
            let target_id = format!("t0-{}", line - 1);
            assert_eq!(
                html.matches(&format!("data-cmp-b-id=\"{target_id}\""))
                    .count(),
                1
            );
            assert_eq!(
                html.matches(&format!("data-tgt-id=\"{target_id}\""))
                    .count(),
                1
            );
            let tag = opening_span(&html, &format!("data-cmp-b-id=\"{target_id}\""));
            assert!(tag.contains(&format!("occurrence-{line}")));
            for other in (1..=3).filter(|other| *other != line) {
                assert!(!tag.contains(&format!("occurrence-{other}")));
            }
        }
        assert!(SCRIPT.contains("cmpWire(seg, 'cmpLinks', '.cmp-seg-b', 'data-cmp-b-id')"));

        fs::remove_dir_all(repo.root).unwrap();
    }
}
