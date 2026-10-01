//! The bundled `stats` module, **built, linked, run**.
//!
//! # Where the expected numbers come from
//!
//! Not from the module. Every constant below was computed outside it, in
//! Python: the variances and covariances in exact rational arithmetic
//! (`fractions.Fraction` over the doubles the program holds) and rounded once
//! at the end, the medians and quantiles by `numpy.quantile` (its `linear`
//! method, which is Hyndman and Fan's type 7), the sums by `math.fsum`, the
//! correlations by `numpy.corrcoef`. The pseudo-random data of the selection
//! tests is the same linear congruential generator written in both languages,
//! so the Python side computes the answer for the very array the program
//! builds.
//!
//! # Tolerances
//!
//! Where the answer is exactly representable — a sum of tenths, a median of
//! multiples of one eighth, a variance of integers — the comparison is exact.
//! Where it is not, it is relative, at `1e-12`: a hundred times the rounding
//! error a correct algorithm leaves, and far below what `E[x²] - E[x]²` leaves
//! on the catastrophic-cancellation fixture, which is the point of that
//! fixture.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("stats", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// Two helpers every program shares: `show` prints `label value` or
/// `label null`, and `lcg` is the generator the Python side repeats.
const SHARED: &str = "def show(label: String, v: F64?):
    if v?:
        print(f\"{label} {v}\")
    else:
        print(f\"{label} null\")

def lcg(count: Int, seed: Int) -> Array[F64]:
    let mutable out be Array[F64].new()
    let mutable s be seed
    for i in 0..count:
        s be (s * 1103515245 + 12345) % 2147483648
        out.push((s % 1000) as F64 / 8.0)
    out
";

/// `Some(value)` for a number, `None` for `null`.
fn get(out: &str, label: &str) -> Option<f64> {
    let line = out
        .lines()
        .find(|l| l.split(' ').next() == Some(label))
        .unwrap_or_else(|| panic!("no line for `{label}` in:\n{out}"));
    let text = line.split(' ').nth(1).unwrap();
    if text == "null" { None } else { Some(text.parse().unwrap_or_else(|_| panic!("`{text}`"))) }
}

fn exactly(out: &str, label: &str, want: f64) {
    let got = get(out, label).unwrap_or_else(|| panic!("`{label}` is null"));
    assert!(got == want, "`{label}`: got {got:e}, want {want:e}");
}

fn close(out: &str, label: &str, want: f64) {
    let got = get(out, label).unwrap_or_else(|| panic!("`{label}` is null"));
    let tolerance = 1e-12 * want.abs().max(f64::MIN_POSITIVE);
    assert!((got - want).abs() <= tolerance, "`{label}`: got {got:e}, want {want:e}");
}

fn null(out: &str, label: &str) {
    assert_eq!(get(out, label), None, "`{label}` should be null");
}

fn nan(out: &str, label: &str) {
    let got = get(out, label).unwrap_or_else(|| panic!("`{label}` is null"));
    assert!(got.is_nan(), "`{label}`: got {got:e}, want NaN");
}

/// Compensated summation, where a plain left-to-right sum is wrong.
#[test]
fn a_sum_is_compensated() {
    let out = prints(
        "sums",
        &format!(
            "use stats (sum)

{SHARED}
def main():
    let tenths be [0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1]
    show(\"tenths\", sum(&tenths))
    let swallowed be [1.0e100, 1.0, -1.0e100]
    show(\"swallowed\", sum(&swallowed))
    let halves be [1.0e16, 1.0, 1.0, -1.0e16]
    show(\"halves\", sum(&halves))
    let empty be Array[F64].new()
    show(\"empty\", sum(&empty))
    let up be [F64.INFINITY, 1.0]
    show(\"infinite\", sum(&up))
    let both be [F64.INFINITY, -F64.INFINITY]
    show(\"both\", sum(&both))
    let poisoned be [1.0, F64.NAN, 2.0]
    show(\"poisoned\", sum(&poisoned))
"
        ),
    );
    // `math.fsum([0.1] * 10)` is 1.0; the plain sum is 0.9999999999999999.
    exactly(&out, "tenths", 1.0);
    exactly(&out, "swallowed", 1.0);
    exactly(&out, "halves", 2.0);
    exactly(&out, "empty", 0.0);
    exactly(&out, "infinite", f64::INFINITY);
    nan(&out, "both");
    nan(&out, "poisoned");
}

/// Mean, extremes, and the three ways an answer is not a number: empty input
/// is `null`, a `NaN` in is a `NaN` out, and an infinity is an infinity.
#[test]
fn mean_min_max_on_empty_and_nan_input() {
    let out = prints(
        "extremes",
        &format!(
            "use stats (mean, min, max, median, quantile, variance, std)

{SHARED}
def main():
    let xs be [3.0, -1.0, 2.0, -1.0, 7.0]
    show(\"mean\", mean(&xs))
    show(\"min\", min(&xs))
    show(\"max\", max(&xs))
    let empty be Array[F64].new()
    show(\"e_mean\", mean(&empty))
    show(\"e_min\", min(&empty))
    show(\"e_max\", max(&empty))
    show(\"e_median\", median(&empty))
    show(\"e_quantile\", quantile(&empty, 0.5))
    show(\"e_var\", variance(&empty, 0))
    show(\"e_std\", std(&empty, 0))
    let bad be [1.0, F64.NAN, 5.0, 2.0]
    show(\"n_mean\", mean(&bad))
    show(\"n_min\", min(&bad))
    show(\"n_max\", max(&bad))
    show(\"n_median\", median(&bad))
    show(\"n_quantile\", quantile(&bad, 0.1))
    show(\"n_var\", variance(&bad, 1))
    show(\"n_std\", std(&bad, 1))
    let last be [1.0, 2.0, F64.NAN]
    show(\"l_min\", min(&last))
    show(\"l_median\", median(&last))
    let wide be [-F64.INFINITY, 1.0, F64.INFINITY]
    show(\"i_min\", min(&wide))
    show(\"i_max\", max(&wide))
    show(\"i_median\", median(&wide))
"
        ),
    );
    // numpy: mean = 10/5.
    exactly(&out, "mean", 2.0);
    exactly(&out, "min", -1.0);
    exactly(&out, "max", 7.0);
    for label in ["e_mean", "e_min", "e_max", "e_median", "e_quantile", "e_var", "e_std"] {
        null(&out, label);
    }
    for label in ["n_mean", "n_min", "n_max", "n_median", "n_quantile", "n_var", "n_std", "l_min", "l_median"] {
        nan(&out, label);
    }
    exactly(&out, "i_min", f64::NEG_INFINITY);
    exactly(&out, "i_max", f64::INFINITY);
    exactly(&out, "i_median", 1.0);
}

/// The fixture the header names: a mean of a billion and a spread of ten.
/// `E[x²] - E[x]²` gives `-128` for it (Python, plain doubles); the exact
/// answer, from rational arithmetic, is 30 and 22.5.
#[test]
fn variance_survives_catastrophic_cancellation() {
    let out = prints(
        "variance",
        &format!(
            "use stats (mean, variance, std)

{SHARED}
def main():
    let a be [1000000004.0, 1000000007.0, 1000000013.0, 1000000016.0]
    show(\"a_mean\", mean(&a))
    show(\"a_var1\", variance(&a, 1))
    show(\"a_var0\", variance(&a, 0))
    show(\"a_std1\", std(&a, 1))
    let b be [1000000000.1, 1000000000.2, 1000000000.4, 1000000000.3, 1000000000.0]
    show(\"b_var1\", variance(&b, 1))
    show(\"b_var0\", variance(&b, 0))
    show(\"b_std1\", std(&b, 1))
    let e be [7.0, 1.0, 5.0, 3.0, 3.0, 9.0]
    show(\"e_var1\", variance(&e, 1))
    let one be [4.0]
    show(\"one_var0\", variance(&one, 0))
    show(\"one_var1\", variance(&one, 1))
    show(\"e_var6\", variance(&e, 6))
    show(\"e_var_neg\", variance(&e, -1))
    let same be [2.5, 2.5, 2.5]
    show(\"same\", variance(&same, 1))
"
        ),
    );
    exactly(&out, "a_mean", 1000000010.0);
    exactly(&out, "a_var1", 30.0);
    exactly(&out, "a_var0", 22.5);
    close(&out, "a_std1", 30.0_f64.sqrt());
    // Fractions over the doubles the literals round to.
    close(&out, "b_var1", 0.024999994039536944);
    close(&out, "b_var0", 0.019999995231629555);
    close(&out, "b_std1", 0.15811386415977868);
    // numpy.var(ddof=1) of the six; 52/6 exactly.
    close(&out, "e_var1", 8.666666666666666);
    exactly(&out, "one_var0", 0.0);
    null(&out, "one_var1");
    null(&out, "e_var6");
    null(&out, "e_var_neg");
    exactly(&out, "same", 0.0);
}

/// Covariance and Pearson correlation, against exact fractions and
/// `numpy.corrcoef`, including the offset that defeats the one-pass formula.
#[test]
fn covariance_and_correlation() {
    let out = prints(
        "covariance",
        &format!(
            "use stats (covariance, correlation)

{SHARED}
def main():
    let x be [1.0, 2.0, 3.0, 4.0, 5.0]
    let y be [2.0, 4.0, 5.0, 4.0, 5.0]
    show(\"cov1\", covariance(&x, &y, 1))
    show(\"cov0\", covariance(&x, &y, 0))
    show(\"corr\", correlation(&x, &y))
    let xo be [1000000001.0, 1000000002.0, 1000000003.0, 1000000004.0, 1000000005.0]
    let yo be [100000002.0, 100000004.0, 100000005.0, 100000004.0, 100000005.0]
    show(\"big_cov\", covariance(&xo, &yo, 1))
    show(\"big_corr\", correlation(&xo, &yo))
    let up be [1.0, 2.0, 3.0, 4.0]
    let down be [8.0, 6.0, 4.0, 2.0]
    show(\"neg\", correlation(&up, &down))
    show(\"pos\", correlation(&up, &up))
    let flat be [3.0, 3.0, 3.0, 3.0]
    show(\"flat\", correlation(&up, &flat))
    let short be [1.0, 2.0]
    show(\"mismatch_cov\", covariance(&x, &short, 1))
    show(\"mismatch_corr\", correlation(&x, &short))
    let single be [1.0]
    show(\"single_corr\", correlation(&single, &single))
    let empty be Array[F64].new()
    show(\"empty_cov\", covariance(&empty, &empty, 0))
    let poisoned be [1.0, F64.NAN, 3.0, 4.0]
    show(\"nan_corr\", correlation(&up, &poisoned))
    show(\"nan_cov\", covariance(&up, &poisoned, 1))
"
        ),
    );
    // Fractions: Sxy = 6, so 6/4 and 6/5; numpy.corrcoef: 0.7745966692414834.
    exactly(&out, "cov1", 1.5);
    exactly(&out, "cov0", 1.2);
    close(&out, "corr", 0.7745966692414834);
    exactly(&out, "big_cov", 1.5);
    close(&out, "big_corr", 0.7745966692414834);
    exactly(&out, "neg", -1.0);
    exactly(&out, "pos", 1.0);
    nan(&out, "flat");
    for label in ["mismatch_cov", "mismatch_corr", "single_corr", "empty_cov"] {
        null(&out, label);
    }
    nan(&out, "nan_corr");
    nan(&out, "nan_cov");
}

/// Median and type 7 quantiles: odd and even counts, duplicates, the ends,
/// and a caller's array that must come back unchanged.
#[test]
fn median_and_quantile_follow_type_7() {
    let out = prints(
        "quantile",
        &format!(
            "use stats (median, quantile)

{SHARED}
def main():
    let odd be [5.0, -2.0, 8.0, 1.0, 4.0]
    show(\"odd_median\", median(&odd))
    show(\"odd_q10\", quantile(&odd, 0.1))
    show(\"odd_q30\", quantile(&odd, 0.3))
    show(\"odd_q75\", quantile(&odd, 0.75))
    print(odd[0])
    print(odd[4])
    let even be [7.0, 1.0, 5.0, 3.0, 3.0, 9.0]
    show(\"even_median\", median(&even))
    show(\"even_q0\", quantile(&even, 0.0))
    show(\"even_q25\", quantile(&even, 0.25))
    show(\"even_q90\", quantile(&even, 0.9))
    show(\"even_q100\", quantile(&even, 1.0))
    let two be [10.0, 20.0]
    show(\"two\", median(&two))
    let one be [42.0]
    show(\"one_median\", median(&one))
    show(\"one_q\", quantile(&one, 0.3))
    show(\"above\", quantile(&even, 1.5))
    show(\"below\", quantile(&even, -0.1))
    show(\"not_a_number\", quantile(&even, F64.NAN))
"
        ),
    );
    // numpy.median / numpy.quantile (method='linear').
    exactly(&out, "odd_median", 4.0);
    close(&out, "odd_q10", -0.7999999999999998);
    close(&out, "odd_q30", 1.5999999999999999);
    exactly(&out, "odd_q75", 5.0);
    let bare: Vec<&str> = out.lines().filter(|l| !l.contains(' ')).collect();
    assert_eq!(bare, ["5.0", "4.0"], "the caller's array is not reordered");
    exactly(&out, "even_median", 4.0);
    exactly(&out, "even_q0", 1.0);
    exactly(&out, "even_q25", 3.0);
    exactly(&out, "even_q90", 8.0);
    exactly(&out, "even_q100", 9.0);
    exactly(&out, "two", 15.0);
    exactly(&out, "one_median", 42.0);
    exactly(&out, "one_q", 42.0);
    for label in ["above", "below", "not_a_number"] {
        null(&out, label);
    }
}

/// The selection over two thousand values with many duplicates, an already
/// sorted array, a reversed one and a constant one: the inputs a careless
/// partition goes quadratic or loops on.
#[test]
fn selection_over_large_and_degenerate_inputs() {
    let out = prints(
        "select",
        &format!(
            "use stats (mean, variance, median, quantile, min, max)

{SHARED}
def main():
    let odd be lcg(2001, 12345)
    show(\"odd_median\", median(&odd))
    show(\"odd_q10\", quantile(&odd, 0.1))
    show(\"odd_q90\", quantile(&odd, 0.9))
    show(\"odd_min\", min(&odd))
    show(\"odd_max\", max(&odd))
    show(\"odd_mean\", mean(&odd))
    show(\"odd_var\", variance(&odd, 1))
    let even be lcg(2000, 7)
    show(\"even_median\", median(&even))
    show(\"even_q25\", quantile(&even, 0.25))
    show(\"even_q975\", quantile(&even, 0.975))
    let mutable up be Array[F64].new()
    let mutable down be Array[F64].new()
    let mutable flat be Array[F64].new()
    for i in 0..1500:
        up.push(i as F64)
        down.push((1499 - i) as F64)
        flat.push(6.0)
    show(\"up\", median(&up))
    show(\"down\", median(&down))
    show(\"flat\", median(&flat))
    show(\"up_q\", quantile(&up, 0.001))
"
        ),
    );
    // numpy on the same generator, written in Python.
    exactly(&out, "odd_median", 63.25);
    exactly(&out, "odd_q10", 12.375);
    exactly(&out, "odd_q90", 112.75);
    exactly(&out, "odd_min", 0.0);
    exactly(&out, "odd_max", 124.875);
    close(&out, "odd_mean", 62.615067466266865);
    close(&out, "odd_var", 1325.983221607946);
    exactly(&out, "even_median", 60.625);
    exactly(&out, "even_q25", 30.375);
    exactly(&out, "even_q975", 122.125);
    // 0..1500: the median is 749.5 and the 0.1 % quantile is 0.001 * 1499.
    exactly(&out, "up", 749.5);
    exactly(&out, "down", 749.5);
    exactly(&out, "flat", 6.0);
    close(&out, "up_q", 1.499);
}

/// Welford's accumulator against exact values, and `merge` against one
/// accumulator that saw everything.
#[test]
fn welford_agrees_with_the_batch_functions() {
    let out = prints(
        "welford",
        &format!(
            "use stats (Welford)

{SHARED}
def main():
    let empty be Welford.new()
    print(empty.count())
    show(\"e_mean\", empty.mean())
    show(\"e_var\", empty.variance(0))
    show(\"e_std\", empty.std(0))
    let data be lcg(2001, 12345)
    let mutable all be Welford.new()
    let mutable left be Welford.new()
    let mutable right be Welford.new()
    for i in 0..data.length():
        all.add(data[i])
        if i < 700:
            left.add(data[i])
        else:
            right.add(data[i])
    print(all.count())
    show(\"mean\", all.mean())
    show(\"var1\", all.variance(1))
    show(\"std1\", all.std(1))
    left.merge(right)
    print(left.count())
    show(\"m_mean\", left.mean())
    show(\"m_var1\", left.variance(1))
    let mutable fresh be Welford.new()
    fresh.merge(left)
    show(\"f_var1\", fresh.variance(1))
    fresh.merge(Welford.new())
    print(fresh.count())
    let mutable shifted be Welford.new()
    shifted.add(1000000004.0)
    shifted.add(1000000007.0)
    shifted.add(1000000013.0)
    shifted.add(1000000016.0)
    show(\"s_var1\", shifted.variance(1))
    show(\"s_var0\", shifted.variance(0))
    show(\"s_over\", shifted.variance(4))
    let mutable poisoned be Welford.new()
    poisoned.add(1.0)
    poisoned.add(F64.NAN)
    poisoned.add(3.0)
    show(\"p_mean\", poisoned.mean())
    show(\"p_var\", poisoned.variance(1))
"
        ),
    );
    null(&out, "e_mean");
    null(&out, "e_var");
    null(&out, "e_std");
    // `fractions` over the generator's 2001 values.
    close(&out, "mean", 62.615067466266865);
    close(&out, "var1", 1325.983221607946);
    close(&out, "std1", 1325.983221607946_f64.sqrt());
    close(&out, "m_mean", 62.615067466266865);
    close(&out, "m_var1", 1325.983221607946);
    close(&out, "f_var1", 1325.983221607946);
    // Welford is one pass and loses nothing to the offset either.
    exactly(&out, "s_var1", 30.0);
    exactly(&out, "s_var0", 22.5);
    null(&out, "s_over");
    nan(&out, "p_mean");
    nan(&out, "p_var");
    let counts: Vec<&str> = out.lines().filter(|l| !l.contains(' ')).collect();
    assert_eq!(counts, ["0", "2001", "2001", "2001"]);
}
