//! `TextError`, and the two `String` methods that produce one:
//! `parse_int(self) -> (I64, TextError?)` and `parse_float(self) -> (F64,
//! TextError?)` of `stdlib-core.md` §6.9.
//!
//! The layout follows `io.rs` exactly, for `io.rs`'s reasons: a payload-free
//! error is its own discriminant byte (the crate documentation, §5.1), its
//! nullable form is that byte after a tag byte (§5.2 and Decision 6), and the
//! pair is a plain struct with both halves live (§5.4).

use crate::abi::{SCIENCE_NULLABLE_NULL, SCIENCE_NULLABLE_PRESENT};
use crate::string::ScienceString;

/// Science's `TextError`.
///
/// # The decision
///
/// **One byte, payload-free, numbered in `stdlib-core.md` §7.4's declaration
/// order**: `NotUtf8`, `NotACharacterBoundary`, `NotANumber`, `OutOfRange` are
/// 0 to 3. Only the last two are produced, by the two parse entry points below;
/// the first two belong to `String.from_bytes` and `String.slice`, which have
/// no entry point yet and are numbered now so that they do not renumber these
/// when they arrive.
///
/// # The reason
///
/// §7.4 gives the four variants payloads — a byte offset for the first two, the
/// offending text for the last two — and the prelude deliberately declares
/// **none** of them: `builtins.rs`'s note above `LIBRARY_TYPES` says four
/// variant names in every program's scope is a namespace decision it has no
/// reason to take on the way past. With no variant a program can name or
/// match, nothing can read a payload, so carrying one would be representation
/// no reader reaches — and a `String` payload would make `TextError` own heap
/// memory, which is a `drop` the compiler would have to emit on every error
/// path for a value nobody can inspect. `IoError` made the same choice against
/// §7.4's `Other(I32, String)` for the same reason.
///
/// # The cost
///
/// `err.message()` cannot say *which* text failed to parse, only why — the
/// payloads §7.4 wants are what would let it. Growing them is a representation
/// change to this type, the one in `science-codegen`'s `RtAggregate::TextError`
/// and the prelude's `choice`, made together; the numbering here does not have
/// to move for it.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScienceTextError(pub u8);

impl ScienceTextError {
    /// The bytes are not valid UTF-8. §7.4's `NotUtf8`; not produced yet.
    pub const NOT_UTF8: Self = Self(0);
    /// A byte offset falls inside a character. §7.4's `NotACharacterBoundary`;
    /// not produced yet.
    pub const NOT_A_CHARACTER_BOUNDARY: Self = Self(1);
    /// The text is not a number in the accepted syntax — including the empty
    /// string and a sign with no digits after it.
    pub const NOT_A_NUMBER: Self = Self(2);
    /// The text is a number, and the type cannot hold it.
    pub const OUT_OF_RANGE: Self = Self(3);
}

/// `TextError implements Error`'s `message(self) -> String`.
///
/// The four sentences are this crate's, as `science_io_error_message`'s five
/// are: §7.4 fixes the variants and not what they read as.
///
/// # Safety
///
/// `error` must be a non-null, aligned pointer to a live [`ScienceTextError`].
#[no_mangle]
pub unsafe extern "C" fn science_text_error_message(error: *const ScienceTextError) -> ScienceString {
    // SAFETY: the caller guarantees a live error code.
    let text = text_error_sentence(unsafe { *error });
    // SAFETY: a `&str` is valid UTF-8 of its own length.
    unsafe { ScienceString::from_raw_utf8(text.as_ptr(), text.len()) }
}

/// The four sentences, once, for `io.rs`'s `io_error_sentence`'s reason.
fn text_error_sentence(error: ScienceTextError) -> &'static str {
    match error {
        ScienceTextError::NOT_UTF8 => "not valid UTF-8",
        ScienceTextError::NOT_A_CHARACTER_BOUNDARY => "not a character boundary",
        ScienceTextError::NOT_A_NUMBER => "not a number",
        ScienceTextError::OUT_OF_RANGE => "number out of range",
        _ => "text error",
    }
}

/// `TextError implements Display`: append the error's sentence to a `String`.
///
/// [`crate::science_string_push_io_error`]'s decision, reason and cost, for
/// the other error type at Level 1: `print(err)` and `f"{err}"` on a
/// `TextError` render exactly what `err.message()` returns.
///
/// # Safety
///
/// `value` must be a non-null, aligned pointer to a live [`ScienceString`];
/// `error` a non-null pointer to a live [`ScienceTextError`].
#[no_mangle]
pub unsafe extern "C" fn science_string_push_text_error(
    value: *mut ScienceString,
    error: *const ScienceTextError,
) {
    // SAFETY: the caller guarantees a live error code.
    let text = text_error_sentence(unsafe { *error });
    // SAFETY: the caller guarantees a live string, and a `&'static str` cannot
    // alias its buffer.
    unsafe { (*value).append(text.as_bytes()) };
}

/// Science's `TextError?`: a discriminant byte, then the error.
///
/// [`crate::ScienceNullableIoError`]'s layout for its reasons, including the
/// refusal to niche the 252 unused code points into a free `null`.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScienceNullableTextError {
    /// [`SCIENCE_NULLABLE_NULL`] or [`SCIENCE_NULLABLE_PRESENT`], and never any
    /// other value. Bit-for-bit the `Bool` that `err?` yields.
    pub present: u8,
    /// The error, meaningful only when `present` is set. Zero when it is not.
    pub error: ScienceTextError,
}

impl ScienceNullableTextError {
    #[inline]
    const fn null() -> Self {
        Self { present: SCIENCE_NULLABLE_NULL, error: ScienceTextError(0) }
    }

    #[inline]
    const fn present(error: ScienceTextError) -> Self {
        Self { present: SCIENCE_NULLABLE_PRESENT, error }
    }
}

/// Science's `(I64, TextError?)`, the return type of
/// [`science_string_parse_int`].
///
/// A plain struct, both halves live (§5.4): `value` at 0, `error` at 8, six
/// bytes of tail padding, sixteen bytes aligned to eight. **Sixteen bytes is
/// under the `sret` line on every F0 target**, so unlike
/// [`crate::ScienceStringAndIoError`] this comes back in two registers — `x0`
/// and `x1` on AArch64, `rax` and `rdx` on System V — and on Windows x64,
/// whose register return stops at eight bytes, through a hidden pointer.
/// Nothing owns memory here, so the failing path has nothing to free: the
/// value half is `0`.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScienceI64AndTextError {
    /// The parsed integer, or `0` when `error` is present.
    pub value: i64,
    /// What went wrong, or null.
    pub error: ScienceNullableTextError,
}

/// Science's `(F64, TextError?)`, the return type of
/// [`science_string_parse_float`].
///
/// [`ScienceI64AndTextError`]'s layout with a `double` in the first word.
/// That word changes the System V classification — `xmm0` for the value and
/// `rax` for the error — and changes nothing on AArch64, where a struct that
/// is not all floats is two `x` registers whatever its first member is.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ScienceF64AndTextError {
    /// The parsed number, or `0.0` when `error` is present.
    pub value: f64,
    /// What went wrong, or null.
    pub error: ScienceNullableTextError,
}

/// Science's `String.parse_int(self) -> (I64, TextError?)`.
///
/// # The syntax, decided here
///
/// **An optional `+` or `-`, then one or more ASCII decimal digits, and nothing
/// else** — no surrounding whitespace, no `_` separators, no `0x` prefix.
/// That is Rust's `i64::from_str`, which this calls, stated as Science's.
///
/// **Whitespace is refused rather than trimmed**, and `stdlib-core.md` §6.11
/// is why: its worked example calls `line.trim()` and then `parse_float()` on
/// the result, which is only the right program if parsing does not trim. §6.9
/// pins `trim` to ASCII whitespace *forever* so that its meaning cannot drift;
/// a parse that trimmed would be a second, unpinned definition of whitespace.
/// And §6.9 makes these Level 1 because the answer is unique — a parse that
/// accepts `" 42"` has chosen an answer for `"4 2"` too, and there is no
/// unique one.
///
/// # Errors
///
/// [`ScienceTextError::NOT_A_NUMBER`] for anything outside the syntax,
/// including `""`, `"-"` and `"+"`; [`ScienceTextError::OUT_OF_RANGE`] for a
/// well-formed integer outside `I64`, in either direction. The value half is
/// `0` on both.
///
/// # Safety
///
/// `text` must be a non-null, aligned pointer to a live [`ScienceString`].
#[no_mangle]
pub unsafe extern "C" fn science_string_parse_int(text: *const ScienceString) -> ScienceI64AndTextError {
    // SAFETY: the caller guarantees a live `ScienceString`, whose bytes are UTF-8.
    let text = unsafe { (*text).as_str() };
    match text.parse::<i64>() {
        Ok(value) => ScienceI64AndTextError { value, error: ScienceNullableTextError::null() },
        Err(error) => {
            let code = match error.kind() {
                std::num::IntErrorKind::PosOverflow | std::num::IntErrorKind::NegOverflow => {
                    ScienceTextError::OUT_OF_RANGE
                }
                _ => ScienceTextError::NOT_A_NUMBER,
            };
            ScienceI64AndTextError { value: 0, error: ScienceNullableTextError::present(code) }
        }
    }
}

/// Science's `String.parse_float(self) -> (F64, TextError?)`.
///
/// # Correctly rounded, which is the whole requirement
///
/// §6.9: *"correctly-rounded decimal parsing has a unique answer"* — the
/// nearest `F64` to the decimal, ties to even — *"the algorithm … is free."*
/// Rust's `f64::from_str` is correctly rounded for every input (Eisel–Lemire
/// with a big-decimal fallback), so the answer is its answer.
///
/// # The syntax, decided here
///
/// Rust's, with its whitespace rule and for [`science_string_parse_int`]'s
/// reasons: an optional sign; digits with an optional `.` and fraction (either
/// side may be empty, not both); an optional `e`/`E` exponent with its own
/// sign; or, case-insensitively, `inf`, `infinity` or `nan`.
///
/// **The three words are accepted because Science prints them.**
/// `science_string_push_f64` renders a non-finite `F64` as `inf`, `-inf` and
/// `NaN`, so accepting them makes `print` and `parse_float` inverses on every
/// `F64` — the shortest round-trip rendering already guarantees it for the
/// finite ones.
///
/// # Errors
///
/// [`ScienceTextError::NOT_A_NUMBER`] outside the syntax.
/// [`ScienceTextError::OUT_OF_RANGE`] for **finite digits whose correctly
/// rounded value is infinite** — `"1e400"`, where Rust answers `inf` and no
/// error. That is decided here: IEEE 754's overflow rounds to infinity, but a
/// text that names a finite number has not been parsed to it by returning one
/// that is not, and §7.4 provides `OutOfRange` for exactly this. Underflow is
/// not an error: `"1e-400"` is `0.0`, which *is* the correctly rounded
/// answer. The value half is `0.0` on both errors.
///
/// # Safety
///
/// `text` must be a non-null, aligned pointer to a live [`ScienceString`].
#[no_mangle]
pub unsafe extern "C" fn science_string_parse_float(text: *const ScienceString) -> ScienceF64AndTextError {
    // SAFETY: the caller guarantees a live `ScienceString`, whose bytes are UTF-8.
    let text = unsafe { (*text).as_str() };
    let failed = |code| ScienceF64AndTextError { value: 0.0, error: ScienceNullableTextError::present(code) };
    match text.parse::<f64>() {
        Ok(value) if value.is_infinite() && !names_infinity(text) => failed(ScienceTextError::OUT_OF_RANGE),
        Ok(value) => ScienceF64AndTextError { value, error: ScienceNullableTextError::null() },
        Err(_) => failed(ScienceTextError::NOT_A_NUMBER),
    }
}

/// Whether an accepted float text spelled infinity out, rather than
/// overflowing to it. After the sign, Rust's grammar starts a number with a
/// digit or a `.` and a word with a letter, so the first byte decides.
fn names_infinity(text: &str) -> bool {
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    unsigned.bytes().next().is_some_and(|byte| byte.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn string(text: &str) -> ScienceString {
        // SAFETY: a `&str` is valid UTF-8 of its own length.
        unsafe { ScienceString::from_raw_utf8(text.as_ptr(), text.len()) }
    }

    fn int(text: &str) -> (i64, Option<ScienceTextError>) {
        let s = string(text);
        // SAFETY: `s` is live for the call.
        let pair = unsafe { science_string_parse_int(&s) };
        // SAFETY: `s` was made above and is freed once.
        unsafe { crate::science_string_free(&s as *const _ as *mut _) };
        let error = (pair.error.present == SCIENCE_NULLABLE_PRESENT).then_some(pair.error.error);
        (pair.value, error)
    }

    fn float(text: &str) -> (f64, Option<ScienceTextError>) {
        let s = string(text);
        // SAFETY: `s` is live for the call.
        let pair = unsafe { science_string_parse_float(&s) };
        // SAFETY: `s` was made above and is freed once.
        unsafe { crate::science_string_free(&s as *const _ as *mut _) };
        let error = (pair.error.present == SCIENCE_NULLABLE_PRESENT).then_some(pair.error.error);
        (pair.value, error)
    }

    #[test]
    fn integers_in_the_syntax_parse_and_the_error_is_null() {
        assert_eq!(int("42"), (42, None));
        assert_eq!(int("-17"), (-17, None));
        assert_eq!(int("+5"), (5, None));
        assert_eq!(int("007"), (7, None));
        assert_eq!(int("9223372036854775807"), (i64::MAX, None));
        assert_eq!(int("-9223372036854775808"), (i64::MIN, None));
    }

    #[test]
    fn integers_outside_the_syntax_are_not_a_number_and_zero() {
        for text in ["", "-", "+", "4x2", " 42", "42 ", "1_000", "0x10", "1.0", "٣"] {
            assert_eq!(int(text), (0, Some(ScienceTextError::NOT_A_NUMBER)), "{text:?}");
        }
    }

    #[test]
    fn integers_past_either_end_are_out_of_range() {
        assert_eq!(int("9223372036854775808"), (0, Some(ScienceTextError::OUT_OF_RANGE)));
        assert_eq!(int("-9223372036854775809"), (0, Some(ScienceTextError::OUT_OF_RANGE)));
    }

    #[test]
    fn floats_are_correctly_rounded() {
        // The halfway case between 1 and the next `F64` up rounds to even, and
        // one digit past it rounds up — the case a naive digit-accumulating
        // parser gets wrong.
        assert_eq!(float("1.00000000000000011102230246251565404236316680908203125"), (1.0, None));
        assert_eq!(
            float("1.00000000000000011102230246251565404236316680908203126"),
            (1.0000000000000002, None)
        );
        assert_eq!(float("0.1"), (0.1, None));
        assert_eq!(float("2.2250738585072014e-308"), (f64::MIN_POSITIVE, None));
        assert_eq!(float("1e-400"), (0.0, None));
        assert_eq!(float("-.5"), (-0.5, None));
        assert_eq!(float("5."), (5.0, None));
    }

    #[test]
    fn what_science_prints_for_a_non_finite_float_parses_back() {
        assert_eq!(float("inf"), (f64::INFINITY, None));
        assert_eq!(float("-inf"), (f64::NEG_INFINITY, None));
        let (nan, error) = float("NaN");
        assert!(nan.is_nan() && error.is_none());
    }

    #[test]
    fn floats_outside_the_syntax_or_the_range_are_errors_and_zero() {
        for text in ["", ".", "e5", "1e", " 1.5", "1.5 ", "1,5", "0x1p3"] {
            assert_eq!(float(text), (0.0, Some(ScienceTextError::NOT_A_NUMBER)), "{text:?}");
        }
        assert_eq!(float("1e400"), (0.0, Some(ScienceTextError::OUT_OF_RANGE)));
        assert_eq!(float("-1e400"), (0.0, Some(ScienceTextError::OUT_OF_RANGE)));
    }

    #[test]
    fn every_code_has_its_own_sentence() {
        let message = |code: ScienceTextError| {
            // SAFETY: `code` is live for the call.
            let s = unsafe { science_text_error_message(&code) };
            // SAFETY: the runtime returned a valid string.
            let text = unsafe { s.as_str() }.to_string();
            // SAFETY: freed once.
            unsafe { crate::science_string_free(&s as *const _ as *mut _) };
            text
        };
        assert_eq!(message(ScienceTextError::NOT_A_NUMBER), "not a number");
        assert_eq!(message(ScienceTextError::OUT_OF_RANGE), "number out of range");
        assert_eq!(message(ScienceTextError::NOT_UTF8), "not valid UTF-8");
        assert_eq!(message(ScienceTextError::NOT_A_CHARACTER_BOUNDARY), "not a character boundary");
    }
}
