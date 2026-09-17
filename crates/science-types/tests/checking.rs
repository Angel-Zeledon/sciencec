//! Bidirectional checking and the THIR it builds — Decisions 1, 2, 3 and 14.
//!
//! The claims here are about three things that a `u32` cannot carry and a
//! snapshot would not notice drifting: that **every node has a type**, that the
//! **implicit conversions are nodes**, and that the **site** is what decides
//! whether one of them is a box. `tests/relations.rs` made the same argument
//! about identity one layer down.

mod support;

use science_types::assign::Coercion;
use science_types::thir::{ExprKind, Place};
use support::{check, describe};

const FIXTURE: &str = "\
type Doc:
    title: String

type Embedding is Array of F32

type MyError:
    detail: String

type Config:
    port: Port?

type Port:
    number: I64

def blank() -> Doc:
    Doc(title: \"\")

def takes_embedding(e: Embedding) -> Bool:
    true

def takes_error(err: Error?) -> Bool:
    true

Doc has:
    def describe(self) -> String:
        self.title

MyError implements Error:
    def message(self) -> String:
        self.detail
";

fn program(body: &str) -> support::Checked {
    check(&format!("{FIXTURE}{body}"))
}

// --- Decision 3, clause by clause ----------------------------------------

#[test]
fn every_node_in_a_checked_body_has_a_type() {
    // Decision 3's first clause. `{unknown}` is `Ty::ERROR`'s rendering, and a
    // body the checker was silent about should contain none.
    let checked = program(
        "
def title_of(doc: Doc) -> String:
    doc.title
",
    );
    checked.assert_clean();
    for (kind, ty) in checked.nodes("title_of") {
        assert_ne!(ty, "{unknown}", "the `{kind}` node has no type");
    }
}

#[test]
fn a_method_call_carries_the_implementation_it_resolved_to() {
    // Decision 3's second clause, which was the largest hole in the layer and
    // is now a lookup: the node's `method: Option<DefId>` names the `describe`
    // in `Doc has:`, and the call has that method's return type rather than
    // `Ty::ERROR`. `check`'s §6 used to price this; `methods` is what closed
    // it.
    let checked = program(
        "
def shout(doc: Doc) -> Bool:
    let _said be doc.describe()
    true
",
    );
    checked.assert_clean();
    let describe = checked.def("describe", science_resolve::hir::DefKind::Fn);
    let body = checked.body("shout");
    let (_, call) = body
        .exprs()
        .find(|(_, expr)| matches!(expr.kind, ExprKind::MethodCall { .. }))
        .expect("the body has a method call");
    let ExprKind::MethodCall { method, .. } = call.kind else { unreachable!() };
    assert_eq!(method, Some(describe));
    assert_eq!(checked.render(call.ty), "String");
}

#[test]
fn a_declared_method_on_a_prelude_type_resolves_and_has_a_type() {
    // The half of the conservatism that is retired. `builtins.rs` declares
    // `String.length`, so the receiver reaches a candidate, the call carries
    // the method's definition, and the result is `Int` rather than a hole.
    let checked = program(
        "
def length_of(doc: Doc) -> Bool:
    let _size be doc.title.length()
    true
",
    );
    checked.assert_clean();
    let body = checked.body("length_of");
    let (_, call) = body
        .exprs()
        .find(|(_, expr)| matches!(expr.kind, ExprKind::MethodCall { .. }))
        .expect("the body has a method call");
    assert!(matches!(call.kind, ExprKind::MethodCall { method: Some(_), .. }));
    assert_eq!(checked.render(call.ty), "Int");
}

#[test]
fn an_undeclared_method_on_a_prelude_type_is_unresolved_and_still_silent() {
    // The half that survives, asserted so that it stays a decision. The
    // prelude's transcription of `stdlib-core.md` §9 is partial — `String` has
    // thirteen of its nineteen methods — so `methods`' §8 keeps a builtin
    // head's method set **open**: an unknown name there is silence, never
    // `SC0532`, because the alternative is a false positive on
    // `text.slice(0..4)`, which the note says exists.
    let checked = program(
        "
def sliced(doc: Doc) -> Bool:
    let _part be doc.title.slice(0)
    true
",
    );
    checked.assert_clean();
    let body = checked.body("sliced");
    let (_, call) = body
        .exprs()
        .find(|(_, expr)| matches!(expr.kind, ExprKind::MethodCall { .. }))
        .expect("the body has a method call");
    assert!(matches!(call.kind, ExprKind::MethodCall { method: None, .. }));
    assert_eq!(checked.render(call.ty), "{unknown}");
}

#[test]
fn an_implicit_widening_is_a_node_and_not_a_retyped_value() {
    // Decision 3's third clause. The operand keeps `String`; the node is
    // `String?`; nothing in between is rewritten.
    let checked = program(
        "
def maybe(doc: Doc) -> String?:
    doc.title
",
    );
    checked.assert_clean();
    let body = checked.body("maybe");
    let (_, coerce) = body
        .exprs()
        .find(|(_, expr)| matches!(expr.kind, ExprKind::Coerce { .. }))
        .expect("`String` into `String?` is a widening");
    let ExprKind::Coerce { operand, coercion } = coerce.kind else { unreachable!() };
    assert_eq!(coercion, Coercion::Widen);
    assert_eq!(checked.render(coerce.ty), "String?");
    assert_eq!(checked.render(body.ty(operand)), "String");
}

#[test]
fn an_identity_conversion_produces_no_node() {
    // `thir`'s §1: *"nothing is emitted when nothing happens, because a node
    // that means 'no change' is a node every later pass has to see through"*.
    let checked = program(
        "
def same(doc: Doc) -> String:
    doc.title
",
    );
    checked.assert_clean();
    assert!(!checked.nodes("same").iter().any(|(kind, _)| kind == "coerce"));
}

// --- Decision 14, and the site that decides it ----------------------------

#[test]
fn a_concrete_error_boxes_at_a_return_and_widens_in_one_step() {
    let checked = program(
        "
def fails() -> Error?:
    return MyError(detail: \"x\")
",
    );
    checked.assert_clean();
    let body = checked.body("fails");
    let coercions: Vec<Coercion> = body
        .exprs()
        .filter_map(|(_, expr)| match expr.kind {
            ExprKind::Coerce { coercion, .. } => Some(coercion),
            _ => None,
        })
        .collect();
    assert_eq!(coercions, vec![Coercion::BoxThenWiden]);
}

#[test]
fn decision_14s_own_example_works_because_the_tuple_is_visited_elementwise() {
    // `assign`'s §2 says the trap in as many words: `(Doc, MyError)` is *not*
    // assignable to `(Doc, Error?)`, because no conversion there recurses. This
    // line compiles only because the thing in return position is a tuple
    // *expression* whose elements the checker visits one at a time, each at
    // `Site::Return`. A checker that compared the synthesised tuple type
    // against the signature and stopped would reject the line that motivated
    // Decision 14.
    let checked = program(
        "
def read() -> (Doc, Error?):
    return (blank(), MyError(detail: \"x\"))
",
    );
    checked.assert_clean();
}

#[test]
fn a_concrete_error_boxes_at_an_argument() {
    let checked = program(
        "
def hand_over() -> Bool:
    takes_error(MyError(detail: \"x\"))
",
    );
    checked.assert_clean();
}

#[test]
fn a_concrete_error_does_not_box_anywhere_else() {
    // `assign`'s §6: *"a checker that passes `Site::Return` everywhere silently
    // makes boxing universal"*. A `let` is `Site::Elsewhere` and must refuse.
    let checked = program(
        "
def stashed() -> Bool:
    let _err: Error? be MyError(detail: \"x\")
    true
",
    );
    assert_eq!(checked.codes(), vec![525]);
}

// --- the seam's middle call ----------------------------------------------

#[test]
fn an_alias_and_its_expansion_are_one_type_at_a_site() {
    // `lib.rs` §5: skipping `reveal` is *"`Embedding` failing to match `Array
    // of F32` at one site in ten"*. This is that site.
    let checked = program(
        "
def pass_through(raw: Array of F32) -> Bool:
    takes_embedding(raw)
",
    );
    checked.assert_clean();
}

#[test]
fn a_mismatch_names_the_type_the_author_wrote_and_not_its_expansion() {
    // `alias`'s §1 is the argument: revealing eagerly would report `Array of
    // F32` at a site where the reader wrote `Embedding`, *"and the name they
    // chose disappears from the compiler's vocabulary"*.
    let checked = program(
        "
def wrong(e: Embedding) -> Doc:
    e
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(checked.messages(), vec!["expected `Doc`, found `Embedding`".to_string()]);
}

// --- Decision 2 -----------------------------------------------------------

#[test]
fn an_unconstrained_integer_literal_defaults_to_i64() {
    let checked = program(
        "
def counted() -> Bool:
    let n be 1
    true
",
    );
    checked.assert_clean();
    let body = checked.body("counted");
    let (_, ty) = body.locals().next().expect("the body binds `n`");
    assert_eq!(checked.render(ty), "I64");
}

#[test]
fn an_unconstrained_float_literal_defaults_to_f64() {
    let checked = program(
        "
def measured() -> Bool:
    let x be 1.5
    true
",
    );
    checked.assert_clean();
    let body = checked.body("measured");
    let (_, ty) = body.locals().next().expect("the body binds `x`");
    assert_eq!(checked.render(ty), "F64");
}

#[test]
fn a_literal_takes_the_type_the_annotation_asked_for() {
    // *"a scientific language that silently makes `1` an `I32` will be wrong on
    // somebody's index arithmetic"* — so the annotation decides and the default
    // only applies when nothing else did.
    let checked = program(
        "
def sized() -> Bool:
    let n: I32 be 1
    true
",
    );
    checked.assert_clean();
    let body = checked.body("sized");
    let (_, ty) = body.locals().next().expect("the body binds `n`");
    assert_eq!(checked.render(ty), "I32");
}

#[test]
fn a_suffix_fixes_the_type_against_the_default() {
    let checked = program(
        "
def sized() -> Bool:
    let n be 7u8
    true
",
    );
    checked.assert_clean();
    let body = checked.body("sized");
    let (_, ty) = body.locals().next().expect("the body binds `n`");
    assert_eq!(checked.render(ty), "U8");
}

#[test]
fn an_integer_literal_is_admitted_at_a_floating_type() {
    // `check`'s §5: *"`let x: F64 be 1` is what a scientific program writes"*.
    let checked = program(
        "
def scaled() -> Bool:
    let x: F64 be 1
    true
",
    );
    checked.assert_clean();
}

#[test]
fn a_floating_literal_is_not_admitted_at_an_integer_type() {
    // The other half of §5, and the asymmetry is the point: the target cannot
    // hold the value.
    let checked = program(
        "
def truncated() -> Bool:
    let n: I32 be 1.5
    true
",
    );
    assert_eq!(checked.codes(), vec![525]);
}

#[test]
fn an_integer_literal_is_not_admitted_at_a_string() {
    let checked = program(
        "
def wrong() -> Bool:
    let s: String be 1
    true
",
    );
    assert_eq!(checked.codes(), vec![525]);
}

#[test]
fn a_value_with_nothing_to_infer_from_is_reported_once() {
    // `SC0526`, and once per inference *class*: `infer`'s §5, *"a class is one
    // unknown however many expressions joined it"*.
    let checked = program(
        "
def nothing() -> Bool:
    let x be null
    true
",
    );
    assert_eq!(checked.codes(), vec![526]);
}

#[test]
fn an_expectation_crosses_an_unsafe_block_the_way_it_crosses_a_plain_one() {
    // What the keyword changes is which operations the body may name, which is
    // not checking mode's question. Without this, the tail is synthesised and
    // the `null` in it has nothing to take its type from — `SC0526` on a line
    // whose type the signature states. `examples/20_extern.science`'s
    // `native_double_type` is the case: its whole body is one `unsafe` block
    // ending in a pair.
    let checked = program(
        "
def native() -> (Doc, Error?):
    unsafe:
        (blank(), null)
",
    );
    checked.assert_clean();
}

// --- the rest of the band -------------------------------------------------

#[test]
fn a_call_with_the_wrong_number_of_arguments_is_reported() {
    let checked = program(
        "
def wrong() -> Bool:
    takes_error()
",
    );
    assert_eq!(checked.codes(), vec![527]);
}

#[test]
fn a_field_the_type_does_not_have_is_reported() {
    let checked = program(
        "
def wrong(doc: Doc) -> String:
    doc.subtitle
",
    );
    assert_eq!(checked.codes(), vec![528]);
    assert_eq!(checked.messages(), vec!["`Doc` has no field `subtitle`".to_string()]);
}

#[test]
fn a_let_that_binds_two_names_against_one_value_is_reported() {
    // The check `hir::LetBinding` hands here by name.
    let checked = program(
        "
def wrong() -> Bool:
    let a, b be blank()
    true
",
    );
    assert_eq!(checked.codes(), vec![529]);
}

#[test]
fn a_presence_test_on_something_that_is_never_absent_is_reported() {
    let checked = program(
        "
def always(doc: Doc) -> Bool:
    doc?
",
    );
    assert_eq!(checked.codes(), vec![530]);
}

#[test]
fn a_presence_test_on_a_nullable_is_fine_and_is_a_bool() {
    let checked = program(
        "
def sometimes(doc: Doc?) -> Bool:
    doc?
",
    );
    checked.assert_clean();
}

// --- one bad annotation stays one diagnostic ------------------------------

#[test]
fn an_erroneous_type_agrees_with_whatever_it_meets() {
    // `ty`'s §5, reached through the checker. The receiver is a method the
    // prelude has **not** declared — `length` now resolves and would give a
    // real `Int` — so the call has no type, and the `Ty::ERROR` it gets makes
    // every use of the result silent rather than producing one message per use.
    let checked = program(
        "
def cascade(doc: Doc) -> String:
    let unknown be doc.title.slice(0)
    let _first be unknown
    let _second: I32 be unknown
    doc.title
",
    );
    checked.assert_clean();
}

// --- places, which is what `region-inference.md` §10 item 2 asks for -------

#[test]
fn two_spellings_of_one_place_produce_one_place() {
    // §10 item 2: *"two expressions that denote the same place must produce the
    // same place"*. `thir`'s §3 meets it for the local-and-field fragment, and
    // this is the assertion that says so.
    let checked = program(
        "
def twice(config: Config) -> Bool:
    let _a be config.port
    let _b be config.port
    true
",
    );
    checked.assert_clean();
    let body = checked.body("twice");
    let places: Vec<Place> = body
        .exprs()
        .filter(|(_, expr)| matches!(expr.kind, ExprKind::Field { .. }))
        .filter_map(|(id, _)| body.place_of(id))
        .collect();
    assert_eq!(places.len(), 2);
    assert_eq!(places[0], places[1]);
}

#[test]
fn a_call_is_not_a_place() {
    // The other half of §3's restriction, and the honest one: `f()` twice is
    // two calls, so it denotes no place at all.
    let checked = program(
        "
def twice() -> Bool:
    let _a be blank()
    true
",
    );
    let body = checked.body("twice");
    let call = checked.find("twice", |kind| matches!(kind, ExprKind::Call { .. }));
    assert_eq!(body.place_of(call), None);
}

// --- the tree keeps the program's shape -----------------------------------

#[test]
fn a_mistyped_expression_is_kept_rather_than_replaced_by_a_hole() {
    // `check`'s `coerce`: *"Decision 3 makes THIR the tree a diagnostic quotes,
    // and replacing a mistyped expression with a hole throws away the structure
    // the next message would have needed."*
    let checked = program(
        "
def wrong(doc: Doc) -> I32:
    doc.title
",
    );
    assert_eq!(checked.codes(), vec![525]);
    let kinds: Vec<String> =
        checked.nodes("wrong").into_iter().map(|(kind, _)| kind).collect();
    assert!(kinds.contains(&"field".to_string()), "the field access survives: {kinds:?}");
    assert!(!kinds.contains(&describe(&ExprKind::Error)));
}

// --- §6.3 of the core spec: auto-borrow at call sites ---------------------

#[test]
fn a_parameter_declared_borrowed_is_borrowed_automatically() {
    // §6.3: *"If a parameter is declared `borrowed T`, the caller writes
    // `compare(a, b)`, not `compare(borrowed a, borrowed b)`."* `check`'s
    // `auto_borrow` is the argument for why that lives here and not in
    // `assign`.
    let checked = program(
        "
def length(text: borrowed String) -> I64:
    0

def measure(owned: String) -> I64:
    length(owned)
",
    );
    checked.assert_clean();
    let body = checked.body("measure");
    let borrows: Vec<String> = body
        .exprs()
        .filter(|(_, expr)| matches!(expr.kind, ExprKind::Borrow { mutable: false, .. }))
        .map(|(_, expr)| checked.render(expr.ty))
        .collect();
    assert_eq!(borrows, vec!["borrowed String".to_string()]);
}

#[test]
fn auto_borrow_covers_the_exclusive_case_too() {
    let checked = program(
        "
def retitle(doc: mutable borrowed Doc, title: String):
    doc.title be title

def rename(start: Doc) -> Bool:
    let mutable doc be start
    retitle(doc, \"renamed\")
    true
",
    );
    checked.assert_clean();
    let body = checked.body("rename");
    assert!(body
        .exprs()
        .any(|(_, expr)| matches!(expr.kind, ExprKind::Borrow { mutable: true, .. })));
}

#[test]
fn an_exclusive_auto_borrow_invalidates_the_narrowing_like_a_written_one() {
    // Decision 8 does not care whether the author typed the word.
    let checked = program(
        "
def retitle(doc: mutable borrowed Doc, title: String):
    doc.title be title

def after(start: Doc?) -> String?:
    let mutable a be start
    if a?:
        retitle(a, \"x\")
        return a.title
    null
",
    );
    assert_eq!(checked.codes(), vec![528]);
}

#[test]
fn auto_borrow_does_not_apply_away_from_a_call() {
    // §6.3 says *"at call sites"*, and a `let` is not one. Admitting it there
    // would be a language change made by a checker.
    let checked = program(
        "
def kept(owned: String) -> Bool:
    let _view: borrowed String be owned
    true
",
    );
    assert_eq!(checked.codes(), vec![525]);
}

// --- §4's unsizing, composed with the auto-borrow above -------------------

/// The shape of `examples/08_dyn_dispatch.science`, cut down to the six lines
/// the decision is about.
///
/// The names are the example's own, because the claim being tested is about
/// that file: `describe_any(doc)` must compile, `clear(note)` must compile,
/// and `describe_boxed(doc)` must not.
const DISPATCH: &str = "
interface Summarize:
    def summarize(self) -> String

interface Reset:
    def reset(mutable self)

Doc implements Summarize:
    def summarize(self) -> String:
        self.title

Doc implements Reset:
    def reset(mutable self):
        self.title be \"\"

def describe_any(value: borrowed any Summarize) -> String:
    \"\"

def clear(value: mutable borrowed any Reset):
    let _touched be 1

def describe_boxed(value: Box of any Summarize) -> String:
    \"\"

type Renderer:
    target: borrowed any Summarize
";

#[test]
fn describe_any_takes_a_doc_by_composing_the_auto_borrow_with_an_unsize() {
    // The acceptance case, by name. §6.3 says the borrow may be taken and
    // `assign`'s §4 says the borrow unsizes, and neither step is a rule this
    // file invented.
    let checked = program(&format!(
        "{DISPATCH}
def main_line(doc: Doc) -> String:
    describe_any(doc)
"
    ));
    checked.assert_clean();

    // And both nodes are there, in that order, with the borrow underneath.
    let body = checked.body("main_line");
    let (_, coerce) = body
        .exprs()
        .find(|(_, expr)| matches!(expr.kind, ExprKind::Coerce { .. }))
        .expect("the argument is coerced");
    let ExprKind::Coerce { operand, coercion } = coerce.kind else { unreachable!() };
    assert_eq!(coercion, Coercion::Unsize);
    assert_eq!(checked.render(coerce.ty), "borrowed any Summarize");
    assert!(matches!(body.expr(operand).kind, ExprKind::Borrow { mutable: false, .. }));
    assert_eq!(checked.render(body.ty(operand)), "borrowed Doc");
}

#[test]
fn clear_takes_a_note_through_an_exclusive_borrowed_object() {
    // `clear(note)` in the same file, and the mutability rides through both
    // steps: the borrow is exclusive and so is the object it unsizes into.
    let checked = program(&format!(
        "{DISPATCH}
def main_line(note: Doc):
    clear(note)
"
    ));
    checked.assert_clean();
    let body = checked.body("main_line");
    let (_, coerce) = body
        .exprs()
        .find(|(_, expr)| matches!(expr.kind, ExprKind::Coerce { .. }))
        .expect("the argument is coerced");
    let ExprKind::Coerce { operand, coercion } = coerce.kind else { unreachable!() };
    assert_eq!(coercion, Coercion::Unsize);
    assert_eq!(checked.render(coerce.ty), "mutable borrowed any Reset");
    assert!(matches!(body.expr(operand).kind, ExprKind::Borrow { mutable: true, .. }));
}

#[test]
fn a_doc_does_not_reach_an_owned_box_of_an_object() {
    // The half of the decision that is easy to lose. `Box of any Summarize`
    // allocates, `assign`'s §5 refuses to do that implicitly, and the example
    // goes on writing `Box.new(Doc(..))`. There is no auto-borrow to compose
    // with here either: the parameter is not a borrow.
    let checked = program(&format!(
        "{DISPATCH}
def main_line(doc: Doc) -> String:
    describe_boxed(doc)
"
    ));
    assert_eq!(checked.codes(), vec![525]);
}

#[test]
fn a_written_borrow_unsizes_at_a_field_initialiser() {
    // `Renderer(target: borrowed doc)` — `Site::Elsewhere`, where Decision
    // 14's boxing does not happen. `assign`'s §4 is not gated on the site
    // because it emits no code, and this is the line that needs it not to be.
    let checked = program(&format!(
        "{DISPATCH}
def main_line(doc: Doc) -> Renderer:
    Renderer(target: borrowed doc)
"
    ));
    checked.assert_clean();
    let body = checked.body("main_line");
    let (_, coerce) = body
        .exprs()
        .find(|(_, expr)| matches!(expr.kind, ExprKind::Coerce { .. }))
        .expect("the field initialiser is coerced");
    let ExprKind::Coerce { coercion, .. } = coerce.kind else { unreachable!() };
    assert_eq!(coercion, Coercion::Unsize);
}

#[test]
fn an_object_behind_a_borrow_does_not_upcast_to_another_one() {
    // `unsizable`'s third refusal, reached through a body: which vtable a
    // `borrowed any Summarize` would carry as a `borrowed any Reset` needs a
    // subinterface relation nobody has specified.
    let checked = program(&format!(
        "{DISPATCH}
def main_line(summary: borrowed any Summarize):
    clear(summary)
"
    ));
    assert_eq!(checked.codes(), vec![525]);
}

// --- `Self` and the block's associated types ------------------------------

#[test]
fn self_in_a_return_type_is_the_blocks_own_type() {
    let checked = program(
        "
Doc has:
    def copy(self) -> Self:
        Doc(title: \"\")
",
    );
    checked.assert_clean();
}

#[test]
fn an_associated_type_is_the_one_the_implementation_answered_with() {
    // `subst`'s §2 leaves `Self.Item` standing *"precisely so that the phase
    // which can see both blocks reports it"*, and a method body is that phase.
    let checked = program(
        "
interface Holds:
    type Item
    def first(self) -> Self.Item?

Doc implements Holds:
    type Item is I64
    def first(self) -> Self.Item?:
        1
",
    );
    checked.assert_clean();
}

// --- a comparison is not an assignment ------------------------------------

#[test]
fn a_comparison_looks_through_the_borrow_the_author_did_not_write() {
    // §5.4 makes `is` and `==` one operator dispatching to `Eq`, whose method
    // takes `borrowed self` — so demanding that the two *written* types agree
    // reports on the borrow §6.3 told the author to leave out.
    let checked = program(
        "
def named(name: borrowed String) -> Bool:
    name is \"\"
",
    );
    checked.assert_clean();
}

#[test]
fn a_comparison_of_two_different_types_is_still_reported() {
    let checked = program(
        "
def wrong(doc: Doc, title: String) -> Bool:
    doc is title
",
    );
    // Two findings, and they are separate mistakes. `SC0535` is §5.4's rule
    // that `is` dispatches to `Eq` and `Doc` implements nothing —
    // `tests/operators.rs`' `is_requires_eq` is where that decision is argued
    // — and `SC0525` is this test's own subject, which survives it: the two
    // operands are still not the same type once the borrows are off.
    assert_eq!(checked.codes(), vec![535, 525]);
}

// --- what the checker refuses to guess about ------------------------------

#[test]
fn a_field_of_a_generic_parameter_is_silent() {
    // *"`T` has no field `name`"* is a claim about an instantiation nobody has
    // made yet, and Decision 11's lookup is what would answer it. `Ty::ERROR`
    // and no diagnostic.
    let checked = program(
        "
def field_of of T(value: T) -> Bool:
    let _read be value.title
    true
",
    );
    checked.assert_clean();
}

#[test]
fn every_block_in_the_body_is_reachable_from_the_arena() {
    // `thir`'s `blocks`: a pass that wants all the statements iterates the
    // arena rather than walking the tree looking for blocks it has an arm for.
    // `SC0140` depends on it, and a block reachable only through a node kind
    // the walk forgot is the bug that abolishes.
    let checked = program(
        "
def nested(ok: Bool) -> Bool:
    if ok:
        loop:
            break
    true
",
    );
    checked.assert_clean();
    let body = checked.body("nested");
    // The body, the `if`'s then-branch, and the `loop`'s body.
    assert_eq!(body.blocks().count(), 3);
    assert_eq!(body.root().index(), 0);
}

#[test]
fn a_body_that_leaves_on_every_path_is_recorded_as_diverging() {
    let checked = program(
        "
def gone() -> Bool:
    return true
",
    );
    checked.assert_clean();
    assert!(checked.body("gone").diverges());
    let falls_through = program(
        "
def stays() -> Bool:
    true
",
    );
    assert!(!falls_through.body("stays").diverges());
}
