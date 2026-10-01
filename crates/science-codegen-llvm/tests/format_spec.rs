//! `strings-formatting-and-docs.md` §2's format spec — `f"{x:>10.3f}"` —
//! **built, linked, run**, with stdout asserted byte for byte.
//!
//! # What this file closes
//!
//! Until this change the lexer refused every spec with `SC0173`, and
//! `science-rt`'s `Formatter` section rendered §2.2's whole table for a
//! caller that did not exist: its own note said *"what a Science program can
//! build today is still only `science_format_spec_default`"*. The caller is
//! `science-mir`'s `Builder::render_with_spec`, and this file is the
//! evidence that it reaches every row.
//!
//! # Why every expectation is a literal
//!
//! §2.1 says the mini-language *is* Python's, *"with four things removed and
//! one thing added"*, so every expected string below is what Python's
//! `format` prints for the same value and spec — except where §2.3 decides
//! otherwise (`NaN` and `inf` spelled that way, `-0.0` kept) or §2.2 adds
//! something Python does not have, and each of those is named where it is
//! asserted. A string computed by calling the runtime would only prove that
//! the runtime agrees with itself.
//!
//! # Why one program per concern, and not one per spec
//!
//! A program costs a link and a first launch, and under load the macOS
//! verification queue is the slowest thing in this suite. Each test is one
//! program printing one bracketed line per family, so a failure still names
//! the family and the diff still names the spec.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

/// What a program printed, having exited 0 with nothing on stderr. `-O2` for
/// `tests/interpolation.rs`'s reason: a `Formatter` is an `alloca` written by
/// one call and read by the next, which is `SROA`'s subject exactly.
fn prints(name: &str, source: &str) -> String {
    let dir = scratch("fmtspec", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// §2.1's precision, width, alignment and fill on a float: the line a
/// scientist writes first.
#[test]
fn a_float_takes_precision_width_alignment_and_fill() {
    let out = prints(
        "float_layout",
        "def main():\n\
         \x20   let x be 3.14159\n\
         \x20   print(f\"[{x:.2}] [{x:>8.3f}] [{x:<8.1f}] [{x:^9.2f}] [{x:*^9.2f}] [{x:.0f}] [{x:#.0f}]\")\n\
         \x20   print(f\"[{-3.5:08.2f}] [{1234567.891:,.2f}] [{1234567.891:_}] [{-0.5:+.0f}] [{2.5:.0f}]\")\n",
    );
    assert_eq!(
        out,
        "[3.14] [   3.142] [3.1     ] [  3.14   ] [**3.14***] [3] [3.]\n\
         [-0003.50] [1,234,567.89] [1_234_567.891] [-0] [2]\n"
    );
}

/// `e`, `E`, `g`, `G` and `%` — §2.2's rows — including §2.2's own two
/// worked examples, `0.000451` at `.3g` and `+12.34%`.
#[test]
fn the_float_codes_render_section_two_two_s_table() {
    let out = prints(
        "float_codes",
        "def main():\n\
         \x20   let g be 9.80665\n\
         \x20   print(f\"[{g:e}] [{g:.2e}] [{1e-7:.2E}] [{0.000451:.3g}] [{g:.3g}] [{g:g}] [{g:G}]\")\n\
         \x20   print(f\"[{123456789.0:.3g}] [{0.00001234:g}] [{0.1234:+.2%}] [{g:10.2%}]\")\n",
    );
    assert_eq!(
        out,
        "[9.806650e+00] [9.81e+00] [1.00E-07] [0.000451] [9.81] [9.80665] [9.80665]\n\
         [1.23e+08] [1.234e-05] [+12.34%] [   980.66%]\n"
    );
}

/// §2.3's special values: `NaN` and `inf` spelled that way, honouring width
/// and alignment and ignoring precision, and `-0.0` kept. Python would print
/// `nan`; §2.3 decides `NaN`.
#[test]
fn nan_infinity_and_negative_zero_keep_section_two_three_s_spelling() {
    let out = prints(
        "float_special",
        "def main():\n\
         \x20   let zero be 0.0\n\
         \x20   let nan be zero / zero\n\
         \x20   let inf be 1.0 / zero\n\
         \x20   let negzero be -0.0\n\
         \x20   print(f\"[{nan:>6}] [{nan:.2f}] [{inf:<6}] [{-inf:^8}] [{inf:+}] [{negzero:.1f}] [{zero:+.2f}]\")\n",
    );
    assert_eq!(out, "[   NaN] [NaN] [inf   ] [  -inf  ] [+inf] [-0.0] [+0.00]\n");
}

/// Sign, width and the `0` flag on integers. **The `0` flag is sign-aware**
/// — `{-7:04}` is `-007` — where a `0` *fill* with an explicit alignment
/// pads the whole rendering — `{-7:0>4}` is `00-7`. Both are Python's, and
/// §2.3's `f"{i:04d}"` → `0007` is the worked example.
#[test]
fn an_integer_takes_sign_width_and_the_zero_flag() {
    let out = prints(
        "int_layout",
        "def main():\n\
         \x20   let n be 42\n\
         \x20   let small: I8 be -5\n\
         \x20   print(f\"[{n:>8}] [{n:<8}] [{n:^8}] [{n:+}] [{-n:+}] [{n: }] [{n:*>6}]\")\n\
         \x20   print(f\"[{-7:04}] [{-7:0>4}] [{7:04d}] [{small:+04}] [{255:#06x}]\")\n",
    );
    assert_eq!(
        out,
        "[      42] [42      ] [   42   ] [+42] [-42] [ 42] [****42]\n\
         [-007] [00-7] [0007] [-005] [0x00ff]\n"
    );
}

/// §2.2's grouping: `,` and `_` every three decimal digits — and at the
/// extremes, where a cast through the wrong width would show: `i64::MIN`,
/// and `u64::MAX`, which through an `i64` would be `-1`.
#[test]
fn grouping_reaches_the_extremes_of_both_signednesses() {
    let out = prints(
        "grouping",
        "def main():\n\
         \x20   let big be 1234567\n\
         \x20   let top: U64 be 18446744073709551615\n\
         \x20   let wide: U32 be 4000000000\n\
         \x20   print(f\"[{big:,}] [{big:_}] [{-big:,}] [{big:>12,}] [{top:,}] [{wide:,}]\")\n\
         \x20   print(f\"[{-9223372036854775807 - 1:,}]\")\n",
    );
    assert_eq!(
        out,
        "[1,234,567] [1_234_567] [-1,234,567] [   1,234,567] [18,446,744,073,709,551,615] \
         [4,000,000,000]\n\
         [-9,223,372,036,854,775,808]\n"
    );
}

/// `x`, `X`, `o`, `b`, with `#`'s prefixes, on a narrow unsigned and on a
/// negative. Grouping a non-decimal base is by four digits, Python's
/// `0xffff_ffff` — `science-rt`'s `render_magnitude` says why.
#[test]
fn the_integer_bases_take_the_alternate_form_and_group_by_four() {
    let out = prints(
        "bases",
        "def main():\n\
         \x20   let n be 42\n\
         \x20   let byte: U8 be 200\n\
         \x20   let mask be 4294967295\n\
         \x20   print(f\"[{n:x}] [{n:#x}] [{n:X}] [{n:#X}] [{n:b}] [{n:#b}] [{n:o}] [{n:#o}]\")\n\
         \x20   print(f\"[{byte:#x}] [{byte:08b}] [{-255:#x}] [{mask:_x}] [{mask:#_b}]\")\n",
    );
    assert_eq!(
        out,
        "[2a] [0x2a] [2A] [0x2A] [101010] [0b101010] [52] [0o52]\n\
         [0xc8] [11001000] [-0xff] [ffff_ffff] [0b1111_1111_1111_1111_1111_1111_1111_1111]\n"
    );
}

/// §2.1 counts width in **characters**, not bytes: `μσ` is two characters
/// and four bytes, and padding it to five adds three. A fill may be any
/// character, a multi-byte one included, and a precision on a `String`
/// (§2.4's *"truncation"*) cuts characters, never a code point in half.
#[test]
fn width_fill_and_truncation_count_characters_not_bytes() {
    let out = prints(
        "unicode",
        "def main():\n\
         \x20   let greek be \"μσ\"\n\
         \x20   let s be \"abc\"\n\
         \x20   print(f\"[{greek:>5}] [{greek:·^6}] [{\"αβγδ\":.2}] [{greek:★<4}] [{'é':>3}]\")\n\
         \x20   print(f\"[{s:>6}] [{s:<6}|] [{s:^7}] [{s:.2}] [{s:>6.1}]\")\n",
    );
    assert_eq!(
        out,
        "[   μσ] [··μσ··] [αβ] [μσ★★] [  é]\n\
         [   abc] [abc   |] [  abc  ] [ab] [     a]\n"
    );
}

/// A `Bool`, a `Char`, an `F32` and a `TextError` — the holes that render
/// through a scratch `String` or an entry point of their own. `0.1f32` is
/// `0.1` at a width, not its `f64` widening `0.10000000149011612`: that is
/// what `science_formatter_number32` is for.
#[test]
fn the_other_displayable_prelude_types_take_width_and_alignment() {
    let out = prints(
        "others",
        "def main():\n\
         \x20   let f: F32 be 0.1\n\
         \x20   let n, err be \"12x\".parse_int()\n\
         \x20   print(f\"[{true:>6}] [{false:^7}] [{'z':*<3}] [{f:>10}] [{f:.3}] [{n:>3}]\")\n\
         \x20   if err?:\n\
         \x20       print(f\"[{err:>14}] [{err:-<14}]\")\n",
    );
    assert_eq!(
        out,
        "[  true] [ false ] [z**] [       0.1] [0.100] [  0]\n\
         [  not a number] [not a number--]\n"
    );
}

/// §3.1: a user type's `display` is handed the hole's spec. `into.text`
/// honours width and alignment by itself, and `into.spec()` lets an
/// implementation branch on the rest — the width, the fill, and whether a
/// spec was written at all. The plain hole beside them sees the default spec.
#[test]
fn a_user_display_reads_the_holes_spec() {
    let out = prints(
        "user_display",
        "type Station:\n\
         \x20   id: String\n\
         \x20   channels: Int\n\
         \n\
         Station implements Display:\n\
         \x20   def display(self, into: &mut Formatter):\n\
         \x20       let spec be into.spec()\n\
         \x20       let width be spec.width\n\
         \x20       if width?:\n\
         \x20           into.raw(f\"{self.id}/{self.channels} in {width}, fill {spec.fill}\")\n\
         \x20       else:\n\
         \x20           into.text(self.id)\n\
         \n\
         type Tag:\n\
         \x20   name: String\n\
         \n\
         Tag implements Display:\n\
         \x20   def display(self, into: &mut Formatter):\n\
         \x20       into.text(self.name)\n\
         \n\
         def main():\n\
         \x20   let s be Station(id: \"A12\", channels: 768)\n\
         \x20   print(f\"[{s}] [{s:*>12}]\")\n\
         \x20   let t be Tag(name: \"B7\")\n\
         \x20   print(f\"[{t:>6}] [{t:-<6}] [{t:^6}] [{t}]\")\n",
    );
    assert_eq!(
        out,
        "[A12] [A12/768 in 12, fill *]\n\
         [    B7] [B7----] [  B7  ] [B7]\n"
    );
}

/// A spec'd hole is still §1.6's borrow: the value is the program's
/// afterwards, a `String` included, and a spec'd `f"…"` bound to a name is a
/// `String` like any other.
#[test]
fn a_specd_hole_borrows_and_a_specd_literal_is_an_ordinary_string() {
    let out = prints(
        "ownership",
        "def main():\n\
         \x20   let name be \"station\"\n\
         \x20   let row be f\"{name:>10}|{3.0:.1f}\"\n\
         \x20   print(row)\n\
         \x20   print(name)\n\
         \x20   print(row.length())\n",
    );
    assert_eq!(out, "   station|3.0\nstation\n14\n");
}
