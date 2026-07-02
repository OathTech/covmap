//! Fingerprinting: propose where a lost slot's content went.
//!
//! Diff-remap (`covmap recut --remap`) resolves a slot only when its
//! boundaries fall on lines preserved by the edit. When a slot was moved
//! or its anchor lines rewritten, the remap reports it as *unresolved*.
//! Fingerprinting is the fallback for exactly those slots: given the
//! slot's recorded content and the new file, it ranks candidate
//! locations by similarity so a human (or agent) can confirm one.
//!
//! It is **propose-only**: it never mutates a covering or a connection.
//! A similarity score is a likelihood, not a proof — boilerplate can
//! score high (false positive) and a heavy rewrite can score low (false
//! negative) — so the decision to relink is always left to the caller.
//!
//! Scoring is layered, cheapest signal winning ties at the top:
//!   (a) exact  — a line window equals the target verbatim        → 1.00
//!   (b) normalized — equal after lowercasing + whitespace-collapse → 0.98
//!   (c) fuzzy  — word-trigram Jaccard overlap of the two texts    → [0,1)
//! No external dependencies; all self-contained.

use std::collections::HashSet;

/// A proposed location in the new file: an inclusive 1-indexed line
/// range `a..=b` and a similarity score in `[0,1]`.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub a: usize,
    pub b: usize,
    pub score: f64,
}

/// Lowercase and collapse all runs of whitespace to a single space,
/// trimmed. The normalization used for the (b) signal and tokenization.
pub fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            in_ws = true;
        } else {
            if in_ws && !out.is_empty() {
                out.push(' ');
            }
            in_ws = false;
            out.extend(c.to_lowercase());
        }
    }
    out
}

/// Word-trigram shingle set of `s` (after normalization). With fewer
/// than three tokens, falls back to the individual tokens so short
/// slots still produce a usable set.
fn shingles(s: &str) -> HashSet<String> {
    // normalize() single-spaces the text, so splitting on ' ' yields the
    // exact token list.
    let norm = normalize(s);
    let toks: Vec<&str> = norm.split(' ').filter(|t| !t.is_empty()).collect();
    let mut set = HashSet::new();
    if toks.len() < 3 {
        for t in &toks {
            set.insert((*t).to_string());
        }
        return set;
    }
    for w in toks.windows(3) {
        set.insert(w.join(" "));
    }
    set
}

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let inter = a.intersection(b).count();
    let union = a.len() + b.len() - inter;
    if union == 0 {
        0.0
    } else {
        inter as f64 / union as f64
    }
}

/// Rank candidate locations of `target` within `n_lines` (each line
/// retaining its trailing newline). Returns up to `max_results`
/// candidates sorted by descending score, then by position. Windows of
/// line length near the target's are scored; exact and normalized
/// matches are promoted above fuzzy ones.
pub fn locate(target: &str, n_lines: &[String], max_results: usize) -> Vec<Candidate> {
    let total = n_lines.len();
    if total == 0 {
        return Vec::new();
    }
    let target_lines = target.split_inclusive('\n').count().max(1);
    let target_norm = normalize(target);
    let target_shingles = shingles(target);

    // Window lengths to try: the target's line count and ±2, clamped.
    let mut widths: Vec<usize> = Vec::new();
    let lo = target_lines.saturating_sub(2).max(1);
    let hi = (target_lines + 2).min(total);
    for w in lo..=hi {
        widths.push(w);
    }

    let mut best: Vec<Candidate> = Vec::new();
    for &w in &widths {
        if w == 0 || w > total {
            continue;
        }
        for start in 0..=(total - w) {
            let window: String = n_lines[start..start + w].concat();
            let score = if window == target {
                1.0
            } else if normalize(&window) == target_norm {
                0.98
            } else {
                jaccard(&shingles(&window), &target_shingles)
            };
            best.push(Candidate {
                a: start + 1,
                b: start + w,
                score,
            });
        }
    }
    // Sort by score desc, then by line position asc, then shorter span.
    best.sort_by(|x, y| {
        y.score
            .partial_cmp(&x.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(x.a.cmp(&y.a))
            .then((x.b - x.a).cmp(&(y.b - y.a)))
    });
    // Deduplicate overlapping near-identical proposals: keep the first
    // (highest-scoring) candidate and drop any later one that overlaps it.
    let mut chosen: Vec<Candidate> = Vec::new();
    for c in best {
        if chosen.iter().any(|k| ranges_overlap(k.a, k.b, c.a, c.b)) {
            continue;
        }
        chosen.push(c);
        if chosen.len() >= max_results {
            break;
        }
    }
    chosen
}

fn ranges_overlap(a1: usize, b1: usize, a2: usize, b2: usize) -> bool {
    a1 <= b2 && a2 <= b1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<String> {
        s.split_inclusive('\n').map(|l| l.to_string()).collect()
    }

    #[test]
    fn normalize_collapses_ws_and_lowercases() {
        assert_eq!(normalize("  Hello   WORLD\n"), "hello world");
    }

    #[test]
    fn exact_match_scores_one_and_locates() {
        let n = lines("alpha\nbeta line\ngamma\ndelta\n");
        let c = locate("beta line\n", &n, 3);
        assert!(!c.is_empty());
        assert_eq!(c[0].score, 1.0);
        assert_eq!((c[0].a, c[0].b), (2, 2));
    }

    #[test]
    fn moved_block_is_top_candidate() {
        // Target lived earlier; in N it sits at lines 4-5.
        let target = "The cache evicts the least-recently-used entry.\n";
        let n = lines(
            "intro\nmore intro\nsetup\nThe cache evicts the least-recently-used entry.\nepilogue\n",
        );
        let c = locate(target, &n, 3);
        assert_eq!(c[0].score, 1.0);
        assert_eq!(c[0].a, 4);
    }

    #[test]
    fn lightly_edited_block_scores_high_but_below_one() {
        let target = "the least-recently-used entry is dropped when the cache is full\n";
        let n = lines(
            "preamble\nthe least-recently-used entry is evicted when the cache is full (lru)\ntail\n",
        );
        let c = locate(target, &n, 3);
        assert!(c[0].score > 0.4, "score was {}", c[0].score);
        assert!(c[0].score < 1.0);
        assert_eq!(c[0].a, 2);
    }

    #[test]
    fn unrelated_text_scores_low() {
        let target = "quantum chromodynamics and the strong nuclear force\n";
        let n = lines("the cat sat on the mat\na dog barked loudly\n");
        let c = locate(target, &n, 3);
        assert!(c[0].score < 0.2, "score was {}", c[0].score);
    }
}
