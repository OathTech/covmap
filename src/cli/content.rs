//! Shared segment-content formatting for inspection and iterator views.

use crate::cache;
use crate::repo::Repo;
use crate::segment;
use std::collections::BTreeMap;
use std::io::{self, Write};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ContentFormat {
    None,
    Plain,
    Numbered,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ContentRange {
    pub first_line: usize,
    pub first_col: Option<usize>,
    pub last_line: usize,
    pub last_col: Option<usize>,
}

impl ContentRange {
    pub(super) fn whole_lines(first_line: usize, last_line: usize) -> Self {
        Self {
            first_line,
            first_col: None,
            last_line,
            last_col: None,
        }
    }

    fn needs_suffix_width(self) -> bool {
        self.last_col.is_some()
    }
}

/// Source text cached for one CLI invocation. Multi-segment region, label, and
/// iterator views may inspect many ranges in one file; reading that file once
/// is enough to establish every exact suffix width.
#[derive(Default)]
pub(super) struct SourceContext {
    lines_by_file: BTreeMap<String, Option<Vec<String>>>,
}

impl SourceContext {
    pub(super) fn matching_widths(
        &mut self,
        repo: &Repo,
        file: &str,
        range: ContentRange,
        recorded: &str,
    ) -> Option<Vec<usize>> {
        if !range.needs_suffix_width() {
            return None;
        }
        let lines = self
            .lines_by_file
            .entry(file.to_string())
            .or_insert_with(|| cache::source_lines(repo, file).ok())
            .as_deref()?;
        matching_source_widths(lines, range, recorded)
    }
}

fn matching_source_widths(
    lines: &[String],
    range: ContentRange,
    recorded: &str,
) -> Option<Vec<usize>> {
    let current = segment::extract_content(
        lines,
        range.first_line,
        range.first_col,
        range.last_line,
        range.last_col,
    )?;
    if current != recorded {
        return None;
    }
    let start = range.first_line.checked_sub(1)?;
    let selected = lines.get(start..range.last_line)?;
    Some(
        selected
            .iter()
            .map(|line| visible_line(line).chars().count())
            .collect(),
    )
}

pub(super) fn content_line_count(content: &str) -> usize {
    content.split_inclusive('\n').count().max(1)
}

pub(super) fn write_content_block(
    out: &mut impl Write,
    handle: Option<&str>,
    content: &str,
    range: ContentRange,
    format: ContentFormat,
    source_widths: Option<&[usize]>,
) -> io::Result<()> {
    match format {
        ContentFormat::None => Ok(()),
        ContentFormat::Plain => {
            write_content_header(out, handle)?;
            out.write_all(content.as_bytes())?;
            if !content.ends_with('\n') {
                writeln!(out)?;
            }
            Ok(())
        }
        ContentFormat::Numbered => {
            write_content_header(out, handle)?;
            write_numbered(out, content, range, source_widths)
        }
    }
}

fn write_content_header(out: &mut impl Write, handle: Option<&str>) -> io::Result<()> {
    match handle {
        Some(handle) => writeln!(out, "--- content {handle} ---"),
        None => writeln!(out, "--- content ---"),
    }
}

fn write_numbered(
    out: &mut impl Write,
    content: &str,
    range: ContentRange,
    source_widths: Option<&[usize]>,
) -> io::Result<()> {
    let line_count = content_line_count(content);
    let rendered_last = range.first_line + line_count - 1;
    let width = rendered_last.max(range.last_line).to_string().len();
    let lines: Vec<&str> = if content.is_empty() {
        vec![""]
    } else {
        content.split_inclusive('\n').collect()
    };

    for (offset, raw_line) in lines.into_iter().enumerate() {
        let line = visible_line(raw_line);
        let line_number = range.first_line + offset;
        let prefix = if offset == 0 {
            range.first_col.unwrap_or(1).saturating_sub(1)
        } else {
            0
        };
        let is_last = offset + 1 == line_count;
        let suffix_width = if is_last && range.last_col.is_some() {
            source_widths
                .and_then(|widths| widths.get(offset))
                .map(|source_width| {
                    source_width.saturating_sub(range.last_col.unwrap_or(*source_width))
                })
        } else {
            Some(0)
        };
        let suffix = suffix_width.unwrap_or(0);
        let masked = format!("{}{}{}", ".".repeat(prefix), line, ".".repeat(suffix));
        if masked.is_empty() {
            writeln!(out, "{line_number:>width$} |", width = width)?;
        } else {
            writeln!(out, "{line_number:>width$} | {masked}", width = width)?;
        }
        if suffix_width.is_none() {
            writeln!(
                out,
                "{:>width$} ! covmap: REST-OF-LINE-UNKNOWN",
                "",
                width = width
            )?;
        }
    }
    Ok(())
}

fn visible_line(line: &str) -> &str {
    let line = line.strip_suffix('\n').unwrap_or(line);
    line.strip_suffix('\r').unwrap_or(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_repo(tag: &str) -> Repo {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "covmap-content-{tag}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        Repo { root }
    }

    #[test]
    fn numbered_masks_char_bounded_segments_to_source_width() {
        let mut out = Vec::new();
        write_content_block(
            &mut out,
            Some("partial"),
            "beta gamma\nsecon",
            ContentRange {
                first_line: 1,
                first_col: Some(7),
                last_line: 2,
                last_col: Some(5),
            },
            ContentFormat::Numbered,
            Some(&[16, 16]),
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "--- content partial ---\n1 | ......beta gamma\n2 | secon...........\n"
        );
    }

    #[test]
    fn numbered_reports_unknown_rest_of_line_outside_source_content() {
        let mut out = Vec::new();
        write_content_block(
            &mut out,
            None,
            "beta",
            ContentRange {
                first_line: 8,
                first_col: Some(7),
                last_line: 8,
                last_col: Some(10),
            },
            ContentFormat::Numbered,
            None,
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "--- content ---\n8 | ......beta\n  ! covmap: REST-OF-LINE-UNKNOWN\n"
        );
    }

    #[test]
    fn exact_one_column_mask_differs_from_unknown_rest_of_line() {
        let range = ContentRange {
            first_line: 8,
            first_col: Some(7),
            last_line: 8,
            last_col: Some(10),
        };
        let mut exact = Vec::new();
        write_content_block(
            &mut exact,
            None,
            "beta",
            range,
            ContentFormat::Numbered,
            Some(&[11]),
        )
        .unwrap();
        let mut unknown = Vec::new();
        write_content_block(
            &mut unknown,
            None,
            "beta",
            range,
            ContentFormat::Numbered,
            None,
        )
        .unwrap();

        assert_eq!(
            String::from_utf8(exact).unwrap(),
            "--- content ---\n8 | ......beta.\n"
        );
        assert_eq!(
            String::from_utf8(unknown).unwrap(),
            "--- content ---\n8 | ......beta\n  ! covmap: REST-OF-LINE-UNKNOWN\n"
        );
    }

    #[test]
    fn numbered_aligns_empty_and_two_digit_lines() {
        let mut out = Vec::new();
        write_content_block(
            &mut out,
            None,
            "alpha\n\nbeta\n",
            ContentRange::whole_lines(8, 10),
            ContentFormat::Numbered,
            None,
        )
        .unwrap();
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains(" 8 | alpha\n 9 |\n10 | beta\n"));
        assert!(!output.lines().any(|line| line.ends_with(' ')));
    }

    #[test]
    fn source_context_reuses_matching_file_snapshot() {
        let repo = temp_repo("widths");
        fs::write(
            repo.root.join("sample.txt"),
            "alpha beta gamma\nsecond line here\n",
        )
        .unwrap();
        let range = ContentRange {
            first_line: 1,
            first_col: Some(7),
            last_line: 2,
            last_col: Some(5),
        };
        let mut context = SourceContext::default();
        assert_eq!(
            context.matching_widths(&repo, "sample.txt", range, "beta gamma\nsecon"),
            Some(vec![16, 16])
        );
        fs::remove_file(repo.root.join("sample.txt")).unwrap();
        let second_range = ContentRange {
            first_line: 2,
            first_col: Some(1),
            last_line: 2,
            last_col: Some(5),
        };
        assert_eq!(
            context.matching_widths(&repo, "sample.txt", second_range, "secon"),
            Some(vec![16])
        );
        assert_eq!(
            context.matching_widths(&repo, "sample.txt", range, "recorded before drift"),
            None
        );
        fs::remove_dir_all(repo.root).unwrap();
    }
}
