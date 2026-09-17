//! What a literal's inference variable may unify with — `check`'s §5.
//!
//! > **Decision 2.** Numeric literals are *"inferred, not defaulted, within a
//! > body, and default to `I64` and `F64` when unconstrained"*.
//!
//! **Inferred among the *numeric* types.** That is the claim this file exists
//! for, because the checker used to read the decision as *"a literal has no
//! type yet"* and stop there: the refusal was a list of four prelude names, so
//! `by_value(42)` at a `value: Doc` parameter checked clean and bound the
//! literal's variable to a record. A missing diagnostic is the direction that
//! does not announce itself — nothing in the corpus, in `tests/checking.rs` or
//! in `tests/inference.rs` was looking at a program that never compiled to
//! begin with — so every shape a number is *not* gets a test named after it
//! here, in the manner `tests/unchecked_errors.rs` argues for: *"the happy path
//! is what the implementation was written against and the exclusions are what
//! it was not"*.
//!
//! **And every refusal is paired with the admission it must not swallow.** §5's
//! whole discipline is that a literal is refused only where this phase can
//! classify the type and admitted wherever it cannot, so a file of refusals
//! alone would pass just as well against a checker that had become useless in
//! the other direction. `1` still reaches an `F64`, a type parameter and a
//! `Self`, and an unconstrained one is still an `I64`.
//!
//! `null` is the same rule read the other way and is in the second half.

mod support;

use science_types::thir::ExprKind;
use support::check;

/// One record, one choice, one interface and one implementation: every shape
/// §5 classifies, declared once.
const FIXTURE: &str = "\
type Doc:
    title: String

choice Colour:
    Red
    Green

interface Summarize:
    def preview(self) -> String

Doc implements Summarize:
    def preview(self) -> String:
        self.title

type Celsius is F64
";

fn program(body: &str) -> support::Checked {
    check(&format!("{FIXTURE}\n{body}"))
}

/// The codes a body reports, for a claim that is about *which* diagnostic.
fn codes(body: &str) -> Vec<u16> {
    program(body).codes()
}

// --- what an integer literal is not --------------------------------------

#[test]
fn an_integer_literal_does_not_unify_with_a_record() {
    // The reported case, verbatim. `by_value(42)` against a `Doc` used to
    // check clean and leave the literal's node typed `Doc`.
    assert_eq!(
        codes(
            "\
def by_value(value: Doc) -> I64:
    1

def a() -> I64:
    by_value(42)
"
        ),
        vec![525]
    );
}

#[test]
fn an_integer_literal_does_not_unify_with_a_choice() {
    assert_eq!(
        codes(
            "\
def by_value(value: Colour) -> I64:
    1

def a() -> I64:
    by_value(42)
"
        ),
        vec![525]
    );
}

#[test]
fn an_integer_literal_does_not_unify_with_a_tuple() {
    assert_eq!(
        codes(
            "\
def by_value(value: (I64, I64)) -> I64:
    1

def a() -> I64:
    by_value(42)
"
        ),
        vec![525]
    );
}

#[test]
fn an_integer_literal_does_not_unify_with_a_closure_type() {
    assert_eq!(
        codes(
            "\
def by_value(value: (I64) -> I64) -> I64:
    1

def a() -> I64:
    by_value(42)
"
        ),
        vec![525]
    );
}

#[test]
fn an_integer_literal_does_not_unify_with_a_name_that_takes_arguments() {
    // Every numeric type in the language is a nullary name, so a name with an
    // `of` after it is a container whatever it is called.
    assert_eq!(
        codes(
            "\
def by_value(value: Array of I64) -> I64:
    1

def a() -> I64:
    by_value(42)
"
        ),
        vec![525]
    );
}

#[test]
fn an_integer_literal_does_not_unify_with_an_interface_object() {
    assert_eq!(
        codes(
            "\
def by_value(value: borrowed any Summarize) -> I64:
    1

def a() -> I64:
    by_value(42)
"
        ),
        vec![525]
    );
}

#[test]
fn an_integer_literal_does_not_unify_with_a_record_behind_an_annotation() {
    // The same refusal reached from a `let` rather than from a call, because
    // the two arrive at `demand` by different routes.
    assert_eq!(
        codes(
            "\
def a() -> I64:
    let d: Doc be 42
    1
"
        ),
        vec![525]
    );
}

#[test]
fn the_message_says_an_integer_literal_and_names_the_declared_type() {
    // Decision 1's first dividend: the expectation came from an annotation a
    // human wrote, so the message names it rather than naming a variable.
    let checked = program(
        "\
def by_value(value: Doc) -> I64:
    1

def a() -> I64:
    by_value(42)
",
    );
    assert_eq!(checked.messages(), vec!["expected `Doc`, found `an integer literal`"]);
}

// --- what an integer literal still is ------------------------------------

#[test]
fn an_integer_literal_still_reaches_every_numeric_type() {
    // §5's own example: `let x: F64 be 1` is what a scientific program writes,
    // and an integer is exact in every one of §5.1's numeric primitives.
    for ty in ["I8", "I16", "I32", "I64", "U8", "U16", "U32", "U64", "F16", "F32", "F64"] {
        let source = format!(
            "\
def by_value(value: {ty}) -> I64:
    1

def a() -> I64:
    by_value(42)
"
        );
        assert_eq!(codes(&source), Vec::<u16>::new(), "`42` should reach `{ty}`");
    }
}

#[test]
fn an_integer_literal_reaches_a_numeric_type_through_an_alias() {
    // Revealed, because `type Celsius is F64` is a floating type and the name
    // is not: the seam's middle call, at the one comparison §5 makes.
    assert_eq!(
        codes(
            "\
def by_value(value: Celsius) -> I64:
    1

def a() -> I64:
    by_value(42)
"
        ),
        Vec::<u16>::new()
    );
}

#[test]
fn an_integer_literal_at_a_type_parameter_is_admitted() {
    // A bound could make `T` numeric and this phase cannot see through one, so
    // refusing here is how a checker acquires a false positive.
    assert_eq!(
        codes(
            "\
def by_value of T(value: T) -> I64:
    1

def a() -> I64:
    by_value(42)
"
        ),
        Vec::<u16>::new()
    );
}

#[test]
fn an_integer_literal_at_self_is_admitted() {
    // `Self` inside an interface's own default body stands for every
    // implementor, which is a set this phase has not been given.
    assert_eq!(
        codes(
            "\
interface Build:
    def make(value: Self) -> I64

    def zero(self) -> I64:
        Build.make(0)
"
        ),
        Vec::<u16>::new()
    );
}

#[test]
fn an_unconstrained_integer_literal_is_still_i64() {
    // Decision 2's default, which the refusal above must not have displaced.
    let checked = program(
        "\
def a() -> I64:
    let n be 7
    n
",
    );
    checked.assert_clean();
    let literal = checked.find("a", |kind| matches!(kind, ExprKind::Literal(_)));
    assert_eq!(checked.render(checked.body("a").ty(literal)), "I64");
}

#[test]
fn a_literal_at_an_already_erroneous_type_says_nothing() {
    // `ty`'s §5: one bad annotation is one diagnostic, and the resolver
    // already reported this one.
    let checked = support::check_allowing_resolution_errors(
        "\
def by_value(value: Bogus) -> I64:
    1

def a() -> I64:
    by_value(42)
",
    );
    assert!(checked.resolution.has_errors());
    assert_eq!(checked.codes(), Vec::<u16>::new());
}

// --- a literal at a borrowed parameter -----------------------------------

#[test]
fn a_literal_at_a_borrowed_parameter_is_borrowed_rather_than_retyped() {
    // §6.3's auto-borrow reaches a literal too. Before it did, the literal's
    // variable was bound to `borrowed I64` itself: a literal whose type is a
    // reference, and no `Borrow` node for MIR to take the borrow at.
    let checked = program(
        "\
def by_ref(value: borrowed I64) -> I64:
    1

def a() -> I64:
    by_ref(42)
",
    );
    checked.assert_clean();
    let body = checked.body("a");
    let literal = checked.find("a", |kind| matches!(kind, ExprKind::Literal(_)));
    assert_eq!(checked.render(body.ty(literal)), "I64");
    let borrow = checked.find("a", |kind| matches!(kind, ExprKind::Borrow { .. }));
    assert_eq!(checked.render(body.ty(borrow)), "borrowed I64");
}

#[test]
fn a_literal_at_a_borrowed_slot_that_is_not_a_call_is_refused() {
    // §6.3 says *"at call sites"*, and a `let` is not one. Admitting it there
    // would be a language change made by a checker.
    assert_eq!(
        codes(
            "\
def a() -> I64:
    let n: borrowed I64 be 42
    1
"
        ),
        vec![525]
    );
}

// --- float literals ------------------------------------------------------

#[test]
fn a_float_literal_does_not_unify_with_a_record() {
    assert_eq!(
        codes(
            "\
def by_value(value: Doc) -> I64:
    1

def a() -> I64:
    by_value(4.5)
"
        ),
        vec![525]
    );
}

#[test]
fn a_float_literal_is_still_refused_at_an_integer_type() {
    // `let n: I32 be 1.5` is a value the target cannot hold, which is
    // Decision 2 read backwards. This held before §5 was widened and must
    // still hold after.
    assert_eq!(
        codes(
            "\
def by_value(value: I32) -> I64:
    1

def a() -> I64:
    by_value(1.5)
"
        ),
        vec![525]
    );
}

#[test]
fn a_float_literal_still_reaches_every_floating_type() {
    for ty in ["F16", "BF16", "F32", "F64", "Celsius"] {
        let source = format!(
            "\
def by_value(value: {ty}) -> I64:
    1

def a() -> I64:
    by_value(4.5)
"
        );
        assert_eq!(codes(&source), Vec::<u16>::new(), "`4.5` should reach `{ty}`");
    }
}

#[test]
fn an_unconstrained_float_literal_is_still_f64() {
    let checked = program(
        "\
def a() -> I64:
    let x be 7.5
    1
",
    );
    checked.assert_clean();
    let literal = checked.find("a", |kind| matches!(kind, ExprKind::Literal(_)));
    assert_eq!(checked.render(checked.body("a").ty(literal)), "F64");
}

// --- `null`, which is the same rule in the other direction ---------------

#[test]
fn null_does_not_unify_with_a_tuple() {
    // Decision 6 makes `T?` a distinct type precisely so that `null` inhabits
    // it and nothing else. `null` was refused only at a name, which left it
    // agreeing with every structural shape there is.
    assert_eq!(
        codes(
            "\
def by_value(value: (I64, I64)) -> I64:
    1

def a() -> I64:
    by_value(null)
"
        ),
        vec![525, 526]
    );
}

#[test]
fn null_does_not_unify_with_a_closure_type() {
    assert_eq!(
        codes(
            "\
def by_value(value: (I64) -> I64) -> I64:
    1

def a() -> I64:
    by_value(null)
"
        ),
        vec![525, 526]
    );
}

#[test]
fn null_does_not_unify_with_unit() {
    assert_eq!(
        codes(
            "\
def by_value(value: ()) -> I64:
    1

def a() -> I64:
    by_value(null)
"
        ),
        vec![525, 526]
    );
}

#[test]
fn null_does_not_unify_with_an_interface_object() {
    assert_eq!(
        codes(
            "\
def by_value(value: borrowed any Summarize) -> I64:
    1

def a() -> I64:
    by_value(null)
"
        ),
        vec![525, 526]
    );
}

#[test]
fn null_does_not_unify_with_a_non_nullable_name() {
    // The one shape `null` was already refused at, kept so that widening the
    // rule cannot quietly lose it.
    assert_eq!(
        codes(
            "\
def by_value(value: Doc) -> I64:
    1

def a() -> I64:
    by_value(null)
"
        ),
        vec![525, 526]
    );
}

#[test]
fn null_still_reaches_every_nullable_type() {
    for ty in ["Doc?", "I64?", "(I64, I64)?", "Colour?"] {
        let source = format!(
            "\
def by_value(value: {ty}) -> I64:
    1

def a() -> I64:
    by_value(null)
"
        );
        assert_eq!(codes(&source), Vec::<u16>::new(), "`null` should reach `{ty}`");
    }
}

#[test]
fn null_at_a_type_parameter_is_admitted() {
    // `T` can be instantiated at a nullable, so this is the unanswerable case
    // and §5 admits it.
    assert_eq!(
        codes(
            "\
def by_value of T(value: T) -> I64:
    1

def a() -> I64:
    by_value(null)
"
        ),
        Vec::<u16>::new()
    );
}
