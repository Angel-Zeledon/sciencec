//! Decision 28's AMENDMENT 6, **built, linked, run**: a `Box` read through by
//! a `match` and by a `&T` argument.
//!
//! # Why every test here runs the program
//!
//! `methods.rs`'s reason, one construct over. A `Box[T]` and a `&T` are the
//! same one-word pointer, so a `&Box[T]` handed where a `&T` was wanted — the
//! address of the box instead of the address in it — verifies, links and
//! reads garbage. So does a tag read off the box's pointer instead of the
//! payload. The values below are chosen so that either mistake prints a
//! different number or crashes, and the tests read what the program wrote.
//!
//! # Why the last test runs `leaks`
//!
//! A binding that moved out of a box instead of borrowing would free the
//! payload twice — once at the binding's scope end and once when the box
//! drops — and a borrow that took the box's address and kept it would leak
//! nothing but read freed memory. A test that reads stdout sees neither
//! reliably. macOS's `/usr/bin/leaks --atExit` reports both classes, and the
//! test runs it when it is there.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

/// Build one program at `-O2`, run it, and give back what it printed.
fn prints(name: &str, source: &str) -> String {
    let dir = scratch("box_match", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// The amendment's reproducer, verbatim: a recursive walk that matches a
/// `&Expr`, binds `inner` as a `&Box[Expr]`, and passes it to a `&Expr`
/// parameter — then a `match` on an owned `Box[Expr]`.
#[test]
fn the_recursive_describe_walks_through_every_box() {
    let source = "\
choice Expr:
    Number(Int)
    Name(String)
    Neg(Box[Expr])

def describe(expr: &Expr) -> String:
    match expr:
        Number(n): f\"number {n}\"
        Name(text): f\"name {text}\"
        Neg(inner): f\"neg ({describe(inner)})\"

def main():
    let e be Expr.Neg(Box.new(Expr.Neg(Box.new(Expr.Number(3)))))
    print(describe(e))
    let boxed be Box.new(Expr.Name(\"x\"))
    match boxed:
        Name(t): print(t)
        _: print(\"other\")
";
    assert_eq!(prints("describe", source), "neg (neg (number 3))\nx\n");
}

/// A `match` on an owned `Box` with no `_` arm — exhaustiveness sees the
/// payload's variants — and an owned box passed straight to a `&T` parameter,
/// which §6.3's auto-borrow and rule 6a carry through together.
#[test]
fn an_owned_box_is_matched_and_passed_by_its_payload() {
    let source = "\
choice Shape:
    Circle(Int)
    Square(Int)
    Label(String)

def area(figure: &Shape) -> Int:
    match figure:
        Circle(r): 3 * r * r
        Square(s): s * s
        Label(text): text.length()

def main():
    let label be Box.new(Shape.Label(\"seven77\"))
    match label:
        Circle(_): print(\"circle\")
        Square(_): print(\"square\")
        Label(text): print(f\"label {text}\")
    print(area(label))
    let square be Box.new(Shape.Square(9))
    print(area(square))
    print(area(Box.new(Shape.Circle(2))))
";
    assert_eq!(prints("owned", source), "label seven77\n7\n81\n12\n");
}

/// A record's `Box` field as the scrutinee, a nested pattern that tests the
/// tag of a payload's own box, and a record pattern under a `Box`.
#[test]
fn a_box_field_a_nested_box_and_a_boxed_record_are_matched_through() {
    let source = "\
choice Tree:
    Leaf(Int)
    Node(Box[Tree], Box[Tree])

type Holder:
    label: String
    inner: Box[Tree]

type Named:
    left: String
    right: String

def sum(tree: &Tree) -> Int:
    match tree:
        Leaf(n): n
        Node(left, right): sum(left) + sum(right)

def depth(tree: &Tree) -> Int:
    match tree:
        Leaf(_): 1
        Node(Leaf(_), right): 1 + depth(right)
        Node(left, _): 1 + depth(left)

def build(n: Int) -> Tree:
    if n <= 0:
        return Tree.Leaf(1)
    Tree.Node(Box.new(build(n - 1)), Box.new(Tree.Leaf(n)))

def main():
    let t be build(5)
    print(sum(t))
    print(depth(t))
    let h be Holder(label: \"h\", inner: Box.new(build(2)))
    match h.inner:
        Leaf(n): print(n)
        Node(_, right): print(sum(right))
    print(sum(h.inner))
    let named be Box.new(Named(left: \"a\", right: \"b\"))
    match named:
        Named(left: l, right: r): print(f\"{l}{r}\")
";
    assert_eq!(prints("nested", source), "16\n6\n2\n4\nab\n");
}

/// **The payload is freed once, by its box, and never by a binding.** A
/// recursive tree walked through its boxes 2 000 times — every binding a
/// shared borrow of a payload, every tree dropped at the end of its iteration
/// — must print the right total, and under `leaks --atExit` report no leak.
/// Each `Leaf` in the tree owns a `String` so there is something real for a
/// double free or a leak to be about.
///
/// Measured before this was checked in: `leaks --atExit` reporting *"0 leaks
/// for 0 total leaked bytes"* on the same program.
#[test]
fn a_recursive_tree_walk_through_boxes_frees_every_payload_exactly_once() {
    let source = "\
choice Tree:
    Leaf(String)
    Node(Box[Tree], Box[Tree])

def total(tree: &Tree) -> Int:
    match tree:
        Leaf(text): text.length()
        Node(left, right): total(left) + total(right)

def build(n: Int) -> Tree:
    if n <= 0:
        return Tree.Leaf(\"abc\")
    Tree.Node(Box.new(build(n - 1)), Box.new(build(n - 1)))

def main():
    let mutable seen be 0
    for round in 0..2000:
        let tree be Box.new(build(4))
        match tree:
            Node(left, _): seen be seen + total(left)
            Leaf(_): seen be seen + 1
    print(f\"{seen}\")
";
    let name = "leak_walk";
    let dir = scratch("box_match", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    // `build(4)` has 16 leaves; `left` is half of them, 8 × 3 bytes.
    assert_eq!(ran.stdout, "48000\n");
    let leaks = std::path::Path::new("/usr/bin/leaks");
    if cfg!(target_os = "macos") && leaks.is_file() {
        let report = std::process::Command::new(leaks)
            .arg("--atExit")
            .arg("--")
            .arg(&built.executable)
            .output()
            .expect("`leaks` runs");
        let text = String::from_utf8_lossy(&report.stdout);
        assert!(
            text.contains(" 0 leaks for 0 total leaked bytes"),
            "`leaks` found something:\n{text}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
