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
//! refusals are easy to get right and easy to test. The admissions are not: a
//! checker that treated `Methods::implements`'s `false` as evidence would
//! report `T: Ord` at an `I64` and every numeric program in the language with
//! it. `Methods::answers_for` is the line and `methods`'s §7 is the argument.
//!
//! **That line moved when `builtins.rs` grew implementations.** It used to be
//! *"is the interface builtin"*, which made every bound at `Ord`, `Clone`,
//! `Eq` and `Add` unanswerable. It is now *"is the interface builtin **and**
//! is the type something whose implementations the prelude enumerates"*, and
//! the prelude enumerates them for the unapplied types — `I64`, `Bool`,
//! `Char`, `String`, `IoError`. So `T: Ord` at a `Bool` is `SC0534` and `T:
//! Ord` at an `I64` is clean, and the tests below say both.
//!
//! **What stays unanswerable, and why it is not an oversight**: a *user* type
//! at a prelude interface, because nothing in the language derives `Clone` for
//! a record and the absence of a declaration means nothing; and an *applied*
//! prelude type — `Array of T` — because `Array of T: Clone` holds exactly
//! when `T: Clone` and `methods`' §4 does not look through a conditional
//! implementation.
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

# Decision 27 (`type-checking-and-mir.md` §7): `self.title` through an
# implicit-borrow `self` is `&String` now, not `String`, so the getter needs
# the explicit copy `String`'s missing `clone` already forces elsewhere —
# `examples/09_absence_and_failure.science`'s `copy_of`.
def copy_of(text: &String) -> String:
    let mutable out be String.new()
    out.push_str(text)
    out

Doc implements Summarize:
    def preview(self) -> String:
        copy_of(self.title)

type Plain:
    n: I64

def describe[T: Summarize](value: &T) -> String:
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
def bad(p: &Plain) -> String:
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
def show[T](value: &T) -> String where T: Summarize:
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
def bad(n: &I64) -> String:
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
    def show[T: Summarize](self, value: &T) -> String:
        value.preview()

def bad(p: &Plain, n: I64) -> String:
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

def both[T: Summarize + Render](value: &T) -> String:
    value.preview()

def bad(d: &Doc) -> String:
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
    assert!(rendered.contains("def describe[T: Summarize]"), "{rendered}");
}

// --- the bound is satisfied ----------------------------------------------

#[test]
fn a_type_that_implements_the_interface_checks_clean() {
    program(
        "\
def good(d: &Doc) -> String:
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
def good(value: &any Summarize) -> String:
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
def outer[U: Summarize](value: &U) -> String:
    describe(value)
",
    )
    .assert_clean();
}

#[test]
fn a_bound_on_a_parameter_solved_one_layer_past_the_root_is_reported() {
    // Retires `a_bound_on_a_parameter_this_call_did_not_solve_is_not_reported`,
    // which pinned the opposite of this: `items: &Array[T]` used to be
    // deeper than `BodyChecker::root_param` could reach, so `T` stayed
    // unsolved and the bound went unchecked from a hole in the checker
    // rather than from anything true about the program. `BodyChecker::
    // structural_solve` now walks one layer past the root the same way
    // `instantiate_return`'s own compound match already does for a return
    // type, `T` solves to `I64` from `xs`'s declared type, and `I64` does
    // not implement `Summarize` — the same fact `a_type_that_does_not_
    // implement_the_interface_is_reported` already pins at the root.
    assert_eq!(
        codes(
            "\
def deep[T: Summarize](items: &Array[T]) -> I64:
    1

def a(xs: &Array[I64]) -> I64:
    deep(xs)
"
        ),
        vec![534]
    );
}

// --- the unanswerable half, and its cost ---------------------------------

#[test]
fn a_bound_at_a_prelude_interface_is_answered_for_a_prelude_type() {
    // `builtins.rs` declares `I64 implements Clone:`, so the bound is
    // satisfied and the call is clean — for a *reason* now, rather than
    // because nothing could be said.
    program(
        "\
def duplicate[T: Clone](value: &T) -> I64:
    1

def a(n: I64) -> I64:
    duplicate(n)
",
    )
    .assert_clean();
}

#[test]
fn a_prelude_type_that_does_not_implement_a_prelude_interface_is_sc0534() {
    // The half that was retired. `Bool` implements `Eq`, `Copy`, `Clone` and
    // `Display` and **not** `Ord` — ordering two booleans is not an operation
    // the prelude offers — and until the prelude declared anything this was
    // silence. `methods`' §7.
    let checked = check(&format!(
        "{FIXTURE}
def biggest[T: Ord](a: &T, b: &T) -> I64:
    1

def a() -> I64:
    biggest(true, false)
"
    ));
    assert_eq!(checked.codes(), vec![534]);
}

#[test]
fn an_applied_prelude_type_is_still_not_answerable() {
    // `Array of Int: Ord` holds when `Int: Ord` does, and a conditional
    // implementation is not something the index can hold — `methods`' §4 says
    // a blanket implementation *"is not looked through"*. Declaring
    // `Array implements Ord:` unconditionally would admit `Array of Doc` for a
    // `Doc` that is not orderable; declaring nothing and reporting would be a
    // false positive here. So the answer is *"cannot say"*, and it is silent.
    program(
        "\
def biggest[T: Ord](a: &T, b: &T) -> I64:
    1

def a(xs: &Array[I64]) -> I64:
    biggest(xs, xs)
",
    )
    .assert_clean();
}

#[test]
fn the_cost_of_that_restraint_is_stated_as_a_test() {
    // **A bound at a prelude interface is unchecked on a *user* type even when
    // it is plainly unsatisfied.** `Plain` implements nothing at all and
    // `duplicate(p)` is admitted, because *"the program contains no
    // `Plain implements Clone:`"* is not evidence: no note has said whether a
    // record is `Clone` by construction, by derivation or by declaration, and
    // reporting here would answer that question by accident.
    //
    // This is what is left of the restraint after the prelude's own
    // implementations landed. It closes when the language says where a user
    // type's `Clone` comes from.
    program(
        "\
def duplicate[T: Clone](value: &T) -> I64:
    1

def a(p: &Plain) -> I64:
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
def apply[F](f: F) -> I64 where F: (I64) -> I64:
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
def describe[T: Bogus](value: &T) -> I64:
    1

def a(n: I64) -> I64:
    describe(n)
",
    );
    assert!(checked.resolution.has_errors());
    assert_eq!(checked.codes(), Vec::<u16>::new());
}
