//! `SCIENCE_BLESS=1` in the environment turns blessing on.
//!
//! This lives in its own test binary on purpose: `cargo test` runs each
//! integration test file as a separate process, so mutating the environment
//! here cannot leak into the tests in `ui_harness.rs`, which run in parallel
//! and assume `SCIENCE_BLESS` is unset.
//!
//! It is also deliberately a single test function, because two tests in one
//! binary would run in parallel and fight over the same variable.

use std::path::Path;

use science_testkit::{run_ui_tests, UiTestOptions};

mod support;
use support::{read, scratch, write};

fn constant(text: &'static str) -> impl Fn(&Path, &str) -> String {
    move |_path, _source| text.to_string()
}

#[test]
fn science_bless_in_the_environment_rewrites_a_stale_expectation() {
    let dir = scratch("env-bless");
    write(dir.join("case.science"), "x\n");
    write(dir.join("case.stderr"), "stale\n");

    // Unset: the case fails and the file on disk is left alone.
    std::env::remove_var("SCIENCE_BLESS");
    assert!(!UiTestOptions::from_env().bless);

    let before = run_ui_tests(&dir, constant("current\n"));
    assert_eq!(before.failed(), 1);
    assert_eq!(read(dir.join("case.stderr")), "stale\n");

    // Only the documented value turns blessing on.
    std::env::set_var("SCIENCE_BLESS", "0");
    assert!(!UiTestOptions::from_env().bless, "0 must not bless");
    std::env::set_var("SCIENCE_BLESS", "");
    assert!(!UiTestOptions::from_env().bless, "an empty value must not bless");

    std::env::set_var("SCIENCE_BLESS", "1");
    assert!(UiTestOptions::from_env().bless);

    let after = run_ui_tests(&dir, constant("current\n"));
    assert_eq!(after.blessed(), 1);
    assert_eq!(after.failed(), 0);
    assert_eq!(read(dir.join("case.stderr")), "current\n");

    std::env::remove_var("SCIENCE_BLESS");
}
