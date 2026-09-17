//! Rendering a value into a `String`: the seven entry points an `f"…"` is
//! built out of.
//!
//! # The decision
//!
//! **A value is rendered by appending it to a `String` the caller already
//! owns, never by returning a fresh one.** Every entry point here takes
//! `*mut ScienceString` first and returns `()`.
//!
//! # The reason
//!
//! Two, and the second is the one that decided it.
//!
//! **It is what `strings-formatting-and-docs.md` §1.7 asks for.** An `f"…"`
//! *"lowers to a builder over the fragments, with the capacity pre-computed
//! from the literal fragments plus a per-type estimate for each hole, so the
//! common case is one allocation"*. A `science_i64_to_string(x) -> String`
//! would give the common case **two** allocations and a free — one for the
//! rendered number, one for the accumulator, and a `science_string_free` for
//! the temporary — on every hole of every f-string. §1.7's "one allocation" is
//! only reachable if the renderer writes into the accumulator.
//!
//! **And this is a scientific-computing language, where a print in a loop is a
//! real pattern.** `print(f"row {i}: {value}")` over a million rows is a
//! reasonable thing to write, and the difference between one allocation per
//! line and four is the difference between a progress log and a profile.
//!
//! **What it does not decide, which is the point.** Nothing here is a Science
//! name, a method, or an interface. `science-resolve`'s `builtins` has refused
//! three times to write `Display.display(Formatter)`, because naming that
//! method would invent a `Formatter` — a type, a `FormatSpec` record, four
//! `choice` types and a closed method set — as a side effect of a bound check.
//! That refusal is untouched: these are entry points a code generator calls,
//! in the same class as [`crate::science_string_from_bytes`], which is how a
//! string literal becomes a value and is likewise not a Science spelling of
//! anything. The language-level question — *what does `Display` require?* —
//! is exactly as open as it was.
//!
//! # The cost
//!
//! **The set is closed at the primitives, and it is a list rather than a
//! rule.** `I8`…`I64` all arrive here as [`science_string_push_i64`] after
//! `science-mir` sign-extends them with an `Rvalue::Cast`, `U8`…`U64` as
//! [`science_string_push_u64`] after it zero-extends them, and
//! `F16` and `BF16` arrive as nothing at all: there is no Rust primitive to
//! render them through and no note that says what their shortest round-trip
//! spelling is. A user type that implements `Display` has no entry point here
//! and cannot: rendering it means calling its `display`, and there is no
//! method to call. So `f"{station}"` on a record type type-checks and does not
//! lower, which `science-types`' `fstring` states at the check that lets it
//! through.
//!
//! **Nothing here honours a format specification**, because there is no way to
//! pass one: §2's mini-language is unimplemented and the lexer refuses every
//! spec with `SC0173`. When it lands, it arrives as arguments to these
//! functions or as siblings of them, and §2.3's defaults — which are what
//! these implement — stay the no-spec case either way.
//!
//! # §2's `sret` list does not move, and that is checked
//!
//! Every function here returns `()`. None is classified MEMORY on either
//! supported convention, none takes a hidden return pointer, and the derived
//! set stays the nine the crate documentation names. That is the same answer
//! `exit.rs`'s two entry points got and it is arrived at the same way — by
//! reading the signatures rather than by deciding — which is the whole of what
//! §2's "this list has been wrong twice" is for. The count of entry points
//! goes from 47 to 54; the count of `sret` returns stays at 9.
//!
//! **§1.7's capacity later moved that count, which nothing here did.**
//! [`crate::science_string_with_capacity`] is the fifty-fifth entry point and
//! the tenth `sret` return: three words by value, MEMORY on both conventions,
//! read off the signature the same way these seven were. This section is about
//! a design that avoided joining the list; that one is about a design that had
//! to. Both numbers came from the same derivation and neither from a
//! judgement.
//!
//! The push shape is what makes that true, and it was not chosen for it: a
//! `science_i64_to_string` returning `ScienceString` by value would have been
//! three words, therefore MEMORY, therefore the tenth member of a list that
//! has twice been wrong. Avoiding an allocation and avoiding an `sret` turn
//! out to be the same edit.

use std::fmt::Write;

use crate::string::ScienceString;

/// A `ScienceString` seen as a sink for `core::fmt`.
///
/// **This is what makes a rendered number cost no allocation at all.**
/// `write!` drives the formatting machinery straight into the accumulator's
/// buffer, so there is no intermediate `String`, no stack buffer with a length
/// limit to get wrong, and no second copy.
struct Sink<'a>(&'a mut ScienceString);

impl Write for Sink<'_> {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        // SAFETY: `Sink` is only ever constructed from a live `ScienceString`,
        // and a `&str` is valid UTF-8 by its own invariant. The text comes
        // from `core::fmt`'s own buffer and cannot alias the string.
        unsafe { self.0.append(text.as_bytes()) };
        Ok(())
    }
}

/// Append raw UTF-8 bytes.
///
/// **Codegen support.** This is how the *literal* half of an `f"…"` is built:
/// a text run is a `private unnamed_addr constant` in the binary, exactly like
/// the operand of [`crate::science_string_from_bytes`], and this appends it in
/// place.
///
/// The bytes are validated, for [`crate::science_string_from_bytes`]'s reason:
/// every other function in this crate treats the UTF-8 invariant as given, and
/// `science_string_truncate` in particular would then be able to cut
/// mid-character.
///
/// # Safety
///
/// `value` must be a non-null, aligned pointer to a live [`ScienceString`];
/// `bytes` must point to `len` readable bytes that are not part of that
/// string's own buffer.
#[no_mangle]
pub unsafe extern "C" fn science_string_push_bytes(
    value: *mut ScienceString,
    bytes: *const u8,
    len: usize,
) {
    if len == 0 {
        return;
    }
    // SAFETY: the caller guarantees `len` readable bytes at `bytes`.
    let bytes = unsafe { std::slice::from_raw_parts(bytes, len) };
    if std::str::from_utf8(bytes).is_err() {
        let message = b"science-rt: String extended with bytes that are not valid UTF-8";
        // SAFETY: a static byte string is a valid buffer of that length.
        unsafe { crate::science_panic_bytes(message.as_ptr(), message.len()) }
    }
    // SAFETY: the caller guarantees a live string, and the bytes are UTF-8.
    unsafe { (*value).append(bytes) };
}

/// Append a signed integer in base ten.
///
/// Every signed width renders through this one: `I8`…`I64` are sign-extended
/// before the call, and sign extension does not change the digits. §2.3 gives
/// no grouping by default, so there is none — `1234567`, not `1,234,567`.
///
/// **The phase that extends is `science-mir`'s `Builder::push_of`**, which
/// emits an `Rvalue::Cast` to `I64` in front of the call for the three narrow
/// widths. This sentence used to say *"by codegen"* and was **false**: there
/// was no `Rvalue::Cast` lowering anywhere, so nothing between the choice of
/// entry point and the call could have done it, and the three narrow widths
/// were refused rather than rendered. `science-mir`'s §7 item 10 is the record
/// of that, and it is named here because a note that has been wrong once
/// should say when it stopped being.
///
/// **The extension is signed and that is not a detail.** `-1i32`
/// zero-extended is `4294967295`, which is a legal `i64` and the wrong number;
/// nothing below this function could tell the two apart, because an LLVM
/// integer carries no sign. `science-codegen-llvm`'s `tests/casts.rs` runs the
/// values where the swap is visible.
///
/// # Safety
///
/// `value` must be a non-null, aligned pointer to a live [`ScienceString`].
#[no_mangle]
pub unsafe extern "C" fn science_string_push_i64(value: *mut ScienceString, number: i64) {
    // SAFETY: the caller guarantees a live string.
    let mut sink = Sink(unsafe { &mut *value });
    let _ = write!(sink, "{number}");
}

/// Append an unsigned integer in base ten.
///
/// `U8`…`U64` are zero-extended before the call, by the same phase and with
/// the same history as [`science_string_push_i64`]'s sign extension — and
/// **zero**-extended, because this is the entry point whose argument has no
/// sign to preserve. Sending an `I32` through here would render `-1` as
/// `4294967295`; `science-mir`'s `Builder::push_of` keeps the signed and the
/// unsigned list separate for exactly that reason.
///
/// # Safety
///
/// `value` must be a non-null, aligned pointer to a live [`ScienceString`].
#[no_mangle]
pub unsafe extern "C" fn science_string_push_u64(value: *mut ScienceString, number: u64) {
    // SAFETY: the caller guarantees a live string.
    let mut sink = Sink(unsafe { &mut *value });
    let _ = write!(sink, "{number}");
}

/// Append an `F64` in §2.3's default rendering.
///
/// **The decision.** The shortest decimal string that round-trips, **always
/// carrying a decimal point or an exponent**: `1.0`, not `1`.
///
/// **The reason.** §2.3 decides the first half in as many words — *"a float
/// with no code prints the shortest decimal string that round-trips …  a
/// default that silently rounds to six places hides exactly the bug the user
/// needed to see"* — so `f"{0.1 + 0.2}"` is `0.30000000000000004` and this
/// function is why. The second half the note leaves open, because the three
/// languages it cites disagree: Python's `repr(1.0)` is `1.0` and Rust's
/// `{}` is `1`. It is decided here as Python's, and the argument is §2.3's own
/// premise: *this is a language whose users publish*. A column of `F64` that
/// reads `1`, `2`, `3` is a column a reader takes for integers, and the type
/// that produced it is exactly the thing the table was meant to report.
///
/// **The cost** is that a float and an integer of the same value no longer
/// print identically, so a program that formats an identifier as an `F64` gets
/// `12.0` where it wanted `12`. That is a program with the wrong type in it,
/// and printing the type it has is how its author finds out.
///
/// §2.3's other three cases come out of the same call and are checked in
/// `tests/format.rs`: `NaN`, `inf` and `-inf` spelled that way, and `-0.0`
/// distinguishable from `0.0` because *"it is a different float and hiding
/// that has cost people days"*.
///
/// # Safety
///
/// `value` must be a non-null, aligned pointer to a live [`ScienceString`].
#[no_mangle]
pub unsafe extern "C" fn science_string_push_f64(value: *mut ScienceString, number: f64) {
    // SAFETY: the caller guarantees a live string.
    let mut sink = Sink(unsafe { &mut *value });
    // Rust's `Debug` for floats is the shortest round-tripping form *with* the
    // point; its `Display` is the same digits without one. The decision above
    // is the whole of the difference between the two.
    let _ = write!(sink, "{number:?}");
}

/// Append an `F32`, in [`science_string_push_f64`]'s rendering at `f32`
/// precision.
///
/// **It is not a widening of the `F64` entry point**, and the reason is the
/// same one that makes the rendering "shortest round-trip" in the first place:
/// `0.1f32 as f64` is `0.10000000149011612`, which round-trips as an `F64` and
/// is the wrong answer about an `F32`. The shortest string that round-trips is
/// a property of the width, so the width has to reach the formatter.
///
/// # Safety
///
/// `value` must be a non-null, aligned pointer to a live [`ScienceString`].
#[no_mangle]
pub unsafe extern "C" fn science_string_push_f32(value: *mut ScienceString, number: f32) {
    // SAFETY: the caller guarantees a live string.
    let mut sink = Sink(unsafe { &mut *value });
    let _ = write!(sink, "{number:?}");
}

/// Append `true` or `false`.
///
/// §2.3: *"Bool prints `true` / `false`, matching §4.2's literals."* So the
/// output of a program pastes back into one.
///
/// # Safety
///
/// `value` must be a non-null, aligned pointer to a live [`ScienceString`].
#[no_mangle]
pub unsafe extern "C" fn science_string_push_bool(value: *mut ScienceString, flag: bool) {
    // SAFETY: the caller guarantees a live string.
    let mut sink = Sink(unsafe { &mut *value });
    let _ = write!(sink, "{flag}");
}

/// Append one `Char`, as the character itself and **not** in quotes.
///
/// §3.2 is the reason for the "not in quotes": *"a string rendered for a user
/// has no quotes; a string rendered inside a structure being inspected must
/// have them"*, and the same division holds for a character. Quoting is
/// `Inspect`'s, and `Inspect` does not exist.
///
/// A `Char` is a Unicode scalar value, so the argument is a `u32`. A value
/// that is not one cannot arise from a Science `Char`; if one does, it is
/// appended as `U+FFFD` rather than silently dropped, because a hole in the
/// output is harder to trace than a replacement character in it.
///
/// # Safety
///
/// `value` must be a non-null, aligned pointer to a live [`ScienceString`].
#[no_mangle]
pub unsafe extern "C" fn science_string_push_char(value: *mut ScienceString, scalar: u32) {
    let character = char::from_u32(scalar).unwrap_or(char::REPLACEMENT_CHARACTER);
    // SAFETY: the caller guarantees a live string.
    let mut sink = Sink(unsafe { &mut *value });
    let _ = write!(sink, "{character}");
}
