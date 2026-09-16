//! `sciencec fmt`, end to end, and the formatter's third acceptance property.
//!
//! `crates/science-fmt/tests/corpus.rs` holds the first two — idempotence and
//! an unchanged syntax tree. The third needs name resolution, which the
//! formatter crate deliberately cannot reach, so it lives here beside the
//! driver that can:
//!
//! > every file in `examples/` still passes `sciencec check` after formatting.
//!
//! Stated exactly: the *result* of `check` is unchanged. One example does not
//! pass today and is pinned in `cli.rs` as `UNRESOLVED` — it imports modules
//! that are not files in this repository — and a test that demanded a clean
//! `check` would either have to exempt it or would quietly be asserting
//! nothing about the other 21. So what is checked is that formatting changes
//! neither the exit code nor a single diagnostic the compiler reports.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn examples() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(repo_root().join("examples"))
        .expect("examples/ must exist")
        .map(|entry| entry.expect("readable directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "science"))
        .collect();
    files.sort();
    files
}

/// Runs `sciencec` from the repository root and returns `(code, stderr)`.
fn sciencec(args: &[&str]) -> (i32, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_sciencec"))
        .current_dir(repo_root())
        .args(args)
        .output()
        .expect("the sciencec binary must be runnable");
    let stderr = String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n");
    assert!(!stderr.contains("panicked at"), "`sciencec {}` panicked:\n{stderr}", args.join(" "));
    (output.status.code().expect("the process must exit normally"), stderr)
}

fn stdout_of(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_sciencec"))
        .current_dir(repo_root())
        .args(args)
        .output()
        .expect("the sciencec binary must be runnable");
    String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n")
}

/// What a `check` said, with the positions taken out.
///
/// Reformatting moves every byte in the file, so the `--> file:line:column`
/// headers and the quoted source lines are expected to move. The codes and the
/// messages are not, and they are what "the same file was checked" means.
fn verdict(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .filter(|line| line.starts_with("error[SC") || line.starts_with("warning[SC"))
        .map(str::to_string)
        .collect()
}

/// A directory under the test target directory, emptied first so that a run
/// never sees the previous one's files.
fn workspace(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a writable temporary directory");
    dir
}

/// The third property: formatting changes nothing the compiler has to say.
#[test]
fn every_example_checks_the_same_after_formatting() {
    let dir = workspace("fmt-corpus");
    let mut failures = Vec::new();

    for path in examples() {
        let file = path.file_name().expect("a file name").to_string_lossy().into_owned();
        let source = std::fs::read_to_string(&path).expect("readable corpus file");

        let formatted = science_fmt::format_source(science_diagnostics::FileId(0), &source)
            .text
            .unwrap_or_else(|| panic!("{file} should format"));
        let copy = dir.join(&file);
        std::fs::write(&copy, &formatted).expect("a writable copy");

        let (before_code, before) = sciencec(&["check", &format!("examples/{file}")]);
        let (after_code, after) = sciencec(&["check", &copy.to_string_lossy()]);

        if before_code != after_code {
            failures.push(format!("{file}: exit {before_code} became {after_code}"));
        }
        if verdict(&before) != verdict(&after) {
            failures.push(format!(
                "{file}: the diagnostics changed\nbefore:\n{}\nafter:\n{}",
                verdict(&before).join("\n"),
                verdict(&after).join("\n")
            ));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn fmt_prints_to_stdout_and_leaves_the_file_alone() {
    let path = repo_root().join("examples").join("14_line_continuation.science");
    let before = std::fs::read_to_string(&path).expect("readable corpus file");

    let out = stdout_of(&["fmt", "examples/14_line_continuation.science"]);
    assert!(!out.is_empty(), "fmt should print the formatted file");
    assert_ne!(out, before, "this example is one the formatter changes");

    let after = std::fs::read_to_string(&path).expect("readable corpus file");
    assert_eq!(before, after, "fmt without --write must not touch the file");
}

#[test]
fn fmt_write_rewrites_the_file_and_is_idempotent_on_disk() {
    let dir = workspace("fmt-write");
    let copy = dir.join("14_line_continuation.science");
    let original = repo_root().join("examples").join("14_line_continuation.science");
    std::fs::copy(&original, &copy).expect("a writable copy");

    let name = copy.to_string_lossy().into_owned();
    let (code, stderr) = sciencec(&["fmt", "--write", &name]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stderr, "", "a clean fmt says nothing");
    let once = std::fs::read_to_string(&copy).expect("readable");
    assert_ne!(once, std::fs::read_to_string(&original).expect("readable"));

    let (code, _) = sciencec(&["fmt", "--write", &name]);
    assert_eq!(code, 0);
    assert_eq!(once, std::fs::read_to_string(&copy).expect("readable"), "gate E1, on disk");
}

#[test]
fn fmt_refuses_a_file_it_cannot_parse() {
    let (code, stderr) = sciencec(&["fmt", "tests/ui/bad_escape.science"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("error[SC0006]"), "{stderr}");
    assert_eq!(
        stdout_of(&["fmt", "tests/ui/bad_escape.science"]),
        "",
        "a refused file produces no text at all"
    );
}

/// A file whose only failure is a name the single-file `check` cannot resolve
/// is still syntax, and syntax is all the formatter needs.
#[test]
fn fmt_does_not_need_a_file_to_resolve() {
    let (code, stderr) = sciencec(&["check", "examples/17_modules.science"]);
    assert_eq!(code, 1, "this test is pointless if the example starts resolving");
    assert!(stderr.contains("error[SC02"), "{stderr}");

    let (code, stderr) = sciencec(&["fmt", "examples/17_modules.science"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(!stdout_of(&["fmt", "examples/17_modules.science"]).is_empty());
}

#[test]
fn fmt_write_leaves_an_already_formatted_file_untouched() {
    let dir = workspace("fmt-notouch");
    let copy = dir.join("01_functions.science");
    std::fs::copy(repo_root().join("examples").join("01_functions.science"), &copy)
        .expect("a writable copy");
    let before = std::fs::metadata(&copy).and_then(|m| m.modified()).expect("a timestamp");

    let (code, _) = sciencec(&["fmt", "--write", &copy.to_string_lossy()]);
    assert_eq!(code, 0);
    let after = std::fs::metadata(&copy).and_then(|m| m.modified()).expect("a timestamp");
    assert_eq!(before, after, "a file the formatter would not change must not be rewritten");
}
