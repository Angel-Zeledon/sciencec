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

// --- Level 1 math ------------------------------------------------------------
//
// The expected values below are **not** read off this program's output. The
// general ones are Python 3's `cmath` (`cmath.exp`, `cmath.log`, `cmath.sqrt`,
// `cmath.sin` and the rest, `z ** w`, and `cmath.exp(x * cmath.log(z))` for a
// real power), and are compared within a few ulps, because two correct
// libraries may round a transcendental differently. The special values and
// the branch cuts are compared as strings, exactly: they are C99 Annex G's
// (and agree with `cmath` wherever `cmath` answers — it raises for `log(0)`,
// where Annex G gives `-∞+0i`).

/// The two parts of a printed `a+bi` / `a-bi`. The split is at the last `+`
/// or `-` that is not a number's leading sign or an exponent's.
fn parts(line: &str) -> (f64, f64) {
    let body = line.strip_suffix('i').unwrap_or_else(|| panic!("`{line}` is not `a+bi`"));
    let bytes = body.as_bytes();
    let at = (1..bytes.len())
        .rev()
        .find(|&i| matches!(bytes[i], b'+' | b'-') && !matches!(bytes[i - 1], b'e' | b'E'))
        .unwrap_or_else(|| panic!("`{line}` has no imaginary sign"));
    let re = body[..at].parse().unwrap_or_else(|_| panic!("real part of `{line}`"));
    let im = body[at..].parse().unwrap_or_else(|_| panic!("imaginary part of `{line}`"));
    (re, im)
}

/// Whether `actual` is within `ulps` units in the last place of `expected`.
fn within_ulps(actual: f64, expected: f64, ulps: f64) -> bool {
    if actual == expected {
        return true;
    }
    let spacing = f64::EPSILON * expected.abs().max(f64::MIN_POSITIVE);
    (actual - expected).abs() <= ulps * spacing
}

/// Each expression is printed on its own line and compared, part by part,
/// with `cmath`'s value: four ulps of that part, or four ulps of the larger
/// part — the absolute error a correctly rounded pair of ordinary formulas
/// can leave in a part that is tiny next to its sibling.
fn agrees_with_cmath(name: &str, prelude: &str, cases: &[(&str, f64, f64)]) {
    let mut source = format!("use complex (Complex)\n\ndef main():\n{prelude}");
    for (expr, _, _) in cases {
        source.push_str(&format!("    print({expr})\n"));
    }
    let out = prints(name, &source);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), cases.len(), "one line per case:\n{out}");
    for ((expr, re, im), line) in cases.iter().zip(lines) {
        let (got_re, got_im) = parts(line);
        let scale = re.abs().max(im.abs());
        let close = |got: f64, want: f64| {
            within_ulps(got, want, 4.0) || (got - want).abs() <= 4.0 * f64::EPSILON * scale
        };
        assert!(
            close(got_re, *re) && close(got_im, *im),
            "`{expr}` printed `{line}`, and cmath gives {re:?}{im:+?}i"
        );
    }
}

/// `modulus` through `hypot`, `argument` through `atan2`, and the polar form
/// both ways, including a modulus whose square overflows.
#[test]
fn the_polar_form() {
    assert_eq!(
        prints(
            "polar",
            "use complex (Complex)

def main():
    let z be Complex.new(3.0, -4.0)
    print(z.modulus())
    print(z.argument())
    let r, theta be z.to_polar()
    print(r)
    print(theta)
    let huge be Complex.new(1.0e300, 1.0e300)
    print(huge.modulus())
    print(huge.modulus_squared())
    print(Complex.from_polar(2.0, 0.5))
    print(Complex.from_polar(2.0, F64.PI / 3.0))
",
        ),
        "5.0\n-0.9272952180016122\n5.0\n-0.9272952180016122\n1.4142135623730952e300\ninf\n\
         1.7551651237807455+0.958851077208406i\n1.0000000000000002+1.7320508075688772i\n"
    );
}

/// `exp`, `ln`, `sqrt`, `pow` and `pow_complex` at ordinary points, and
/// `sqrt` at parts near the bottom and the top of the range, where the
/// unhalved `sqrt((|z| + |re|) / 2)` underflows or overflows.
#[test]
fn exp_ln_sqrt_and_pow_agree_with_cmath() {
    agrees_with_cmath(
        "elementary",
        "    let z be Complex.new(3.0, -4.0)
    let w be Complex.new(-0.5, 2.25)
    let s be Complex.new(0.5, 1.0)
",
        &[
            ("z.exp()", -13.128783081462158, 15.200784463067956),
            ("w.exp()", -0.38100656180409787, 0.47192524941314856),
            ("z.ln()", 1.6094379124341003, -0.9272952180016122),
            ("w.ln()", 0.8350312671252675, 1.7894652726688385),
            ("z.sqrt()", 2.0, -1.0),
            ("w.sqrt()", 0.9499700296123088, 1.18424788670346),
            ("Complex.new(1.0e-300, 3.0e-300).sqrt()", 1.442615274452683e-150, 1.0397782600555704e-150),
            ("Complex.new(-1.0e308, 1.0e308).sqrt()", 4.5508986056222734e+153, 1.09868411346781e+154),
            ("w.pow(0.5)", 0.9499700296123088, 1.1842478867034598),
            ("z.pow(-1.5)", 0.016000000000000014, 0.08800000000000002),
            ("Complex.new(-8.0, 0.0).pow(1.0 / 3.0)", 1.0000000000000002, 1.7320508075688772),
            ("z.pow_complex(s)", 2.3304790489341816, 5.149201131069768),
            ("w.pow_complex(z)", -6945.1128250654465, 14107.27556273668),
        ],
    );
}

/// The six trigonometric and hyperbolic functions at two points, and `tan`
/// and `tanh` past the cutover at 20, where the parts are taken from
/// `e^(-2|b|)` and `tanh 2b`.
#[test]
fn trigonometric_and_hyperbolic_agree_with_cmath() {
    agrees_with_cmath(
        "trigonometric",
        "    let z be Complex.new(3.0, -4.0)
    let w be Complex.new(-0.5, 2.25)
",
        &[
            ("z.sin()", 3.853738037919377, 27.016813258003936),
            ("z.cos()", -27.034945603074224, 3.8511533348117775),
            ("z.tan()", -0.0001873462046294784, -0.999355987381473),
            ("w.sin()", -2.2995969717423113, 4.116887500149177),
            ("w.cos()", 4.209384021661437, 2.2490658917382738),
            ("w.tan()", -0.01847177374820728, 0.9878955483859253),
            ("z.sinh()", -6.5481200409110025, 7.619231720321411),
            ("z.cosh()", -6.580663040551157, 7.5815527427465454),
            ("z.tanh()", 1.000709536067233, -0.00490825806749606),
            ("w.sinh()", 0.3273383258358198, 0.8773755396419565),
            ("w.cosh()", -0.7083448876399175, -0.40545029022880796),
            ("w.tanh()", -0.8820945509784084, -0.7337245697786591),
            ("Complex.new(0.75, 30.0).tan()", 1.7469151171868444e-26, 1.0),
            ("Complex.new(-30.0, 0.75).tanh()", -1.0, 1.7469151171868444e-26),
        ],
    );
}

/// The cut along the negative real axis, read off the sign of a zero
/// imaginary part, and Annex G's special values: exact strings.
#[test]
fn branch_cuts_signed_zeros_and_special_values() {
    assert_eq!(
        prints(
            "branch_cuts",
            "use complex (Complex)

def main():
    print(Complex.new(-4.0, 0.0).sqrt())
    print(Complex.new(-4.0, -0.0).sqrt())
    print(Complex.new(-1.0, 0.0).ln())
    print(Complex.new(-1.0, -0.0).ln())
    print(Complex.new(-1.0, 0.0).argument())
    print(Complex.new(-1.0, -0.0).argument())
    print(Complex.new(0.0, -0.0).sqrt())
    print(Complex.new(-0.0, 0.0).sqrt())
    print(Complex.new(1.0, F64.INFINITY).sqrt())
    print(Complex.new(-F64.INFINITY, 1.0).sqrt())
    print(Complex.new(F64.INFINITY, -1.0).sqrt())
    print(Complex.new(0.0, 0.0).ln())
    print(Complex.new(F64.INFINITY, 0.0).exp())
    print(Complex.new(0.0, -0.0).exp())
    print(Complex.new(-0.0, 0.0).sin())
    print(Complex.new(1.0, 400.0).tan())
    print(Complex.new(400.0, 1.0).tanh())
    print(Complex.new(0.0, 0.0).pow(2.5))
    print(Complex.new(3.0, -4.0).pow(0.0))
    print(Complex.new(0.0, 0.0).pow_complex(Complex.new(0.0, 0.0)))
",
        ),
        "0.0+2.0i\n0.0-2.0i\n0.0+3.141592653589793i\n0.0-3.141592653589793i\n\
         3.141592653589793\n-3.141592653589793\n0.0-0.0i\n0.0+0.0i\ninf+infi\n0.0+infi\n\
         inf-0.0i\n-inf+0.0i\ninf+0.0i\n1.0-0.0i\n-0.0+0.0i\n0.0+1.0i\n1.0+0.0i\n0.0+0.0i\n\
         1.0+0.0i\n1.0+0.0i\n"
    );
}
