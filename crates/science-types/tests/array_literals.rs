//! `indexing-and-array-literals.md` §2 and §3 — the literal, and the slice
//! that is not built.
//!
//! Both silences closed here had the same shape and it is the expensive one:
//! the program checked clean and the type was wrong. `[1, 2, 3]` lowered to a
//! resolver hole, so an annotated literal never met its annotation and an
//! unannotated one left its binding at `Ty::ERROR` — which, by `ty`'s §5,
//! agreed with every slot downstream of it. `xs[1..3]` had an index of
//! `Ty::ERROR` against an `Index` implementation wanting an `Int`, so the two
//! **agreed by cancelling** and a slice came out as one element.
//!
//! **Every case is a pair**, because a check that refuses everything is not
//! progress: the wrong program is refused *and* the right one still passes,
//! with the type it should have. That is the discipline
//! `tests/operators.rs`' index section states one construct along, and these
//! two constructs are the same section of the same note.
//!
//! The whole pipeline runs, for `tests/support`'s reason — the contract is
//! *"what the author wrote checks"* — and because both decisions need a **real
//! prelude**: Decision 10 says the result is an `Array of T` **always**, which
//! means the checker has to be able to name `Array`, and §2's refusal renders
//! the type it could not slice.

mod support;

use science_types::thir::ExprKind;

/// The type the named local ended up with.
fn local_ty(checked: &support::Checked, body: &str, name: &str) -> String {
    let body = checked.body(body);
    let def = checked
        .krate
        .defs
        .iter()
        .find(|def| def.name == name && body.local_ty(def.id).is_some())
        .unwrap_or_else(|| panic!("the body binds no local named `{name}`"));
    checked.render(body.local_ty(def.id).expect("the local has a type"))
}

// --- Decision 10: the type is `Array of T`, always ------------------------

#[test]
fn an_unannotated_literal_takes_the_unification_of_its_elements() {
    let checked = support::check(
        "\
def counts() -> Bool:
    let counts be [1, 2, 3]
    true
",
    );
    checked.assert_clean();
    // Decision 2's defaults, applied to the element class: an unsuffixed
    // integer literal is `I64`. §3.2 says so of a literal array in as many
    // words.
    assert_eq!(local_ty(&checked, "counts", "counts"), "Array[I64]");
}

#[test]
fn an_unannotated_float_literal_defaults_to_f64() {
    let checked = support::check(
        "\
def weights() -> Bool:
    let weights be [0.1, 0.5, 0.4]
    true
",
    );
    checked.assert_clean();
    assert_eq!(local_ty(&checked, "weights", "weights"), "Array[F64]");
}

#[test]
fn a_nested_literal_is_an_array_of_arrays() {
    // §3.1's own third example. Nothing special happens: the elements are
    // literals like any other and the unification recurses through `synth`.
    let checked = support::check(
        "\
def identity() -> Bool:
    let identity be [[1.0, 0.0], [0.0, 1.0]]
    true
",
    );
    checked.assert_clean();
    // **This used to pin a known-wrong rendering, and the bracket revision
    // fixed it as a side effect.** `render_args` used to add parentheses
    // around a nested application only when there was more than one
    // argument — `Array of (Array of F64)` needed them and `render_args`
    // never wrote them for a single argument, so `Array of Array of F64`
    // came out ambiguous with no bracket of its own to say where the inner
    // list ended. Brackets close what they open: `Array[Array[F64]]` needs no
    // parenthesis to tell the outer `]` from the inner one, so
    // `Types::render_args`'s own comment now reads "one argument and several
    // are printed the same way" and this is the case that used to be the
    // exception.
    assert_eq!(local_ty(&checked, "identity", "identity"), "Array[Array[F64]]");
}

#[test]
fn a_literal_no_longer_makes_its_binding_agree_with_everything() {
    // The second half of the silence, and the expensive half: the literal used
    // to lower to a hole, so `counts` was `Ty::ERROR` and every check
    // downstream of it was vacuous.
    let checked = support::check(
        "\
def flagged() -> Bool:
    let counts be [1, 2, 3]
    let flag: Bool be counts
    flag
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(checked.messages(), vec!["expected `Bool`, found `Array[I64]`"]);
}

// --- §3.2: the elements unify, with no implicit numeric conversion --------

#[test]
fn elements_that_disagree_are_reported_with_both_spans() {
    // §3.2's own example: *"`[1, 2.0]` — `SC0281`, `Int` and `F64` do not
    // unify"*. Neither element is more right than the other, which is why this
    // is not `SC0525`: there is no annotation for an *expected* type to come
    // from, so the message shows the element that fixed the type as well.
    let checked = support::check(
        "\
def bad() -> Bool:
    let bad be [1, 2.0]
    true
",
    );
    assert_eq!(checked.codes(), vec![281]);
    assert_eq!(
        checked.messages(),
        vec!["the elements of this array literal do not have one type"]
    );
    let diagnostic = checked.diagnostics.iter().next().expect("one diagnostic");
    // §7.2: *"primary on the first element that differs, secondary on the
    // element that fixed the type"*.
    assert_eq!(diagnostic.labels.len(), 2);
}

#[test]
fn a_literal_against_a_type_is_reported_rather_than_bound_to_it() {
    // The case `Inference::bind` cannot answer on its own: an unbound class
    // accepts any type it is handed, so without §5's admission question the
    // integer's class would simply become `String`.
    let checked = support::check(
        "\
def mixed() -> Bool:
    let mixed be [\"alpha\", 1]
    true
",
    );
    assert_eq!(checked.codes(), vec![281]);
}

#[test]
fn two_values_of_different_declared_types_do_not_unify() {
    let checked = support::check(
        "\
type Doc:
    title: String

type Excerpt:
    body: String

def both(doc: Doc, excerpt: Excerpt) -> Bool:
    let both be [doc, excerpt]
    true
",
    );
    // Unification and not assignability — `infer`'s §5. There is no slot here,
    // so nothing makes these two an `Array of any Summarize` even if both
    // implemented one.
    assert_eq!(checked.codes(), vec![281]);
}

#[test]
fn an_integer_literal_still_reaches_a_float_element_type() {
    // §5.1 refuses *implicit numeric conversion*, and an unsuffixed literal is
    // not one: `let x: F64 be 1` is what a scientific program writes, and the
    // rule that admits it is `literal_admits`, asked here through the
    // annotation rather than at a slot.
    let checked = support::check(
        "\
def weights() -> Bool:
    let weights: Array[F64] be [1, 2.5, 3]
    true
",
    );
    checked.assert_clean();
    assert_eq!(local_ty(&checked, "weights", "weights"), "Array[F64]");
}

// --- Decision 11: the empty literal --------------------------------------

#[test]
fn an_empty_literal_with_no_expected_type_is_reported() {
    let checked = support::check(
        "\
def nothing() -> Bool:
    let nothing be []
    true
",
    );
    assert_eq!(checked.codes(), vec![282]);
    assert_eq!(
        checked.messages(),
        vec!["this empty array literal has no element type"]
    );
}

#[test]
fn an_empty_literal_takes_its_element_type_from_an_annotation() {
    // §3.3's first example, and the reason the bidirectional push exists at
    // all: *"the empty literal appears mostly as a default … exactly where an
    // expected type is available, and refusing it there would be pedantry"*.
    let checked = support::check(
        "\
def empty() -> Bool:
    let empty: Array[F64] be []
    true
",
    );
    checked.assert_clean();
    assert_eq!(local_ty(&checked, "empty", "empty"), "Array[F64]");
}

#[test]
fn an_empty_literal_takes_its_element_type_from_a_parameter() {
    // §3.3's second example, `push_all(results, [])`. The expectation arrives
    // at `Site::Argument` and the arm is the same one.
    support::check(
        "\
def push_all(into: Array[F64], more: Array[F64]) -> Bool:
    true

def run(results: Array[F64]) -> Bool:
    push_all(results, [])
",
    )
    .assert_clean();
}

#[test]
fn an_empty_literal_at_a_type_that_is_not_an_array_is_still_reported() {
    // The expectation exists and gives no element type, so §3.3's question —
    // *"what does `[]` hold"* — is still unanswered. One diagnostic and not
    // two: the literal takes `Ty::ERROR`, which `ty`'s §5 makes agree with the
    // `Bool` it was assigned to.
    let checked = support::check(
        "\
def nothing() -> Bool:
    let flag: Bool be []
    flag
",
    );
    assert_eq!(checked.codes(), vec![282]);
}

// --- the bidirectional half: an annotation is `SC0525`, not `SC0281` ------

#[test]
fn an_annotated_literal_checks_its_elements_against_the_annotation() {
    // The headline silence: this checked clean. It is `SC0525` and not
    // `SC0281` because there *is* an expected type and it came from an
    // annotation a human wrote, which is Decision 1's whole shape — so the
    // message names `String` rather than pointing at a sibling element.
    let checked = support::check(
        "\
def counts() -> Bool:
    let counts: Array[String] be [1, 2, 3]
    true
",
    );
    assert_eq!(checked.codes(), vec![525, 525, 525]);
    assert!(checked.messages().iter().all(|message| message.starts_with("expected `String`")));
}

#[test]
fn an_annotated_literal_whose_elements_fit_still_passes() {
    let checked = support::check(
        "\
def names() -> Bool:
    let names: Array[String] be [\"alpha\", \"beta\", \"gamma\"]
    true
",
    );
    checked.assert_clean();
    assert_eq!(local_ty(&checked, "names", "names"), "Array[String]");
}

#[test]
fn an_alias_of_an_array_still_admits_a_literal() {
    // The annotation is revealed before its element type is read, which is
    // `lib.rs` §5's second call: `type Embedding is Array of F32` is one type
    // with `Array of F32` and a literal reaches it.
    support::check(
        "\
type Embedding is Array[F32]

def make() -> Bool:
    let e: Embedding be [1.0f32, 2.0f32]
    true
",
    )
    .assert_clean();
}

#[test]
fn a_synthesised_literal_is_the_same_type_the_prelude_writes() {
    // The interning check, and it is not pedantry: `array_of` builds `Array of
    // T` from `Prelude::get("Array")`, and a resolver-written `Array of I64`
    // in a signature has to be the *same* `Ty` or nothing a literal produces
    // would ever reach a declared parameter. §6.3's auto-borrow rides on top,
    // which is the second half of what this passes through.
    support::check(
        "def total(xs: &Array[I64]) -> I64:
    0

def run() -> I64:
    total([1, 2, 3])
",
    )
    .assert_clean();
}

// --- the node, and what it is honest about -------------------------------

#[test]
fn the_literal_is_a_typed_hole_and_not_an_untyped_one() {
    // **This test used to pin a hole, and the hole is closed.** The sentence
    // it asserted was *"the node is `ExprKind::Error`, and its type is not
    // `Ty::ERROR`"* — true while THIR had no array-literal variant, so the
    // literal's *shape* stopped in this crate and only its *type* travelled.
    // `thir::ExprKind::Array` is that variant, the three exhaustive matches in
    // `science-mir` that its doc comment named as the cost have their arms,
    // and `[1, 2, 3]` now reaches the LLVM backend as Decision 5's
    // `science_array_with_capacity` and one `science_array_push` per element.
    //
    // What the test is *for* is unchanged and is why it keeps its name: the
    // literal carries a real `Array of I64` and not `Ty::ERROR`, because that
    // type is what a declared parameter is checked against and `ty`'s §5
    // absorption would have made an erroneous one agree with everything. The
    // half that changed is the node beside it.
    let checked = support::check(
        "\
def counts() -> Bool:
    let counts be [1, 2, 3]
    true
",
    );
    checked.assert_clean();
    let body = checked.body("counts");
    let literal = body
        .exprs()
        .find(|(_, expr)| matches!(expr.kind, ExprKind::Array(_)))
        .expect("the literal is an array node");
    assert_eq!(checked.render(literal.1.ty), "Array[I64]");
    let ExprKind::Array(elements) = &literal.1.kind else { unreachable!() };
    assert_eq!(elements.len(), 3, "the elements travel, in the order they were written");
    assert!(
        !body.exprs().any(|(_, expr)| matches!(expr.kind, ExprKind::Error)),
        "a clean program has no hole left in it"
    );
}

// --- §2: the slice that is not built -------------------------------------

#[test]
fn a_closed_range_index_is_refused() {
    // This is the whole of hole 2: the range carries no type, `Index.index`
    // wants an `Int`, and `Ty::ERROR` agreed with it — so `xs[1..3]` came out
    // as one `borrowed I64` and was returned as an `I64` with nothing said.
    let checked = support::check(
        "\
def slice(xs: &Array[I64]) -> I64:
    let s be xs[1..3]
    s
",
    );
    assert_eq!(checked.codes(), vec![538]);
    assert_eq!(
        checked.messages(),
        vec!["a range index would produce a slice, and there is no `Slice` type"]
    );
}

#[test]
fn an_ordinary_index_is_untouched() {
    // The other half of the pair. §2's refusal is about a *range* written in
    // an index bracket and about nothing else.
    support::check(
        "\
def first(xs: &Array[I64]) -> I64:
    xs[0]
",
    )
    .assert_clean();
}

#[test]
fn an_inclusive_range_index_is_refused_too() {
    let checked = support::check(
        "\
def slice(xs: &Array[I64]) -> I64:
    let s be xs[1..=3]
    s
",
    );
    assert_eq!(checked.codes(), vec![538]);
}

#[test]
fn a_range_driven_loop_is_untouched() {
    // `for i in 0..n:` is the construct that made the range's `Ty::ERROR`
    // acceptable in the first place, and refusing it would be refusing the
    // only thing ranges are for today.
    support::check(
        "\
def total(n: I64) -> I64:
    let mutable sum be 0
    for i in 0..n:
        sum be sum + i
    sum
",
    )
    .assert_clean();
}

#[test]
fn a_range_passed_to_a_method_is_not_an_index() {
    // `String.slice(0..4)` is a call, not an index bracket. `methods`' §8
    // leaves a prelude head's method set open, so this is silent — and it is
    // silent for the reason it was before this change, not because of it.
    support::check(
        "\
def head(text: &String) -> Bool:
    let s be text.slice(0..4)
    true
",
    )
    .assert_clean();
}
