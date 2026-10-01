//! The five entry points `io`'s `File` is written over, and the one its
//! `Stdin` is.
//!
//! # Not codegen support, and not in `RUNTIME`
//!
//! Every other `#[no_mangle]` function in this crate is called by code the
//! compiler emits, and `science-codegen`'s `RUNTIME` table is the whole list of
//! those (Decision 14). These six are called by **Science source** — the
//! bundled `io` module declares them in an `unsafe extern "C":` block and wraps
//! them, which is `stdlib-core.md` §9's *"Science + `science-rt`"* taken
//! literally: the library is Science, and the runtime is what it stands on.
//! They cross the boundary as C scalars and an `ffi.Span[U8]` pointer, so
//! nothing here knows a Science layout.
//!
//! # The handle and the status, in one `i64`
//!
//! A non-negative result is a handle (from [`science_file_create`] and
//! [`science_file_open`]), a count of bytes (from [`science_file_read`] and
//! [`science_stdin_read`]) or success (from [`science_file_write`] and
//! [`science_file_close`]); a negative one is `-(code + 1)`, where `code` is
//! [`ScienceIoError`]'s byte. One integer rather than an out-parameter because
//! `io` would otherwise need a place for the runtime to write through, and an
//! `i64` already has room for both answers.
//!
//! The handle is the operating system's own — a file descriptor on Unix and a
//! `HANDLE` on Windows — owned by the Science `File` from `create` or `open`
//! until `close`.
//!
//! # Standard input is not a handle
//!
//! [`science_stdin_read`] takes no handle, and `io`'s `Stdin` holds none. The
//! bytes come through Rust's `std::io::stdin()`, whose one process-wide buffer
//! is also what `read_line` ([`science_read_line`](crate::science_read_line))
//! reads from — so a program that reads a line with the free function and the
//! rest through `Stdin` loses nothing between them. A descriptor `0` wrapped in
//! a `File` would read past that buffer, and its `drop` would close standard
//! input.

use crate::io::ScienceIoError;

fn status_of(error: &std::io::Error) -> i64 {
    -(i64::from(ScienceIoError::from_io(error).0) + 1)
}

/// Open the file at the UTF-8 path `path[..len]` for writing, creating it or
/// truncating it. Returns the handle, or a negative status.
///
/// # Safety
///
/// `path` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn science_file_create(path: *const u8, len: i64) -> i64 {
    // SAFETY: the caller guarantees `len` readable bytes.
    let bytes = unsafe { std::slice::from_raw_parts(path, len.max(0) as usize) };
    let Ok(path) = std::str::from_utf8(bytes) else {
        return -(i64::from(ScienceIoError::INVALID_DATA.0) + 1);
    };
    match std::fs::File::create(path) {
        Ok(file) => into_handle(file),
        Err(error) => status_of(&error),
    }
}

/// Open the file at the UTF-8 path `path[..len]` for reading. Returns the
/// handle, or a negative status — `data-io.md` §7's `File.open`.
///
/// # Safety
///
/// `path` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn science_file_open(path: *const u8, len: i64) -> i64 {
    // SAFETY: the caller guarantees `len` readable bytes.
    let bytes = unsafe { std::slice::from_raw_parts(path, len.max(0) as usize) };
    let Ok(path) = std::str::from_utf8(bytes) else {
        return -(i64::from(ScienceIoError::INVALID_DATA.0) + 1);
    };
    match std::fs::File::open(path) {
        Ok(file) => into_handle(file),
        Err(error) => status_of(&error),
    }
}

/// Read at most `len` bytes from `handle` into `into[..len]`. Returns the
/// count — `0` at end of input and only there, `stdlib-core.md` §4.2's one
/// end-of-input signal — or a negative status.
///
/// **One system call, not a loop to fill**: §4.3 makes `File` unbuffered, and
/// a short read is how a pipe or a terminal says *"that is all I have now"*.
/// The one thing retried is an interrupted call, which delivered nothing and
/// would otherwise reach the program as an error it did not cause.
///
/// # Safety
///
/// `handle` must be a handle [`science_file_open`] or [`science_file_create`]
/// returned and nothing has closed; `into` must point to `len` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn science_file_read(handle: i64, into: *mut u8, len: i64) -> i64 {
    // SAFETY: the caller guarantees `len` writable bytes.
    let into = unsafe { std::slice::from_raw_parts_mut(into, len.max(0) as usize) };
    // SAFETY: the caller guarantees a live handle; `ManuallyDrop` keeps this
    // borrow of it from closing it.
    let mut file = std::mem::ManuallyDrop::new(unsafe { from_handle(handle) });
    read_counted(&mut *file, into)
}

/// Read at most `len` bytes of standard input into `into[..len]`, through the
/// buffer `read_line` shares. Returns the count — `0` at end of input — or a
/// negative status.
///
/// Standard output is flushed first when it is a terminal, which is
/// [`science_read_line`](crate::science_read_line)'s decision and its reason:
/// a prompt written with `write` is on the screen before the program waits.
///
/// # Safety
///
/// `into` must point to `len` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn science_stdin_read(into: *mut u8, len: i64) -> i64 {
    // SAFETY: the caller guarantees `len` writable bytes.
    let into = unsafe { std::slice::from_raw_parts_mut(into, len.max(0) as usize) };
    crate::stdout::flush_if_terminal();
    read_counted(&mut std::io::stdin().lock(), into)
}

/// One read into `into`, retried only while it is interrupted, as a count or a
/// negative status.
fn read_counted(source: &mut impl std::io::Read, into: &mut [u8]) -> i64 {
    loop {
        match source.read(into) {
            Ok(count) => return count as i64,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return status_of(&error),
        }
    }
}

/// Write all of `bytes[..len]` to `handle`, or fail. Returns `0` or a negative
/// status. `stdlib-core.md` §4.2: *"There is no partial write"* — the loop over
/// short writes is here, once, rather than in every caller.
///
/// # Safety
///
/// `handle` must be a handle [`science_file_create`] returned and nothing has
/// closed; `bytes` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn science_file_write(handle: i64, bytes: *const u8, len: i64) -> i64 {
    use std::io::Write;
    // SAFETY: the caller guarantees `len` readable bytes.
    let bytes = unsafe { std::slice::from_raw_parts(bytes, len.max(0) as usize) };
    // SAFETY: the caller guarantees a live handle; `ManuallyDrop` keeps this
    // borrow of it from closing it.
    let mut file = std::mem::ManuallyDrop::new(unsafe { from_handle(handle) });
    match file.write_all(bytes) {
        Ok(()) => 0,
        Err(error) => status_of(&error),
    }
}

/// Close `handle`, flushing nothing because nothing here buffers. Returns `0`
/// or a negative status.
///
/// # Safety
///
/// `handle` must be a handle [`science_file_create`] returned and nothing has
/// closed; it is invalid afterwards.
#[no_mangle]
pub unsafe extern "C" fn science_file_close(handle: i64) -> i64 {
    // SAFETY: the caller guarantees a live handle, and gives it up here.
    let file = unsafe { from_handle(handle) };
    // `sync_all` is not a flush and is not asked for: `File` is unbuffered
    // (§4.3), so every byte has already left the process. Dropping closes.
    drop(file);
    0
}

#[cfg(unix)]
fn into_handle(file: std::fs::File) -> i64 {
    use std::os::fd::IntoRawFd;
    i64::from(file.into_raw_fd())
}

#[cfg(unix)]
unsafe fn from_handle(handle: i64) -> std::fs::File {
    use std::os::fd::FromRawFd;
    // SAFETY: the caller guarantees the descriptor is live and owned.
    unsafe { std::fs::File::from_raw_fd(handle as i32) }
}

#[cfg(windows)]
fn into_handle(file: std::fs::File) -> i64 {
    use std::os::windows::io::IntoRawHandle;
    file.into_raw_handle() as i64
}

#[cfg(windows)]
unsafe fn from_handle(handle: i64) -> std::fs::File {
    use std::os::windows::io::FromRawHandle;
    // SAFETY: the caller guarantees the handle is live and owned.
    unsafe { std::fs::File::from_raw_handle(handle as _) }
}
