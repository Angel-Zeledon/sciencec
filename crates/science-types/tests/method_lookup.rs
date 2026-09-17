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
