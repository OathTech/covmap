//! Shared message normalization and durable/plumbing text codecs.

use std::io;

/// Normalize user-supplied message text to the canonical stored form.
pub fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim_end_matches('\n')
        .to_string()
}

/// Join repeated `-m` values as paragraphs, then normalize once.
pub fn normalize_values(values: &[String]) -> String {
    normalize(&values.join("\n\n"))
}

/// Append a length-prefixed text stanza. `header` excludes the line count.
///
/// `body` must already use canonical LF line endings. Message-input callers
/// establish that invariant with `normalize` / `normalize_values` before
/// durable serialization.
pub fn push_stanza(out: &mut String, header: &str, body: &str) {
    let lines: Vec<&str> = body.split('\n').collect();
    out.push_str(&format!("{header} {}\n", lines.len()));
    for line in lines {
        out.push_str(line);
        out.push('\n');
    }
}

/// Return whether a physical line is a note stanza header candidate.
pub fn is_note_header(line: &str) -> bool {
    line.strip_prefix("# note")
        .is_some_and(|rest| rest.is_empty() || rest.chars().next().is_some_and(char::is_whitespace))
}

/// Parse the one non-negative line-count field after a stanza prefix.
pub fn parse_line_count(line: &str, prefix: &str) -> io::Result<usize> {
    let rest = line.strip_prefix(prefix).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("malformed stanza header: {line}"),
        )
    })?;
    let mut fields = rest.split_whitespace();
    let count = fields
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("malformed stanza header: {line}"),
            )
        })?;
    if fields.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("malformed stanza header: {line}"),
        ));
    }
    Ok(count)
}

/// Read exactly `count` physical body lines beginning at `body_start`.
pub fn read_body(
    lines: &[&str],
    body_start: usize,
    count: usize,
    context: &str,
) -> io::Result<(String, usize)> {
    let body_end = body_start.checked_add(count).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{context} line count overflows"),
        )
    })?;
    if body_end > lines.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{context} claims {count} lines past end of ref"),
        ));
    }
    Ok((lines[body_start..body_end].join("\n"), body_end))
}

/// Escape a free-form field for one-physical-line TAB-delimited plumbing.
pub fn escape_field(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_is_idempotent_and_normalizes_line_endings() {
        let cases = [
            ("", ""),
            ("\n\n", ""),
            ("a\n\n", "a"),
            ("\n\na", "\n\na"),
            ("a\r\nb\r\n", "a\nb"),
            ("a\rb\r", "a\nb"),
        ];
        for (input, expected) in cases {
            let normalized = normalize(input);
            assert_eq!(normalized, expected);
            assert_eq!(normalize(&normalized), normalized);
        }
    }

    #[test]
    fn repeated_values_become_ordered_paragraphs() {
        let values = vec!["one".into(), "two\r\nlines".into(), "three\n".into()];
        assert_eq!(
            normalize_values(&values),
            "one\n\ntwo\nlines\n\nthree".to_string()
        );
    }

    #[test]
    fn body_codec_preserves_header_looking_and_blank_lines() {
        let body = "\n# note 2\n# message 0 1";
        let mut raw = String::new();
        push_stanza(&mut raw, "# note", body);
        let lines: Vec<&str> = raw.lines().collect();
        let count = parse_line_count(lines[0], "# note ").unwrap();
        let (decoded, end) = read_body(&lines, 1, count, "note stanza").unwrap();
        assert_eq!(decoded, body);
        assert_eq!(end, lines.len());
    }

    #[test]
    fn note_header_detection_does_not_claim_unrelated_comments() {
        assert!(is_note_header("# note"));
        assert!(is_note_header("# note 2"));
        assert!(is_note_header("# note\t2"));
        assert!(!is_note_header("# noteworthy"));
        assert!(!is_note_header("# notes 2"));
    }

    #[test]
    fn plumbing_escape_is_one_line_and_unambiguous() {
        assert_eq!(escape_field("a\\b\tc\rd\ne"), r"a\\b\tc\rd\ne");
    }
}
