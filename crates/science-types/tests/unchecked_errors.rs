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

Doc has:
    # Exclusion 2 at a method: a parameter that is itself an `E?`, so the
    # obligation moves on.
    def carry(self, err: Error?) -> Bool:
        true

    # And a method that takes *anything*, which is not the same thing. The
    # parameter is a type parameter, so the argument type-checks and the
    # obligation stays where it was.
    def show[T](self, value: T) -> Bool:
        true
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
fn an_error_given_to_a_method_that_takes_one_counts() {
    // Exclusion 2 at a method. This was the second *refusal to guess* — with
    // no Decision 11 lookup there was no parameter type to compare against, so
    // an argument to any method at all excused the binding. There is a
    // parameter type now, and `carry` takes an `Error?`, so this is the
    // exclusion proper rather than a shrug.
    let checked = program(
        "
def given() -> Doc:
    let doc, err be find(\"a\")
    let _shown be doc.carry(err)
    doc
",
    );
    checked.assert_clean();
}

#[test]
fn an_error_given_to_a_method_that_takes_anything_is_still_unchecked() {
    // The other half, which the old refusal could not see: `show` takes a `T`,
    // the argument type-checks, and nothing about the call is the obligation
    // moving on. §5's second exclusion is about a function *"taking `E?`"* for
    // exactly this reason, and the `Call` arm has always read it that way.
    let checked = program(
        "
def given() -> Doc:
    let doc, err be find(\"a\")
    let _shown be doc.show(err)
    doc
",
    );
    assert_eq!(checked.codes(), vec![140]);
}

#[test]
fn an_error_given_to_a_method_this_crate_cannot_resolve_still_counts() {
    // What survives of the refusal, and its domain: the receiver is a
    // `String` and the call resolves to no candidate, so there is no parameter
    // list to read. `method_takes_error` errs the way §5 says this diagnostic
    // has to err.
    //
    // **The name is `split` and it used to be `append`.** `methods`' §8a made
    // *"no candidate"* on a prelude receiver two different facts: `append` is a
    // name no note gives and is now `SC0532`, while `split` is one of the six
    // `stdlib-core.md` §6.9 methods `builtins.rs`' `String` block leaves out and
    // is still unresolved-and-silent. This test wants the second, because its
    // subject is what `SC0140` does with a call it cannot see the parameters of
    // — not what `SC0532` does with a misspelling.
    let checked = program(
        "
def given() -> Doc:
    let doc, err be find(\"a\")
    let _shown be doc.title.split(err)
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

#[test]
fn a_concrete_error_type_is_a_candidate_and_not_only_any_error() {
    // §5 of `unchecked`, and the widening Decision 11's index paid for: the
    // condition is *"`E?` where `E` implements `Error`"*, and until there was
    // a lookup this pass could only recognise `E?` where `E` was literally
    // `any Error`. `ConfigError` is the shape `examples/09_absence_and_failure
    // .science` returns from half its functions.
    let checked = check(
        "type Doc:
    title: String

type ConfigError:
    detail: String

def copy_of(text: &String) -> String:
    let mutable out be String.new()
    out.push_str(text)
    out

ConfigError implements Error:
    # `message` and not `describe`: `Error` is the one-method interface of
    # revision 2 §3.4, and `conform`'s `SC0539` found this fixture calling it by
    # a name the interface does not declare. The fixture's subject is `SC0140`
    # and it reads the same with the method named correctly.
    def message(self) -> String:
        copy_of(self.detail)

def load(key: String) -> (Doc, ConfigError?):
    (Doc(title: key), null)

def ignores() -> Doc:
    let doc, err be load(\"a\")
    doc
",
    );
    assert_eq!(checked.codes(), vec![140]);
    assert_eq!(checked.messages(), vec!["`err` is never checked for an error".to_string()]);
}

// --- what is not a candidate ---------------------------------------------

#[test]
fn a_nullable_that_is_not_an_error_is_not_a_candidate() {
    // §5 of `unchecked`: the condition is `E?` where `E` implements `Error`,
    // and `Doc` implements nothing. There is no *position* to read either —
    // §5b needs a pair from a call, and this is one binding of a `null` — so
    // an ordinary optional owes nothing.
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

// --- §5b: the error position of a fallible call ---------------------------
//
// The condition §5 states is *"`E?` where `E` implements `Error`"*, and read
// literally it leaves the diagnostic silent on the program a beginner writes:
// a `choice` used as the error of a `-> (T, E?)` with no `implements Error:`
// block anywhere. `syntax-revision-2.md` §3.4 recommends exactly that shape
// for a concrete error, and `examples/00_kitchen_sink.science` writes it. So
// the *position* is a second way in, and these are its tests — the wrong
// program refused, the right one still passing, and the cost named.

/// The fixture the hole was measured with, minus the body.
const CONCRETE: &str = "\
choice ParseError:
    Bad(I64)

def parse(s: &String) -> (I64, ParseError?):
    (0, null)
";

#[test]
fn an_error_type_that_implements_nothing_is_still_a_candidate() {
    let checked = check(&format!(
        "{CONCRETE}
def ignores() -> Bool:
    let value, err be parse(\"x\")
    true
"
    ));
    assert_eq!(checked.codes(), vec![140]);
    assert_eq!(checked.messages(), vec!["`err` is never checked for an error".to_string()]);
}

#[test]
fn the_same_program_with_the_test_written_still_passes() {
    // The other half of the pair. A check that refuses everything is not
    // progress, and the exclusions are reached through the *same* function —
    // widening the candidate set changes which bindings are asked, never what
    // excuses one.
    check(&format!(
        "{CONCRETE}
def checks() -> Bool:
    let value, err be parse(\"x\")
    if err?:
        return false
    true
"
    ))
    .assert_clean();
}

#[test]
fn a_positional_candidate_is_excused_by_being_returned() {
    // Exclusion 1, unchanged, over a binding that is a candidate only by
    // position.
    check(&format!(
        "{CONCRETE}
def passes_on() -> (I64, ParseError?):
    let value, err be parse(\"x\")
    (value, err)
"
    ))
    .assert_clean();
}

#[test]
fn a_positional_candidate_is_silenced_by_the_underscore() {
    // Decision 10, unchanged, for the same reason.
    check(&format!(
        "{CONCRETE}
def ignores() -> Bool:
    let value, _err be parse(\"x\")
    true
"
    ))
    .assert_clean();
}

#[test]
fn only_the_last_binding_of_the_pair_is_read_positionally() {
    // §3.1 puts the error after the value. A nullable in the *value* position
    // is an optional result and owes nothing, and one diagnostic comes out,
    // not two.
    let checked = check(
        "\
choice ParseError:
    Bad(I64)

type Doc:
    title: String

def parse(s: &String) -> (Doc?, ParseError?):
    (null, null)

def ignores() -> Bool:
    let doc, err be parse(\"x\")
    true
",
    );
    assert_eq!(checked.codes(), vec![140]);
}

#[test]
fn a_pair_that_did_not_come_from_a_call_is_not_read_positionally() {
    // *Fallible* means a function returned this pair. A tuple the author
    // built and then destructured has no failure in it, and §3's model never
    // claimed it did.
    check(
        "\
type Doc:
    title: String

def ignores(pair: (I64, Doc?)) -> Bool:
    let value, maybe be pair
    true
",
    )
    .assert_clean();
}

#[test]
fn the_cost_a_non_error_nullable_in_the_error_position_is_reported() {
    // **This is the stated cost of §5b and it is deliberate.** `F64?` is not
    // an error type by anybody's reading, and it is in the position §3.1
    // reserves for one. The corpus contains no such signature — every
    // multi-value return in `examples/` is `(T, E?)` — and the escape is
    // Decision 10's one character. Pinning it here means a future change to
    // the rule has to come past this test rather than past a silence.
    let checked = check(
        "\
def midpoint(lo: F64, hi: F64) -> (F64, F64?):
    (lo, null)

def ignores() -> Bool:
    let mid, rest be midpoint(0.0, 1.0)
    true
",
    );
    assert_eq!(checked.codes(), vec![140]);
}
