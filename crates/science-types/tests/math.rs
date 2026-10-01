//! `stdlib-core.md` §8's Level 1 math, as the checker sees it: thirty-five
//! methods on `F64` and `F32` and seven on `Int`, declared in `builtins.rs`, and
//! §8.1's constants, read as `F64.PI`.
//!
//! The execution half — what each method answers — is
//! `science-codegen-llvm/tests/math.rs`. This half is the types: a method is
//! found on the receiver it was declared on and **not** on another numeric
//! type, its arguments are the receiver's width, and the three heads §8
//! closes (`F64`, `F32`, `Int`) say `SC0532` for a name nothing gives them,
//! where until §8 they said nothing at all.

mod support;

use support::check;

/// `SC0532`: no such method.
const NO_SUCH_METHOD: u16 = 532;
/// `SC0525`: a mismatch between an expected and a found type.
const MISMATCH: u16 = 525;

#[test]
fn every_method_checks_on_the_receiver_it_was_declared_on() {
    check(
        "\
def floats(x: F64, f: F32) -> F64:
    let a be x.abs() + x.sign() + x.floor() + x.ceil() + x.round() + x.trunc() + x.fract()
    let b be x.min(1.0) + x.max(1.0) + x.clamp(0.0, 1.0) + x.rem_euclid(2.0)
    let c be x.sqrt() + x.cbrt() + x.hypot(1.0) + x.exp() + x.ln() + x.log2() + x.log10()
    let d be x.pow(2.0) + x.sin() + x.cos() + x.tan() + x.asin() + x.acos() + x.atan()
    let e be x.atan2(1.0) + x.sinh() + x.cosh() + x.tanh() + x.to_degrees() + x.to_radians()
    let narrow: F32 be f.sqrt() + f.atan2(f) + f.clamp(0.0, 1.0)
    if x.is_nan() or x.is_infinite() or not x.is_finite() or x.is_close(1.0):
        return 0.0
    a + b + c + d + e + narrow as F64

def ints(n: Int) -> Int:
    n.abs() + n.sign() + n.min(1) + n.max(1) + n.clamp(0, 9) + n.rem_euclid(3) + n.pow(2)
",
    )
    .assert_clean();
}

/// The receiver that most programs will get wrong: `sqrt` is a float method,
/// and an `Int` has none. Reported, by name and on `Int`, rather than accepted
/// — `Int`'s surface was wholly open before §8 gave it one, so this was
/// silence and then `SC0400` in the backend.
#[test]
fn a_float_method_on_an_int_is_no_such_method() {
    let checked = check(
        "\
def root(n: Int) -> F64:
    n.sqrt()
",
    );
    assert_eq!(checked.codes(), vec![NO_SUCH_METHOD]);
    let message = &checked.messages()[0];
    assert!(message.contains("sqrt"), "{message}");
}

/// A misspelling on a float is now a misspelling.
#[test]
fn a_misspelled_float_method_is_no_such_method() {
    let checked = check(
        "\
def root(x: F64) -> F64:
    x.square_root()
",
    );
    assert_eq!(checked.codes(), vec![NO_SUCH_METHOD]);
}

/// The names a later note adds on top of §8.1 — `intrinsics-math-physics.md`
/// §4.3's `fma`, `expm1`, `count_ones` — are in `UNWRITTEN` and stay silent:
/// the note gives them, and saying *"no such method"* would be false.
#[test]
fn a_name_the_extension_note_gives_is_not_reported() {
    check(
        "\
def later(x: F64, n: Int):
    let a be x.fma(1.0, 2.0)
    let b be x.expm1()
    let c be n.count_ones()
",
    )
    .assert_clean();
}

/// No implicit width change: `min` on an `F64` takes an `F64`, and an `F32`
/// argument is a mismatch, as it would be for `+`.
#[test]
fn an_argument_of_the_other_float_width_is_a_mismatch() {
    let checked = check(
        "\
def smaller(x: F64, f: F32) -> F64:
    x.min(f)
",
    );
    assert_eq!(checked.codes(), vec![MISMATCH]);
}

/// `Int.rem_euclid` takes an `Int`; a float divisor is a mismatch rather than a
/// conversion.
#[test]
fn a_float_argument_to_an_int_method_is_a_mismatch() {
    let checked = check(
        "\
def wrap(n: Int) -> Int:
    n.rem_euclid(2.5)
",
    );
    assert_eq!(checked.codes(), vec![MISMATCH]);
}

/// The result is the receiver's type, so an `F32` method's answer does not
/// fit an `F64` slot.
#[test]
fn the_result_is_the_receivers_width() {
    let checked = check(
        "\
def widen(f: F32) -> F64:
    f.sqrt()
",
    );
    assert_eq!(checked.codes(), vec![MISMATCH]);
}

/// An unsuffixed literal receiver takes Decision 2's default at the call, so
/// `let y be 2.0` then `y.sqrt()` is an `F64` method — and `let n be 2` then
/// `n.sqrt()` is an `Int` with no `sqrt`, reported rather than missed.
#[test]
fn a_literal_receiver_is_settled_at_the_call() {
    check(
        "\
def main():
    let y be 2.0
    let root: F64 be y.sqrt()
    let n be -3
    let size: Int be n.abs()
",
    )
    .assert_clean();
    let checked = check(
        "\
def main():
    let n be 2
    let root be n.sqrt()
",
    );
    assert_eq!(checked.codes(), vec![NO_SUCH_METHOD]);
}

/// §8.1's constants, typed at their own width, through both spellings of the
/// aliased primitives.
#[test]
fn the_constants_have_their_types() {
    check(
        "\
def constants():
    let a: F64 be F64.PI + F64.E + F64.TAU + F64.INFINITY + F64.NAN + F64.EPSILON
    let b: F64 be F64.MIN + F64.MAX + Float.PI
    let c: F32 be F32.PI + F32.E + F32.TAU + F32.INFINITY + F32.NAN + F32.EPSILON
    let d: F32 be F32.MIN + F32.MAX
    let e: Int be Int.MIN + Int.MAX + I64.MAX
",
    )
    .assert_clean();
    let checked = check(
        "\
def wrong() -> F64:
    F32.PI
",
    );
    assert_eq!(checked.codes(), vec![MISMATCH]);
}

/// `Int.PI` is not a value — §8.1 gives `Int` `MIN` and `MAX` only — and it is
/// said, as a missing field on the type, rather than read as a hole.
#[test]
fn a_constant_the_type_does_not_have_is_reported() {
    let checked = check(
        "\
def wrong() -> Int:
    Int.PI
",
    );
    assert_eq!(checked.codes(), vec![528]);
    let message = &checked.messages()[0];
    assert!(message.contains("PI") && message.contains("I64"), "{message}");
}
