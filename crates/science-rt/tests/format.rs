//! `format.rs`: the seven entry points an `f"…"` is built out of, and §2.3's
//! defaults, which are the part a note can be wrong about and a test cannot.

mod common;

use common::{as_str, free};
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
