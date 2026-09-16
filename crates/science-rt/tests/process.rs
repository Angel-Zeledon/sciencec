//! `science_panic` aborts the process, so it cannot be observed from inside the
//! process that calls it. These tests re-run this same test binary as a child,
//! ask it to do one thing, and inspect what came out.
//!
//! Skipped under Miri, which cannot spawn processes.

#![cfg(not(miri))]

mod common;

use common::{free, s};
use science_rt::*;

const ROLE: &str = "LINK_RT_CHILD_ROLE";

/// The child entry point. It runs as an ordinary `#[test]`, but only does
/// anything when the parent has set `LINK_RT_CHILD_ROLE`; otherwise the normal
/// test run would abort itself.
#[test]
fn child_entry_point() {
    let Ok(role) = std::env::var(ROLE) else {
        return;
    };
    match role.as_str() {
        "panic" => unsafe {
            let message = s("the sky is falling");
            science_panic(&message);
        },
        "panic_after_print" => unsafe {
            // Buffered stdout must reach the terminal before the abort, or the
            // program's last words are lost exactly when they matter most.
            let out = s("printed before the panic");
            science_println(&out);
            let message = s("and then it panicked");
            science_panic(&message);
        },
        "print" => unsafe {
            let a = s("alpha");
            let b = s("beta");
            science_print(&a);
            science_print(&b);
            science_println(&a);
            science_println(&b);
            let empty = science_string_new();
            science_println(&empty);
            free(empty);
            free(b);
            free(a);
        },
        "abort" => {
            science_abort();
        }
        "panic_bytes" => unsafe {
            let message = b"a runtime-internal failure";
            science_panic_bytes(message.as_ptr(), message.len());
        },
        other => panic!("unknown child role {other:?}"),
    }
}

fn run_child(role: &str) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().expect("this test binary"))
        .arg("--exact")
        .arg("child_entry_point")
        .arg("--nocapture")
        .env(ROLE, role)
        .output()
        .expect("spawn the child")
}

#[test]
fn panic_prints_the_message_to_stderr_and_aborts() {
    let output = run_child("panic");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("the sky is falling"),
        "the message must reach stderr, got: {stderr:?}"
    );
    assert!(
        stderr.contains("panic"),
        "the message must be labelled a panic, got: {stderr:?}"
    );
    assert!(
        !output.status.success(),
        "a panic must not exit successfully"
    );
}

#[test]
fn panic_bytes_prints_the_message_to_stderr_and_aborts() {
    let output = run_child("panic_bytes");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("a runtime-internal failure"),
        "got: {stderr:?}"
    );
    assert!(!output.status.success());
}

#[test]
fn panic_flushes_stdout_first() {
    let output = run_child("panic_after_print");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("printed before the panic"),
        "stdout was lost on abort, got: {stdout:?}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("and then it panicked"), "got: {stderr:?}");
    assert!(!output.status.success());
}

#[test]
fn abort_exits_unsuccessfully() {
    let output = run_child("abort");
    assert!(!output.status.success());
}

#[test]
fn print_writes_verbatim_and_println_adds_one_newline() {
    let output = run_child("print");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("alphabetaalpha\nbeta\n\n"),
        "unexpected stdout: {stdout:?}"
    );
}
