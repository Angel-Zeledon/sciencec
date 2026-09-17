//! `science_panic` aborts the process and `science_exit` ends it, so neither
//! can be observed from inside the process that calls it. These tests re-run
//! this same test binary as a child, ask it to do one thing, and inspect what
//! came out.
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
            science_write(&out);
            let message = s("and then it panicked");
            science_panic(&message);
        },
        "print" => unsafe {
            let a = s("alpha");
            let b = s("beta");
            science_write(&a);
            science_write(&b);
            science_print(&a);
            science_print(&b);
            let empty = science_string_new();
            science_print(&empty);
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
        // `script-mode.md` §2.3's fourth row, as the emitted `main` performs
        // it: the whole message including the prefix and the newline is the
        // caller's, then the status.
        "script_failed" => unsafe {
            let out = s("printed before the failure");
            science_write(&out);
            free(out);
            let message = b"error: something went wrong\n";
            science_write_error_bytes(message.as_ptr(), message.len());
            science_exit(1);
        },
        // Rows 1 to 3. The `write` has no newline in it, so nothing but
        // `science_exit`'s own flush can get it out of the buffer — which is
        // the reason the emitted `main` ends here rather than at a `ret`.
        "script_ok" => unsafe {
            let out = s("no newline in sight");
            science_write(&out);
            free(out);
            science_exit(0);
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
fn write_is_verbatim_and_print_adds_one_newline() {
    let output = run_child("print");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("alphabetaalpha\nbeta\n\n"),
        "unexpected stdout: {stdout:?}"
    );
}

/// The failing row of `script-mode.md` §2.3, end to end through the two symbols
/// the emitted `main` calls.
///
/// **Both halves, because either alone is half a test.** A status of 1 with
/// nothing on stderr is a program that fails silently; a message with the abort
/// status is what this replaced. The third assertion is the one that says it is
/// not a panic: `science_panic_bytes` prefixes `panic: `, and a failing script
/// is not a crash.
#[test]
fn write_error_bytes_then_exit_is_status_one_with_the_message_on_stderr() {
    let output = run_child("script_failed");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "stderr was: {stderr:?}");
    assert!(
        stderr.contains("error: something went wrong\n"),
        "the message must reach stderr verbatim, got: {stderr:?}"
    );
    assert!(!stderr.contains("panic:"), "a failing script is not a panic, got: {stderr:?}");
    // And stdout is flushed rather than lost, for the same reason the panic
    // path flushes: the last thing printed is usually the explanation.
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("printed before the failure"),
        "stdout was lost"
    );
}

/// `science_exit(0)` is a successful exit, and it takes the buffer with it.
///
/// The `write` in the child has no newline, so a `LineWriter` holds it. Nothing
/// in a Science binary flushes at exit — Rust's `lang_start` never runs, because
/// the entry point is the `main` codegen emitted — so if this passes it is
/// because `science_exit` flushed, and if `science_exit` stopped flushing this
/// is the test that notices.
#[test]
fn exit_zero_succeeds_and_flushes_what_print_left_behind() {
    let output = run_child("script_ok");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        "",
        "a successful exit says nothing on stderr"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("no newline in sight"),
        "an unterminated line was buffered and never flushed: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}
