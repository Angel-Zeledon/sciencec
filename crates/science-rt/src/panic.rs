//! Panic and abort.
//!
//! # Why there is no unwinding
//!
//! A panic prints its message to standard error and terminates the process.
//! Nothing is unwound: no destructor runs, no frame is popped, no landing pad
//! is emitted anywhere in a Science binary.
//!
//! This is deliberate. F0 has no way to catch a panic — §8's library has no
//! `catch`, `try` or `recover`, and §12 puts nothing of the sort in scope — so
//! there is no program that could observe an unwind. Supporting one anyway
//! would mean personality routines, LLVM `invoke` at every call that might
//! panic, `landingpad` blocks, unwind tables in the binary, and a cleanup path
//! through every MIR block that the region checker would then have to reason
//! about. That is a large amount of machinery for something nothing uses yet,
//! and it would have to be built before the first Science program ever ran.
//!
//! When it is eventually wanted, F1 is the phase that will want it: an actor
//! that fails should be restarted by its supervisor rather than take the
//! process down with it. Adding it then is additive — codegen starts emitting
//! `invoke`, this module grows a real unwind — and, importantly, F1's
//! supervision design gets to decide what a failed actor's state means before
//! the mechanism is fixed. Deciding that now, with no supervisor to answer to,
//! would be guessing.
//!
//! # Standard output is flushed before the message
//!
//! **Decision.** A panic empties every stdout layer — `stdout.rs`'s 64 KiB
//! block buffer, Rust's `LineWriter` behind it, and C's streams — and only
//! then writes `panic: …` to stderr and aborts. `science_abort` does the same
//! without the message.
//!
//! **Reason.** In a pipe, stdout is block-buffered (`stdout.rs`), so what a
//! program printed before it failed is up to 64 KiB of whole lines rather than
//! at most one partial one. `abort()` runs no `atexit` handler, so without
//! this flush all of it — very often including the line that explains what the
//! program was doing when it failed — would be discarded exactly when it
//! matters most. Flushing *before* the message keeps the two in the order they
//! were written when both streams go to the same place.
//!
//! **Cost.** C's streams are flushed too, which the panic path did not do
//! before block buffering: harmless, since a flush discards nothing, and a C
//! library's last output is as worth keeping as Science's. The stdout lock is
//! taken with `try_lock`, so a panic can never wait on it; see
//! `stdout::flush_before_abort` for why it is always free in F0.

use std::io::Write;

use crate::string::ScienceString;

/// Terminate the process immediately, with no message.
///
/// The exit status is the platform's abort status: never a success.
#[no_mangle]
pub extern "C" fn science_abort() -> ! {
    flush_stdout();
    std::process::abort()
}

/// Science's `panic(message: &String) -> Never`.
///
/// Prints `panic: <message>` to standard error and aborts. Does not return,
/// and does not unwind; see the module documentation for why.
///
/// `Never` (§4.5) is the type of this call in Science, and it coerces to any type,
/// which is what lets a `match` arm panic.
///
/// # Safety
///
/// `message` must be a non-null, aligned pointer to a live [`ScienceString`].
#[no_mangle]
pub unsafe extern "C" fn science_panic(message: *const ScienceString) -> ! {
    // SAFETY: the caller guarantees `message` points to a live `ScienceString`.
    let text = unsafe { &*message };
    // SAFETY: a live `ScienceString` owns `len` initialised bytes at `ptr`.
    unsafe { science_panic_bytes(text.ptr, text.len) }
}

/// Panic with a message given as raw UTF-8 bytes.
///
/// **Codegen support.** This is what the runtime itself calls when it fails
/// (allocation failure, capacity overflow), and what codegen can call to raise
/// a panic whose message is a static string in the binary — `Option::unwrap`
/// on `None`, `Result::unwrap` on `Err`, an arithmetic overflow — without
/// having to build a `String` on a path that is about to abort anyway.
///
/// # Safety
///
/// `ptr` must point to `len` readable, initialised bytes. They need not be
/// valid UTF-8; invalid sequences are replaced rather than rejected, because a
/// panic is no place to raise a second failure.
#[no_mangle]
pub unsafe extern "C" fn science_panic_bytes(ptr: *const u8, len: usize) -> ! {
    // SAFETY: the caller guarantees `len` readable bytes at `ptr`. `len == 0`
    // is fine: `ptr` is still required to be non-null and aligned, and
    // `from_raw_parts` accepts a dangling pointer for an empty slice.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };

    flush_stdout();

    let mut err = std::io::stderr().lock();
    let _ = err.write_all(b"panic: ");
    let _ = err.write_all(String::from_utf8_lossy(bytes).as_bytes());
    let _ = err.write_all(b"\n");
    let _ = err.flush();
    drop(err);

    std::process::abort()
}

fn flush_stdout() {
    crate::stdout::flush_before_abort();
}
