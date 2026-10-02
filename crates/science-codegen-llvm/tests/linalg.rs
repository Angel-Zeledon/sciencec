//! The bundled `linalg` module, **built, linked, run**.
//!
//! `linalg.science` is dense `F64` linear algebra over `ndarray`'s `NdArray`:
//! identity, transpose, products, trace, determinant, LU with partial
//! pivoting, `solve`, `inverse`, Cholesky, Householder QR and four norms.
//!
//! # Where the expected numbers come from
//!
//! Not from the module. Two kinds of check are made, both on programs that are
//! compiled and run:
//!
//! - **Known answers**, written out here from the mathematics: the determinant
//!   of `[[2,1,1],[4,3,3],[8,7,9]]` is 4, the inverse of `[[4,7],[2,6]]` is
//!   `[[0.6,-0.7],[-0.2,0.4]]`, the Cholesky factor of the classic
//!   `[[4,12,-16],[12,37,-43],[-16,-43,98]]` is `[[2,0,0],[6,1,0],[-8,5,3]]`.
//! - **Identities**, which hold for any correct answer: `P*A = L*U`,
//!   `A*x = b`, `A*inv(A) = I`, `L*L^T = A`, `Q*R = A` and `Q^T*Q = I`. The
//!   program computes the Frobenius norm of the difference with `linalg`'s
//!   own `matmul` (itself pinned exactly by the product test) and prints it;
//!   it is compared to zero at a tolerance.
//!
//! # Tolerances
//!
//! Where the answer is exactly representable (small-integer products, traces,
//! norms of integer matrices, the Cholesky factor above, whose arithmetic is
//! exact in doubles) the comparison is exact. Elsewhere it is absolute at
//! `1e-12` on well-conditioned matrices of entries of order 1 to 10, which is
//! thousands of times the rounding error a correct algorithm leaves.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("linalg", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// The imports and helpers every program shares, then `main`'s body.
///
/// `emit(label, a)` prints `label v1 v2 ...`, the elements row-major;
/// `scalar(label, x)` prints `label x`; `must` unwraps a refusal into a panic
/// so that a test expecting success fails loudly; `matrix(values, rows,
/// columns)` builds an array; `kind_of` is the `kind` of an error and -1 for
/// none; `residual(a, b)` is `||a - b||_F`.
fn program(body: &str) -> String {
    format!(
        "use ndarray (NdArray)
use linalg (identity, transpose, matmul, dot, trace, determinant, lu, solve, inverse, cholesky, qr, norm_frobenius, norm_1, norm_inf, norm_2, LinalgError, eigh, eig, svd, rank, cond, pinv, lstsq, slogdet, norm, Norm, Lstsq)

def emit(label: String, a: &NdArray):
    let mutable line be label
    for i in 0..a.data.length():
        line be f\"{{line}} {{a.data[i]}}\"
    print(line)

def scalar(label: String, x: F64):
    print(f\"{{label}} {{x}}\")

def matrix(values: Array[F64], rows: Int, columns: Int) -> NdArray:
    let result, err be NdArray.from_array(values, &[rows, columns])
    if err?:
        panic(err.message())
    result

def vector(values: Array[F64]) -> NdArray:
    let n be values.length()
    let result, err be NdArray.from_array(values, &[n])
    if err?:
        panic(err.message())
    result

def must(pair: (NdArray, LinalgError?)) -> NdArray:
    let value, err be pair
    if err?:
        panic(err.message())
    value

def kind_of(err: LinalgError?) -> Int:
    if err?:
        return err.kind
    -1

def flag(b: Bool) -> F64:
    if b:
        return 1.0
    0.0

def column(values: &Array[F64]) -> NdArray:
    let n be values.length()
    let mutable copy be Array[F64].new()
    for i in 0..n:
        copy.push(values[i])
    let result, err be NdArray.from_array(copy, &[n, 1])
    if err?:
        panic(err.message())
    result

def report(label: String, err: LinalgError?):
    if err?:
        print(f\"{{label}} {{err.kind}}\")
        print(f\"{{label}}_msg {{err.message()}}\")
    else:
        print(f\"{{label}} -1\")

def residual(a: &NdArray, b: &NdArray) -> F64:
    norm_frobenius(a - b)

def diag(values: &NdArray) -> NdArray:
    let n be values.data.length()
    let mutable result be NdArray.zeros(&[n, n])
    for i in 0..n:
        result.data[i * n + i] be values.data[i]
    result

def ascending(values: &NdArray) -> F64:
    for i in 1..values.data.length():
        if values.data[i - 1] > values.data[i]:
            return 0.0
    1.0

def descending(values: &NdArray) -> F64:
    for i in 1..values.data.length():
        if values.data[i - 1] < values.data[i]:
            return 0.0
    1.0

def gram(v: &NdArray) -> F64:
    let vt be must(transpose(v))
    let k: Int be v.dims[1]
    residual(must(matmul(vt, v)), identity(k))

{}",
        if body.starts_with("def ") { body.to_string() } else { format!("def main():\n{body}") }
    )
}

/// The numbers on the line that starts with `label`.
fn row(out: &str, label: &str) -> Vec<f64> {
    let line = out
        .lines()
        .find(|l| l.split(' ').next() == Some(label))
        .unwrap_or_else(|| panic!("no line for `{label}` in:\n{out}"));
    line.split(' ')
        .skip(1)
        .map(|t| t.parse().unwrap_or_else(|_| panic!("`{t}` in `{line}`")))
        .collect()
}

fn text<'a>(out: &'a str, label: &str) -> &'a str {
    let line = out
        .lines()
        .find(|l| l.split(' ').next() == Some(label))
        .unwrap_or_else(|| panic!("no line for `{label}` in:\n{out}"));
    &line[label.len() + 1..]
}

fn exactly(out: &str, label: &str, want: &[f64]) {
    assert_eq!(row(out, label), want, "`{label}`");
}

fn near(out: &str, label: &str, want: &[f64], tolerance: f64) {
    let got = row(out, label);
    assert_eq!(got.len(), want.len(), "`{label}`: {got:?} vs {want:?}");
    for (g, w) in got.iter().zip(want) {
        assert!((g - w).abs() <= tolerance, "`{label}`: got {got:?}, want {want:?}");
    }
}

fn small(out: &str, label: &str) {
    let got = row(out, label);
    assert_eq!(got.len(), 1, "`{label}`");
    assert!(got[0].abs() <= 1e-12, "`{label}` should be ~0, got {:e}", got[0]);
}

fn kind(out: &str, label: &str, want: f64) {
    exactly(out, label, &[want]);
}

#[test]
fn identity_transpose_products_trace() {
    let out = prints(
        "basics",
        &program(
            "    emit(\"eye\", identity(3))
    emit(\"eye0\", identity(0))
    let a be matrix([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3)
    emit(\"t\", must(transpose(a)))
    let b be matrix([1.0, 0.0, 2.0, -1.0, 3.0, 1.0], 3, 2)
    emit(\"ab\", must(matmul(a, b)))
    emit(\"ai\", must(matmul(a, identity(3))))
    emit(\"ia\", must(matmul(identity(2), a)))
    let u be vector([1.0, 2.0, 3.0])
    let v be vector([4.0, -5.0, 6.0])
    let d, derr be dot(u, v)
    scalar(\"dot\", d)
    scalar(\"dotk\", kind_of(derr) as F64)
    let w be vector([1.0, 2.0])
    let _bad, berr be dot(u, w)
    scalar(\"dotbad\", kind_of(berr) as F64)
    let _m, merr be matmul(a, a)
    scalar(\"mmbad\", kind_of(merr) as F64)
    let sq be matrix([1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 10.0], 3, 3)
    let tr, terr be trace(sq)
    scalar(\"trace\", tr)
    scalar(\"tracek\", kind_of(terr) as F64)
    let tr2, _t2 be trace(a)
    scalar(\"trace_rect\", tr2)
    let _tv, tverr be trace(u)
    scalar(\"trace_vec\", kind_of(tverr) as F64)
    let _tt, tterr be transpose(NdArray.zeros(&[2, 2, 2]))
    scalar(\"transpose3\", kind_of(tterr) as F64)
",
        ),
    );
    exactly(&out, "eye", &[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
    exactly(&out, "eye0", &[]);
    exactly(&out, "t", &[1.0, 4.0, 2.0, 5.0, 3.0, 6.0]);
    // [[1,2,3],[4,5,6]] * [[1,0],[2,-1],[3,1]] = [[14,1],[32,1]]... computed by hand:
    // row0: 1+4+9=14, 0-2+3=1; row1: 4+10+18=32, 0-5+6=1.
    exactly(&out, "ab", &[14.0, 1.0, 32.0, 1.0]);
    exactly(&out, "ai", &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    exactly(&out, "ia", &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    exactly(&out, "dot", &[12.0]); // 4 - 10 + 18
    kind(&out, "dotk", -1.0);
    kind(&out, "dotbad", 4.0); // SHAPE_MISMATCH
    kind(&out, "mmbad", 4.0);
    exactly(&out, "trace", &[16.0]); // 1 + 5 + 10
    kind(&out, "tracek", -1.0);
    exactly(&out, "trace_rect", &[6.0]); // 1 + 5
    kind(&out, "trace_vec", 5.0); // BAD_RANK
    kind(&out, "transpose3", 5.0);
}

#[test]
fn determinants() {
    let out = prints(
        "det",
        &program(
            "    let a be matrix([2.0, 1.0, 1.0, 4.0, 3.0, 3.0, 8.0, 7.0, 9.0], 3, 3)
    let d, _e1 be determinant(a)
    scalar(\"det3\", d)
    let b be matrix([1.0, 2.0, 3.0, 4.0], 2, 2)
    let d2, _e2 be determinant(b)
    scalar(\"det2\", d2)
    let swap be matrix([0.0, 1.0, 1.0, 0.0], 2, 2)
    let d3, _e3 be determinant(swap)
    scalar(\"swap\", d3)
    let d4, _e4 be determinant(identity(5))
    scalar(\"eye\", d4)
    let sing be matrix([1.0, 2.0, 2.0, 4.0], 2, 2)
    let d5, e5 be determinant(sing)
    scalar(\"sing2\", d5)
    scalar(\"sing2k\", kind_of(e5) as F64)
    let nine be matrix([1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0], 3, 3)
    let d6, _e6 be determinant(nine)
    scalar(\"sing3\", d6)
    let z be matrix([0.0, 0.0, 0.0, 0.0], 2, 2)
    let d7, _e7 be determinant(z)
    scalar(\"zero\", d7)
    let four be matrix([4.0, 3.0, 2.0, 1.0, 3.0, 5.0, 1.0, 2.0, 2.0, 1.0, 6.0, 3.0, 1.0, 2.0, 3.0, 7.0], 4, 4)
    let d8, _e8 be determinant(four)
    scalar(\"det4\", d8)
    let diag be matrix([2.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, -4.0], 3, 3)
    let d9, _e9 be determinant(diag)
    scalar(\"diag\", d9)
    let rect be matrix([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3)
    let _d10, e10 be determinant(rect)
    scalar(\"rectk\", kind_of(e10) as F64)
    let scaled be matrix([1e-8, 0.0, 0.0, 2e-8], 2, 2)
    let d11, e11 be determinant(scaled)
    scalar(\"tiny\", d11 * 1e16)
    scalar(\"tinyk\", kind_of(e11) as F64)
",
        ),
    );
    near(&out, "det3", &[4.0], 1e-12);
    near(&out, "det2", &[-2.0], 1e-12);
    exactly(&out, "swap", &[-1.0]);
    exactly(&out, "eye", &[1.0]);
    exactly(&out, "sing2", &[0.0]);
    kind(&out, "sing2k", -1.0); // singular is a determinant of 0, not an error
    exactly(&out, "sing3", &[0.0]);
    exactly(&out, "zero", &[0.0]);
    // numpy.linalg.det of this matrix is 252.00000000000003.
    near(&out, "det4", &[252.0], 1e-9);
    exactly(&out, "diag", &[-24.0]);
    kind(&out, "rectk", 0.0); // NOT_SQUARE
    // A badly scaled but regular matrix is not called singular.
    near(&out, "tiny", &[2.0], 1e-12);
    kind(&out, "tinyk", -1.0);
}

#[test]
fn lu_factors_reconstruct_the_matrix() {
    let out = prints(
        "lu",
        &program(
            "    let a be matrix([1.0, 2.0, 3.0, 4.0, 2.0, 1.0, 0.0, 5.0, 6.0, 3.0, 1.0, 2.0, 0.0, 7.0, 8.0, 9.0], 4, 4)
    let f, err be lu(a)
    if err?:
        panic(err.message())
    let l be f.lower()
    let u be f.upper()
    let p be f.permutation()
    scalar(\"recon\", residual(must(matmul(p, a)), must(matmul(l, u))))
    scalar(\"singular\", flag(f.singular))
    scalar(\"size\", f.size() as F64)
    scalar(\"sign\", f.sign)
    emit(\"lower\", l)
    emit(\"upper\", u)
    let mutable perm_line be \"perm\"
    for i in 0..f.perm.length():
        perm_line be f\"{perm_line} {f.perm[i]}\"
    print(perm_line)
    # |multipliers| <= 1 under partial pivoting.
    let mutable biggest be 0.0
    for i in 0..16:
        if i / 4 > i % 4 and l.data[i].abs() > biggest:
            biggest be l.data[i].abs()
    scalar(\"multipliers\", biggest)
    # det(P) is the sign: the product of U's diagonal times it is det(A).
    let d, _e be determinant(a)
    scalar(\"det_match\", d - f.sign * u.data[0] * u.data[5] * u.data[10] * u.data[15])
    # A matrix that needs no pivot has the identity permutation.
    let b be matrix([4.0, 3.0, 6.0, 3.0], 2, 2)
    let g, _err2 be lu(b)
    emit(\"b_lu\", g.lu)
    scalar(\"b_sign\", g.sign)
    # A singular matrix factors, flagged.
    let c be matrix([1.0, 2.0, 2.0, 4.0], 2, 2)
    let h, err3 be lu(c)
    scalar(\"c_singular\", flag(h.singular))
    scalar(\"c_kind\", kind_of(err3) as F64)
    let r be matrix([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3)
    let _bad, err4 be lu(r)
    scalar(\"r_kind\", kind_of(err4) as F64)
",
        ),
    );
    small(&out, "recon");
    exactly(&out, "singular", &[0.0]);
    exactly(&out, "size", &[4.0]);
    let l = row(&out, "lower");
    let u = row(&out, "upper");
    for i in 0..4 {
        assert_eq!(l[i * 4 + i], 1.0, "L has a unit diagonal");
        for j in 0..4 {
            if j > i {
                assert_eq!(l[i * 4 + j], 0.0, "L is lower");
            }
            if j < i {
                assert_eq!(u[i * 4 + j], 0.0, "U is upper");
            }
        }
    }
    // Column 0 of A is (1, 4, 6, 0): the pivot is row 2, so perm starts 2.
    assert_eq!(row(&out, "perm")[0], 2.0);
    let mut perm = row(&out, "perm");
    perm.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(perm, [0.0, 1.0, 2.0, 3.0], "a permutation");
    let sign = row(&out, "sign")[0];
    assert!(sign == 1.0 || sign == -1.0);
    assert!(row(&out, "multipliers")[0] <= 1.0);
    small(&out, "det_match");
    // [[4,3],[6,3]]: pivot 6 (swap), multiplier 4/6, U = [[6,3],[0,3-2]].
    near(&out, "b_lu", &[6.0, 3.0, 2.0 / 3.0, 1.0], 1e-15);
    exactly(&out, "b_sign", &[-1.0]);
    exactly(&out, "c_singular", &[1.0]);
    kind(&out, "c_kind", -1.0);
    kind(&out, "r_kind", 0.0);
}

#[test]
fn solving_linear_systems() {
    let out = prints(
        "solve",
        &program(
            "    let a be matrix([2.0, 1.0, 1.0, 4.0, 3.0, 3.0, 8.0, 7.0, 9.0], 3, 3)
    # x = (1, 2, 3) gives b = A x = (7, 19, 49).
    let b_vec be vector([7.0, 19.0, 49.0])
    let x, err be solve(a, b_vec)
    scalar(\"k\", kind_of(err) as F64)
    emit(\"x\", x)
    scalar(\"xrank\", x.dims.length() as F64)
    scalar(\"xlen\", x.dims[0] as F64)
    # Several right-hand sides at once: the columns are b and 2b.
    let two be matrix([7.0, 14.0, 19.0, 38.0, 49.0, 98.0], 3, 2)
    let xs, err2 be solve(a, two)
    scalar(\"k2\", kind_of(err2) as F64)
    emit(\"xs\", xs)
    scalar(\"resid2\", residual(must(matmul(a, xs)), two))
    # A pivot is needed: the leading entry is zero.
    let p be matrix([0.0, 2.0, 1.0, 1.0, 0.0, 3.0, 4.0, 1.0, 0.0], 3, 3)
    let pb be vector([5.0, 10.0, 6.0])
    let px, perr be solve(p, pb)
    scalar(\"pk\", kind_of(perr) as F64)
    scalar(\"presid\", residual(must(matmul(p, column(px.data))), column(pb.data)))
    # A 6x6 diagonally dominant system.
    let mutable big be Array[F64].new()
    for i in 0..6:
        for j in 0..6:
            if i is j:
                big.push(10.0 + (i as F64))
            else:
                big.push(1.0 / ((i + j + 1) as F64))
    let bigm be matrix(big, 6, 6)
    let rhs be vector([1.0, -2.0, 3.0, -4.0, 5.0, -6.0])
    let bx, berr be solve(bigm, rhs)
    scalar(\"bigk\", kind_of(berr) as F64)
    scalar(\"bigresid\", residual(must(matmul(bigm, column(bx.data))), column(rhs.data)))
    # Refusals.
    let sing be matrix([1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0], 3, 3)
    let _s, serr be solve(sing, b_vec)
    report(\"singk\", serr)
    let _r, rerr be solve(matrix([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3), vector([1.0, 2.0]))
    scalar(\"rectk\", kind_of(rerr) as F64)
    let _m, merr be solve(a, vector([1.0, 2.0]))
    report(\"mismatchk\", merr)
    let _t, terr be solve(a, NdArray.zeros(&[3, 1, 1]))
    scalar(\"rank3k\", kind_of(terr) as F64)
",
        ),
    );
    kind(&out, "k", -1.0);
    near(&out, "x", &[1.0, 2.0, 3.0], 1e-12);
    exactly(&out, "xrank", &[1.0]);
    exactly(&out, "xlen", &[3.0]);
    kind(&out, "k2", -1.0);
    near(&out, "xs", &[1.0, 2.0, 2.0, 4.0, 3.0, 6.0], 1e-12);
    small(&out, "resid2");
    kind(&out, "pk", -1.0);
    small(&out, "presid");
    kind(&out, "bigk", -1.0);
    small(&out, "bigresid");
    kind(&out, "singk", 1.0); // SINGULAR
    assert_eq!(text(&out, "singk_msg"), "solve: the matrix is singular");
    kind(&out, "rectk", 0.0); // NOT_SQUARE
    kind(&out, "mismatchk", 4.0); // SHAPE_MISMATCH
    assert_eq!(
        text(&out, "mismatchk_msg"),
        "solve needs b to have 3 rows, not dims (2)"
    );
    kind(&out, "rank3k", 4.0);
}

#[test]
fn inverses() {
    let out = prints(
        "inverse",
        &program(
            "    let a be matrix([4.0, 7.0, 2.0, 6.0], 2, 2)
    let inv, err be inverse(a)
    scalar(\"k\", kind_of(err) as F64)
    emit(\"inv\", inv)
    let b be matrix([2.0, 1.0, 1.0, 4.0, 3.0, 3.0, 8.0, 7.0, 9.0], 3, 3)
    let binv be must(inverse(b))
    scalar(\"right\", residual(must(matmul(b, binv)), identity(3)))
    scalar(\"left\", residual(must(matmul(binv, b)), identity(3)))
    # The inverse of the inverse is the matrix.
    scalar(\"twice\", residual(must(inverse(binv)), b))
    # A permutation matrix's inverse is its transpose.
    let swap be matrix([0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0], 3, 3)
    scalar(\"perm\", residual(must(inverse(swap)), must(transpose(swap))))
    let _s, serr be inverse(matrix([1.0, 2.0, 2.0, 4.0], 2, 2))
    report(\"singk\", serr)
    let _r, rerr be inverse(matrix([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3))
    scalar(\"rectk\", kind_of(rerr) as F64)
    let one, oerr be inverse(matrix([4.0], 1, 1))
    scalar(\"onek\", kind_of(oerr) as F64)
    emit(\"one\", one)
",
        ),
    );
    kind(&out, "k", -1.0);
    near(&out, "inv", &[0.6, -0.7, -0.2, 0.4], 1e-14);
    small(&out, "right");
    small(&out, "left");
    small(&out, "twice");
    small(&out, "perm");
    kind(&out, "singk", 1.0);
    assert_eq!(text(&out, "singk_msg"), "inverse: the matrix is singular");
    kind(&out, "rectk", 0.0);
    kind(&out, "onek", -1.0);
    exactly(&out, "one", &[0.25]);
}

#[test]
fn cholesky_factors() {
    let out = prints(
        "cholesky",
        &program(
            "    let a be matrix([4.0, 12.0, -16.0, 12.0, 37.0, -43.0, -16.0, -43.0, 98.0], 3, 3)
    let l, err be cholesky(a)
    scalar(\"k\", kind_of(err) as F64)
    emit(\"l\", l)
    scalar(\"recon\", residual(must(matmul(l, must(transpose(l)))), a))
    # A Gram matrix B^T B of a full-rank B is positive definite.
    let bm be matrix([1.0, 2.0, 0.0, 3.0, 1.0, 4.0, 2.0, 2.0, 5.0, 0.0, 1.0, 1.0], 4, 3)
    let gram be must(matmul(must(transpose(bm)), bm))
    let g, gerr be cholesky(gram)
    scalar(\"gk\", kind_of(gerr) as F64)
    scalar(\"grecon\", residual(must(matmul(g, must(transpose(g)))), gram))
    # The diagonal is positive and the upper triangle zero.
    let mutable ok be 1.0
    for i in 0..3:
        if g.data[i * 3 + i] <= 0.0:
            ok be 0.0
        for j in (i + 1)..3:
            if g.data[i * 3 + j] is not 0.0:
                ok be 0.0
    scalar(\"shape_ok\", ok)
    let idl, _ierr be cholesky(identity(4))
    scalar(\"eye\", residual(idl, identity(4)))
    # Refusals.
    let _p, perr be cholesky(matrix([1.0, 2.0, 2.0, 1.0], 2, 2))
    report(\"pdk\", perr)
    let _z, zerr be cholesky(matrix([0.0, 0.0, 0.0, 0.0], 2, 2))
    scalar(\"zerok\", kind_of(zerr) as F64)
    let _n, nerr be cholesky(matrix([2.0, 1.0, 0.0, 2.0], 2, 2))
    scalar(\"asymk\", kind_of(nerr) as F64)
    let _s, serr be cholesky(matrix([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3))
    scalar(\"rectk\", kind_of(serr) as F64)
    let _neg, negerr be cholesky(matrix([-4.0], 1, 1))
    scalar(\"negk\", kind_of(negerr) as F64)
",
        ),
    );
    kind(&out, "k", -1.0);
    // The classic example: exact in doubles.
    exactly(&out, "l", &[2.0, 0.0, 0.0, 6.0, 1.0, 0.0, -8.0, 5.0, 3.0]);
    exactly(&out, "recon", &[0.0]);
    kind(&out, "gk", -1.0);
    small(&out, "grecon");
    exactly(&out, "shape_ok", &[1.0]);
    exactly(&out, "eye", &[0.0]);
    kind(&out, "pdk", 3.0); // NOT_POSITIVE_DEFINITE
    assert_eq!(
        text(&out, "pdk_msg"),
        "cholesky: the matrix is not positive definite (pivot 1)"
    );
    kind(&out, "zerok", 3.0);
    kind(&out, "asymk", 2.0); // NOT_SYMMETRIC
    kind(&out, "rectk", 0.0);
    kind(&out, "negk", 3.0);
}

#[test]
fn qr_factors() {
    let out = prints(
        "qr",
        &program(
            "def check(label: String, a: &NdArray):
    let f, err be qr(a)
    if err?:
        panic(err.message())
    let m: Int be a.dims[0]
    let n: Int be a.dims[1]
    scalar(f\"{label}_qdims\", (f.q.dims[0] * 10 + f.q.dims[1]) as F64)
    scalar(f\"{label}_rdims\", (f.r.dims[0] * 10 + f.r.dims[1]) as F64)
    scalar(f\"{label}_recon\", residual(must(matmul(f.q, f.r)), a))
    scalar(f\"{label}_orth\", residual(must(matmul(must(transpose(f.q)), f.q)), identity(m)))
    let mutable lower be 0.0
    for i in 0..m:
        for j in 0..n:
            if j < i and f.r.data[i * n + j].abs() > lower:
                lower be f.r.data[i * n + j].abs()
    scalar(f\"{label}_lower\", lower)

def main():
    let a be matrix([3.0, 0.0, 4.0, 5.0], 2, 2)
    let f, err be qr(a)
    scalar(\"k\", kind_of(err) as F64)
    emit(\"q\", f.q)
    emit(\"r\", f.r)
    check(\"sq\", matrix([12.0, -51.0, 4.0, 6.0, 167.0, -68.0, -4.0, 24.0, -41.0], 3, 3))
    check(\"tall\", matrix([1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 10.0, 1.0, 0.0, 2.0], 4, 3))
    check(\"wide\", matrix([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3))
    check(\"col\", matrix([3.0, 4.0, 0.0], 3, 1))
    check(\"eye\", identity(3))
    check(\"zero\", NdArray.zeros(&[3, 2]))
    check(\"deficient\", matrix([1.0, 2.0, 2.0, 4.0, 3.0, 6.0], 3, 2))
    let g be matrix([12.0, -51.0, 4.0, 6.0, 167.0, -68.0, -4.0, 24.0, -41.0], 3, 3)
    let h, _herr be qr(g)
    emit(\"absr\", h.r)
    let _v, verr be qr(vector([1.0, 2.0]))
    scalar(\"vk\", kind_of(verr) as F64)
",
        ),
    );
    kind(&out, "k", -1.0);
    // numpy: Q = [[-0.6, -0.8], [-0.8, 0.6]], R = [[-5, -4], [0, 3]].
    near(&out, "q", &[-0.6, -0.8, -0.8, 0.6], 1e-14);
    near(&out, "r", &[-5.0, -4.0, 0.0, 3.0], 1e-14);
    for (label, q, r) in [
        ("sq", 33.0, 33.0),
        ("tall", 44.0, 43.0),
        ("wide", 22.0, 23.0),
        ("col", 33.0, 31.0),
        ("eye", 33.0, 33.0),
        ("zero", 33.0, 32.0),
        ("deficient", 33.0, 32.0),
    ] {
        exactly(&out, &format!("{label}_qdims"), &[q]);
        exactly(&out, &format!("{label}_rdims"), &[r]);
        small(&out, &format!("{label}_recon"));
        small(&out, &format!("{label}_orth"));
        exactly(&out, &format!("{label}_lower"), &[0.0]);
    }
    // numpy: |R| of the classic Wikipedia matrix is [[14,21,14],[0,175,70],[0,0,35]].
    let r = row(&out, "absr");
    let want = [14.0, 21.0, 14.0, 0.0, 175.0, 70.0, 0.0, 0.0, 35.0];
    for (g, w) in r.iter().zip(want) {
        assert!((g.abs() - w).abs() <= 1e-10, "|R|: {r:?}");
    }
    kind(&out, "vk", 5.0);
}

#[test]
fn norms() {
    let out = prints(
        "norms",
        &program(
            "    let a be matrix([1.0, -2.0, 3.0, -4.0], 2, 2)
    scalar(\"fro\", norm_frobenius(a))
    let n1, e1 be norm_1(a)
    scalar(\"one\", n1)
    let ni, e2 be norm_inf(a)
    scalar(\"inf\", ni)
    scalar(\"k\", (kind_of(e1) + kind_of(e2)) as F64)
    let v be vector([3.0, -4.0])
    let n2, e3 be norm_2(v)
    scalar(\"two\", n2)
    scalar(\"two_k\", kind_of(e3) as F64)
    scalar(\"fro_vec\", norm_frobenius(v))
    scalar(\"fro_rank3\", norm_frobenius(NdArray.ones(&[2, 2, 2])))
    scalar(\"fro_empty\", norm_frobenius(NdArray.zeros(&[0])))
    let r be matrix([1.0, 2.0, 3.0, -4.0, 5.0, -6.0], 2, 3)
    let r1, _e4 be norm_1(r)
    let ri, _e5 be norm_inf(r)
    scalar(\"rect_one\", r1)
    scalar(\"rect_inf\", ri)
    let _a, bad1 be norm_1(v)
    let _b, bad2 be norm_inf(v)
    let _c, bad3 be norm_2(a)
    scalar(\"bad\", (kind_of(bad1) * 100 + kind_of(bad2) * 10 + kind_of(bad3)) as F64)
    # ||R||_F^2 = trace(R^T R), and ||R||_1 = ||R^T||_inf.
    let tr, _te be trace(must(matmul(must(transpose(r)), r)))
    scalar(\"trace_id\", tr - norm_frobenius(r) * norm_frobenius(r))
    let ta be must(transpose(r))
    let tinf, _te2 be norm_inf(ta)
    scalar(\"transpose_id\", r1 - tinf)
",
        ),
    );
    near(&out, "fro", &[30.0_f64.sqrt()], 1e-15);
    exactly(&out, "one", &[6.0]); // max(|1|+|3|, |-2|+|-4|)
    exactly(&out, "inf", &[7.0]); // max(|1|+|-2|, |3|+|-4|)
    exactly(&out, "k", &[-2.0]); // both -1: no error
    exactly(&out, "two", &[5.0]);
    exactly(&out, "two_k", &[-1.0]);
    exactly(&out, "fro_vec", &[5.0]);
    near(&out, "fro_rank3", &[8.0_f64.sqrt()], 1e-15);
    exactly(&out, "fro_empty", &[0.0]);
    // [[1,2,3],[-4,5,-6]]: columns 5,7,9 and rows 6,15.
    exactly(&out, "rect_one", &[9.0]);
    exactly(&out, "rect_inf", &[15.0]);
    exactly(&out, "bad", &[555.0]); // BAD_RANK = 5, three times
    small(&out, "trace_id");
    exactly(&out, "transpose_id", &[0.0]);
}

/// Order-insensitive comparison: both sides sorted, then within `tolerance`.
fn near_sorted(out: &str, label: &str, want: &[f64], tolerance: f64) {
    let mut got = row(out, label);
    let mut want = want.to_vec();
    got.sort_by(|a, b| a.partial_cmp(b).unwrap());
    want.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(got.len(), want.len(), "`{label}`: {got:?} vs {want:?}");
    for (g, w) in got.iter().zip(&want) {
        assert!((g - w).abs() <= tolerance, "`{label}`: got {got:?}, want {want:?}");
    }
}

/// Zero to `1e-10`, the bound the spectral identities are held to.
fn tiny(out: &str, label: &str) {
    let got = row(out, label);
    assert_eq!(got.len(), 1, "`{label}`");
    assert!(got[0].abs() <= 1e-10, "`{label}` should be ~0, got {:e}", got[0]);
}

#[test]
fn eigh_decomposes_symmetric_matrices() {
    let out = prints(
        "eigh",
        &program(
            "def check(label: String, a: &NdArray):
    let f, err be eigh(a)
    if err?:
        panic(err.message())
    let av be must(matmul(a, f.vectors))
    let vl be must(matmul(f.vectors, diag(f.values)))
    scalar(f\"{label}_av\", residual(av, vl))
    scalar(f\"{label}_orth\", gram(f.vectors))
    scalar(f\"{label}_asc\", ascending(f.values))
    emit(f\"{label}_values\", f.values)

def main():
    check(\"a\", matrix([4.0, 1.0, 2.0, 1.0, 3.0, 0.0, 2.0, 0.0, 5.0], 3, 3))
    check(\"c\", matrix([2.0, -1.0, 0.0, 0.0, 1.0, -1.0, 2.0, -1.0, 0.0, 0.0, 0.0, -1.0, 3.0, -1.0, 0.0, 0.0, 0.0, -1.0, 2.0, -1.0, 1.0, 0.0, 0.0, -1.0, 4.0], 5, 5))
    check(\"repeated\", matrix([2.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 1.0], 3, 3))
    check(\"dense_repeated\", matrix([1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0], 3, 3))
    check(\"eye\", identity(4))
    check(\"zero\", NdArray.zeros(&[3, 3]))
    check(\"one\", matrix([7.0], 1, 1))
    check(\"scaled\", matrix([1e8, 3e7, 3e7, 2e8], 2, 2))
    let _f1, e1 be eigh(matrix([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3))
    scalar(\"notsq\", kind_of(e1) as F64)
    let _f2, e2 be eigh(matrix([1.0, 2.0, 3.0, 4.0], 2, 2))
    scalar(\"notsym\", kind_of(e2) as F64)
    let _f3, e3 be eigh(vector([1.0, 2.0]))
    scalar(\"vec\", kind_of(e3) as F64)
",
        ),
    );
    for label in ["a", "c", "repeated", "dense_repeated", "eye", "zero", "one", "scaled"] {
        let scale = if label == "scaled" { 1e8 } else { 1.0 };
        let av = row(&out, &format!("{label}_av"))[0];
        assert!(av.abs() <= 1e-10 * scale, "`{label}` A*V - V*L = {av:e}");
        tiny(&out, &format!("{label}_orth"));
        exactly(&out, &format!("{label}_asc"), &[1.0]);
    }
    // numpy.linalg.eigh.
    near(&out, "a_values", &[1.8548973087995773, 3.476023602918134, 6.6690790882822855], 1e-12);
    near(
        &out,
        "c_values",
        &[0.6201218766157858, 1.0000000000000007, 2.7086331097644316, 3.7510241439497927, 4.920220869669986],
        1e-12,
    );
    near(&out, "repeated_values", &[1.0, 2.0, 2.0], 1e-14);
    near(&out, "dense_repeated_values", &[0.0, 0.0, 3.0], 1e-14);
    near(&out, "zero_values", &[0.0, 0.0, 0.0], 0.0);
    near(&out, "one_values", &[7.0], 0.0);
    // [[1e8,3e7],[3e7,2e8]]: trace 3e8, det 2e16 - 9e14 = 1.91e16.
    let s = row(&out, "scaled_values");
    assert!((s[0] + s[1] - 3e8).abs() <= 1e-4 && (s[0] * s[1] - 1.91e16).abs() <= 1e6, "{s:?}");
    kind(&out, "notsq", 0.0);
    kind(&out, "notsym", 2.0);
    kind(&out, "vec", 0.0);
}

#[test]
fn svd_reconstructs_and_orders() {
    let out = prints(
        "svd",
        &program(
            "def check(label: String, a: &NdArray):
    let d, err be svd(a)
    if err?:
        panic(err.message())
    let k: Int be d.s.data.length()
    scalar(f\"{label}_k\", (d.u.dims[0] * 100 + d.u.dims[1] * 10 + d.vt.dims[0]) as F64)
    scalar(f\"{label}_vtcols\", d.vt.dims[1] as F64)
    let usv be must(matmul(must(matmul(d.u, diag(d.s))), d.vt))
    scalar(f\"{label}_recon\", residual(usv, a))
    scalar(f\"{label}_uorth\", gram(d.u))
    scalar(f\"{label}_vorth\", gram(must(transpose(d.vt))))
    scalar(f\"{label}_desc\", descending(d.s))
    let mutable negative be 0.0
    for i in 0..k:
        if d.s.data[i] < 0.0:
            negative be 1.0
    scalar(f\"{label}_neg\", negative)
    emit(f\"{label}_s\", d.s)

def main():
    check(\"tall\", matrix([1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], 4, 2))
    check(\"wide\", matrix([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3))
    check(\"square\", matrix([1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0], 3, 3))
    check(\"zero\", NdArray.zeros(&[3, 2]))
    check(\"col\", matrix([3.0, 4.0, 0.0], 3, 1))
    check(\"row\", matrix([3.0, 4.0, 0.0], 1, 3))
    check(\"eye\", identity(3))
    check(\"scaled\", matrix([1e-9, 2e-9, 3e-9, 4e-9], 2, 2))
    check(\"rank1\", matrix([1.0, 2.0, 2.0, 4.0, 3.0, 6.0], 3, 2))
    let _d, e be svd(vector([1.0, 2.0]))
    scalar(\"vec\", kind_of(e) as F64)
",
        ),
    );
    for (label, shape, vtcols) in [
        ("tall", 422.0, 2.0),
        ("wide", 222.0, 3.0),
        ("square", 333.0, 3.0),
        ("zero", 322.0, 2.0),
        ("col", 311.0, 1.0),
        ("row", 111.0, 3.0),
        ("eye", 333.0, 3.0),
        ("scaled", 222.0, 2.0),
        ("rank1", 322.0, 2.0),
    ] {
        exactly(&out, &format!("{label}_k"), &[shape]);
        exactly(&out, &format!("{label}_vtcols"), &[vtcols]);
        let scale = if label == "scaled" { 1e-9 } else { 1.0 };
        let recon = row(&out, &format!("{label}_recon"))[0];
        assert!(recon.abs() <= 1e-10 * scale, "`{label}` U S Vt - A = {recon:e}");
        tiny(&out, &format!("{label}_uorth"));
        tiny(&out, &format!("{label}_vorth"));
        exactly(&out, &format!("{label}_desc"), &[1.0]);
        exactly(&out, &format!("{label}_neg"), &[0.0]);
    }
    // numpy.linalg.svd.
    near(&out, "tall_s", &[14.269095499261486, 0.6268282324175426], 1e-12);
    near(&out, "wide_s", &[9.508032000695724, 0.7728696356734844], 1e-12);
    let s = row(&out, "square_s");
    assert!((s[0] - 16.84810335261421).abs() <= 1e-12 && (s[1] - 1.0683695145547092).abs() <= 1e-12, "{s:?}");
    assert!(s[2] <= 1e-14, "the third singular value of [[1..9]] is ~0: {s:?}");
    near(&out, "zero_s", &[0.0, 0.0], 0.0);
    near(&out, "col_s", &[5.0], 1e-14);
    near(&out, "eye_s", &[1.0, 1.0, 1.0], 1e-14);
    kind(&out, "vec", 5.0);
}

#[test]
fn rank_cond_pinv() {
    let out = prints(
        "rank_cond_pinv",
        &program(
            "def count(a: &NdArray) -> F64:
    let r, err be rank(a)
    if err?:
        panic(err.message())
    r as F64

def condition(a: &NdArray) -> F64:
    let c, err be cond(a)
    if err?:
        panic(err.message())
    c

def penrose(label: String, a: &NdArray, rcond: F64):
    let p, err be pinv(a, rcond)
    if err?:
        panic(err.message())
    scalar(f\"{label}_dims\", (p.dims[0] * 10 + p.dims[1]) as F64)
    let apa be must(matmul(must(matmul(a, p)), a))
    let pap be must(matmul(must(matmul(p, a)), p))
    scalar(f\"{label}_apa\", residual(apa, a))
    scalar(f\"{label}_pap\", residual(pap, p))
    let ap be must(matmul(a, p))
    let pa be must(matmul(p, a))
    scalar(f\"{label}_sym1\", residual(ap, must(transpose(ap))))
    scalar(f\"{label}_sym2\", residual(pa, must(transpose(pa))))
    emit(f\"{label}\", p)

def main():
    scalar(\"rank_sing\", count(matrix([1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0], 3, 3)))
    scalar(\"rank_eye\", count(identity(3)))
    scalar(\"rank_zero\", count(NdArray.zeros(&[3, 4])))
    scalar(\"rank_tall\", count(matrix([1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], 4, 2)))
    scalar(\"rank_wide\", count(matrix([1.0, 2.0, 3.0, 2.0, 4.0, 6.0], 2, 3)))
    scalar(\"rank_one\", count(matrix([1.0, 2.0, 2.0, 4.0], 2, 2)))
    let _r, rerr be rank(vector([1.0]))
    scalar(\"rank_vec\", kind_of(rerr) as F64)
    scalar(\"cond12\", condition(matrix([1.0, 2.0, 3.0, 4.0], 2, 2)))
    scalar(\"cond_eye\", condition(identity(3)))
    scalar(\"cond_sing\", condition(matrix([1.0, 2.0, 2.0, 4.0], 2, 2)))
    scalar(\"cond_zero\", condition(NdArray.zeros(&[2, 2])))
    scalar(\"cond_diag\", condition(diag(vector([10.0, 1.0, 0.001]))))
    scalar(\"cond_wide\", condition(matrix([3.0, 0.0, 0.0, 0.0, 4.0, 0.0], 2, 3)))
    penrose(\"tall\", matrix([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 3, 2), 1e-15)
    penrose(\"wide\", matrix([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3), 1e-15)
    penrose(\"rank1\", matrix([1.0, 2.0, 2.0, 4.0], 2, 2), 1e-15)
    penrose(\"sing3\", matrix([1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0], 3, 3), 1e-10)
    penrose(\"zero\", NdArray.zeros(&[2, 3]), 1e-15)
    let inv be must(inverse(matrix([4.0, 7.0, 2.0, 6.0], 2, 2)))
    let pi, perr be pinv(matrix([4.0, 7.0, 2.0, 6.0], 2, 2), 1e-15)
    if perr?:
        panic(perr.message())
    scalar(\"inv_vs_pinv\", residual(inv, pi))
    # A cutoff above the small singular value drops it.
    let cut, cerr be pinv(matrix([3.0, 0.0, 0.0, 1e-3], 2, 2), 1e-2)
    if cerr?:
        panic(cerr.message())
    emit(\"cut\", cut)
",
        ),
    );
    kind(&out, "rank_sing", 2.0);
    kind(&out, "rank_eye", 3.0);
    kind(&out, "rank_zero", 0.0);
    kind(&out, "rank_tall", 2.0);
    kind(&out, "rank_wide", 1.0);
    kind(&out, "rank_one", 1.0);
    kind(&out, "rank_vec", 5.0);
    near(&out, "cond12", &[14.933034373659263], 1e-10);
    near(&out, "cond_eye", &[1.0], 1e-14);
    assert!(row(&out, "cond_sing")[0] > 1e15, "{out}");
    assert!(row(&out, "cond_zero")[0].is_infinite(), "{out}");
    near(&out, "cond_diag", &[10000.0], 1e-8);
    near(&out, "cond_wide", &[4.0 / 3.0], 1e-14);
    for (label, dims) in [("tall", 23.0), ("wide", 32.0), ("rank1", 22.0), ("sing3", 33.0), ("zero", 32.0)] {
        exactly(&out, &format!("{label}_dims"), &[dims]);
        for check in ["apa", "pap", "sym1", "sym2"] {
            tiny(&out, &format!("{label}_{check}"));
        }
    }
    // numpy.linalg.pinv([[1,2],[3,4],[5,6]]).
    near(
        &out,
        "tall",
        &[-1.3333333333333324, -0.3333333333333325, 0.6666666666666657, 1.0833333333333326, 0.33333333333333265, -0.41666666666666596],
        1e-12,
    );
    // The pseudo-inverse of a rank-one matrix is A^T / ||A||_F^2.
    near(&out, "rank1", &[0.04, 0.08, 0.08, 0.16], 1e-14);
    exactly(&out, "zero", &[0.0; 6]);
    tiny(&out, "inv_vs_pinv");
    near(&out, "cut", &[1.0 / 3.0, 0.0, 0.0, 0.0], 1e-14);
}

#[test]
fn least_squares() {
    let out = prints(
        "lstsq",
        &program(
            "def fit(a: &NdArray, b: &NdArray) -> Lstsq:
    let f, err be lstsq(a, b)
    if err?:
        panic(err.message())
    f

def main():
    let x be matrix([1.0, 0.0, 1.0, 1.0, 1.0, 2.0, 1.0, 3.0], 4, 2)
    let f be fit(x, vector([1.0, 3.0, 4.0, 8.0]))
    emit(\"line\", f.x)
    emit(\"line_res\", f.residuals)
    scalar(\"line_rank\", f.rank as F64)
    scalar(\"line_dims\", f.x.dims.length() as F64)
    # Normal equations hold: A^T (A x - b) = 0.
    let fitted be must(matmul(x, matrix([f.x.data[0], f.x.data[1]], 2, 1)))
    let r be fitted - column([1.0, 3.0, 4.0, 8.0])
    scalar(\"line_normal\", norm_frobenius(must(matmul(must(transpose(x)), r))))

    let exact be fit(matrix([2.0, 1.0, 1.0, 3.0], 2, 2), vector([3.0, 5.0]))
    emit(\"exact\", exact.x)
    emit(\"exact_res\", exact.residuals)

    let wide be fit(matrix([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3), vector([1.0, 2.0]))
    emit(\"wide\", wide.x)
    scalar(\"wide_rank\", wide.rank as F64)
    scalar(\"wide_res\", wide.residuals.data[0])

    let deficient be fit(matrix([1.0, 2.0, 2.0, 4.0, 3.0, 6.0], 3, 2), vector([1.0, 2.0, 4.0]))
    emit(\"def\", deficient.x)
    scalar(\"def_rank\", deficient.rank as F64)

    let many be fit(x, matrix([1.0, 0.0, 3.0, 1.0, 4.0, 2.0, 8.0, 3.0], 4, 2))
    scalar(\"many_dims\", (many.x.dims[0] * 10 + many.x.dims[1]) as F64)
    emit(\"many\", many.x)
    emit(\"many_res\", many.residuals)

    let _g, e1 be lstsq(x, vector([1.0, 2.0, 3.0]))
    scalar(\"mismatch\", kind_of(e1) as F64)
    let _h, e2 be lstsq(vector([1.0, 2.0]), vector([1.0, 2.0]))
    scalar(\"notmatrix\", kind_of(e2) as F64)
",
        ),
    );
    // numpy.linalg.lstsq(X, y): x = [0.7, 2.2], residuals = [1.8], rank 2.
    near(&out, "line", &[0.7, 2.2], 1e-12);
    near(&out, "line_res", &[1.8], 1e-12);
    kind(&out, "line_rank", 2.0);
    kind(&out, "line_dims", 1.0);
    tiny(&out, "line_normal");
    near(&out, "exact", &[0.8, 1.4], 1e-12);
    near(&out, "exact_res", &[0.0], 1e-20);
    // Minimum-norm solutions, as numpy's.
    near(&out, "wide", &[-0.055555555555555816, 0.11111111111111115, 0.27777777777777807], 1e-12);
    kind(&out, "wide_rank", 2.0);
    tiny(&out, "wide_res");
    near(&out, "def", &[0.24285714285714272, 0.48571428571428554], 1e-12);
    kind(&out, "def_rank", 1.0);
    kind(&out, "many_dims", 22.0);
    // Column 0 is the same fit as `line`; column 1 is exact (y = 0, 1, 2, 3).
    let m = row(&out, "many");
    assert!((m[0] - 0.7).abs() <= 1e-12 && (m[2] - 2.2).abs() <= 1e-12, "{m:?}");
    assert!(m[1].abs() <= 1e-12 && (m[3] - 1.0).abs() <= 1e-12, "{m:?}");
    let res = row(&out, "many_res");
    assert!((res[0] - 1.8).abs() <= 1e-12 && res[1].abs() <= 1e-20, "{res:?}");
    kind(&out, "mismatch", 4.0);
    kind(&out, "notmatrix", 5.0);
}

#[test]
fn slogdet_and_norm() {
    let out = prints(
        "slogdet_norm",
        &program(
            "def sl(label: String, a: &NdArray):
    let s, err be slogdet(a)
    if err?:
        panic(err.message())
    scalar(f\"{label}_sign\", s.sign)
    scalar(f\"{label}_log\", s.logabsdet)

def nrm(label: String, a: &NdArray, order: Norm):
    let value, err be norm(a, order)
    if err?:
        panic(err.message())
    scalar(label, value)

def main():
    sl(\"neg\", matrix([1.0, 2.0, 3.0, 4.0], 2, 2))
    sl(\"pos\", matrix([2.0, 1.0, 1.0, 4.0, 3.0, 3.0, 8.0, 7.0, 9.0], 3, 3))
    sl(\"sing\", matrix([1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0], 3, 3))
    # det = 1e600: determinant overflows to inf, slogdet does not.
    let big be diag(vector([1e200, 1e200, 1e200]))
    sl(\"big\", big)
    let d, _de be determinant(big)
    scalar(\"big_det_infinite\", flag(d > 1e300))
    sl(\"small\", diag(vector([1e-200, 1e-200, 1e-200])))
    let _s, e1 be slogdet(matrix([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3))
    scalar(\"notsq\", kind_of(e1) as F64)

    let a be matrix([4.0, 1.0, 2.0, 1.0, 3.0, 0.0, 2.0, 0.0, 5.0], 3, 3)
    nrm(\"fro\", a, Norm.Frobenius)
    nrm(\"one\", a, Norm.One)
    nrm(\"inf\", a, Norm.Infinity)
    nrm(\"two\", a, Norm.Two)
    nrm(\"nuc\", a, Norm.Nuclear)
    let v be vector([3.0, -4.0, 12.0])
    nrm(\"v_fro\", v, Norm.Frobenius)
    nrm(\"v_one\", v, Norm.One)
    nrm(\"v_inf\", v, Norm.Infinity)
    nrm(\"v_two\", v, Norm.Two)
    nrm(\"rect_two\", matrix([3.0, 0.0, 0.0, 0.0, 4.0, 0.0], 2, 3), Norm.Two)
    let _n, e2 be norm(v, Norm.Nuclear)
    scalar(\"v_nuclear\", kind_of(e2) as F64)
    let _m, e3 be norm(a, Norm.Frobenius)
    scalar(\"fro_ok\", kind_of(e3) as F64)
",
        ),
    );
    near(&out, "neg_sign", &[-1.0], 0.0);
    near(&out, "neg_log", &[0.6931471805599455], 1e-14);
    near(&out, "pos_sign", &[1.0], 0.0);
    near(&out, "pos_log", &[4.0_f64.ln()], 1e-14);
    near(&out, "sing_sign", &[0.0], 0.0);
    assert_eq!(row(&out, "sing_log")[0], f64::NEG_INFINITY);
    near(&out, "big_sign", &[1.0], 0.0);
    near(&out, "big_log", &[600.0 * std::f64::consts::LN_10], 1e-9);
    exactly(&out, "big_det_infinite", &[1.0]);
    near(&out, "small_log", &[-600.0 * std::f64::consts::LN_10], 1e-9);
    kind(&out, "notsq", 0.0);
    // numpy.linalg.norm of [[4,1,2],[1,3,0],[2,0,5]].
    near(&out, "fro", &[7.745966692414834], 1e-14);
    near(&out, "one", &[7.0], 0.0);
    near(&out, "inf", &[7.0], 0.0);
    near(&out, "two", &[6.669079088282289], 1e-12);
    near(&out, "nuc", &[12.0], 1e-12);
    near(&out, "v_fro", &[13.0], 1e-14);
    near(&out, "v_one", &[19.0], 0.0);
    near(&out, "v_inf", &[12.0], 0.0);
    near(&out, "v_two", &[13.0], 1e-14);
    near(&out, "rect_two", &[4.0], 1e-14);
    kind(&out, "v_nuclear", 5.0);
    kind(&out, "fro_ok", -1.0);
}

#[test]
fn eig_real_eigenvalues() {
    let out = prints(
        "eig",
        &program(
            "def check(label: String, a: &NdArray):
    let f, err be eig(a)
    if err?:
        panic(err.message())
    let av be must(matmul(a, f.vectors))
    let vl be must(matmul(f.vectors, diag(f.values)))
    scalar(f\"{label}_av\", residual(av, vl))
    let n: Int be a.dims[0]
    let mutable worst be 0.0
    for k in 0..n:
        let mutable length be 0.0
        for i in 0..n:
            length be length + f.vectors.data[i * n + k] * f.vectors.data[i * n + k]
        let off be (length.sqrt() - 1.0).abs()
        if off > worst:
            worst be off
    scalar(f\"{label}_unit\", worst)
    emit(f\"{label}_values\", f.values)

def main():
    check(\"b\", matrix([1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 10.0], 3, 3))
    check(\"sym\", matrix([4.0, 1.0, 2.0, 1.0, 3.0, 0.0, 2.0, 0.0, 5.0], 3, 3))
    check(\"upper\", matrix([2.0, 5.0, 7.0, 0.0, 3.0, 1.0, 0.0, 0.0, -4.0], 3, 3))
    check(\"lower\", matrix([1.0, 0.0, 0.0, 2.0, 5.0, 0.0, 3.0, 4.0, 9.0], 3, 3))
    check(\"companion\", matrix([0.0, 0.0, 6.0, 1.0, 0.0, -11.0, 0.0, 1.0, 6.0], 3, 3))
    check(\"swap\", matrix([0.0, 1.0, 1.0, 0.0], 2, 2))
    check(\"shear\", matrix([3.0, 1.0, 0.0, 3.0], 2, 2))
    check(\"eye\", identity(3))
    check(\"zero\", NdArray.zeros(&[3, 3]))
    check(\"one\", matrix([-5.0], 1, 1))
    check(\"four\", matrix([4.0, 1.0, -2.0, 2.0, 1.0, 2.0, 0.0, 1.0, 0.0, 1.0, 3.0, 3.0, 0.0, 0.0, 1.0, 5.0], 4, 4))
    let _a, e1 be eig(matrix([0.0, -1.0, 1.0, 0.0], 2, 2))
    scalar(\"rotation\", kind_of(e1) as F64)
    let _b, e2 be eig(matrix([1.0, 2.0, 0.0, 0.0, 0.0, -3.0, 0.0, 3.0, 0.0], 3, 3))
    scalar(\"mixed\", kind_of(e2) as F64)
    let _c, e3 be eig(matrix([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3))
    scalar(\"notsq\", kind_of(e3) as F64)
",
        ),
    );
    for label in ["b", "sym", "upper", "lower", "companion", "swap", "shear", "eye", "zero", "one", "four"] {
        tiny(&out, &format!("{label}_av"));
        tiny(&out, &format!("{label}_unit"));
    }
    // numpy.linalg.eigvals (order-insensitive).
    near_sorted(&out, "b_values", &[-0.9057401795217573, 0.19824686339700862, 16.707493316124747], 1e-10);
    near_sorted(&out, "sym_values", &[1.8548973087995773, 3.476023602918134, 6.6690790882822855], 1e-10);
    near_sorted(&out, "upper_values", &[2.0, 3.0, -4.0], 1e-12);
    near_sorted(&out, "lower_values", &[1.0, 5.0, 9.0], 1e-12);
    near_sorted(&out, "companion_values", &[1.0, 2.0, 3.0], 1e-10);
    near_sorted(&out, "swap_values", &[-1.0, 1.0], 1e-12);
    near_sorted(&out, "shear_values", &[3.0, 3.0], 1e-12);
    near_sorted(&out, "eye_values", &[1.0, 1.0, 1.0], 1e-14);
    near_sorted(&out, "zero_values", &[0.0, 0.0, 0.0], 0.0);
    near_sorted(&out, "one_values", &[-5.0], 0.0);
    near_sorted(&out, "four_values", &[1.1080455593360838, 2.827519906867343, 4.000000000000001, 6.064434533796575], 1e-9);
    kind(&out, "rotation", 6.0);
    kind(&out, "mixed", 6.0);
    kind(&out, "notsq", 0.0);
}
