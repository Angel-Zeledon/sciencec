//! UI tests for `sciencec`, in the style of rustc's.
//!
//! A UI test is a `.science` program that must be rejected, sitting next to a
//! `.stderr` file holding, byte for byte, the diagnostics the compiler is
//! expected to print for it. Section 10 of the design spec calls this the
//! only known way of stopping error messages from rotting, and says that for
//! a language whose regions are inferred it is not optional: the message *is*
//! the feature, because the programmer never wrote the annotation the
//! compiler is complaining about.
//!
//! # Compiling is a parameter
//!
//! [`run_ui_tests`] takes the compile step as a closure:
//!
//! ```no_run
//! use std::path::Path;
//! use science_testkit::run_ui_tests;
//!
//! # fn lex_and_render(_source: &str) -> String { String::new() }
//! let report = run_ui_tests(Path::new("tests/ui"), |_path: &Path, source: &str| {
//!     lex_and_render(source)
//! });
//! report.assert_success();
//! ```
//!
//! The harness therefore has no dependency on any compiler phase, and can be
//! finished, tested and relied upon before the lexer, the parser or the
//! diagnostic renderer exist. Each crate plugs in whatever it has: the lexer
//! crate may render only lexical errors, the driver may render everything.
//!
//! # Updating expectations
//!
//! If a `.stderr` file is missing, or if `SCIENCE_BLESS=1` is set in the
//! environment, the harness *writes* the output it got instead of failing.
//! That is the whole update workflow:
//!
//! ```text
//! SCIENCE_BLESS=1 cargo test -p science-lexer
//! git diff tests/ui        # read what changed before committing it
//! ```
//!
//! The `git diff` step is the point. Blessing is only safe if a human reads
//! the diff, which is also why a failure prints one.
//!
//! # Normalisation
//!
//! Two normalisations are applied to both sides before comparing, and to
//! anything written to disk:
//!
//! 1. `\r\n` and a lone `\r` both become `\n`. Development happens on
//!    Windows, and git may hand back whatever it likes; without this every
//!    case would fail for a reason that has nothing to do with the compiler.
//! 2. Trailing blank lines are collapsed to exactly one final `\n`, because
//!    whether a renderer ends its last line is not a fact worth failing over.
//!
//! Everything else is compared literally: spaces, carets and alignment are
//! part of a diagnostic and are exactly what regresses.

use std::fmt;
use std::path::{Path, PathBuf};

pub mod diff;

/// The extension of a UI test's source program.
pub const SOURCE_EXTENSION: &str = "science";
/// The extension of the file holding its expected diagnostics.
pub const EXPECTATION_EXTENSION: &str = "stderr";
/// Set this to `1` to rewrite expectation files instead of failing.
pub const BLESS_ENV_VAR: &str = "SCIENCE_BLESS";

/// How a run should behave.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiTestOptions {
    /// Rewrite every expectation file with the output obtained, rather than
    /// comparing against it. A missing expectation is written regardless.
    pub bless: bool,
}

impl UiTestOptions {
    /// Options taken from the environment: `bless` is on when
    /// [`BLESS_ENV_VAR`] is exactly `1`.
    ///
    /// Any other value, including an empty one, leaves it off, so that
    /// `SCIENCE_BLESS=0` means what it looks like it means.
    pub fn from_env() -> Self {
        let bless = std::env::var(BLESS_ENV_VAR).map(|v| v == "1").unwrap_or(false);
        UiTestOptions { bless }
    }
}

/// Why one case failed, and the diff that shows it.
#[derive(Debug, Clone)]
pub struct UiFailure {
    /// The `.science` program.
    pub source: PathBuf,
    /// The `.stderr` file it was compared against.
    pub expectation: PathBuf,
    /// One line saying what went wrong.
    pub reason: String,
    /// The rendered line-by-line diff, empty when there is nothing to diff
    /// (an unreadable file, say).
    pub diff: String,
}

impl fmt::Display for UiFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "FAILED {}", self.source.display())?;
        writeln!(f, "  {}", self.reason)?;
        writeln!(f, "  expectation: {}", self.expectation.display())?;
        for line in self.diff.lines() {
            writeln!(f, "  {line}")?;
        }
        Ok(())
    }
}

/// What a run found.
#[derive(Debug, Clone)]
pub struct UiTestReport {
    directory: PathBuf,
    passed: Vec<PathBuf>,
    blessed: Vec<PathBuf>,
    failures: Vec<UiFailure>,
}

impl UiTestReport {
    fn new(directory: &Path) -> Self {
        UiTestReport {
            directory: directory.to_path_buf(),
            passed: Vec::new(),
            blessed: Vec::new(),
            failures: Vec::new(),
        }
    }

    /// The directory that was walked.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// How many cases matched their expectation.
    pub fn passed(&self) -> usize {
        self.passed.len()
    }

    /// How many cases did not.
    pub fn failed(&self) -> usize {
        self.failures.len()
    }

    /// How many expectation files were written rather than compared.
    pub fn blessed(&self) -> usize {
        self.blessed.len()
    }

    /// Every case considered.
    pub fn total(&self) -> usize {
        self.passed() + self.failed() + self.blessed()
    }

    pub fn passed_cases(&self) -> &[PathBuf] {
        &self.passed
    }

    pub fn blessed_cases(&self) -> &[PathBuf] {
        &self.blessed
    }

    pub fn failures(&self) -> &[UiFailure] {
        &self.failures
    }

    /// Nothing failed. A blessed case is not a failure: blessing is the
    /// documented way of creating or updating an expectation.
    pub fn is_ok(&self) -> bool {
        self.failures.is_empty()
    }

    /// The whole report as text: the counts, then every failure with its
    /// diff, then how to update the expectations.
    pub fn summary(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "ui tests in {}: {} passed, {} failed, {} blessed\n",
            self.directory.display(),
            self.passed(),
            self.failed(),
            self.blessed()
        ));

        for path in &self.blessed {
            out.push_str(&format!("blessed {}\n", path.display()));
        }

        for failure in &self.failures {
            out.push('\n');
            out.push_str(&failure.to_string());
        }

        if !self.is_ok() {
            out.push_str(&format!(
                "\nIf these outputs are the new correct ones, rerun with {}=1 to \
                 overwrite the expectation files, then read `git diff` before committing.\n",
                BLESS_ENV_VAR
            ));
        }
        out
    }

    /// Panic with [`summary`](Self::summary) if anything failed.
    ///
    /// This is the line that hooks a UI suite into `cargo test`.
    ///
    /// # Panics
    /// If any case failed.
    pub fn assert_success(&self) {
        if !self.is_ok() {
            panic!("{}", self.summary());
        }
    }
}

/// Run every UI test under `dir`, taking the bless flag from the environment.
///
/// `compile` is handed the path of the `.science` file and its text, and returns
/// the diagnostics as they should appear in the `.stderr` file. Returning an
/// empty string means "this program produced no diagnostics", which for a
/// case that is supposed to fail is itself a failure the expectation file
/// will catch.
pub fn run_ui_tests(dir: &Path, compile: impl Fn(&Path, &str) -> String) -> UiTestReport {
    run_ui_tests_with(dir, UiTestOptions::from_env(), compile)
}

/// Run every UI test under `dir` with explicit options.
///
/// Prefer this when the caller decides whether to bless, for instance in the
/// harness's own tests, where reading a process-wide environment variable
/// would make parallel tests interfere.
pub fn run_ui_tests_with(
    dir: &Path,
    options: UiTestOptions,
    compile: impl Fn(&Path, &str) -> String,
) -> UiTestReport {
    let mut report = UiTestReport::new(dir);

    let mut cases = Vec::new();
    if let Err(e) = collect_cases(dir, &mut cases) {
        report.failures.push(UiFailure {
            source: dir.to_path_buf(),
            expectation: dir.to_path_buf(),
            reason: format!("could not read the UI test directory {}: {e}", dir.display()),
            diff: String::new(),
        });
        return report;
    }

    for source in cases {
        run_one(&source, options, &compile, &mut report);
    }
    report
}

/// The `.stderr` file that goes with a `.science` source.
pub fn expectation_path(source: &Path) -> PathBuf {
    source.with_extension(EXPECTATION_EXTENSION)
}

/// Line endings to `\n`, and exactly one final newline when non-empty.
///
/// See the module documentation for why this happens before any comparison.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            // Swallow the `\n` of a `\r\n` pair; a lone `\r` becomes `\n` too.
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push('\n');
        } else {
            out.push(c);
        }
    }

    while out.ends_with('\n') {
        out.pop();
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

fn run_one(
    source: &Path,
    options: UiTestOptions,
    compile: &impl Fn(&Path, &str) -> String,
    report: &mut UiTestReport,
) {
    let text = match std::fs::read_to_string(source) {
        Ok(text) => text,
        Err(e) => {
            report.failures.push(UiFailure {
                source: source.to_path_buf(),
                expectation: expectation_path(source),
                reason: format!("could not read the source: {e}"),
                diff: String::new(),
            });
            return;
        }
    };

    // The source is normalised too, so that a `.science` file checked out with
    // CRLF does not shift every span by one byte per line.
    let actual = normalize(&compile(source, &normalize(&text)));
    let expectation = expectation_path(source);

    let expected = match std::fs::read_to_string(&expectation) {
        Ok(text) => Some(normalize(&text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            report.failures.push(UiFailure {
                source: source.to_path_buf(),
                expectation,
                reason: format!("could not read the expectation: {e}"),
                diff: String::new(),
            });
            return;
        }
    };

    // A missing expectation is written rather than failing: that is how a new
    // case is created. An existing one is only rewritten when blessing.
    if expected.is_none() || options.bless {
        match std::fs::write(&expectation, actual.as_bytes()) {
            Ok(()) => report.blessed.push(source.to_path_buf()),
            Err(e) => report.failures.push(UiFailure {
                source: source.to_path_buf(),
                expectation,
                reason: format!("could not write the expectation: {e}"),
                diff: String::new(),
            }),
        }
        return;
    }

    let expected = expected.expect("checked just above");
    if diff::is_identical(&expected, &actual) {
        report.passed.push(source.to_path_buf());
    } else {
        report.failures.push(UiFailure {
            source: source.to_path_buf(),
            expectation,
            reason: "the diagnostics produced differ from the expectation".to_string(),
            diff: diff::render(&expected, &actual),
        });
    }
}

/// Collect every `.science` file under `dir`, depth first, sorted by name.
///
/// The order is stable so that a failing run is reproducible and the report
/// reads the same way twice.
fn collect_cases(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut entries: Vec<PathBuf> =
        std::fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?.iter().map(|e| e.path()).collect();
    entries.sort();

    for entry in entries {
        if entry.is_dir() {
            collect_cases(&entry, out)?;
        } else if entry.extension().and_then(|e| e.to_str()) == Some(SOURCE_EXTENSION) {
            out.push(entry);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crlf_becomes_lf() {
        assert_eq!(normalize("a\r\nb\r\n"), "a\nb\n");
    }

    #[test]
    fn a_lone_cr_becomes_lf() {
        assert_eq!(normalize("a\rb\r"), "a\nb\n");
    }

    #[test]
    fn a_missing_final_newline_is_added() {
        assert_eq!(normalize("a\nb"), "a\nb\n");
    }

    #[test]
    fn trailing_blank_lines_collapse_to_one_newline() {
        assert_eq!(normalize("a\n\n\n"), "a\n");
    }

    #[test]
    fn empty_stays_empty() {
        assert_eq!(normalize(""), "");
        assert_eq!(normalize("\n\n"), "");
    }

    #[test]
    fn interior_blank_lines_are_preserved() {
        assert_eq!(normalize("a\n\nb\n"), "a\n\nb\n");
    }

    #[test]
    fn leading_and_interior_whitespace_is_preserved() {
        // Alignment inside a diagnostic is exactly what regresses, so it is
        // compared literally.
        assert_eq!(normalize("  |  ^^^ here\n"), "  |  ^^^ here\n");
    }

    #[test]
    fn the_expectation_sits_next_to_the_source() {
        let expectation = expectation_path(Path::new("tests/ui/tab_in_indentation.science"));
        assert_eq!(expectation, PathBuf::from("tests/ui/tab_in_indentation.stderr"));
    }
}
