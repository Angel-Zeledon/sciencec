//! `collections-and-chains.md` §1.4's chain vocabulary, **at the type level**.
//!
//! What runs is `science-codegen-llvm/tests/chains.rs`'s: every link and
//! terminal is built, linked and run there. This file holds the half that
//! must never reach the backend — the chains a checker has to refuse, each
//! with a real diagnostic rather than an `SC0400` about MIR — and the types
//! the checker gives the terminals whose result is not simply their
//! declaration.

mod support;

use science_types::thir::ExprKind;
use support::{check, Checked};

/// The type the checker gave the call to the chain method `name` in `body`.
fn terminal_type(checked: &Checked, body: &str, name: &str) -> String {
    let found = checked.body(body).exprs().find(|(_, expr)| match expr.kind {
        ExprKind::MethodCall { method: Some(method), .. } => {
            checked.krate.defs.get(method).name == name
        }
        _ => false,
    });
    let (_, expr) = found.unwrap_or_else(|| panic!("`{body}` calls no `{name}`"));
    checked.render(expr.ty)
}

/// **`sum()` over a chain of borrowed numbers is a number**, which is §1.5's
/// *"no `sum` over references as a separate method"*: `iterate()` yields
/// `&Int` (§4.3), and the declaration says the item. `chain_total` peels the
/// borrow, on the source and after a `keep`, whose `Item` is still `&Int`.
///
/// An `Int` return would not prove this on its own — the checker copies a
/// `&Int` out into an `Int` slot (`Coercion::Copy`) — so the assertion is on
/// the call's own type.
#[test]
fn sum_over_borrowed_numbers_is_a_number() {
    let checked = check(
        "\
def on_the_source(xs: &Array[Int]) -> Int:
    xs.iterate().sum()

def after_a_filter(xs: &Array[F64]) -> F64:
    xs.iterate().keep(each > 0.5).sum()

def after_a_map(xs: &Array[Int]) -> Int:
    xs.iterate().map(each * 2).sum()
",
    );
    checked.assert_clean();
    assert_eq!(terminal_type(&checked, "on_the_source", "sum"), "I64");
    assert_eq!(terminal_type(&checked, "after_a_filter", "sum"), "F64");
    assert_eq!(terminal_type(&checked, "after_a_map", "sum"), "I64");
}

/// **A `sum()` over strings is `SC0547`**, although `String implements Add`
/// — §1.5: *"concatenation is not summation"*. The same code for a chain of
/// records, which is the other thing an author writes by mistake, and whose
/// fix (`map` to the number first) the note carries.
#[test]
fn sum_over_non_numeric_items_is_refused() {
    let checked = check(
        "\
type Doc:
    title: String
    score: Int

def titles(docs: &Array[Doc]) -> Int:
    let total be docs.iterate().map(each.title).sum()
    0

def records(docs: &Array[Doc]) -> Int:
    let total be docs.iterate().sum()
    0

def scores(docs: &Array[Doc]) -> Int:
    docs.iterate().map(each.score).sum()
",
    );
    assert_eq!(checked.codes(), vec![547, 547], "{:?}", checked.messages());
    let messages = checked.messages();
    assert_eq!(messages[0], "`sum()` needs numeric items, and these are `String`");
    assert_eq!(messages[1], "`sum()` needs numeric items, and these are `Doc`");
}

/// **A predicate that is not a predicate is an ordinary type mismatch** at
/// the closure's body — `keep`, `has_any`, `has_all` and `find` are declared
/// `(borrowed Item) -> Bool` (§1.2), and the arrow written out is what gives
/// `each` a type to be wrong against. A `skip` whose count is a string is the
/// same, one parameter along.
#[test]
fn a_closure_of_the_wrong_type_is_refused_at_the_closure() {
    let checked = check(
        "\
def uses(xs: &Array[Int]) -> Int:
    let a be xs.iterate().keep(each + 1).count()
    let b be xs.iterate().has_any(each * 2)
    let c be xs.iterate().find(each - 1)
    let d be xs.iterate().skip(\"two\").count()
    0
",
    );
    assert_eq!(checked.codes(), vec![525, 525, 525, 525], "{:?}", checked.messages());
}

/// `first()` and `find` hand back the chain's item, nullable (§1.4:
/// `-> Self.Item?`), so a chain of borrows gives a nullable borrow. `count()`
/// is an `Int` and the two `has_` terminals a `Bool`.
#[test]
fn the_scalar_terminals_have_the_types_the_note_gives() {
    let checked = check(
        "\
type Doc:
    title: String

def first_title(docs: &Array[Doc]) -> (&String)?:
    docs.iterate().map(each.title).first()

def first_long(docs: &Array[Doc]) -> (&Doc)?:
    docs.iterate().find(each.title.length() > 3)

def how_many(docs: &Array[Doc]) -> Int:
    docs.iterate().skip(1).count()

def any_empty(docs: &Array[Doc]) -> Bool:
    docs.iterate().has_any(each.title.length() is 0)

def all_named(docs: &Array[Doc]) -> Bool:
    docs.iterate().has_all(each.title.length() > 0)
",
    );
    checked.assert_clean();
    assert_eq!(terminal_type(&checked, "first_title", "first"), "(&String)?");
    assert_eq!(terminal_type(&checked, "first_long", "find"), "(&Doc)?");
    assert_eq!(terminal_type(&checked, "how_many", "count"), "I64");
    assert_eq!(terminal_type(&checked, "any_empty", "has_any"), "Bool");
    assert_eq!(terminal_type(&checked, "all_named", "has_all"), "Bool");
}

/// **A misspelled link is `SC0532`**, and a name §1.4 gives that the prelude
/// has not transcribed is silent. The chain types used to be wholly open —
/// every name on them silent — and the misspelling reached the backend as a
/// hole; `builtins.rs`' `CHAIN_UNWRITTEN` is the list that now tells the two
/// apart. The misspelling is tested after a `keep`, too, because every
/// adapter carries its own copy of the surface.
#[test]
fn a_misspelled_link_is_reported_and_an_untranscribed_one_is_not() {
    let checked = check(
        "\
def uses(xs: &Array[Int]) -> Int:
    let a be xs.iterate().frist()
    let b be xs.iterate().keep(each > 1).colect()
    let c be xs.iterate().numbered()
    let d be xs.iterate().map(each * 2).reduce(0, each)
    0
",
    );
    assert_eq!(checked.codes(), vec![532, 532], "{:?}", checked.messages());
    let messages = checked.messages();
    assert!(messages[0].contains("`frist`"), "{messages:?}");
    assert!(messages[1].contains("`colect`"), "{messages:?}");
}
