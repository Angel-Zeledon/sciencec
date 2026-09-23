//! `format.rs`: the seven entry points an `f"…"` is built out of, and §2.3's
//! defaults, which are the part a note can be wrong about and a test cannot.

mod common;

use common::{as_str, free, s};
use science_rt::*;

/// Renders one value into a fresh `String` and hands back the text.
///
/// Every test below goes through this rather than through `format!`, because
/// the thing under test is *what the runtime appends*, and a helper that
/// formatted the expectation the same way would prove nothing.
fn rendered(render: impl FnOnce(*mut ScienceString)) -> String {
    let mut out = science_string_new();
    render(&mut out);
    let text = as_str(&out).to_string();
    free(out);
    text
}

// --- integers ------------------------------------------------------------

#[test]
fn an_i64_renders_in_base_ten() {
    assert_eq!(rendered(|s| unsafe { science_string_push_i64(s, 42) }), "42");
    assert_eq!(rendered(|s| unsafe { science_string_push_i64(s, 0) }), "0");
    assert_eq!(rendered(|s| unsafe { science_string_push_i64(s, -7) }), "-7");
}

/// §2.3 gives no grouping by default. A number that came out with separators
/// in it would not parse back.
#[test]
fn an_integer_carries_no_grouping() {
    assert_eq!(rendered(|s| unsafe { science_string_push_i64(s, 1_234_567) }), "1234567");
}

#[test]
fn the_edges_of_i64_and_u64_survive() {
    assert_eq!(
        rendered(|s| unsafe { science_string_push_i64(s, i64::MIN) }),
        "-9223372036854775808"
    );
    assert_eq!(
        rendered(|s| unsafe { science_string_push_u64(s, u64::MAX) }),
        "18446744073709551615"
    );
}

// --- floats, which is where §2.3 makes its decisions ---------------------

/// §2.3's worked example, and the whole argument for shortest-round-trip:
/// *"a default that silently rounds to six places hides exactly the bug the
/// user needed to see"*.
#[test]
fn a_float_prints_the_shortest_string_that_round_trips() {
    assert_eq!(
        rendered(|s| unsafe { science_string_push_f64(s, 0.1 + 0.2) }),
        "0.30000000000000004"
    );
    assert_eq!(rendered(|s| unsafe { science_string_push_f64(s, 1.0 / 3.0) }), "0.3333333333333333");
}

/// The half §2.3 leaves open and `science_string_push_f64` decides: a float
/// always carries a point, so a column of `F64` is not mistaken for integers.
#[test]
fn a_whole_float_still_carries_its_point() {
    assert_eq!(rendered(|s| unsafe { science_string_push_f64(s, 1.0) }), "1.0");
    assert_eq!(rendered(|s| unsafe { science_string_push_f64(s, -3.0) }), "-3.0");
    assert_eq!(rendered(|s| unsafe { science_string_push_f32(s, 2.0) }), "2.0");
}

/// §2.3: *"`NaN`, `inf`, `-inf`, spelled that way … Not `nan`."*
#[test]
fn the_three_special_values_are_spelled_as_section_two_three_says() {
    assert_eq!(rendered(|s| unsafe { science_string_push_f64(s, f64::NAN) }), "NaN");
    assert_eq!(rendered(|s| unsafe { science_string_push_f64(s, f64::INFINITY) }), "inf");
    assert_eq!(rendered(|s| unsafe { science_string_push_f64(s, f64::NEG_INFINITY) }), "-inf");
}

/// §2.3: *"`-0.0` prints as `-0.0`. It is a different float and hiding that
/// has cost people days."*
#[test]
fn negative_zero_is_visible() {
    assert_eq!(rendered(|s| unsafe { science_string_push_f64(s, -0.0) }), "-0.0");
    assert_eq!(rendered(|s| unsafe { science_string_push_f64(s, 0.0) }), "0.0");
}

/// The reason `F32` has an entry point of its own rather than widening: the
/// shortest string that round-trips is a property of the width.
#[test]
fn an_f32_is_not_rendered_as_the_f64_it_widens_to() {
    assert_eq!(rendered(|s| unsafe { science_string_push_f32(s, 0.1) }), "0.1");
    assert_eq!(
        rendered(|s| unsafe { science_string_push_f64(s, 0.1f32 as f64) }),
        "0.10000000149011612"
    );
}

// --- bool and char -------------------------------------------------------

#[test]
fn a_bool_prints_as_its_literal() {
    assert_eq!(rendered(|s| unsafe { science_string_push_bool(s, true) }), "true");
    assert_eq!(rendered(|s| unsafe { science_string_push_bool(s, false) }), "false");
}

/// §3.2: a value rendered for a user has no quotes. Quoting is `Inspect`'s,
/// and `Inspect` does not exist.
#[test]
fn a_char_prints_unquoted() {
    assert_eq!(rendered(|s| unsafe { science_string_push_char(s, 'A' as u32) }), "A");
    assert_eq!(rendered(|s| unsafe { science_string_push_char(s, 'σ' as u32) }), "σ");
}

#[test]
fn a_char_that_is_not_a_scalar_value_becomes_a_replacement() {
    // A lone surrogate. It cannot arise from a Science `Char`; a hole in the
    // output would be harder to trace than this.
    assert_eq!(rendered(|s| unsafe { science_string_push_char(s, 0xD800) }), "\u{FFFD}");
}

// --- bytes, and the whole of an f-string ---------------------------------

#[test]
fn push_bytes_appends_a_literal_fragment() {
    let text = rendered(|s| unsafe {
        let fragment = b"n is ";
        science_string_push_bytes(s, fragment.as_ptr(), fragment.len());
    });
    assert_eq!(text, "n is ");
}

#[test]
fn push_bytes_of_nothing_is_nothing() {
    let text = rendered(|s| unsafe { science_string_push_bytes(s, [].as_ptr(), 0) });
    assert!(text.is_empty());
}

/// The sequence `f"n is {n} rows"` lowers to, written out by hand. This is the
/// acceptance target of the whole exercise, at the one layer that can run it
/// today.
#[test]
fn the_builder_for_an_f_string_produces_the_line() {
    let text = rendered(|s| unsafe {
        let head = b"n is ";
        science_string_push_bytes(s, head.as_ptr(), head.len());
        science_string_push_i64(s, 42);
        let tail = b" rows";
        science_string_push_bytes(s, tail.as_ptr(), tail.len());
    });
    assert_eq!(text, "n is 42 rows");
}

/// `print(f"{n}")` for an `I64` and for an `F64`, which is the sentence the
/// whole feature exists for.
#[test]
fn the_acceptance_target_renders() {
    assert_eq!(rendered(|s| unsafe { science_string_push_i64(s, 42) }), "42");
    assert_eq!(rendered(|s| unsafe { science_string_push_f64(s, 42.0) }), "42.0");
}

// --- the allocation claim ------------------------------------------------

/// §1.7 asks for *"one allocation"* in the common case, and the push shape is
/// how this crate delivers it: an accumulator that has grown once does not
/// grow again for a number, because the growth floor is eight bytes and no
/// rendering here is longer than twenty-four.
///
/// The observable proxy for "did not reallocate" is the capacity: it is set by
/// the first push and unchanged by the second.
#[test]
fn rendering_into_a_grown_string_does_not_allocate_again() {
    unsafe {
        let mut out = science_string_new();
        let head = b"n is ";
        science_string_push_bytes(&mut out, head.as_ptr(), head.len());
        let capacity = out.cap;
        assert!(capacity >= 8, "the growth floor is eight bytes");
        science_string_push_i64(&mut out, 42);
        assert_eq!(out.cap, capacity, "rendering a number reallocated");
        assert_eq!(as_str(&out), "n is 42");
        free(out);
    }
}

/// Nothing here allocates when there is nothing to append, which is what makes
/// an `f"…"` with no holes cost what a plain literal costs.
#[test]
fn an_empty_render_allocates_nothing() {
    unsafe {
        let mut out = science_string_new();
        science_string_push_bytes(&mut out, [].as_ptr(), 0);
        assert_eq!(out.cap, 0);
        free(out);
    }
}

// --- `Formatter`, `strings-formatting-and-docs.md` §3.1 -------------------
//
// Every spec below is built by hand: §2's mini-language has no lexer yet, so
// there is no `f"{x:.2f}"` to write and let the compiler parse. What is under
// test is the *rendering* — the thing a lexer will one day hand a `FormatSpec`
// to — not the parsing, which does not exist.

fn default_spec() -> ScienceFormatSpec {
    science_format_spec_default()
}

/// Renders through a `Formatter`, the way `Display.display` will one day call
/// it, and hands back the text.
fn formatted(spec: ScienceFormatSpec, render: impl FnOnce(*mut ScienceFormatter)) -> String {
    let mut out = science_string_new();
    let mut formatter = ScienceFormatter { sink: &mut out, spec };
    render(&mut formatter);
    let text = as_str(&out).to_string();
    free(out);
    text
}

#[test]
fn the_default_spec_is_section_two_three_verbatim() {
    let spec = default_spec();
    assert_eq!(spec.fill, ' ' as u32);
    assert_eq!(spec.align, ScienceNullableAlign::null());
    assert_eq!(spec.sign, ScienceNullableSign::null());
    assert_eq!(spec.width, ScienceNullableInt::null());
    assert_eq!(spec.precision, ScienceNullableInt::null());
    assert_eq!(spec.code, ScienceNullableCode::null());
    assert!(!spec.alternate);
    assert_eq!(spec.grouping, ScienceNullableGrouping::null());
}

/// `Formatter.raw` never pads, no matter what the spec's width asks for —
/// §3.1: *"Write text with no padding, for a fragment of a larger
/// rendering."*
#[test]
fn raw_ignores_width_entirely() {
    let mut spec = default_spec();
    spec.width = ScienceNullableInt::some(12);
    let value = s("hi");
    let text = formatted(spec, |f| unsafe { science_formatter_raw(f, &value) });
    free(value);
    assert_eq!(text, "hi");
}

/// `Formatter.text` honours width and defaults to left alignment for a
/// string — §2.2's own example, `f"{name:<12}"` rendering `"A12         "`.
#[test]
fn text_pads_left_by_default() {
    let mut spec = default_spec();
    spec.width = ScienceNullableInt::some(12);
    let value = s("A12");
    let text = formatted(spec, |f| unsafe { science_formatter_text(f, &value) });
    free(value);
    assert_eq!(text, "A12         ");
    assert_eq!(text.chars().count(), 12);
}

/// A spec that explicitly asks for a different alignment and fill is
/// honoured over `text`'s own default.
#[test]
fn text_honours_an_explicit_fill_and_alignment() {
    let mut spec = default_spec();
    spec.width = ScienceNullableInt::some(6);
    spec.fill = '*' as u32;
    spec.align = ScienceNullableAlign::some(ScienceAlign::RIGHT);
    let value = s("hi");
    let text = formatted(spec, |f| unsafe { science_formatter_text(f, &value) });
    free(value);
    assert_eq!(text, "****hi");
}

/// A `text` narrower than its own width is returned unchanged, not truncated.
#[test]
fn text_with_no_width_is_unpadded() {
    let spec = default_spec();
    let value = s("hello");
    let text = formatted(spec, |f| unsafe { science_formatter_text(f, &value) });
    free(value);
    assert_eq!(text, "hello");
}

// --- `Formatter.integer`, §2.2's table -------------------------------------

/// §2.2: `f"{n:d}"` renders `1234567`.
#[test]
fn integer_with_the_decimal_code() {
    let mut spec = default_spec();
    spec.code = ScienceNullableCode::some(ScienceCode::DECIMAL);
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_integer(f, 1_234_567) }), "1234567");
}

/// §2.2: `f"{mask:#x}"` renders `0xff`.
#[test]
fn integer_hex_with_the_alternate_prefix() {
    let mut spec = default_spec();
    spec.code = ScienceNullableCode::some(ScienceCode::HEX);
    spec.alternate = true;
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_integer(f, 255) }), "0xff");
}

/// Without `#`, `x` has no prefix at all — the flag that turns it on is named
/// "alternate form" and not "hex form".
#[test]
fn integer_hex_without_the_alternate_prefix() {
    let mut spec = default_spec();
    spec.code = ScienceNullableCode::some(ScienceCode::HEX);
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_integer(f, 255) }), "ff");
}

/// §2.2: `f"{mode:o}"` renders `755`.
#[test]
fn integer_octal() {
    let mut spec = default_spec();
    spec.code = ScienceNullableCode::some(ScienceCode::OCTAL);
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_integer(f, 493) }), "755");
}

/// §2.2: `f"{flags:#b}"` renders `0b1010`.
#[test]
fn integer_binary_with_the_alternate_prefix() {
    let mut spec = default_spec();
    spec.code = ScienceNullableCode::some(ScienceCode::BINARY);
    spec.alternate = true;
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_integer(f, 10) }), "0b1010");
}

/// §2.2: `,` gives `1,234,567`.
#[test]
fn integer_grouped_with_a_comma() {
    let mut spec = default_spec();
    spec.grouping = ScienceNullableGrouping::some(ScienceGrouping::COMMA);
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_integer(f, 1_234_567) }), "1,234,567");
}

/// §2.2: `_` gives `1_234_567`, matching Science's own integer literal
/// syntax, so a number printed by a program can be pasted back into one.
#[test]
fn integer_grouped_with_an_underscore() {
    let mut spec = default_spec();
    spec.grouping = ScienceNullableGrouping::some(ScienceGrouping::UNDERSCORE);
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_integer(f, 1_234_567) }), "1_234_567");
}

/// §2.3's worked example for width and zero-fill: `f"{i:04d}"` renders
/// `0007`. There is no separate zero-fill flag in `FormatSpec` — §3.1 gives
/// it none — so the lexer that does not exist yet would spell `0` before a
/// width as `fill: '0', align: Right`, and this is that spec, built by hand.
#[test]
fn integer_zero_padded_via_fill_and_right_alignment() {
    let mut spec = default_spec();
    spec.code = ScienceNullableCode::some(ScienceCode::DECIMAL);
    spec.width = ScienceNullableInt::some(4);
    spec.fill = '0' as u32;
    spec.align = ScienceNullableAlign::some(ScienceAlign::RIGHT);
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_integer(f, 7) }), "0007");
}

/// Numbers align right by default, unlike `text`'s left.
#[test]
fn integer_pads_right_by_default() {
    let mut spec = default_spec();
    spec.width = ScienceNullableInt::some(5);
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_integer(f, 7) }), "    7");
}

/// A negative integer's sign survives grouping and padding.
#[test]
fn a_negative_integer_keeps_its_sign_through_grouping_and_width() {
    let mut spec = default_spec();
    spec.grouping = ScienceNullableGrouping::some(ScienceGrouping::COMMA);
    spec.width = ScienceNullableInt::some(10);
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_integer(f, -1_234) }), "    -1,234");
}

/// `+` on a positive integer, and unchanged on a negative one.
#[test]
fn integer_explicit_plus_sign() {
    let mut spec = default_spec();
    spec.sign = ScienceNullableSign::some(ScienceSign::PLUS);
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_integer(f, 7) }), "+7");
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_integer(f, -7) }), "-7");
}

/// The i64 edges, run through the default (no-spec) rendering, so the new
/// entry point agrees with `science_string_push_i64` on the case they share.
#[test]
fn integer_default_rendering_matches_push_i64() {
    let spec = default_spec();
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_integer(f, i64::MIN) }), "-9223372036854775808");
}

// --- `Formatter.number`, §2.2's table and §2.3's defaults ------------------

/// §2.2: `f"{t:.3f}"` renders `0.333`.
#[test]
fn number_fixed_precision() {
    let mut spec = default_spec();
    spec.code = ScienceNullableCode::some(ScienceCode::FIXED);
    spec.precision = ScienceNullableInt::some(3);
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_number(f, 1.0 / 3.0) }), "0.333");
}

/// §2.2: `f"{p:.2e}"` renders `4.51e-07` — a signed, two-digit exponent, not
/// Rust's bare `e-7`.
#[test]
fn number_exponential() {
    let mut spec = default_spec();
    spec.code = ScienceNullableCode::some(ScienceCode::EXP);
    spec.precision = ScienceNullableInt::some(2);
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_number(f, 0.000000451) }), "4.51e-07");
}

/// The upper-case exponential code capitalises the marker only.
#[test]
fn number_exponential_upper() {
    let mut spec = default_spec();
    spec.code = ScienceNullableCode::some(ScienceCode::EXP_UPPER);
    spec.precision = ScienceNullableInt::some(2);
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_number(f, 0.000000451) }), "4.51E-07");
}

/// §2.2: `f"{x:.3g}"` renders `0.000451` — three significant figures, and
/// the fixed branch of `g`'s own rule because the exponent (`-4`) is not less
/// than `-4`.
#[test]
fn number_general_uses_fixed_form_at_the_boundary() {
    let mut spec = default_spec();
    spec.code = ScienceNullableCode::some(ScienceCode::GENERAL);
    spec.precision = ScienceNullableInt::some(3);
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_number(f, 0.000451234) }), "0.000451");
}

/// `g` switches to exponential once the exponent reaches the significant
/// figure count.
#[test]
fn number_general_switches_to_exponential() {
    let mut spec = default_spec();
    spec.code = ScienceNullableCode::some(ScienceCode::GENERAL);
    spec.precision = ScienceNullableInt::some(3);
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_number(f, 1_234_000.0) }), "1.23e+06");
}

/// §2.2: `f"{r:+.2%}"` renders `+12.34%`.
#[test]
fn number_percent_with_explicit_sign() {
    let mut spec = default_spec();
    spec.code = ScienceNullableCode::some(ScienceCode::PERCENT);
    spec.precision = ScienceNullableInt::some(2);
    spec.sign = ScienceNullableSign::some(ScienceSign::PLUS);
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_number(f, 0.1234) }), "+12.34%");
}

/// §2.3: *"`NaN`, `inf`, `-inf`, spelled that way … Not `nan`."*
#[test]
fn number_special_values_are_spelled_as_section_two_three_says() {
    let spec = default_spec();
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_number(f, f64::NAN) }), "NaN");
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_number(f, f64::INFINITY) }), "inf");
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_number(f, f64::NEG_INFINITY) }), "-inf");
}

/// `NaN` carries no sign, even when the spec asks for one on every positive
/// number.
#[test]
fn nan_ignores_an_explicit_sign_request() {
    let mut spec = default_spec();
    spec.sign = ScienceNullableSign::some(ScienceSign::PLUS);
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_number(f, f64::NAN) }), "NaN");
}

/// §2.3: *"`-0.0` prints as `-0.0`."* — through the new entry point, not just
/// the old one.
#[test]
fn number_negative_zero_is_visible() {
    let spec = default_spec();
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_number(f, -0.0) }), "-0.0");
}

/// With no code and no precision, `number` renders exactly what
/// `science_string_push_f64` does — the entry point this one sits beside,
/// not replaces.
#[test]
fn number_default_rendering_matches_push_f64() {
    let spec = default_spec();
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_number(f, 0.1 + 0.2) }), "0.30000000000000004");
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_number(f, 1.0) }), "1.0");
}

/// A float pads right by default, honouring an explicit width.
#[test]
fn number_pads_right_by_default() {
    let mut spec = default_spec();
    spec.width = ScienceNullableInt::some(8);
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_number(f, 1.5) }), "     1.5");
}

/// Grouping applies to a float's integer part and leaves the fraction alone.
#[test]
fn number_fixed_grouped() {
    let mut spec = default_spec();
    spec.code = ScienceNullableCode::some(ScienceCode::FIXED);
    spec.precision = ScienceNullableInt::some(2);
    spec.grouping = ScienceNullableGrouping::some(ScienceGrouping::COMMA);
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_number(f, 1_234_567.5) }), "1,234,567.50");
}

/// `#` at precision zero keeps the decimal point — §2.2: *"a retained
/// decimal point for `f`/`e`/`g` at precision zero."*
#[test]
fn number_alternate_keeps_the_point_at_zero_precision() {
    let mut spec = default_spec();
    spec.code = ScienceNullableCode::some(ScienceCode::FIXED);
    spec.precision = ScienceNullableInt::some(0);
    spec.alternate = true;
    assert_eq!(formatted(spec, |f| unsafe { science_formatter_number(f, 4.0) }), "4.");
}

// --- `Formatter.spec` -------------------------------------------------------

/// `Formatter.spec()` hands back exactly what the `Formatter` was built
/// with — §3.1's own words, *"the parsed spec, for an implementation that
/// needs to branch on it."*
#[test]
fn spec_reads_back_what_the_formatter_was_built_with() {
    let mut out = science_string_new();
    let mut spec = default_spec();
    spec.width = ScienceNullableInt::some(9);
    spec.code = ScienceNullableCode::some(ScienceCode::HEX);
    let mut formatter = ScienceFormatter { sink: &mut out, spec };
    let read_back = unsafe { science_formatter_spec(&formatter) };
    assert_eq!(read_back.width, ScienceNullableInt::some(9));
    assert_eq!(read_back.code, ScienceNullableCode::some(ScienceCode::HEX));
    // `spec()` is a read: calling it does not disturb the sink or the spec
    // a later `.text()`/`.number()`/`.integer()` call on the same
    // `Formatter` would use.
    let value = s("ok");
    unsafe { science_formatter_raw(&mut formatter, &value) };
    free(value);
    assert_eq!(as_str(&out), "ok");
    free(out);
}
