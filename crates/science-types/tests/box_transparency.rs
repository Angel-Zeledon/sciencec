//! Decision 28's AMENDMENT 6 — a `Box` is transparent, read-only, to a `match`
//! and to a `&T` argument, and to nothing that writes or moves.
//!
//! The amendment extends Decision 28's method-call transparency in two places
//! with the same scope: a `match` whose scrutinee is a `Box[T]` or a
//! `&Box[T]` matches on the `T` and binds shared borrows of the payload, and a
//! shared `&Box[T]` argument reaches a `&T` parameter (`assign`'s rule 6a;
//! `tests/relations.rs` pins the relation itself). What it leaves closed is
//! pinned here too, each by the code that says so: an exclusive borrow through
//! a `Box` is `SC0546`, and a move out of a boxed binding is a type error
//! because the binding is a borrow.

mod support;

use science_types::assign::Coercion;
use science_types::thir::ExprKind;
use support::check;

/// The program the amendment was written against, verbatim.
const DESCRIBE: &str = "\
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

/// The reproducer checks — the `SC0525` at `describe(inner)` is gone — and the
/// argument is a [`Coercion::BorrowThroughBox`] over the `&Box[Expr]` binding
/// Decision 27 gave `inner`, typed as the parameter was written.
#[test]
fn a_borrowed_box_binding_reaches_a_borrowed_parameter() {
    let checked = check(DESCRIBE);
    checked.assert_clean();
    let body = checked.body("describe");
    let coerce = checked.find("describe", |kind| {
        matches!(kind, ExprKind::Coerce { coercion: Coercion::BorrowThroughBox, .. })
    });
    let ExprKind::Coerce { operand, .. } = body.expr(coerce).kind else { unreachable!() };
    assert_eq!(checked.render(body.ty(coerce)), "&Expr");
    assert_eq!(checked.render(body.ty(operand)), "&Box[Expr]");
}

/// A `match` over an owned `Box[Expr]` sees `Expr`'s variants: the three arms
/// below are exhaustive with no `_`, and `Name(t)` binds `t` as a shared
/// borrow of the payload's `String`, never the `String` itself.
#[test]
fn a_match_on_an_owned_box_matches_the_payload_and_binds_borrows() {
    let checked = check(
        "\
choice Expr:
    Number(Int)
    Name(String)
    Neg(Box[Expr])

def show(boxed: Box[Expr]) -> Int:
    match boxed:
        Number(n): n
        Name(t): t.length()
        Neg(_): 0
",
    );
    checked.assert_clean();
    let body = checked.body("show");
    let receiver = checked.find("show", |kind| matches!(kind, ExprKind::MethodCall { .. }));
    let ExprKind::MethodCall { receiver, .. } = &body.expr(receiver).kind else {
        unreachable!()
    };
    assert_eq!(checked.render(body.ty(*receiver)), "&String");
    // And `n` is an `Int`, copied: `Int` owns nothing, so Decision 27 leaves
    // it by value, exactly as it does under a borrowed scrutinee.
    let tail = checked.nodes("show");
    assert!(tail.contains(&("local".to_string(), "I64".to_string())), "{tail:?}");
}

/// Exhaustiveness reads the constructor set off the payload, so a `match` on a
/// `Box[Expr]` that leaves variants out is `SC0250` naming them — not the
/// constructor-less `Box` that made every such `match` need a `_`.
#[test]
fn exhaustiveness_names_the_payloads_missing_variants() {
    let checked = check(
        "\
choice Expr:
    Number(Int)
    Name(String)
    Neg(Box[Expr])

def main():
    let boxed be Box.new(Expr.Name(\"x\"))
    match boxed:
        Name(t): print(t)
",
    );
    assert_eq!(checked.codes(), vec![250]);
    let message = &checked.messages()[0];
    assert!(message.contains("Number(_)") && message.contains("Neg(_)"), "{message}");
}

/// A nested pattern crosses a `Box` one level down: `Node(Leaf(_), right)`
/// tests the tag of the `Tree` inside the first `Box[Tree]` of the payload.
#[test]
fn a_nested_pattern_matches_through_a_payload_box() {
    let checked = check(
        "\
choice Tree:
    Leaf(Int)
    Node(Box[Tree], Box[Tree])

def depth(tree: &Tree) -> Int:
    match tree:
        Leaf(_): 1
        Node(Leaf(_), right): 1 + depth(right)
        Node(left, _): 1 + depth(left)
",
    );
    checked.assert_clean();
}

/// A record's `Box` field is a scrutinee like any other, and a record pattern
/// under a `Box` reads its fields as borrows.
#[test]
fn a_box_field_and_a_boxed_record_are_matched_through() {
    let checked = check(
        "\
choice Tree:
    Leaf(Int)
    Node(Box[Tree], Box[Tree])

type Holder:
    inner: Box[Tree]

type Named:
    left: String
    right: String

def sum(tree: &Tree) -> Int:
    match tree:
        Leaf(n): n
        Node(left, right): sum(left) + sum(right)

def main():
    let h be Holder(inner: Box.new(Tree.Leaf(2)))
    match h.inner:
        Leaf(n): print(n)
        Node(_, right): print(sum(right))
    let named be Box.new(Named(left: \"a\", right: \"b\"))
    match named:
        Named(left: l, right: r): print(f\"{l}{r}\")
",
    );
    checked.assert_clean();
}

/// §6.3's auto-borrow composes with rule 6a: an owned `Box[Expr]` passed to a
/// `&Expr` parameter is borrowed — a `Borrow` node the author did not write,
/// typed `&Box[Expr]` — and that borrow is carried through the box.
#[test]
fn an_owned_box_argument_is_auto_borrowed_and_then_borrowed_through() {
    let checked = check(
        "\
choice Expr:
    Number(Int)
    Neg(Box[Expr])

def value(expr: &Expr) -> Int:
    match expr:
        Number(n): n
        Neg(_): 0

def main():
    let boxed be Box.new(Expr.Number(3))
    print(value(boxed))
",
    );
    checked.assert_clean();
    let body = checked.body("main");
    let coerce = checked.find("main", |kind| {
        matches!(kind, ExprKind::Coerce { coercion: Coercion::BorrowThroughBox, .. })
    });
    let ExprKind::Coerce { operand, .. } = body.expr(coerce).kind else { unreachable!() };
    assert!(matches!(body.expr(operand).kind, ExprKind::Borrow { mutable: false, .. }));
    assert_eq!(checked.render(body.ty(operand)), "&Box[Expr]");
}

/// **At a call and nowhere else**: a `let` annotated `&Expr` is not a call
/// site, so a `&Box[Expr]` does not reach it.
#[test]
fn a_borrow_through_a_box_is_not_taken_at_a_let() {
    let checked = check(
        "\
choice Expr:
    Number(Int)
    Neg(Box[Expr])

def peel(expr: &Expr) -> Int:
    match expr:
        Neg(inner):
            let plain: &Expr be inner
            0
        Number(n): n
",
    );
    assert_eq!(checked.codes(), vec![525]);
}

/// The amendment's named exclusion. `&mut Box[Expr]` into `&mut Expr` is
/// `SC0546`, whose message says the scope is deliberate, rather than an
/// `SC0525` quoting two types one word apart — and so is an owned box, which
/// the auto-borrow would have made the same `&mut Box[Expr]`, and a shared
/// `&Box[Expr]` binding handed to a `&mut Expr`.
#[test]
fn an_exclusive_borrow_through_a_box_is_refused_by_name() {
    let prefix = "\
choice Expr:
    Number(Int)
    Neg(Box[Expr])

def bump(expr: &mut Expr):
    expr be Expr.Number(0)
";
    for tail in [
        "\
def main():
    let mutable boxed be Box.new(Expr.Number(1))
    bump(&mut boxed)
",
        "\
def main():
    let mutable boxed be Box.new(Expr.Number(1))
    bump(boxed)
",
        "\
def walk(expr: &Expr):
    match expr:
        Neg(inner): bump(inner)
        Number(_): print(\"n\")
",
    ] {
        let checked = check(&format!("{prefix}\n{tail}"));
        assert_eq!(checked.codes(), vec![546], "{tail}");
        assert!(checked.messages()[0].contains("through a `Box`"), "{:?}", checked.messages());
    }
}

/// Moving out of a `Box` stays refused, and here the refusal is the type
/// checker's: a payload that owns something binds as a borrow, so handing it
/// to a by-value parameter is `expected String, found &String` — there is no
/// move to reach `science-regions` at all, and nothing to free twice.
#[test]
fn a_boxed_match_binding_cannot_be_moved_out() {
    let checked = check(
        "\
choice Expr:
    Name(String)
    Neg(Box[Expr])

def consume(text: String) -> String:
    text

def main():
    let boxed be Box.new(Expr.Name(\"x\"))
    match boxed:
        Name(t): print(consume(t))
        _: print(\"other\")
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert!(checked.messages()[0].contains("&String"), "{:?}", checked.messages());
}
