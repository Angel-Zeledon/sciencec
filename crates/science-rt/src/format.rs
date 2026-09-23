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

use crate::abi::{SCIENCE_NULLABLE_NULL, SCIENCE_NULLABLE_PRESENT};
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

// --- `Formatter`, `strings-formatting-and-docs.md` §3.1 --------------------
//
// `Display.display(self, into: &mut Formatter)` needs somewhere to land at
// the runtime layer: a sink to write into and the parsed spec to honour.
// `science-resolve`'s `builtins.rs` now declares `Formatter`, `FormatSpec`
// and the four `choice` types §3.1 names (`Align`, `Sign`, `Code`,
// `Grouping`); this section is their runtime representation and the five
// methods `Formatter has:` promises — `text`, `raw`, `number`, `integer`,
// `spec`.
//
// **Nothing here is reachable from a Science program yet.** `print`'s own
// lowering does not call a user `display` at all — `STDLIB-DECISIONS.md` §1
// measures that gap precisely, quoting `lower.rs` — so these entry points sit
// ahead of their caller, exactly the position `science_string_with_capacity`
// was once in per the crate documentation's §2. What is exercised here is the
// rendering itself, called directly from `tests/format.rs` the same way the
// seven push functions above are.
//
// **§2's mini-language is still unparsed**, so every spec these tests build
// is built by hand in Rust, standing in for what a lexer and `science-types`'
// spec checker will one day produce. The rendering rules below are read out
// of §2.2's table and §2.3's defaults; where a rule for an *explicit* code is
// not written down — the default precision for `.e` with no digit after it,
// say — it follows Python's, because §2.1 says this mini-language *is*
// Python's "with four things removed and one thing added," and a default
// precision is not one of the four removed.

/// Science's `Align`, §3.1's first `choice` type: `<`, `>`, `^` in the spec
/// grammar (§2.1), in that order. Payload-free, so by the crate
/// documentation's §5.1 the type is its discriminant alone.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScienceAlign(pub u8);

impl ScienceAlign {
    /// `<`.
    pub const LEFT: Self = Self(0);
    /// `>`.
    pub const RIGHT: Self = Self(1);
    /// `^`.
    pub const CENTER: Self = Self(2);
}

/// Science's `Sign`: `+`, `-`, `' '` in the spec grammar, in that order.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScienceSign(pub u8);

impl ScienceSign {
    /// `+`.
    pub const PLUS: Self = Self(0);
    /// `-`.
    pub const MINUS: Self = Self(1);
    /// `' '`.
    pub const SPACE: Self = Self(2);
}

/// Science's `Code`: the twelve letters of §2.1's `code` production, in the
/// grammar's own order. One type covers both the float codes and the integer
/// codes — §2.4's table is what restricts which of the twelve a given
/// argument type may name, and that check belongs to `science-types`, not to
/// this crate. The runtime renders whichever one it is given.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScienceCode(pub u8);

impl ScienceCode {
    /// `f`.
    pub const FIXED: Self = Self(0);
    /// `e`.
    pub const EXP: Self = Self(1);
    /// `E`.
    pub const EXP_UPPER: Self = Self(2);
    /// `g`.
    pub const GENERAL: Self = Self(3);
    /// `G`.
    pub const GENERAL_UPPER: Self = Self(4);
    /// `%`.
    pub const PERCENT: Self = Self(5);
    /// `d`.
    pub const DECIMAL: Self = Self(6);
    /// `x`.
    pub const HEX: Self = Self(7);
    /// `X`.
    pub const HEX_UPPER: Self = Self(8);
    /// `o`.
    pub const OCTAL: Self = Self(9);
    /// `b`.
    pub const BINARY: Self = Self(10);
    /// `s`.
    pub const STR: Self = Self(11);
}

/// Science's `Grouping`: `,`, `_` in the spec grammar, in that order.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScienceGrouping(pub u8);

impl ScienceGrouping {
    /// `,`.
    pub const COMMA: Self = Self(0);
    /// `_`.
    pub const UNDERSCORE: Self = Self(1);
}

/// `Align?`. `Align` is payload-free and has no niche of its own — the crate
/// documentation's §5.2 niche table is three pointer-like types and nothing
/// else — so this is the tagged form of §5.1: a discriminant byte, then the
/// payload, [`crate::io::ScienceNullableIoError`]'s shape exactly.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScienceNullableAlign {
    /// [`SCIENCE_NULLABLE_NULL`] or [`SCIENCE_NULLABLE_PRESENT`].
    pub present: u8,
    /// The alignment, meaningful only when `present` is
    /// [`SCIENCE_NULLABLE_PRESENT`].
    pub value: ScienceAlign,
}

impl ScienceNullableAlign {
    /// `Align?`'s `null`.
    pub const fn null() -> Self {
        Self { present: SCIENCE_NULLABLE_NULL, value: ScienceAlign(0) }
    }
    /// `Align?`'s presence case.
    pub const fn some(value: ScienceAlign) -> Self {
        Self { present: SCIENCE_NULLABLE_PRESENT, value }
    }
    fn get(self) -> Option<ScienceAlign> {
        (self.present == SCIENCE_NULLABLE_PRESENT).then_some(self.value)
    }
}

/// `Sign?`, [`ScienceNullableAlign`]'s shape.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScienceNullableSign {
    /// [`SCIENCE_NULLABLE_NULL`] or [`SCIENCE_NULLABLE_PRESENT`].
    pub present: u8,
    /// The sign, meaningful only when `present` is
    /// [`SCIENCE_NULLABLE_PRESENT`].
    pub value: ScienceSign,
}

impl ScienceNullableSign {
    /// `Sign?`'s `null`.
    pub const fn null() -> Self {
        Self { present: SCIENCE_NULLABLE_NULL, value: ScienceSign(0) }
    }
    /// `Sign?`'s presence case.
    pub const fn some(value: ScienceSign) -> Self {
        Self { present: SCIENCE_NULLABLE_PRESENT, value }
    }
    fn get(self) -> Option<ScienceSign> {
        (self.present == SCIENCE_NULLABLE_PRESENT).then_some(self.value)
    }
}

/// `Code?`, [`ScienceNullableAlign`]'s shape.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScienceNullableCode {
    /// [`SCIENCE_NULLABLE_NULL`] or [`SCIENCE_NULLABLE_PRESENT`].
    pub present: u8,
    /// The code, meaningful only when `present` is
    /// [`SCIENCE_NULLABLE_PRESENT`].
    pub value: ScienceCode,
}

impl ScienceNullableCode {
    /// `Code?`'s `null`.
    pub const fn null() -> Self {
        Self { present: SCIENCE_NULLABLE_NULL, value: ScienceCode(0) }
    }
    /// `Code?`'s presence case.
    pub const fn some(value: ScienceCode) -> Self {
        Self { present: SCIENCE_NULLABLE_PRESENT, value }
    }
    fn get(self) -> Option<ScienceCode> {
        (self.present == SCIENCE_NULLABLE_PRESENT).then_some(self.value)
    }
}

/// `Grouping?`, [`ScienceNullableAlign`]'s shape.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScienceNullableGrouping {
    /// [`SCIENCE_NULLABLE_NULL`] or [`SCIENCE_NULLABLE_PRESENT`].
    pub present: u8,
    /// The grouping, meaningful only when `present` is
    /// [`SCIENCE_NULLABLE_PRESENT`].
    pub value: ScienceGrouping,
}

impl ScienceNullableGrouping {
    /// `Grouping?`'s `null`.
    pub const fn null() -> Self {
        Self { present: SCIENCE_NULLABLE_NULL, value: ScienceGrouping(0) }
    }
    /// `Grouping?`'s presence case.
    pub const fn some(value: ScienceGrouping) -> Self {
        Self { present: SCIENCE_NULLABLE_PRESENT, value }
    }
    fn get(self) -> Option<ScienceGrouping> {
        (self.present == SCIENCE_NULLABLE_PRESENT).then_some(self.value)
    }
}

/// `Int?`, for `width` and `precision`. `Int` has no niche either (crate
/// documentation §5.2), so this is the tagged form again: one byte, seven of
/// padding, then the eight-byte payload.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScienceNullableInt {
    /// [`SCIENCE_NULLABLE_NULL`] or [`SCIENCE_NULLABLE_PRESENT`].
    pub present: u8,
    /// The `Int`, meaningful only when `present` is
    /// [`SCIENCE_NULLABLE_PRESENT`].
    pub value: i64,
}

impl ScienceNullableInt {
    /// `Int?`'s `null`.
    pub const fn null() -> Self {
        Self { present: SCIENCE_NULLABLE_NULL, value: 0 }
    }
    /// `Int?`'s presence case.
    pub const fn some(value: i64) -> Self {
        Self { present: SCIENCE_NULLABLE_PRESENT, value }
    }
    fn get(self) -> Option<i64> {
        (self.present == SCIENCE_NULLABLE_PRESENT).then_some(self.value)
    }
}

/// Science's `FormatSpec`, §3.1's plain record: `{ fill: Char, align: Align?,
/// sign: Sign?, width: Int?, precision: Int?, code: Code?, alternate: Bool,
/// grouping: Grouping? }`, in that field order — an ordinary `#[repr(C)]`
/// aggregate and not an enum, [`crate::io::ScienceStringAndIoError`]'s shape
/// of thing rather than [`ScienceNullableIoError`]'s.
///
/// `fill` is a `u32` because `Char` is a Unicode scalar value end to end in
/// this crate — [`science_string_push_char`]'s own signature takes one — not
/// because this record chose a narrower representation than the language's.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ScienceFormatSpec {
    /// The fill character, as a Unicode scalar value. A space when the
    /// program wrote no spec at all (§2.3's default).
    pub fill: u32,
    /// `<`, `>` or `^`, or null for the type's own default (left for a
    /// string, right for a number).
    pub align: ScienceNullableAlign,
    /// `+`, `-` or `' '`, or null for `-`'s own behaviour.
    pub sign: ScienceNullableSign,
    /// The minimum field width, in characters, or null for none.
    pub width: ScienceNullableInt,
    /// The decimal precision or significant-figure count, or null for the
    /// code's own default.
    pub precision: ScienceNullableInt,
    /// `f e E g G % d x X o b s`, or null for the no-code default.
    pub code: ScienceNullableCode,
    /// `#`: alternate form. `false` when the program wrote no spec at all.
    pub alternate: bool,
    /// `,` or `_`, or null for no grouping (§2.3's default).
    pub grouping: ScienceNullableGrouping,
}

/// The spec `f"{x}"` carries when nothing after the `:` was written — §2.3's
/// "Defaults" section, as a value: fill is a space and nothing else is
/// present.
///
/// This is the only `FormatSpec` any Science program can produce today,
/// because §2's mini-language has no lexer yet. It exists so the entry
/// points below have something to be called with, ahead of the caller that
/// will one day build a non-default one.
#[no_mangle]
pub extern "C" fn science_format_spec_default() -> ScienceFormatSpec {
    ScienceFormatSpec {
        fill: ' ' as u32,
        align: ScienceNullableAlign::null(),
        sign: ScienceNullableSign::null(),
        width: ScienceNullableInt::null(),
        precision: ScienceNullableInt::null(),
        code: ScienceNullableCode::null(),
        alternate: false,
        grouping: ScienceNullableGrouping::null(),
    }
}

/// Science's `Formatter`: the sink `Display.display` writes into, plus the
/// spec it writes under. §3.1 gives it five methods and no fields a program
/// can name; this representation is this crate's own choice, not the
/// language's.
///
/// **Why the spec lives here and not beside the sink.** `into.spec()` is one
/// of the five methods, so a `display` implementation that branches on the
/// spec (§3.3's `Quantity`, choosing `.number()`'s rendering) needs to read
/// it off `self`. Storing it anywhere else would mean threading it through
/// every call by hand, which is exactly what a receiver exists to avoid.
///
/// **Not constructed by any entry point here.** Nothing allocates: `sink` is
/// a borrow of a `String` the caller already owns and `spec` is a plain
/// value, so building a `Formatter` is a field-init a code generator can do
/// directly, the same way it already builds [`crate::io::ScienceStringAndIoError`]
/// by hand rather than calling into this crate for it.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ScienceFormatter {
    /// Never null. Borrows a `ScienceString` the caller owns for the
    /// duration of the call.
    pub sink: *mut ScienceString,
    /// The spec this `Formatter` writes under. `Formatter.spec()` reads it
    /// back unchanged.
    pub spec: ScienceFormatSpec,
}

/// Renders `text` to `spec.width` characters using `spec.fill`, aligned per
/// `spec.align` or `default_align` when the spec names none — §2.3: *"Default
/// alignment is left for strings and anything `Display`, right for
/// numbers."* A spec with no width, or a width no wider than `text` already
/// is, is returned unchanged.
fn pad(text: String, spec: &ScienceFormatSpec, default_align: ScienceAlign) -> String {
    let Some(width) = spec.width.get() else { return text };
    let width = width.max(0);
    let len = text.chars().count() as i64;
    if width <= len {
        return text;
    }
    let gap = (width - len) as usize;
    let fill = char::from_u32(spec.fill).unwrap_or(' ');
    let filler = |n: usize| fill.to_string().repeat(n);
    match spec.align.get().unwrap_or(default_align) {
        ScienceAlign::LEFT => text + &filler(gap),
        ScienceAlign::CENTER => {
            let left = gap / 2;
            let right = gap - left;
            format!("{}{text}{}", filler(left), filler(right))
        }
        // `RIGHT`, and any value this predicate does not recognise — the
        // note's own default for a number, which is where an unrecognised
        // tag can only arrive from a bug elsewhere in this crate.
        _ => filler(gap) + &text,
    }
}

/// Inserts `sep` every three digits from the right of `digits`, which must be
/// ASCII decimal digits only — §2.2: *"`,` gives `1,234,567`… `_` gives
/// `1_234_567`."*
fn group(digits: &str, sep: char) -> String {
    let len = digits.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push(sep);
        }
        out.push(ch);
    }
    out
}

/// Groups the integer part of `text` (the part before a `.`, or all of it)
/// under `grouping`, leaving a fractional part untouched. A `None` grouping
/// is a no-op, which is §2.3's default: *"§2.3 gives no grouping by
/// default."*
fn apply_grouping(text: &str, grouping: Option<ScienceGrouping>) -> String {
    let Some(grouping) = grouping else { return text.to_string() };
    let sep = if grouping == ScienceGrouping::UNDERSCORE { '_' } else { ',' };
    match text.split_once('.') {
        Some((int_part, frac_part)) => format!("{}.{}", group(int_part, sep), frac_part),
        None => group(text, sep),
    }
}

/// The sign a rendering carries, under §2.1's three-way `sign` production.
/// `None` is `-`'s own behaviour: a sign only on the negative case.
fn sign_str(negative: bool, sign: Option<ScienceSign>) -> &'static str {
    match sign {
        Some(ScienceSign::PLUS) => {
            if negative {
                "-"
            } else {
                "+"
            }
        }
        Some(ScienceSign::SPACE) => {
            if negative {
                "-"
            } else {
                " "
            }
        }
        _ => {
            if negative {
                "-"
            } else {
                ""
            }
        }
    }
}

/// `f`, `x`/`X`, `o`, `b` and the no-code default for an integer — §2.2's
/// table, minus the width/alignment/sign this function's caller applies
/// afterwards. `magnitude` is already non-negative; the sign is
/// [`render_integer`]'s to add.
fn render_integer(value: i64, spec: &ScienceFormatSpec) -> String {
    let negative = value < 0;
    let magnitude = value.unsigned_abs();
    let code = spec.code.get();
    let (mut digits, prefix) = match code {
        Some(ScienceCode::HEX) => (format!("{magnitude:x}"), "0x"),
        Some(ScienceCode::HEX_UPPER) => (format!("{magnitude:X}"), "0x"),
        Some(ScienceCode::OCTAL) => (format!("{magnitude:o}"), "0o"),
        Some(ScienceCode::BINARY) => (format!("{magnitude:b}"), "0b"),
        _ => (format!("{magnitude}"), ""),
    };
    // §2.2's alternate-form prefixes are for the three non-decimal codes
    // only; grouping is decimal's, in the same table.
    if matches!(code, None | Some(ScienceCode::DECIMAL)) {
        digits = apply_grouping(&digits, spec.grouping.get());
    }
    let prefix = if spec.alternate { prefix } else { "" };
    let sign = sign_str(negative, spec.sign.get());
    pad(format!("{sign}{prefix}{digits}"), spec, ScienceAlign::RIGHT)
}

/// Trims trailing zeros after a decimal point, and the point itself if
/// nothing is left after it — `g`/`G`'s own behaviour, and not `f`/`e`'s,
/// which always show exactly the requested precision. A no-op when
/// `alternate` asks for the point to stay, or when there is no point to
/// trim.
fn trim_trailing_zeros(text: &str, alternate: bool) -> String {
    if alternate || !text.contains('.') {
        return text.to_string();
    }
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// `magnitude` at `precision` decimal places — `f`'s rendering, §2.2's row
/// for it. `#` at precision zero retains the point (§2.2: *"a retained
/// decimal point for `f`/`e`/`g` at precision zero"*).
fn render_fixed(magnitude: f64, precision: i64, alternate: bool) -> String {
    let precision = precision.max(0) as usize;
    let mut text = format!("{magnitude:.precision$}");
    if alternate && precision == 0 && !text.contains('.') {
        text.push('.');
    }
    text
}

/// `magnitude` in scientific notation with `precision` mantissa digits after
/// the point — `e`/`E`'s rendering. The exponent is always signed and at
/// least two digits (`e-07`, not `e-7`), matching §2.2's own worked example
/// literally: `f"{p:.2e}"` renders `4.51e-07`.
fn render_exp(magnitude: f64, precision: i64, upper: bool, alternate: bool) -> String {
    let precision = precision.max(0) as usize;
    let raw = format!("{magnitude:.precision$e}");
    let (mantissa, exp_str) = raw.split_once('e').expect("Rust's `{:e}` always writes an exponent");
    let exp: i32 = exp_str.parse().expect("Rust's exponent is a plain signed integer");
    let mut mantissa = mantissa.to_string();
    if alternate && precision == 0 && !mantissa.contains('.') {
        mantissa.push('.');
    }
    let exp_sign = if exp < 0 { '-' } else { '+' };
    let e = if upper { 'E' } else { 'e' };
    format!("{mantissa}{e}{exp_sign}{:02}", exp.abs())
}

/// `magnitude` at `sig` significant figures — `g`/`G`'s rendering, and the
/// code the note recommends for a scientist: *"three significant digits
/// whatever the magnitude."* Follows Python's own rule for choosing between
/// fixed and exponential form, because §2.1 adopts Python's mini-language
/// and does not list this choice among the four things it removes: fixed
/// when the decimal exponent is in `[-4, sig)`, exponential otherwise.
/// §2.2's own example, `f"{x:.3g}"` rendering `0.000451`, is exactly the
/// fixed branch at the boundary (exponent `-4`).
fn render_general(magnitude: f64, sig: i64, upper: bool, alternate: bool) -> String {
    let sig = sig.max(1);
    if magnitude == 0.0 {
        let text = render_fixed(0.0, sig - 1, alternate);
        return if alternate { text } else { trim_trailing_zeros(&text, false) };
    }
    let raw = format!("{magnitude:.*e}", (sig - 1) as usize);
    let (mantissa, exp_str) = raw.split_once('e').expect("Rust's `{:e}` always writes an exponent");
    let exp: i32 = exp_str.parse().expect("Rust's exponent is a plain signed integer");
    if exp < -4 || exp >= sig as i32 {
        let mut mantissa = if alternate { mantissa.to_string() } else { trim_trailing_zeros(mantissa, false) };
        if alternate && !mantissa.contains('.') {
            mantissa.push('.');
        }
        let exp_sign = if exp < 0 { '-' } else { '+' };
        let e = if upper { 'E' } else { 'e' };
        format!("{mantissa}{e}{exp_sign}{:02}", exp.abs())
    } else {
        let decimals = (sig as i32 - 1 - exp).max(0) as usize;
        let text = format!("{magnitude:.decimals$}");
        if alternate {
            if text.contains('.') { text } else { format!("{text}.") }
        } else {
            trim_trailing_zeros(&text, false)
        }
    }
}

/// §2.2's whole table for an `F64`, plus §2.3's no-code default and its three
/// special values. `spec.code` selects the row; everything else in `spec` is
/// applied uniformly afterwards — sign, then grouping (skipped for `e`/`E`,
/// where it would only ever touch a single mantissa digit and is never
/// requested in practice), then width and alignment.
fn render_f64(value: f64, spec: &ScienceFormatSpec) -> String {
    let negative = value.is_sign_negative();
    // §2.3: *"`NaN`, `inf`, `-inf`, spelled that way, honouring width and
    // alignment and ignoring precision."* Neither carries a sign the way an
    // ordinary number does; `NaN` has none at all, and `inf`'s is read off
    // the value itself, not manufactured by `sign_str` for a `NaN` that has
    // no notion of negative.
    if value.is_nan() {
        return pad("NaN".to_string(), spec, ScienceAlign::RIGHT);
    }
    if value.is_infinite() {
        let body = format!("{}inf", sign_str(negative, spec.sign.get()));
        return pad(body, spec, ScienceAlign::RIGHT);
    }

    let magnitude = value.abs();
    let precision = spec.precision.get();
    let code = spec.code.get();
    let body = match code {
        None => match precision {
            Some(p) => apply_grouping(&render_fixed(magnitude, p, spec.alternate), spec.grouping.get()),
            // §2.3's own default: the shortest decimal string that
            // round-trips, always carrying a point.
            None => apply_grouping(&format!("{magnitude:?}"), spec.grouping.get()),
        },
        Some(ScienceCode::FIXED) => {
            apply_grouping(&render_fixed(magnitude, precision.unwrap_or(6), spec.alternate), spec.grouping.get())
        }
        Some(ScienceCode::EXP) => render_exp(magnitude, precision.unwrap_or(6), false, spec.alternate),
        Some(ScienceCode::EXP_UPPER) => render_exp(magnitude, precision.unwrap_or(6), true, spec.alternate),
        Some(ScienceCode::GENERAL) => apply_grouping(
            &render_general(magnitude, precision.unwrap_or(6), false, spec.alternate),
            spec.grouping.get(),
        ),
        Some(ScienceCode::GENERAL_UPPER) => apply_grouping(
            &render_general(magnitude, precision.unwrap_or(6), true, spec.alternate),
            spec.grouping.get(),
        ),
        Some(ScienceCode::PERCENT) => {
            let grouped = apply_grouping(
                &render_fixed(magnitude * 100.0, precision.unwrap_or(6), spec.alternate),
                spec.grouping.get(),
            );
            format!("{grouped}%")
        }
        // A code this predicate does not recognise for a float — `d`, `x`,
        // `X`, `o`, `b`, `s` — is `science-types`' `SC0274` to refuse before
        // this is ever called; this crate does not re-check the table and
        // falls back to the no-code rendering rather than panicking on a
        // program that should not have compiled.
        _ => format!("{magnitude:?}"),
    };
    let sign = sign_str(negative, spec.sign.get());
    pad(format!("{sign}{body}"), spec, ScienceAlign::RIGHT)
}

/// `Formatter.text`: append `value`, honouring fill, alignment and width from
/// the spec — default alignment left, per §2.3.
///
/// # Safety
///
/// `formatter` must be a non-null, aligned pointer to a live
/// [`ScienceFormatter`] whose `sink` field is itself a non-null, aligned
/// pointer to a live [`ScienceString`]. `value` must be a non-null, aligned
/// pointer to a live `ScienceString`.
#[no_mangle]
pub unsafe extern "C" fn science_formatter_text(
    formatter: *mut ScienceFormatter,
    value: *const ScienceString,
) {
    // SAFETY: the caller guarantees a live `Formatter` and a live `value`.
    let spec = unsafe { (*formatter).spec };
    let text = unsafe { (*value).as_str() }.to_string();
    let padded = pad(text, &spec, ScienceAlign::LEFT);
    // SAFETY: the caller guarantees `formatter.sink` is a live `ScienceString`.
    let mut sink = Sink(unsafe { &mut *((*formatter).sink) });
    let _ = sink.write_str(&padded);
}

/// `Formatter.raw`: append `value` with no padding at all, for a fragment of
/// a larger rendering — §3.1's own words for the method.
///
/// # Safety
///
/// Same as [`science_formatter_text`].
#[no_mangle]
pub unsafe extern "C" fn science_formatter_raw(
    formatter: *mut ScienceFormatter,
    value: *const ScienceString,
) {
    // SAFETY: the caller guarantees a live `value`.
    let bytes = unsafe { (*value).bytes() };
    // SAFETY: the caller guarantees `formatter.sink` is a live `ScienceString`,
    // and `bytes` cannot alias it: `value` and `formatter.sink` are the
    // receiver's own two distinct parameters.
    unsafe { (*(*formatter).sink).append(bytes) };
}

/// `Formatter.number`: render an `F64` under the spec's code, precision,
/// sign, grouping and alternate flag (§2.2, §2.3), then pad it — §2.3:
/// numbers align right by default.
///
/// # Safety
///
/// `formatter` must be a non-null, aligned pointer to a live
/// [`ScienceFormatter`] whose `sink` field is itself a live `ScienceString`.
#[no_mangle]
pub unsafe extern "C" fn science_formatter_number(formatter: *mut ScienceFormatter, value: f64) {
    // SAFETY: the caller guarantees a live `Formatter`.
    let spec = unsafe { (*formatter).spec };
    let text = render_f64(value, &spec);
    // SAFETY: the caller guarantees `formatter.sink` is a live `ScienceString`.
    let mut sink = Sink(unsafe { &mut *((*formatter).sink) });
    let _ = sink.write_str(&text);
}

/// `Formatter.integer`: render an `I64` under the spec's code, sign and
/// grouping (§2.2), then pad it.
///
/// # Safety
///
/// Same as [`science_formatter_number`].
#[no_mangle]
pub unsafe extern "C" fn science_formatter_integer(formatter: *mut ScienceFormatter, value: i64) {
    // SAFETY: the caller guarantees a live `Formatter`.
    let spec = unsafe { (*formatter).spec };
    let text = render_integer(value, &spec);
    // SAFETY: the caller guarantees `formatter.sink` is a live `ScienceString`.
    let mut sink = Sink(unsafe { &mut *((*formatter).sink) });
    let _ = sink.write_str(&text);
}

/// `Formatter.spec`: the parsed spec, for an implementation that needs to
/// branch on it — §3.3's `Quantity`, choosing `.number()`'s rendering by
/// what `into.spec()` says, is the worked example.
///
/// Returned by value. [`ScienceFormatSpec`] is well past the three-word
/// aggregate threshold the crate documentation's §2 tracks, so a caller
/// emitting this as an LLVM call needs the `sret` convention, exactly like
/// [`crate::science_string_new`]. **`science-codegen`'s `RUNTIME` table does
/// not name this symbol**, and adding it there — along with
/// [`science_format_spec_default`], which is the same shape — is
/// `science-codegen`'s to do, not this crate's; nothing calls either yet.
///
/// # Safety
///
/// `formatter` must be a non-null, aligned pointer to a live
/// [`ScienceFormatter`].
#[no_mangle]
pub unsafe extern "C" fn science_formatter_spec(formatter: *const ScienceFormatter) -> ScienceFormatSpec {
    // SAFETY: the caller guarantees a live `Formatter`.
    unsafe { (*formatter).spec }
}
