//! The bundled `complex` module, **built, linked, run**.
//!
//! `crates/science-resolve/stdlib/complex.science` is Science with no runtime
//! of its own, so everything it promises is a printed line here: each
//! operator on pinned values, Smith's division where the textbook formula
//! would overflow or underflow, the display of a sign, and IEEE equality.
//!
//! # The operator on a generic record, which was a miscompile
//!
//! `Pair(re: 1.5, im: 2.0) + b` on a `type Pair[T]` passed `check` and was
//! refused by the backend as *"an operator on a value that is not a scalar"*.
//! The receiver's type was still a deferred composite — `T` waited on an
//! unsuffixed literal — and the operator read it as an error type, declined
//! to dispatch, and fell through to the structural `Binary` that only a
//! scalar has. `a.add(b)` on the same line worked, because a written call
//! settles its receiver first; the operator now does the same.
//!
//! # A method call with named arguments on a local, which was refused
//!
//! `z.scaled(by: 0.5)` is parsed as a qualified record literal and was
//! resolved as one: `SC0204`. The resolver now reads a dotted literal whose
//! head is a local binding as the method call the parser said it would be.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("complex", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// `+ - * /` and unary `-` through the operator interfaces, on values whose
/// results are exact in binary, both ways of building one, and a part read
/// as a field.
#[test]
fn the_operators_on_pinned_values() {
    assert_eq!(
        prints(
            "operators",
            "use complex (Complex)

def main():
    let a be Complex(re: 1.5, im: 2.0)
    let b be Complex.new(0.5, -1.0)
    print(a + b)
    print(a - b)
    print(a * b)
    print(a / b)
    print(-a)
    print(a + b * b)
    print(a.re)
    print(b.im)
",
        ),
        "2.0+1.0i\n1.0+3.0i\n2.75-0.5i\n-1.0+2.0i\n-1.5-2.0i\n0.75+1.0i\n1.5\n-1.0\n"
    );
}

/// The parts that need no square root, and the real factor the operator
/// cannot take.
#[test]
fn conjugate_modulus_squared_and_a_real_factor() {
    assert_eq!(
        prints(
            "parts",
            "use complex (Complex)

def main():
    let z be Complex.new(3.0, -4.0)
    print(z.conjugate())
    print(z.conjugate().conjugate())
    print(z.modulus_squared())
    print(z * z.conjugate())
    print(z.scaled(by: 0.5))
    print(Complex.from_real(2.5))
",
        ),
        "3.0+4.0i\n3.0-4.0i\n25.0\n25.0+0.0i\n1.5-2.0i\n2.5+0.0i\n"
    );
}

/// Smith's algorithm, through both of its branches, where the textbook
/// `(ac + bd) / (c² + d²)` breaks.
///
/// - A divisor at `1e300`: `c² + d²` overflows to `inf` and the textbook
///   answer is `0+0i`; the quotient is exactly one.
/// - A divisor at `1e-300`: `c² + d²` underflows to `0` and the textbook
///   answer is `inf`; the quotient is `1e300`, here one ulp under it because
///   `2e-300` is not exact in binary.
/// - A divisor whose imaginary part dominates by six hundred orders of
///   magnitude: the second branch, with `ratio` underflowing to zero and the
///   quotient still the right `1e-300 - 5e-301i`.
/// - The first branch on ordinary values, and a zero divisor, which is `NaN`
///   in both parts — the header says why it is not Annex G's infinity.
#[test]
fn division_is_stable_at_huge_and_tiny_components() {
    assert_eq!(
        prints(
            "division",
            "use complex (Complex)

def main():
    print(Complex.new(1.0e300, 1.0e300) / Complex.new(1.0e300, 1.0e300))
    print(Complex.new(1.0, 1.0) / Complex.new(1.0e-300, 1.0e-300))
    print(Complex.new(1.0, 2.0) / Complex.new(1.0e-300, 2.0e300))
    print(Complex.new(1.0, 2.0) / Complex.new(2.0, 1.0))
    print(Complex.new(1.0, 0.0) / Complex.new(0.0, 0.0))
",
        ),
        "1.0+0.0i\n9.999999999999999e299+0.0i\n1e-300-5e-301i\n0.8+0.6i\nNaN+NaNi\n"
    );
}

/// The sign of the imaginary part is always written, a negative one reads
/// `a-bi`, and `-0.0` keeps its sign.
#[test]
fn display_writes_the_sign_of_the_imaginary_part() {
    assert_eq!(
        prints(
            "display",
            "use complex (Complex)

def main():
    print(Complex.new(1.5, 2.0))
    print(Complex.new(1.5, -2.0))
    print(Complex.new(-1.5, -2.0))
    print(Complex.new(0.0, 0.0))
    print(Complex.new(0.0, -0.0))
    print(-Complex.new(0.0, 0.0))
    let z be Complex.new(2.0, -0.25)
    print(f\"z = {z}\")
",
        ),
        "1.5+2.0i\n1.5-2.0i\n-1.5-2.0i\n0.0+0.0i\n0.0-0.0i\n-0.0-0.0i\nz = 2.0-0.25i\n"
    );
}

/// `is` and `is not` are IEEE equality part by part: `-0.0` equals `0.0`,
/// and a `NaN` part makes a value unequal to itself.
#[test]
fn equality_is_ieee_part_by_part() {
    assert_eq!(
        prints(
            "equality",
            "use complex (Complex)

def main():
    let a be Complex.new(1.0, 2.0)
    print(a is Complex.new(1.0, 2.0))
    print(a is a.conjugate())
    print(a is not a.conjugate())
    print(Complex.new(0.0, 0.0) is Complex.new(-0.0, -0.0))
    let nan be Complex.new(1.0, 0.0) / Complex.new(0.0, 0.0)
    print(nan is nan)
",
        ),
        "true\nfalse\ntrue\ntrue\nfalse\n"
    );
}

/// The miscompile in this file's header, with no module in the way: an
/// operator whose receiver is a generic record built from unsuffixed
/// literals.
#[test]
fn an_operator_on_a_generic_record_built_from_literals() {
    assert_eq!(
        prints(
            "generic_operator",
            "type Pair[T: Add + Copy]:
    re: T
    im: T

Pair[T: Add + Copy] implements Add:
    def add(self: Self, other: Pair[T]) -> Pair[T]:
        Pair(re: self.re + other.re, im: self.im + other.im)

def main():
    let a be Pair(re: 1.5, im: 2.0)
    let b be Pair(re: 1, im: 2)
    let c be a + Pair(re: 0.5, im: 0.5)
    let d be b + b
    print(c.re)
    print(c.im)
    print(d.im)
",
        ),
        "2.0\n2.5\n4\n"
    );
}

/// `z.scaled(by: 0.5)` with `z` a local, with no module in the way. The
/// parser hands `local.method(name: value)` over as a qualified record
/// literal, and the resolver refused it as `SC0204` — *"`p` is a local
/// binding, so `scaled` cannot be reached through it"* — while the same call
/// on a receiver that is not a bare name was a method call all along.
#[test]
fn a_method_with_named_arguments_on_a_local() {
    assert_eq!(
        prints(
            "named_method_args",
            "type P:
    x: F64

P has:
    def scaled(self, by: F64) -> F64:
        self.x * by

def main():
    let p be P(x: 2.0)
    print(p.scaled(by: 3.0))
    print(P(x: 1.0).scaled(by: 3.0))
",
        ),
        "6.0\n3.0\n"
    );
}
