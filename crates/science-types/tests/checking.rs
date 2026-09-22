//! Bidirectional checking and the THIR it builds — Decisions 1, 2, 3 and 14.
//!
//! The claims here are about three things that a `u32` cannot carry and a
//! snapshot would not notice drifting: that **every node has a type**, that the
//! **implicit conversions are nodes**, and that the **site** is what decides
//! whether one of them is a box. `tests/relations.rs` made the same argument
//! about identity one layer down.

mod support;

use science_types::assign::Coercion;
use science_types::thir::{ExprId, ExprKind, Place};
use support::{check, describe};

const FIXTURE: &str = "\
type Doc:
    title: String

type Embedding is Array[F32]

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

# Decision 27 (`type-checking-and-mir.md` §7): `self.title`/`self.detail`
# through an implicit-borrow `self` now types as `&String`, not `String`, so
# a getter that used to return the field directly needs the same explicit
# copy `examples/09_absence_and_failure.science`'s `ConfigError.message`
# already uses — `String` has no `clone`, so this is what one looks like.
def copy_of(text: &String) -> String:
    let mutable out be String.new()
    out.push_str(text)
    out

Doc has:
    def describe(self) -> String:
        copy_of(self.title)

MyError implements Error:
    def message(self) -> String:
        copy_of(self.detail)
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
    assert_eq!(checked.render(call.ty), "I64");
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
def pass_through(raw: Array[F32]) -> Bool:
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
fn a_deferred_receiver_parameter_does_not_poison_the_argument_that_would_solve_it() {
    // `Box.new(Leaf(1))` nested inside another generic constructor —
    // `04_enums.science`'s `Node(Box.new(Leaf(1)), Box.new(Leaf(2)))`, reduced
    // to one argument. `Leaf(1)` defers its own `T` to `1`'s placeholder
    // (`call_variant`), so `Leaf(1)`'s own type is a `pending_named`
    // placeholder rather than a known `Ty`. `Box.new`'s receiver has no
    // argument of its own to solve its `T` from except this one, so
    // `receiver_arguments` defers `Box`'s `T` to the *same* placeholder — and
    // `call_method` then substitutes `Box.new`'s declared parameter type
    // (`T`) with `Ty::ERROR`, for lack of anything else to put there, and
    // demands `Leaf(1)` agree with it. Demanding used to bind the
    // placeholder to `Ty::ERROR` right there, before `finish` ever ran,
    // which is `Ty::ERROR` reaching a value with no diagnostic beside it —
    // `science-codegen-llvm`'s `SC0400` for a `TyKind::Error` with nothing
    // reported. `Node`'s own generic argument reads the same placeholder
    // through `structural_solve`, so the corruption reached `tree`'s type
    // too, one call further out.
    let checked = program(
        "
choice Tree[T]:
    Leaf(T)
    Node(Box[Tree[T]])

def build() -> Bool:
    let tree be Node(Box.new(Leaf(1)))
    true
",
    );
    checked.assert_clean();
    // Every node except the two `Item` callees (`Node`, `Box.new`'s own
    // `new`, `Leaf`) has a real type — those are pushed at `Ty::ERROR` by
    // design (`check`'s `call`/`call_variant`/`associated_call`: a callee is
    // not itself a value), so they are excluded here rather than asserted on.
    for (kind, ty) in checked.nodes("build") {
        if kind == "item" {
            continue;
        }
        assert_ne!(ty, "{unknown}", "the `{kind}` node has no type");
    }
    let body = checked.body("build");
    let (_, ty) = body.locals().next().expect("the body binds `tree`");
    assert_eq!(checked.render(ty), "Tree[I64]");
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
def length(text: &String) -> I64:
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
    assert_eq!(borrows, vec!["&String".to_string()]);
}

#[test]
fn auto_borrow_covers_the_exclusive_case_too() {
    let checked = program(
        "
def retitle(doc: &mut Doc, title: String):
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
def retitle(doc: &mut Doc, title: String):
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
    let _view: &String be owned
    true
",
    );
    assert_eq!(checked.codes(), vec![525]);
}

/// **§1b: the borrow is taken at the argument, not inside the branch.**
///
/// The bug this pins was one line of `check` pushing the expectation inward:
/// `borrowed String` crossed into each arm of the `if`, each arm auto-borrowed
/// its own `"yes"`, and each of those temporaries dies at the close of its arm.
/// `science-regions` then reported `SC0333` twice on a correct program — right
/// about the MIR and wrong about the program — and the repair belongs here
/// because the borrow is this phase's node.
///
/// The assertion is the shape rather than the absence of a region diagnostic:
/// **one** borrow, and the thing under it is the `if`. Two borrows under the
/// arms is the old tree, and it is what the region engine was right about.
#[test]
fn a_branch_at_a_borrowed_argument_takes_one_borrow_above_the_branch() {
    let checked = program(
        "
def length(text: &String) -> I64:
    0

def pick(flag: Bool) -> I64:
    length(if flag: \"yes\" else: \"no\")
",
    );
    checked.assert_clean();
    let body = checked.body("pick");
    let borrows: Vec<ExprId> = body
        .exprs()
        .filter(|(_, expr)| matches!(expr.kind, ExprKind::Borrow { .. }))
        .map(|(id, _)| id)
        .collect();
    assert_eq!(borrows.len(), 1, "one call site, one borrow");
    let ExprKind::Borrow { operand, .. } = body.expr(borrows[0]).kind else { unreachable!() };
    assert!(
        matches!(body.expr(operand).kind, ExprKind::If { .. }),
        "the borrow names the branch, not a tail inside one"
    );
    assert_eq!(checked.render(body.ty(operand)), "String");
}

/// **The control. §1b changes where the borrow goes and not whether the arms
/// are checked**, so a branch whose value is the wrong type is refused exactly
/// as it was — once, at the argument, with the branch's own type in it.
#[test]
fn a_branch_whose_value_does_not_fit_the_parameter_is_still_refused() {
    let checked = program(
        "
def length(text: &String) -> I64:
    0

def pick(flag: Bool) -> I64:
    length(if flag: true else: false)
",
    );
    assert_eq!(checked.codes(), vec![525]);
}

/// **And the site rule still holds.** §6.3 says *"at call sites"*; a `let` is
/// not one, and a branch in one is refused for
/// [`auto_borrow_does_not_apply_away_from_a_call`]'s reason and not by a new
/// rule about branches.
#[test]
fn a_branch_at_a_borrowed_binding_is_still_refused() {
    let checked = program(
        "
def pick(flag: Bool) -> Bool:
    let _view: &String be if flag: \"yes\" else: \"no\"
    true
",
    );
    // Two, one per arm: away from a call the expectation still crosses into
    // the branch, so each arm is measured against `borrowed String` on its
    // own. §1b did not move that and was not meant to.
    assert_eq!(checked.codes(), vec![525, 525]);
}

/// §1b composed with §4's unsizing, which is the shape `print(if …)` would have
/// the day `print` is declared: one borrow of the branch, one coercion above
/// it, and nothing inside the arms.
#[test]
fn a_branch_at_a_borrowed_object_parameter_borrows_then_unsizes() {
    let checked = program(&format!(
        "{DISPATCH}
def pick(flag: Bool, left: Doc, right: Doc) -> String:
    describe_any(if flag: left else: right)
"
    ));
    checked.assert_clean();
    let body = checked.body("pick");
    let (coerce_id, coerce) = body
        .exprs()
        .find(|(_, expr)| matches!(expr.kind, ExprKind::Coerce { .. }))
        .expect("the argument is unsized");
    let _ = coerce_id;
    let ExprKind::Coerce { operand, coercion } = coerce.kind else { unreachable!() };
    assert_eq!(coercion, Coercion::Unsize);
    assert_eq!(checked.render(coerce.ty), "&any Summarize");
    let ExprKind::Borrow { operand: branch, .. } = body.expr(operand).kind else {
        panic!("the coercion sits on a borrow")
    };
    assert!(matches!(body.expr(branch).kind, ExprKind::If { .. }));
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
        copy_of(self.title)

Doc implements Reset:
    def reset(mutable self):
        self.title be \"\"

def describe_any(value: &any Summarize) -> String:
    \"\"

def clear(value: &mut any Reset):
    let _touched be 1

def describe_boxed(value: Box[any Summarize]) -> String:
    \"\"

type Renderer:
    target: &any Summarize
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
    assert_eq!(checked.render(coerce.ty), "&any Summarize");
    assert!(matches!(body.expr(operand).kind, ExprKind::Borrow { mutable: false, .. }));
    assert_eq!(checked.render(body.ty(operand)), "&Doc");
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
    assert_eq!(checked.render(coerce.ty), "&mut any Reset");
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
    Renderer(target: &doc)
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
def main_line(summary: &any Summarize):
    clear(summary)
"
    ));
    assert_eq!(checked.codes(), vec![525]);
}

// --- §4a's unsizing under `Box`, by the corpus shape it was admitted for --

/// The shape of the six sites, with the example's own names.
///
/// `Note` is here because two of the six are `Box.new(Note(..))` rather than
/// `Box.new(Doc(..))`, and one concrete type would let a rule that had somehow
/// been keyed on `Doc` pass. `Untouched` implements nothing and is what the
/// negatives are written against.
const BOXED_DISPATCH: &str = "
interface Summarize:
    def summarize(self) -> String

type Note:
    text: String

type Untouched:
    detail: String

Doc implements Summarize:
    def summarize(self) -> String:
        copy_of(self.title)

Note implements Summarize:
    def summarize(self) -> String:
        copy_of(self.text)

def describe_boxed(value: Box[any Summarize]) -> String:
    \"\"
";

/// Every coercion in a body, in the order the checker made them.
fn coercions(checked: &support::Checked, name: &str) -> Vec<Coercion> {
    checked
        .body(name)
        .exprs()
        .filter_map(|(_, expr)| match expr.kind {
            ExprKind::Coerce { coercion, .. } => Some(coercion),
            _ => None,
        })
        .collect()
}

#[test]
fn into_summary_returns_two_different_boxed_concrete_types() {
    // Two of the six, by name: `into_summary`'s arms in
    // `examples/08_dyn_dispatch.science`. `Site::Return`, and two concrete
    // types so that the rule is not reading one of them.
    let checked = program(&format!(
        "{BOXED_DISPATCH}
def into_summary(flag: Bool) -> Box[any Summarize]:
    if flag:
        Box.new(Doc(title: \"a\"))
    else:
        Box.new(Note(text: \"...\"))
"
    ));
    checked.assert_clean();
    assert_eq!(
        coercions(&checked, "into_summary"),
        vec![Coercion::UnsizeInBox, Coercion::UnsizeInBox],
    );
}

#[test]
fn as_summary_returns_a_boxed_concrete_type_from_every_match_arm() {
    // Three more of the six: `as_summary` in
    // `examples/00_kitchen_sink.science`, whose arms are a `match` rather than
    // an `if`. The site is the same and so is the answer, which is the point —
    // §4a is a rule about types and not about which statement reached it.
    let checked = program(&format!(
        "{BOXED_DISPATCH}
choice Format:
    Json
    Plain
    Markdown

def as_summary(format: &Format) -> Box[any Summarize]:
    match format:
        Json: Box.new(Doc(title: \"json\"))
        Plain: Box.new(Doc(title: \"plain\"))
        Markdown: Box.new(Note(text: \"# ...\"))
"
    ));
    checked.assert_clean();
    assert_eq!(
        coercions(&checked, "as_summary"),
        vec![Coercion::UnsizeInBox, Coercion::UnsizeInBox, Coercion::UnsizeInBox],
    );
}

#[test]
fn the_sixth_site_is_an_argument_and_the_node_carries_the_object_type() {
    // The last of the six: `describe_boxed(Box.new(Doc(..)))` in `main`.
    // `Site::Argument`, and the one of the six where the operand is a call
    // rather than the tail of a branch — which is why `science-mir`'s
    // `argument` does *not* take its two-phase path for it.
    let checked = program(&format!(
        "{BOXED_DISPATCH}
def main_line() -> String:
    describe_boxed(Box.new(Doc(title: \"b\")))
"
    ));
    checked.assert_clean();
    let body = checked.body("main_line");
    let (_, coerce) = body
        .exprs()
        .find(|(_, expr)| matches!(expr.kind, ExprKind::Coerce { .. }))
        .expect("the argument is coerced");
    let ExprKind::Coerce { operand, coercion } = coerce.kind else { unreachable!() };
    assert_eq!(coercion, Coercion::UnsizeInBox);
    assert_eq!(checked.render(coerce.ty), "Box[any Summarize]");
    // The operand is the `Box.new` the author wrote, at its own type. That is
    // where the allocation is, and §4a's claim is that the conversion adds no
    // second one.
    assert_eq!(checked.render(body.ty(operand)), "Box[Doc]");
    // And it is not a `Borrow`, which is the fact `science-mir`'s `argument`
    // keys its two-phase path on: a coercion over a borrow reserves a loan
    // there, and this one has no loan under it to reserve.
    assert!(!matches!(body.expr(operand).kind, ExprKind::Borrow { .. }));
}

#[test]
fn a_box_of_a_type_that_implements_nothing_still_does_not_reach_the_object() {
    // §4's obligation through a body. `Untouched` is declared in the same
    // fixture as `Doc` and `Note` and implements nothing, so this is the rule
    // refusing rather than the index being empty.
    let checked = program(&format!(
        "{BOXED_DISPATCH}
def main_line() -> String:
    describe_boxed(Box.new(Untouched(detail: \"x\")))
"
    ));
    assert_eq!(checked.codes(), vec![525]);
}

#[test]
fn a_bare_concrete_type_still_does_not_reach_a_box_of_an_object() {
    // §4's decision keeps its second sentence: `Box.new` is written, and what
    // §4a admits is what `Box.new` produces. `describe_boxed(doc)` is still
    // the line that does not compile, and `assign`'s §5 says it should stay
    // that way so that the allocation is visible in the source.
    let checked = program(&format!(
        "{BOXED_DISPATCH}
def main_line(doc: Doc) -> String:
    describe_boxed(doc)
"
    ));
    assert_eq!(checked.codes(), vec![525]);
}

#[test]
fn a_box_under_a_container_still_does_not_unsize() {
    // §2's exemption is one constructor deep. `examples/08_dyn_dispatch.science`
    // builds its `Array of (Box of any Summarize)` by pushing values that are
    // already objects, which is why this refusal costs the corpus nothing.
    let checked = program(&format!(
        "{BOXED_DISPATCH}
def describe_all(items: &Array[Box[any Summarize]]) -> Bool:
    true

def main_line(docs: &Array[Box[Doc]]) -> Bool:
    describe_all(docs)
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
def named(name: &String) -> Bool:
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
def field_of[T](value: T) -> Bool:
    let _read be value.title
    true
",
    );
    checked.assert_clean();
}

// --- a generic call's type argument, from an argument not yet resolved ---
//
// `BodyChecker::probe`'s Decision 2 default, argued in `check`'s §6.

const IDENTITY: &str = "\
def identity[T](x: T) -> T:
    x
";

#[test]
fn an_unsuffixed_literal_argument_solves_the_call() {
    // `identity(7)`, with no annotation anywhere to lean on. `T` used to have
    // nothing to probe — an unsuffixed literal has no suffix and no declared
    // type — so the call itself was typed `Ty::ERROR`. `probe` now asks
    // Decision 2's default for it, `T := I64`, and the call is typed `I64`
    // rather than `{unknown}`.
    let checked = check(&format!(
        "{IDENTITY}
def main():
    let a be identity(7)
"
    ));
    checked.assert_clean();
    let call = checked.find("main", |kind| matches!(kind, ExprKind::Call { .. }));
    assert_eq!(checked.render(checked.body("main").ty(call)), "I64");
}

#[test]
fn a_name_still_carrying_an_unresolved_literal_solves_the_call_too() {
    // `let n be 7` then `identity(n)`: `n`'s own type is an `InferTy::Var`
    // until the body's writeback runs, so the old probe answered `None` for it
    // exactly as it did for the bare literal above, and for the same reason —
    // there was no `Ty` to hand back yet.
    let checked = check(&format!(
        "{IDENTITY}
def main():
    let n be 7
    let a be identity(n)
"
    ));
    checked.assert_clean();
    let call = checked.find("main", |kind| matches!(kind, ExprKind::Call { .. }));
    assert_eq!(checked.render(checked.body("main").ty(call)), "I64");
}

#[test]
fn an_explicit_annotation_still_solves_the_call_the_old_way() {
    // The case that already worked, kept as the control: an annotation on the
    // binding does not drive `T` at all here — `identity`'s destination type
    // is not the source of truth `instantiate_call` reads, its own argument
    // is — so this must keep resolving to `I64` exactly as it did before the
    // two tests above started passing.
    let checked = check(&format!(
        "{IDENTITY}
def main():
    let a: Int be identity(7)
"
    ));
    checked.assert_clean();
    let call = checked.find("main", |kind| matches!(kind, ExprKind::Call { .. }));
    assert_eq!(checked.render(checked.body("main").ty(call)), "I64");
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

// --- §9: a declaration's parameters, at a pattern -------------------------
//
// `check`'s `scrutinee_substitution`. Each of these is a pair: the program that
// was refused and must now pass, and the neighbouring wrong one that must still
// be refused — because a payload bound at `Ty::ERROR` would make both silent and
// only the first half would notice.

const GENERIC_CHOICE: &str = "\
choice E[L, R]:
    Left(L)
    Right(R)
";

#[test]
fn a_generic_choices_payload_binds_at_the_scrutinees_arguments() {
    // The false positive: `n` used to bind at the declaration's `L`, so
    // returning it where `I64` is written was `SC0525`, *expected `I64`, found
    // `L`*, on a correct program.
    let checked = check(&format!(
        "{GENERIC_CHOICE}
def f(e: E[I64, Bool]) -> I64:
    match e:
        Left(n): n
        Right(_): 0
"
    ));
    checked.assert_clean();
}

#[test]
fn a_generic_choices_payload_is_still_checked_at_those_arguments() {
    // The other half. `Right(b)` is a `Bool` at this instantiation, so
    // returning it as an `I64` is a mistake and the substitution must not have
    // turned the payload into something that agrees with everything.
    let checked = check(&format!(
        "{GENERIC_CHOICE}
def f(e: E[I64, Bool]) -> I64:
    match e:
        Left(n): n
        Right(b): b
"
    ));
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(checked.messages(), vec!["expected `I64`, found `Bool`"]);
}

#[test]
fn a_borrowed_scrutinee_is_read_through_for_its_arguments() {
    // A borrow is transparent to the substitution for `BodyChecker::field`'s
    // reason, and the binding is *not* re-borrowed: `n` is an `I64`, not a
    // `borrowed I64`.
    let checked = check(&format!(
        "{GENERIC_CHOICE}
def f(e: &E[I64, Bool]) -> I64:
    match e:
        Left(n): n
        Right(_): 0
"
    ));
    checked.assert_clean();
}

#[test]
fn a_nested_payload_is_substituted_at_every_depth() {
    // One substitution applied to the whole declared type, so `Array[T]` and
    // `Pair[T, T]` are rewritten by the same fold that rewrites a bare `T`.
    let checked = check(
        "\
type Pair[A, B]:
    left: A
    right: B

choice Wrap[T]:
    One(Array[T])
    Two(Pair[T, T])

def total(w: &Wrap[I64]) -> I64:
    match w:
        One(xs): 0
        Two(p): p.left
",
    );
    checked.assert_clean();
}

#[test]
fn a_nested_payload_is_still_checked_at_every_depth() {
    let checked = check(
        "\
type Pair[A, B]:
    left: A
    right: B

choice Wrap[T]:
    One(Array[T])
    Two(Pair[T, T])

def total(w: &Wrap[Bool]) -> I64:
    match w:
        One(xs): 0
        Two(p): p.left
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(checked.messages(), vec!["expected `I64`, found `Bool`"]);
}

#[test]
fn a_generic_records_field_pattern_binds_at_the_scrutinees_arguments() {
    // The same hole at the other shape. `BodyChecker::field` already
    // substituted for `p.left`; the pattern spelling did not.
    let checked = check(
        "\
type Pair[A, B]:
    left: A
    right: B

def left_of(p: Pair[I64, Bool]) -> I64:
    match p:
        Pair(left: l, right: _): l
",
    );
    checked.assert_clean();
}

#[test]
fn a_generic_records_field_pattern_is_still_checked_at_those_arguments() {
    let checked = check(
        "\
type Pair[A, B]:
    left: A
    right: B

def left_of(p: Pair[I64, Bool]) -> I64:
    match p:
        Pair(left: _, right: r): r
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(checked.messages(), vec!["expected `I64`, found `Bool`"]);
}

#[test]
fn an_uninstantiated_scrutinee_leaves_the_payload_as_declared() {
    // The stated cost: inside a generic function there is nothing to
    // substitute, so the payload is the declaration's `L` and `L` is what the
    // binding has. That is right — it is the same type the body's own `T` is —
    // and this pins that the empty answer is not `Ty::ERROR`.
    let checked = check(
        "\
choice E[L, R]:
    Left(L)
    Right(R)

def first[A, B](e: E[A, B], fallback: A) -> A:
    match e:
        Left(n): n
        Right(_): fallback
",
    );
    checked.assert_clean();
}


// --- an `extern` block's `type Herr is I32` is an alias ---------------------
//
// `alias`' `Aliases::of` collected module-level aliases and nothing else, so an
// extern alias was a `TyKind::Named` with no body: it equalled only itself,
// implemented nothing and had no methods. Nothing asked until
// `implements_operand` asked whether `examples/20_extern.science`'s `opened`
// implements `Ord`. It does — it is an `I32`.

const EXTERN: &str = "\
unsafe extern \"C\" library \"hdf5\":
    type Hid is I64
    type Herr is I32

    def H5open() -> Herr
";

#[test]
fn an_extern_alias_is_revealed_to_the_type_it_names() {
    let checked = check(&format!(
        "{EXTERN}
def opened() -> I32:
    unsafe:
        H5open()
"
    ));
    checked.assert_clean();
}

#[test]
fn an_extern_alias_still_refuses_the_wrong_type() {
    // The other half: revealing `Herr` to `I32` must not make it agree with
    // everything. An `I32` returned where a `String` is written is a mismatch.
    let checked = check(&format!(
        "{EXTERN}
def opened() -> String:
    unsafe:
        H5open()
"
    ));
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(checked.messages(), vec!["expected `String`, found `Herr`"]);
}

#[test]
fn a_comparison_through_an_extern_alias_finds_the_primitives_ord() {
    // `examples/20_extern.science`'s own line, reduced: `if opened < 0:` where
    // `opened` is a `Herr`. Before the alias was collected this was
    // `SC0535`, *`Herr` does not implement `Ord`*, about a type that is an
    // `I32`.
    let checked = check(&format!(
        "{EXTERN}
def failed() -> Bool:
    unsafe:
        let opened be H5open()
        opened < 0
"
    ));
    checked.assert_clean();
}
