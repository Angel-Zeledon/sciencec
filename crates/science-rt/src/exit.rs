//! Ending a program with a status, and saying why on standard error.
//!
//! # The decision
//!
//! Two entry points, both **codegen support**, both reachable only from the
//! `main` that `science-codegen-llvm` emits:
//!
//! - [`science_write_error_bytes`] writes raw bytes to standard error **and
//!   returns**;
//! - [`science_exit`] flushes standard output and ends the process with a
//!   status the caller chooses.
//!
//! # The reason
//!
//! `script-mode.md` §2.3 gives a script's exit table, and its fourth row —
//! *`return err`, `err` is not null* — asks for exit status **1** after
//! `error: ` and the error's rendering on **stderr**. Nothing in this crate
//! could do that. `science_print` and `science_write` go to stdout;
//! [`science_panic_bytes`](crate::science_panic_bytes) goes to stderr and then
//! calls `abort()`, whose status is `SIGABRT` on POSIX and `3` on Windows and
//! is not 1. `codegen-and-linking.md` §9.3 finding 5 states the consequence in
//! one sentence: *"a hello world can be emitted from this page. A `main` that
//! returns an error cannot."* These two symbols are that finding's repair, and
//! §13 of that note names them in the same breath — *"add a stderr writer that
//! does not abort, and `science_exit(code: I32)`"*.
//!
//! **Why the writer does not abort, and why that is the whole point.**
//! `science_panic_bytes` is not a writer that happens to abort; aborting is its
//! contract, and `panic.rs` argues at length that F0 has no way to observe an
//! unwind and so no reason to grow one. A failing script is not a panic: the
//! program did the thing the language asked it to do — `if err?: return err` —
//! and the process must end **normally**, with a status a shell can test.
//! Reusing the panic path would report the language's own error idiom as a
//! runtime crash.
//!
//! **Why the writer takes bytes and not a `String`.** Same reason
//! `science_panic_bytes` does: the message is a `private unnamed_addr constant`
//! in the binary (Decision 15), and building a `ScienceString` around it would
//! mean an allocation and a matching free on the path that is about to exit.
//! There is no `science_write_error(text: *const ScienceString)` beside this
//! one, because codegen has no call site for it — and §2.6's rule is that the
//! table of runtime symbols is the set codegen calls, so a symbol nothing calls
//! is dead weight the linker still carries.
//!
//! **Why `science_exit` exists at all, when C's `main` could `return 1`.** A
//! Science binary's entry point is the emitted `main`, so Rust's `lang_start`
//! never runs and **nothing installs a flush of Rust's buffered stdout**.
//! Returning from `main` would end the process through the C runtime, which
//! flushes C's stdio and knows nothing about the `LineWriter` inside
//! `std::io::stdout()`. Today that is invisible, because Science's `print`
//! appends a newline and a `LineWriter` flushes on one — but `write` does not,
//! and the first program that ends with a `write` would lose its last line.
//! Routing both exits through one symbol that flushes first makes the buffer
//! somebody else's problem exactly once.
//!
//! # The cost
//!
//! **Neither symbol renders anything.** [`science_write_error_bytes`] is a
//! writer: it has no idea what an error is, and it cannot, because the runtime
//! is not generic over Science types and an `any Error` is a two-word fat
//! pointer whose vtable this crate does not define. §2.3 asks for the error's
//! `Display`; what reaches this function is whatever bytes codegen chose. See
//! `science-codegen-llvm`'s `lower` for what those bytes are and how far short
//! of `Display` they fall.
//!
//! **`science_exit` runs no destructor.** It is `std::process::exit`, so
//! `atexit` handlers run and nothing else does: no
//! Science value is dropped, no `_free` is called, no memory is returned. That
//! is correct rather than a shortcut — the process is ending and the operating
//! system reclaims the address space — but it means a `Drop` with an
//! observable effect (a temporary file removed, a lock released) does **not**
//! run on the exit path. `script-mode.md` §6.2 drops a script's bindings at the
//! end of the script body, which is *before* `main` calls this, so F0's one
//! such case is already handled above here; the cost is owed by whatever later
//! phase puts a value's lifetime past the script body's end.
//!
//! **Neither symbol is in §2's `sret` list, and neither can be.** That list is
//! the entry points returning an aggregate by value; one of these returns
//! nothing and the other returns `!`. The list is derived from the signatures
//! by `science-codegen`'s `runtime.rs` rather than maintained by hand, so
//! adding these two moved the count of entry points from 45 to 47 and left the
//! `sret` set at the same nine it was. That is §2's own lesson working: a
//! hand-maintained list of everything with a property gets this wrong, and a
//! derived one cannot.

use std::io::Write;

// C's own buffered output, flushed.
//
// **This is here because a program that called one C function and printed
// nothing is how it was found.** `codegen-and-linking.md` §10's stage 2 is *a
// call into a C library*, and the smallest program that proves the path works
// is one whose output the C library writes. That program —
// `putchar(65)` through an `extern "C"` block — emitted the right IR, linked
// cleanly, exited 0 and **printed nothing**, because the byte was in the C
// runtime's `stdout` buffer when the process ended and nothing emptied it.
//
// **Why nothing did.** Rust's `std::process::exit` on
// `x86_64-pc-windows-msvc` ends the process without running the C runtime's
// stream teardown, and the buffer is only invisible when standard output is a
// terminal — a console is line-buffered, a pipe is not — so the failure does
// not appear when a program is run by hand and does appear under every test
// harness that captures output. This module's own documentation used to claim
// *"`atexit` handlers and C stdio flushing happen"*; the first half is true
// and the second was not.
//
// `fflush(NULL)` is C's "flush every output stream", which is what is wanted:
// the runtime cannot know which streams a library the program linked has
// written to, and naming `stdout` alone would lose whatever a library wrote
// to `stderr` or to a file it opened.
//
// **What it costs.** One declaration of a libc symbol in a crate that
// otherwise reaches the platform only through `std`, and it is not free of
// assumptions: `fflush` flushes the streams of *the C runtime this image
// links*, so a Windows program that pulled in a DLL built against a different
// CRT has a second set of buffers this does not reach. That is a fact about
// mixing C runtimes on Windows rather than about Science, it is the same
// limitation every C program has, and the alternative — not flushing —
// discards the output of every C library Science can call.
unsafe extern "C" {
    // `int fflush(FILE *stream)`; a null `stream` means all of them.
    fn fflush(stream: *mut core::ffi::c_void) -> core::ffi::c_int;
}

/// Flush both buffered layers a Science process writes through: Rust's
/// `LineWriter` inside `std::io::stdout()`, and the C runtime's streams.
///
/// **Both, and in that order.** They are different buffers with no knowledge
/// of each other: `print` goes through the first and a C library's `printf`
/// goes through the second, and flushing one leaves the other's bytes in
/// memory. Rust's goes first so that a program which printed and then called a
/// C library gets its output in the order it wrote it — which
/// `stdlib-core.md` §4.1 does not guarantee across streams, and which is worth
/// having anyway for the same reason `science_write_error_bytes` flushes
/// stdout before writing to stderr.
fn flush_all() {
    let _ = std::io::stdout().lock().flush();
    // SAFETY: a null `stream` is `fflush`'s documented "every output stream".
    unsafe { fflush(core::ptr::null_mut()) };
}

/// Write bytes to standard error, verbatim, and return.
///
/// **Codegen support.** The emitted `main` calls this on the failing edge of
/// `script-mode.md` §2.3's exit table, with a static message, and then calls
/// [`science_exit`]. It is the *non-aborting* half that
/// [`science_panic_bytes`](crate::science_panic_bytes) is not.
///
/// Named for the Science spelling it will eventually implement:
/// `stdlib-core.md` §4.1 gives `write_error` as the stderr form that **adds
/// nothing** and `print_error` as the form that appends a newline, exactly as
/// `write` and `print` divide stdout between them. This is the `write_error`
/// side, so the caller supplies its own line terminator. §8 of the crate
/// documentation records what happened the last time two symbols in this crate
/// were named the other way round, which is why the distinction is spelled out
/// here rather than left to the reader.
///
/// Standard output is flushed first. `stdlib-core.md` §4.1 says cross-stream
/// ordering *"is not guaranteed"*, so this is a courtesy rather than a
/// contract — but it is the same courtesy `science_panic_bytes` pays, and for
/// the same reason: the program's last `print` is very often the one that
/// explains what it was doing when it failed.
///
/// Invalid UTF-8 is written through unchanged. `science_panic_bytes` replaces
/// it, because a panic is no place to raise a second failure; here there is no
/// second failure to raise — the bytes go to a file descriptor, not through a
/// `str`.
///
/// A write error is ignored, for `science_write`'s reason: a program whose
/// standard error has gone away has no better answer available to it than
/// carrying on to its exit.
///
/// # Safety
///
/// `ptr` must point to `len` readable, initialised bytes. `len == 0` is fine;
/// `ptr` must still be non-null and aligned.
#[no_mangle]
pub unsafe extern "C" fn science_write_error_bytes(ptr: *const u8, len: usize) {
    // SAFETY: the caller guarantees `len` readable bytes at `ptr`.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    flush_all();
    let mut err = std::io::stderr().lock();
    let _ = err.write_all(bytes);
    let _ = err.flush();
}

/// End the process with `code`, after flushing standard output.
///
/// **Codegen support.** The emitted `main` ends both of its paths here: `0`
/// where the script body returned null, `1` where it did not
/// (`script-mode.md` §2.3). Nothing in Science calls it; §8's library has no
/// `exit`, and adding one is a spec change rather than a side effect of this
/// symbol existing.
///
/// `code` is an `i32` because that is what the platform takes — `int` in C's
/// `exit`, and `DWORD`-truncated-from-`int` on Windows. Only the low eight bits
/// survive on POSIX, so `science_exit(256)` is a successful exit; F0 emits `0`
/// and `1` and nothing else, so that cannot bite yet, and it is stated here
/// rather than clamped because clamping would be this crate inventing a policy
/// no note has.
///
/// The flush is the reason this is a symbol rather than a `ret` in the emitted
/// `main`; the module documentation gives it in full. It is a no-op on the
/// failing path, where [`science_write_error_bytes`] has already flushed, and
/// it is the only flush on the succeeding one.
///
/// **It flushes C's buffers as well as Rust's**, and that is not belt and
/// braces: a Science program that calls a C library writes through the C
/// runtime's `stdout`, which nothing else here empties. See [`flush_all`] for
/// the program that found this and for what the declaration assumes.
///
/// Nothing is unwound and no destructor runs. See the module documentation.
#[no_mangle]
pub extern "C" fn science_exit(code: i32) -> ! {
    flush_all();
    std::process::exit(code)
}
