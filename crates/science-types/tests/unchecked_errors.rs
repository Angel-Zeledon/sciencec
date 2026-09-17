//! `SC0140` — Decision 9's diagnostic, and Decision 10's escape.
//!
//! §5 names four things the check *must not* report and says why the list
//! matters: *"each of these is a false positive that would make the diagnostic
//! hated"*. §15 then says the list *"is a guess … they came from reading the
//! corpus, not from running the analysis over it"*.
//!
//! **So each of the four has a test named after it.** A test called
//! `a_returned_error_is_not_unchecked` is worth more than a hundred assertions
//! about the happy path, because the happy path is what the implementation was
//! written against and the exclusions are what it was not.

mod support;

use support::check;

/// One fallible function in the shape revision 2 §3.1 gives every one of them,
/// and one that cannot fail, so a test can say *"not every binding"*.
const PRELUDE: &str = "\
type Doc:
    title: String

def find(key: String) -> (Doc, Error?):
    (Doc(title: key), null)

def blank() -> Doc:
    Doc(title: \"\")
";

fn program(body: &str) -> support::Checked {
    check(&format!("{PRELUDE}{body}"))
}

// --- the diagnostic itself -----------------------------------------------

#[test]
fn an_error_that_is_never_tested_is_reported() {
    let checked = program(
        "
def ignores() -> Doc:
    let doc, err be find(\"a\")
    doc
",
    );
    assert_eq!(checked.codes(), vec![140]);
    assert_eq!(checked.messages(), vec!["`err` is never checked for an error".to_string()]);
}

#[test]
fn an_error_tested_with_the_presence_operator_is_not_reported() {
    let checked = program(
        "
def checks() -> Doc:
    let doc, err be find(\"a\")
    if err?:
        return blank()
    doc
",
    );
    checked.assert_clean();
}

#[test]
fn the_test_may_be_anywhere_and_under_a_not() {
    // §5: *"mark every `?` test that reads it"* — not every path, one reader.
    let checked = program(
        "
def checks() -> Doc:
    let doc, err be find(\"a\")
    if not err?:
        return doc
    blank()
",
    );
    checked.assert_clean();
}

#[test]
fn only_the_error_half_of_the_pair_is_a_candidate() {
    // `doc` is an ordinary binding and owes nothing; one diagnostic, not two.
    let checked = program(
        "
def ignores() -> Bool:
    let doc, err be find(\"a\")
    true
",
    );
    assert_eq!(checked.codes(), vec![140]);
}

// --- §5's four exclusions, one test each ---------------------------------

#[test]
fn a_returned_error_is_not_unchecked() {
    // Exclusion 1. *"`return (value, err)` passes the obligation to the
    // caller; that is the model working."*
    let checked = program(
        "
def passes_on() -> (Doc, Error?):
    let doc, err be find(\"a\")
    (doc, err)
",
    );
    checked.assert_clean();
}

#[test]
fn a_returned_error_is_not_unchecked_through_an_explicit_return() {
    let checked = program(
        "
def passes_on() -> (Doc, Error?):
    let doc, err be find(\"a\")
    return (doc, err)
",
    );
    checked.assert_clean();
}

#[test]
fn an_error_passed_to_a_function_taking_one_is_not_unchecked() {
    // Exclusion 2, and the parameter's type is what makes it one.
    let checked = program(
        "
def handle(err: Error?) -> Bool:
    true

def hands_off() -> Doc:
    let doc, err be find(\"a\")
    let _handled be handle(err)
    doc
",
    );
    checked.assert_clean();
}

#[test]
fn an_error_passed_to_a_function_that_does_not_take_one_is_still_unchecked() {
    // The other side of exclusion 2, and the reason it is written as *"a
    // function taking `E?`"* rather than *"a function"*: printing an error is
    // not dealing with it.
    let checked = program(
        "
def announce(what: Bool) -> Bool:
    what

def prints() -> Doc:
    let doc, err be find(\"a\")
    let _said be announce(err?)
    doc
",
    );
    // `err?` *is* a test, so this one is clean — which is the point: the only
    // way to get an error into that call is to have tested it.
    checked.assert_clean();

    let untested = program(
        "
def announce(what: Doc) -> Bool:
    true

def prints() -> Doc:
    let doc, err be find(\"a\")
    let _said be announce(doc)
    doc
",
    );
    assert_eq!(untested.codes(), vec![140]);
}

#[test]
fn a_statically_null_error_is_not_unchecked() {
    // Exclusion 3. §3.1's own example: *"`let missing: Error? be null` … has
    // nothing to check"*.
    let checked = program(
        "
def nothing() -> Bool:
    let missing: Error? be null
    true
",
    );
    checked.assert_clean();
}

#[test]
fn a_binding_in_a_function_that_cannot_return_is_not_unchecked() {
    // Exclusion 4, the declared half: `-> Never`.
    let checked = program(
        "
def give_up() -> Never:
    let doc, err be find(\"a\")
    panic(\"stop\")
",
    );
    checked.assert_clean();
}

#[test]
fn a_binding_in_a_function_that_panics_on_every_path_is_not_unchecked() {
    // Exclusion 4, the half that is a property of the body rather than of the
    // signature. Nothing downstream of this binding will ever see the error,
    // because there is no downstream.
    let checked = program(
        "
def always_panics() -> Doc:
    let doc, err be find(\"a\")
    panic(\"stop\")
",
    );
    checked.assert_clean();
}

#[test]
fn a_function_that_panics_on_one_path_only_is_still_checked() {
    // The exclusion is *"cannot return"*, not *"might not"*. A `panic` behind
    // an `if` leaves a path to the end of the body, and the obligation with it.
    let checked = program(
        "
def sometimes(ok: Bool) -> Doc:
    let doc, err be find(\"a\")
    if ok:
        panic(\"stop\")
    doc
",
    );
    assert_eq!(checked.codes(), vec![140]);
}

// --- Decision 10 ----------------------------------------------------------

#[test]
fn a_deliberately_ignored_error_is_not_unchecked() {
    // *"Deliberately ignoring an error is spelled by binding it to a name
    // beginning with an underscore."* One character, no grammar, greppable.
    let checked = program(
        "
def on_purpose() -> Doc:
    let doc, _err be find(\"a\")
    doc
",
    );
    checked.assert_clean();
}

#[test]
fn the_underscore_has_to_be_the_first_character() {
    // Decision 10 says *"a name beginning with an underscore"*, and a name that
    // merely contains one is an ordinary name. Testing the weaker rule — *"a
    // name containing an underscore"* — would silence `parse_err`, which is
    // what half the corpus calls the binding it forgot to check.
    let checked = program(
        "
def not_on_purpose() -> Doc:
    let doc, parse_err be find(\"a\")
    doc
",
    );
    assert_eq!(checked.codes(), vec![140]);
}

// --- the readers §5 does not name -----------------------------------------

#[test]
fn a_match_on_the_error_counts_as_having_examined_it() {
    // `unchecked`'s §2: a refusal to guess. Exhaustiveness is not built, so
    // this pass cannot confirm the arms are right — and reporting a binding the
    // author plainly examined would be the fifth entry in §5's hated list.
    let checked = program(
        "
def examined() -> Doc:
    let doc, err be find(\"a\")
    match err:
        _: doc
",
    );
    checked.assert_clean();
}

#[test]
fn an_error_given_to_a_method_counts_because_nothing_can_say_otherwise() {
    // The second refusal to guess: Decision 11's lookup does not exist, so
    // there is no parameter type to compare against.
    let checked = program(
        "
def given() -> Doc:
    let doc, err be find(\"a\")
    let _shown be doc.describe(err)
    doc
",
    );
    checked.assert_clean();
}

#[test]
fn a_test_inside_a_nested_block_still_counts() {
    // The reader search is over the whole body, not over the statement list of
    // the block the `let` is in.
    let checked = program(
        "
def nested(ok: Bool) -> Doc:
    let doc, err be find(\"a\")
    if ok:
        if err?:
            return blank()
    doc
",
    );
    checked.assert_clean();
}

#[test]
fn a_let_inside_a_branch_is_a_candidate_too() {
    let checked = program(
        "
def branching(ok: Bool) -> Doc:
    if ok:
        let doc, err be find(\"a\")
        return doc
    blank()
",
    );
    assert_eq!(checked.codes(), vec![140]);
}

// --- what is not a candidate ---------------------------------------------

#[test]
fn a_nullable_that_is_not_an_error_is_not_a_candidate() {
    // §5 of `unchecked`: the condition is `E?` where `E` implements `Error`,
    // and this pass answers it only for `any Error`. A `Doc?` owes nothing.
    let checked = program(
        "
def optional() -> Bool:
    let maybe: Doc? be null
    true
",
    );
    checked.assert_clean();
}

#[test]
fn a_parameter_of_error_type_is_not_a_candidate() {
    // §5 says *"a binding"* and its examples are all `let`s. A parameter's
    // obligation was discharged by whoever called, and reporting here would
    // report it twice.
    let checked = program(
        "
def receives(err: Error?) -> Bool:
    true
",
    );
    checked.assert_clean();
}
