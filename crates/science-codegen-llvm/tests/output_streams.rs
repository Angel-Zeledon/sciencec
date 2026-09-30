//! `print_error`, `write_error` and `flush`, **built, linked, run**, with both
//! streams captured and asserted.
//!
//! # What these are
//!
//! `strings-formatting-and-docs.md` §4.2's table has five output rows and the
//! compiler had three of them: `print`, `write` and `panic`. The other two —
//! `print_error` and `write_error`, the stderr twins of the first two — and the
//! free function `flush` were names §4.3 added to §8 and `science-resolve`'s
//! `builtins.rs` kept out of the prelude until `science-rt` had an entry point
//! behind each. They have them now: `science_print_error`,
//! `science_write_error` and `science_flush`.
//!
//! # Why every test here reads both streams
//!
//! Every other execution test in this directory asserts that stderr is
//! **empty**, because nothing they build has any business writing to it. These
//! programs do, so the helper below returns both and each test says what
//! belongs on which. The stdout half is the one that proves the routing: a
//! stderr writer that also echoed to stdout would pass any test that read
//! stderr alone.
//!
//! # Why `flush` is tested by ending the process with `_exit`
//!
//! A Science program ends through `science_exit`, which flushes stdout on the
//! way out — so a program that calls `write`, then `flush()`, then returns,
//! prints the same bytes whether or not `flush` did anything. The test has to
//! end the process by a door that flushes nothing, and libc's `_exit` is that
//! door: it is reached through an `extern` block, exactly as a C library would
//! be, and it discards whatever is still in a buffer. The control program is
//! the same source without the `flush()` line, and its output must be empty —
//! otherwise stdout was not buffering under the harness's pipe and the first
//! assertion proved nothing.

#![cfg(feature = "llvm")]

mod harness;

use harness::{Ran, executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

/// Build one program at `-O2` and give back both streams and the status.
///
/// `-O2` for `tests/printing.rs`'s reason: a `print` is `alloca`s and
/// addresses, and the optimiser is where a load of something freed stops
/// looking like a load of something live.
fn streams(name: &str, source: &str) -> Ran {
    let dir = scratch("output_streams", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    ran
}

/// **`print_error` appends a newline and `write_error` adds nothing**, on
/// stderr, and stdout sees neither.
#[test]
fn the_stderr_pair_divides_stderr_as_print_and_write_divide_stdout() {
    let ran = streams(
        "pair",
        "print(\"result\")\n\
         write_error(\"a\")\n\
         write_error(\"b\")\n\
         print_error(\"c\")\n\
         print_error(\"\")\n\
         print(\"done\")\n",
    );
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "abc\n\n");
    assert_eq!(ran.stdout, "result\ndone\n");
}

/// **A non-`String` is rendered, exactly as `print`'s is.** §4.1 gives the
/// pair `print`'s `borrowed any Display` parameter, so `print_error(42)` has
/// to mean what `print(42)` means: `science-mir`'s `prints_by_rendering` builds
/// the text through the f-string builder and `lower_print` hands it to the
/// stderr symbol. Each width the builder renders is here once, and an
/// interpolation, which reaches the pair already a `String`.
#[test]
fn a_value_that_is_not_a_string_is_rendered_through_display() {
    let ran = streams(
        "rendered",
        "let n be 42\n\
         print_error(n)\n\
         print_error(2.5)\n\
         print_error(true)\n\
         write_error(-7)\n\
         write_error(' ')\n\
         print_error(f\"{n} rows\")\n",
    );
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "42\n2.5\ntrue\n-7 42 rows\n");
    assert_eq!(ran.stdout, "");
}

/// **A binding printed to stderr is still the program's afterwards.**
///
/// `tests/printing.rs`'s finding, asked of the new pair: MIR hands a
/// signature-less callee a `copy`, `lower_print` frees only a `move` or a
/// literal, and the scope's drop releases the binding once. Were the pair to
/// take the other arm, the second use would read a freed buffer — an empty
/// line here, or an abort inside the interpolation.
#[test]
fn a_binding_survives_every_print_error_of_it() {
    let ran = streams(
        "binding",
        "let s be \"hola\"\n\
         print_error(s)\n\
         write_error(s)\n\
         print_error(s)\n\
         print(f\"{s}!\")\n",
    );
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "hola\nholahola\n");
    assert_eq!(ran.stdout, "hola!\n");
}

/// A field of a record, which reaches `lower_print` as a projected place and
/// not a bare local — the arm `print(b.label)` needed its own fix for.
#[test]
fn a_string_field_goes_to_stderr_without_being_moved_out() {
    let ran = streams(
        "field",
        "type Doc:\n\
         \x20   title: String\n\
         \n\
         def main():\n\
         \x20   let doc be Doc(title: \"report\")\n\
         \x20   print_error(doc.title)\n\
         \x20   print(doc.title)\n",
    );
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "report\n");
    assert_eq!(ran.stdout, "report\n");
}

/// The `extern` block that reaches `_exit`, the one way out of a Science
/// program that flushes nothing. See the module documentation.
const EXIT_WITHOUT_FLUSHING: &str = "unsafe extern \"C\" library \"c\":\n    def _exit(status: I32)\n\n";

/// **`flush()` makes an unterminated `write` visible**, which is §4.2's one
/// reason for it: *"`write` of a progress line with no newline is
/// line-buffered into invisibility otherwise"*.
///
/// Both programs end through `_exit`, and only the one that called `flush()`
/// delivered its bytes. The status is asserted as `0` in both, so that an
/// `_exit` that failed to link or to run cannot pass as a lost buffer.
#[test]
fn flush_delivers_a_partial_line_that_exit_without_flushing_would_lose() {
    let flushed = streams(
        "flushed",
        &format!("{EXIT_WITHOUT_FLUSHING}write(\"working...\")\nflush()\nunsafe: _exit(0)\n"),
    );
    assert_eq!(flushed.status, Some(0), "stderr: {}", flushed.stderr);
    assert_eq!(flushed.stdout, "working...", "`flush()` left the write in the buffer");
    assert_eq!(flushed.stderr, "");

    let control = streams(
        "unflushed",
        &format!("{EXIT_WITHOUT_FLUSHING}write(\"working...\")\nunsafe: _exit(0)\n"),
    );
    assert_eq!(control.status, Some(0), "stderr: {}", control.stderr);
    assert_eq!(
        control.stdout, "",
        "stdout was not buffered under the harness's pipe, so the test above proves nothing"
    );
}

/// `flush()` is a statement of `()`, and a program may call it any number of
/// times, including with nothing to flush.
#[test]
fn flush_with_an_empty_buffer_is_harmless() {
    let ran = streams("empty", "flush()\nprint(\"a\")\nflush()\nflush()\nprint(\"b\")\n");
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stdout, "a\nb\n");
    assert_eq!(ran.stderr, "");
}
