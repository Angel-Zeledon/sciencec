//! The bundled `ndarray` module, **built, linked, run**.
//!
//! `ndarray.science` is `DREAM.md` §15's n-dimensional array of `F64` written
//! in Science, Phase 3's first module. Every expected value below was computed
//! with numpy 2.0 and is written out here, not derived by the code under test:
//! the products, sums, means and extrema of a fixed array, each broadcast of
//! `broadcasting.md` §4.3, the strings numpy prints for `arange` and
//! `linspace`. What numpy does not decide — the error sentences, the panic of
//! a same-shape operator, the display without column alignment — is the
//! module's own, and each is pinned on the input where another decision would
//! print something else.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("ndarray", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// A program: the import, two helpers every test shares, and `main`'s body.
///
/// `shaped(start, stop, dims)` is `numpy.arange(start, stop).reshape(dims)`
/// and `must` unwraps a refusal into a panic, so a test that expects success
/// fails loudly when it is refused.
fn program(body: &str) -> String {
    format!(
        "use ndarray (NdArray)

def shaped(start: F64, stop: F64, dims: &Array[Int]) -> NdArray:
    let flat be NdArray.arange(start, stop, 1.0)
    let result, err be flat.reshape(dims)
    if err?:
        panic(err.message())
    result

def must(pair: (NdArray, Error?)) -> NdArray:
    let value, err be pair
    if err?:
        panic(err.message())
    value

def main():
{}",
        body
    )
}

#[test]
fn construction() {
    // numpy: arange(1, 2, .25), arange(5, 0, -2), arange(3, 1, 1) is empty,
    // linspace(0, 1, 5), linspace(2, 3, 1).
    assert_eq!(
        prints(
            "construction",
            &program(
                "    print(NdArray.zeros(&[2, 2]))
    print(NdArray.ones(&[3]))
    print(NdArray.full(&[1, 2], 7.5))
    print(NdArray.arange(1.0, 2.0, 0.25))
    print(NdArray.arange(5.0, 0.0, -2.0))
    print(NdArray.arange(3.0, 1.0, 1.0))
    print(NdArray.linspace(0.0, 1.0, 5))
    print(NdArray.linspace(2.0, 3.0, 1))
    let a be shaped(0.0, 24.0, &[2, 3, 4])
    print(a.rank())
    print(a.size())
    print(a.dims[0] + a.dims[1] * 10 + a.dims[2] * 100)
    print(a.strides[0] + a.strides[1] * 1000 + a.strides[2] * 1000000)
",
            ),
        ),
        "[[0.0 0.0]\n [0.0 0.0]]\n\
         [1.0 1.0 1.0]\n\
         [[7.5 7.5]]\n\
         [1.0 1.25 1.5 1.75]\n\
         [5.0 3.0 1.0]\n\
         []\n\
         [0.0 0.25 0.5 0.75 1.0]\n\
         [2.0]\n\
         3\n24\n432\n1004012\n"
    );
}

/// `from_array` checks the length, and says which dims and how many values.
#[test]
fn from_array_checks_its_length() {
    assert_eq!(
        prints(
            "from_array",
            &program(
                "    let good, e1 be NdArray.from_array([1.0, 2.0, 3.0, 4.0], &[2, 2])
    if e1?:
        panic(e1)
    print(good)
    let bad, e2 be NdArray.from_array([1.0, 2.0, 3.0], &[2, 2])
    if e2?:
        print(e2.message())
        print(bad.size())
    let negative, e3 be NdArray.from_array([1.0], &[-1, -1])
    if e3?:
        print(e3.message())
        print(negative.size())
",
            ),
        ),
        "[[1.0 2.0]\n [3.0 4.0]]\n\
         3 values do not fill dims (2, 2)\n0\n\
         a dimension of (-1, -1) is negative\n0\n"
    );
}

/// Multi-index `get` and `set`, refused when the rank or a position is wrong.
/// `x?` on an `F64?` asks whether a value is there, and on an `Error?` whether
/// an error is.
#[test]
fn indexing() {
    assert_eq!(
        prints(
            "indexing",
            &program(
                "    let mutable a be shaped(0.0, 24.0, &[2, 3, 4])
    let first be a.get(&[1, 2, 3])
    if first?:
        print(first)
    let second be a.get(&[0, 1, 2])
    if second?:
        print(second)
    print(a.get(&[2, 0, 0])?)
    print(a.get(&[0, 0])?)
    print(a.get(&[0, -1, 0])?)
    print(a.get(&[0, 0, 0, 0])?)
    let ok be a.set(&[1, 0, 0], -5.0)
    print(ok?)
    let moved be a.get(&[1, 0, 0])
    if moved?:
        print(moved)
    let bad be a.set(&[1, 3, 0], 9.0)
    if bad?:
        print(bad.message())
    let mutable m be shaped(1.0, 7.0, &[2, 3])
    let e be m.set_2d(1, 1, 0.5)
    print(e?)
    print(m)
    let corner be m.get_2d(1, 2)
    if corner?:
        print(corner)
",
            ),
        ),
        // numpy: arange(24).reshape(2,3,4)[1,2,3] is 23 and [0,1,2] is 6.
        "23.0\n6.0\nfalse\nfalse\nfalse\nfalse\nfalse\n-5.0\n\
         index is outside dims (2, 3, 4)\n\
         false\n[[1.0 2.0 3.0]\n [4.0 0.5 6.0]]\n6.0\n"
    );
}

/// `reshape` keeps the elements and replaces the dims; a refusal gives the
/// array back whole; `transpose` is numpy's `.T` for a matrix, a vector is
/// its own transpose, and rank 3 is refused.
#[test]
fn reshape_and_transpose() {
    assert_eq!(
        prints(
            "reshape",
            &program(
                "    let a be shaped(0.0, 6.0, &[2, 3])
    let b, e1 be a.reshape(&[3, 2])
    print(e1?)
    print(b)
    let c, e2 be b.reshape(&[4, 2])
    if e2?:
        print(e2.message())
    print(c)
    let d be must(c.reshape(&[6]))
    print(d)
    let t be must(shaped(0.0, 6.0, &[2, 3]).transpose())
    print(t)
    print(t.dims[0] * 10 + t.dims[1])
    print(t.strides[0] * 10 + t.strides[1])
    let v be must(d.transpose())
    print(v)
    let cube, e3 be shaped(0.0, 8.0, &[2, 2, 2]).transpose()
    if e3?:
        print(e3.message())
",
            ),
        ),
        // numpy: arange(6).reshape(2,3).T is [[0,3],[1,4],[2,5]]; a `(3, 2)`
        // reshape of arange(6) is [[0,1],[2,3],[4,5]].
        "false\n[[0.0 1.0]\n [2.0 3.0]\n [4.0 5.0]]\n\
         cannot reshape 6 elements into (4, 2)\n\
         [[0.0 1.0]\n [2.0 3.0]\n [4.0 5.0]]\n\
         [0.0 1.0 2.0 3.0 4.0 5.0]\n\
         [[0.0 3.0]\n [1.0 4.0]\n [2.0 5.0]]\n\
         32\n21\n\
         [0.0 1.0 2.0 3.0 4.0 5.0]\n\
         transpose is written for rank 2 or less, not dims (2, 2, 2)\n"
    );
}

/// Same-shape `+ - * /` are element-wise, leave both operands usable, and
/// `-a` negates. `scaled` and `shifted` take a scalar.
#[test]
fn element_wise_operators() {
    assert_eq!(
        prints(
            "operators",
            &program(
                "    let a be shaped(1.0, 7.0, &[2, 3])
    let b be NdArray.full(&[2, 3], 2.0)
    print(a + b)
    print(a - b)
    print(a * b)
    print(a / b)
    print(-a)
    print(a.scaled(by: 0.5))
    print(a.shifted(by: -1.0))
    print(a + b * a)
    print(a is a.clone())
    print(a is b)
",
            ),
        ),
        "[[3.0 4.0 5.0]\n [6.0 7.0 8.0]]\n\
         [[-1.0 0.0 1.0]\n [2.0 3.0 4.0]]\n\
         [[2.0 4.0 6.0]\n [8.0 10.0 12.0]]\n\
         [[0.5 1.0 1.5]\n [2.0 2.5 3.0]]\n\
         [[-1.0 -2.0 -3.0]\n [-4.0 -5.0 -6.0]]\n\
         [[0.5 1.0 1.5]\n [2.0 2.5 3.0]]\n\
         [[0.0 1.0 2.0]\n [3.0 4.0 5.0]]\n\
         [[3.0 6.0 9.0]\n [12.0 15.0 18.0]]\n\
         true\nfalse\n"
    );
}

/// `a + b` over different dims is a panic, not a broadcast: the runtime
/// shadow of `broadcasting.md`'s `SC0290`.
#[test]
fn a_same_shape_operator_refuses_other_dims() {
    let dir = scratch("ndarray", "mismatch");
    require_runtime();
    let source = program(
        "    let a be shaped(0.0, 6.0, &[2, 3])
    let b be NdArray.ones(&[3])
    print(a + b)
",
    );
    let built = lower(&source).build_at(&executable(&dir, "mismatch"), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_ne!(ran.status, Some(0), "the mismatch must stop the program");
    assert_eq!(ran.stdout, "");
    assert!(
        ran.stderr.contains("dims (2, 3) and (3,) do not agree"),
        "the panic names both dims, got: {}",
        ran.stderr
    );
}

/// Each row of `broadcasting.md` §4.3 that has numbers, against numpy, and the
/// refusals: `(2, 3)` against `(3,)` and `(2, 1)`; the outer product of
/// `(3, 1)` and `(2,)`; `(2, 3)` against `(2,)`, which numpy refuses too; and
/// the empty case.
#[test]
fn broadcasting() {
    assert_eq!(
        prints(
            "broadcasting",
            &program(
                "    let x be shaped(0.0, 6.0, &[2, 3])
    let row, e0 be NdArray.from_array([10.0, 20.0, 30.0], &[3])
    let col, e1 be NdArray.from_array([2.0, 3.0], &[2, 1])
    let tall, e2 be NdArray.from_array([1.0, 2.0, 3.0], &[3, 1])
    let wide, e3 be NdArray.from_array([10.0, 20.0], &[2])
    let ratio, e4 be NdArray.from_array([1.0, 2.0, 4.0], &[3])
    if e0? or e1? or e2? or e3? or e4?:
        panic(\"setup\")
    print(must(x.broadcast_add(&row)))
    print(must(x.broadcast_mul(&col)))
    print(must(row.broadcast_add(&x)))
    print(must(tall.broadcast_add(&wide)))
    print(must(x.broadcast_sub(&col.shifted(by: -1.0).scaled(by: 0.5))))
    print(must(x.broadcast_div(&ratio)))
    let scalar be NdArray.full(&Array[Int].new(), 4.0)
    print(must(x.broadcast_add(&scalar)))
    let bad, err be x.broadcast_add(&wide)
    if err?:
        print(err.message())
        print(bad.size())
    let clash, err2 be tall.broadcast_mul(&x)
    if err2?:
        print(err2.message())
    let none, e5 be NdArray.from_array(Array[F64].new(), &[0])
    let one, e6 be NdArray.from_array([5.0], &[1])
    if e5? or e6?:
        panic(\"setup\")
    let nothing be must(none.broadcast_add(&one))
    print(nothing.size())
    print(nothing.dims[0])
    let same be must(x.broadcast_add(&x))
    print(same)
",
            ),
        ),
        // numpy, in the order printed. `x - (col - 1) / 2` is `x - [[.5],
        // [1]]`, which is not in the script and is hand-checked: row 0 is
        // 0-.5, 1-.5, 2-.5 and row 1 is 3-1, 4-1, 5-1.
        "[[10.0 21.0 32.0]\n [13.0 24.0 35.0]]\n\
         [[0.0 2.0 4.0]\n [9.0 12.0 15.0]]\n\
         [[10.0 21.0 32.0]\n [13.0 24.0 35.0]]\n\
         [[11.0 21.0]\n [12.0 22.0]\n [13.0 23.0]]\n\
         [[-0.5 0.5 1.5]\n [2.0 3.0 4.0]]\n\
         [[0.0 0.5 0.5]\n [3.0 2.0 1.25]]\n\
         [[4.0 5.0 6.0]\n [7.0 8.0 9.0]]\n\
         dims (2, 3) and (2,) do not agree\n0\n\
         dims (3, 1) and (2, 3) do not agree\n\
         0\n0\n\
         [[0.0 2.0 4.0]\n [6.0 8.0 10.0]]\n"
    );
}

/// `[[1, 2], [3, 4]]`-style products against numpy's `@`, a refusal of
/// dims that do not chain, and `dot`.
#[test]
fn matmul_and_dot() {
    assert_eq!(
        prints(
            "matmul",
            &program(
                "    let a be shaped(1.0, 13.0, &[3, 4])
    let b, e0 be NdArray.from_array([0.5, -1.0, 2.0, 0.25, 1.5, 3.0, -2.0, 1.0], &[4, 2])
    if e0?:
        panic(e0)
    print(must(a.matmul(&b)))
    let identity, e1 be NdArray.from_array([1.0, 0.0, 0.0, 1.0], &[2, 2])
    if e1?:
        panic(e1)
    print(must(b.matmul(&identity)) is b)
    let wrong, e2 be b.matmul(&a)
    if e2?:
        print(e2.message())
    let vector be NdArray.ones(&[4])
    let nonmatrix, e3 be a.matmul(&vector)
    if e3?:
        print(e3.message())
    let u, e4 be NdArray.from_array([1.0, 2.0, 3.0], &[3])
    let v, e5 be NdArray.from_array([4.0, -5.0, 6.0], &[3])
    if e4? or e5?:
        panic(\"setup\")
    let product, e6 be u.dot(&v)
    print(e6?)
    print(product)
    let short, e7 be u.dot(&NdArray.ones(&[2]))
    if e7?:
        print(e7.message())
    let matrix, e8 be u.dot(&a)
    if e8?:
        print(e8.message())
",
            ),
        ),
        // numpy: a @ b for a = arange(1, 13).reshape(3, 4), and dot = 12.
        "[[1.0 12.5]\n [9.0 25.5]\n [17.0 38.5]]\n\
         true\n\
         matmul needs the inner dims to agree, not (4, 2) and (3, 4)\n\
         matmul takes two matrices, not dims (3, 4) and (4,)\n\
         false\n12.0\n\
         dot needs vectors of one length, not (3,) and (2,)\n\
         dot takes two vectors, not dims (3,) and (3, 4)\n"
    );
}

/// The whole-array reductions, and every axis of a `(2, 3, 4)` array against
/// numpy's `sum`, `mean`, `min` and `max`.
#[test]
fn reductions() {
    assert_eq!(
        prints(
            "reductions",
            &program(
                "    let c be shaped(0.0, 24.0, &[2, 3, 4])
    print(c.sum())
    print(c.mean())
    let low be c.min()
    let high be c.max()
    if low? and high?:
        print(low)
        print(high)
    for axis in 0..3:
        print(must(c.sum_axis(axis)))
        print(must(c.mean_axis(axis)))
        print(must(c.min_axis(axis)))
        print(must(c.max_axis(axis)))
    let mixed, e0 be NdArray.from_array([3.0, -1.0, 2.0], &[3])
    if e0?:
        panic(e0)
    let least be mixed.min()
    if least?:
        print(least)
    let nobody be NdArray.zeros(&[0])
    print(nobody.sum())
    print(nobody.min()?)
    let bad, e1 be c.sum_axis(3)
    if e1?:
        print(e1.message())
    let negative, e2 be c.max_axis(-1)
    if e2?:
        print(e2.message())
    let hollow be NdArray.zeros(&[2, 0])
    let empty_sum be must(hollow.sum_axis(1))
    print(empty_sum)
    let empty_min, e3 be hollow.min_axis(1)
    if e3?:
        print(e3.message())
",
            ),
        ),
        // numpy: sum 276, mean 11.5, min 0, max 23, and the twelve axis
        // reductions printed by the script.
        "276.0\n11.5\n0.0\n23.0\n\
         [[12.0 14.0 16.0 18.0]\n [20.0 22.0 24.0 26.0]\n [28.0 30.0 32.0 34.0]]\n\
         [[6.0 7.0 8.0 9.0]\n [10.0 11.0 12.0 13.0]\n [14.0 15.0 16.0 17.0]]\n\
         [[0.0 1.0 2.0 3.0]\n [4.0 5.0 6.0 7.0]\n [8.0 9.0 10.0 11.0]]\n\
         [[12.0 13.0 14.0 15.0]\n [16.0 17.0 18.0 19.0]\n [20.0 21.0 22.0 23.0]]\n\
         [[12.0 15.0 18.0 21.0]\n [48.0 51.0 54.0 57.0]]\n\
         [[4.0 5.0 6.0 7.0]\n [16.0 17.0 18.0 19.0]]\n\
         [[0.0 1.0 2.0 3.0]\n [12.0 13.0 14.0 15.0]]\n\
         [[8.0 9.0 10.0 11.0]\n [20.0 21.0 22.0 23.0]]\n\
         [[6.0 22.0 38.0]\n [54.0 70.0 86.0]]\n\
         [[1.5 5.5 9.5]\n [13.5 17.5 21.5]]\n\
         [[0.0 4.0 8.0]\n [12.0 16.0 20.0]]\n\
         [[3.0 7.0 11.0]\n [15.0 19.0 23.0]]\n\
         -1.0\n0.0\nfalse\n\
         axis 3 does not exist for dims (2, 3, 4)\n\
         axis -1 does not exist for dims (2, 3, 4)\n\
         [0.0 0.0]\n\
         no least or greatest element along an axis of length 0\n"
    );
}

/// numpy's nesting, minus its column alignment (the module header says so):
/// a scalar, an empty array, a vector, a matrix, and a rank-3 block, which
/// numpy separates with a blank line.
#[test]
fn display() {
    assert_eq!(
        prints(
            "display",
            &program(
                "    print(NdArray.full(&Array[Int].new(), 3.5))
    print(NdArray.zeros(&[0]))
    print(NdArray.zeros(&[2, 0]))
    print(NdArray.linspace(0.0, 1.0, 3))
    print(shaped(0.0, 6.0, &[3, 2]))
    print(shaped(0.0, 8.0, &[2, 2, 2]))
    print(shaped(0.0, 4.0, &[1, 1, 2, 2]))
    let text be f\"{shaped(0.0, 2.0, &[2])}!\"
    print(text)
",
            ),
        ),
        // numpy prints arange(8).reshape(2,2,2) with a blank line between the
        // two 2x2 blocks, and the 4-d array with a blank line between its
        // outermost items and two between rank-3 blocks.
        "3.5\n[]\n[]\n[0.0 0.5 1.0]\n\
         [[0.0 1.0]\n [2.0 3.0]\n [4.0 5.0]]\n\
         [[[0.0 1.0]\n  [2.0 3.0]]\n\n [[4.0 5.0]\n  [6.0 7.0]]]\n\
         [[[[0.0 1.0]\n   [2.0 3.0]]]]\n\
         [0.0 1.0]!\n"
    );
}

/// A loop that makes, combines, reduces and drops arrays each turn: the number
/// is the assertion that the loop ran. That the buffers are released is
/// measured by [`a_loop_over_arrays_leaks_nothing`].
const LOOP_BODY: &str = "    let mutable total be 0.0
    for turn in 0..ITERATIONS:
        let a be shaped(0.0, 6.0, &[2, 3])
        let b be NdArray.full(&[2, 3], 2.0)
        let sum be a + b * a
        let t be must(sum.transpose())
        let product be must(sum.matmul(&t))
        let rows be must(product.sum_axis(0))
        let wide be must(sum.broadcast_add(&NdArray.ones(&[3])))
        let reshaped be must(wide.reshape(&[3, 2]))
        total be total + rows.sum() + reshaped.mean() + a.scaled(by: 2.0).sum()
    print(total)
";

#[test]
fn a_loop_of_arrays_runs() {
    // Per turn: a = [[0,1,2],[3,4,5]], sum = a + 2a = 3a, product = (3a)(3a)^T
    // = 9 * [[5, 14], [14, 50]], whose column sums total 9 * 83 = 747; `wide`
    // is 3a + 1, whose mean is 3 * 2.5 + 1 = 8.5; a.scaled(2).sum() is 30.
    // numpy: 747 + 8.5 + 30 = 785.5, and ten turns are 7855.
    assert_eq!(
        prints("loop", &program(&LOOP_BODY.replace("ITERATIONS", "10"))),
        "7855.0\n"
    );
}

/// The same loop under `leaks --atExit`, at two lengths: no leak at either, and
/// the same number of allocations live at exit, which is what makes the live
/// set constant and not merely small. Skipped where `/usr/bin/leaks` is not.
#[test]
fn a_loop_over_arrays_leaks_nothing() {
    let leaks = std::path::Path::new("/usr/bin/leaks");
    if !leaks.exists() {
        return;
    }
    require_runtime();
    let mut measured = Vec::new();
    for (name, turns) in [("leaks_short", "1000"), ("leaks_long", "5000")] {
        let dir = scratch("ndarray", name);
        let source = program(&LOOP_BODY.replace("ITERATIONS", turns));
        let built = lower(&source).build_at(&executable(&dir, name), OptLevel::O2);
        let output = std::process::Command::new(leaks)
            .arg("--atExit")
            .arg("--")
            .arg(&built.executable)
            .output()
            .expect("leaks runs");
        let _ = std::fs::remove_dir_all(&dir);
        let report = String::from_utf8_lossy(&output.stdout).to_string();
        assert!(
            report.contains("0 leaks for 0 total leaked bytes"),
            "{name}: expected no leaks, got:\n{report}"
        );
        let nodes = report
            .lines()
            .find(|line| line.contains("nodes malloced"))
            .unwrap_or_else(|| panic!("{name}: no live-set line in:\n{report}"))
            .split("nodes malloced")
            .next()
            .unwrap()
            .split_whitespace()
            .last()
            .unwrap()
            .to_string();
        measured.push(nodes);
    }
    assert_eq!(measured[0], measured[1], "the live set must not grow with the loop");
}
