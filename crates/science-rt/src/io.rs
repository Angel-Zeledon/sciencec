//! The free functions of §8: `print`, `println`, `read_file`, `write_file`.
//!
//! The two file functions are the one place in this crate where the runtime
//! assembles a Science aggregate itself rather than leaving it to codegen: every
//! type involved is concrete here, so its layout is known. `read_file` returns
//! the pair `(String, IoError?)` of `syntax-revision-2.md` §3 and `write_file`
//! returns `IoError?` alone. See the crate documentation, §5, for the rules the
//! three types below follow.

use std::io::Write;

use crate::abi::{SCIENCE_NULLABLE_NULL, SCIENCE_NULLABLE_PRESENT};
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

/// Science's `IoError?`, the return type of [`science_write_file`].
///
/// **`IoError` has no niche, so this is the tagged form: a discriminant byte
/// then the payload.** Two bytes, alignment one, the discriminant at offset 0
/// and the error at offset 1.
///
/// That follows Decision 6 of `type-checking-and-mir.md` §4.1 — *a pointer-like
/// `T` uses the null niche, a `T` with no niche gets a discriminant byte* —
/// read against the closed table of niche-carrying types in the crate
/// documentation, §5.2. That table is the three never-null pointers and nothing
/// else. `IoError` is a plain byte, so it is on the other side of the line, and
/// this type is laid out by the general rule of §5.1 exactly as any other
/// tagged enum is.
///
/// **It would have been tempting to niche this into one byte.** `IoError` uses
/// five of its 256 code points, so 251 patterns are idle and `255 = null` would
/// have made `IoError?` free. That is refused deliberately: Decision 6 grants a
/// niche to pointer-like types and to nothing else, and a reserved-code-point
/// niche is a representation no design note licenses. A runtime and a code
/// generator that invented it together would agree with each other and with
/// nothing else, which is the worst of the available failures — consistent and
/// wrong.
///
/// **The cost is one byte per fallible call, and it is paid where it is
/// cheapest.** `IoError?` is two bytes rather than one, which on both supported
/// conventions is still a register return; and inside
/// [`ScienceStringAndIoError`] the second byte is absorbed by the tail padding
/// the pair's eight-byte alignment forces anyway, so the niched form would have
/// produced a pair of exactly the same size.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScienceNullableIoError {
    /// [`SCIENCE_NULLABLE_NULL`] or [`SCIENCE_NULLABLE_PRESENT`], and never any
    /// other value.
    ///
    /// This byte is bit-for-bit the `Bool` that Science's `?` yields, so
    /// `err?` is a load of it and not a comparison.
    pub present: u8,
    /// The error, meaningful only when `present` is
    /// [`SCIENCE_NULLABLE_PRESENT`].
    ///
    /// The runtime writes a zero byte when the answer is null, so a null
    /// `IoError?` is two zero bytes end to end. That is a convenience for
    /// anyone reading a memory dump, not a second way to ask the question:
    /// the presence test is `present`, and only `present`.
    pub error: ScienceIoError,
}

impl ScienceNullableIoError {
    #[inline]
    pub(crate) const fn null() -> Self {
        Self {
            present: SCIENCE_NULLABLE_NULL,
            error: ScienceIoError(0),
        }
    }

    #[inline]
    pub(crate) const fn present(error: ScienceIoError) -> Self {
        Self {
            present: SCIENCE_NULLABLE_PRESENT,
            error,
        }
    }
}

/// Science's `(String, IoError?)`, the return type of [`science_read_file`].
///
/// A pair is an ordinary struct and **both fields are live at once**. There is
/// no tag choosing between them and no union: `value` is a `String` whatever
/// happened, and `error` answers separately whether anything went wrong. Field
/// order is declaration order, so on a 64-bit target `value` sits at offset 0
/// and occupies three words, `error` sits at offset 24, and the whole value is
/// 32 bytes aligned to 8: 24 bytes of `String`, two of `IoError?`, and six of
/// tail padding that the struct's eight-byte alignment forces.
///
/// **The caller owns `value` on every path, including the failing one, and must
/// free it on every path.** This is the one place where the pair model costs
/// something the removed `Result` did not: a failed `Result[String, IoError]`
/// had no `String` in it, so there was nothing to release, whereas a failed
/// pair still hands back a `String`. On the failing path the runtime returns
/// the empty string, which allocates nothing (§7) and whose
/// [`science_string_free`](crate::science_string_free) is a no-op — so the cost
/// is a call codegen must emit, not memory anyone must find.
#[repr(C)]
pub struct ScienceStringAndIoError {
    /// The first element of the pair: the file's contents, or the empty string
    /// when `error` is present. Owned by the caller either way.
    pub value: ScienceString,
    /// The second element of the pair: what went wrong, or null.
    pub error: ScienceNullableIoError,
}

/// Science's `write(text: borrowed String)` — the form that adds nothing.
///
/// Writes the bytes to standard output verbatim, adding nothing. Standard
/// output is line buffered, so a `print` with no newline in it may sit in the
/// buffer until one arrives; [`science_panic`](crate::science_panic) flushes before
/// aborting so that it is not lost.
///
/// A write error is ignored. §8's signature returns nothing to report one
/// through, and a program whose standard output has gone away has no better
/// answer available to it than carrying on.
///
/// # Safety
///
/// `text` must be a non-null, aligned pointer to a live [`ScienceString`].
#[no_mangle]
pub unsafe extern "C" fn science_write(text: *const ScienceString) {
    // SAFETY: the caller guarantees a live `ScienceString`.
    let bytes = unsafe { (*text).bytes() };
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(bytes);
}

/// Science's `println(text: &String)`.
///
/// As [`science_write`], followed by one `\n`. The newline is a line feed on every
/// platform, including Windows: Science text is UTF-8 and its line terminator is
/// `\n`, and translating it would make a program's output depend on where it
/// was compiled.
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
    let _ = out.write_all(b"\n");
}

/// Science's `read_file(path: borrowed String) -> (String, IoError?)`.
///
/// Reads the whole file and returns its contents as a `String`. Because a
/// `String` is UTF-8 by invariant (§5.1), a file that is not valid UTF-8 is
/// [`ScienceIoError::INVALID_DATA`] rather than a mis-encoded string. Reading raw
/// bytes is not in F0: §8 has no `Array[U8]` file entry point.
///
/// The returned `String` is the caller's to own and eventually free **whether
/// or not the read succeeded**; on the failing path it is the empty string. See
/// [`ScienceStringAndIoError`] for why the failing path still carries one.
///
/// The value is 32 bytes, so it comes back through an `sret` pointer on both
/// supported conventions (§2).
///
/// # Safety
///
/// `path` must be a non-null, aligned pointer to a live [`ScienceString`].
#[no_mangle]
pub unsafe extern "C" fn science_read_file(path: *const ScienceString) -> ScienceStringAndIoError {
    // SAFETY: the caller guarantees a live `ScienceString`, whose bytes are UTF-8.
    let path = unsafe { (*path).as_str() };

    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => return read_failed(ScienceIoError::from_io(&error)),
    };

    if std::str::from_utf8(&bytes).is_err() {
        return read_failed(ScienceIoError::INVALID_DATA);
    }

    // SAFETY: just validated as UTF-8, and the vector owns `len` readable
    // bytes.
    let text = unsafe { ScienceString::from_raw_utf8(bytes.as_ptr(), bytes.len()) };
    ScienceStringAndIoError {
        value: text,
        error: ScienceNullableIoError::null(),
    }
}

/// Science's `write_file(path: borrowed String, contents: borrowed String) -> IoError?`.
///
/// Creates the file if it does not exist and truncates it if it does. Parent
/// directories are not created: a missing one is [`ScienceIoError::NOT_FOUND`].
///
/// There is no value half to this signature — the old `Result[(), IoError]` had
/// a zero-sized `Ok` payload and the pair would have a zero-sized first element,
/// so the pair degenerates to its second element and the entry point returns
/// [`ScienceNullableIoError`] alone. At two bytes it comes back in a register on
/// both supported conventions (§2).
///
/// # Safety
///
/// Both pointers must be non-null, aligned and point to live [`ScienceString`]s.
#[no_mangle]
pub unsafe extern "C" fn science_write_file(
    path: *const ScienceString,
    contents: *const ScienceString,
) -> ScienceNullableIoError {
    // SAFETY: the caller guarantees two live `ScienceString`s.
    let (path, contents) = unsafe { ((*path).as_str(), (*contents).bytes()) };
    match std::fs::write(path, contents) {
        Ok(()) => ScienceNullableIoError::null(),
        Err(error) => ScienceNullableIoError::present(ScienceIoError::from_io(&error)),
    }
}

fn read_failed(error: ScienceIoError) -> ScienceStringAndIoError {
    ScienceStringAndIoError {
        value: ScienceString::empty(),
        error: ScienceNullableIoError::present(error),
    }
}
