//! A line-by-line diff, rendered for a human reading `cargo test` output.
//!
//! A UI test that fails prints *why* it failed. Printing the expected block
//! and the actual block one after the other and leaving the reader to spot
//! the difference is how expectation files rot: nobody reads them carefully,
//! so everybody blesses. The diff has to point at the line.
//!
//! The algorithm is a plain longest-common-subsequence over lines. Diagnostic
//! output is a handful of lines, so the quadratic table is free; past
//! `LCS_LINE_LIMIT` lines it falls back to a positional comparison rather
//! than allocating a huge table.

/// Above this many lines on either side, fall back to a positional diff.
const LCS_LINE_LIMIT: usize = 2_000;

/// One line of the diff, with the 1-based line numbers it had on each side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change<'a> {
    /// Present in both, at `expected_line` and `actual_line`.
    Same { expected_line: usize, actual_line: usize, text: &'a str },
    /// Present only in the expectation file.
    Removed { expected_line: usize, text: &'a str },
    /// Present only in the output just produced.
    Added { actual_line: usize, text: &'a str },
}

impl Change<'_> {
    pub fn is_same(&self) -> bool {
        matches!(self, Change::Same { .. })
    }
}

/// The line-level edit script turning `expected` into `actual`.
pub fn changes<'a>(expected: &'a str, actual: &'a str) -> Vec<Change<'a>> {
    let left: Vec<&str> = expected.lines().collect();
    let right: Vec<&str> = actual.lines().collect();

    if left.len() > LCS_LINE_LIMIT || right.len() > LCS_LINE_LIMIT {
        return positional(&left, &right);
    }
    lcs(&left, &right)
}

/// Whether the two texts are identical line for line.
pub fn is_identical(expected: &str, actual: &str) -> bool {
    expected == actual
}

/// Render the edit script as text, with a line-number gutter.
///
/// Columns: the expectation's line number, the actual output's line number,
/// a marker (`-` removed, `+` added, blank for context) and the text.
pub fn render(expected: &str, actual: &str) -> String {
    let changes = changes(expected, actual);
    let width = gutter_width(&changes);

    let mut out = String::new();
    out.push_str("--- expected (the .stderr file on disk)\n");
    out.push_str("+++ actual (what compile() just returned)\n");

    for change in &changes {
        match *change {
            Change::Same { expected_line, actual_line, text } => {
                push_row(&mut out, width, Some(expected_line), Some(actual_line), ' ', text);
            }
            Change::Removed { expected_line, text } => {
                push_row(&mut out, width, Some(expected_line), None, '-', text);
            }
            Change::Added { actual_line, text } => {
                push_row(&mut out, width, None, Some(actual_line), '+', text);
            }
        }
    }

    if changes.is_empty() {
        out.push_str("(both sides are empty)\n");
    }
    out
}

fn push_row(
    out: &mut String,
    width: usize,
    expected_line: Option<usize>,
    actual_line: Option<usize>,
    marker: char,
    text: &str,
) {
    let left = expected_line.map(|n| n.to_string()).unwrap_or_default();
    let right = actual_line.map(|n| n.to_string()).unwrap_or_default();
    out.push_str(&format!("{left:>width$} {right:>width$} {marker} {text}\n"));
}

fn gutter_width(changes: &[Change<'_>]) -> usize {
    let mut widest = 1;
    for change in changes {
        let (left, right) = match *change {
            Change::Same { expected_line, actual_line, .. } => (expected_line, actual_line),
            Change::Removed { expected_line, .. } => (expected_line, 0),
            Change::Added { actual_line, .. } => (0, actual_line),
        };
        widest = widest.max(digits(left)).max(digits(right));
    }
    widest
}

fn digits(n: usize) -> usize {
    if n == 0 {
        1
    } else {
        (n as f64).log10().floor() as usize + 1
    }
}

/// Longest common subsequence over lines, then walk the table back.
fn lcs<'a>(left: &[&'a str], right: &[&'a str]) -> Vec<Change<'a>> {
    let rows = left.len() + 1;
    let cols = right.len() + 1;
    let mut table = vec![0u32; rows * cols];

    for i in (0..left.len()).rev() {
        for j in (0..right.len()).rev() {
            table[i * cols + j] = if left[i] == right[j] {
                table[(i + 1) * cols + (j + 1)] + 1
            } else {
                table[(i + 1) * cols + j].max(table[i * cols + (j + 1)])
            };
        }
    }

    let mut changes = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < left.len() && j < right.len() {
        if left[i] == right[j] {
            changes.push(Change::Same {
                expected_line: i + 1,
                actual_line: j + 1,
                text: left[i],
            });
            i += 1;
            j += 1;
        } else if table[(i + 1) * cols + j] >= table[i * cols + (j + 1)] {
            changes.push(Change::Removed { expected_line: i + 1, text: left[i] });
            i += 1;
        } else {
            changes.push(Change::Added { actual_line: j + 1, text: right[j] });
            j += 1;
        }
    }
    while i < left.len() {
        changes.push(Change::Removed { expected_line: i + 1, text: left[i] });
        i += 1;
    }
    while j < right.len() {
        changes.push(Change::Added { actual_line: j + 1, text: right[j] });
        j += 1;
    }
    changes
}

/// The fallback for very large inputs: compare line `n` with line `n`.
fn positional<'a>(left: &[&'a str], right: &[&'a str]) -> Vec<Change<'a>> {
    let mut changes = Vec::new();
    let shared = left.len().min(right.len());
    for n in 0..shared {
        if left[n] == right[n] {
            changes.push(Change::Same {
                expected_line: n + 1,
                actual_line: n + 1,
                text: left[n],
            });
        } else {
            changes.push(Change::Removed { expected_line: n + 1, text: left[n] });
            changes.push(Change::Added { actual_line: n + 1, text: right[n] });
        }
    }
    for (n, text) in left.iter().enumerate().skip(shared) {
        changes.push(Change::Removed { expected_line: n + 1, text });
    }
    for (n, text) in right.iter().enumerate().skip(shared) {
        changes.push(Change::Added { actual_line: n + 1, text });
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_text_is_all_context() {
        let changes = changes("a\nb\n", "a\nb\n");
        assert_eq!(changes.len(), 2);
        assert!(changes.iter().all(Change::is_same));
    }

    #[test]
    fn a_replaced_line_becomes_one_removal_and_one_addition() {
        let changes = changes("a\nb\nc\n", "a\nB\nc\n");
        assert_eq!(
            changes,
            vec![
                Change::Same { expected_line: 1, actual_line: 1, text: "a" },
                Change::Removed { expected_line: 2, text: "b" },
                Change::Added { actual_line: 2, text: "B" },
                Change::Same { expected_line: 3, actual_line: 3, text: "c" },
            ]
        );
    }

    #[test]
    fn an_inserted_line_is_only_an_addition() {
        let changes = changes("a\nc\n", "a\nb\nc\n");
        assert_eq!(
            changes,
            vec![
                Change::Same { expected_line: 1, actual_line: 1, text: "a" },
                Change::Added { actual_line: 2, text: "b" },
                Change::Same { expected_line: 2, actual_line: 3, text: "c" },
            ]
        );
    }

    #[test]
    fn a_deleted_line_is_only_a_removal() {
        let changes = changes("a\nb\nc\n", "a\nc\n");
        assert_eq!(
            changes,
            vec![
                Change::Same { expected_line: 1, actual_line: 1, text: "a" },
                Change::Removed { expected_line: 2, text: "b" },
                Change::Same { expected_line: 3, actual_line: 2, text: "c" },
            ]
        );
    }

    #[test]
    fn an_empty_expectation_is_all_additions() {
        let changes = changes("", "a\nb\n");
        assert_eq!(
            changes,
            vec![
                Change::Added { actual_line: 1, text: "a" },
                Change::Added { actual_line: 2, text: "b" },
            ]
        );
    }

    #[test]
    fn the_rendering_has_a_header_a_marker_and_a_line_number() {
        let rendered = render("a\nb\n", "a\nB\n");
        assert!(rendered.contains("--- expected"), "{rendered}");
        assert!(rendered.contains("+++ actual"), "{rendered}");
        assert!(rendered.lines().any(|l| l.contains("- b")), "{rendered}");
        assert!(rendered.lines().any(|l| l.contains("+ B")), "{rendered}");
        assert!(rendered.lines().any(|l| l.contains('2')), "{rendered}");
    }

    #[test]
    fn the_gutter_widens_with_the_line_count() {
        let expected: String = (1..=12).map(|n| format!("line {n}\n")).collect();
        let mut actual = expected.clone();
        actual.push_str("line 13\n");
        let rendered = render(&expected, &actual);
        assert!(rendered.lines().any(|l| l.contains("13 + line 13")), "{rendered}");
    }

    #[test]
    fn the_positional_fallback_keeps_the_line_count() {
        let left: Vec<&str> = vec!["a", "b", "c"];
        let right: Vec<&str> = vec!["a", "x"];
        let changes = positional(&left, &right);
        assert_eq!(changes.len(), 4);
        assert!(changes[0].is_same());
    }
}
