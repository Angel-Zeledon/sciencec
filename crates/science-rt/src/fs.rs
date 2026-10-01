//! The entry points the bundled `fs` module is written over.
//!
//! # Not codegen support, and not in `RUNTIME`
//!
//! [`crate::file`]'s reason, word for word: these are called by **Science
//! source** — `fs.science` declares them in an `unsafe extern "C":` block —
//! and never by code the compiler emits, so `science-codegen`'s `RUNTIME`
//! table does not list them and `science-codegen-llvm`'s `tests/symbols.rs`
//! exempts them by name, beside `io`'s three.
//!
//! # The status, in one `i64`
//!
//! [`crate::file`]'s convention: a non-negative result is success, a count, a
//! length or a handle; a negative one is `-(code + 1)`. `code` is
//! [`ScienceIoError`]'s byte for the five it has, and one more that is this
//! module's alone, [`DIRECTORY_NOT_EMPTY`] — see there for why.
//!
//! # Paths are UTF-8, as `data-io.md` §7 says
//!
//! Every path arrives as `ffi.Span[U8]` and a length, the bytes of a Science
//! `String`, so it is UTF-8 by construction; a path that is not is refused
//! with `INVALID_DATA` rather than trusted, as [`crate::file`] does.
//!
//! # A list of strings, through C scalars
//!
//! An `extern` call returns a C scalar and nothing else, so a function whose
//! answer is `Array[String]` cannot return it. **Decision: the answer is kept
//! here, behind a handle, and the Science side copies it out one entry at a
//! time** — [`science_fs_list_open`] (or [`science_fs_current_directory`])
//! builds the whole answer and returns a handle to it, [`science_fs_list_count`]
//! says how many entries, [`science_fs_list_entry_length`] how many bytes
//! entry `i` has, [`science_fs_list_entry_copy`] copies those bytes into a
//! buffer the caller owns, and [`science_fs_list_close`] frees it.
//!
//! **Reason.** The alternatives are worse each in its own way: a runtime that
//! allocated Science `String`s and `Array`s would have to know their layout,
//! which is exactly what [`crate::file`]'s *"nothing here knows a Science
//! layout"* keeps out of this half; and a call per entry against a live
//! `read_dir` could not sort. The handle holds the finished, sorted answer, so
//! the Science loop over it is a loop over a fixed list.
//!
//! **Cost.** Two calls and a copy per name, and a handle the Science side must
//! close — which `fs.science` does in a `Drop`, so no path through it leaks
//! one.
//!
//! The handle is a pointer to a boxed `Vec<String>`, as an `i64`. A user-space
//! address is positive on every target F0 supports, so it never collides with
//! a status.

use crate::io::ScienceIoError;

/// `remove_directory` on a directory that still has entries.
///
/// **Not one of [`ScienceIoError`]'s five, on purpose.** `IoError` is a
/// prelude type whose variant count `science-codegen`'s layout tests pin, and
/// its catch-all would read this as *"input/output error"* — a sentence that
/// sends the reader to the disk when the answer is in the directory. `data-io.md`
/// §7 makes the refusal deliberate (*"`remove_directory` refuses a non-empty
/// directory"*), so the refusal gets a sentence of its own: code 5, after
/// `IoError`'s, so none of those renumbers. `io.science`'s `FileError` reads
/// it.
pub const DIRECTORY_NOT_EMPTY: u8 = 5;

fn status(code: u8) -> i64 {
    -(i64::from(code) + 1)
}

fn status_of(error: &std::io::Error) -> i64 {
    if error.kind() == std::io::ErrorKind::DirectoryNotEmpty {
        return status(DIRECTORY_NOT_EMPTY);
    }
    status(ScienceIoError::from_io(error).0)
}

fn done(result: std::io::Result<()>) -> i64 {
    match result {
        Ok(()) => 0,
        Err(error) => status_of(&error),
    }
}

/// The UTF-8 path `path[..len]`, or `None` for bytes that are not UTF-8.
///
/// # Safety
///
/// `path` must point to `len` readable bytes.
unsafe fn path_of<'a>(path: *const u8, len: i64) -> Option<&'a str> {
    // SAFETY: the caller guarantees `len` readable bytes.
    let bytes = unsafe { std::slice::from_raw_parts(path, len.max(0) as usize) };
    std::str::from_utf8(bytes).ok()
}

/// `create_directory`: creates `path[..len]` and every missing parent —
/// `data-io.md` §7, *"`create_directory` creates parents"*. A directory that
/// already exists is success, which is `create_dir_all`'s answer and the one a
/// "make sure it is there" call wants. Returns `0` or a negative status.
///
/// # Safety
///
/// `path` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn science_fs_create_directory(path: *const u8, len: i64) -> i64 {
    // SAFETY: forwarded from the caller.
    let Some(path) = (unsafe { path_of(path, len) }) else {
        return status(ScienceIoError::INVALID_DATA.0);
    };
    done(std::fs::create_dir_all(path))
}

/// `remove_file`: removes the file `path[..len]`. Returns `0` or a negative
/// status.
///
/// # Safety
///
/// `path` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn science_fs_remove_file(path: *const u8, len: i64) -> i64 {
    // SAFETY: forwarded from the caller.
    let Some(path) = (unsafe { path_of(path, len) }) else {
        return status(ScienceIoError::INVALID_DATA.0);
    };
    done(std::fs::remove_file(path))
}

/// `remove_directory`: removes the **empty** directory `path[..len]`, and
/// refuses one with entries ([`DIRECTORY_NOT_EMPTY`]). There is no recursive
/// form — `data-io.md` §7: *"a bug in a path expression should not be able to
/// erase a results directory"*. Returns `0` or a negative status.
///
/// # Safety
///
/// `path` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn science_fs_remove_directory(path: *const u8, len: i64) -> i64 {
    // SAFETY: forwarded from the caller.
    let Some(path) = (unsafe { path_of(path, len) }) else {
        return status(ScienceIoError::INVALID_DATA.0);
    };
    done(std::fs::remove_dir(path))
}

/// `rename`: moves `from[..from_len]` to `to[..to_len]`. Returns `0` or a
/// negative status.
///
/// What happens when `to` exists is the operating system's answer, and
/// `stdlib-core.md` §5.1 names that as the reason this is Level 2: POSIX
/// replaces a file silently, Windows refuses. This passes the difference
/// through rather than picking a side by a second system call, which would
/// only move the race.
///
/// # Safety
///
/// `from` must point to `from_len` readable bytes and `to` to `to_len`.
#[no_mangle]
pub unsafe extern "C" fn science_fs_rename(from: *const u8, from_len: i64, to: *const u8, to_len: i64) -> i64 {
    // SAFETY: forwarded from the caller, once per path.
    let (Some(from), Some(to)) = (unsafe { path_of(from, from_len) }, unsafe { path_of(to, to_len) }) else {
        return status(ScienceIoError::INVALID_DATA.0);
    };
    done(std::fs::rename(from, to))
}

/// What [`science_fs_kind`] answers for a file.
pub const KIND_FILE: i64 = 1;
/// What [`science_fs_kind`] answers for a directory.
pub const KIND_DIRECTORY: i64 = 2;
/// What [`science_fs_kind`] answers for anything else that is there — a
/// device, a socket, a FIFO.
pub const KIND_OTHER: i64 = 3;

/// `Path.exists` and `Path.is_directory`, the bundled `path` module's: what
/// is at `path[..len]` — [`KIND_FILE`], [`KIND_DIRECTORY`] or [`KIND_OTHER`] —
/// or a negative status when nothing can be found there.
///
/// **Symbolic links are followed**, so a link to a directory is a directory
/// and a dangling link is not found. That is what `exists` means in Rust's,
/// Python's and POSIX `test -e`'s hands, and the one a program asking "can I
/// open this" wants. **One call, not two**: `exists` and `is_directory` read
/// the same `stat`, and the Science side asks the question it has.
///
/// # Safety
///
/// `path` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn science_fs_kind(path: *const u8, len: i64) -> i64 {
    // SAFETY: forwarded from the caller.
    let Some(path) = (unsafe { path_of(path, len) }) else {
        return status(ScienceIoError::INVALID_DATA.0);
    };
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => KIND_DIRECTORY,
        Ok(metadata) if metadata.is_file() => KIND_FILE,
        Ok(_) => KIND_OTHER,
        Err(error) => status_of(&error),
    }
}

/// `Path.size`: the length in bytes of what is at `path[..len]`, following
/// symbolic links, or a negative status. A directory's size is whatever the
/// operating system reports for it, which is not a sum of anything — `ls -l`'s
/// number, passed through rather than refused, since refusing it would be a
/// second `stat` and the same race [`science_fs_rename`] declines to move.
///
/// A length past `i64::MAX` cannot exist on any filesystem F0 runs on; it is
/// clamped rather than allowed to read as a status.
///
/// # Safety
///
/// `path` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn science_fs_size(path: *const u8, len: i64) -> i64 {
    // SAFETY: forwarded from the caller.
    let Some(path) = (unsafe { path_of(path, len) }) else {
        return status(ScienceIoError::INVALID_DATA.0);
    };
    match std::fs::metadata(path) {
        Ok(metadata) => i64::try_from(metadata.len()).unwrap_or(i64::MAX),
        Err(error) => status_of(&error),
    }
}

fn into_handle(entries: Vec<String>) -> i64 {
    Box::into_raw(Box::new(entries)) as i64
}

/// # Safety
///
/// `handle` must be one [`into_handle`] returned that nothing has closed.
unsafe fn entries<'a>(handle: i64) -> &'a [String] {
    // SAFETY: the caller guarantees a live handle.
    unsafe { &*(handle as *const Vec<String>) }
}

/// `list_directory`: the names in `path[..len]`, as a handle, or a negative
/// status.
///
/// **Names, not paths.** `a.txt`, never `dir/a.txt`: a name is what `ls`
/// prints, and the bundled `path` module's `join` makes the path. `.` and `..` are not
/// entries (`read_dir` never yields them).
///
/// **Sorted in byte order.** `reproducibility.md` G7: *"`list_directory`
/// should sort in byte order"*, for `data-io.md` §7's own reason about
/// `glob` — a program whose input order is the host filesystem's produces
/// different output on a laptop and on a cluster. Byte order of UTF-8 is
/// code-point order, and it is what Rust's `str` `Ord` is.
///
/// **A name that is not UTF-8 fails the whole listing with `INVALID_DATA`.**
/// `data-io.md` §7: *"a non-Unicode path is a `NotUnicode` error"*. Skipping
/// it would hand back a listing that silently lacks a file, and a lossy
/// conversion a name that opens nothing; both are a wrong answer that looks
/// like a right one. The cost is §7's own — such a directory is unlistable
/// from Science and needs `extern`.
///
/// # Safety
///
/// `path` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn science_fs_list_open(path: *const u8, len: i64) -> i64 {
    // SAFETY: forwarded from the caller.
    let Some(path) = (unsafe { path_of(path, len) }) else {
        return status(ScienceIoError::INVALID_DATA.0);
    };
    let reader = match std::fs::read_dir(path) {
        Ok(reader) => reader,
        Err(error) => return status_of(&error),
    };
    let mut names = Vec::new();
    for entry in reader {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => return status_of(&error),
        };
        match entry.file_name().into_string() {
            Ok(name) => names.push(name),
            Err(_) => return status(ScienceIoError::INVALID_DATA.0),
        }
    }
    names.sort();
    into_handle(names)
}

/// `current_directory`: the working directory as a one-entry list, or a
/// negative status. A handle rather than a length-then-copy pair of calls,
/// because the directory can change between two calls and the handle holds
/// the answer once; and the listing's handle rather than a second kind, so
/// the Science side has one protocol to read.
#[no_mangle]
pub extern "C" fn science_fs_current_directory() -> i64 {
    let directory = match std::env::current_dir() {
        Ok(directory) => directory,
        Err(error) => return status_of(&error),
    };
    match directory.into_os_string().into_string() {
        Ok(text) => into_handle(vec![text]),
        Err(_) => status(ScienceIoError::INVALID_DATA.0),
    }
}

/// How many entries the list behind `handle` has.
///
/// # Safety
///
/// `handle` must be a live handle from this module.
#[no_mangle]
pub unsafe extern "C" fn science_fs_list_count(handle: i64) -> i64 {
    // SAFETY: the caller guarantees a live handle.
    unsafe { entries(handle) }.len() as i64
}

/// The length in bytes of entry `index`, or `-1` past the end.
///
/// # Safety
///
/// `handle` must be a live handle from this module.
#[no_mangle]
pub unsafe extern "C" fn science_fs_list_entry_length(handle: i64, index: i64) -> i64 {
    // SAFETY: the caller guarantees a live handle.
    let list = unsafe { entries(handle) };
    usize::try_from(index).ok().and_then(|i| list.get(i)).map_or(-1, |name| name.len() as i64)
}

/// Copies at most `capacity` bytes of entry `index` into `buffer`, and returns
/// how many it copied, or `-1` past the end. Copying fewer than the entry has
/// is the caller's mistake and is reported by the count, not by a panic
/// across a C boundary.
///
/// # Safety
///
/// `handle` must be a live handle from this module, and `buffer` must point to
/// `capacity` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn science_fs_list_entry_copy(handle: i64, index: i64, buffer: *mut u8, capacity: i64) -> i64 {
    // SAFETY: the caller guarantees a live handle.
    let list = unsafe { entries(handle) };
    let Some(name) = usize::try_from(index).ok().and_then(|i| list.get(i)) else {
        return -1;
    };
    let count = name.len().min(capacity.max(0) as usize);
    // SAFETY: `count` is within the entry and, by the caller's guarantee,
    // within the buffer; a Science `Array` and a Rust `String` do not overlap.
    unsafe { std::ptr::copy_nonoverlapping(name.as_ptr(), buffer, count) };
    count as i64
}

/// Frees the list behind `handle`. Returns `0`.
///
/// # Safety
///
/// `handle` must be a live handle from this module; it is invalid afterwards.
#[no_mangle]
pub unsafe extern "C" fn science_fs_list_close(handle: i64) -> i64 {
    // SAFETY: the caller guarantees a live handle, and gives it up here.
    drop(unsafe { Box::from_raw(handle as *mut Vec<String>) });
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("science-rt-fs-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        dir
    }

    fn listed(dir: &std::path::Path) -> Result<Vec<String>, i64> {
        let path = dir.to_str().expect("a UTF-8 scratch path");
        // SAFETY: `path` is live for the call.
        let handle = unsafe { science_fs_list_open(path.as_ptr(), path.len() as i64) };
        if handle < 0 {
            return Err(handle);
        }
        let mut names = Vec::new();
        // SAFETY: `handle` is live until the close below, and each buffer
        // holds the length the copy is told.
        unsafe {
            for index in 0..science_fs_list_count(handle) {
                let length = science_fs_list_entry_length(handle, index);
                let mut buffer = vec![0u8; length as usize];
                let copied = science_fs_list_entry_copy(handle, index, buffer.as_mut_ptr(), length);
                assert_eq!(copied, length);
                names.push(String::from_utf8(buffer).expect("UTF-8"));
            }
            assert_eq!(science_fs_list_entry_length(handle, science_fs_list_count(handle)), -1);
            science_fs_list_close(handle);
        }
        Ok(names)
    }

    fn kind_and_size(path: &std::path::Path) -> (i64, i64) {
        let path = path.to_str().expect("a UTF-8 scratch path");
        // SAFETY: `path` is live for both calls.
        unsafe {
            (
                science_fs_kind(path.as_ptr(), path.len() as i64),
                science_fs_size(path.as_ptr(), path.len() as i64),
            )
        }
    }

    #[test]
    fn a_file_a_directory_and_nothing_are_told_apart() {
        let dir = scratch("kind");
        std::fs::write(dir.join("five"), "12345").unwrap();
        assert_eq!(kind_and_size(&dir.join("five")), (KIND_FILE, 5));
        assert_eq!(kind_and_size(&dir).0, KIND_DIRECTORY);
        let missing = status(ScienceIoError::NOT_FOUND.0);
        assert_eq!(kind_and_size(&dir.join("missing")), (missing, missing));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_listing_is_sorted_in_byte_order() {
        let dir = scratch("sorted");
        for name in ["b", "é", "Z", "a", "_"] {
            std::fs::write(dir.join(name), name).unwrap();
        }
        let want: Vec<String> = ["Z", "_", "a", "b", "é"].map(String::from).to_vec();
        assert_eq!(listed(&dir), Ok(want));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_directory_is_not_found_and_a_full_one_is_not_removed() {
        let dir = scratch("errors");
        assert_eq!(listed(&dir.join("missing")), Err(status(ScienceIoError::NOT_FOUND.0)));
        std::fs::write(dir.join("kept"), "").unwrap();
        let path = dir.to_str().unwrap();
        // SAFETY: `path` is live for the call.
        let removed = unsafe { science_fs_remove_directory(path.as_ptr(), path.len() as i64) };
        assert_eq!(removed, status(DIRECTORY_NOT_EMPTY));
        assert!(dir.join("kept").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A name that is not UTF-8 refuses the whole listing — where the
    /// filesystem lets such a name exist at all. APFS refuses to create one,
    /// so on macOS this has nothing to list and says so.
    #[cfg(unix)]
    #[test]
    fn a_name_that_is_not_utf8_fails_the_listing() {
        use std::os::unix::ffi::OsStrExt;
        let dir = scratch("not_utf8");
        std::fs::write(dir.join("fine"), "").unwrap();
        let bad = dir.join(std::ffi::OsStr::from_bytes(b"bad\xff"));
        if std::fs::write(&bad, "").is_err() {
            eprintln!("skipped: this filesystem refuses a name that is not UTF-8");
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
        assert_eq!(listed(&dir), Err(status(ScienceIoError::INVALID_DATA.0)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
