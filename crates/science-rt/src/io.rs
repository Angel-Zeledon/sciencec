//! The free functions of §8: `print`, `write`, `print_error`, `write_error`,
//! `flush`, `read_file`, `write_file` — and `stdlib-core.md` §4.4's
//! `read_line`, the ninth.
//!
//! The two file functions are the one place in this crate where the runtime
//! assembles a Science aggregate itself rather than leaving it to codegen: every
//! type involved is concrete here, so its layout is known. `read_file` returns
//! the pair `(String, IoError?)` of `syntax-revision-2.md` §3 and `write_file`
//! returns `IoError?` alone. See the crate documentation, §5, for the rules the
//! three types below follow.

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

    pub(crate) fn from_io(error: &std::io::Error) -> Self {
        match error.kind() {
            std::io::ErrorKind::NotFound => Self::NOT_FOUND,
            std::io::ErrorKind::PermissionDenied => Self::PERMISSION_DENIED,
            std::io::ErrorKind::AlreadyExists => Self::ALREADY_EXISTS,
            std::io::ErrorKind::InvalidData => Self::INVALID_DATA,
            _ => Self::OTHER,
        }
    }
}

/// `IoError implements Error`'s `message(self) -> String`.
///
/// **The five sentences are this crate's, and they are all there is.** §7.4 of
/// `stdlib-core.md` gives `Error` its one required method and says nothing about
/// what an `IoError`'s message reads; the variants carry no path and no OS
/// detail, so the sentence names the kind and nothing more. A richer message is
/// a richer `IoError`, which is a representation change and not this function's.
///
/// # Safety
///
/// `error` must be a non-null, aligned pointer to a live [`ScienceIoError`].
#[no_mangle]
pub unsafe extern "C" fn science_io_error_message(error: *const ScienceIoError) -> ScienceString {
    // SAFETY: the caller guarantees a live error code.
    let text = io_error_sentence(unsafe { *error });
    // SAFETY: a `&str` is valid UTF-8 of its own length.
    unsafe { ScienceString::from_raw_utf8(text.as_ptr(), text.len()) }
}

/// The five sentences, once: what [`science_io_error_message`] returns and
/// what [`science_string_push_io_error`] appends.
///
/// One function rather than two `match`es, because `print(err)` and
/// `print(err.message())` printing different text would be a defect no type
/// could catch, and one table is what makes it impossible rather than
/// unlikely.
fn io_error_sentence(error: ScienceIoError) -> &'static str {
    match error {
        ScienceIoError::NOT_FOUND => "not found",
        ScienceIoError::PERMISSION_DENIED => "permission denied",
        ScienceIoError::ALREADY_EXISTS => "already exists",
        ScienceIoError::INVALID_DATA => "invalid data",
        _ => "input/output error",
    }
}

/// `IoError implements Display`: append the error's sentence to a `String`.
///
/// # The decision
///
/// **An eighth `science_string_push_*`, and `IoError`'s `Display` is its
/// `message()`.** `science-mir`'s `Builder::push_of` answers a hole of type
/// `IoError` with this entry point, by pointer, exactly as it answers a
/// `String` hole with `science_string_push_str` — so `print(err)`,
/// `write(err)`, `print_error(err)`, `write_error(err)` and `f"… {err} …"`
/// are one lowering, the builder's, and not five.
///
/// # The reason
///
/// The prelude has listed `IoError` as implementing `Display` for as long as
/// it has had the type, and `stdlib-core.md` §7.4 gives `Error` exactly one
/// method, whose job is the human-readable sentence; a rendering that said
/// anything else would be a second answer to one question. The alternatives
/// were §3.1's `Formatter` — a `display` body for a one-byte code, with no
/// user source for it to live in — or composing `message()`,
/// `science_string_push_str` and a free in `science-mir`, which is three calls
/// and an allocation per hole to produce the bytes this function copies from a
/// `&'static str`.
///
/// # The cost
///
/// One more symbol in `science-codegen`'s `RUNTIME`, and its count tests. A
/// format specification on the hole needs nothing here: the lexer refuses
/// every one, for every type, before any hole is typed.
///
/// # Safety
///
/// `value` must be a non-null, aligned pointer to a live [`ScienceString`];
/// `error` a non-null pointer to a live [`ScienceIoError`].
#[no_mangle]
pub unsafe extern "C" fn science_string_push_io_error(
    value: *mut ScienceString,
    error: *const ScienceIoError,
) {
    // SAFETY: the caller guarantees a live error code.
    let text = io_error_sentence(unsafe { *error });
    // SAFETY: the caller guarantees a live string, and a `&'static str` cannot
    // alias its buffer.
    unsafe { (*value).append(text.as_bytes()) };
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

/// Science's `String?`, the first half of [`ScienceNullableStringAndIoError`].
///
/// **Tagged, not niched**: a `ScienceString`'s pointer is a raw one, and the
/// closed niche table of §5.2 is the three never-null pointer kinds and
/// nothing else — so `String?` is a
/// discriminant byte then the string at the next eight-byte offset — the
/// layout `science-codegen`'s `CgTy::nullable` gives any `T?` whose `T` has
/// no niche, which is what makes this the same bytes as Science's own.
#[repr(C)]
pub struct ScienceNullableString {
    /// [`SCIENCE_NULLABLE_NULL`] or [`SCIENCE_NULLABLE_PRESENT`].
    pub present: u8,
    /// The string, meaningful — and owned by the caller — only when
    /// `present` is [`SCIENCE_NULLABLE_PRESENT`]. The empty string otherwise,
    /// which allocates nothing.
    pub value: ScienceString,
}

/// Science's `(String?, IoError?)`, the return type of [`science_read_line`].
///
/// [`ScienceStringAndIoError`] with a `String?` where that has a `String`:
/// forty bytes on a 64-bit target — the tag, seven of padding, three words of
/// string, the two error bytes and six of tail padding — so it comes back
/// through `sret` on every supported convention.
#[repr(C)]
pub struct ScienceNullableStringAndIoError {
    /// The line, or null at end of input.
    pub value: ScienceNullableString,
    /// What reading failed with, or null.
    pub error: ScienceNullableIoError,
}

/// Science's `read_line() -> (String?, IoError?)` — `stdlib-core.md` §4.4's
/// ninth free function, and the first input a program in a pipe can use.
///
/// # The decision
///
/// - **`null` at end of input, `""` for a blank line**, which is §4.4's whole
///   point: the two are distinguishable. A last line with no `\n` after it is
///   a line, and the read after it is `null`.
/// - **The `\n` is removed, and so is one `\r` at the end of what is left** —
///   exactly what `String.lines()` does (`science_lines_next`), including on
///   a last line with no `\n`, so a file read line by line and the same file
///   read whole and split agree on every line. A `\r` anywhere else is text.
/// - **Not UTF-8 is [`ScienceIoError::INVALID_DATA`]**, `read_file`'s answer
///   to the same bytes; the line is consumed, so the next call reads the line
///   after it rather than failing forever.
/// - **`IoError?`, not §4.4's `Error?`**: §7.3 of the same note, *"no Level 1
///   function has `Error?` in its signature"*, which §4.7's own `count_lines`
///   follows by returning `read_line`'s error as an `IoError?`.
/// - **Standard output is flushed first when it is a terminal** — C's line
///   discipline, and `stdout.rs`'s `flush_if_terminal` says why only then.
///
/// # The reason it reads through `std::io::stdin()`
///
/// That is the process's one buffer in front of descriptor 0, and `io`'s
/// `Stdin` ([`science_stdin_read`](crate::science_stdin_read)) reads through
/// it too, so a program may read a header with this and the rest through a
/// `BufferedReader` over `Stdin` without a byte falling between them.
///
/// # The cost
///
/// A `String` per line, which §4.4 states; a program reading ten million lines
/// reads them through `io`.
#[no_mangle]
pub extern "C" fn science_read_line() -> ScienceNullableStringAndIoError {
    use std::io::BufRead;
    crate::stdout::flush_if_terminal();
    let mut line = Vec::new();
    let read = std::io::stdin().lock().read_until(b'\n', &mut line);
    let absent = |error: ScienceNullableIoError| ScienceNullableStringAndIoError {
        value: ScienceNullableString { present: SCIENCE_NULLABLE_NULL, value: ScienceString::empty() },
        error,
    };
    match read {
        Err(error) => absent(ScienceNullableIoError::present(ScienceIoError::from_io(&error))),
        Ok(0) => absent(ScienceNullableIoError::null()),
        Ok(_) => {
            let text = line.strip_suffix(b"\n").unwrap_or(&line);
            let text = text.strip_suffix(b"\r").unwrap_or(text);
            if std::str::from_utf8(text).is_err() {
                return absent(ScienceNullableIoError::present(ScienceIoError::INVALID_DATA));
            }
            ScienceNullableStringAndIoError {
                value: ScienceNullableString {
                    present: SCIENCE_NULLABLE_PRESENT,
                    // SAFETY: just validated as UTF-8, and `text` is `len`
                    // readable bytes of `line`.
                    value: unsafe { ScienceString::from_raw_utf8(text.as_ptr(), text.len()) },
                },
                error: ScienceNullableIoError::null(),
            }
        }
    }
}

/// Science's `write(text: borrowed String)` — the form that adds nothing.
///
/// Writes the bytes to standard output verbatim, adding nothing. Standard
/// output is line-buffered on a terminal and 64 KiB block-buffered otherwise
/// (`stdout.rs`), so a `write` with no newline in it may sit in the buffer
/// until one arrives — or, in a pipe, until the block fills;
/// [`science_flush`], [`science_exit`](crate::science_exit) and
/// [`science_panic`](crate::science_panic) all empty it so that it is not lost.
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
    crate::stdout::write_parts(&[bytes]);
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
    crate::stdout::write_parts(&[bytes, b"\n"]);
}

/// Science's `write_error(value: borrowed any Display)` — [`science_write`]'s
/// stderr twin, and the form that adds nothing.
///
/// # The decision
///
/// **This is [`science_write_error_bytes`](crate::science_write_error_bytes)
/// with a `ScienceString` in front of it, and nothing else.** That symbol was
/// named for this spelling before this one existed — its own note says so — and
/// it already does every part of `strings-formatting-and-docs.md` §4.2's row:
/// stderr, verbatim, unbuffered, a write error ignored. Writing the body a
/// second time would be two stderr writers that could come to disagree, which
/// is the defect `science-codegen-llvm`'s `runtime_reachability.rs` catalogues
/// under other names.
///
/// # The reason
///
/// §4.2's table gives `print_error` and `write_error` the one stream and the
/// one policy — *"stderr is unbuffered. A message that precedes a crash must
/// survive the crash"* — and Rust's `std::io::stderr()` is unbuffered, so the
/// policy is the platform's and not something this crate implements.
///
/// # The cost
///
/// **Standard output is flushed first, on every call**, because the delegate
/// does. §4.2 says cross-stream ordering *"is not guaranteed"*, so this is the
/// same courtesy `science_panic` pays and not a contract; what it costs is a
/// flush of whatever stdout holds each time a message goes to stderr. On a
/// terminal that is at most a partial line. **In a pipe it is whatever the
/// 64 KiB block buffer has collected**, so a program that interleaves every
/// `print` with a `print_error` gets one stdout `write` per stderr line and
/// none of block buffering's saving — but no more syscalls than it made
/// before `stdout.rs` existed, and the ordering it would see at a terminal.
/// A program that follows §4.2's own rule — *"results to stdout, progress
/// and warnings to stderr"* — with occasional progress pays one stdout
/// `write` per progress message, which it would not notice.
///
/// # Safety
///
/// `text` must be a non-null, aligned pointer to a live [`ScienceString`].
#[no_mangle]
pub unsafe extern "C" fn science_write_error(text: *const ScienceString) {
    // SAFETY: the caller guarantees a live `ScienceString`, and its `bytes()`
    // are `len` readable bytes at a non-null, aligned pointer — including the
    // empty string's dangling one (crate documentation, §7).
    unsafe {
        let bytes = (*text).bytes();
        crate::science_write_error_bytes(bytes.as_ptr(), bytes.len());
    }
}

/// Science's `print_error(value: borrowed any Display)` — [`science_print`]'s
/// stderr twin.
///
/// As [`science_write_error`], followed by one `\n`, which is exactly
/// [`science_print`]'s relation to [`science_write`] one stream over. The line
/// feed is `\n` on every platform for the reason [`science_print`] gives.
///
/// **One write, not two.** The text and its newline are handed to the stderr
/// writer as a single buffer, so an unbuffered stream sees one `write(2)` and a
/// line from `print_error` cannot be split by anything another thread or a C
/// library writes to stderr between the halves. F0 has no threads; a C library
/// linked through an `extern` block does write to stderr, and it is the reason
/// the allocation is paid rather than a second syscall.
///
/// # Safety
///
/// `text` must be a non-null, aligned pointer to a live [`ScienceString`].
#[no_mangle]
pub unsafe extern "C" fn science_print_error(text: *const ScienceString) {
    // SAFETY: the caller guarantees a live `ScienceString`.
    let bytes = unsafe { (*text).bytes() };
    let mut line = Vec::with_capacity(bytes.len() + 1);
    line.extend_from_slice(bytes);
    line.push(b'\n');
    // SAFETY: `line` owns `line.len()` initialised bytes and is non-null.
    unsafe { crate::science_write_error_bytes(line.as_ptr(), line.len()) };
}

/// Science's `flush()` — empties standard output's buffer, and returns.
///
/// # The decision
///
/// **It flushes every buffered layer a Science program's stdout goes
/// through**, which is three: `stdout.rs`'s 64 KiB block buffer, where
/// `print` and `write` put their bytes when stdout is not a terminal; Rust's
/// `LineWriter` inside `std::io::stdout()`, where they put them when it is;
/// and the C runtime's streams, where a C library called through an `extern`
/// block puts its own. It is the same `flush_all`
/// [`science_exit`](crate::science_exit) ends the process with, called
/// without ending it.
///
/// # The reason
///
/// `strings-formatting-and-docs.md` §4.2 adds `flush` *"reluctantly"* and for
/// exactly one case: *"`write` of a progress line with no newline is
/// line-buffered into invisibility otherwise"*. That case is live on a
/// terminal, where a `LineWriter` holds a partial line until a `\n` arrives,
/// and wider in a pipe, where the block buffer holds whole lines until 64 KiB
/// of them have collected. Flushing only one layer would make `flush()` a
/// statement about which buffer the user's bytes happened to be in, and the
/// user cannot see which one that is.
///
/// # The cost
///
/// **`fflush(NULL)` flushes every C output stream, not only `stdout`** — a
/// file a C library opened is flushed too. That is harmless, since flushing
/// never discards anything, and it is `flush_all`'s documented choice; naming
/// C's `stdout` alone would need its per-platform spelling (`stdout`,
/// `__stdoutp`, `__acrt_iob_func(1)`) in a crate that otherwise reaches libc
/// through one portable declaration.
///
/// A flush error is ignored, for [`science_write`]'s reason: `flush()` returns
/// nothing to report one through.
#[no_mangle]
pub extern "C" fn science_flush() {
    crate::exit::flush_all();
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
