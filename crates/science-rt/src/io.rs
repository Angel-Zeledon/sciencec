//! The free functions of §8: `print`, `println`, `read_file`, `write_file`.
//!
//! The two file functions return a `Result`, which is the one place in this
//! crate where the runtime builds a Science enum itself rather than leaving it to
//! codegen: both `T` and `E` are concrete here, so their layout is known. See
//! the crate documentation, §5, for the rule the two types below follow.

use std::io::Write;

use crate::abi::{LINK_RESULT_ERR, LINK_RESULT_OK};
use crate::string::ScienceString;

/// Science's `IoError`.
///
/// Every variant is payload-free, so by the general enum rule of the crate
/// documentation, §5.1, the type **is** its discriminant: one byte, alignment
/// one. Codegen compares it against the constants below.
///
/// The set is deliberately small. F0 has no error hierarchy, no `source`, no
/// message: §8 names `IoError` and gives it no methods, so anything richer
/// would be inventing library surface that the spec says does not exist.
/// [`ScienceIoError::OTHER`] is the catch-all, and is where a future phase would
/// grow new variants without disturbing the numbering of these.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScienceIoError(pub u8);

impl ScienceIoError {
    /// The path does not exist, or a directory along it does not.
    pub const NOT_FOUND: Self = Self(0);
    /// The process is not permitted to do this.
    pub const PERMISSION_DENIED: Self = Self(1);
    /// The path already exists and the operation required that it not.
    pub const ALREADY_EXISTS: Self = Self(2);
    /// The bytes are not what was expected — for [`science_read_file`], not valid
    /// UTF-8, which a `String` must be (§5.1).
    pub const INVALID_DATA: Self = Self(3);
    /// Anything else.
    pub const OTHER: Self = Self(4);

    fn from_io(error: &std::io::Error) -> Self {
        match error.kind() {
            std::io::ErrorKind::NotFound => Self::NOT_FOUND,
            std::io::ErrorKind::PermissionDenied => Self::PERMISSION_DENIED,
            std::io::ErrorKind::AlreadyExists => Self::ALREADY_EXISTS,
            std::io::ErrorKind::InvalidData => Self::INVALID_DATA,
            _ => Self::OTHER,
        }
    }
}

// ---------------------------------------------------------------------------
// STALE ABI — do not emit code against the three types below.
//
// `syntax-revision-2.md` §3 removed `Result` and replaced it with a pair:
// `read_file` returns `(String, IoError?)` and `write_file` returns
// `IoError?`. That is not a rename, it is a different layout. A `Result` is a
// tagged union — one discriminant, one live payload, 32 bytes here. A pair is
// a struct with *both* fields live at once, and the presence test reads the
// second one rather than a tag.
//
// The types are left standing rather than deleted because the replacement
// depends on a decision that has not been made: the representation of `T?`
// when `T` is not a pointer. `Option[&T]` had the null niche and `(&T)?`
// inherits it unchanged, but `IoError?` is a small value with no obvious
// niche, and whether it gets a discriminant byte or a reserved bit pattern is
// `type-checking-and-mir.md`'s call. Guessing here and having codegen agree
// with the guess would be worse than the gap, because the two would be
// consistent and wrong.
//
// What survives from below: the `ScienceIoError` codes and §5.1's general
// enum layout rule. What does not: these three shapes and both entry points'
// return types.
// ---------------------------------------------------------------------------

/// The payload union of `Result[String, IoError]`.
///
/// `ok` is live exactly when the tag is [`LINK_RESULT_OK`]; `err` exactly when
/// it is [`LINK_RESULT_ERR`]. Reading the other one is reading uninitialised
/// memory.
#[repr(C)]
#[derive(Clone, Copy)]
pub union ScienceIoResultStringPayload {
    /// The `Ok(String)` payload.
    pub ok: ScienceString,
    /// The `Err(IoError)` payload.
    pub err: ScienceIoError,
}

/// Science's `Result[String, IoError]`, the return type of [`science_read_file`].
///
/// Laid out by the general enum rule of the crate documentation, §5.1: a `u8`
/// discriminant at offset 0, then the payload union at the next multiple of its
/// alignment. The union is as wide as a `String`, so on a 64-bit target the tag
/// sits at 0, the payload at 8, and the whole value is 32 bytes aligned to 8.
///
/// No niche optimisation applies, and none ever will: `Result` has a payload in
/// **both** variants, so there is nothing for a niche to disambiguate.
#[repr(C)]
pub struct ScienceIoResultString {
    /// [`LINK_RESULT_OK`] or [`LINK_RESULT_ERR`].
    pub tag: u8,
    /// The payload of whichever variant `tag` names.
    pub payload: ScienceIoResultStringPayload,
}

/// Science's `Result[(), IoError]`, the return type of [`science_write_file`].
///
/// The `Ok` payload is `()`, which is zero-sized, so the payload union of §5.1
/// degenerates to the error byte alone and the whole enum is two bytes with
/// alignment one: the tag at offset 0, the error at offset 1. `err` is
/// meaningful only when `tag` is [`LINK_RESULT_ERR`].
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ScienceIoResultUnit {
    /// [`LINK_RESULT_OK`] or [`LINK_RESULT_ERR`].
    pub tag: u8,
    /// The `Err(IoError)` payload.
    pub err: ScienceIoError,
}

/// Science's `print(text: &String)`.
///
/// Writes the bytes to standard output verbatim, adding nothing. Standard
/// output is line buffered, so a `print` with no newline in it may sit in the
/// buffer until one arrives; [`science_panic`](crate::science_panic) flushes before
/// aborting so that it is not lost.
///
/// A write error is ignored. There is no `Result` in §8's signature to report
/// one through, and a program whose standard output has gone away has no better
/// answer available to it than carrying on.
///
/// # Safety
///
/// `text` must be a non-null, aligned pointer to a live [`ScienceString`].
#[no_mangle]
pub unsafe extern "C" fn science_print(text: *const ScienceString) {
    // SAFETY: the caller guarantees a live `ScienceString`.
    let bytes = unsafe { (*text).bytes() };
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(bytes);
}

/// Science's `println(text: &String)`.
///
/// As [`science_print`], followed by one `\n`. The newline is a line feed on every
/// platform, including Windows: Science text is UTF-8 and its line terminator is
/// `\n`, and translating it would make a program's output depend on where it
/// was compiled.
///
/// # Safety
///
/// `text` must be a non-null, aligned pointer to a live [`ScienceString`].
#[no_mangle]
pub unsafe extern "C" fn science_println(text: *const ScienceString) {
    // SAFETY: the caller guarantees a live `ScienceString`.
    let bytes = unsafe { (*text).bytes() };
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(bytes);
    let _ = out.write_all(b"\n");
}

/// Science's `read_file(path: &String) -> Result[String, IoError]`.
///
/// Reads the whole file and returns its contents as a `String`. Because a
/// `String` is UTF-8 by invariant (§5.1), a file that is not valid UTF-8 is
/// [`ScienceIoError::INVALID_DATA`] rather than a mis-encoded string. Reading raw
/// bytes is not in F0: §8 has no `Array[U8]` file entry point.
///
/// On `Ok` the returned `String` is the caller's to own and eventually free.
///
/// # Safety
///
/// `path` must be a non-null, aligned pointer to a live [`ScienceString`].
#[no_mangle]
pub unsafe extern "C" fn science_read_file(path: *const ScienceString) -> ScienceIoResultString {
    // SAFETY: the caller guarantees a live `ScienceString`, whose bytes are UTF-8.
    let path = unsafe { (*path).as_str() };

    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => return read_err(ScienceIoError::from_io(&error)),
    };

    if std::str::from_utf8(&bytes).is_err() {
        return read_err(ScienceIoError::INVALID_DATA);
    }

    // SAFETY: just validated as UTF-8, and the vector owns `len` readable
    // bytes.
    let text = unsafe { ScienceString::from_raw_utf8(bytes.as_ptr(), bytes.len()) };
    ScienceIoResultString {
        tag: LINK_RESULT_OK,
        payload: ScienceIoResultStringPayload { ok: text },
    }
}

/// Science's `write_file(path: &String, contents: &String) -> Result[(), IoError]`.
///
/// Creates the file if it does not exist and truncates it if it does. Parent
/// directories are not created: a missing one is [`ScienceIoError::NOT_FOUND`].
///
/// # Safety
///
/// Both pointers must be non-null, aligned and point to live [`ScienceString`]s.
#[no_mangle]
pub unsafe extern "C" fn science_write_file(
    path: *const ScienceString,
    contents: *const ScienceString,
) -> ScienceIoResultUnit {
    // SAFETY: the caller guarantees two live `ScienceString`s.
    let (path, contents) = unsafe { ((*path).as_str(), (*contents).bytes()) };
    match std::fs::write(path, contents) {
        Ok(()) => ScienceIoResultUnit {
            tag: LINK_RESULT_OK,
            err: ScienceIoError::OTHER,
        },
        Err(error) => ScienceIoResultUnit {
            tag: LINK_RESULT_ERR,
            err: ScienceIoError::from_io(&error),
        },
    }
}

fn read_err(error: ScienceIoError) -> ScienceIoResultString {
    ScienceIoResultString {
        tag: LINK_RESULT_ERR,
        payload: ScienceIoResultStringPayload { err: error },
    }
}
