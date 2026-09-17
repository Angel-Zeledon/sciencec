//! Generic bounds, held at the call site — `check`'s §8 and `methods`'s §7.
//!
//! A bound was declared, parsed, resolved and carried into
//! `items::Signature::bounds`, and then nothing read it. `describe(n)` at an
//! `I64` checked clean against `def describe of T: Summarize(..)`, which is
//! worse than a missing feature: the *body* of `describe` is checked on the
//! strength of the bound — `value.preview()` resolves because `T` implements
//! `Summarize` — so the promise was being spent at one end and never collected
//! at the other. It is also what made a solved type parameter agree with
//! anything, because nothing else constrains one.
//!
//! **Two halves, and the second is the one that could go wrong quietly.** The
//! refusals are easy to get right and easy to test. The admissions are not:
//! `builtins.rs` declares seventeen interfaces and **no implementation of any**,
//! so a checker that treated `Methods::implements`'s `false` as evidence would
//! report `T: Ord` at an `I64` and every numeric program in the language with
//! it. `Methods::answers_for` is the line, `methods`'s §7 is the argument, and
//! the tests below say both sides of it out loud — including the cost, which is
//! that `T: Clone` is not checked at all.
//!
//! The corpus is the other witness: all five bounded generics in `examples/`
//! are declared `borrowed T`, four of their five bounds are at prelude
//! interfaces, and the whole directory still reports nothing.

mod support;

use science_diagnostics::{render, SourceMap};
use support::check;

/// One user interface with one implementor, which is every answerable case,
/// and one prelude interface reachable by name for the unanswerable one.
const FIXTURE: &str = "\
interface Summarize:
    def preview(self) -> String

type Doc:
    title: String

Doc implements Summarize:
    def preview(self) -> String:
        self.title

type Plain:
    n: I64

def describe of T: Summarize(value: borrowed T) -> String:
    value.preview()
";

fn program(body: &str) -> support::Checked {
    check(&format!("{FIXTURE}\n{body}"))
}

fn codes(body: &str) -> Vec<u16> {
    program(body).codes()
}

// --- the bound is enforced ------------------------------------------------

#[test]
fn a_type_that_does_not_implement_the_interface_is_reported() {
    // The reported case, verbatim.
    assert_eq!(
        codes(
            "\
def bad(n: I64) -> String:
    describe(n)
"
        ),
        vec![534]
    );
}

#[test]
fn a_user_type_with_no_implementation_is_reported_too() {
    // Not only a prelude type: the index holds every `T implements I:` the
    // crate wrote, so its `false` about a record is a fact.
    assert_eq!(
        codes(
            "\
def bad(p: borrowed Plain) -> String:
    describe(p)
"
        ),
        vec![534]
    );
}

#[test]
fn a_bound_written_in_a_where_clause_is_the_same_bound() {
    // `examples/07_generics.science` writes both spellings and comments that
    // the second *"keeps a long signature readable"*. A rule that depended on
    // which one the author reached for would not be a rule.
    assert_eq!(
        codes(
            "\
def show of T(value: borrowed T) -> String where T: Summarize:
    value.preview()

def bad(n: I64) -> String:
    show(n)
"
        ),
        vec![534]
    );
}

#[test]
fn an_explicit_type_argument_is_held_to_the_bound() {
    // Writing the argument out does not buy an exemption from it.
    assert_eq!(
        codes(
            "\
def bad(n: borrowed I64) -> String:
    describe of I64(n)
"
        ),
        vec![534]
    );
}

#[test]
fn a_bound_on_a_method_is_checked_as_well_as_one_on_a_function() {
    // Both call forms go through `instantiate_call`, so both go through the
    // check that follows it.
    assert_eq!(
        codes(
            "\
Plain has:
    def show of T: Summarize(self, value: borrowed T) -> String:
        value.preview()

def bad(p: borrowed Plain, n: I64) -> String:
    p.show(n)
"
        ),
        vec![534]
    );
}

#[test]
fn a_parameter_with_two_bounds_reports_the_one_that_fails() {
    // One diagnostic per unsatisfied bound, and the satisfied half stays
    // quiet: `Doc` implements `Summarize` and no `Render` block exists.
    assert_eq!(
        codes(
            "\
interface Render:
    def render(self) -> String

def both of T: Summarize + Render(value: borrowed T) -> String:
    value.preview()

def bad(d: borrowed Doc) -> String:
    both(d)
"
        ),
        vec![534]
    );
}

#[test]
fn the_message_names_the_type_the_interface_and_the_bound_as_written() {
    // `diagnostics`' §2 one level up: the call is being held to a promise
    // written somewhere else, and a message that does not show where is asking
    // the reader to go and find it.
    let source = format!(
        "{FIXTURE}\n\
def bad(n: I64) -> String:
    describe(n)
"
    );
    let checked = check(&source);
    assert_eq!(checked.messages(), vec!["`I64` does not implement `Summarize`"]);

    let mut map = SourceMap::new();
    map.add_file("bounds.science".to_string(), source.clone());
    let rendered = render(&map, &checked.diagnostics.iter().next().cloned().unwrap());
    assert!(rendered.contains("`T` is `I64` here"), "{rendered}");
    assert!(rendered.contains("`T` was declared `Summarize`"), "{rendered}");
    // The note names the block the author would have to write.
    assert!(rendered.contains("`I64 implements Summarize:`"), "{rendered}");
    // The secondary label points at the declaration, not at the call.
    assert!(rendered.contains("def describe of T: Summarize"), "{rendered}");
}

// --- the bound is satisfied ----------------------------------------------

#[test]
fn a_type_that_implements_the_interface_checks_clean() {
    program(
        "\
def good(d: borrowed Doc) -> String:
    describe(d)
",
    )
    .assert_clean();
}

#[test]
fn an_owned_argument_reaches_a_borrowed_bounded_parameter() {
    // §6.3's auto-borrow, composed with the bound: `T` is solved from the
    // referent whichever side wrote the borrow, so `describe(d)` with an owned
    // `Doc` is `T := Doc` and not `T := Doc` refused for being unborrowed.
    program(
        "\
def good(d: Doc) -> String:
    describe(d)
",
    )
    .assert_clean();
}

#[test]
fn an_interface_object_satisfies_a_bound_at_its_own_interface() {
    // `any Summarize` heads at `Summarize`, and a thing that *is* the
    // interface implements it.
    program(
        "\
def good(value: borrowed any Summarize) -> String:
    describe(value)
",
    )
    .assert_clean();
}

#[test]
fn the_callers_own_bounded_parameter_is_passed_on_without_a_report() {
    // `methods`'s §4 does not look through a bound on a type parameter, and
    // this is the case where that would have mattered: a parameter has no
    // head, so `implements` admits it rather than refusing on a question it
    // cannot ask.
    program(
        "\
def outer of U: Summarize(value: borrowed U) -> String:
    describe(value)
",
    )
    .assert_clean();
}

#[test]
fn a_bound_on_a_parameter_this_call_did_not_solve_is_not_reported() {
    // §6 leaves an unsolved parameter `Ty::ERROR`, which agrees with whatever
    // it meets; reporting against it would blame a call for a hole the checker
    // left. `largest(items)` in `examples/07_generics.science` is this case.
    program(
        "\
def deep of T: Summarize(items: borrowed Array of T) -> I64:
    1

def a(xs: borrowed Array of I64) -> I64:
    deep(xs)
",
    )
    .assert_clean();
}

// --- the unanswerable half, and its cost ---------------------------------

#[test]
fn a_bound_at_a_prelude_interface_is_not_answerable_and_stays_silent() {
    // `builtins.rs` declares `Clone` and no implementation of it, so a `false`
    // from the index is silence and not a no. This is the same restraint
    // `SC0532` is under, one level out.
    program(
        "\
def duplicate of T: Clone(value: borrowed T) -> I64:
    1

def a(n: I64) -> I64:
    duplicate(n)
",
    )
    .assert_clean();
}

#[test]
fn the_cost_of_that_restraint_is_stated_as_a_test() {
    // **A bound at a prelude interface is unchecked even when it is plainly
    // unsatisfied.** `Plain` implements nothing at all and `duplicate(p)` is
    // admitted, because the compiler cannot tell that from `I64`, which
    // implements `Clone` in every program anyone would write and in no
    // declaration this compiler can see.
    //
    // This test fails the day the prelude declares its own implementations,
    // and that is what it is for: the entry goes with the fix.
    program(
        "\
def duplicate of T: Clone(value: borrowed T) -> I64:
    1

def a(p: borrowed Plain) -> I64:
    duplicate(p)
",
    )
    .assert_clean();
}

#[test]
fn a_closure_bound_names_no_interface_and_is_not_checked() {
    // `hir::Bound::interface_res` is `None` for `where F: (A) -> B`, so it is
    // not in `Signature::bounds` at all. Whether a given `F` has that shape is
    // a structural question and nothing asks it yet; the test records that
    // rather than leaving it to be discovered.
    program(
        "\
def apply of F(f: F) -> I64 where F: (I64) -> I64:
    1

def a(n: I64) -> I64:
    apply(n)
",
    )
    .assert_clean();
}

#[test]
fn a_bound_whose_interface_did_not_resolve_says_nothing() {
    // The resolver already reported it; `ty`'s §5 discipline, applied to a
    // bound rather than to a type.
    let checked = support::check_allowing_resolution_errors(
        "\
def describe of T: Bogus(value: borrowed T) -> I64:
    1

def a(n: I64) -> I64:
    describe(n)
",
    );
    assert!(checked.resolution.has_errors());
    assert_eq!(checked.codes(), Vec::<u16>::new());
}
