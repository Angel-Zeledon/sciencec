//! End-to-end tests: the real binary, over real files, asserting on stdout,
//! stderr and the exit code.
//!
//! This is the first place the whole front half runs in one pass. The lexer
//! checks `examples/` and the parser checks `examples/`, each on its own; until
//! there was a binary nothing ran text → tokens → AST → resolved names over
//! them together. Doing so found something, and it is pinned below rather than
//! papered over: see [`UNRESOLVED`].
//!
//! Every command runs with the repository root as its working directory,
//! because that is what makes the `-->` path in a diagnostic read
//! `examples/01_functions.science` on every platform.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Runs `sciencec` from the repository root.
fn sciencec(args: &[&str]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_sciencec"))
        .current_dir(repo_root())
        .args(args)
        .output()
        .expect("the sciencec binary must be runnable");
    Run::new(args, output)
}

struct Run {
    command: String,
    code: i32,
    stdout: String,
    stderr: String,
}

impl Run {
    fn new(args: &[&str], output: Output) -> Self {
        let stderr = String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n");
        assert!(
            !stderr.contains("panicked at"),
            "`sciencec {}` panicked:\n{stderr}",
            args.join(" ")
        );
        Run {
            command: format!("sciencec {}", args.join(" ")),
            code: output.status.code().expect("the process must exit normally"),
            stdout: String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
            stderr,
        }
    }

    #[track_caller]
    fn succeeded(&self) -> &Self {
        assert_eq!(self.code, 0, "`{}` should exit 0\nstderr:\n{}", self.command, self.stderr);
        self
    }

    #[track_caller]
    fn failed(&self) -> &Self {
        assert_eq!(self.code, 1, "`{}` should exit 1\nstderr:\n{}", self.command, self.stderr);
        self
    }

    #[track_caller]
    fn silent_stderr(&self) -> &Self {
        assert_eq!(self.stderr, "", "`{}` should report nothing", self.command);
        self
    }

    #[track_caller]
    fn stderr_contains(&self, needle: &str) -> &Self {
        assert!(
            self.stderr.contains(needle),
            "`{}` should mention `{needle}`\nstderr:\n{}",
            self.command,
            self.stderr
        );
        self
    }

    /// The trailing `3 errors, 1 warning` line.
    fn summary(&self) -> Option<&str> {
        self.stderr.trim_end().lines().last().filter(|line| {
            line.contains(" error") || line.contains(" warning") || *line == "1 error"
        })
    }
}

fn examples() -> Vec<PathBuf> {
    science_files(&repo_root().join("examples"))
}

fn ui_cases() -> Vec<PathBuf> {
    science_files(&repo_root().join("tests").join("ui"))
}

fn science_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{} must exist: {e}", dir.display()))
        .map(|entry| entry.expect("a readable directory entry").path())
        .filter(|path| path.extension().is_some_and(|e| e == "science"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "{} has no .science files", dir.display());
    files
}

/// `examples/<name>`, the way the command line spells it.
fn relative(path: &Path) -> String {
    format!("examples/{}", path.file_name().unwrap().to_string_lossy())
}

// --- the command surface -------------------------------------------------

#[test]
fn version_and_help_succeed_and_go_to_stdout() {
    let version = sciencec(&["--version"]);
    version.succeeded().silent_stderr();
    assert!(version.stdout.starts_with("sciencec "), "{}", version.stdout);

    let help = sciencec(&["--help"]);
    help.succeeded().silent_stderr();
    for command in ["check", "tokens", "ast", "resolve"] {
        assert!(help.stdout.contains(command), "--help should list `{command}`");
    }
}

#[test]
fn no_arguments_prints_the_usage_and_fails() {
    sciencec(&[]).failed().stderr_contains("Usage:");
}

#[test]
fn an_unknown_command_is_named() {
    sciencec(&["frobnicate", "examples/01_functions.science"])
        .failed()
        .stderr_contains("unknown command `frobnicate`");
}

#[test]
fn an_unknown_option_is_named() {
    sciencec(&["check", "--jobs", "examples/01_functions.science"])
        .failed()
        .stderr_contains("unknown option `--jobs`");
}

// --- check ---------------------------------------------------------------

#[test]
fn a_clean_check_says_nothing_at_all() {
    let run = sciencec(&["check", "examples/01_functions.science"]);
    run.succeeded().silent_stderr();
    assert_eq!(run.stdout, "", "check writes nothing to stdout");
}

/// Every program in `examples/` passes lexing and parsing, end to end through
/// the binary, with nothing reported and an exit code of 0.
///
/// `ast` is the command that runs exactly those two phases — see
/// [`UNRESOLVED`] for why the full `check` is a separate test.
#[test]
fn every_example_is_clean_through_lexing_and_parsing() {
    for path in examples() {
        let name = relative(&path);
        let run = sciencec(&["ast", &name]);
        run.succeeded().silent_stderr();
        assert!(run.stdout.starts_with("Module @"), "{name} should dump a module");
    }
}

/// The examples the front half cannot yet resolve, with the number of errors
/// each produces.
///
/// **This is a finding, not a fixture.** Running the whole front half over
/// `examples/` for the first time — which nothing did before this driver
/// existed, because the lexer and parser test the corpus separately and
/// `science-resolve` tests hand-built trees — showed that
/// `crates/science-resolve/src/builtins.rs` was missing names the examples
/// use. Six files failed. The prelude has since gained the ten operator
/// interfaces of §5.4, the half-precision primitives `F16` and `BF16`, the
/// free function `write`, and the C scalar vocabulary of
/// `ffi-c-boundary.md` §1.3, and four of the six now pass.
///
/// `20_extern.science` was the sixth, and the two things it needed have since
/// been built: the prelude now has a *module* — `ffi`, holding the closed
/// vocabulary of `ffi-c-boundary.md` §1.3 — so `ffi.Span` resolves like any
/// two-segment path, and a foreign function is a `DefKind` of its own, so
/// named arguments at an `extern` call site are a call rather than `SC0206`,
/// which is what §1.7 permits them to be. It is off this list.
///
/// The one that remains is **not** a prelude gap, and a longer list will not
/// fix it: `17_modules.science` imports `text.parser` and
/// `compiler.frontend.lexer`, which are not files in this repository, so
/// resolving it on its own cannot succeed. It needs a multi-file crate —
/// `resolve_crate`'s job, and a driver command that does not exist yet.
///
/// The counts are exact on purpose: closing a gap breaks this test, which is
/// the point — the list has to shrink deliberately rather than rot.
const UNRESOLVED: &[(&str, usize)] = &[("17_modules.science", 7)];

#[test]
fn every_example_is_clean_through_the_whole_front_half_except_the_known_gaps() {
    for path in examples() {
        let name = relative(&path);
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        let expected = UNRESOLVED.iter().find(|(n, _)| *n == file).map(|(_, count)| *count);
        let run = sciencec(&["check", &name]);

        match expected {
            None => {
                run.succeeded().silent_stderr();
            }
            Some(count) => {
                run.failed();
                let reported = run.stderr.matches("error[SC").count();
                assert_eq!(
                    reported, count,
                    "{name} reports a different number of errors than pinned\n{}",
                    run.stderr
                );
                // Only resolution errors, never a lexical or syntax one: the
                // examples are syntactically valid and must stay that way.
                for line in run.stderr.lines().filter(|l| l.starts_with("error[SC")) {
                    assert!(
                        line.starts_with("error[SC02"),
                        "{name} should only fail to resolve names: {line}"
                    );
                }
            }
        }
    }
}

#[test]
fn check_takes_several_files_and_reports_all_of_them() {
    let run = sciencec(&[
        "check",
        "tests/ui/unknown_character.science",
        "tests/ui/unterminated_string.science",
    ]);
    run.failed()
        .stderr_contains("tests/ui/unknown_character.science")
        .stderr_contains("tests/ui/unterminated_string.science");
}

#[test]
fn a_file_that_cannot_be_read_does_not_hide_the_next_one() {
    let run = sciencec(&[
        "check",
        "no/such/file.science",
        "tests/ui/unterminated_string.science",
        "examples/01_functions.science",
    ]);
    run.failed()
        .stderr_contains("cannot read `no/such/file.science`")
        .stderr_contains("tests/ui/unterminated_string.science");
    // The clean file contributed nothing, which is how you can tell it was not
    // skipped along with the broken ones.
    assert!(!run.stderr.contains("examples/01_functions.science"), "{}", run.stderr);
}

#[test]
fn the_summary_counts_every_error_and_is_the_last_line() {
    let single = sciencec(&["check", "tests/ui/unterminated_string.science"]);
    single.failed();
    let one = single.stderr.matches("error[SC").count();
    assert_eq!(single.summary(), Some(plural(one).as_str()), "{}", single.stderr);

    let missing = sciencec(&["check", "no/such/file.science"]);
    missing.failed();
    assert_eq!(missing.summary(), Some("1 error"));
}

fn plural(n: usize) -> String {
    if n == 1 {
        "1 error".to_string()
    } else {
        format!("{n} errors")
    }
}

// --- the UI suite, run through the binary --------------------------------

/// Every program in `tests/ui/` fails, and every error code its `.stderr`
/// expectation contains is still reported by the driver.
///
/// The output is not compared byte for byte — that is the lexer's UI suite's
/// job, and `check` runs more phases than the lexer does — but a code going
/// missing here would mean the driver dropped a diagnostic on the floor.
#[test]
fn every_ui_case_fails_with_the_codes_its_expectation_names() {
    for path in ui_cases() {
        let name = format!("tests/ui/{}", path.file_name().unwrap().to_string_lossy());
        let expectation = std::fs::read_to_string(path.with_extension("stderr"))
            .unwrap_or_else(|e| panic!("{name} must have a .stderr next to it: {e}"));

        let run = sciencec(&["check", &name]);
        run.failed().stderr_contains(&name);

        for code in codes_in(&expectation) {
            run.stderr_contains(&format!("error[{code}]"));
        }
    }
}

/// The distinct `SC0001`-style codes appearing in a rendered expectation.
fn codes_in(text: &str) -> Vec<String> {
    let mut codes: Vec<String> = text
        .match_indices("error[SC")
        .filter_map(|(at, _)| text[at + 6..].split(']').next())
        .map(str::to_string)
        .collect();
    codes.sort();
    codes.dedup();
    assert!(!codes.is_empty(), "an expectation with no codes in it");
    codes
}

// --- the dumps -----------------------------------------------------------

#[test]
fn the_token_dump_uses_the_lexers_snapshot_format() {
    let file = scratch("tokens.science", b"def main():\n    1\n");
    let run = sciencec(&["tokens", &file]);
    run.succeeded().silent_stderr();
    assert_eq!(
        run.stdout,
        "   0..3     Function\n   4..8     Ident(\"main\")\n   8..9     LParen\n   9..10    RParen\n  10..11    Colon\n  11..12    Newline\n  12..16    Indent\n  16..17    Int { value: 1, base: Dec, suffix: None }\n  17..18    Newline\n  18..18    Dedent\n  18..18    Eof\n",
        "the format is `{{:>4}}..{{:<4}}  {{kind:?}}`, as in crates/science-lexer/tests/common/mod.rs"
    );
}

#[test]
fn the_ast_dump_is_the_parsers_dump() {
    let run = sciencec(&["ast", "examples/01_functions.science"]);
    run.succeeded().silent_stderr();
    assert!(run.stdout.starts_with("Module @"), "{}", run.stdout);
    assert!(run.stdout.contains("Fn `greet`"), "{}", run.stdout);
}

#[test]
fn the_resolve_dump_is_the_resolvers_dump() {
    let run = sciencec(&["resolve", "examples/01_functions.science"]);
    run.succeeded().silent_stderr();
    assert!(run.stdout.starts_with("Module `01_functions` #"), "{}", run.stdout);
    // What distinguishes an HIR dump from an AST one: references carry what
    // they resolved to.
    assert!(run.stdout.contains("Path -> `print` #"), "{}", run.stdout);
}

#[test]
fn a_dump_still_goes_to_stdout_when_the_file_has_errors() {
    let run = sciencec(&["tokens", "tests/ui/unknown_character.science"]);
    run.failed().stderr_contains("error[SC0001]");
    assert!(run.stdout.contains("Function"), "the recovered stream is still dumped");
}

#[test]
fn a_dump_command_takes_exactly_one_file() {
    sciencec(&["ast", "examples/01_functions.science", "examples/02_bindings.science"])
        .failed()
        .stderr_contains("ast expects exactly one file");
}

// --- failing to read a file ----------------------------------------------

#[test]
fn a_missing_file_is_a_clean_error() {
    let run = sciencec(&["check", "no/such/file.science"]);
    run.failed().stderr_contains("error: cannot read `no/such/file.science`: no such file");
    assert_eq!(run.stdout, "");
}

#[test]
fn a_directory_is_a_clean_error() {
    let run = sciencec(&["check", "examples"]);
    run.failed().stderr_contains("error: `examples` is a directory, not a source file");
}

#[test]
fn a_file_that_is_not_utf8_is_a_clean_error() {
    // `0xFF` begins no UTF-8 sequence, and the lexer cannot be handed it: the
    // whole compiler indexes source by byte offset into a `str`.
    let file = scratch("not_utf8.science", b"def main():\n    print(\"\xff\")\n");
    let run = sciencec(&["check", &file]);
    run.failed().stderr_contains("is not valid UTF-8").stderr_contains("offset 23");
}

#[test]
fn each_dump_command_fails_cleanly_on_a_missing_file() {
    for command in ["tokens", "ast", "resolve"] {
        let run = sciencec(&[command, "no/such/file.science"]);
        run.failed().stderr_contains("cannot read");
        assert_eq!(run.stdout, "", "{command} should dump nothing");
    }
}

/// Writes a file under the target directory and returns its path, relative to
/// the repository root so that it is what the `-->` header shows.
fn scratch(name: &str, bytes: &[u8]) -> String {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("sciencec");
    std::fs::create_dir_all(&dir).expect("the target directory is writable");
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("the scratch file is writable");
    path.to_string_lossy().replace('\\', "/")
}
