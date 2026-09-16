//! Pinned formatter output, for the rules `examples/` does not reach.
//!
//! The corpus is the acceptance check, but it cannot be the whole test: nothing
//! in it is wider than 97 columns, so nothing in it makes the formatter break a
//! line. `examples/` is also, deliberately, already well formatted — it says so
//! — which means it exercises the *joining* half of the formatter and almost
//! none of the splitting half.
//!
//! Each `NAME.science` here is an input and each `NAME.expected` is what the
//! formatter produces from it, checked in so that a change to the rules shows
//! up as a diff in a review rather than as a surprise in someone's working
//! tree. Every pair is also checked for the two properties that matter:
//! formatting the expectation again changes nothing, and the tree survives.
//!
//! These files are not `examples/` and are not held to its contract. They are
//! allowed to be ugly — `messy_spacing.science` is ugly on purpose — and they
//! are not required to resolve.
//!
//! `suppression.science` is the one to read first if you are looking for what
//! `# fmt: off`, `# fmt: on` and `# fmt: skip` do. It also holds the two
//! spellings that look like markers and are not — one at the end of a line,
//! one inside a bracket — because a mechanism's edges are the part a test has
//! to pin.

use std::path::{Path, PathBuf};

use science_diagnostics::FileId;

fn fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("the fixture directory must exist")
        .map(|entry| entry.expect("readable directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "science"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "there must be fixtures");
    files
}

fn format(source: &str) -> science_fmt::Formatting {
    science_fmt::format_source(FileId(0), source)
}

/// Line endings are normalised, because git may check these files out with
/// `\r\n` on Windows and the formatter's output always uses `\n`.
fn read(path: &Path) -> String {
    std::fs::read_to_string(path).expect("a readable fixture").replace("\r\n", "\n")
}

#[test]
fn every_fixture_formats_to_its_expectation() {
    for path in fixtures() {
        let expectation = path.with_extension("expected");
        let expected = read(&expectation);
        let produced = format(&read(&path)).text.expect("a fixture must format");
        assert_eq!(
            produced,
            expected,
            "{} does not match {}\n--- produced ---\n{produced}",
            path.display(),
            expectation.display()
        );
    }
}

#[test]
fn every_expectation_is_a_fixed_point() {
    for path in fixtures() {
        let expected = read(&path.with_extension("expected"));
        let again = format(&expected).text.expect("an expectation must format");
        assert_eq!(again, expected, "{} is not stable under a second pass", path.display());
    }
}

/// The count of logical lines the formatter refused to lay out, pinned per
/// fixture. `comments_refused.science` is the only one that has any, and it has
/// exactly the two the layout has no slot for; if a third appears, or one of
/// the two stops being refused, that is a change worth noticing.
#[test]
fn the_refused_lines_are_the_ones_that_should_be() {
    for path in fixtures() {
        let name = path.file_name().expect("a name").to_string_lossy().into_owned();
        let preserved = format(&read(&path)).preserved;
        let expected = match name.as_str() {
            "comments_refused.science" => 2,
            _ => 0,
        };
        assert_eq!(preserved, expected, "{name} preserved {preserved} lines");
    }
}
