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
//! **What the seven do not decide, and what the section below them now does.**
//! None of the seven is a Science name, a method, or an interface: they are
//! entry points a code generator calls, in the same class as
//! [`crate::science_string_from_bytes`], which is how a string literal becomes
//! a value and is likewise not a Science spelling of anything.
//!
//! The language-level question — *what does `Display` require?* — used to be
//! open here, and this paragraph used to say so: *"`science-resolve`'s
//! `builtins` has refused three times to write `Display.display(Formatter)`,
//! because naming that method would invent a `Formatter`."* **That is no
//! longer true and was wrong when it was written.**
//! `strings-formatting-and-docs.md` §3.1 specifies `Formatter` completely —
//! five methods, a `FormatSpec` record and four `choice` types — so writing
//! the signature was transcription and not invention. `builtins.rs` now
//! transcribes it, and the `Formatter` section at the bottom of this file is
//! its runtime side: the representation, and the five methods §3.1 promises.
//!
//! # The cost
//!
//! **The set is closed at the primitives, and a user type leaves it through
//! the `Formatter` section below.** `I8`…`I64` all arrive here as
//! [`science_string_push_i64`] after
//! `science-mir` sign-extends them with an `Rvalue::Cast`, `U8`…`U64` as
//! [`science_string_push_u64`] after it zero-extends them, and
//! `F16` and `BF16` arrive as nothing at all: there is no Rust primitive to
//! render them through and no note that says what their shortest round-trip
//! spelling is. A user type that implements `Display` has no entry point among
//! the seven and never will: rendering it means calling its own `display`, and
//! what that call needs is not an eighth push but a sink to write into —
//! [`ScienceFormatter`], below.
//!
//! **None of the seven honours a format specification, and none needs to.**
//! They are §2.3's defaults — the no-spec case. A hole that writes a spec
//! (`f"{x:>10.3f}"`, §2.1) does not reach them: `science-mir` routes it
//! through the `Formatter` section below, initialised with the hole's spec by
//! [`science_formatter_init_spec`], so §2.2's table is rendered in one place.
//! That section was written to honour a spec before any program could write
//! one, because §3.1 hands one to every `display` through `into.spec()`; the
//! lexer has since learned to read them, and the representation did not have
//! to change.
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
//!
//! **The `Formatter` section moves both counts, and the same derivation says
//! by how much.** Seven entry points join — [`science_formatter_init`],
//! [`science_formatter_text`], [`science_formatter_raw`],
//! [`science_formatter_number`], [`science_formatter_integer`],
//! [`science_formatter_spec`] and [`science_format_spec_default`] — taking the
//! table from 59 to 66. Five of the seven return `()` or take their result
//! through an out-pointer and so stay off the `sret` list; the two that hand
//! back a [`ScienceFormatSpec`] by value join it, because that record is far
//! past the two-word threshold and is MEMORY on both conventions. Ten becomes
//! twelve. [`science_formatter_init`] is the out-pointer one and it is the
//! reason the other five did not have to join: a `science_formatter_new(sink)
//! -> ScienceFormatter` would have been the thirteenth, for a value the caller
//! has a stack slot for either way.

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
// **Four of these are now reachable from a Science program**, which the
// first draft of this section said none of them was. `science-mir` lowers a
// `print` of a type that implements `Display` to an accumulator, a
// [`science_formatter_init`] and a call to the user's own `display`;
// `science-codegen-llvm`'s `PRELUDE_METHODS` maps `Formatter.text`, `.raw`,
// `.number` and `.integer` to the four entry points below.
// `examples/06_traits.science` runs the path end to end.
//
// **And now all of them are, with a spec.** The lexer reads §2.1's grammar,
// `science-types` checks it against the hole's type (`SC0274`), and
// `science-mir` builds a spec'd hole's `Formatter` with
// [`science_formatter_init_spec`] and renders through `number`, `integer`,
// `text` — or the two this section added for the types a widening cast
// would get wrong, [`science_formatter_unsigned`] and
// [`science_formatter_number32`] — or through the user's own `display`, which
// reads the spec back with `into.spec()`. `science-codegen-llvm`'s
// `tests/format_spec.rs` runs each of them. The rendering rules below are read out
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
/// What [`science_formatter_init`] writes, for a hole with no spec — a
/// `print(v)` of a user type, or `f"{v}"`. A hole that writes a spec gets
/// [`science_formatter_init_spec`]'s instead.
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

/// Writes a default-spec [`ScienceFormatter`] over `sink` into `formatter`.
///
/// **This is how a `Formatter` comes into existence, and it is an
/// out-pointer rather than a return.** `science-mir` lowers `print(v)` on a
/// type that implements `Display` to an accumulator, this call, and then the
/// user's own `display`; the accumulator is a `String` local the caller
/// already has, and the `Formatter` is a second local it already has a slot
/// for. Returning a [`ScienceFormatter`] by value instead would be an `sret`
/// — the struct is well past the two-word threshold — for a copy into a slot
/// that is already there, which is the crate documentation's §2 list growing
/// for nothing.
///
/// **The spec is [`science_format_spec_default`]'s and not a parameter.**
/// This used to say that a spec, when the lexer could read one, would arrive
/// as a parameter here. It arrived as a sibling instead,
/// [`science_formatter_init_spec`], so that the no-spec hole — every `print`
/// of a user type — keeps its two-argument call and the spec'd one pays for
/// four more scalars only where a spec was written.
///
/// # Safety
///
/// `formatter` must be a non-null, aligned pointer to writable storage the
/// size and alignment of a [`ScienceFormatter`]; it need not be initialised.
/// `sink` must be a non-null, aligned pointer to a live [`ScienceString`]
/// that outlives every use of `*formatter`.
#[no_mangle]
pub unsafe extern "C" fn science_formatter_init(
    formatter: *mut ScienceFormatter,
    sink: *mut ScienceString,
) {
    // SAFETY: the caller guarantees writable, aligned storage.
    unsafe { formatter.write(ScienceFormatter { sink, spec: science_format_spec_default() }) };
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
    group_by(digits, sep, 3)
}

/// [`group`] at any stride: three for decimal, four for the other bases.
fn group_by(digits: &str, sep: char, stride: usize) -> String {
    let len = digits.len();
    let mut out = String::with_capacity(len + len / stride);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (len - i) % stride == 0 {
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

/// `d`, `x`/`X`, `o`, `b` and the no-code default for a signed integer —
/// §2.2's table — with the sign, grouping, width and alignment applied.
fn render_integer(value: i64, spec: &ScienceFormatSpec) -> String {
    render_magnitude(value < 0, value.unsigned_abs(), spec)
}

/// [`render_integer`] for a value already split into a sign and a
/// magnitude, which is what lets a `U64` above `i64::MAX` share it:
/// [`science_formatter_unsigned`] passes `negative: false` and the whole
/// value.
///
/// **Grouping a non-decimal base is by four digits, not three.** §2.2 gives
/// `,` and `_` for *"`1,234,567`"* and says nothing about `x`/`o`/`b`, and
/// §2.4 makes grouping legal whenever *"the code is numeric"* — which those
/// three are. Python groups them by four with `_` (`0xffff_ffff`), because a
/// hex digit is half a byte and four of them are a 16-bit word; three would
/// be a grouping no reader of a mask has ever wanted. `,` gets the same
/// four, rather than Python's refusal, because §2.4 does not refuse it.
fn render_magnitude(negative: bool, magnitude: u64, spec: &ScienceFormatSpec) -> String {
    let code = spec.code.get();
    let (mut digits, prefix) = match code {
        Some(ScienceCode::HEX) => (format!("{magnitude:x}"), "0x"),
        Some(ScienceCode::HEX_UPPER) => (format!("{magnitude:X}"), "0x"),
        Some(ScienceCode::OCTAL) => (format!("{magnitude:o}"), "0o"),
        Some(ScienceCode::BINARY) => (format!("{magnitude:b}"), "0b"),
        _ => (format!("{magnitude}"), ""),
    };
    // §2.2's alternate-form prefixes are for the three non-decimal codes
    // only.
    if matches!(code, None | Some(ScienceCode::DECIMAL) | Some(ScienceCode::STR)) {
        digits = apply_grouping(&digits, spec.grouping.get());
    } else if let Some(grouping) = spec.grouping.get() {
        let sep = if grouping == ScienceGrouping::UNDERSCORE { '_' } else { ',' };
        digits = group_by(&digits, sep, 4);
    }
    let prefix = if spec.alternate { prefix } else { "" };
    let sign = sign_str(negative, spec.sign.get());
    pad_number(&format!("{sign}{prefix}"), &digits, spec)
}

/// The `0` flag's padding: §2.3's *"`0` before the width means zero-fill"*,
/// placed **after** the sign and any `0x` prefix, so `{-7:04}` is `-007` and
/// `{255:#06x}` is `0x00ff`. Anything else is [`pad`] over the whole
/// rendering, aligned right by default.
///
/// **How a `0` flag is told from a `0` fill.** §3.1's `FormatSpec` has no
/// field for the flag, so the compiler spells it as `fill: '0'` with
/// `align` left null — a combination §2.1's grammar cannot produce any other
/// way, because a written fill always comes with a written alignment. So
/// `{-7:0>4}` (fill `0`, align `>`) pads the whole string to `00-7`, as
/// Python does, and only the flag is sign-aware.
fn pad_number(lead: &str, body: &str, spec: &ScienceFormatSpec) -> String {
    let zero_flag = spec.fill == '0' as u32 && spec.align.get().is_none();
    match spec.width.get() {
        Some(width) if zero_flag => {
            let len = (lead.chars().count() + body.chars().count()) as i64;
            let gap = (width - len).max(0) as usize;
            format!("{lead}{}{body}", "0".repeat(gap))
        }
        _ => pad(format!("{lead}{body}"), spec, ScienceAlign::RIGHT),
    }
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
    render_float(value, spec, |magnitude| format!("{magnitude:?}"))
}

/// [`render_f64`] with the no-code, no-precision spelling supplied by the
/// caller — `f64`'s shortest round-trip, or `f32`'s. Every explicit code and
/// every precision renders through `f64` either way, because widening an
/// `f32` is exact and `.3f` of the wide value is `.3f` of the narrow one.
fn render_float(value: f64, spec: &ScienceFormatSpec, shortest: impl Fn(f64) -> String) -> String {
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
            // round-trips, always carrying a point. Rust spells a very large
            // or very small one with an exponent (`1e20`), and grouping the
            // digits of a mantissa is not grouping a number, so that spelling
            // is left ungrouped.
            None => {
                let text = shortest(magnitude);
                if text.contains('e') { text } else { apply_grouping(&text, spec.grouping.get()) }
            }
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
        // `s` on a float is §2.2's *"anything `Display`"*: the default
        // rendering. A code this predicate does not recognise for a float —
        // `d`, `x`, `X`, `o`, `b` — is `science-types`' `SC0274` to refuse
        // before this is ever called; this crate does not re-check the table
        // and falls back to the no-code rendering rather than panicking on a
        // program that should not have compiled.
        _ => shortest(magnitude),
    };
    let sign = sign_str(negative, spec.sign.get());
    pad_number(sign, &body, spec)
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
    let text = unsafe { (*value).as_str() };
    // §2.4: a precision on a `String` is truncation, in characters — the
    // same unit the width is counted in.
    let text = match spec.precision.get() {
        Some(p) => text.chars().take(p.max(0) as usize).collect(),
        None => text.to_string(),
    };
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

/// `Formatter.integer` for a `U64`: [`science_formatter_integer`] over a
/// value an `i64` cannot hold.
///
/// **An entry point of its own, and not a cast to `I64`**, because the cast
/// is wrong for exactly the values grouping exists for: `u64::MAX` is
/// `18,446,744,073,709,551,615`, and through an `i64` it is `-1`. `U8`,
/// `U16` and `U32` do not need it — every value they hold is an `i64` — and
/// `science-mir` widens them to `I64` for [`science_formatter_integer`].
///
/// Not a Science method. §3.1's `Formatter.integer` takes an `I64` and
/// nothing here changes that: this is reached only from a spec'd `f"…"`
/// hole of type `U64`, which `science-mir` lowers to it directly.
///
/// # Safety
///
/// Same as [`science_formatter_number`].
#[no_mangle]
pub unsafe extern "C" fn science_formatter_unsigned(formatter: *mut ScienceFormatter, value: u64) {
    // SAFETY: the caller guarantees a live `Formatter`.
    let spec = unsafe { (*formatter).spec };
    let text = render_magnitude(false, value, &spec);
    // SAFETY: the caller guarantees `formatter.sink` is a live `ScienceString`.
    let mut sink = Sink(unsafe { &mut *((*formatter).sink) });
    let _ = sink.write_str(&text);
}

/// `Formatter.number` for an `F32`, whose shortest round-trip spelling is
/// not its `f64` widening's.
///
/// **The one case it exists for** is a spec with a width and no precision —
/// `f"{x:>10}"` on an `F32` — where §2.3's default applies, and the default
/// is *"the shortest decimal string that round-trips"* **for the value's own
/// type**: `0.1f32` is `0.1`, and widened it is `0.10000000149011612`. Every
/// other spec renders the widened value, which is exact.
///
/// # Safety
///
/// Same as [`science_formatter_number`].
#[no_mangle]
pub unsafe extern "C" fn science_formatter_number32(formatter: *mut ScienceFormatter, value: f32) {
    // SAFETY: the caller guarantees a live `Formatter`.
    let spec = unsafe { (*formatter).spec };
    let text = render_float(f64::from(value), &spec, |magnitude| format!("{:?}", magnitude as f32));
    // SAFETY: the caller guarantees `formatter.sink` is a live `ScienceString`.
    let mut sink = Sink(unsafe { &mut *((*formatter).sink) });
    let _ = sink.write_str(&text);
}

/// The bits of [`science_formatter_init_spec`]'s `flags` word. Each field is
/// zero when the spec left it out, and otherwise its runtime tag plus one.
pub mod spec_flags {
    /// `align`: bits 0–1, [`super::ScienceAlign`]'s tag plus one.
    pub const ALIGN_SHIFT: u32 = 0;
    /// `sign`: bits 2–3, [`super::ScienceSign`]'s tag plus one.
    pub const SIGN_SHIFT: u32 = 2;
    /// `code`: bits 4–7, [`super::ScienceCode`]'s tag plus one.
    pub const CODE_SHIFT: u32 = 4;
    /// `alternate`: bit 8.
    pub const ALTERNATE: i64 = 1 << 8;
    /// `grouping`: bits 9–10, [`super::ScienceGrouping`]'s tag plus one.
    pub const GROUPING_SHIFT: u32 = 9;
}

/// Writes a [`ScienceFormatter`] over `sink` with the spec a hole wrote —
/// [`science_formatter_init`] for `f"{x:>10.3f}"`.
///
/// # The decision
///
/// **The spec crosses as four scalars, not as a `FormatSpec`.** `fill` is
/// the fill character; `flags` packs the four nullable `choice` fields and
/// `alternate` per [`spec_flags`]; `width` and `precision` are the numbers,
/// or `-1` for absent.
///
/// # The reason
///
/// A hole's spec is a compile-time constant, and the cheapest constant a
/// code generator can pass is an integer in a register. Passing a
/// [`ScienceFormatSpec`] instead would mean `science-mir` building a
/// forty-byte aggregate of four nullable `choice` values field by field —
/// MIR for a record no program wrote, for a value that never changes — or
/// a private global the MIR has no operand to name. Four integers are four
/// `Constant::Count`s.
///
/// # The cost
///
/// **The packing is a second spelling of the record**, and it is this
/// crate's alone: [`spec_flags`] is the one table both sides read, and
/// `tests/format.rs` round-trips it against the record. And §2.5's dynamic
/// width, when it is built, arrives as a run-time `width` here rather than
/// a constant — which is the other reason the number is its own parameter
/// and not a bit field.
///
/// The `0` flag is not a parameter: the compiler passes it as `fill: '0'`
/// with no alignment, which [`pad_number`] reads as the flag.
///
/// # Safety
///
/// Same as [`science_formatter_init`].
#[no_mangle]
pub unsafe extern "C" fn science_formatter_init_spec(
    formatter: *mut ScienceFormatter,
    sink: *mut ScienceString,
    fill: u32,
    flags: i64,
    width: i64,
    precision: i64,
) {
    let field = |shift: u32, mask: i64| ((flags >> shift) & mask) as u8;
    let align = match field(spec_flags::ALIGN_SHIFT, 0b11) {
        0 => ScienceNullableAlign::null(),
        tag => ScienceNullableAlign::some(ScienceAlign(tag - 1)),
    };
    let sign = match field(spec_flags::SIGN_SHIFT, 0b11) {
        0 => ScienceNullableSign::null(),
        tag => ScienceNullableSign::some(ScienceSign(tag - 1)),
    };
    let code = match field(spec_flags::CODE_SHIFT, 0b1111) {
        0 => ScienceNullableCode::null(),
        tag => ScienceNullableCode::some(ScienceCode(tag - 1)),
    };
    let grouping = match field(spec_flags::GROUPING_SHIFT, 0b11) {
        0 => ScienceNullableGrouping::null(),
        tag => ScienceNullableGrouping::some(ScienceGrouping(tag - 1)),
    };
    let number = |n: i64| if n < 0 { ScienceNullableInt::null() } else { ScienceNullableInt::some(n) };
    let spec = ScienceFormatSpec {
        fill: if char::from_u32(fill).is_some() { fill } else { ' ' as u32 },
        align,
        sign,
        width: number(width),
        precision: number(precision),
        code,
        alternate: flags & spec_flags::ALTERNATE != 0,
        grouping,
    };
    // SAFETY: the caller guarantees writable, aligned storage.
    unsafe { formatter.write(ScienceFormatter { sink, spec }) };
}

/// `Formatter.spec`: the parsed spec, for an implementation that needs to
/// branch on it — §3.3's `Quantity`, choosing `.number()`'s rendering by
/// what `into.spec()` says, is the worked example.
///
/// Returned by value. [`ScienceFormatSpec`] is well past the three-word
/// aggregate threshold the crate documentation's §2 tracks, so a caller
/// emitting this as an LLVM call needs the `sret` convention, exactly like
/// [`crate::science_string_new`]. `science-codegen`'s `RUNTIME` names it —
/// this and [`science_format_spec_default`] are the two entries that took
/// that list from ten `sret` returns to twelve.
///
/// **Reached by `into.spec()`**, through `science-codegen-llvm`'s
/// `PRELUDE_METHODS`. It used to have no row there, by that table's own rule
/// that a row arrives with a program that runs it, and nothing could branch
/// on a spec while the lexer refused every one. A hole's spec now reaches
/// `display` through [`science_formatter_init_spec`], and
/// `tests/format_spec.rs`'s `a_user_display_reads_the_holes_spec` reads its
/// width and fill back through this.
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

// --- `Display.display` on a prelude type ------------------------------------
//
// A generic `def show[T](x: &T) where T: Display` renders its hole through the
// interface's `display`, and `science-codegen`'s monomorphiser redirects that
// call to the concrete `Self`. A prelude type has no Science body to redirect
// to, so each one gets an entry point of the same shape a user's `display` has
// — `(self: &T, into: &mut Formatter)` — and `science-codegen-llvm`'s
// `PRELUDE_METHODS` maps `(T, "display")` to it. Each is the `Formatter` entry
// point for the type with the value read through the pointer, so a spec on the
// hole is honoured exactly as it is for a concrete one.

// Each is written out rather than generated by a macro: `tests/symbols.rs`
// reads every `#[no_mangle]` name from this source to check it against
// `RUNTIME`, and a name a macro produces is a name it cannot read.

/// `Display.display` for `i8`: read the value and hand it to `science_formatter_integer`.
///
/// # Safety
///
/// `value` must point to a live `i8`, `formatter` to a live
/// [`ScienceFormatter`].
#[no_mangle]
pub unsafe extern "C" fn science_display_i8(value: *const i8, formatter: *mut ScienceFormatter) {
    // SAFETY: the caller guarantees both pointers.
    unsafe { science_formatter_integer(formatter, *value as i64) }
}

/// `Display.display` for `i16`: read the value and hand it to `science_formatter_integer`.
///
/// # Safety
///
/// `value` must point to a live `i16`, `formatter` to a live
/// [`ScienceFormatter`].
#[no_mangle]
pub unsafe extern "C" fn science_display_i16(value: *const i16, formatter: *mut ScienceFormatter) {
    // SAFETY: the caller guarantees both pointers.
    unsafe { science_formatter_integer(formatter, *value as i64) }
}

/// `Display.display` for `i32`: read the value and hand it to `science_formatter_integer`.
///
/// # Safety
///
/// `value` must point to a live `i32`, `formatter` to a live
/// [`ScienceFormatter`].
#[no_mangle]
pub unsafe extern "C" fn science_display_i32(value: *const i32, formatter: *mut ScienceFormatter) {
    // SAFETY: the caller guarantees both pointers.
    unsafe { science_formatter_integer(formatter, *value as i64) }
}

/// `Display.display` for `i64`: read the value and hand it to `science_formatter_integer`.
///
/// # Safety
///
/// `value` must point to a live `i64`, `formatter` to a live
/// [`ScienceFormatter`].
#[no_mangle]
pub unsafe extern "C" fn science_display_i64(value: *const i64, formatter: *mut ScienceFormatter) {
    // SAFETY: the caller guarantees both pointers.
    unsafe { science_formatter_integer(formatter, *value as i64) }
}

/// `Display.display` for `u8`: read the value and hand it to `science_formatter_unsigned`.
///
/// # Safety
///
/// `value` must point to a live `u8`, `formatter` to a live
/// [`ScienceFormatter`].
#[no_mangle]
pub unsafe extern "C" fn science_display_u8(value: *const u8, formatter: *mut ScienceFormatter) {
    // SAFETY: the caller guarantees both pointers.
    unsafe { science_formatter_unsigned(formatter, *value as u64) }
}

/// `Display.display` for `u16`: read the value and hand it to `science_formatter_unsigned`.
///
/// # Safety
///
/// `value` must point to a live `u16`, `formatter` to a live
/// [`ScienceFormatter`].
#[no_mangle]
pub unsafe extern "C" fn science_display_u16(value: *const u16, formatter: *mut ScienceFormatter) {
    // SAFETY: the caller guarantees both pointers.
    unsafe { science_formatter_unsigned(formatter, *value as u64) }
}

/// `Display.display` for `u32`: read the value and hand it to `science_formatter_unsigned`.
///
/// # Safety
///
/// `value` must point to a live `u32`, `formatter` to a live
/// [`ScienceFormatter`].
#[no_mangle]
pub unsafe extern "C" fn science_display_u32(value: *const u32, formatter: *mut ScienceFormatter) {
    // SAFETY: the caller guarantees both pointers.
    unsafe { science_formatter_unsigned(formatter, *value as u64) }
}

/// `Display.display` for `u64`: read the value and hand it to `science_formatter_unsigned`.
///
/// # Safety
///
/// `value` must point to a live `u64`, `formatter` to a live
/// [`ScienceFormatter`].
#[no_mangle]
pub unsafe extern "C" fn science_display_u64(value: *const u64, formatter: *mut ScienceFormatter) {
    // SAFETY: the caller guarantees both pointers.
    unsafe { science_formatter_unsigned(formatter, *value as u64) }
}

/// `Display.display` for `f32`: read the value and hand it to `science_formatter_number32`.
///
/// # Safety
///
/// `value` must point to a live `f32`, `formatter` to a live
/// [`ScienceFormatter`].
#[no_mangle]
pub unsafe extern "C" fn science_display_f32(value: *const f32, formatter: *mut ScienceFormatter) {
    // SAFETY: the caller guarantees both pointers.
    unsafe { science_formatter_number32(formatter, *value as f32) }
}

/// `Display.display` for `f64`: read the value and hand it to `science_formatter_number`.
///
/// # Safety
///
/// `value` must point to a live `f64`, `formatter` to a live
/// [`ScienceFormatter`].
#[no_mangle]
pub unsafe extern "C" fn science_display_f64(value: *const f64, formatter: *mut ScienceFormatter) {
    // SAFETY: the caller guarantees both pointers.
    unsafe { science_formatter_number(formatter, *value as f64) }
}

/// Pad `text` under the formatter's spec and append it: what `Formatter.text`
/// does for a value that is not already a `ScienceString`.
///
/// # Safety
///
/// `formatter` must be a live [`ScienceFormatter`] with a live sink.
unsafe fn display_text(formatter: *mut ScienceFormatter, text: String) {
    // SAFETY: the caller guarantees a live `Formatter`.
    let spec = unsafe { (*formatter).spec };
    let padded = pad(text, &spec, ScienceAlign::LEFT);
    // SAFETY: the caller guarantees `formatter.sink` is a live `ScienceString`.
    let mut sink = Sink(unsafe { &mut *((*formatter).sink) });
    let _ = sink.write_str(&padded);
}

/// `Display.display` for `Bool`: `true` or `false`, padded.
///
/// # Safety
///
/// As [`science_display_i64`].
#[no_mangle]
pub unsafe extern "C" fn science_display_bool(value: *const bool, formatter: *mut ScienceFormatter) {
    // SAFETY: the caller guarantees both pointers.
    unsafe { display_text(formatter, (*value).to_string()) }
}

/// `Display.display` for `Char`: the character itself, padded.
///
/// # Safety
///
/// As [`science_display_i64`].
#[no_mangle]
pub unsafe extern "C" fn science_display_char(value: *const u32, formatter: *mut ScienceFormatter) {
    // SAFETY: the caller guarantees both pointers.
    let character = char::from_u32(unsafe { *value }).unwrap_or(char::REPLACEMENT_CHARACTER);
    // SAFETY: as above.
    unsafe { display_text(formatter, character.to_string()) }
}

/// `Display.display` for `String`: [`science_formatter_text`] with the
/// arguments in `display`'s order.
///
/// # Safety
///
/// As [`science_display_i64`], with `value` a live [`ScienceString`].
#[no_mangle]
pub unsafe extern "C" fn science_display_str(
    value: *const ScienceString,
    formatter: *mut ScienceFormatter,
) {
    // SAFETY: the caller guarantees both pointers.
    unsafe { science_formatter_text(formatter, value) }
}

/// `Display.display` for `IoError`: its sentence, padded.
///
/// # Safety
///
/// As [`science_display_i64`], with `value` a live `IoError`.
#[no_mangle]
pub unsafe extern "C" fn science_display_io_error(
    value: *const crate::io::ScienceIoError,
    formatter: *mut ScienceFormatter,
) {
    // SAFETY: the caller guarantees both pointers.
    unsafe { display_text(formatter, crate::io::io_error_sentence(*value).to_string()) }
}

/// `Display.display` for `TextError`: its sentence, padded.
///
/// # Safety
///
/// As [`science_display_i64`], with `value` a live `TextError`.
#[no_mangle]
pub unsafe extern "C" fn science_display_text_error(
    value: *const crate::text::ScienceTextError,
    formatter: *mut ScienceFormatter,
) {
    // SAFETY: the caller guarantees both pointers.
    unsafe { display_text(formatter, crate::text::text_error_sentence(*value).to_string()) }
}
