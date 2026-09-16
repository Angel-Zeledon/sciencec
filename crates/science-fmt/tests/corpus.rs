//! The acceptance check for the formatter, over every program in `examples/`.
//!
//! Three properties, and they matter more than any formatting rule:
//!
//! 1. **Idempotence.** `fmt(fmt(x)) == fmt(x)`. This is gate E1 of
//!    `docs/superpowers/design/self-hosting.md` stated as a test.
//! 2. **Parse-preservation.** The syntax tree of `fmt(x)` is identical to the
//!    tree of `x`, modulo spans. Science delimits blocks by indentation, so a
//!    formatter that gets the indentation wrong does not produce ugly code, it
//!    produces a different program; this is the property that says it did not.
//! 3. **Nothing is lost.** Every comment in the input is in the output, in the
//!    same order, with the same text. A comment is not in the tree, so
//!    property 2 cannot see it, and a formatter that deleted every comment in
//!    the corpus would pass 1 and 2 with room to spare.
//!
//! The corpus is the only place these can be checked against real indentation,
//! real comments and real line continuations at once. `examples/README.md`
//! states the contract those files live under, and `science-parser`'s own
//! corpus test refuses exemptions on the grounds that "a corpus with a list of
//! files that do not have to work stops being an acceptance check". That holds
//! word for word here.
//!
//! The fourth property — that every formatted file still passes
//! `sciencec check` — needs name resolution and so lives in
//! `crates/sciencec/tests/fmt.rs`, next to the driver that can reach it.
//!
//! Two more tests below are about the corpus rather than the formatter, and
//! they exist because running the formatter over `examples/` is the only thing
//! that has ever told this crate something it did not already know.
//! [`UNFORMATTED`] is the ledger of files the formatter and the corpus still
//! disagree about, with the exact line counts, so the list shrinks
//! deliberately instead of rotting. And
//! [`every_corpus_marker_is_load_bearing`] checks that every `# fmt:` marker
//! checked into `examples/` is protecting something: a marker that protects
//! nothing is a marker somebody will delete, and a corpus full of them is how
//! an escape hatch turns into decoration.

use std::path::{Path, PathBuf};

use science_diagnostics::FileId;

fn corpus() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("examples");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("examples/ must exist")
        .map(|entry| entry.expect("readable directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "science"))
        .collect();
    files.sort();
    files
}

fn name(path: &Path) -> String {
    path.file_name().expect("a file name").to_string_lossy().into_owned()
}

/// Formats a source, or explains why it would not.
fn format(source: &str) -> String {
    let out = science_fmt::format_source(FileId(0), source);
    out.text.unwrap_or_else(|| {
        panic!(
            "the formatter refused a file it should have accepted: {}",
            out.diagnostics.iter().map(|d| d.message.clone()).collect::<Vec<_>>().join("; ")
        )
    })
}

/// The AST dump with every span removed.
///
/// `science-parser`'s dump ends every line with the node's span — `@12..20`, or
/// `@f3:12..20` for another file — and reformatting moves every byte in the
/// file, so the spans are the one thing that is *expected* to differ. Stripping
/// them by hand rather than with a pattern keeps the check honest: a line whose
/// tail does not have the exact shape of a span keeps its tail, so a dump that
/// changed shape shows up as a difference instead of being trimmed away.
fn tree(source: &str) -> String {
    use science_parser::Dump as _;
    let (tokens, lexed) = science_lexer::lex(FileId(0), source);
    assert!(!lexed.has_errors(), "the corpus must lex clean");
    let (module, parsed) = science_parser::parse_module(&tokens, FileId(0));
    assert!(!parsed.has_errors(), "the corpus must parse clean");
    module.dump().lines().map(strip_span).collect::<Vec<_>>().join("\n")
}

/// Removes a trailing ` @start..end` or ` @f1:start..end`, and nothing else.
fn strip_span(line: &str) -> String {
    let Some(at) = line.rfind(" @") else { return line.to_string() };
    let tail = &line[at + 2..];
    let tail = match tail.strip_prefix('f') {
        Some(rest) => match rest.split_once(':') {
            Some((file, rest)) if !file.is_empty() && file.bytes().all(|b| b.is_ascii_digit()) => {
                rest
            }
            _ => return line.to_string(),
        },
        None => tail,
    };
    match tail.split_once("..") {
        Some((start, end))
            if !start.is_empty()
                && !end.is_empty()
                && start.bytes().all(|b| b.is_ascii_digit())
                && end.bytes().all(|b| b.is_ascii_digit()) =>
        {
            line[..at].to_string()
        }
        _ => line.to_string(),
    }
}

/// Property 2: the tree the formatter hands back is the tree it was given.
#[test]
fn every_example_keeps_its_syntax_tree() {
    let files = corpus();
    assert!(!files.is_empty(), "the corpus must not be empty");

    let mut failures = Vec::new();
    for path in &files {
        let source = std::fs::read_to_string(path).expect("readable corpus file");
        let formatted = format(&source);
        let (before, after) = (tree(&source), tree(&formatted));
        if before != after {
            let at = before
                .lines()
                .zip(after.lines())
                .position(|(a, b)| a != b)
                .map_or_else(|| "length".to_string(), |n| format!("line {}", n + 1));
            failures.push(format!("{}: the tree differs at {at}", name(path)));
        }
    }
    assert!(failures.is_empty(), "{} of {} files:\n{}", failures.len(), files.len(), failures.join("\n"));
}

/// Not one line of the corpus defeats the layout.
///
/// The formatter is allowed to give up on a logical line and reproduce it as
/// written — that is what it does instead of dropping a comment it cannot place
/// — and over `examples/` it never has to, although the corpus puts comments
/// inside bracketed lists, between the links of a chain, after block headers
/// and at every indentation a comment can be written at. If this starts
/// failing, a real construct has arrived that the layout cannot express, and
/// the right response is to look at it rather than to raise the number.
#[test]
fn no_corpus_line_is_left_alone() {
    let mut given_up = Vec::new();
    for path in &corpus() {
        let source = std::fs::read_to_string(path).expect("readable corpus file");
        let out = science_fmt::format_source(FileId(0), &source);
        if out.preserved > 0 {
            given_up.push(format!("{}: {} lines", name(path), out.preserved));
        }
    }
    assert!(given_up.is_empty(), "{}", given_up.join("\n"));
}

/// Property 1: gate E1.
#[test]
fn formatting_is_idempotent() {
    let files = corpus();
    let mut failures = Vec::new();
    for path in &files {
        let source = std::fs::read_to_string(path).expect("readable corpus file");
        let once = format(&source);
        let twice = format(&once);
        if once != twice {
            let at = once
                .lines()
                .zip(twice.lines())
                .position(|(a, b)| a != b)
                .map_or_else(|| "the end".to_string(), |n| format!("line {}", n + 1));
            failures.push(format!("{}: a second pass changed {at}", name(path)));
        }
    }
    assert!(failures.is_empty(), "{} of {} files:\n{}", failures.len(), files.len(), failures.join("\n"));
}

/// Property 3: dropping a comment is the one unacceptable failure.
#[test]
fn no_comment_is_ever_lost() {
    let files = corpus();
    let mut total = 0usize;
    let mut failures = Vec::new();
    for path in &files {
        let source = std::fs::read_to_string(path).expect("readable corpus file");
        let formatted = format(&source);
        let before = comments(&source);
        let after = comments(&formatted);
        total += before.len();
        if before != after {
            failures.push(format!(
                "{}: {} comments went in and {} came out",
                name(path),
                before.len(),
                after.len()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(total > 200, "the corpus should be heavily commented; found {total}");
}

/// Every comment in a source, as text, in order. Written out here rather than
/// reusing the formatter's own scanner, so that the test does not agree with
/// the implementation by construction.
fn comments(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in source.lines() {
        let mut in_string = false;
        let mut in_char = false;
        let mut escaped = false;
        for (i, c) in line.char_indices() {
            if escaped {
                escaped = false;
                continue;
            }
            match c {
                '\\' if in_string || in_char => escaped = true,
                '"' if !in_char => in_string = !in_string,
                '\'' if !in_string => in_char = !in_char,
                '#' if !in_string && !in_char => {
                    out.push(line[i..].trim_end().to_string());
                    break;
                }
                _ => {}
            }
        }
    }
    out
}

/// Formatting never leaves a line with trailing whitespace, a tab in its
/// indentation, or a missing final newline.
#[test]
fn the_output_is_tidy() {
    for path in &corpus() {
        let source = std::fs::read_to_string(path).expect("readable corpus file");
        let formatted = format(&source);
        assert!(formatted.ends_with('\n'), "{}: no final newline", name(path));
        assert!(!formatted.ends_with("\n\n"), "{}: a blank line at the end", name(path));
        for (n, line) in formatted.lines().enumerate() {
            assert_eq!(line.trim_end(), line, "{}:{}: trailing whitespace", name(path), n + 1);
            assert!(
                !line.starts_with('\t'),
                "{}:{}: a tab in the indentation",
                name(path),
                n + 1
            );
        }
    }
}

/// The corpus files the formatter still wants to change, with how many lines.
///
/// **This is a ledger, not a fixture.** Every entry is a place where the
/// formatter and a checked-in file disagree, and each one is either a bug in
/// the formatter, a line that should be reformatted, or a demonstration that
/// needs a `# fmt:` marker. The counts are exact on purpose: closing one breaks
/// this test, which is the point — the list has to shrink deliberately rather
/// than rot.
///
/// * `07_generics.science` — two signatures the file says are "broken inside
///   the parameter list, where indentation takes no part (§4.2)". The
///   formatter rebreaks them before `where` instead, which is the house style
///   of `AGENTS.md` §4 but is not what the comment beside them describes. The
///   demonstration and the style rule disagree, and that is a language
///   question rather than a formatter one.
/// * `08_dyn_dispatch.science` — the one line in the corpus over 88 columns,
///   which the formatter breaks into three because a doubled
///   `Array of (Box of any Summarize)` has no good break in it.
/// * `09_absence_and_failure.science` — one hand-broken signature with no
///   trailing comma, which the formatter rebreaks. No demonstration is at
///   stake; it is simply not formatted yet.
const UNFORMATTED: &[(&str, usize)] = &[
    ("07_generics.science", 8),
    ("08_dyn_dispatch.science", 4),
    ("09_absence_and_failure.science", 6),
];

/// Nineteen of the twenty-two corpus files are what the formatter would write.
#[test]
fn the_corpus_is_what_the_formatter_would_write_except_for_the_known_ledger() {
    let mut wrong = Vec::new();
    for path in &corpus() {
        let file = name(path);
        let source = std::fs::read_to_string(path).expect("readable corpus file");
        let changed = differing_lines(&source, &format(&source));
        let pinned = UNFORMATTED.iter().find(|(n, _)| *n == file).map_or(0, |(_, n)| *n);
        if changed != pinned {
            wrong.push(format!("{file}: the ledger says {pinned} lines, the formatter wants {changed}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// How many lines two texts do not have in common, counted the way `diff` does.
fn differing_lines(before: &str, after: &str) -> usize {
    let (before, after): (Vec<&str>, Vec<&str>) = (before.lines().collect(), after.lines().collect());
    // A full LCS is overkill for a count that only has to be stable, but it is
    // the only way the number means "lines a reviewer would see marked".
    let mut table = vec![vec![0usize; after.len() + 1]; before.len() + 1];
    for i in (0..before.len()).rev() {
        for j in (0..after.len()).rev() {
            table[i][j] = if before[i] == after[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    before.len() + after.len() - 2 * table[0][0]
}

/// Every `# fmt:` marker in the corpus is doing work.
///
/// A marker that protects nothing is a marker somebody will delete, and a file
/// full of them is how an escape hatch turns into decoration. Deleting the
/// markers from a file that has them must change what the formatter produces —
/// otherwise there was nothing to hold.
#[test]
fn every_corpus_marker_is_load_bearing() {
    let mut idle = Vec::new();
    let mut marked = 0usize;
    for path in &corpus() {
        let source = std::fs::read_to_string(path).expect("readable corpus file");
        let stripped: String = source
            .lines()
            .filter(|line| !is_marker(line))
            .map(|line| format!("{line}\n"))
            .collect();
        if stripped == source {
            continue;
        }
        marked += 1;
        if format(&stripped) == stripped {
            idle.push(format!("{}: its markers protect nothing", name(path)));
        }
    }
    assert!(idle.is_empty(), "{}", idle.join("\n"));
    assert_eq!(marked, 4, "four corpus files carry markers; see the crate documentation");
}

fn is_marker(line: &str) -> bool {
    matches!(line.trim(), "# fmt: off" | "# fmt: on" | "# fmt: skip")
}

/// A file with markers in it is still a fixed point.
#[test]
fn the_marked_examples_format_to_themselves() {
    for path in &corpus() {
        let source = std::fs::read_to_string(path).expect("readable corpus file");
        if !source.lines().any(is_marker) {
            continue;
        }
        assert_eq!(format(&source), source, "{} is not a fixed point", name(path));
    }
}

/// How many physical lines of the corpus each marked file holds back, pinned.
///
/// The count is the real cost of the escape hatch and the number that should
/// worry anybody: it is source the formatter will never touch again. Pinning it
/// means a marker cannot quietly grow to cover a function, and a new one cannot
/// appear without somebody typing the number.
const SUPPRESSED: &[(&str, usize)] = &[
    // Three `# fmt: skip`s: one free-form nested call and two parenthesized
    // expressions written one operand per line.
    ("14_line_continuation.science", 19),
    // Two regions, both holding the column of a comment whose own text says
    // what that column is.
    ("15_comments.science", 9),
    // One region around `with_gaps`, whose runs of blank lines are the point.
    ("16_indentation.science", 11),
    // One `# fmt: skip` on the `cblas_dgemm` call site. The six declarations
    // above it need no marker: their trailing commas hold their grouping.
    ("20_extern.science", 13),
];

#[test]
fn the_markers_hold_back_no_more_of_the_corpus_than_they_are_pinned_to() {
    let mut wrong = Vec::new();
    let mut total = 0usize;
    for path in &corpus() {
        let file = name(path);
        let source = std::fs::read_to_string(path).expect("readable corpus file");
        let held = science_fmt::format_source(FileId(0), &source).suppressed;
        total += held;
        let pinned = SUPPRESSED.iter().find(|(n, _)| *n == file).map_or(0, |(_, n)| *n);
        if held != pinned {
            wrong.push(format!("{file}: pinned at {pinned} suppressed lines, actually {held}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    // Against a corpus of some three and a half thousand lines.
    assert!(total < 100, "the escape hatch is being used as a policy: {total} lines");
}
