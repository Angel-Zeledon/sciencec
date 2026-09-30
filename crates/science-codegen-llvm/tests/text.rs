//! `String.parse_int` and `String.parse_float`, **built, linked, run**.
//!
//! # What these pin
//!
//! `stdlib-core.md` §6.9 declares both as `-> (value, TextError?)`, and the
//! prelude had declared them for as long as `TextError` existed; the backend
//! refused the type they return. `science-rt`'s `text.rs` now implements both
//! and `TextError.message()`, and each test here runs one of the decisions it
//! takes: the syntax (no surrounding whitespace), the two error codes, the
//! value half on failure (`0` and `0.0`), correct rounding, and overflow.
//!
//! # And the call that returns the pair
//!
//! **The pair comes back in two registers, and that is where the risk was.**
//! LLVM splits a returned struct one register per member; C packs it. A
//! declaration `{ i64, [2 x i8], [6 x i8] } @science_string_parse_int(ptr)`
//! reads its result from a hidden return slot on AArch64 that the runtime
//! never writes. `emit.rs`'s `c_return_ty` declares it `[2 x i64]` instead,
//! and these programs are what show the two halves arriving where the
//! runtime put them — every assertion on an error below would read `false`
//! if the tag were read from the wrong byte.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("text", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// A valid integer, and the extremes of `I64` in both directions.
#[test]
fn an_integer_parses_and_its_error_is_null() {
    assert_eq!(
        prints(
            "parse_int_valid",
            "def main():
    let n, err be \"42\".parse_int()
    print(err?)
    print(n)
    let low, low_err be \"-9223372036854775808\".parse_int()
    print(low_err?)
    print(low)
    let high, high_err be \"+9223372036854775807\".parse_int()
    print(high_err?)
    print(high)
",
        ),
        "false\n42\nfalse\n-9223372036854775808\nfalse\n9223372036854775807\n"
    );
}

/// A negative number is arithmetic like any other `I64`.
#[test]
fn a_negative_integer_parses_and_takes_part_in_arithmetic() {
    assert_eq!(
        prints(
            "parse_int_negative",
            "def main():
    let n, err be \"-17\".parse_int()
    if err?:
        print(err.message())
    print(n * 2 + 1)
",
        ),
        "-33\n"
    );
}

/// Text outside the syntax is `not a number`, the value half is `0`, and
/// surrounding whitespace is outside the syntax: §6.11's own example trims
/// before it parses.
#[test]
fn invalid_input_is_not_a_number_and_its_message_says_so() {
    assert_eq!(
        prints(
            "parse_int_invalid",
            "def main():
    let n, err be \"4x2\".parse_int()
    if err?:
        print(err.message())
    print(n)
    let blank, blank_err be \"\".parse_int()
    if blank_err?:
        print(blank_err.message())
    print(blank)
    let spaced, spaced_err be \" 7\".parse_int()
    if spaced_err?:
        print(spaced_err.message())
    print(spaced)
",
        ),
        "not a number\n0\nnot a number\n0\nnot a number\n0\n"
    );
}

/// One past either end of `I64` is `number out of range`, not a wrapped value.
#[test]
fn an_integer_past_either_end_is_out_of_range() {
    assert_eq!(
        prints(
            "parse_int_overflow",
            "def main():
    let high, high_err be \"9223372036854775808\".parse_int()
    if high_err?:
        print(high_err.message())
    print(high)
    let low, low_err be \"-9223372036854775809\".parse_int()
    if low_err?:
        print(low_err.message())
    print(low)
",
        ),
        "number out of range\n0\nnumber out of range\n0\n"
    );
}

/// The case a digit-accumulating parser gets wrong.
///
/// `1 + 2^-53` written out exactly is halfway between `1.0` and the next
/// `F64` up, so it rounds to even — `1.0`; one more unit in the last decimal
/// place is past halfway, so it rounds up. Both come back through
/// `parse_float`'s `{ f64, TextError? }`, which on AArch64 is two `x`
/// registers holding a `double`'s bits — so this is also the test that the
/// value half survives the integer-register trip.
#[test]
fn a_float_is_correctly_rounded() {
    assert_eq!(
        prints(
            "parse_float_rounding",
            "def main():
    let tie, tie_err be \"1.00000000000000011102230246251565404236316680908203125\".parse_float()
    print(tie_err?)
    print(tie)
    let past, past_err be \"1.00000000000000011102230246251565404236316680908203126\".parse_float()
    print(past_err?)
    print(past)
    let tenth, tenth_err be \"0.1\".parse_float()
    print(tenth_err?)
    print(tenth + 0.2)
",
        ),
        "false\n1.0\nfalse\n1.0000000000000002\nfalse\n0.30000000000000004\n"
    );
}

/// Finite digits that round to infinity are `number out of range` and `0.0`;
/// `inf` spelled out is what Science prints for one, so it parses back.
#[test]
fn a_float_that_overflows_is_out_of_range_and_inf_spelled_out_is_not() {
    assert_eq!(
        prints(
            "parse_float_overflow",
            "def main():
    let huge, huge_err be \"1e400\".parse_float()
    if huge_err?:
        print(huge_err.message())
    print(huge)
    let named, named_err be \"-inf\".parse_float()
    print(named_err?)
    print(named)
    let junk, junk_err be \"1.5.2\".parse_float()
    if junk_err?:
        print(junk_err.message())
    print(junk)
",
        ),
        "number out of range\n0.0\nfalse\n-inf\nnot a number\n0.0\n"
    );
}

/// §6.11's `readings` shape: a loop that parses, returns the error it met,
/// and hands the pair on through a function of the user's own.
#[test]
fn a_parse_error_is_returned_through_a_function_of_the_programs_own() {
    assert_eq!(
        prints(
            "parse_through_a_function",
            "def total(texts: &Array[String]) -> (I64, TextError?):
    let mutable sum be 0
    for text in texts:
        let n, err be text.parse_int()
        if err?:
            return (0, err)
        sum be sum + n
    (sum, null)

def main():
    let good be [\"1\", \"2\", \"39\"]
    let sum, err be total(&good)
    print(err?)
    print(sum)
    let bad be [\"1\", \"x\"]
    let partial, bad_err be total(&bad)
    if bad_err?:
        print(f\"total failed: {bad_err.message()}\")
    print(partial)
",
        ),
        "false\n42\ntotal failed: not a number\n0\n"
    );
}
