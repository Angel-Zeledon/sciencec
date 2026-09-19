//! §4.5's `a..b`, and the type it did not have.
//!
//! `0..n` is half-open, `0..=n` inclusive, and `for i in 0..n:` is the counting
//! loop — §4.5 gives the expression and stops there. It gives no *type*, and
//! for as long as there was none `check`'s `synth` said so in a comment and
//! wrote [`Ty::ERROR`]: *"§6: there is no `Range` type in the prelude to give
//! this"*.
//!
//! **An error type is not a missing type; it is a type that agrees with
//! everything.** `ty`'s §5 absorption is deliberate — one mistake costs one
//! diagnostic — so a construct that is typed `Ty::ERROR` *with no diagnostic*
//! spends that absorption on a program nobody reported anything about. Three
//! things followed, and this file is one test per thing:
//!
//! 1. `for i in 0..3:` bound `i` at `Ty::ERROR`, so the loop variable of the
//!    most-written loop in the language was the one binding in a checked
//!    program no rule could read. `mutability.rs` holds that flip, because the
//!    rule it unblocks is that file's.
//! 2. A range written anywhere *other* than a `for`'s iterable position —
//!    `f(0..3)`, `let r be 0..5` handed on — checked against anything at all.
//! 3. The two ends of a range were checked against each other only when the
//!    first one already had a type, which the corpus's own form does not:
//!    `0..width` starts at a literal.
//!
//! `builtins.rs` now declares `Range of T implements Iterate: type Item is T`
//! — `collections-and-chains.md`'s AMENDMENT 14, *"`Range` is not a container
//! and implements `Iterate` directly, which is what makes `for i in 0..n:` the
//! same construct as everything else rather than a special case in the
//! parser"* — and `check`'s `range_expr` builds the type.
//!
//! **Every case here is a pair**, for `array_literals.rs`' reason: a rule that
//! refuses everything is not a rule. So each refusal sits beside the program it
//! must not touch, and the corpus's own spellings — `0..width`, `first..last`,
//! `1..=limit` — are the ones on the accepting side.

mod support;

use science_types::thir::ExprKind;

/// The type of the range node itself, as a diagnostic would spell it.
fn range_ty(source: &str, function: &str) -> String {
    let checked = support::check(source);
    let id = checked.find(function, |kind| matches!(kind, ExprKind::Range { .. }));
    checked.render(checked.body(function).ty(id))
}

// --- the type -------------------------------------------------------------

/// **Decision 2's default reaches the element**, and it is not this
/// construct's decision. `0..5` is two unsuffixed integer literals, so `T` is
/// `I64` for the reason `[1, 2]` is an `Array of I64`: the class has to be a
/// real type before `Range of T` can be interned, so `check`'s `range_expr`
/// runs the default where `array_lit` runs it, one construct over.
#[test]
fn a_range_of_literals_is_a_range_of_the_default_integer() {
    assert_eq!(
        range_ty(
            "\
def counted() -> Bool:
    let r be 0..5
    true
",
            "counted",
        ),
        "Range of I64",
    );
}

/// **The concrete end fixes the element whichever side it is on.** This is the
/// corpus's own spelling — `examples/10_loops.science`' `for i in 0..width:`
/// with `width: Int` — and it is why the two ends are unified rather than the
/// second being checked against the first. Checking would push the *start's*
/// unresolved class inward and report `Int` against a hole.
#[test]
fn a_literal_start_takes_its_type_from_the_end() {
    assert_eq!(
        range_ty(
            "\
def ruler(width: Int) -> Bool:
    let r be 0..width
    true
",
            "ruler",
        ),
        "Range of I64",
    );
}

/// And the other direction, which is the same unification read backwards.
#[test]
fn a_literal_end_takes_its_type_from_the_start() {
    assert_eq!(
        range_ty(
            "\
def ruler(first: I32) -> Bool:
    let r be first..8
    true
",
            "ruler",
        ),
        "Range of I32",
    );
}

/// `..=` is the same type as `..`. The inclusivity is a fact about which
/// values the iteration visits and not about what it yields, so nothing in the
/// type may depend on it — `examples/10`'s `triangular` writes `0..=n` and its
/// `sum` is the same `Int` `ruler`'s is.
#[test]
fn an_inclusive_range_has_the_same_type_as_a_half_open_one() {
    let half_open = range_ty(
        "\
def counted(n: Int) -> Bool:
    let r be 0..n
    true
",
        "counted",
    );
    let inclusive = range_ty(
        "\
def counted(n: Int) -> Bool:
    let r be 0..=n
    true
",
        "counted",
    );
    assert_eq!(half_open, inclusive);
    assert_eq!(inclusive, "Range of I64");
}

// --- the loop, which is what the type is for ------------------------------

/// The corpus's counting loop, unchanged and still clean.
///
/// It reads its `i` through the same `iterate_item` that `for c in
/// text.chars()` reads its `Char` through — `Item is T` answered with `Int` —
/// so the binding is an `Int` and `sum be sum + i` is `Int + Int`. Before
/// this, `i` was `Ty::ERROR` and the addition was checked against nothing.
#[test]
fn the_counting_loop_binds_its_variable_at_the_element_type() {
    let checked = support::check(
        "\
def triangular(n: Int) -> Int:
    let mutable sum be 0
    for i in 0..=n:
        sum be sum + i
    sum
",
    );
    checked.assert_clean();
}

/// The same loop with the accumulator pinned to the *other* integer type, which
/// is the first thing the element type is able to say.
///
/// **The disagreement this test was written for is settled, and the test now
/// pins the settlement.** It read: *"`sum` is an `I64` by annotation and `i` is
/// an `Int` off the range, and §5.1 keeps those two primitives apart …  the
/// point here is not which side is right; it is that the checker can now see
/// there are two sides."* There are no longer two sides.
/// `codegen-and-linking.md` §4 says ***"`Int` is `I64`"*** and
/// `indexing-and-array-literals.md` §3.2 writes *"an unsuffixed integer literal
/// is `Int` (`I64`)"*; `builtins.rs` declared them as two primitives anyway,
/// which made them two types that do not unify. They are one definition now,
/// so the first program below checks clean.
///
/// **The test keeps its name and its job**, because the job was never about
/// `Int` in particular: a loop variable that meets an integer of a *different
/// width* is still `SC0525`, and that is the half worth guarding. `I32` is the
/// wrong integer here in the way `Int` never really was.
#[test]
fn a_loop_variable_that_meets_the_wrong_integer_is_reported() {
    // One type, two spellings: nothing to report.
    support::check(
        "\
def triangular(n: Int) -> I64:
    let mutable sum: I64 be 0
    for i in 0..=n:
        sum be sum + i
    sum
",
    )
    .assert_clean();

    // A genuinely different width still is.
    let checked = support::check(
        "\
def triangular(n: Int) -> I32:
    let mutable sum: I32 be 0
    for i in 0..=n:
        sum be sum + i
    sum
",
    );
    assert_eq!(checked.codes(), vec![525]);
}

/// A range bound to a name and iterated through it, which is the shape that had
/// no type at all to carry: `for` never met the `..`, only the local.
#[test]
fn a_range_may_be_bound_to_a_name_and_iterated_through_it() {
    let checked = support::check(
        "\
def counted(n: Int) -> Int:
    let mutable sum be 0
    let r be 0..n
    for i in r:
        sum be sum + i
    sum
",
    );
    checked.assert_clean();
}

// --- the diagnostics, which are the whole point ---------------------------

/// **Cost 2, closed.** `f(0..3)` where `f` takes an `Int` used to check: the
/// range was `Ty::ERROR`, the parameter was `Int`, and `ty`'s §5 made them
/// agree. The message names `Range of I64` rather than saying nothing, which
/// is the difference between a type and the absence of one.
#[test]
fn a_range_handed_to_a_function_that_wants_a_number_is_a_mismatch() {
    let checked = support::check(
        "\
def double(n: Int) -> Int:
    n + n

def caller() -> Int:
    double(0..3)
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(checked.messages(), vec!["expected `I64`, found `Range of I64`"]);
}

/// Its pair: the argument that is a number is untouched, so the rule above is
/// about ranges and not about the call.
#[test]
fn a_number_handed_to_the_same_function_is_not() {
    let checked = support::check(
        "\
def double(n: Int) -> Int:
    n + n

def caller(n: Int) -> Int:
    double(n)
",
    );
    checked.assert_clean();
}

/// **Cost 3, closed.** Two ends that are not one type are a mismatch at the end
/// that broke it, and one mistake costs one diagnostic: the range keeps the
/// start's type so nothing downstream reports the wreckage a second time.
#[test]
fn two_ends_of_different_types_are_reported_once() {
    let checked = support::check(
        "\
def counted(n: Int, text: String) -> Bool:
    let r be n..text
    true
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(checked.messages(), vec!["expected `I64`, found `String`"]);
}

/// The same disagreement between two *literal classes*, where neither end has a
/// type yet. `render_infer`'s wording is the one `[1, 2.0]` already gets, for
/// the same reason: naming a default the author did not write would be the
/// checker inventing half the sentence.
#[test]
fn two_ends_of_different_literal_classes_are_reported_by_class() {
    let checked = support::check(
        "\
def counted() -> Bool:
    let r be 0..5.0
    true
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(
        checked.messages(),
        vec!["expected an integer literal, found a floating-point literal"],
    );
}

/// `Range of T` is a name a signature may write, which is the test that the
/// type is a real prelude declaration rather than something `check` invents at
/// the expression. `data-io.md` §2 already writes `range: Range of U64` in a
/// parameter list.
#[test]
fn a_range_may_be_written_in_a_signature_and_returned() {
    let checked = support::check(
        "\
def upto(n: Int) -> Range of Int:
    0..n
",
    );
    checked.assert_clean();
}

/// And its pair, which is the same signature meeting a range of the other
/// element type. Without this, `Range of Int` in a return position would be
/// satisfied by any range at all and the declaration above would be decoration.
#[test]
fn a_returned_range_must_have_the_element_type_the_signature_says() {
    let checked = support::check(
        "\
def upto(n: I32) -> Range of Int:
    0..n
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(checked.messages(), vec!["expected `Range of I64`, found `Range of I32`"]);
}

// --- the index bracket, which this change had to leave alone ---------------

/// `SC0538` survives the new type, and that is deliberate.
///
/// The code's own note said a range expression had no type and so the untyped
/// index and `Index of Int`'s `Int` *"agreed by cancelling"* and `xs[1..3]`
/// came out as one element. The type closes the cancelling; it does not build
/// `Slice of T`, which is what the expression's *result* still has no spelling
/// for. So the syntactic refusal stays exactly where it was.
#[test]
fn a_range_written_in_an_index_bracket_is_still_the_slice_refusal() {
    let checked = support::check(
        "\
def middle(xs: borrowed Array of Int) -> Bool:
    let cell be xs[1..3]
    true
",
    );
    assert_eq!(checked.codes(), vec![538]);
}

/// And the case that *moved*: a range reached through a name.
///
/// This used to be unreachable — there was no range value to bind — and it is
/// not `SC0538`, because that code is about a slice the author did not ask
/// for. It is the ordinary mismatch, which names both types.
#[test]
fn a_range_bound_to_a_name_and_used_as_an_index_is_an_ordinary_mismatch() {
    let checked = support::check(
        "\
def middle(xs: borrowed Array of Int) -> Bool:
    let r be 1..3
    let cell be xs[r]
    true
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(checked.messages(), vec!["expected `I64`, found `Range of I64`"]);
}

/// **What is *not* closed, pinned as the silence it is.** `T` is unbounded:
/// nothing refuses a range over a type that cannot be stepped, because the
/// interface that would refuse it — a `Step`, or §5.4's `Ord` used for the
/// purpose — is not written in any note. So this program checks, and the loop
/// it would feed yields a `String` nothing can produce.
///
/// **This is deliberately not fixed here.** Inventing the bound means choosing
/// which interface a range's element must implement, declaring it in
/// `builtins.rs` for every integer primitive, and deciding what `Float` does —
/// a spec decision in a file that transcribes spec decisions. What the checker
/// *can* say without inventing anything is that the two ends must agree, and it
/// says it above.
#[test]
fn a_range_over_a_type_that_cannot_be_stepped_is_accepted_and_should_not_be() {
    let checked = support::check(
        "\
def counted(first: String, last: String) -> Bool:
    let r be first..last
    true
",
    );
    checked.assert_clean();
    assert_eq!(
        range_ty(
            "\
def counted(first: String, last: String) -> Bool:
    let r be first..last
    true
",
            "counted",
        ),
        "Range of String",
    );
}
