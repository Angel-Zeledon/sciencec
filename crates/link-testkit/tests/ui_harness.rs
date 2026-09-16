//! Behaviour of the UI test harness itself.
//!
//! Every test builds a throwaway directory of `.link` cases, runs the harness
//! over it with a stub `compile` closure, and inspects the report. Nothing
//! here touches the real compiler: the point of the `compile` parameter is
//! that the harness works before any phase of it exists.

use std::path::{Path, PathBuf};

use link_testkit::{run_ui_tests_with, UiTestOptions};

mod support;
use support::{read, scratch, write};

/// A stub compiler that always returns the same text.
fn constant(text: &'static str) -> impl Fn(&Path, &str) -> String {
    move |_path, _source| text.to_string()
}

#[test]
fn a_case_whose_expectation_matches_passes() {
    let dir = scratch("matching");
    write(dir.join("case.link"), "fn main():\n    bad\n");
    write(dir.join("case.stderr"), "error[LK0100]: boom\n");

    let report =
        run_ui_tests_with(&dir, UiTestOptions::default(), constant("error[LK0100]: boom\n"));

    assert_eq!(report.passed(), 1);
    assert_eq!(report.failed(), 0);
    assert_eq!(report.blessed(), 0);
    assert!(report.is_ok());
}

#[test]
fn a_case_whose_expectation_differs_fails() {
    let dir = scratch("differing");
    write(dir.join("case.link"), "fn main():\n    bad\n");
    write(dir.join("case.stderr"), "error[LK0100]: expected this\n");

    let report =
        run_ui_tests_with(&dir, UiTestOptions::default(), constant("error[LK0100]: got that\n"));

    assert_eq!(report.passed(), 0);
    assert_eq!(report.failed(), 1);
    assert!(!report.is_ok());

    // The expectation on disk is left alone when we are not blessing.
    assert_eq!(read(dir.join("case.stderr")), "error[LK0100]: expected this\n");
}

#[test]
fn a_failure_reports_a_line_by_line_diff() {
    let dir = scratch("diff");
    write(dir.join("case.link"), "x\n");
    write(dir.join("case.stderr"), "same line\nexpected only\nalso same\n");

    let report = run_ui_tests_with(
        &dir,
        UiTestOptions::default(),
        constant("same line\nactual only\nalso same\n"),
    );

    let failure = &report.failures()[0];
    let diff = &failure.diff;

    // The differing pair shows up as one removal and one addition, each with
    // a line number; the shared lines are context.
    assert!(diff.contains('-'), "no removal marker in:\n{diff}");
    assert!(diff.contains('+'), "no addition marker in:\n{diff}");
    assert!(diff.contains("expected only"), "missing expected text in:\n{diff}");
    assert!(diff.contains("actual only"), "missing actual text in:\n{diff}");
    assert!(diff.contains('2'), "no line number in:\n{diff}");
    assert!(diff.contains("same line"), "context line dropped in:\n{diff}");
    // Line oriented, not two opaque blobs.
    assert!(diff.lines().count() >= 4, "diff collapsed to nothing:\n{diff}");
}

#[test]
fn the_diff_marks_a_line_present_only_in_the_actual_output() {
    let dir = scratch("diff-extra");
    write(dir.join("case.link"), "x\n");
    write(dir.join("case.stderr"), "one\n");

    let report =
        run_ui_tests_with(&dir, UiTestOptions::default(), constant("one\ntwo\nthree\n"));

    let diff = &report.failures()[0].diff;
    assert!(diff.contains("+"), "{diff}");
    assert!(diff.contains("two"), "{diff}");
    assert!(diff.contains("three"), "{diff}");
}

#[test]
fn a_missing_expectation_is_written_instead_of_failing() {
    let dir = scratch("missing");
    write(dir.join("case.link"), "fn main():\n    bad\n");

    let report =
        run_ui_tests_with(&dir, UiTestOptions::default(), constant("error[LK0003]: fresh\n"));

    assert_eq!(report.blessed(), 1);
    assert_eq!(report.failed(), 0);
    assert_eq!(report.passed(), 0);
    assert!(report.is_ok(), "a blessed case is not a failure");
    assert_eq!(read(dir.join("case.stderr")), "error[LK0003]: fresh\n");
}

#[test]
fn blessing_overwrites_an_expectation_that_differs() {
    let dir = scratch("bless-overwrite");
    write(dir.join("case.link"), "x\n");
    write(dir.join("case.stderr"), "stale\n");

    let report = run_ui_tests_with(&dir, UiTestOptions { bless: true }, constant("current\n"));

    assert_eq!(report.blessed(), 1);
    assert_eq!(report.failed(), 0);
    assert_eq!(read(dir.join("case.stderr")), "current\n");
}

#[test]
fn blessing_rewrites_even_an_expectation_that_already_matches() {
    let dir = scratch("bless-matching");
    write(dir.join("case.link"), "x\n");
    write(dir.join("case.stderr"), "same\n");

    let report = run_ui_tests_with(&dir, UiTestOptions { bless: true }, constant("same\n"));

    assert_eq!(report.blessed(), 1);
    assert_eq!(report.passed(), 0);
    assert_eq!(read(dir.join("case.stderr")), "same\n");
}

#[test]
fn crlf_in_the_expectation_file_is_normalised_away() {
    let dir = scratch("crlf-expected");
    write(dir.join("case.link"), "x\n");
    // What a Windows editor, or git with autocrlf, leaves on disk.
    write(dir.join("case.stderr"), "error[LK0003]: tab\r\n  --> case.link:1:1\r\n");

    let report = run_ui_tests_with(
        &dir,
        UiTestOptions::default(),
        constant("error[LK0003]: tab\n  --> case.link:1:1\n"),
    );

    assert_eq!(report.failed(), 0, "CRLF alone must not fail a case");
    assert_eq!(report.passed(), 1);
}

#[test]
fn crlf_in_the_compiler_output_is_normalised_away() {
    let dir = scratch("crlf-actual");
    write(dir.join("case.link"), "x\n");
    write(dir.join("case.stderr"), "one\ntwo\n");

    let report = run_ui_tests_with(&dir, UiTestOptions::default(), constant("one\r\ntwo\r\n"));

    assert_eq!(report.failed(), 0);
    assert_eq!(report.passed(), 1);
}

#[test]
fn a_lone_carriage_return_is_normalised_too() {
    let dir = scratch("cr-only");
    write(dir.join("case.link"), "x\n");
    write(dir.join("case.stderr"), "one\ntwo\n");

    let report = run_ui_tests_with(&dir, UiTestOptions::default(), constant("one\rtwo\r"));

    assert_eq!(report.failed(), 0);
}

#[test]
fn a_blessed_file_is_written_with_unix_line_endings() {
    let dir = scratch("crlf-bless");
    write(dir.join("case.link"), "x\n");

    run_ui_tests_with(&dir, UiTestOptions::default(), constant("one\r\ntwo\r\n"));

    let raw = read(dir.join("case.stderr"));
    assert!(!raw.contains('\r'), "blessed file kept a CR: {raw:?}");
    assert_eq!(raw, "one\ntwo\n");
}

#[test]
fn a_missing_trailing_newline_is_not_a_difference() {
    let dir = scratch("trailing-newline");
    write(dir.join("case.link"), "x\n");
    write(dir.join("case.stderr"), "error[LK0100]: boom\n");

    let report =
        run_ui_tests_with(&dir, UiTestOptions::default(), constant("error[LK0100]: boom"));

    assert_eq!(report.failed(), 0);
    assert_eq!(report.passed(), 1);
}

#[test]
fn files_that_are_not_link_sources_are_ignored() {
    let dir = scratch("ignored");
    write(dir.join("case.link"), "x\n");
    write(dir.join("case.stderr"), "out\n");
    write(dir.join("README.md"), "not a test\n");
    write(dir.join("notes.txt"), "not a test\n");

    let report = run_ui_tests_with(&dir, UiTestOptions::default(), constant("out\n"));

    assert_eq!(report.passed(), 1);
    assert_eq!(report.failed(), 0);
    assert_eq!(report.blessed(), 0);
}

#[test]
fn cases_are_visited_in_a_stable_sorted_order() {
    let dir = scratch("order");
    for name in ["c", "a", "b"] {
        write(dir.join(format!("{name}.link")), "x\n");
        write(dir.join(format!("{name}.stderr")), "out\n");
    }

    let report = run_ui_tests_with(&dir, UiTestOptions::default(), constant("out\n"));

    let names: Vec<String> = report
        .passed_cases()
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["a.link", "b.link", "c.link"]);
}

#[test]
fn nested_directories_are_walked() {
    let dir = scratch("nested");
    write(dir.join("top.link"), "x\n");
    write(dir.join("top.stderr"), "out\n");
    write(dir.join("regions").join("deep.link"), "x\n");
    write(dir.join("regions").join("deep.stderr"), "out\n");

    let report = run_ui_tests_with(&dir, UiTestOptions::default(), constant("out\n"));

    assert_eq!(report.passed(), 2);
}

#[test]
fn an_empty_directory_produces_an_empty_report() {
    let dir = scratch("empty");

    let report = run_ui_tests_with(&dir, UiTestOptions::default(), constant("out\n"));

    assert_eq!(report.total(), 0);
    assert!(report.is_ok());
}

#[test]
fn the_compile_closure_receives_the_path_and_the_source_text() {
    let dir = scratch("arguments");
    write(dir.join("case.link"), "fn main():\n    bad\n");
    write(dir.join("case.stderr"), "case.link|fn main():\n    bad\n");

    let report =
        run_ui_tests_with(&dir, UiTestOptions::default(), |path: &Path, source: &str| {
            format!("{}|{}", path.file_name().unwrap().to_string_lossy(), source)
        });

    assert_eq!(report.failed(), 0, "{}", report.summary());
}

#[test]
#[should_panic(expected = "1 failed")]
fn assert_success_panics_with_the_summary_when_a_case_fails() {
    let dir = scratch("panic");
    write(dir.join("case.link"), "x\n");
    write(dir.join("case.stderr"), "expected\n");

    run_ui_tests_with(&dir, UiTestOptions::default(), constant("actual\n")).assert_success();
}

#[test]
fn assert_success_is_quiet_when_nothing_fails() {
    let dir = scratch("no-panic");
    write(dir.join("case.link"), "x\n");
    write(dir.join("case.stderr"), "same\n");

    run_ui_tests_with(&dir, UiTestOptions::default(), constant("same\n")).assert_success();
}

#[test]
fn the_summary_counts_passes_failures_and_blessings() {
    let dir = scratch("summary");
    write(dir.join("a.link"), "x\n");
    write(dir.join("a.stderr"), "out\n");
    write(dir.join("b.link"), "x\n");
    write(dir.join("b.stderr"), "different\n");
    write(dir.join("c.link"), "x\n"); // no expectation: gets blessed

    let report = run_ui_tests_with(&dir, UiTestOptions::default(), constant("out\n"));

    assert_eq!((report.passed(), report.failed(), report.blessed()), (1, 1, 1));
    assert_eq!(report.total(), 3);

    let summary = report.summary();
    assert!(summary.contains("1 passed"), "{summary}");
    assert!(summary.contains("1 failed"), "{summary}");
    assert!(summary.contains("1 blessed"), "{summary}");
    // The summary is what a developer reads after `cargo test`, so the diff
    // has to be inside it.
    assert!(summary.contains("different"), "{summary}");
    // And the way to update the expectation.
    assert!(summary.contains("LINK_BLESS=1"), "{summary}");
}

#[test]
fn a_missing_directory_is_reported_rather_than_panicking() {
    let dir: PathBuf = scratch("absent").join("does-not-exist");

    let report = run_ui_tests_with(&dir, UiTestOptions::default(), constant("out\n"));

    assert_eq!(report.failed(), 1);
    assert!(report.summary().contains("does-not-exist"), "{}", report.summary());
}
