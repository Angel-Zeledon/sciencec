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
    let c be xs.iterate().group()
    let d be xs.iterate().map(each * 2).collect_or_error()
    0
",
    );
    assert_eq!(checked.codes(), vec![532, 532], "{:?}", checked.messages());
    let messages = checked.messages();
    assert!(messages[0].contains("`frist`"), "{messages:?}");
    assert!(messages[1].contains("`colect`"), "{messages:?}");
}

/// **`numbered()` yields §1.3's `Numbered of T`**, and the chain's own type
/// says so: the adapter carries the record as its `Item`, so `collect()` names
/// it and the next link's closure takes it. `index` is an `Int` and `item` is
/// whatever the chain carried — a borrow, over the source.
#[test]
fn numbered_yields_the_numbered_record() {
    let checked = check(
        "\
def pairs(xs: &Array[Int]) -> Int:
    let all be xs.iterate().numbered().collect()
    0

def positions(xs: &Array[Int]) -> Int:
    let mutable total be 0
    for pair in xs.iterate().numbered():
        total be total + pair.index
    total
",
    );
    checked.assert_clean();
    assert_eq!(
        terminal_type(&checked, "pairs", "collect"),
        "Array[Numbered[&I64]]",
        "{:?}",
        checked.messages()
    );
}

/// **The new terminals have the types the note gives them**: `last()` and the
/// two extremes are `Self.Item?`, a nullable borrow over the source, and the
/// extremes' key is an `Int` the way `sorted(by:)`'s is.
#[test]
fn last_and_the_extremes_hand_back_a_nullable_item() {
    let checked = check(
        "\
type Doc:
    title: String
    score: Int

def last_doc(docs: &Array[Doc]) -> (&Doc)?:
    docs.iterate().last()

def best(docs: &Array[Doc]) -> (&Doc)?:
    docs.iterate().maximum(by: each.score)

def worst(docs: &Array[Doc]) -> (&Doc)?:
    docs.iterate().minimum(by: doc giving doc.score)
",
    );
    checked.assert_clean();
    assert_eq!(terminal_type(&checked, "last_doc", "last"), "(&Doc)?");
    assert_eq!(terminal_type(&checked, "best", "maximum"), "(&Doc)?");
    assert_eq!(terminal_type(&checked, "worst", "minimum"), "(&Doc)?");
}

/// **A closure of the wrong type is refused at the closure** for the new links
/// too: `take_while` and `skip_while` take a predicate, `every` takes a count,
/// and the extremes take an `Int` key. A bare `minimum()` is a wrong argument
/// count and not a silence — `sorted()`'s narrowing, and the reason is the same.
#[test]
fn the_new_links_refuse_a_closure_of_the_wrong_type() {
    let checked = check(
        "\
def uses(xs: &Array[Int]) -> Int:
    let a be xs.iterate().take_while(each + 1).count()
    let b be xs.iterate().skip_while(each * 2).count()
    let c be xs.iterate().every(\"two\").count()
    let d be xs.iterate().minimum(by: each > 1)
    0
",
    );
    assert_eq!(checked.codes(), vec![525, 525, 525, 525], "{:?}", checked.messages());
    let bare = check(
        "\
def uses(xs: &Array[Int]) -> Int:
    let a be xs.iterate().minimum()
    0
",
    );
    assert_eq!(bare.codes().len(), 1, "{:?}", bare.messages());
}

/// **`Map.keys()` and `values()` are chain sources typed over the map's own
/// arguments**: a borrow of each key, a borrow of each value, and `sum()` over
/// the values is the number it is over any other chain of borrowed numbers —
/// over the keys of a `Map[String, _]` it is `SC0547`.
#[test]
fn a_maps_sources_are_typed_over_its_arguments() {
    let checked = check(
        "\
def names(m: &Map[String, Int]) -> Int:
    let keys be m.keys().collect()
    let entries be m.iterate().collect()
    m.values().sum()

def walk(m: &Map[String, Int]) -> Int:
    let mutable total be 0
    for k in m.keys():
        total be total + k.length()
    for v in m.values():
        total be total + v
    total
",
    );
    checked.assert_clean();
    assert_eq!(terminal_type(&checked, "names", "sum"), "I64");
    assert_eq!(terminal_type(&checked, "names", "collect"), "Array[&String]");
    let refused = check(
        "\
def bad(m: &Map[String, Int]) -> Int:
    let total be m.keys().sum()
    0
",
    );
    assert_eq!(refused.codes(), vec![547], "{:?}", refused.messages());
}

/// **`zip` yields §1.3's `Pair`**, `left` the chain's item and `right` a
/// borrow of the other array's element. The other side is declared as an
/// `ArrayIterate[U]` — `builtins.rs` says why — and what that costs is in
/// `science-codegen-llvm/tests/chains.rs`: anything else leaves `U` unsolved,
/// the checker is silent, and the backend refuses the `Zip` it cannot build.
#[test]
fn zip_yields_the_pair_record() {
    let checked = check(
        "\
def together(xs: &Array[Int], names: &Array[String]) -> Int:
    let all be xs.iterate().zip(names.iterate()).collect()
    0
",
    );
    checked.assert_clean();
    assert_eq!(
        terminal_type(&checked, "together", "collect"),
        "Array[Pair[&I64, &String]]",
        "{:?}",
        checked.messages()
    );
}

// --- the third tranche: reduce, product ------------------------------------

/// **`reduce`'s result is its accumulator's type**, and its closure takes the
/// accumulator and the item: `(acc, x) giving ...`, the multi-parameter form
/// (`def-and-lambda.md` §4.5). Both parameters are typed from the declaration,
/// so `acc + x` over an `Int` and a borrowed `Int` checks clean and the call is
/// an `Int`.
#[test]
fn reduce_is_typed_by_its_accumulator() {
    let checked = check(
        "\
def total(xs: &Array[Int]) -> Int:
    xs.iterate().reduce(0, (acc, x) giving acc + x)

def widest(xs: &Array[F64]) -> F64:
    xs.iterate().map(each * 2.0).reduce(0.0, (a, b) giving if b > a: b else: a)
",
    );
    checked.assert_clean();
    assert_eq!(terminal_type(&checked, "total", "reduce"), "I64");
    assert_eq!(terminal_type(&checked, "widest", "reduce"), "F64");
}

/// **A closure with the wrong number of parameters is refused**, where it used
/// to be accepted in silence: the arity of the closure a position takes is
/// part of its type, and `reduce`'s fold takes two.
#[test]
fn a_closure_of_the_wrong_arity_is_refused() {
    let checked = check(
        "\
def one(xs: &Array[Int]) -> Int:
    xs.iterate().reduce(0, x giving x)

def three(xs: &Array[Int]) -> Int:
    xs.iterate().reduce(0, (a, b, c) giving a)
",
    );
    assert_eq!(checked.codes(), vec![527, 527], "{:?}", checked.messages());
}

/// **`product()` over non-numbers is `SC0547`**, `sum()`'s refusal, and names
/// itself in the message.
#[test]
fn product_over_non_numeric_items_is_refused() {
    let checked = check(
        "\
type Doc:
    title: String

def f(docs: &Array[Doc]) -> Int:
    let t be docs.iterate().product()
    0
",
    );
    assert_eq!(checked.codes(), vec![547], "{:?}", checked.messages());
    assert!(checked.messages()[0].contains("`product()`"), "{:?}", checked.messages());
}

// --- keep_some, accumulate --------------------------------------------------

/// **`keep_some()` peels the nullable off the item**, and a borrow of a
/// nullable of a type that owns nothing with it: an `Array[Int?]` iterates
/// `&Int?` and `keep_some()` yields `Int`. The declaration says the item and
/// `chain_item_shape` does the peeling, as `chain_total` does for `sum()`.
#[test]
fn keep_some_peels_the_nullable_off_the_item() {
    let checked = check(
        "\
def half(n: Int) -> Int?:
    null

def from_a_map(xs: &Array[Int]) -> Array[Int]:
    xs.iterate().map(x giving half(x)).keep_some().collect()

def through_a_borrow(xs: &Array[Int?]) -> Array[Int]:
    xs.iterate().keep_some().collect()
",
    );
    checked.assert_clean();
    assert_eq!(terminal_type(&checked, "from_a_map", "collect"), "Array[I64]");
    assert_eq!(terminal_type(&checked, "through_a_borrow", "collect"), "Array[I64]");
}

/// **A `keep_some()` over items that cannot be absent is `SC0549`**, once, and
/// the chain after it is silent rather than a cascade.
#[test]
fn keep_some_over_items_that_cannot_be_absent_is_refused() {
    let checked = check(
        "\
def f(xs: &Array[Int]) -> Int:
    xs.iterate().keep_some().count()
",
    );
    assert_eq!(checked.codes(), vec![549], "{:?}", checked.messages());
    assert!(checked.messages()[0].contains("`keep_some()`"), "{:?}", checked.messages());
}

/// **`accumulate` yields its state, so the state must own nothing**: a number
/// is a state and a `String` is `SC0549` — the chain would have to clone it
/// per item, and the note does not say that it does.
#[test]
fn accumulate_is_typed_by_its_state_and_refuses_one_that_owns() {
    let checked = check(
        "\
def running(xs: &Array[Int]) -> Array[Int]:
    xs.iterate().accumulate(0, (acc, x) giving acc + x).collect()
",
    );
    checked.assert_clean();
    assert_eq!(terminal_type(&checked, "running", "collect"), "Array[I64]");
    let refused = check(
        "\
def f(xs: &Array[Int]) -> Int:
    xs.iterate().accumulate(\"\", (acc, x) giving acc).count()
",
    );
    assert_eq!(refused.codes(), vec![549], "{:?}", refused.messages());
}

// --- unique -----------------------------------------------------------------

/// **`unique()` keeps the item type and admits only keys a `Set` can file and
/// the loop can copy**: integers, `Bool`, `Char` and `String`, borrowed or
/// owned. A float (no equivalence) or a record is `SC0549`.
#[test]
fn unique_keeps_the_item_and_refuses_what_cannot_be_a_key() {
    let checked = check(
        "\
def ints(xs: &Array[Int]) -> Array[&Int]:
    xs.iterate().unique().collect()

def words(xs: &Array[String]) -> Array[String]:
    xs.iterate().map(each.clone()).unique().collect()
",
    );
    checked.assert_clean();
    assert_eq!(terminal_type(&checked, "ints", "collect"), "Array[&I64]");
    let windows = check(
        "\
def runs(xs: &Array[Int]) -> Array[Array[&Int]]:
    xs.iterate().windows(3).collect()
",
    );
    windows.assert_clean();
    assert_eq!(terminal_type(&windows, "runs", "collect"), "Array[Array[&I64]]");
    let flat = check(
        "\
def through_borrows(xs: &Array[Array[Int]]) -> Array[&Int]:
    xs.iterate().flatten().collect()

def owned(xs: &Array[Int]) -> Array[Int]:
    xs.iterate().map(x giving [x + 0, x + 0]).flatten().collect()

def refused(xs: &Array[Int]) -> Int:
    xs.iterate().flatten().count()
",
    );
    assert_eq!(flat.codes(), vec![549], "{:?}", flat.messages());
    assert!(flat.messages()[0].contains("`flatten()`"), "{:?}", flat.messages());
    assert_eq!(terminal_type(&flat, "through_borrows", "collect"), "Array[&I64]");
    assert_eq!(terminal_type(&flat, "owned", "collect"), "Array[I64]");
    let refused = check(
        "\
def f(xs: &Array[F64]) -> Int:
    xs.iterate().unique().count()
",
    );
    assert_eq!(refused.codes(), vec![549], "{:?}", refused.messages());
    assert!(refused.messages()[0].contains("`unique()`"), "{:?}", refused.messages());
}
