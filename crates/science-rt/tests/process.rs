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
        // The other buffered layer, and the one nothing used to empty. C's
        // `stdout` is a different buffer from Rust's `LineWriter`, and a
        // Science program that calls a C library writes through it. `putchar`
        // rather than a Science entry point because the entry points all go
        // through Rust; this is the layer under them.
        "c_stdio" => unsafe {
            unsafe extern "C" {
                fn putchar(c: core::ffi::c_int) -> core::ffi::c_int;
            }
            putchar(b'A' as core::ffi::c_int);
            science_exit(0);
        },
        // `strings-formatting-and-docs.md` §4.2's two stderr rows. Written
        // between two stdout writes so the test can say which stream each
        // byte went to, not merely that it arrived somewhere.
        "print_error" => unsafe {
            let a = s("alpha");
            let b = s("beta");
            science_print(&a);
            science_write_error(&a);
            science_write_error(&b);
            science_print_error(&a);
            science_print_error(&b);
            let empty = science_string_new();
            science_print_error(&empty);
            science_print(&b);
            free(empty);
            free(b);
            free(a);
            science_exit(0);
        },
        // §4.2's one case for `flush`: a `write` with no newline, which the
        // `LineWriter` holds. `std::process::abort` runs no flush of its own —
        // unlike `science_abort` and `science_exit`, which both do — so the
        // bytes reach the pipe only if `science_flush` put them there.
        "flush" => unsafe {
            let progress = s("working...");
            science_write(&progress);
            science_flush();
            free(progress);
            std::process::abort();
        },
        // The control for `flush`: the same program without the call. If this
        // one's bytes arrive too, the `flush` test above proves nothing.
        "no_flush" => unsafe {
            let progress = s("working...");
            science_write(&progress);
            free(progress);
            std::process::abort();
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

/// `science_exit` empties **C's** buffered output too, and that is the half
/// that was missing.
///
/// **The program that found it wrote one byte and printed nothing.**
/// `codegen-and-linking.md` §10's stage 2 is a call into a C library, and the
/// smallest program that proves the path works is one whose output the C
/// library writes — `putchar(65)` through an `extern "C"` block. It emitted the
/// right call, linked, exited 0 and produced no output, because
/// `std::process::exit` on `x86_64-pc-windows-msvc` ends the process without
/// running the C runtime's stream teardown. This module's own documentation
/// claimed *"`atexit` handlers and C stdio flushing happen"*; the first half
/// was true.
///
/// **It is invisible at a terminal**, which is why it survived being tried by
/// hand: a console stream is line-buffered and a pipe is not, so the byte comes
/// out when a person runs the program and is lost under every harness that
/// captures the output. This test captures it, which is the only way to see it.
#[test]
fn exit_flushes_what_a_c_library_wrote() {
    let output = run_child("c_stdio");
    assert_eq!(output.status.code(), Some(0));
    // The child is a test binary, so libtest's own banner is on the same
    // stream; the byte is the last thing written and `ends_with` is the
    // assertion that does not depend on what libtest printed first.
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.ends_with('A'),
        "a byte written through C's `stdout` was still in its buffer at exit: {stdout:?}"
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

/// `print_error` and `write_error` go to **stderr**, and divide it exactly as
/// `print` and `write` divide stdout: one appends a `\n` and the other adds
/// nothing.
///
/// **Both streams are asserted, and the stdout half is the one that proves
/// the routing.** A stderr writer that also echoed to stdout would pass a test
/// that only read stderr; this one requires stdout to hold the two `print`s
/// and nothing between them.
#[test]
fn write_error_is_verbatim_and_print_error_adds_one_newline_on_stderr() {
    let output = run_child("print_error");
    assert_eq!(output.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr, "alphabetaalpha\nbeta\n\n", "stderr");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.ends_with("alpha\nbeta\n"), "stdout: {stdout:?}");
    assert!(!stdout.contains("alphabeta"), "a stderr write reached stdout: {stdout:?}");
}

/// `science_flush` empties stdout's buffer **without** ending the process.
///
/// The child writes a line with no newline, flushes, and then aborts through
/// `std::process::abort`, which flushes nothing. `no_flush` is the control:
/// the same program without the call, whose bytes must be lost — otherwise the
/// pipe would not be buffering and the first assertion would be vacuous.
#[test]
fn flush_makes_an_unterminated_write_visible_before_the_process_dies() {
    let flushed = run_child("flush");
    assert!(!flushed.status.success());
    let stdout = String::from_utf8_lossy(&flushed.stdout);
    assert!(stdout.ends_with("working..."), "the write was still buffered: {stdout:?}");

    let control = run_child("no_flush");
    assert!(!control.status.success());
    let stdout = String::from_utf8_lossy(&control.stdout);
    assert!(
        !stdout.contains("working..."),
        "stdout was not buffered, so the test above proves nothing: {stdout:?}"
    );
}
