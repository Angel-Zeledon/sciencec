//! The five entry points `os`'s `args` and `env` are written over.
//!
//! # Not codegen support, and not in `RUNTIME`
//!
//! `file.rs`'s situation exactly: these are called by **Science source** — the
//! bundled `os` module declares them in an `unsafe extern "C":` block and
//! wraps them — and never by code the compiler emits, so `science-codegen`'s
//! `RUNTIME` table does not list them and `science-codegen-llvm`'s
//! `tests/symbols.rs` exempts them by name.
//!
//! # The protocol: a length, then a copy into the caller's buffer
//!
//! An `extern` call returns a C scalar and nothing else, and what `args` and
//! `env` produce are strings. **Decision: every string leaves the runtime in
//! two calls — one that answers its length in bytes, and one that copies it
//! into an `ffi.MutableSpan[U8]` the Science side allocated at that length.**
//! The Science side then builds its `String` with `String.from_bytes`, which
//! checks the bytes rather than trusting them.
//!
//! **Reason.** It is the smallest protocol that stays inside C scalars and a
//! span, and it keeps ownership on one side: the runtime never allocates
//! anything Science frees, and Science never frees anything the runtime
//! allocated. The alternatives each break one of those — returning a pointer
//! to a runtime-owned buffer needs a second entry point to free it and a rule
//! for when, and writing a `ScienceString` through an out-pointer would make
//! this file know a Science layout, which `file.rs` is careful not to.
//!
//! **Cost.** Every string is two calls and, for an environment variable, two
//! lookups. Between them the variable can change — only through `setenv` on
//! another thread, which `stdlib-standard.md` §4.2 makes `unsafe` — so the copy
//! returns the length it found *then*, and the Science side starts over when
//! the two disagree. For arguments nothing can change: they are captured once.
//!
//! A negative length means *absent*: an index past the last argument, or a
//! variable that is not set.
//!
//! # Not UTF-8: replaced, not refused
//!
//! **Decision: an argument or a variable's value that is not valid UTF-8
//! reaches Science with each invalid sequence replaced by U+FFFD**, which is
//! `String::from_utf8_lossy`'s rule — the WHATWG one.
//!
//! **Reason.** `stdlib-standard.md` §4.6 gives `args() -> Array of String`,
//! with no error channel, and an `env` whose only absent case is *unset*.
//! Refusing would have to be a panic, and a panic in `args()` is a program that
//! cannot start — not even to print its own usage — because one file name on
//! its command line was written by a different locale. Rust's `std::env::args`
//! takes that choice and is the counterexample.
//!
//! **Cost.** A non-UTF-8 file name passed on the command line does not
//! round-trip: the `String` names a file that is not the one the user meant.
//! On Unix the operating system's strings are bytes, and a byte-preserving
//! `OsString` is the type that would fix it; Science has none yet.
//!
//! # Captured how
//!
//! Through `std::env::args_os` and `std::env::var_os`, which work inside a
//! static library linked into a C `main` — on macOS `std` asks
//! `_NSGetArgv`, and on Linux glibc hands `argc`/`argv` to `std`'s
//! `.init_array` entry — so the C `main` the compiler emits needs no change and
//! Science's `main()` keeps taking no parameters. The arguments are read once,
//! on first use, and kept for the life of the process.

use std::sync::OnceLock;

/// The command line, argument zero included, each decoded lossily.
fn arguments() -> &'static [String] {
    static ARGUMENTS: OnceLock<Vec<String>> = OnceLock::new();
    ARGUMENTS.get_or_init(|| std::env::args_os().map(|argument| argument.to_string_lossy().into_owned()).collect())
}

fn argument(index: i64) -> Option<&'static str> {
    usize::try_from(index).ok().and_then(|index| arguments().get(index)).map(String::as_str)
}

/// Copy as much of `text` as fits in `buffer[..capacity]`, and return
/// `text`'s whole length.
///
/// # Safety
///
/// `buffer` must point to `capacity` writable bytes.
unsafe fn copy_out(text: &str, buffer: *mut u8, capacity: i64) -> i64 {
    let count = text.len().min(capacity.max(0) as usize);
    // SAFETY: the caller guarantees `capacity` writable bytes and `count` is no
    // more than that; `text` is ours and cannot overlap the caller's buffer.
    unsafe { std::ptr::copy_nonoverlapping(text.as_ptr(), buffer, count) };
    text.len() as i64
}

/// The value of the variable named `name[..len]`, decoded lossily, or `None`
/// when it is unset.
///
/// **A name that cannot name a variable is unset**: one that is empty, is not
/// UTF-8, or holds `=` or NUL. POSIX's `getenv` can find none of those, and
/// `std::env::var_os` would otherwise be free to panic on them.
///
/// The lookup is the platform's: case-sensitive on Unix and case-insensitive
/// on Windows. `stdlib-standard.md` §4.2 decides that `os` does not paper over
/// the difference.
///
/// # Safety
///
/// `name` must point to `len` readable bytes.
unsafe fn variable(name: *const u8, len: i64) -> Option<String> {
    // SAFETY: the caller guarantees `len` readable bytes.
    let name = unsafe { std::slice::from_raw_parts(name, len.max(0) as usize) };
    let name = std::str::from_utf8(name).ok()?;
    if name.is_empty() || name.contains(['=', '\0']) {
        return None;
    }
    std::env::var_os(name).map(|value| value.to_string_lossy().into_owned())
}

/// How many arguments the process was started with, argument zero included.
#[no_mangle]
pub extern "C" fn science_os_argument_count() -> i64 {
    arguments().len() as i64
}

/// The length in bytes of argument `index`, or `-1` past the last one.
#[no_mangle]
pub extern "C" fn science_os_argument_length(index: i64) -> i64 {
    argument(index).map_or(-1, |text| text.len() as i64)
}

/// Copy argument `index` into `buffer[..capacity]`, as much of it as fits.
/// Returns its whole length, or `-1` past the last argument.
///
/// # Safety
///
/// `buffer` must point to `capacity` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn science_os_argument_copy(index: i64, buffer: *mut u8, capacity: i64) -> i64 {
    match argument(index) {
        // SAFETY: forwarded from the caller.
        Some(text) => unsafe { copy_out(text, buffer, capacity) },
        None => -1,
    }
}

/// The length in bytes of the variable named `name[..len]`, or `-1` when it is
/// unset.
///
/// # Safety
///
/// `name` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn science_os_variable_length(name: *const u8, len: i64) -> i64 {
    // SAFETY: forwarded from the caller.
    unsafe { variable(name, len) }.map_or(-1, |value| value.len() as i64)
}

/// Copy the variable named `name[..len]` into `buffer[..capacity]`, as much of
/// it as fits. Returns its whole length *now* — which the caller compares with
/// the `capacity` it asked for — or `-1` when it is unset.
///
/// # Safety
///
/// `name` must point to `len` readable bytes and `buffer` to `capacity`
/// writable ones.
#[no_mangle]
pub unsafe extern "C" fn science_os_variable_copy(
    name: *const u8,
    len: i64,
    buffer: *mut u8,
    capacity: i64,
) -> i64 {
    // SAFETY: forwarded from the caller.
    match unsafe { variable(name, len) } {
        // SAFETY: forwarded from the caller.
        Some(value) => unsafe { copy_out(&value, buffer, capacity) },
        None => -1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(name: &str) -> Option<String> {
        // SAFETY: `name` is live for both calls, and `buffer` holds `length` bytes.
        unsafe {
            let length = science_os_variable_length(name.as_ptr(), name.len() as i64);
            if length < 0 {
                return None;
            }
            let mut buffer = vec![0u8; length as usize];
            let whole = science_os_variable_copy(name.as_ptr(), name.len() as i64, buffer.as_mut_ptr(), length);
            assert_eq!(whole, length);
            Some(String::from_utf8(buffer).unwrap())
        }
    }

    #[test]
    fn a_name_no_variable_can_have_is_unset_rather_than_a_panic() {
        for name in ["", "A=B", "A\0B"] {
            assert_eq!(read(name), None, "{name:?}");
        }
    }

    #[test]
    fn the_test_binary_s_own_arguments_round_trip() {
        let count = science_os_argument_count();
        assert!(count >= 1, "argument zero is always there");
        assert_eq!(science_os_argument_length(count), -1);
        assert_eq!(science_os_argument_length(-1), -1);
        let zero = std::env::args().next().unwrap();
        let mut buffer = vec![0u8; zero.len()];
        // SAFETY: `buffer` holds `zero.len()` bytes.
        let whole = unsafe { science_os_argument_copy(0, buffer.as_mut_ptr(), buffer.len() as i64) };
        assert_eq!(whole, zero.len() as i64);
        assert_eq!(buffer, zero.as_bytes());
        // A short buffer gets a prefix and the whole length back.
        let mut short = [0u8; 1];
        // SAFETY: `short` holds one byte.
        assert_eq!(unsafe { science_os_argument_copy(0, short.as_mut_ptr(), 1) }, zero.len() as i64);
        assert_eq!(short[0], zero.as_bytes()[0]);
    }

    #[test]
    fn path_is_set_in_any_environment_the_tests_run_in() {
        assert_eq!(read("PATH"), std::env::var("PATH").ok());
    }
}
