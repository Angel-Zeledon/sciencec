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
use linalg (identity, transpose, matmul, dot, trace, determinant, lu, solve, inverse, cholesky, qr, norm_frobenius, norm_1, norm_inf, norm_2, LinalgError)

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
    scalar(\"singk\", kind_of(serr) as F64)
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
