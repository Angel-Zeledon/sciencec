//! Standard output, and the one buffer this crate keeps in front of it.
//!
//! # The decision
//!
//! **stdout is line-buffered when it is a terminal and block-buffered in
//! 64 KiB otherwise**, which is `strings-formatting-and-docs.md` §4.2's first
//! bullet and `stdlib-core.md` §4.1's restatement of it. The choice is made
//! once, at the first `print` or `write`, by asking whether file descriptor 1
//! is a terminal ([`std::io::IsTerminal`]), and it is kept for the life of the
//! process.
//!
//! - **A terminal** writes through Rust's `std::io::stdout()` as before. That
//!   is a `LineWriter`, and a `LineWriter` is exactly the line-buffered half of
//!   the policy: a `\n` sends everything up to it.
//! - **Anything else** — a pipe, a file, `/dev/null` — collects bytes in a
//!   64 KiB `Vec` owned here and hands them to `std::io::stdout()` a block at a
//!   time, flushing that too, so nothing is left behind in the `LineWriter`
//!   after a block goes out.
//!
//! Every way out of the process that can deliver the buffer does:
//! [`science_flush`](crate::science_flush) and
//! [`science_exit`](crate::science_exit) through `exit::flush_all`,
//! [`science_panic_bytes`](crate::science_panic_bytes) and
//! [`science_abort`](crate::science_abort) through [`flush_before_abort`], and
//! any other `exit(3)` — a C library that calls `exit`, a `main` that returns
//! through the C runtime — through an `atexit` handler registered at the same
//! first use. `_exit(2)` and a signal are the doors that flush nothing, as they
//! are for C's own stdio.
//!
//! # The reason
//!
//! Before this, stdout was Rust's `LineWriter` unconditionally, and a program
//! printing a million lines into a pipe made a million `write` syscalls. §4.2
//! calls the policy *"C's policy and every tool in a pipeline assumes it"*;
//! the cost of not having it was never lost output, only time, and the time was
//! most of the program's: measured on the commit that introduced this module,
//! a million `print("line")` into a pipe went from 1 000 000 `write` calls on
//! file descriptor 1 to 77, and from about 0.9 s to about 0.4 s.
//!
//! **Why a buffer of this crate's own, and not a larger Rust `BufWriter`
//! around `stdout()`.** `std::io::stdout()` is a `LineWriter` with no way to
//! replace or resize it, and anything placed in front of it has to be a static
//! somebody owns. Owning it here is also what lets the panic path reach it
//! with a `try_lock` rather than trusting a lock it cannot see.
//!
//! **Why the terminal case is not buffered by this module at all.** Rust's
//! `LineWriter` already implements that case exactly, and a second buffer in
//! front of it would have to reimplement the newline scan to behave the same.
//!
//! # The cost
//!
//! **A block that ends mid-line goes out as two `write`s, not one.** The
//! `LineWriter` behind the buffer passes the complete lines of a block
//! straight to the file descriptor and holds the partial line at its end,
//! which the flush that follows sends. Two syscalls per 64 KiB instead of one
//! is a rounding error beside one per line, and it keeps this module out of
//! the platform's file descriptor API.
//!
//! **Output interleaves differently across streams in a pipe**, which §4.2
//! says out loud: *"output ordering between the two streams is not
//! guaranteed"*. `print_error`, `write_error` and a panic still flush stdout
//! before writing to stderr, as a courtesy and not a contract, so a program's
//! last line before an error reaches the pipe before the error does.
//!
//! **A program that dies by a signal, or ends through `_exit`, loses up to
//! 64 KiB instead of up to one line.** That is what block buffering means in
//! every C program, and `flush()` is the documented way to say *now*.
//!
//! **The mode is decided once.** A program whose stdout is redirected with
//! `dup2` after its first `print` keeps the mode it started with. F0 gives a
//! program no way to do that except through a C library, and C's stdio makes
//! the same decision at the same moment.

use std::io::{IsTerminal, Write};
use std::sync::{Mutex, MutexGuard, PoisonError, TryLockError};

/// §4.2's block size for a stdout that is not a terminal.
pub(crate) const BLOCK: usize = 64 * 1024;

/// How stdout is buffered, decided at first use.
enum Policy {
    /// A terminal: straight through Rust's `LineWriter`.
    Line,
    /// Anything else: this buffer, never grown past [`BLOCK`].
    Block(Vec<u8>),
}

/// `None` until the first `print` or `write`. A `Mutex` because a `static`
/// must be `Sync`, not because F0 has threads (crate documentation, §10).
static STDOUT: Mutex<Option<Policy>> = Mutex::new(None);

unsafe extern "C" {
    // `int atexit(void (*function)(void))`, in every C runtime this crate
    // links against.
    fn atexit(function: extern "C" fn()) -> core::ffi::c_int;
}

/// Run at `exit(3)`, after `science_exit`'s own flush where there was one, so
/// that a process ending by any other `exit` still delivers the buffer.
extern "C" fn flush_at_exit() {
    flush_before_abort();
}

fn choose() -> Policy {
    // SAFETY: `flush_at_exit` is an `extern "C" fn()` with a `'static`
    // lifetime; `atexit` stores the pointer and calls it once at exit. A
    // failure to register (the table is full) leaves `science_exit`'s flush,
    // which every Science `main` ends through, as the only one.
    unsafe { atexit(flush_at_exit) };
    if std::io::stdout().is_terminal() {
        Policy::Line
    } else {
        Policy::Block(Vec::with_capacity(BLOCK))
    }
}

/// The lock, taken whole. Poisoning is ignored: the buffer is plain bytes and
/// is valid at every point a panic could have left it.
fn lock() -> MutexGuard<'static, Option<Policy>> {
    STDOUT.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Hand a block to the file descriptor and empty it.
fn drain(buffer: &mut Vec<u8>) {
    if buffer.is_empty() {
        return;
    }
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(buffer);
    let _ = out.flush();
    buffer.clear();
}

/// Write `parts` to stdout under one lock, as though they were one slice.
///
/// `print` is the text and a `\n`; taking them as parts lets it say so without
/// copying the text into a fresh allocation first.
///
/// **The buffer never grows past [`BLOCK`]**, so nothing here allocates after
/// the first use: a part that does not fit drains the buffer first, and a part
/// as large as the buffer goes straight through. That matters because the
/// runtime's own allocation failure panics, and the panic path flushes this.
pub(crate) fn write_parts(parts: &[&[u8]]) {
    let mut guard = lock();
    match guard.get_or_insert_with(choose) {
        Policy::Line => {
            let mut out = std::io::stdout().lock();
            for part in parts {
                let _ = out.write_all(part);
            }
        }
        Policy::Block(buffer) => {
            for part in parts {
                if buffer.len() + part.len() > BLOCK {
                    drain(buffer);
                }
                if part.len() >= BLOCK {
                    let mut out = std::io::stdout().lock();
                    let _ = out.write_all(part);
                    let _ = out.flush();
                } else {
                    buffer.extend_from_slice(part);
                }
            }
        }
    }
}

/// Empty this module's buffer and Rust's `LineWriter` behind it.
///
/// Before the first use there is nothing of this module's to empty, and the
/// policy is not decided just to flush.
pub(crate) fn flush() {
    let mut guard = lock();
    if let Some(Policy::Block(buffer)) = guard.as_mut() {
        drain(buffer);
    }
    drop(guard);
    let _ = std::io::stdout().lock().flush();
}

/// [`flush`] when stdout is a terminal, and nothing otherwise: what a read
/// of standard input does before it waits.
///
/// **C's line discipline, and only its terminal half.** A program that
/// writes `write("name? ")` and then reads a line means the prompt to be on
/// the screen while it waits, and on a terminal the prompt is held in Rust's
/// `LineWriter` for want of a `\n`. In a pipe nobody is waiting on a prompt,
/// and flushing the 64 KiB block before every line read would make a filter —
/// `read_line` then `print`, a million times — pay one `write` per line,
/// which is the cost this module exists to remove. Before the first `print`
/// or `write` there is nothing to flush and the policy is not decided for it.
pub(crate) fn flush_if_terminal() {
    let guard = lock();
    if let Some(Policy::Line) = guard.as_ref() {
        drop(guard);
        let _ = std::io::stdout().lock().flush();
    }
}

/// [`flush`] on a path that is about to end the process: a panic, an abort, an
/// `atexit` handler.
///
/// **`try_lock` rather than `lock`**, because a path that ends the process must
/// not be the thing that hangs it. F0 has no threads and nothing inside the
/// lock can panic back into this crate, so the lock is always free here; if a
/// later phase changes either, the cost is a buffer not delivered rather than
/// a process that never dies. C's streams are flushed too, for
/// `exit::flush_all`'s reason: a C library's last words matter as much as
/// Science's.
pub(crate) fn flush_before_abort() {
    match STDOUT.try_lock() {
        Ok(mut guard) => {
            if let Some(Policy::Block(buffer)) = guard.as_mut() {
                drain(buffer);
            }
        }
        Err(TryLockError::Poisoned(poisoned)) => {
            if let Some(Policy::Block(buffer)) = poisoned.into_inner().as_mut() {
                drain(buffer);
            }
        }
        Err(TryLockError::WouldBlock) => {}
    }
    let _ = std::io::stdout().lock().flush();
    crate::exit::flush_c_streams();
}
