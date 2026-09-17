//! Decision 11 — method lookup, and the two diagnostics it owes.
//!
//! > **Decision 11. Method lookup is: inherent methods on the type, then
//! > interface methods from interfaces the type implements that are in scope.
//! > Ambiguity is an error, never a priority ordering.**
//!
//! The claims here are of two kinds and the second is the one worth the file.
//! The first is that a call *resolves* — that `MethodCall.method` names a
//! definition and the arguments are checked against that definition's
//! parameters, which is the hole `check`'s §6 used to price. The second is
//! about the **messages**: `methods`'s §3 says the whole value of the ambiguity
//! rule is that the error names both candidates and where each came from, so a
//! test that only asserted the *code* would be asserting the half that is
//! cheap. Both message tests below read the text.

mod support;

use science_resolve::hir::DefKind;
use science_types::thir::ExprKind;
use support::check;

/// A type with an inherent block and two interfaces, which is every arm of the
/// lookup in one program.
const FIXTURE: &str = "\
interface Summarize:
    def summarize(self) -> String

    def headline(self) -> String:
        self.summarize()

interface Reset:
    def reset(mutable self)

type Doc:
    title: String

Doc has:
    def describe(self) -> String:
        self.title

    def retitle(mutable self, title: String):
        self.title be title

    def blank() -> Doc:
        Doc(title: \"\")

Doc implements Summarize:
    def summarize(self) -> String:
        self.title

Doc implements Reset:
    def reset(mutable self):
        self.title be \"\"
";

fn program(body: &str) -> support::Checked {
    check(&format!("{FIXTURE}{body}"))
}

// --- the lookup ------------------------------------------------------------

#[test]
fn an_inherent_method_resolves_and_the_call_has_its_return_type() {
    let checked = program(
        "
def read(doc: Doc) -> String:
    doc.describe()
",
    );
    checked.assert_clean();
    let describe = checked.def("describe", DefKind::Fn);
    let body = checked.body("read");
    let (id, call) = body
        .exprs()
        .find(|(_, expr)| matches!(expr.kind, ExprKind::MethodCall { .. }))
        .expect("the body has a method call");
    let ExprKind::MethodCall { method, .. } = call.kind else { unreachable!() };
    assert_eq!(method, Some(describe));
    assert_eq!(checked.render(body.ty(id)), "String");
}

#[test]
fn an_interface_method_resolves_through_the_implementation_that_writes_it() {
    let checked = program(
        "
def read(doc: Doc) -> String:
    doc.summarize()
",
    );
    checked.assert_clean();
    let body = checked.body("read");
    let (_, call) = body
        .exprs()
        .find(|(_, expr)| matches!(expr.kind, ExprKind::MethodCall { .. }))
        .expect("the body has a method call");
    let ExprKind::MethodCall { method, .. } = call.kind else { unreachable!() };
    // The *implementation's* `summarize`, not the interface's declaration:
    // `Doc implements Summarize:` writes one, and that is the one that runs.
    let summarize = method.expect("the call resolved");
    let owner = checked.krate.defs.get(summarize).parent.expect("a method has a block");
    assert_eq!(checked.krate.defs.get(owner).kind, DefKind::Impl);
}

#[test]
fn a_default_body_is_reachable_on_an_implementor_that_never_wrote_it() {
    // `methods`'s §2, second half. `Doc implements Summarize:` says nothing
    // about `headline`, and an index holding only what the block spells would
    // resolve `summarize` and not `headline` for no reason the author can see.
    let checked = program(
        "
def read(doc: Doc) -> String:
    doc.headline()
",
    );
    checked.assert_clean();
    let headline = checked.def("headline", DefKind::Fn);
    let body = checked.body("read");
    let (_, call) = body
        .exprs()
        .find(|(_, expr)| matches!(expr.kind, ExprKind::MethodCall { .. }))
        .expect("the body has a method call");
    let ExprKind::MethodCall { method, .. } = call.kind else { unreachable!() };
    assert_eq!(method, Some(headline));
}

#[test]
fn a_method_is_found_through_a_borrow() {
    // The same rule a field read already had: F0 has no explicit dereference
    // to write instead, so a borrow is transparent to the lookup.
    let checked = program(
        "
def read(doc: borrowed Doc) -> String:
    doc.describe()
",
    );
    checked.assert_clean();
}

#[test]
fn self_at_a_call_is_the_receivers_type_and_not_the_blocks() {
    // `-> Doc` here is written `Doc`, but the associated function's own
    // `Doc(title: "")` is checked against `Self` inside the block. What this
    // asserts is the call site: the type of `Doc.blank()` is `Doc`, not
    // `Self`.
    let checked = program(
        "
def fresh() -> Doc:
    Doc.blank()
",
    );
    checked.assert_clean();
    let body = checked.body("fresh");
    let (id, _) = body
        .exprs()
        .find(|(_, expr)| matches!(expr.kind, ExprKind::Call { .. }))
        .expect("an associated function is a call");
    assert_eq!(checked.render(body.ty(id)), "Doc");
}

#[test]
fn an_associated_function_lowers_to_a_call_and_not_to_a_method_call() {
    // `check`'s `associated_call`: the receiver names a type, so there is no
    // receiver value, and a `MethodCall` whose receiver node stood for a type
    // would put an expression in the tree for something the author never
    // evaluated.
    let checked = program(
        "
def fresh() -> Doc:
    Doc.blank()
",
    );
    checked.assert_clean();
    let blank = checked.def("blank", DefKind::Fn);
    let body = checked.body("fresh");
    assert!(
        body.exprs().all(|(_, expr)| !matches!(expr.kind, ExprKind::MethodCall { .. })),
        "an associated function is not a method call"
    );
    let (_, call) = body
        .exprs()
        .find(|(_, expr)| matches!(expr.kind, ExprKind::Call { .. }))
        .expect("it is a call");
    let ExprKind::Call { callee, .. } = call.kind else { unreachable!() };
    assert!(matches!(body.expr(callee).kind, ExprKind::Item(def) if def == blank));
}

// --- the arguments, which is what the hole cost --------------------------

#[test]
fn a_methods_arguments_are_checked_against_its_parameters() {
    let checked = program(
        "
def rename(doc: mutable borrowed Doc) -> Bool:
    doc.retitle(1)
    true
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(
        checked.messages(),
        vec!["expected `String`, found `an integer literal`".to_string()]
    );
}

#[test]
fn an_argument_takes_the_parameters_type_rather_than_being_synthesised() {
    // `21_compiler_shapes.science`'s own diagnostic, in six lines. The `null`
    // has no type of its own; `parent: Doc?` is where it gets one, and until
    // the call resolved there was nothing to get it from — which is `SC0526`,
    // *"the type of this value cannot be inferred"*.
    let checked = program(
        "
Doc has:
    def child(self, parent: Doc?) -> Bool:
        true

def walk(doc: Doc) -> Bool:
    doc.child(null)
",
    );
    checked.assert_clean();
}

#[test]
fn a_method_call_with_the_wrong_number_of_arguments_is_reported() {
    let checked = program(
        "
def rename(doc: mutable borrowed Doc) -> Bool:
    doc.retitle()
    true
",
    );
    assert_eq!(checked.codes(), vec![527]);
}

#[test]
fn a_generic_block_solves_its_parameter_from_the_receiver() {
    // `check`'s `block_substitution`: `Wrapper of T has:` writes `-> T`, the
    // receiver is a `Wrapper of I64`, and the call has type `I64`.
    let checked = check(
        "\
type Wrapper of T:
    value: T

Wrapper of T has:
    def get(self) -> T:
        self.value

def unwrap(w: Wrapper of I64) -> I64:
    w.get()
",
    );
    checked.assert_clean();
}

// --- the two messages ----------------------------------------------------

#[test]
fn an_ambiguous_method_names_both_candidates_and_where_each_came_from() {
    // `methods`'s §3, and the reason the rule is worth having: the message is
    // the whole value of refusing to pick. A diagnostic that said only
    // *"ambiguous"* would have refused to answer and refused to explain.
    let checked = check(
        "\
interface Long:
    def length(self) -> I64

interface Wide:
    def length(self) -> I64

type Doc:
    title: String

Doc implements Long:
    def length(self) -> I64:
        1

Doc implements Wide:
    def length(self) -> I64:
        2

def measure(doc: Doc) -> I64:
    doc.length()
",
    );
    assert_eq!(checked.codes(), vec![531]);
    assert_eq!(checked.messages(), vec!["`length` on `Doc` could be 2 methods".to_string()]);

    let diagnostic = checked.diagnostics.iter().next().expect("one diagnostic");
    let labels: Vec<&str> =
        diagnostic.labels.iter().map(|label| label.message.as_str()).collect();
    assert_eq!(
        labels,
        vec![
            "2 implementations declare `length`",
            "one is declared by `Doc implements Long:`",
            "one is declared by `Doc implements Wide:`",
        ]
    );
    // Each candidate's label points at that method's own definition, so the
    // reader is shown two places and not told about them.
    assert_eq!(diagnostic.labels.iter().filter(|label| !label.primary).count(), 2);
    assert!(diagnostic.notes[0].contains("never a priority ordering"));
}

#[test]
fn an_inherent_method_and_an_interface_one_are_ambiguous_together() {
    // Decision 11 names inherent methods *then* interface methods, and the
    // second half of the sentence is what decides what "then" means: it is the
    // order of the search and not a priority. A checker that let the inherent
    // one win would resolve this silently.
    let checked = check(
        "\
interface Wide:
    def length(self) -> I64

type Doc:
    title: String

Doc has:
    def length(self) -> I64:
        1

Doc implements Wide:
    def length(self) -> I64:
        2

def measure(doc: Doc) -> I64:
    doc.length()
",
    );
    assert_eq!(checked.codes(), vec![531]);
    let diagnostic = checked.diagnostics.iter().next().expect("one diagnostic");
    let labels: Vec<&str> =
        diagnostic.labels.iter().map(|label| label.message.as_str()).collect();
    assert_eq!(labels[1], "one is declared by `Doc has:`");
    assert_eq!(labels[2], "one is declared by `Doc implements Wide:`");
}

#[test]
fn an_implementation_and_the_default_it_answers_are_one_method_and_not_two() {
    // `methods`'s §2: the subtlety that would otherwise make every
    // implementation of every interface ambiguous with itself. `Doc implements
    // Summarize:` writes `summarize`, the interface declares it, and they are
    // the declaration and the implementation of one thing.
    let checked = program(
        "
def read(doc: Doc) -> String:
    doc.summarize()
",
    );
    checked.assert_clean();
}

// --- `methods`'s §6: one interface, several instantiations ----------------

/// The corpus case, in the shape `examples/00_kitchen_sink.science` writes it:
/// one interface implemented twice at two different type arguments, and a call
/// that names the method by one name.
const INSTANCES: &str = "\
type ParseError:
    detail: String

type IoError:
    detail: String

type LoadError:
    detail: String

LoadError implements From of ParseError:
    def from(value: ParseError) -> Self:
        LoadError(detail: value.detail)

LoadError implements From of IoError:
    def from(value: IoError) -> Self:
        LoadError(detail: value.detail)
";

/// What an associated call resolved to, named by the type of its parameter —
/// which is the only thing that tells two implementations of one interface
/// apart, and therefore the only assertion worth making about which one won.
fn selected(checked: &support::Checked, function: &str) -> String {
    let body = checked.body(function);
    let (_, call) = body
        .exprs()
        .find(|(_, expr)| matches!(expr.kind, ExprKind::Call { .. }))
        .expect("the body has a call");
    let ExprKind::Call { callee, .. } = call.kind else { unreachable!() };
    let ExprKind::Item(def) = body.expr(callee).kind else {
        panic!("the call did not resolve to a method");
    };
    let signature = checked.decls.signature(def).expect("the method has a signature");
    checked.render(signature.params[0].ty)
}

#[test]
fn the_corpus_case_resolves_by_the_argument_type() {
    // `methods`'s §6, and the case Decision 11 calls an ambiguity and the
    // corpus calls a program: `examples/00_kitchen_sink.science` implements
    // `From` twice and then writes `LoadError.from(io_err)`. The two
    // candidates are one method at two instantiations, so the argument picks,
    // and the call is checked like any other.
    let checked = check(&format!(
        "{INSTANCES}
def widen(err: IoError) -> LoadError:
    LoadError.from(err)
"
    ));
    checked.assert_clean();
    assert_eq!(selected(&checked, "widen"), "IoError");
    let body = checked.body("widen");
    let (id, _) = body
        .exprs()
        .find(|(_, expr)| matches!(expr.kind, ExprKind::Call { .. }))
        .expect("the body has a call");
    // The whole of what the silence used to cost: the call has the method's
    // return type instead of `Ty::ERROR`.
    assert_eq!(checked.render(body.ty(id)), "LoadError");
}

#[test]
fn the_other_argument_selects_the_other_implementation() {
    let checked = check(&format!(
        "{INSTANCES}
def widen(err: ParseError) -> LoadError:
    LoadError.from(err)
"
    ));
    checked.assert_clean();
    assert_eq!(selected(&checked, "widen"), "ParseError");
}

#[test]
fn selection_works_through_a_value_receiver_too() {
    // Nothing in §6 is about the associated form. The candidate set comes from
    // the receiver either way, and the receiver is not selected on again.
    let checked = check(
        "\
interface Absorb of T:
    def absorb(self, value: T) -> String

type IoError:
    detail: String

type ParseError:
    detail: String

type Log:
    detail: String

Log implements Absorb of IoError:
    def absorb(self, value: IoError) -> String:
        value.detail

Log implements Absorb of ParseError:
    def absorb(self, value: ParseError) -> String:
        value.detail

def record(log: Log, err: ParseError) -> String:
    log.absorb(err)
",
    );
    checked.assert_clean();
    let body = checked.body("record");
    let (_, call) = body
        .exprs()
        .find(|(_, expr)| matches!(expr.kind, ExprKind::MethodCall { .. }))
        .expect("the body has a method call");
    let ExprKind::MethodCall { method, .. } = call.kind else { unreachable!() };
    let method = method.expect("the call resolved");
    let signature = checked.decls.signature(method).expect("a signature");
    assert_eq!(checked.render(signature.params[0].ty), "ParseError");
}

#[test]
fn an_argument_that_fits_more_than_one_implementation_is_reported() {
    // Selection that does not narrow to one is `SC0531` — Decision 11's code,
    // because it is Decision 11's problem — under a message that says what
    // failed here, which is the arguments and not the name. `IoError` fits the
    // concrete implementation directly and the interface-object one through
    // §6.3's auto-borrow and `assign`'s §4 unsizing.
    let checked = check(
        "\
interface Note:
    def note(self) -> String

type IoError:
    detail: String

IoError implements Note:
    def note(self) -> String:
        self.detail

type LoadError:
    detail: String

LoadError implements From of IoError:
    def from(value: IoError) -> Self:
        LoadError(detail: value.detail)

LoadError implements From of (any Note):
    def from(value: borrowed any Note) -> Self:
        LoadError(detail: value.note())

def widen(err: IoError) -> LoadError:
    LoadError.from(err)
",
    );
    assert_eq!(checked.codes(), vec![531]);
    assert_eq!(
        checked.messages(),
        vec!["`from` on `LoadError` could be 2 implementations of `From`".to_string()]
    );
    let diagnostic = checked.diagnostics.iter().next().expect("one diagnostic");
    let labels: Vec<&str> =
        diagnostic.labels.iter().map(|label| label.message.as_str()).collect();
    assert_eq!(
        labels,
        vec![
            "supplied `IoError`, which fits all of them",
            "one accepts `IoError`",
            "one accepts `borrowed any Note`",
        ]
    );
    assert!(diagnostic.notes[0].contains("did not narrow it to one"));
}

#[test]
fn an_argument_that_fits_no_implementation_is_reported() {
    let checked = check(&format!(
        "{INSTANCES}
def widen(text: String) -> LoadError:
    LoadError.from(text)
"
    ));
    assert_eq!(checked.codes(), vec![533]);
    assert_eq!(
        checked.messages(),
        vec!["no implementation of `From` for `LoadError` accepts `String`".to_string()]
    );
    let diagnostic = checked.diagnostics.iter().next().expect("one diagnostic");
    let labels: Vec<&str> =
        diagnostic.labels.iter().map(|label| label.message.as_str()).collect();
    assert_eq!(
        labels,
        vec![
            "`from` was given `String`",
            "this one accepts `ParseError`",
            "this one accepts `IoError`",
        ]
    );
}

#[test]
fn an_argument_of_the_wrong_count_fits_no_implementation_either() {
    // Arity is part of selection rather than a check after it: an
    // implementation that cannot take this many arguments is not the callee
    // whatever the types say, so this is the same diagnostic and not
    // `SC0527` reported against a candidate nobody chose.
    let checked = check(&format!(
        "{INSTANCES}
def widen(io: IoError, parse: ParseError) -> LoadError:
    LoadError.from(io, parse)
"
    ));
    assert_eq!(checked.codes(), vec![533]);
    assert_eq!(
        checked.messages(),
        vec!["no implementation of `From` for `LoadError` accepts `IoError`, `ParseError`"
            .to_string()]
    );
}

#[test]
fn a_literal_argument_cannot_select_and_the_message_says_why() {
    // The case the ruling names: an argument that has no type until something
    // expects one, at the one call where what expects it is what is being
    // decided. It refutes no candidate, so nothing narrows — and the author is
    // told that rather than left with a call nobody checked.
    let checked = check(
        "\
type Load:
    detail: String

Load implements From of I64:
    def from(value: I64) -> Self:
        Load(detail: \"\")

Load implements From of F64:
    def from(value: F64) -> Self:
        Load(detail: \"\")

def widen() -> Load:
    Load.from(1)
",
    );
    assert_eq!(checked.codes(), vec![531]);
    let diagnostic = checked.diagnostics.iter().next().expect("one diagnostic");
    assert_eq!(diagnostic.labels[0].message, "supplied an integer literal, which fits all of them");
    assert!(diagnostic.notes[1].contains("has no type until a signature expects one"));
}

#[test]
fn a_null_argument_cannot_select_either_and_then_has_no_type_at_all() {
    // `null` is the same case and one worse: no candidate won, so nothing ever
    // expected a type of it, and Decision 2 has no default for `null` to fall
    // back on. The second code is `SC0526` and it is the truth about the same
    // line — pinned here so that a future fix to either one is noticed.
    let checked = check(
        "\
type Load:
    detail: String

Load implements From of I64:
    def from(value: I64) -> Self:
        Load(detail: \"\")

Load implements From of F64:
    def from(value: F64) -> Self:
        Load(detail: \"\")

def widen() -> Load:
    Load.from(null)
",
    );
    assert_eq!(checked.codes(), vec![531, 526]);
    let diagnostic = checked.diagnostics.iter().next().expect("one diagnostic");
    assert_eq!(diagnostic.labels[0].message, "supplied `null`, which fits all of them");
}

#[test]
fn two_different_interfaces_stay_decision_11s_ambiguity() {
    // **Not widened.** Both interfaces are generic, both implementations are at
    // different type arguments, and the argument would have told them apart —
    // and it is still `SC0531`'s original message, because these are two
    // methods and not one method twice. §6's condition is the interface and
    // nothing else.
    let checked = check(
        "\
interface Alpha of T:
    def make(value: T) -> Self

interface Beta of T:
    def make(value: T) -> Self

type IoError:
    detail: String

type ParseError:
    detail: String

type LoadError:
    detail: String

LoadError implements Alpha of IoError:
    def make(value: IoError) -> Self:
        LoadError(detail: value.detail)

LoadError implements Beta of ParseError:
    def make(value: ParseError) -> Self:
        LoadError(detail: value.detail)

def widen(err: IoError) -> LoadError:
    LoadError.make(err)
",
    );
    assert_eq!(checked.codes(), vec![531]);
    assert_eq!(
        checked.messages(),
        vec!["`make` on `LoadError` could be 2 methods".to_string()]
    );
}

#[test]
fn an_inherent_method_beside_an_interface_one_is_not_selection() {
    // `methods`'s §6's other half: one of these two is not chosen by any
    // argument, so there is nothing to select on and §3's rule stands.
    let checked = check(&format!(
        "{INSTANCES}
LoadError has:
    def from(value: IoError) -> LoadError:
        LoadError(detail: value.detail)

def widen(err: IoError) -> LoadError:
    LoadError.from(err)
"
    ));
    assert_eq!(checked.codes(), vec![531]);
    assert_eq!(
        checked.messages(),
        vec!["`from` on `LoadError` could be 3 methods".to_string()]
    );
}

#[test]
fn a_method_the_type_does_not_have_is_reported_and_the_message_names_the_type() {
    let checked = program(
        "
def read(doc: Doc) -> String:
    doc.shorten()
",
    );
    assert_eq!(checked.codes(), vec![532]);
    assert_eq!(checked.messages(), vec!["`Doc` has no method `shorten`".to_string()]);

    let diagnostic = checked.diagnostics.iter().next().expect("one diagnostic");
    assert_eq!(diagnostic.labels[0].message, "no such method");
    assert!(diagnostic.notes[0].contains("interfaces it implements"));
}

#[test]
fn a_method_named_through_its_type_is_not_reported_as_missing() {
    // `Found::Mismatched`. `describe` takes a receiver and `Doc.describe()`
    // does not supply one; F0 has no qualified-call syntax, so a diagnostic
    // about it would be inventing a spelling for the fix.
    let checked = program(
        "
def read() -> String:
    Doc.describe()
",
    );
    checked.assert_clean();
}

// --- a prelude type as the receiver of an associated call ------------------
//
// `BodyChecker::type_receiver` used to accept `Record | Choice | Alias |
// Interface | Union` and nothing else, and `builtins.rs` allocates `Array`,
// `Map`, `Box`, `String` and `Chars` as `DefKind::Primitive`. So a call
// *through* one of those names found no receiver type, fell through to the
// value path, synthesised the *type's* path expression as if it were a value,
// and came back `Ty::ERROR` — silently, because `ty`'s §5 makes an error type
// agree with everything downstream.
//
// That is every `String.new()`, `(Array of T).new()`, `Map.new()` and
// `Box.new(x)` in `examples/`, which is most files in the corpus. The tests
// below are the hole named: each one asserts a *type*, because the code the
// hole produced was not a missing diagnostic but a missing type.

/// The hole, at the simplest receiver there is: a prelude type with no
/// parameters and an associated function that takes nothing.
///
/// `String.new()` is `stdlib-core.md` §6.9's first line. The assertion is that
/// it lowers to an [`ExprKind::Call`] at a definition — `check`'s §"a method
/// reached through a type" — and that the call's type is `String` and not
/// `Ty::ERROR`.
#[test]
fn an_associated_function_on_a_prelude_type_resolves_through_its_type() {
    let checked = check(
        "\
def scratch() -> String:
    String.new()
",
    );
    checked.assert_clean();
    let body = checked.body("scratch");
    let (id, call) = body
        .exprs()
        .find(|(_, expr)| matches!(expr.kind, ExprKind::Call { .. }))
        .expect("the body has a call");
    let ExprKind::Call { callee, args } = &call.kind else { unreachable!() };
    assert!(args.is_empty());
    let ExprKind::Item(method) = body.expr(*callee).kind else {
        panic!("the callee is the method's own definition")
    };
    assert!(checked.krate.defs.get(method).is_builtin(), "`String.new` is the prelude's");
    assert_eq!(checked.render(body.ty(id)), "String");
}

/// The same receiver, with the type's own arguments written — which is how
/// `examples/07_generics.science`, `examples/10_loops.science` and
/// `examples/12_operators.science` spell every `Array` and `Map` construction
/// in the corpus, and why §4.3 requires the parentheses.
#[test]
fn an_explicit_instantiation_fixes_the_prelude_blocks_parameters() {
    let checked = check(
        "\
def numbers() -> Array of Int:
    (Array of Int).new()

def settings() -> Map of (String, Int):
    (Map of (String, Int)).new()
",
    );
    checked.assert_clean();
    assert_eq!(checked.render(tail(&checked, "numbers")), "Array of Int");
    assert_eq!(checked.render(tail(&checked, "settings")), "Map of (String, Int)");
}

/// The receiver written bare, with the *argument* fixing the block's parameter.
///
/// `Box.new(doc)` is the one prelude associated function the corpus writes
/// without an instantiation, five times, and `check`'s `receiver_arguments` is
/// the rule that gives it a type. Both halves are asserted: the call's type,
/// and that the argument met a real parameter rather than a bare `T`.
#[test]
fn an_argument_fixes_the_receivers_parameter_at_an_associated_call() {
    let checked = program(
        "
def own(doc: Doc) -> Box of Doc:
    Box.new(doc)
",
    );
    checked.assert_clean();
    assert_eq!(checked.render(tail(&checked, "own")), "Box of Doc");
}

/// And the argument is *checked*, which is what the hole cost: before, every
/// `Box.new(..)` in the corpus accepted anything.
#[test]
fn the_argument_of_an_associated_call_is_checked_against_the_solved_parameter() {
    let checked = program(
        "
def own(doc: Doc) -> Box of String:
    Box.new(doc)
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(
        checked.messages(),
        vec!["expected `Box of String`, found `Box of Doc`".to_string()]
    );
}

/// `SC0536`, and the case it is for: a generic type named bare with nothing at
/// the call to fix its arguments.
///
/// What this replaces is a [`Ty`] with a free parameter in it — `Array of T`
/// for a `T` bound in `builtins.rs` — reported at the author as `expected Array
/// of Int, found Array of T`. The fix the message offers is the instantiation,
/// which the corpus already writes everywhere else.
///
/// [`Ty`]: science_types::Ty
#[test]
fn a_bare_generic_receiver_that_nothing_fixes_is_reported() {
    let checked = check(
        "\
def numbers() -> Array of Int:
    Array.new()
",
    );
    assert_eq!(checked.codes(), vec![536]);
    let diagnostic = checked.diagnostics.iter().next().expect("one diagnostic");
    assert_eq!(diagnostic.message, "the type argument of `Array` cannot be inferred here");
    assert_eq!(diagnostic.labels[0].message, "nothing here fixes `T`");
    assert!(diagnostic.notes[1].contains("(Array of ..).new(..)"));
}

/// The same code, plural, and on a *user's* generic type — because the rule is
/// about an associated call and not about the prelude.
///
/// `Wrapper.holding(7)` cannot be solved: an unsuffixed literal is an inference
/// variable until a parameter expects a type of it, and the parameter here is
/// the unsolved one. This is the narrowing `receiver_arguments` prices.
#[test]
fn an_unsuffixed_literal_cannot_fix_a_receivers_parameter() {
    let checked = check(
        "\
type Wrapper of T:
    inner: T

Wrapper of T has:
    def holding(value: T) -> Wrapper of T:
        Wrapper(inner: value)

def wrapped() -> Wrapper of Int:
    Wrapper.holding(7)
",
    );
    assert_eq!(checked.codes(), vec![536]);
    assert_eq!(checked.messages(), vec![
        "the type argument of `Wrapper` cannot be inferred here".to_string()
    ]);

    // The instantiation is the fix, and it is the spelling the message offers.
    let fixed = check(
        "\
type Wrapper of T:
    inner: T

Wrapper of T has:
    def holding(value: T) -> Wrapper of T:
        Wrapper(inner: value)

def wrapped() -> Wrapper of Int:
    (Wrapper of Int).holding(7)
",
    );
    fixed.assert_clean();
}

/// The suffixed literal, for the control: it is the *probe* that fails above
/// and not the rule, so a literal with a type solves the receiver.
#[test]
fn a_suffixed_literal_does_fix_a_receivers_parameter() {
    let checked = check(
        "\
def held() -> Box of I64:
    Box.new(7i64)
",
    );
    checked.assert_clean();
    assert_eq!(checked.render(tail(&checked, "held")), "Box of I64");
}

/// An argument whose own type references an error solves nothing **and reports
/// nothing**, which is `ty`'s §5 rather than a hole left in the rule.
///
/// This is `examples/04_enums.science`'s `Box.new(Leaf(1))`: `Leaf` is a
/// variant of `Tree of T`, §6's probe cannot type the unsuffixed `1`, and the
/// variant call is `Tree of <error>` before the receiver is ever solved.
/// Telling that author to instantiate `Box` would be advice that does not help.
#[test]
fn an_erroneous_argument_neither_solves_the_receiver_nor_reports_it() {
    let checked = check(
        "\
choice Tree of T:
    Leaf(T)
    Node(Box of (Tree of T), Box of (Tree of T))

def grow():
    let tree be Node(Box.new(Leaf(1)), Box.new(Leaf(2)))
",
    );
    checked.assert_clean();
}

// --- the negative: what a builtin still does not answer --------------------

/// **`Methods::surface_is_closed` is why this reports nothing**, and the test
/// is written as a pair so that the silence is visible as a *decision* rather
/// than as a gap somebody forgot.
///
/// A user's `Doc has:` block is every inherent method `Doc` will ever have, so
/// a name it does not have is `SC0532`. The prelude's blocks are a partial
/// transcription of `stdlib-core.md` §9 — `String` has thirteen of its nineteen
/// — so a name they do not have means *"not written down yet"*, and reporting
/// it would put a diagnostic on `text.slice(0..4)`, which is a correct program.
///
/// The day §9 is transcribed whole, the second half of this test is what has to
/// change, and `methods`'s §8 says so.
#[test]
fn a_missing_method_reports_on_a_user_type_and_is_silent_on_a_builtin() {
    let user = program(
        "
def read(doc: Doc) -> String:
    doc.shorten()
",
    );
    assert_eq!(user.codes(), vec![532]);

    let builtin = check(
        "\
def read(text: borrowed String) -> Int:
    text.shorten()
",
    );
    builtin.assert_clean();
}

/// The same silence through a *type* receiver, which is the arm this change
/// opened: `String.bogus()` now reaches the index where it used to stop at
/// `type_receiver`, and `surface_is_closed` is what keeps it quiet.
#[test]
fn a_missing_associated_function_on_a_builtin_is_silent_for_the_same_reason() {
    let checked = check(
        "\
def scratch() -> String:
    String.bogus()
",
    );
    checked.assert_clean();
}

/// A prelude head the index has **no entry for at all** — `Methods::receiver`'s
/// own arm, one step before `surface_is_closed`.
///
/// `builtins.rs` declares blocks on five of its types and on none of the
/// numeric primitives, so `I64` is a receiver this compiler cannot speak for
/// rather than one it answers no about. Accepting `DefKind::Primitive` at
/// `type_receiver` does not change that, and this is the test that says so.
#[test]
fn a_prelude_type_with_no_declared_block_is_still_a_receiver_nothing_is_known_about() {
    let checked = check(
        "\
def zero() -> I64:
    I64.new()
",
    );
    checked.assert_clean();
}

/// `Found::Mismatched` through a prelude type: the name is there and the form
/// is not.
///
/// `String.length()` names an instance method through its type. F0 has no
/// qualified-call syntax, so `methods`'s §5 makes this silence rather than a
/// diagnostic — the same answer `Doc.describe()` gets above, now reachable on a
/// builtin too.
#[test]
fn an_instance_method_named_through_a_prelude_type_is_not_reported_either() {
    let checked = check(
        "\
def size() -> Int:
    String.length()
",
    );
    checked.assert_clean();
}

// --- the finding this change exposed, and what closed it ------------------

/// **`Box of C` reaches `Box of any I`, and `assign`'s §4a says so.**
///
/// This test asserted the reverse, and the reversal is the finding rather than
/// a change of taste. Six corpus sites write exactly this and were invisible
/// while `Box.new` resolved to nothing; when the declaration landed, the
/// refusal they met was `assign`'s §4 listing *an unsizing under a type
/// constructor* among *"three things it deliberately does not reach"* — while
/// the same file's §5 told an author to write *"`Box.new(doc)`"*, in those
/// words. Two sentences, one file, no agreement.
///
/// **What it is pinned here for is unchanged by which way it now goes.** The
/// corpus test pins a *code* against a file; this pins the **relation**, and
/// the relation is that a mismatch between two `Box`es is settled by a question
/// about the element rather than by one about `Box`. The refusal below the
/// admission is that same question answered no, and it still names both
/// `Box`es.
#[test]
fn a_box_of_a_concrete_type_reaches_a_box_of_an_interface_object() {
    let checked = program(
        "
def into_summary(doc: Doc) -> Box of any Summarize:
    Box.new(doc)
",
    );
    checked.assert_clean();

    // The refusal that did not move: `Untouched` implements nothing, so the
    // same shape is the same mismatch, with the message this test used to
    // assert of `Doc`.
    let refused = program(
        "
type Untouched:
    detail: String

def into_summary(value: Untouched) -> Box of any Summarize:
    Box.new(value)
",
    );
    assert_eq!(refused.codes(), vec![525]);
    assert_eq!(
        refused.messages(),
        vec!["expected `Box of any Summarize`, found `Box of Untouched`".to_string()]
    );

    // The control, and it is the half `assign`'s §4 admitted first: behind a
    // borrow the same unsizing is free and happens.
    let borrowed = program(
        "
def describe(doc: Doc) -> String:
    describe_any(borrowed doc)

def describe_any(value: borrowed any Summarize) -> String:
    value.summarize()
",
    );
    borrowed.assert_clean();
}

/// The tail expression's type, for the tests above that assert what a call
/// produced rather than what it reported.
fn tail(checked: &support::Checked, function: &str) -> science_types::Ty {
    let body = checked.body(function);
    let (id, _) = body
        .exprs()
        .filter(|(_, expr)| matches!(expr.kind, ExprKind::Call { .. }))
        .last()
        .expect("the body has a call");
    body.ty(id)
}

// --- the prelude's own declarations ---------------------------------------

/// `builtins.rs`' blocks reach this index through the same two arms a user's
/// `Doc has:` reaches it through, so a prelude method call resolves, its
/// arguments are checked, and it has a return type.
#[test]
fn a_prelude_method_resolves_with_its_declared_types() {
    let checked = check(
        "\
def lookup(settings: borrowed Map of (String, String), key: borrowed String) -> (borrowed String)?:
    settings.get(key)
",
    );
    checked.assert_clean();
    let body = checked.body("lookup");
    let (_, call) = body
        .exprs()
        .find(|(_, expr)| matches!(expr.kind, ExprKind::MethodCall { .. }))
        .expect("the body has a method call");
    assert!(matches!(call.kind, ExprKind::MethodCall { method: Some(_), .. }));
    assert_eq!(checked.render(call.ty), "(borrowed String)?");
}

/// The three ways a declared prelude method is now *checked* rather than
/// ignored: the argument type, the argument count, and the receiver's own
/// generic arguments reaching the parameter.
#[test]
fn a_prelude_method_call_is_checked_against_its_declaration() {
    let checked = check(
        "\
def wrong(settings: borrowed Map of (String, I64)) -> Bool:
    settings.contains(1)
",
    );
    assert_eq!(checked.codes(), vec![525]);
}

/// `Declarations::borrow_sources` — the decision `science-regions`' `summary`
/// §4 consumes, asserted where it is made.
///
/// `Map.get(self, key: borrowed K) -> (borrowed V)?` borrows the map and not
/// the key, and that answer is what closed the two `SC0333`s in
/// `examples/09_absence_and_failure.science`. The three cases below are the
/// whole of the rule: a referent reachable from the receiver only, a return
/// with no reference in it at all, and a body-less declaration in the program
/// rather than in the prelude.
#[test]
fn a_declarations_borrow_sources_names_the_parameters_the_return_can_reach() {
    let checked = check(
        "\
def read(path: borrowed String) -> (borrowed String)?:
    null
",
    );
    checked.assert_clean();

    // The prelude's `Map.get`, found the way a reader would: a builtin `get`
    // whose block's `Self` is a `Map`.
    let get = checked
        .krate
        .defs
        .iter()
        .filter(|def| def.kind == DefKind::Fn && def.name == "get" && def.is_builtin())
        .find(|def| {
            def.parent
                .and_then(|block| checked.decls.self_ty(block))
                .is_some_and(|ty| checked.render(ty).starts_with("Map of"))
        })
        .expect("the prelude declares `Map.get`")
        .id;

    // Parameter 0 is the receiver; parameter 1 is the key, and `borrowed K`
    // mentions nothing the referent `V` mentions.
    assert_eq!(checked.decls.borrow_sources(&checked.types, get), Some(vec![0]));

    // A return with no reference in it borrows nothing at all, which is
    // strictly better than the opaque assumption and is what `read_file` gets.
    let read_file = checked.def("read_file", DefKind::Fn);
    assert_eq!(checked.decls.borrow_sources(&checked.types, read_file), Some(Vec::new()));

    // A function with a body is not this question: `science-regions` analyses
    // it and the analysis is better evidence than the signature.
    let read = checked.def("read", DefKind::Fn);
    assert_eq!(checked.decls.borrow_sources(&checked.types, read), None);
}
