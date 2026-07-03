//! Minimal line-level diff for drift remapping.
//!
//! `line_map(old, new)` returns, for each line of `old`, the index of
//! the line in `new` it is preserved as (`Some(j)`), or `None` if that
//! line was changed or deleted. The mapping is monotonic and injective:
//! preserved lines keep their relative order. This is the
//! position-carry-forward primitive behind `recut --remap` — boundaries
//! that fall on preserved lines move deterministically with them; the
//! rest are reported as unresolved.
//!
//! The algorithm trims the common prefix and suffix (the cheap, common
//! case for a localized edit) and runs an LCS dynamic program only on
//! the residual middle. A size guard keeps a pathological whole-file
//! rewrite from allocating a huge DP table: above the cap the middle is
//! treated as fully changed (no matches), which is safe — it just means
//! more boundaries land in changed regions and are reported unresolved.

/// Maximum DP cells (old_middle × new_middle) before we bail out and
/// treat the middle as fully changed.
const DP_CELL_CAP: usize = 4_000_000;

/// For each line of `old`, the matching line index in `new` (when the
/// line is preserved unchanged), else `None`.
pub fn line_map(old: &[&str], new: &[&str]) -> Vec<Option<usize>> {
    let n = old.len();
    let m = new.len();
    let mut out = vec![None; n];

    // Common prefix.
    let mut p = 0;
    while p < n && p < m && old[p] == new[p] {
        out[p] = Some(p);
        p += 1;
    }
    // Common suffix (over what's left after the prefix).
    let mut s = 0;
    while s < (n - p) && s < (m - p) && old[n - 1 - s] == new[m - 1 - s] {
        out[n - 1 - s] = Some(m - 1 - s);
        s += 1;
    }

    let o_lo = p;
    let o_hi = n - s; // exclusive
    let n_lo = p;
    let n_hi = m - s; // exclusive
    let ol = o_hi - o_lo;
    let nl = n_hi - n_lo;
    if ol == 0 || nl == 0 {
        return out; // one middle is empty: pure insert or delete
    }
    if ol * nl > DP_CELL_CAP {
        return out; // too large: leave the middle unmatched
    }

    // LCS DP over the middles. dp[i][j] = LCS length of old_mid[i..],
    // new_mid[j..]. Backtrack to recover matched pairs.
    let mut dp = vec![vec![0u32; nl + 1]; ol + 1];
    for i in (0..ol).rev() {
        for j in (0..nl).rev() {
            dp[i][j] = if old[o_lo + i] == new[n_lo + j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < ol && j < nl {
        if old[o_lo + i] == new[n_lo + j] {
            out[o_lo + i] = Some(n_lo + j);
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<&str> {
        // Mirror read_text_lines: keep trailing '\n' on each line.
        let mut v: Vec<&str> = s.split_inclusive('\n').collect();
        if v.last().is_some_and(|l| l.is_empty()) {
            v.pop();
        }
        v
    }

    #[test]
    fn identical_files_map_one_to_one() {
        let o = lines("a\nb\nc\n");
        let n = lines("a\nb\nc\n");
        assert_eq!(line_map(&o, &n), vec![Some(0), Some(1), Some(2)]);
    }

    #[test]
    fn insertion_in_middle_shifts_suffix() {
        // Insert two lines after "b".
        let o = lines("a\nb\nc\n");
        let n = lines("a\nb\nX\nY\nc\n");
        // a->0, b->1, c->4 (shifted by the two inserted lines).
        assert_eq!(line_map(&o, &n), vec![Some(0), Some(1), Some(4)]);
    }

    #[test]
    fn deletion_marks_gone_lines_none() {
        let o = lines("a\nb\nc\nd\n");
        let n = lines("a\nd\n");
        assert_eq!(line_map(&o, &n), vec![Some(0), None, None, Some(1)]);
    }

    #[test]
    fn changed_line_is_unmatched_neighbors_preserved() {
        let o = lines("a\nb\nc\n");
        let n = lines("a\nB!\nc\n");
        let map = line_map(&o, &n);
        assert_eq!(map[0], Some(0));
        assert_eq!(map[1], None); // the changed line
        assert_eq!(map[2], Some(2));
    }
}
