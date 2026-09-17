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

#[test]
fn two_implementations_of_one_interface_are_not_an_ambiguity() {
    // `methods`'s §6, which is the case Decision 11 calls an ambiguity and the
    // corpus calls a program: `examples/00_kitchen_sink.science` implements
    // `From` twice and then calls `LoadError.from(..)`. Not resolved, because
    // choosing needs the argument's type; not reported, because the program is
    // correct.
    let checked = check(
        "\
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

def widen(err: IoError) -> LoadError:
    LoadError.from(err)
",
    );
    checked.assert_clean();
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
