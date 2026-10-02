//! An operator at several right-hand sides, **built, linked, run**.
//!
//! `V implements Mul:` beside `V implements Mul[F64]:` is one operator with two
//! implementations, and the right operand's type picks between them
//! (`science-types`' `BodyChecker::select_operator`, over `methods`' §6
//! `Found::Instances`). What has to hold:
//!
//! - **A user record** answers `a * b` and `a * 2.0` with the right method each.
//! - **The bundled `complex`** takes `z * 2.0`, `z / 2.0`, `z + 1.0`, `z - 1.0`.
//! - **The bundled `ndarray`** takes the same four scalars on the right.
//! - **The scalar on the left** is writable beside the local type:
//!   `F64 implements Mul[V]` is local because `V` is an argument of the
//!   interface, and `2.0 * z` works in the bundled `complex`. With only foreign
//!   types in play (`F64 implements Mul[String]`) it is still an orphan,
//!   `SC0207`.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("mixed_operators", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

#[test]
fn a_record_takes_a_record_and_a_scalar() {
    assert_eq!(
        prints(
            "record",
            "type V:
    x: F64

V implements Copy

V implements Add:
    def add(self, other: V) -> V:
        V(x: self.x + other.x)

V implements Add[F64]:
    def add(self, other: F64) -> V:
        V(x: self.x + other + 100.0)

V implements Mul:
    def mul(self, other: V) -> V:
        V(x: self.x * other.x)

V implements Mul[F64]:
    def mul(self, other: F64) -> V:
        V(x: self.x * other)

def main():
    let a be V(x: 2.0)
    print((a * a).x)
    print((a * 3.0).x)
    print((a * 4).x)
    print((a + a).x)
    print((a + 1.0).x)
    for k in [1.5, 2.5]:
        print((a * k).x)
"
        ),
        "4.0\n6.0\n8.0\n4.0\n103.0\n3.0\n5.0\n"
    );
}

#[test]
fn complex_takes_a_real_on_the_right() {
    assert_eq!(
        prints(
            "complex",
            "use complex (Complex)

def main():
    let z be Complex(re: 1.5, im: -2.0)
    print(z * 2.0)
    print(z / 2.0)
    print(z + 1.0)
    print(z - 1.0)
    print(z * Complex(re: 0.0, im: 1.0))
    print(z.scaled(by: 2.0))
"
        ),
        "3.0-4.0i\n0.75-1.0i\n2.5-2.0i\n0.5-2.0i\n2.0+1.5i\n3.0-4.0i\n"
    );
}

#[test]
fn ndarray_takes_a_scalar_on_the_right() {
    let out = prints(
        "ndarray",
        "use ndarray (NdArray)

def main():
    let a be NdArray.arange(1.0, 5.0, 1.0)
    print(a * 2.0)
    print(a / 2.0)
    print(a + 1.0)
    print(a - 1.0)
    print(a * a)
    print(a.scaled(by: 2.0) is a * 2.0)
",
    );
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 6, "{out}");
    assert_eq!(lines[5], "true");
    assert!(lines[0].contains('8') && lines[1].contains("0.5"), "{out}");
}

#[test]
fn a_scalar_on_the_left_is_legal_beside_the_local_type() {
    assert_eq!(
        prints(
            "scalar_left",
            "type V:
    x: F64

V implements Mul[F64]:
    def mul(self, other: F64) -> V:
        V(x: self.x * other)

F64 implements Mul[V]:
    def mul(self, other: V) -> V:
        V(x: self * other.x)

def main():
    let first be V(x: 1.5)
    let second be V(x: 1.5)
    let third be V(x: 1.5)
    let k: F64 be 2.0
    print((2.0 * first).x)
    print((k * second).x)
    print((third * 2.0).x)
    print(k * k + 1.0)
",
        ),
        "3.0\n3.0\n3.0\n5.0\n"
    );
}

#[test]
fn complex_takes_a_real_on_the_left() {
    assert_eq!(
        prints(
            "complex_left",
            "use complex (Complex)

def main():
    let z be Complex(re: 1.5, im: -2.0)
    print(2.0 * z)
    print(z * 2.0)
"
        ),
        "3.0-4.0i\n3.0-4.0i\n"
    );
}

#[test]
fn ndarray_takes_a_scalar_on_the_left() {
    assert_eq!(
        prints(
            "ndarray_left",
            "use ndarray (NdArray)

def main():
    let a be NdArray.arange(1.0, 4.0, 1.0)
    let b be NdArray.arange(1.0, 4.0, 1.0)
    print(2.0 * a)
    print(1.0 + b)
"
        ),
        "[2.0 4.0 6.0]\n[2.0 3.0 4.0]\n"
    );
}

#[test]
fn a_scalar_on_the_left_with_only_foreign_types_is_an_orphan() {
    let caught = std::panic::catch_unwind(|| {
        lower(
            "F64 implements Mul[String]:
    def mul(self, other: String) -> String:
        other

def main():
    print(1)
",
        );
    });
    let payload = caught.expect_err("an orphan must not lower");
    let message = payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|text| text.to_string()))
        .unwrap_or_default();
    assert!(message.contains("207"), "{message}");
}
