//! Bidirectional checking — Decision 1, and the phase that finally walks an
//! expression.
//!
//! > **Decision 1. Type checking is bidirectional, and there is no unifier
//! > across function boundaries.**
//!
//! §5.2 makes every signature fully annotated, so every call site knows its
//! callee's types before it looks at the arguments and every body has a known
//! expected return type. That gives **checking** mode where the type is known
//! and **synthesis** where it is not, and that is the whole algorithm: there is
//! no Hindley-Milner, no generalisation, no let-polymorphism and no principal
//! types.
//!
//! # 1. The two modes, and the one place they meet
//!
//! `BodyChecker::check` takes an expected type and a [`Site`];
//! `BodyChecker::synth` takes neither and returns one. Four forms have a
//! genuine checking rule — an `if`, a `match`, a block and a closure, each of
//! which pushes the expectation *into* its sub-expressions rather than
//! comparing afterwards — and everything else meets the expectation at
//! `BodyChecker::demand`, which is the one place in this file that calls
//! [`assignable`].
//!
//! That is deliberate and it is `lib.rs` §5's obligation discharged in one
//! place: *"`check_expr` and `synth_expr` call `assignable`, and owe it the
//! `Site` at every use. Passing one everywhere removes a decision from the
//! language in silence"*. One call site means one thing to audit.
//!
//! **The order is the seam's order**, and it is not negotiable:
//! [`TypeLowerer::lower`](crate::lowering::TypeLowerer::lower) at the annotation, [`Aliases::reveal`] before the
//! comparison, [`assignable`] with the site. `BodyChecker::coerce` does the
//! last two together so that a caller cannot do the third without the second —
//! which is `lib.rs` §5's *"`Embedding` failing to match `Array of F32` at one
//! site in ten"*.
//!
//! **And the types on THIR nodes are the *written* ones, not the revealed
//! ones.** `alias`'s §1 is the argument: revealing eagerly would make a
//! mismatch report `Array of F32` at a site where the reader wrote `Embedding`,
//! *"and the name they chose disappears from the compiler's vocabulary"*.
//! Revealing happens at the comparison and nowhere else, which is why
//! `BodyChecker::revealed` is called at every point that inspects a type's
//! *structure* and never at a point that stores one.
//!
//! ## 1b. A borrowed expectation stops at a branch
//!
//! **Three of those four forms do not get a `borrowed T` pushed into them at an
//! argument.** §6.3 puts the auto-borrow *"at call sites"*, and the call site of
//! `show(if flag: "yes" else: "no")` is the argument, not the two arms. Pushing
//! `borrowed String` inward made each arm borrow its own temporary, whose
//! storage ends inside the arm, so `science-regions` reported `SC0333` on a
//! correct program — right about the MIR and wrong about the program. The
//! branch is synthesised instead and `BodyChecker::demand` takes one borrow
//! above it; `branches` is the predicate and carries the cost.
//!
//! # 2. Sites, named where they are
//!
//! Decision 14 boxes at a `return` and at an argument. This file passes
//! [`Site::Return`] at a `return` statement and at the trailing expression of
//! the *body* block, [`Site::Argument`] at every call argument, and
//! [`Site::Elsewhere`] at a `let`, an assignment, a field initialiser and a
//! tuple element. `assign`'s §6 says what getting that wrong costs in each
//! direction, and the reason the site is threaded through `BodyChecker::check`
//! rather than inferred from the node is that a trailing expression and a
//! statement's expression are the same node kind.
//!
//! **Decision 14's own motivating example works because of the tuple rule.**
//! `assign`'s §2: `(Doc, MyError)` is *not* assignable to `(Doc, Error?)`,
//! because no conversion there recurses. `return (doc, err)` works because the
//! thing in return position is a tuple **expression**, whose elements this
//! checker visits one at a time — `BodyChecker::check`'s tuple arm — each at
//! [`Site::Return`]. A checker that compared the synthesised tuple type against
//! the signature and stopped would have implemented Decision 14 in a way that
//! rejects the line that motivated it, and `tests/checking.rs` says so by name.
//!
//! # 3. One [`Inference`] per body, and it never escapes
//!
//! `infer`'s §1 makes an inference variable an index into a context *"scoped to
//! one body and discarded when the body is done"*. This file is the only thing
//! that makes one, it makes exactly one per body, and it drops it at the end of
//! [`check_fn`]. Nothing it produces outlives it except a [`Ty`], which belongs
//! to the table.
//!
//! # 4. The writeback, which is what `infer`'s §2 costs
//!
//! [`thir::Expr`] carries a [`Ty`] and not an [`InferTy`], because Decision 3
//! says *a type* on every node and because every consumer of THIR — MIR,
//! `SC0140`, exhaustiveness — wants one. But `1` has no type until the body
//! ends.
//!
//! **Decision. A node whose type is still a variable is pushed with
//! [`Ty::ERROR`] and recorded; when the body ends, Decision 2's defaulting
//! runs, and then every recorded node is rewritten.** Two passes over a list,
//! not a tree.
//!
//! The alternative — an `InferTy` on the node, collapsed by a fold afterwards —
//! is the same work with a worse invariant: every consumer would have to handle
//! a variant that cannot occur in a finished body, which is exactly the
//! *"representation that can hold a type the language does not have"* `ty`'s §4
//! refuses. **What this costs** is that a body inspected before the writeback
//! sees `{unknown}` where a number will be, and that is a hazard for a future
//! pass that wants to run during checking rather than after it.
//!
//! **That hazard arrived and was paid, which is worth recording.** Decision
//! 16's exhaustiveness reads the scrutinee's type off its node, and a
//! `match` over an integer literal has [`Ty::ERROR`] there until the writeback
//! runs — so [`crate::exhaustive`] is the third THIR pass and runs *after*
//! `check_fn` returns rather than inside it, and [`check_crate`] says so. A
//! pass that had run during checking would have been silent on every numeric
//! `match` in the crate and would have looked correct while doing it.
//!
//! **Decision 2's defaulting is here and it needs two prelude ids**, which is
//! `lib.rs` §5's own note: an unconstrained integer literal becomes `I64` and a
//! float `F64`. A variable with no numeric origin that is still unbound is
//! `SC0526`, once per class, because *"a class is one unknown however many
//! expressions joined it"* (`infer`'s §5).
//!
//! # 5. What a literal will agree to
//!
//! **Decision. An unsuffixed integer literal is admitted at any numeric type; a
//! float literal only at a floating one.** `let x: F64 be 1` is what a
//! scientific program writes and refusing it would make the language spell
//! `1.0` in a context where `1` is exact; `let n: I32 be 1.5` is a value the
//! target cannot hold, and admitting it would be Decision 2's *"a scientific
//! language that silently makes `1` an `I32` will be wrong on somebody's index
//! arithmetic"* read backwards.
//!
//! **Decision 2 says a literal is *inferred*, and inferred *among the numeric
//! types*.** It is not a wildcard, and the difference is the whole of this
//! section: *"numeric literals are inferred, not defaulted, within a body, and
//! default to `I64` and `F64` when unconstrained"* is a rule about which
//! number `1` is, and nothing in it licenses `1` being a `Doc`. The variable a
//! literal makes is therefore constrained where it meets a type, and not only
//! defaulted when the body ends; [`BodyChecker::numeric_shape`] is the
//! constraint.
//!
//! **The refusal used to be a four-name list and that was the hole.** The
//! predicate was `Bool`, `String`, `Char`, `Never` and nothing else, so
//! `by_value(42)` at a `value: Doc` parameter checked clean and bound the
//! literal's variable to a record. The four names are still the *prelude* half
//! of the answer — `ffi.CInt` is numeric although §5.1 does not list it, so a
//! builtin name this phase cannot place stays unanswerable — but the question
//! is now asked of the type's **shape** first, and the shapes have answers the
//! name list cannot reach:
//!
//! - **A tuple, a closure, unit, a borrow, an interface object.** A number is
//!   none of them, and no rule in [`crate::assign`] turns a literal into one.
//! - **A name applied to generic arguments** — `Array of I64`, `ffi.Span of
//!   F64`. Every numeric type in the language is a nullary name: §5.1's
//!   primitives are, and `ffi-c-boundary.md` §1.3's C scalars are. A name with
//!   an `of` after it is a container.
//! - **A record or a choice.** This phase holds the declaration and it says
//!   `Doc` has a `title` field, not a value. If `From of Int` ever makes
//!   `Doc(42)` implicit it arrives as a *coercion*, in [`crate::assign`], where
//!   the site is known and §6.2 can count it — not as a literal quietly
//!   unifying with a record.
//!
//! **And a literal reaches a `borrowed T` parameter through §6.3, not by
//! becoming one.** `by_ref(42)` at a `value: borrowed I64` used to bind the
//! literal's variable to `borrowed I64` itself: a literal whose type is a
//! reference, and no [`ExprKind::Borrow`] for MIR to find. The auto-borrow is
//! in `BodyChecker::demand` for the same reason `BodyChecker::coerce` runs it
//! for a value whose type is known — the literal takes the *referent's* type
//! and the borrow the author was told to leave out becomes a node.
//!
//! **What is still admitted is what this phase cannot classify** — a type
//! parameter, `Self`, `Self.Item`, an already-erroneous type, anything in a
//! table with no prelude, and a builtin name that is on neither §5.1's numeric
//! lists nor the four — because refusing on an unanswerable question is how a
//! checker acquires a false positive. `BodyChecker::is_opaque` is the first
//! four, in one place, because `null` asks the same question.
//!
//! **`null` is the same rule in the other direction.** Decision 6 makes `T?` a
//! distinct type *"precisely so that `null` inhabits it and nothing else"*, so
//! `null` has a claim on every nullable type and on no other — and, unlike a
//! number, no default: an unconstrained one is `SC0526` and not a guess. It was
//! refused only at a [`TyKind::Named`], which left it agreeing with a tuple, a
//! closure, unit and an interface object; it is now refused at every shape this
//! phase can classify, which is the same line the numeric half draws.
//!
//! ## 5b. An interface object is the one refusal a default can answer
//!
//! **`show(42)` at a `borrowed any Display` is not a number meeting a
//! non-number.** The classification above is right that a trait object is not a
//! numeric type, and it used to end the argument there. But the question at
//! that slot is *"is there a number that reaches it"*, and Decision 2 already
//! says which one: the literal is an `I64`, `I64 implements Display`, and
//! `assign`'s §4 unsizes `borrowed I64` into `borrowed any Display`. So the
//! default is taken at the slot and the ordinary coercion runs.
//! `BodyChecker::literal_unsizes` is the rule, and it refuses an exclusive
//! borrow, an owned object and a default the interface is not implemented by —
//! the three cases where there would be no coercion to compose with.
//!
//! # 6. The holes, each priced
//!
//! These are real and they are stated rather than hidden. Every one of them
//! produces [`Ty::ERROR`] and **no diagnostic**, which is `ty`'s §5: an
//! erroneous type agrees with whatever it meets, so a hole costs nothing
//! downstream and, in particular, cannot manufacture a cascade.
//!
//! - **Method calls on a receiver this crate holds no implementations for.**
//!   The lookup exists — [`crate::methods`], and `BodyChecker::method_call` is
//!   the call site — so `doc.describe()` resolves, its arguments are checked
//!   against real parameters, and the call has the method's return type. So
//!   does `text.length()`: `builtins.rs` declares a prelude surface now, and
//!   `methods`' §8 is what changed. What is left is a **name the prelude has
//!   not transcribed** — `text.slice(0..4)` — and a **type parameter**, because
//!   a method reached through a bound is generic in a way monomorphisation has
//!   to resolve (`methods`'s §5). Both leave `method: None` and [`Ty::ERROR`],
//!   and neither reports.
//!
//!   **One case that used to be here is not any more**, and it is the one
//!   whose price this list understated: several implementations of one
//!   interface, at different type arguments, reached by one name. That was
//!   `method: None` and no diagnostic — an unchecked call rather than a hole in
//!   a type — and `methods`'s §6 makes the arguments choose between them.
//!   `BodyChecker::select` is the selection and it says what it costs Decision
//!   1.
//! - **Operators on user types: closed, except for `Ord`'s dispatch.** `a + b`
//!   on a record is `Add.add` and `a[i]` is `Index.index`; `builtins.rs` now
//!   declares `Index of Idx` with its `type Output` and its method, and
//!   `IndexMutably` beside it, so `a[i]` has a type and `a[i] be v` is checked.
//!   `is` requires `Eq` and `< > <= >=` require `Ord`, both through
//!   `BodyChecker::implements_operand`, which asks whether the implementation
//!   exists and calls nothing.
//!
//!   **What is left is one row of `binary_operator`.** `Ord`'s *dispatch* needs
//!   a method name, an `Ordering` return type that is not in §8's closed
//!   library, and a rule for how four operators sit over one `compare` —
//!   including what `F64`'s NaN does to a total order. Nothing here invents
//!   any of the three; `implements_operand` is where the line between
//!   requiring an implementation and calling into one is argued, and
//!   `builtins.rs`' `INTERFACE_DECLS` states the same refusal at the
//!   declaration. `Display` keeps its whole hole for the same reason, one type
//!   along: its method would name a `Formatter`.
//!
//!   Operators on the prelude's numeric primitives *are* checked structurally,
//!   because those do not go through an implementation.
//! - **A generic call's type arguments.** Explicit ones are used. An omitted
//!   one is solved only where a parameter's type is the generic parameter
//!   itself or a borrow of it — `BodyChecker::root_param`, which is the
//!   root-level match `infer`'s §2 admits plus the one indirection §6.3 makes
//!   invisible at the call. Anything deeper — `xs: Array of T` against an
//!   `Array of Int` — leaves `T` unsolved, and an unsolved parameter becomes
//!   [`Ty::ERROR`] so that the arguments are still checked against something
//!   that agrees. The general answer needs the nested representation `infer`'s
//!   §2 describes and does not build.
//! - **`Iterate`, and therefore `for`, is narrowed rather than closed.** The
//!   prelude declares `interface Iterate:` with `type Item` and
//!   `def next(mutable self) -> Self.Item?`, and `Chars implements Iterate:`
//!   answers `Item` with `Char`, so `for c in text.chars()` binds `c` at
//!   `Char`. `BodyChecker::iterate_item` is the reading.
//!
//!   **What does not close is `for x in xs` over an `Array` or a `Map`**, and
//!   that is a language decision rather than a transcription: whether
//!   `Array of T`'s `Item` is `T` or `borrowed T` decides whether every `for`
//!   loop in the language copies its element, and no note states it —
//!   `collections-and-chains.md` §5.4 asks every collection for three
//!   `iterate*` methods without saying which one `for` desugars to. Those
//!   loops still bind at [`Ty::ERROR`]. Decision 15's `TryIterate` does not
//!   exist at all, which is why `SC0521` is still unclaimed by this crate.
//! - **A `loop`'s value.** `break e` is checked and its type discarded; a
//!   `loop` is [`Ty::UNIT`].
//! - **Arity and kind of generic arguments**, which `lowering`'s §1 deferred to
//!   *"whoever holds the declaration and the use at once"*. This file holds
//!   both and still does not check it: the check wants `hir::GenericArity` and a
//!   message about variadic const parameters, and it is one diagnostic that
//!   belongs with the monomorphiser's, not four lines here.
//! - **A record literal that omits a field.** The resolver reports an unknown
//!   field (`SC0205`); a *missing* one is nobody's yet.
//!
//! # 7. Two rules the note does not name, and where they came from
//!
//! `type-checking-and-mir.md` §14 says the language has *"no subtyping, apart
//! from the two implicit coercions of Decisions 6 and 14"*, and that is true of
//! *coercions*. It is not the whole of what a call site does, and both of the
//! following came from running this checker over `examples/` and finding it
//! reporting on code the spec says is correct.
//!
//! **Auto-borrow at call sites** is §6.3 of the core spec — *"if a parameter is
//! declared `borrowed T`, the caller writes `compare(a, b)"* — and the note
//! does not mention it at all. It is an elaboration rather than a coercion and
//! `BodyChecker::auto_borrow` is the argument for why that distinction is the
//! one that decides where it lives. Without it, 60% of every diagnostic this
//! checker produced over the corpus was a borrow the language had told the
//! author not to write.
//!
//! **A comparison is compared through a borrow**, for the same reason one step
//! further on: §5.4 makes `is` and `==` one operator dispatching to `Eq`, whose
//! method takes `borrowed self`, so `name is ""` has a `borrowed String` and a
//! `String` in the source and two `String`s in the call.
//! `BodyChecker::compare` says what that can and cannot conclude.
//!
//! **What the first of them composes with** is `assign`'s §4 unsizing, and
//! that composition is `describe_any(doc)` in
//! `examples/08_dyn_dispatch.science`. `Doc` reaches `borrowed any Summarize`
//! in two steps that are each decided elsewhere: §6.3 says the borrow may be
//! taken, and §4 says a `borrowed Doc` unsizes into a `borrowed any Summarize`
//! because doing so allocates nothing and changes no value. What does *not*
//! happen here is the owning form — `Doc` into `Box of any Summarize` — which
//! `assign`'s §5 still refuses, and which that file still writes out as
//! `Box.new(Doc(..))`. `BodyChecker::auto_borrow` asks the relation rather than
//! deciding for itself, so this file knows nothing about interfaces.
//!
//! # 8. A bound is checked where the argument is
//!
//! **Decision. At a call to a generic callee, every interface bound on a
//! generic parameter that call solved is checked, and an unsatisfied one is
//! `SC0534`.** `BodyChecker::check_bounds` is the check, it runs at both call
//! forms — `BodyChecker::call_signature` and `BodyChecker::call_method` — and
//! it runs immediately after `BodyChecker::instantiate_call`, which is the one
//! place a callee's generics are solved.
//!
//! The bound was declared, parsed, resolved, carried into
//! [`Signature::bounds`](crate::items::Signature::bounds) — and then nothing
//! read it. `describe(n)` at an `I64` checked clean against `def describe of T:
//! Summarize(..)`, which is not a missing feature but a missing *diagnostic*:
//! the body of `describe` was checked on the strength of the bound, so the
//! promise was being spent and never collected. It is also what made `T` agree
//! with anything, since nothing else constrains a solved parameter at all.
//!
//! **It is asked of [`Methods::implements`], which `assign`'s §3 and §4 already
//! ask**, and it is asked under one restraint those two do not need:
//! `methods`'s §7. They name one interface each and the answer for it is
//! whatever the crate wrote; a bound names whatever interface the *author*
//! wrote, and seventeen of them come from a prelude that declares no
//! implementations of any. So a bound at a **builtin** interface — `T: Ord`,
//! `T: Clone`, `T: Eq` — is unanswerable and admitted, exactly as a method on a
//! `String` receiver is, and a bound at a **user** interface is answered in
//! full. That is `assign`'s §3 discipline — *"refusing on an unanswerable
//! question is how a checker acquires a false positive"* — applied to the half
//! of the question this phase can see.
//!
//! **Three things it deliberately does not reach**, each stated rather than
//! hidden:
//!
//! - **A parameter this call did not solve.** §6 leaves an unsolved one
//!   [`Ty::ERROR`], and `ty`'s §5 makes that agree with whatever it meets, so
//!   the bound is skipped rather than reported against a hole. `largest(items)`
//!   inside `rank of T: Ord` is the corpus case: the parameter is `items:
//!   borrowed Array of T`, which is deeper than a root-level match, so `T` is
//!   unsolved and nothing is claimed about it.
//! - **A record literal and a variant's payload.** Both instantiate a *type's*
//!   generics — `BodyChecker::instantiate_record` and
//!   `BodyChecker::instantiate_payload` — and neither is checked here, because
//!   [`Record`](crate::items::Record) and [`Variant`](crate::items::Variant)
//!   carry the generic parameters and not the `where` clause beside them, and a
//!   check that saw one spelling of a bound and not the other would enforce a
//!   rule that depends on where the author put it. That is the same argument
//!   [`ParamBound`](crate::items::ParamBound) makes for reading both, one
//!   declaration kind over.
//! - **An implementation block's own generic parameters.** `Wrapper of T has:`
//!   declares them and `BodyChecker::block_substitution` solves them from the
//!   receiver, and a bound on one is in `hir::Impl`'s generics rather than in
//!   any [`Signature`](crate::items::Signature). It is the record literal's
//!   case again with a different declaration kind, and it closes the same way.
//! - **A const generic parameter's kind.** §5.3's `const N: Int` carries a
//!   *kind* rather than a bound, and nothing checks it at a call: §6's
//!   penultimate bullet already prices the generic-argument arity and kind
//!   check, and the kind of a const argument is that check rather than this
//!   one.

use std::collections::{HashMap, HashSet};

use science_diagnostics::{Diagnostic, Diagnostics, Label, Span};
use science_lexer::NumSuffix;
use science_resolve::hir::{self, BinaryOp, DefId, DefTable, Literal, Res, SelfKind, UnaryOp};

use crate::alias::Aliases;
use crate::assign::{assignable, Coercion, Coercions, Site};
use crate::codes;
use crate::diagnostics;
use crate::infer::{InferTy, InferVar, Inference};
use crate::items::{named, Declarations, Named, Signature};
use crate::methods::{Candidate, Form, Found};
use crate::narrow::{self, Fact, Facts};
use crate::normal::AtomOrder;
use crate::subst::Substitution;
use crate::thir::{
    self, Block, Body, ExprId, ExprKind, FStringPart, PatId, PatKind, Stmt, StmtKind,
};
use crate::ty::{GenericArg, Ty, TyKind, Types};

/// Checks every body in the crate, and runs the THIR analyses over each.
///
/// The order is `type-checking-and-mir.md` §12's, as far as this crate goes:
/// declarations, then bodies, then the THIR passes. `SC0140` runs per body
/// because it is a per-body question; narrowing runs *during* the body because
/// its answer is a type.
///
/// **Decision 16's exhaustiveness is the third THIR pass and it runs last**,
/// after the writeback of §4. It has to: a `match` whose scrutinee is still an
/// inference variable carries [`Ty::ERROR`] on its node until the writeback
/// runs, and [`crate::exhaustive`]'s §5 refuses to answer about an erroneous
/// type — so a pass that ran during the body would be silent on every `match`
/// over a numeric literal in the crate.
pub fn check_crate(
    krate: &hir::Crate,
    decls: &Declarations,
    types: &mut Types,
    aliases: &mut Aliases,
    order: &AtomOrder,
    diagnostics: &mut Diagnostics,
) -> Vec<Body> {
    // Before the bodies, and that is deliberate: a block that does not conform
    // is wrong before anything calls it, and an author told *"`render` returns
    // `Int` and `Render` declares `String`"* would rather have that than a page
    // of diagnostics about what the wrong return type does inside the body.
    // `conform`'s own head comment is the decision; this is its one call site.
    crate::conform::report(krate, decls, types, diagnostics);
    let mut bodies = Vec::new();
    for module in &krate.modules {
        for item in &module.items {
            collect(&item.kind, &mut |function, owner| {
                let body =
                    check_fn(function, owner, krate, decls, types, aliases, order, diagnostics);
                crate::unchecked::report(&body, krate, decls, types, diagnostics);
                crate::exhaustive::report(&body, krate, decls, types, aliases, diagnostics);
                bodies.push(body);
            });
        }
    }
    bodies
}

/// Walks the functions with bodies in one item, with the block that owns each.
fn collect(kind: &hir::ItemKind, visit: &mut impl FnMut(&hir::Fn, Option<DefId>)) {
    match kind {
        hir::ItemKind::Fn(function) if function.body.is_some() => visit(function, None),
        hir::ItemKind::Impl(block) => {
            for method in block.methods.iter().filter(|m| m.body.is_some()) {
                visit(method, Some(block.def));
            }
        }
        hir::ItemKind::Interface(interface) => {
            for method in interface.methods.iter().filter(|m| m.body.is_some()) {
                visit(method, Some(interface.def));
            }
        }
        _ => {}
    }
}

/// Checks one body and hands back its THIR.
#[allow(clippy::too_many_arguments)]
pub fn check_fn(
    function: &hir::Fn,
    owner: Option<DefId>,
    krate: &hir::Crate,
    decls: &Declarations,
    types: &mut Types,
    aliases: &mut Aliases,
    order: &AtomOrder,
    diagnostics: &mut Diagnostics,
) -> Body {
    let coercions = Coercions::of(&krate.defs);
    // `Self` and the block's associated types are substituted before anything
    // in the body is compared, because both are holes `subst`'s §2 leaves
    // standing and a body is the place that can fill them.
    let self_subst = match owner {
        Some(owner) => decls.body_substitution(&krate.defs, owner),
        None => Substitution::new(),
    };

    let signature = decls.signature(function.def);
    let ret = signature.map(|sig| sig.ret).unwrap_or(Ty::UNIT);

    let checker = BodyChecker {
        defs: &krate.defs,
        decls,
        order,
        coercions,
        types,
        aliases,
        diagnostics,
        infer: Inference::new(),
        body: Body::new(function.def, ret),
        locals: Vec::new(),
        bound_as: Vec::new(),
        assignments: Vec::new(),
        facts: Facts::new(),
        pending: Vec::new(),
        numeric: Vec::new(),
        self_subst,
        ret,
        diverged: false,
        breaks: Vec::new(),
    };
    checker.run(function, signature)
}

/// What [`BodyChecker::numeric_shape`] can say about a type a number is meeting.
///
/// The third variant is the one §5 is about: a checker with two answers refuses
/// wherever it cannot admit, or admits wherever it cannot refuse, and this
/// phase needs to do neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// One of §5.1's integer primitives.
    Integer,
    /// One of §5.1's floating primitives.
    Float,
    /// A type no number has, and this phase is sure.
    NotANumber,
    /// This phase cannot classify it. §5 admits.
    Unanswerable,
}

/// Where an unsuffixed literal's type comes from when nothing constrains it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Numeric {
    Integer,
    Float,
    /// `null`, which has no default at all: there is no type it means on its
    /// own, so an unconstrained one is `SC0526` and not a guess.
    Null,
}

/// A synthesised expression: the node, and what is known of its type.
#[derive(Debug, Clone, Copy)]
struct Typed {
    id: ExprId,
    ty: InferTy,
}

/// What the lookup at a call found: a callee, or none.
///
/// **It carries the arguments when they have already been synthesised**, which
/// is `methods`'s §6 leaking exactly one fact into the shape of the answer:
/// selection has to look at the argument types before it knows the callee, so
/// at those calls the argument nodes exist before the lookup returns and every
/// path afterwards — the resolved one and the two failed ones — has to use
/// those nodes rather than build a second set. `None` is every other call,
/// where nothing has been synthesised yet.
enum Callee {
    Found(Candidate, Option<Vec<Typed>>),
    Missing(Option<Vec<Typed>>),
}

/// Which half of `indexing-and-array-literals.md` §1.1's Decision 2 an `a[i]`
/// is, which is decided by the position and never by the type.
///
/// **A bracket on the left of a `be` is a write and a bracket anywhere else is
/// a read.** That is the whole rule, and it is the parser's shape rather than
/// an inference: §1.1 makes `a[i] be v` the syntax §6.5 promised, and a
/// container that admits the read and not the write is the case Decision 2's
/// two interfaces exist to tell apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Indexing {
    /// `a[i]` — `Index.index`.
    Read,
    /// `a[i] be v` — `IndexMutably.index_mutably`.
    Write,
}

impl Indexing {
    /// The interface this dispatches to and the method's name on it.
    ///
    /// The name is §4.5's Decision 4c one more time — *"an operator trait's
    /// method takes the trait's lowercase name only when that name is free"* —
    /// and it is free for both, so `Index` is `index` and `IndexMutably` is
    /// `index_mutably`. Unlike the eight in `OPERATORS`, the *types* are
    /// written down too, because §1.1 writes them: see `builtins.rs`'
    /// `INTERFACE_DECLS`.
    fn dispatch(self) -> (&'static str, &'static str) {
        match self {
            Indexing::Read => ("Index", "index"),
            Indexing::Write => ("IndexMutably", "index_mutably"),
        }
    }
}

/// One candidate of `methods`'s §6, with the parameter types selection
/// compares the arguments against.
///
/// The parameters are substituted — `Self` and the block's own generics — for
/// the reason `BodyChecker::call_method` gives for substituting them there: a
/// method's signature is written inside a block, and reading it raw compares
/// against a `Self` rather than against a type a value can have.
#[derive(Clone)]
struct Instance {
    candidate: Candidate,
    params: Vec<(DefId, Ty)>,
}

/// The checker for one body. §3: one of these per body, dropped with it.
/// `SC0304` — a write to a binding that was never declared `mutable`.
///
/// **Borrowed from the ownership band, not claimed from this crate's.**
/// `science-mir` and `science-regions` both record that `SC0303`-`SC0329` is
/// free, and this takes the first of them. It is not in
/// [`crate::codes::ALL`] for the same reason
/// [`crate::unchecked::UNCHECKED_ERROR`] is not: that list is the codes this
/// crate owns a *band* for, and the test beside it would rightly refuse a
/// `3xx`.
///
/// **Why the type checker reports an ownership-band code.** The rule needs one
/// fact — the word at the declaration — and no flow analysis whatsoever, so it
/// is decided wherever the binding and the write are both in hand. That is
/// this phase. Deferring it to `science-mir` would buy nothing and cost a
/// diagnostic on programs that never reach MIR because they failed to type.
pub const NOT_MUTABLE: science_diagnostics::Code = science_diagnostics::Code(304);

/// How a binding was introduced, which decides whether `x be v` may write to
/// it — and, when it may not, what the fix is.
///
/// **Decision. A binding is immutable unless it says otherwise**, which is
/// revision 2 §2.2's own spelling: it writes `let mutable i be 0` before
/// `i be i + 1`, and that word would mean nothing if the plain `let` also
/// admitted the write. The rule is Rust's, arrived at from the same direction:
/// the common case is a name given a value once, so the common case is the one
/// that needs no keyword, and the rarer reassignment is the one that announces
/// itself at the declaration where a reader is looking.
///
/// **The cost, stated.** A counting loop written the old way needs the word,
/// and a parameter cannot be given it at all — there is nowhere in
/// `name: Type` to put it, so a body that wants to reassign an argument binds
/// a local from it. That is one extra line, and it buys a signature whose
/// parameters mean the same thing on the last line of the body as on the
/// first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BoundAs {
    /// `let mutable x`, `mutable self`, or a pattern binding written
    /// `mutable`. Assignable.
    Mutable,
    /// `let x be v`. The fix is one word at the `let`.
    Let,
    /// Bound by a `match`, `for` or closure pattern. The same word applies,
    /// but at the pattern rather than at a `let`.
    Pattern,
    /// A parameter, or a `self` that is not `mutable self`. There is no word
    /// to add, so the fix is a local.
    Parameter,
}

struct BodyChecker<'a> {
    defs: &'a DefTable,
    decls: &'a Declarations,
    order: &'a AtomOrder,
    coercions: Coercions,
    types: &'a mut Types,
    aliases: &'a mut Aliases,
    diagnostics: &'a mut Diagnostics,
    infer: Inference,
    body: Body,
    /// The bindings in scope, with the type each was given. A `Vec` for the
    /// reason `Body::locals` is one.
    locals: Vec<(DefId, InferTy)>,
    /// How each binding in scope was introduced, which is what decides whether
    /// it may be assigned to and what the fix is when it may not. Parallel to
    /// `locals` rather than folded into it because only assignment reads it.
    bound_as: Vec<(DefId, BoundAs)>,
    /// Every assignment target, held until §4's writeback.
    ///
    /// The rule has to read types, and until writeback an unresolved variable
    /// *is* [`Ty::ERROR`] in the body — `let count be 0` and a genuinely
    /// failed lookup are the same value. Running the rule while that is true
    /// means either missing `count be 3` or inventing an error on top of
    /// somebody else's, so it runs once the variables have been resolved.
    assignments: Vec<ExprId>,
    facts: Facts,
    /// Nodes whose type is still a variable. §4.
    pending: Vec<(ExprId, InferVar)>,
    numeric: Vec<(InferVar, Numeric)>,
    self_subst: Substitution,
    ret: Ty,
    /// Whether control has already left on this path.
    diverged: bool,
    /// One entry per enclosing `loop`, true once it has seen a `break`.
    breaks: Vec<bool>,
}

impl<'a> BodyChecker<'a> {
    fn run(mut self, function: &hir::Fn, signature: Option<&Signature>) -> Body {
        // The root block takes index zero, before anything inside it exists,
        // which is what makes `Body::root` a constant.
        let body_block = function.body.as_ref().expect("check_fn is given a function with a body");
        let root = self.body.reserve_block(body_block.span);
        // The return type is the signature's with `Self` and the block's
        // associated types replaced. Reading it raw is the bug that says
        // `expected Self, found ConfigError` at a `-> Self` that is right.
        let ret = self.instantiate(self.ret, function.span);
        self.ret = ret;
        self.body.ret = ret;

        if let Some(sig) = signature {
            if let Some((def, kind)) = sig.self_param {
                let owner = sig.owner.and_then(|owner| self.decls.self_ty(owner));
                let mut ty = owner.unwrap_or(Ty::ERROR);
                ty = match kind {
                    SelfKind::Value => ty,
                    SelfKind::Shared => self.types.borrowed(false, ty),
                    SelfKind::Mutable => self.types.borrowed(true, ty),
                };
                // `mutable self` is the receiver that may be written through;
                // the other two spellings are the ones that promise not to.
                let bound = match kind {
                    SelfKind::Mutable => BoundAs::Mutable,
                    SelfKind::Value | SelfKind::Shared => BoundAs::Parameter,
                };
                self.bind_local(def, InferTy::Known(ty), bound);
            }
            for param in &sig.params {
                let ty = self.instantiate(param.ty, param.span);
                self.bind_local(param.def, InferTy::Known(ty), BoundAs::Parameter);
            }
        }

        let ret = self.ret;
        let block = self.block(body_block, Some((ret, Site::Return)));
        self.body.fill_block(root, block);
        self.body.diverges = self.diverged;
        self.finish()
    }

    /// Decision 2's defaulting, then §4's writeback.
    fn finish(mut self) -> Body {
        // A class's numeric origin is a property of the class, not of the
        // variable that happened to be created first, so the kinds are folded
        // onto roots before anything is defaulted.
        let mut kinds: HashMap<InferVar, Numeric> = HashMap::new();
        for (var, kind) in std::mem::take(&mut self.numeric) {
            let root = self.infer.find(var);
            kinds
                .entry(root)
                .and_modify(|existing| *existing = widen_numeric(*existing, kind))
                .or_insert(kind);
        }

        for root in self.infer.unresolved() {
            let default = match kinds.get(&root) {
                Some(Numeric::Integer) => self.decls.prelude().default_int(self.types),
                Some(Numeric::Float) => self.decls.prelude().default_float(self.types),
                Some(Numeric::Null) | None => None,
            };
            match default {
                Some(ty) => {
                    let _ = self.infer.bind(self.types, root, ty);
                }
                None => {
                    let span = self.infer.origin(root);
                    self.diagnostics.push(cannot_infer(span));
                }
            }
        }

        for (id, var) in std::mem::take(&mut self.pending) {
            if let Some(ty) = self.infer.binding(var) {
                self.body.set_ty(id, ty);
            }
        }
        let locals = std::mem::take(&mut self.locals);
        for (def, ty) in locals {
            if let InferTy::Var(var) = ty {
                if let Some(ty) = self.infer.binding(var) {
                    self.body.set_local_ty(def, ty);
                }
            }
        }

        // Every type in the body is now the type it will be, which is what
        // the mutability rule needs and could not have had earlier.
        for target in std::mem::take(&mut self.assignments) {
            self.check_assignable(target);
        }
        self.body
    }

    // --- the seam's three calls ------------------------------------------

    /// The second of `lib.rs` §5's three calls, with its overflow reported.
    fn revealed(&mut self, ty: Ty, span: Span) -> Ty {
        match self.aliases.reveal(self.types, ty) {
            Ok(revealed) => revealed,
            Err(error) => {
                self.diagnostics.push(diagnostics::overflowed(error, span));
                Ty::ERROR
            }
        }
    }

    /// The first: an annotation inside a body.
    fn lower_ty(&mut self, ty: &hir::Type) -> Ty {
        let lowered = crate::lowering::TypeLowerer::new(
            self.types,
            self.defs,
            self.order,
            self.diagnostics,
        )
        .lower(ty);
        self.instantiate(lowered, ty.span)
    }

    /// One generic argument at a use, which may be a type or a const.
    ///
    /// [`TypeLowerer::lower_arg`] is the one function that can tell them apart,
    /// because the answer is the *kind of the parameter* and not the shape of
    /// what was written.
    fn lower_generic_arg(&mut self, ty: &hir::Type) -> GenericArg {
        let arg = crate::lowering::TypeLowerer::new(
            self.types,
            self.defs,
            self.order,
            self.diagnostics,
        )
        .lower_arg(ty);
        match arg {
            GenericArg::Type(lowered) => GenericArg::Type(self.instantiate(lowered, ty.span)),
            other => other,
        }
    }

    /// `Self` replaced by the block's self type.
    fn instantiate(&mut self, ty: Ty, span: Span) -> Ty {
        if self.self_subst.is_empty() {
            return ty;
        }
        match self.self_subst.apply(self.types, ty) {
            Ok(applied) => applied,
            Err(error) => {
                self.diagnostics.push(diagnostics::overflowed(error, span));
                Ty::ERROR
            }
        }
    }

    /// The third, and the only call to it in this file. §1.
    fn coerce(&mut self, expr: ExprId, from: Ty, to: Ty, site: Site, span: Span) -> ExprId {
        let source = self.revealed(from, span);
        let target = self.revealed(to, span);
        match assignable(self.types, self.decls.methods(), self.coercions, site, source, target) {
            Some(Coercion::Identity) => expr,
            Some(coercion) => {
                self.body.push_expr(ExprKind::Coerce { operand: expr, coercion }, to, span)
            }
            None => match self.auto_borrow(expr, source, to, target, site, span) {
                Some(borrowed) => borrowed,
                None => {
                    self.mismatch(from, to, span);
                    // The node stays. Decision 3 makes THIR the tree a
                    // diagnostic quotes, and replacing a mistyped expression
                    // with a hole throws away the structure the next message
                    // would have needed.
                    expr
                }
            },
        }
    }

    /// §6.3 of the core spec: **auto-borrow at call sites**.
    ///
    /// > *"If a parameter is declared `borrowed T`, the caller writes
    /// > `compare(a, b)`, not `compare(borrowed a, borrowed b)`. … An explicit
    /// > borrow remains legal where it clarifies."*
    ///
    /// **Decision. This is an elaboration and not a coercion, so it is here and
    /// not in [`crate::assign`].** That module's §4 refuses *"anything about
    /// regions, mutability or variance"* and it is right to: a coercion is a
    /// relation between two types, and whether a borrow may be taken is a
    /// question about a *place* and a region, which nothing in the type table
    /// knows. What happens here is the other thing — the checker writing down
    /// the `borrowed` the author was allowed to leave out, as an ordinary
    /// [`ExprKind::Borrow`] node, which is Decision 3's third clause. MIR sees
    /// the same tree whether or not the author typed the word, and the region
    /// engine gets a borrow at a point rather than a coercion it has no lattice
    /// for.
    ///
    /// **It applies at [`Site::Argument`] and nowhere else**, because §6.3 says
    /// *"at call sites"*. A `let x: borrowed String be s` is not one, and
    /// admitting it there would be a language change made by a checker.
    ///
    /// **What the borrow reaches is [`assignable`]'s question, asked again.**
    /// This method decides only that a borrow may be *taken* — the target is a
    /// borrow and the site is a call — and then hands the borrowed type back to
    /// the relation. `Doc` reaches `borrowed Doc` because `borrowed Doc` is
    /// compatible with it, and `Doc` reaches `borrowed any Summarize` because
    /// `borrowed Doc` unsizes into it: `assign`'s §4. The composition is
    /// deliberate and is what `describe_any(doc)` in
    /// `examples/08_dyn_dispatch.science` is — one elaboration and one
    /// coercion, each decided where it lives, rather than a third rule here
    /// that knows about interfaces.
    ///
    /// **Both nodes are emitted.** The borrow the author did not write is an
    /// [`ExprKind::Borrow`] typed `borrowed Doc`, and the unsizing is an
    /// [`ExprKind::Coerce`] above it typed as the parameter was written. A
    /// single node carrying both would hide from MIR the place where the borrow
    /// is taken, which is the point the region engine needs.
    ///
    /// **And an exclusive auto-borrow invalidates the narrowing**, exactly as a
    /// written one does — `narrow`'s §4 and Decision 8. It is the same event
    /// and the author not having typed it changes nothing about who may write
    /// through it.
    fn auto_borrow(
        &mut self,
        expr: ExprId,
        source: Ty,
        written_target: Ty,
        target: Ty,
        site: Site,
        span: Span,
    ) -> Option<ExprId> {
        if site != Site::Argument {
            return None;
        }
        let TyKind::Borrowed { mutable, .. } = *self.types.kind(target) else {
            return None;
        };
        // `target` arrived revealed, so this is the revealed borrowed source
        // and the relation's precondition holds on both sides.
        let borrowed = self.types.borrowed(mutable, source);
        // The target is a borrow, so the only verdicts reachable are the two
        // below: neither Decision 6's nor Decision 14's rule has a borrow on
        // its right.
        let coercion =
            assignable(self.types, self.decls.methods(), self.coercions, site, borrowed, target)?;
        if mutable {
            if let Some(place) = self.body.place_of(expr) {
                self.facts.invalidate(&place);
            }
        }
        // The borrow alone is typed as the parameter was written, so that a
        // diagnostic downstream quotes the author's spelling of the alias. The
        // borrow under a coercion is typed by what it actually is.
        let borrow_ty = match coercion {
            Coercion::Identity => written_target,
            _ => borrowed,
        };
        let taken =
            self.body.push_expr(ExprKind::Borrow { mutable, operand: expr }, borrow_ty, span);
        Some(match coercion {
            Coercion::Identity => taken,
            coercion => self.body.push_expr(
                ExprKind::Coerce { operand: taken, coercion },
                written_target,
                span,
            ),
        })
    }

    fn mismatch(&mut self, found: Ty, expected: Ty, span: Span) {
        let found = self.types.render(self.defs, found);
        let expected = self.types.render(self.defs, expected);
        self.diagnostics.push(mismatched_types(span, &expected, &found));
    }

    // --- modes ------------------------------------------------------------

    /// Checking mode: the type is known, so push it inward. §1.
    fn check(&mut self, expr: &hir::Expr, expected: Ty, site: Site) -> ExprId {
        // §1b. §6.3's auto-borrow is taken **at the argument**, so a borrowed
        // parameter stops the expectation at a branch rather than pushing it
        // into the arms.
        if site == Site::Argument && branches(expr) {
            let target = self.revealed(expected, expr.span);
            if matches!(self.types.kind(target), TyKind::Borrowed { .. }) {
                let typed = self.synth(expr);
                return self.demand(typed, expected, site, expr.span);
            }
        }
        match &expr.kind {
            hir::ExprKind::If(if_expr) => {
                self.if_expr(if_expr, expr.span, Some((expected, site))).id
            }
            // An `unsafe` block is a block. What the keyword changes is which
            // operations the body may name, and that is not this mode's
            // question — so an expectation crosses it exactly as it crosses a
            // plain block. Leaving it out of this arm is not a lost
            // optimisation: the tail gets synthesised instead, and a `null` or
            // a bare integer in it then has nothing to take its type from.
            // `examples/20_extern.science`'s `native_double_type` is the case,
            // whose whole body is one `unsafe` block ending in
            // `(H5T_NATIVE_DOUBLE_g, null)`.
            hir::ExprKind::Block(block) | hir::ExprKind::Unsafe(block) => {
                let id = self.body.reserve_block(block.span);
                let filled = self.block(block, Some((expected, site)));
                let ty = filled.tail.map(|tail| self.body.ty(tail)).unwrap_or(Ty::UNIT);
                self.body.fill_block(id, filled);
                let kind = if matches!(expr.kind, hir::ExprKind::Unsafe(_)) {
                    ExprKind::Unsafe(id)
                } else {
                    ExprKind::Block(id)
                };
                self.body.push_expr(kind, ty, expr.span)
            }
            hir::ExprKind::Match(match_expr) => {
                self.match_expr(match_expr, expr.span, Some((expected, site))).id
            }
            hir::ExprKind::Closure { param, body } => {
                self.closure(*param, body, expr.span, Some(expected)).id
            }
            // Decision 11's bidirectional push, and the whole of it: an
            // expected `Array of E` drives `E` into every element, an empty
            // literal takes `E` and reports nothing, and anything else falls
            // through to synthesis — where the literal gets its own type and
            // `demand` reports the mismatch against the annotation once.
            // §3.3 prices this as *"a bounded amount of bidirectional
            // checking — an expected type pushed into one expression form"*,
            // and this arm is that bound.
            hir::ExprKind::ArrayLit(elements) => {
                let revealed = self.revealed(expected, expr.span);
                if let Some(element_ty) = self.array_element(revealed) {
                    return self.array_lit_expecting(elements, element_ty, expected, expr.span);
                }
                let typed = self.synth(expr);
                self.demand(typed, expected, site, expr.span)
            }
            hir::ExprKind::Tuple(elements) => {
                // §2: the elements are visited one at a time, at this site,
                // which is what makes Decision 14's own example compile.
                let revealed = self.revealed(expected, expr.span);
                if let TyKind::Tuple(expected_elements) = self.types.kind(revealed).clone() {
                    if expected_elements.len() == elements.len() {
                        let ids: Vec<ExprId> = elements
                            .iter()
                            .zip(expected_elements)
                            .map(|(element, want)| self.check(element, want, site))
                            .collect();
                        return self.body.push_expr(ExprKind::Tuple(ids), expected, expr.span);
                    }
                }
                let typed = self.synth(expr);
                self.demand(typed, expected, site, expr.span)
            }
            _ => {
                let typed = self.synth(expr);
                self.demand(typed, expected, site, expr.span)
            }
        }
    }

    /// The one place [`assignable`] is reached from. §1.
    fn demand(&mut self, typed: Typed, expected: Ty, site: Site, span: Span) -> ExprId {
        match typed.ty {
            InferTy::Known(found) => self.coerce(typed.id, found, expected, site, span),
            InferTy::Var(var) => {
                let target = self.revealed(expected, span);
                // Decision 2's default, run here rather than at
                // [`Self::finish`]. §5b.
                if site == Site::Argument {
                    if let Some(defaulted) = self.literal_unsizes(var, target, span) {
                        return self.coerce(typed.id, defaulted, expected, site, span);
                    }
                }
                // §6.3's auto-borrow, reached one mode over. `coerce` runs it
                // for a value whose type is known; a literal has no type yet,
                // so the peeling is here instead — the literal takes the
                // *referent's* type and the borrow the author was told to leave
                // out becomes a node. Without it `by_ref(42)` bound the
                // literal's variable to `borrowed I64` itself: a literal whose
                // type is a reference, and nothing for MIR to take a borrow at.
                //
                // `Site::Elsewhere` inside, because one call site takes one
                // borrow: §6.3 says the caller writes `compare(a, b)`, not that
                // a `borrowed borrowed T` is reachable.
                if site == Site::Argument {
                    if let TyKind::Borrowed { mutable, inner } = *self.types.kind(target) {
                        let operand = self.demand(typed, inner, Site::Elsewhere, span);
                        return self.body.push_expr(
                            ExprKind::Borrow { mutable, operand },
                            expected,
                            span,
                        );
                    }
                }
                // A variable stands for a *value*, so Decision 6's widening
                // applies to it as it does to anything else: a literal reaching
                // a `T?` slot takes `T` and widens, rather than becoming a `T?`
                // that no arithmetic will accept afterwards.
                //
                // **Except `null`**, which is the one value whose type *is* the
                // nullable. Widening it would ask `T` to hold it, and there is
                // no `T` that does — Decision 6 makes `T?` a distinct type
                // precisely so that `null` inhabits it and nothing else.
                let nullable = matches!(*self.types.kind(target), TyKind::Nullable(_));
                if self.literal_kind(var) == Some(Numeric::Null) {
                    // §5: `null` inhabits a nullable and nothing else, so it is
                    // refused at every shape this phase can classify and
                    // admitted at the four it cannot.
                    let admits = nullable
                        || !self.decls.prelude().is_available()
                        || is_opaque(self.types, target);
                    if !admits {
                        let rendered = self.types.render(self.defs, expected);
                        self.diagnostics.push(mismatched_types(span, &rendered, "`null`"));
                        return typed.id;
                    }
                    let _ = self.infer.bind(self.types, var, expected);
                    return typed.id;
                }
                let (payload, widen) = match *self.types.kind(target) {
                    TyKind::Nullable(inner) => (inner, true),
                    _ => (expected, false),
                };
                if !self.literal_admits(var, payload, span) {
                    let found = self.numeric_name(var);
                    let expected = self.types.render(self.defs, expected);
                    self.diagnostics.push(mismatched_types(span, &expected, found));
                    return typed.id;
                }
                match self.infer.bind(self.types, var, payload) {
                    Ok(_) => {}
                    Err(_) => {
                        self.mismatch(payload, expected, span);
                        return typed.id;
                    }
                }
                if widen {
                    self.body.push_expr(
                        ExprKind::Coerce { operand: typed.id, coercion: Coercion::Widen },
                        expected,
                        span,
                    )
                } else {
                    typed.id
                }
            }
        }
    }

    /// The literal kind a variable came from, if it came from one.
    fn literal_kind(&self, var: InferVar) -> Option<Numeric> {
        self.numeric.iter().find(|(v, _)| *v == var).map(|(_, kind)| *kind)
    }

    /// §5. Whether a literal's variable will agree to this type.
    fn literal_admits(&mut self, var: InferVar, target: Ty, span: Span) -> bool {
        let Some(kind) = self.literal_kind(var) else {
            // Not a literal: an ordinary variable, which takes what it is
            // given and reports at the unification instead.
            return true;
        };
        // §5's admission, asked *before* revealing. A type this phase cannot
        // classify takes any literal, `null` included — which is why this
        // returns rather than falling into the match below, where `null`'s arm
        // is an unconditional refusal that `demand` has already earned by
        // peeling the nullable off. And asking first keeps an already-reported
        // type out of `reveal`, whose overflow the caller has reported once
        // already.
        if is_opaque(self.types, target) || !self.decls.prelude().is_available() {
            return true;
        }
        // Revealed, because `type Celsius is F64` is a floating type and the
        // name is not. The seam's middle call, at the one comparison §5 makes.
        let target = self.revealed(target, span);
        match kind {
            // An integer is exact in every numeric type — `let x: F64 be 1` is
            // what a scientific program writes — so both numeric answers admit
            // it and only a shape this phase can place refuses.
            Numeric::Integer => self.numeric_shape(target) != Shape::NotANumber,
            // A float is not: `let n: I32 be 1.5` is a value the target cannot
            // hold, which is Decision 2 read backwards.
            Numeric::Float => matches!(
                self.numeric_shape(target),
                Shape::Float | Shape::Unanswerable
            ),
            // `null` only fits a nullable, and `demand` has already peeled one
            // off, so reaching here at all means the slot was not one.
            Numeric::Null => false,
        }
    }

    /// §5b. **A numeric literal at a `borrowed any I` parameter is defaulted
    /// here, and then asked the ordinary question.**
    ///
    /// **The decision.** [`Self::literal_admits`] is asked whether a literal's
    /// *variable* may be bound to the slot's type, and against an interface
    /// object it says no — correctly, because
    /// [`Self::numeric_shape`] classifies [`TyKind::Object`] as
    /// [`Shape::NotANumber`] and a trait object is not a numeric type. But the
    /// question at a `borrowed any Display` parameter is not *"is this slot a
    /// number"*; it is *"is there a number that reaches this slot"*, and there
    /// is: Decision 2 says an unconstrained integer literal is an `I64`,
    /// `builtins.rs` says `I64 implements Display`, and [`assignable`]'s rule 6
    /// unsizes `borrowed I64` into `borrowed any Display`. So the literal is
    /// bound to its default *first* and handed to [`Self::coerce`], which
    /// composes §6.3's auto-borrow with §4's unsizing exactly as it does for a
    /// value whose type was written.
    ///
    /// **The reason it is not left to [`Self::finish`].** Decision 2's
    /// defaulting runs over the variables that are still *unresolved* when the
    /// body ends, and a literal that reaches an interface object never gets
    /// that far: `demand` reports at the slot and returns. The default has to
    /// run at the one place the expectation makes it necessary, which is here.
    ///
    /// **What it refuses, and each refusal is the honest half.**
    ///
    /// - **A `mutable borrowed any I` slot.** Rule 6 requires the same
    ///   mutability on both sides and a literal has no place to borrow
    ///   exclusively from; admitting it would invent an lvalue.
    /// - **A bare `any I` slot**, and `(any I)?`. §5 of [`crate::assign`]
    ///   refuses owned unsizing — *"an owned `any Summarize` is constructed
    ///   where it is written"* — so there is no coercion to compose with, and
    ///   defaulting first would only change *expected `any Display`, found an
    ///   integer literal* into the same refusal with `I64` in it. The literal
    ///   is the more useful noun.
    /// - **A default that does not implement the interface.** The relation is
    ///   asked of `I64` or `F64` before the variable is bound, so a
    ///   `borrowed any Iterate` parameter given `1` still reports at the
    ///   literal rather than at a coercion the reader has to work backwards
    ///   from.
    ///
    /// **The cost.** Decision 2's default is now taken at a *slot* as well as
    /// at the end of a body, so `show(1)` against `borrowed any Display` fixes
    /// the literal at `I64` where a later `show(1 + x)` with `x: I32` would
    /// have inferred `I32`. That is the same commitment `let n: I64 be 1`
    /// makes and it is made for the same reason — the slot is the only
    /// constraint there is — but it is a commitment, and a literal that meets
    /// an object *and* a numeric type in one class will now find the class
    /// already bound.
    fn literal_unsizes(&mut self, var: InferVar, target: Ty, span: Span) -> Option<Ty> {
        let kind = self.literal_kind(var)?;
        let TyKind::Borrowed { mutable: false, inner } = *self.types.kind(target) else {
            return None;
        };
        let object = self.revealed(inner, span);
        let TyKind::Object { interface, .. } = *self.types.kind(object) else {
            return None;
        };
        let default = match kind {
            Numeric::Integer => self.decls.prelude().default_int(self.types)?,
            Numeric::Float => self.decls.prelude().default_float(self.types)?,
            // `demand` peeled `null` off above; the arm is here so that a new
            // `Numeric` cannot be admitted by a wildcard.
            Numeric::Null => return None,
        };
        if !self.decls.methods().implements(self.types, default, interface) {
            return None;
        }
        self.infer.bind(self.types, var, default).ok()?;
        Some(default)
    }

    /// §5. What this phase can say about a type a *number* is being put in.
    ///
    /// **Three answers, and the third is what keeps the check honest.** The
    /// predicate this replaces had two and named four types — `Bool`, `String`,
    /// `Char`, `Never` — so every other type in the language was an admission
    /// by default, which is how `by_value(42)` reached a `Doc`. The four are
    /// still here and still right about what they name; what has changed is
    /// that a type they do not name is now classified by its **shape** before
    /// it falls through to [`Shape::Unanswerable`].
    ///
    /// **What it refuses, and the reason each refusal is safe:**
    ///
    /// - A **tuple, closure, unit, borrow or interface object** is a shape no
    ///   number has and no rule in [`crate::assign`] produces from a literal.
    ///   (A borrow is refused *here*; §6.3's auto-borrow in
    ///   [`BodyChecker::demand`] runs before this is asked, so the one place a
    ///   literal legitimately meets a `borrowed T` never reaches this arm.)
    /// - A **name applied to generic arguments**. Every numeric type in the
    ///   language is a nullary name: §5.1's primitives are, and
    ///   `ffi-c-boundary.md` §1.3's C scalars are. `Array of I64` is a
    ///   container.
    /// - A **record or a choice**, because this phase holds the declaration.
    ///
    /// **What it admits, and why refusing would be a false positive:** a type
    /// parameter, a `Self`, a `Self.Item`, an already-erroneous type, anything
    /// at all when there is no prelude to compare against — and a **builtin
    /// name this phase cannot place**. That last one is the compromise
    /// [`Prelude::is_definitely_not_numeric`] already documents and it survives
    /// unchanged: `ffi.CInt` is a number although §5.1 does not list it, so a
    /// builtin nullary name that is on none of the three lists gets no verdict.
    ///
    /// **A nullable is answered by its payload.** [`BodyChecker::demand`] peels
    /// one off before asking — Decision 6's widening is the rule there — so a
    /// nullable reaching here came from `BodyChecker::compare`, where the
    /// question is which *number* is on the other side of an `is`.
    fn numeric_shape(&mut self, target: Ty) -> Shape {
        if !self.decls.prelude().is_available() || is_opaque(self.types, target) {
            return Shape::Unanswerable;
        }
        match *self.types.kind(target) {
            TyKind::Unit
            | TyKind::Tuple(_)
            | TyKind::Closure { .. }
            | TyKind::Borrowed { .. }
            | TyKind::Object { .. } => Shape::NotANumber,
            TyKind::Nullable(inner) => self.numeric_shape(inner),
            // `is_opaque` answered all three of these above; the arm is here
            // so that a new `TyKind` cannot be admitted by a wildcard.
            TyKind::Error | TyKind::Param { .. } | TyKind::SelfType { .. }
            | TyKind::SelfAssoc { .. } => Shape::Unanswerable,
            TyKind::Named { def, .. } => self.named_shape(def, target),
        }
    }

    /// [`BodyChecker::numeric_shape`] for a name, which is where the prelude's
    /// three lists and the definition table meet.
    fn named_shape(&mut self, def: DefId, target: Ty) -> Shape {
        let prelude = self.decls.prelude();
        if prelude.is_integer(self.types, target) {
            return Shape::Integer;
        }
        if prelude.is_float(self.types, target) {
            return Shape::Float;
        }
        if prelude.is_definitely_not_numeric(self.types, target) {
            return Shape::NotANumber;
        }
        // The three predicates above all require an empty argument list, so a
        // name that survives them and has arguments is a container.
        if matches!(self.types.kind(target), TyKind::Named { args, .. } if !args.is_empty()) {
            return Shape::NotANumber;
        }
        match self.defs.get(def).kind {
            // The declaration is in hand and it describes fields or variants.
            hir::DefKind::Record | hir::DefKind::Choice => Shape::NotANumber,
            // A builtin name on none of the three lists — `ffi.CInt`, `Array`,
            // an `extern` type — and anything else. §5 admits rather than
            // guesses.
            _ => Shape::Unanswerable,
        }
    }

    /// `f"…"` — §1.1's interpolating literal.
    ///
    /// **The decision. The literal has type `String`, every hole is
    /// synthesised, and every hole's type is required to implement
    /// `Display`.** Nothing else is decided here: no rendering is chosen, no
    /// method is named, and the node keeps its parts.
    ///
    /// **The reason `Display` is required and not *dispatched to*.** §3.1
    /// respecifies `Display` as `def display(self, into: mutable borrowed
    /// Formatter)`, and `science-resolve`'s `builtins` declares it with **no
    /// methods**, three times over, because naming that method would invent a
    /// `Formatter` no note specifies. That refusal stands. What a bound check
    /// needs is the *relation* — *"does this type implement `Display`"* — and
    /// the relation is declared for every prelude type. So this asks the
    /// question the prelude can answer and does not ask the one it cannot,
    /// which is [`Self::implements_operand`]'s rule applied to a second
    /// construct.
    ///
    /// **The cost, and it is the whole of what this check cannot do.** A user
    /// type with an `implements Display:` block satisfies this check, and
    /// nothing anywhere can call its `display`, because there is no method name
    /// to call. The interpolation of a user type is therefore *accepted here
    /// and refused by codegen*, which is a worse place to find out. The
    /// alternative — restricting the check to the prelude types that have a
    /// renderer — would put a list of what the backend happens to support into
    /// the type system, and that list would be wrong the day the backend grew.
    ///
    /// **A hole whose type is still a variable is not reported.** Decision 2
    /// defaults an unconstrained numeric literal, and the default runs after
    /// this; asking now would report `f"{1 + 1}"` as a value of undetermined
    /// type. [`Self::known_or_error`] turning a variable into [`Ty::ERROR`] is
    /// what makes the silence fall out rather than needing an arm.
    fn fstring(&mut self, parts: &[hir::FStringPart], span: Span) -> Typed {
        let mut lowered = Vec::with_capacity(parts.len());
        for part in parts {
            match part {
                hir::FStringPart::Text(text) => lowered.push(FStringPart::Text(text.clone())),
                hir::FStringPart::Hole(expr) => {
                    let typed = self.synth(expr);
                    self.requires_display(typed, expr.span);
                    lowered.push(FStringPart::Hole(typed.id));
                }
            }
        }
        let ty = match self.decls.prelude().ty(self.types, "String") {
            Some(ty) => InferTy::Known(ty),
            None => InferTy::Known(Ty::ERROR),
        };
        self.push_typed(ExprKind::FString(lowered), ty, span)
    }

    /// `SC0275`: the hole's type must implement `Display`.
    ///
    /// Conservative in the four ways [`Self::implements_operand`] is, and for
    /// the same reasons: an unresolved variable, a type that already carries an
    /// error, a receiver with no head, and a type whose method surface is open
    /// are each a *"cannot say"* rather than a *"no"*.
    ///
    /// **`T?` is reported although those guards would let it through, and it is
    /// the only type here that gets its own arm.** §3.4 decides that a nullable
    /// does not implement `Display`, and unlike §3.4's other three refusals
    /// this one is *knowable*: `T?` is a type the compiler builds, no file can
    /// write `I64? implements Display:`, and so the absence is a fact rather
    /// than an untranscribed prelude row. The value of reporting it is what
    /// §3.4 says it is — *"a silent `null` or an empty cell in a published
    /// table is the failure this prevents"* — and the fix is a narrowing the
    /// author writes.
    ///
    /// **The other three of §3.4's refusals are not reported, and this is the
    /// hole to record.** An array, a map and a closure all reach here and all
    /// pass, because `builtins.rs`' `IMPLEMENTS` table has no `Array` row at
    /// all — deliberately, since `Array of T implements Clone` holds only where
    /// `T: Clone` and a conditional implementation is not something that index
    /// can express. So *"`Array` does not implement `Display`"* is
    /// indistinguishable here from *"nobody has written `Array`'s row yet"*,
    /// and reporting the first on the evidence for the second is the mistake
    /// `Methods::answers_for` exists to prevent. §3.4's array decision is
    /// therefore **specified and unenforced**, and closing it means giving the
    /// prelude a way to say that an implementation is absent on purpose.
    fn requires_display(&mut self, operand: Typed, span: Span) {
        let Some(display) = self.decls.prelude().get("Display") else { return };
        let written = self.known_or_error(operand.ty);
        let revealed = self.revealed(written, span);
        if self.types.references_error(revealed) {
            return;
        }
        if matches!(self.types.kind(revealed), TyKind::Nullable(_)) {
            let rendered = self.types.render(self.defs, revealed);
            self.diagnostics.push(not_displayable(span, &rendered));
            return;
        }
        let Some(head) = self.decls.methods().receiver(self.defs, self.types, revealed) else {
            return;
        };
        if !self.decls.methods().surface_is_closed(self.defs, head) {
            return;
        }
        if self.decls.methods().declares(self.types, revealed, display) {
            return;
        }
        let self_ty = self.receiver_self_ty(revealed, span);
        let rendered = self.types.render(self.defs, self_ty);
        self.diagnostics.push(not_displayable(span, &rendered));
    }

    fn numeric_name(&self, var: InferVar) -> &'static str {
        match self.literal_kind(var) {
            Some(Numeric::Integer) => "an integer literal",
            Some(Numeric::Float) => "a floating-point literal",
            Some(Numeric::Null) => "`null`",
            None => "a value of an undetermined type",
        }
    }

    // --- synthesis --------------------------------------------------------

    fn synth(&mut self, expr: &hir::Expr) -> Typed {
        let span = expr.span;
        match &expr.kind {
            hir::ExprKind::Literal(literal) => self.literal(literal, span),
            hir::ExprKind::FString(parts) => self.fstring(parts, span),
            hir::ExprKind::Path { res, generics } => self.path(*res, generics, span),
            hir::ExprKind::SelfValue(res) => match res {
                Res::Def(def) => {
                    let ty = self.local_ty(*def);
                    let typed = self.push_typed(ExprKind::SelfValue(*def), ty, span);
                    self.narrowed(typed.id, ty, span)
                }
                _ => self.error_expr(span),
            },
            hir::ExprKind::Call { callee, args } => self.call(callee, args, span),
            hir::ExprKind::MethodCall { receiver, method, generics, args } => {
                self.method_call(receiver, method, generics, args, span)
            }
            hir::ExprKind::Field { base, name } => self.field(base, name, span),
            hir::ExprKind::Index { base, index } => {
                self.index_expr(base, index, span, Indexing::Read)
            }
            hir::ExprKind::ArrayLit(elements) => self.array_lit(elements, span),
            hir::ExprKind::StructLit { res, fields } => self.record_lit(*res, fields, span),
            hir::ExprKind::Tuple(elements) => {
                let mut ids = Vec::with_capacity(elements.len());
                let mut tys = Vec::with_capacity(elements.len());
                for element in elements {
                    let typed = self.synth(element);
                    ids.push(typed.id);
                    tys.push(self.known_or_error(typed.ty));
                }
                let ty = self.types.tuple(tys);
                let id = self.body.push_expr(ExprKind::Tuple(ids), ty, span);
                Typed { id, ty: InferTy::Known(ty) }
            }
            hir::ExprKind::Unit => {
                let id = self.body.push_expr(ExprKind::Unit, Ty::UNIT, span);
                Typed { id, ty: InferTy::Known(Ty::UNIT) }
            }
            hir::ExprKind::Unary { op, operand } => self.unary(*op, operand, span),
            hir::ExprKind::Binary { op, lhs, rhs } => self.binary(*op, lhs, rhs, span),
            hir::ExprKind::Cast { expr: operand, ty } => {
                let operand = self.synth(operand).id;
                let ty = self.lower_ty(ty);
                let id = self.body.push_expr(ExprKind::Cast { operand }, ty, span);
                Typed { id, ty: InferTy::Known(ty) }
            }
            hir::ExprKind::Present(inner) => self.present(inner, span),
            hir::ExprKind::Borrowed { mutable, expr: inner } => {
                let typed = self.synth(inner);
                let inner_ty = self.known_or_error(typed.ty);
                if *mutable {
                    // Decision 8: the invalidation is at the point the
                    // exclusive borrow is *created*. `narrow`'s §4.
                    if let Some(place) = self.body.place_of(typed.id) {
                        self.facts.invalidate(&place);
                    }
                }
                let ty = self.types.borrowed(*mutable, inner_ty);
                let id = self.body.push_expr(
                    ExprKind::Borrow { mutable: *mutable, operand: typed.id },
                    ty,
                    span,
                );
                Typed { id, ty: InferTy::Known(ty) }
            }
            hir::ExprKind::Range { start, end, inclusive } => {
                let start_typed = self.synth(start);
                let end_ty = self.known_or_error(start_typed.ty);
                let end = self.check(end, end_ty, Site::Elsewhere);
                // §6: there is no `Range` type in the prelude to give this.
                let id = self.body.push_expr(
                    ExprKind::Range { start: start_typed.id, end, inclusive: *inclusive },
                    Ty::ERROR,
                    span,
                );
                Typed { id, ty: InferTy::Known(Ty::ERROR) }
            }
            hir::ExprKind::Closure { param, body } => self.closure(*param, body, span, None),
            hir::ExprKind::Each(res) => match res {
                Res::Def(def) => {
                    let ty = self.local_ty(*def);
                    let typed = self.push_typed(ExprKind::Local(*def), ty, span);
                    self.narrowed(typed.id, ty, span)
                }
                _ => self.error_expr(span),
            },
            hir::ExprKind::If(if_expr) => self.if_expr(if_expr, span, None),
            hir::ExprKind::Match(match_expr) => self.match_expr(match_expr, span, None),
            hir::ExprKind::Loop { body } => self.loop_expr(body, span),
            hir::ExprKind::For { pattern, iter, body } => self.for_expr(pattern, iter, body, span),
            hir::ExprKind::Unsafe(block) | hir::ExprKind::Block(block) => {
                let id = self.body.reserve_block(block.span);
                let filled = self.block(block, None);
                let ty = filled.tail.map(|tail| self.body.ty(tail)).unwrap_or(Ty::UNIT);
                self.body.fill_block(id, filled);
                let kind = if matches!(expr.kind, hir::ExprKind::Unsafe(_)) {
                    ExprKind::Unsafe(id)
                } else {
                    ExprKind::Block(id)
                };
                let id = self.body.push_expr(kind, ty, span);
                Typed { id, ty: InferTy::Known(ty) }
            }
            hir::ExprKind::Error => self.error_expr(span),
        }
    }

    fn literal(&mut self, literal: &Literal, span: Span) -> Typed {
        let (kind, name): (Option<Numeric>, Option<&str>) = match literal {
            Literal::Bool(_) => (None, Some("Bool")),
            Literal::Str(_) => (None, Some("String")),
            Literal::Char(_) => (None, Some("Char")),
            Literal::Int { suffix: Some(suffix), .. }
            | Literal::Float { suffix: Some(suffix), .. } => (None, Some(suffix_name(*suffix))),
            Literal::Int { .. } => (Some(Numeric::Integer), None),
            Literal::Float { .. } => (Some(Numeric::Float), None),
            Literal::Null => (Some(Numeric::Null), None),
        };
        let node = ExprKind::Literal(literal.clone());
        match (kind, name.and_then(|name| self.decls.prelude().ty(self.types, name))) {
            (_, Some(ty)) => {
                let id = self.body.push_expr(node, ty, span);
                Typed { id, ty: InferTy::Known(ty) }
            }
            (Some(kind), None) => {
                // §4: the type is a variable until the body ends.
                let var = self.infer.fresh(span);
                self.numeric.push((var, kind));
                let id = self.body.push_expr(node, Ty::ERROR, span);
                self.pending.push((id, var));
                Typed { id, ty: InferTy::Var(var) }
            }
            // A table with no prelude: there is no `Bool` to give this.
            (None, None) => {
                let id = self.body.push_expr(node, Ty::ERROR, span);
                Typed { id, ty: InferTy::Known(Ty::ERROR) }
            }
        }
    }

    fn path(&mut self, res: Res, generics: &[hir::Type], span: Span) -> Typed {
        match named(self.defs, res) {
            Some(Named::Local(def)) => {
                let ty = self.local_ty(def);
                let typed = self.push_typed(ExprKind::Local(def), ty, span);
                self.narrowed(typed.id, ty, span)
            }
            Some(Named::Function(def)) => {
                let ty = self.function_as_value(def, span);
                let id = self.body.push_expr(ExprKind::Item(def), ty, span);
                Typed { id, ty: InferTy::Known(ty) }
            }
            Some(Named::Const(def)) => {
                let ty = self.decls.const_ty(def).unwrap_or(Ty::ERROR);
                let id = self.body.push_expr(ExprKind::Item(def), ty, span);
                Typed { id, ty: InferTy::Known(ty) }
            }
            Some(Named::Variant(def)) => {
                let ty = self.variant_as_value(def, generics);
                let id = self.body.push_expr(ExprKind::Item(def), ty, span);
                Typed { id, ty: InferTy::Known(ty) }
            }
            // A type in a value position, or a name that did not resolve.
            // Both are already reported, by `SC0211` and `SC0200`.
            Some(Named::Other) | None => self.error_expr(span),
        }
    }

    /// A function named rather than called: its closure type (§1.2).
    fn function_as_value(&mut self, def: DefId, span: Span) -> Ty {
        let Some(sig) = self.decls.signature(def) else {
            return Ty::ERROR;
        };
        let params: Vec<Ty> = sig.params.iter().map(|param| param.ty).collect();
        let ret = sig.ret;
        let params = params.into_iter().map(|ty| self.instantiate(ty, span)).collect();
        let ret = self.instantiate(ret, span);
        self.types.closure(params, ret)
    }

    fn variant_as_value(&mut self, def: DefId, generics: &[hir::Type]) -> Ty {
        let Some(variant) = self.decls.variant(def) else {
            return Ty::ERROR;
        };
        let (choice, payload, declared) =
            (variant.choice, variant.payload.clone(), variant.generics.clone());
        let written: Vec<GenericArg> = generics
            .iter()
            .map(|ty| {
                let lowered = self.lower_ty(ty);
                GenericArg::Type(lowered)
            })
            .collect();
        // `Unbounded` on its own says nothing about the `T` of `Bound of T`, so
        // every argument the author did not write is `Ty::ERROR` — §6's hole,
        // spelled the way `ty`'s §5 spells every hole, so that the type agrees
        // with whatever slot it reaches instead of contradicting it.
        let args = if written.is_empty() { unknown_args(&declared) } else { written };
        let choice_ty = self.types.named(choice, args);
        if payload.is_empty() {
            choice_ty
        } else {
            self.types.closure(payload, choice_ty)
        }
    }

    /// A read, with Decision 7's fact applied if there is one.
    ///
    /// This is the only place narrowing changes anything, and it is one node:
    /// `thir`'s §1 says why a narrowed read is not a retyped one.
    fn narrowed(&mut self, id: ExprId, ty: InferTy, span: Span) -> Typed {
        let InferTy::Known(known) = ty else {
            return Typed { id, ty };
        };
        let Some(place) = self.body.place_of(id) else {
            return Typed { id, ty };
        };
        if self.facts.get(&place) != Some(Fact::NonNull) {
            return Typed { id, ty };
        }
        let revealed = self.revealed(known, span);
        let TyKind::Nullable(inner) = *self.types.kind(revealed) else {
            return Typed { id, ty };
        };
        let id = self.body.push_expr(ExprKind::Narrow(id), inner, span);
        Typed { id, ty: InferTy::Known(inner) }
    }

    fn field(&mut self, base: &hir::Expr, name: &hir::Ident, span: Span) -> Typed {
        let base = self.synth(base);
        let base_ty = self.known_or_error(base.ty);
        let revealed = self.revealed(base_ty, span);
        let (def, args) = match self.types.kind(revealed).clone() {
            TyKind::Named { def, args } => (def, args),
            // A borrow is transparent to a field read: `borrowed Doc` has a
            // `title` because a `Doc` has one, and F0 has no explicit
            // dereference to write instead.
            TyKind::Borrowed { inner, .. } => {
                let inner = self.revealed(inner, span);
                match self.types.kind(inner).clone() {
                    TyKind::Named { def, args } => (def, args),
                    _ => return self.unknown_field(base.id, revealed, name, span),
                }
            }
            _ => return self.unknown_field(base.id, revealed, name, span),
        };
        let Some(record) = self.decls.record(def) else {
            return self.unknown_field(base.id, revealed, name, span);
        };
        let generics = record.generics.clone();
        let Some((field, field_ty)) =
            record.fields.iter().find(|(id, _)| self.defs.get(*id).name == name.name).copied()
        else {
            return self.unknown_field(base.id, revealed, name, span);
        };
        // A field of `Matrix of (F32, 4)` is the declared field with the
        // block's parameters replaced, which is `subst`'s whole job.
        let substitution = Substitution::of_generics(&generics, &args);
        let field_ty = match substitution.apply(self.types, field_ty) {
            Ok(ty) => ty,
            Err(error) => {
                self.diagnostics.push(diagnostics::overflowed(error, span));
                Ty::ERROR
            }
        };
        let id =
            self.body.push_expr(ExprKind::Field { base: base.id, field: Some(field) }, field_ty, span);
        self.narrowed(id, InferTy::Known(field_ty), span)
    }

    fn unknown_field(
        &mut self,
        base: ExprId,
        base_ty: Ty,
        name: &hir::Ident,
        span: Span,
    ) -> Typed {
        // A type that is already wrong says nothing more (`ty`'s §5), and a
        // type this phase cannot see inside — a generic parameter, `Self`, an
        // unresolved `Self.Item` — says nothing either, because *"`T` has no
        // field `name`"* is a claim about an instantiation nobody has made yet.
        let answerable = !matches!(
            self.types.kind(base_ty),
            TyKind::Param { .. } | TyKind::SelfType { .. } | TyKind::SelfAssoc { .. }
        );
        if answerable && !self.types.references_error(base_ty) && self.decls.prelude().is_available()
        {
            let rendered = self.types.render(self.defs, base_ty);
            self.diagnostics.push(no_such_field(name.span, &name.name, &rendered));
        }
        let id =
            self.body.push_expr(ExprKind::Field { base, field: None }, Ty::ERROR, span);
        Typed { id, ty: InferTy::Known(Ty::ERROR) }
    }

    fn record_lit(&mut self, res: Res, fields: &[hir::FieldInit], span: Span) -> Typed {
        let Some(def) = res.def_id() else {
            for field in fields {
                self.synth(&field.value);
            }
            return self.error_expr(span);
        };
        let Some(record) = self.decls.record(def) else {
            for field in fields {
                self.synth(&field.value);
            }
            return self.error_expr(span);
        };
        let declared = record.fields.clone();
        let generics = record.generics.clone();

        // `Wrapper(inner: value)` has to become `Wrapper of T`, not `Wrapper`.
        // The arguments are solved the same root-level way a call's are (§6),
        // out of the field types the declaration gives and the values the
        // literal supplies — and an unsolved one becomes `Ty::ERROR` so that the
        // fields are still checked against something that agrees.
        let (substitution, args) = self.instantiate_record(&generics, &declared, fields);

        let mut checked = Vec::with_capacity(fields.len());
        for init in fields {
            match declared.iter().find(|(id, _)| Some(*id) == init.field.def_id()).copied() {
                Some((field, ty)) => {
                    let ty = self.apply(&substitution, ty, init.span);
                    let ty = self.instantiate(ty, init.span);
                    let value = self.check(&init.value, ty, Site::Elsewhere);
                    checked.push((field, value));
                }
                // The resolver reported `SC0205`. §6: a *missing* field is
                // nobody's yet.
                None => {
                    self.synth(&init.value);
                }
            }
        }
        let ty = self.types.named(def, args);
        let id = self.body.push_expr(ExprKind::Record { def, fields: checked }, ty, span);
        Typed { id, ty: InferTy::Known(ty) }
    }

    /// §6's root-level instantiation, for a record literal.
    ///
    /// The same rule as a call's and the same refusal: a field whose declared
    /// type *is* a parameter of the record solves it, and anything deeper does
    /// not. A const parameter is never solved here at all — the matching of
    /// [`crate::matching`] is the machinery for it and it wants an obligation to
    /// discharge, which is `SC0262` and F1's — so it becomes
    /// [`GenericArg::Error`], which `subst`'s own documentation says is *"the
    /// same as too few arguments"*.
    fn instantiate_record(
        &mut self,
        generics: &[hir::GenericParam],
        declared: &[(DefId, Ty)],
        fields: &[hir::FieldInit],
    ) -> (Substitution, Vec<GenericArg>) {
        if generics.is_empty() {
            return (Substitution::new(), Vec::new());
        }
        let mut solved: HashMap<DefId, Ty> = HashMap::new();
        for init in fields {
            let Some(field) = init.field.def_id() else { continue };
            let Some((_, field_ty)) = declared.iter().find(|(id, _)| *id == field) else {
                continue;
            };
            let TyKind::Param { def } = *self.types.kind(*field_ty) else { continue };
            if solved.contains_key(&def) || !generics.iter().any(|p| p.def == def) {
                continue;
            }
            if let Some(ty) = self.probe(&init.value) {
                solved.insert(def, ty);
            }
        }
        let mut substitution = Substitution::new();
        let mut args = Vec::with_capacity(generics.len());
        for param in generics {
            match param.kind {
                hir::GenericParamKind::Type { .. } => {
                    let ty = solved.get(&param.def).copied().unwrap_or(Ty::ERROR);
                    substitution = substitution.with_type(param.def, ty);
                    args.push(GenericArg::Type(ty));
                }
                hir::GenericParamKind::Const { .. } => args.push(GenericArg::Error),
            }
        }
        (substitution, args)
    }

    fn call(&mut self, callee: &hir::Expr, args: &[hir::Arg], span: Span) -> Typed {
        if let hir::ExprKind::Path { res, generics } = &callee.kind {
            let resolved = named(self.defs, *res);
            if let Some(Named::Function(def)) = resolved {
                // §4.1's arity, before anything reads a signature: `print` has
                // none, so this is the only place the call is looked at at all.
                self.output_is_unary(def, args, span);
            }
            match resolved {
                // `panic` is a prelude name with no `hir::Fn` behind it, so
                // there is no signature to read `-> Never` off. It is the one
                // way a body diverges without a `return`, which is the half of
                // `SC0140`'s fourth exclusion — *"or that panics on every
                // path"* — that is a property of the body.
                Some(Named::Function(def)) if Some(def) == self.decls.prelude().panic() => {
                    let callee = self.body.push_expr(ExprKind::Item(def), Ty::ERROR, span);
                    let ids = args.iter().map(|arg| self.synth(&arg.value).id).collect();
                    let id = self
                        .body
                        .push_expr(ExprKind::Call { callee, args: ids }, Ty::ERROR, span);
                    self.diverged = true;
                    return Typed { id, ty: InferTy::Known(Ty::ERROR) };
                }
                Some(Named::Function(def)) if self.decls.signature(def).is_some() => {
                    return self.call_signature(def, generics, args, span);
                }
                Some(Named::Variant(def)) if self.decls.variant(def).is_some() => {
                    return self.call_variant(def, args, span);
                }
                _ => {}
            }
        }
        // A closure value, or something already reported.
        let callee = self.synth(callee);
        let callee_ty = self.known_or_error(callee.ty);
        let revealed = self.revealed(callee_ty, span);
        if let TyKind::Closure { params, ret } = self.types.kind(revealed).clone() {
            if params.len() != args.len() {
                self.diagnostics.push(wrong_argument_count(span, params.len(), args.len()));
            }
            let ids = args
                .iter()
                .enumerate()
                .map(|(at, arg)| match params.get(at) {
                    Some(ty) => self.check(&arg.value, *ty, Site::Argument),
                    None => self.synth(&arg.value).id,
                })
                .collect();
            let id = self.body.push_expr(ExprKind::Call { callee: callee.id, args: ids }, ret, span);
            return Typed { id, ty: InferTy::Known(ret) };
        }
        let ids = args.iter().map(|arg| self.synth(&arg.value).id).collect();
        let id =
            self.body.push_expr(ExprKind::Call { callee: callee.id, args: ids }, Ty::ERROR, span);
        Typed { id, ty: InferTy::Known(Ty::ERROR) }
    }

    /// §7's `SC0275`, second clause: **`print` takes one value.**
    ///
    /// **The decision. §4.1's arity is checked here although §4.1's parameter
    /// type is not declared anywhere.** `strings-formatting-and-docs.md` §4.1
    /// settles two separate things about `print` — that it is unary, and that
    /// its argument is a `borrowed any Display` — and §7 renders the first as a
    /// diagnostic with a mechanical fix:
    ///
    /// ```text
    /// error[SC0275]: `print` takes one value
    ///  --> run.science:9:5
    ///    |
    ///  9 |     print("rows:", n)
    ///    |     ^^^^^^^^^^^^^^^^^
    ///    |
    /// help: interpolate instead
    ///    |
    ///  9 |     print(f"rows: {n}")
    /// ```
    ///
    /// **The reason it is not the ordinary arity check.** It would be, if
    /// `print` had a signature. It does not — `science-resolve`'s `builtins`
    /// measures what declaring one costs, and the cost is every `print` in
    /// every program becoming `Unlowered` — so `print("rows:", n)` fell through
    /// to the closure arm below, was typed [`Ty::ERROR`], and **reported
    /// nothing at all**, while §7's table said this code covered it. A
    /// diagnostic that is specified and numbered and fires on nothing is worse
    /// than one that was never written, because the note is evidence the
    /// language made a decision the compiler did not keep.
    ///
    /// Splitting it this way is what lets the half that needs nothing from the
    /// back half of the compiler land on its own. Arity is a fact about the
    /// *call*: it is decided by §4.4 having no variadic parameter form, it
    /// needs no parameter type, no `Display` relation, no vtable and no
    /// runtime entry point. The `Display` obligation is a fact about the
    /// *value* and needs all four.
    ///
    /// **Zero arguments too**, which §4.1 decides in its last paragraph:
    /// *"There is no overloading and there are no default arguments in §4.4, so
    /// a blank line is `print("")`"*. Same code, different note, because the
    /// fix is a different edit.
    ///
    /// **What it costs, and it is the reason this is keyed on a `DefId`.** A
    /// user may write `def print(a: Int, b: Int)` in a module of their own, and
    /// that call must not be refused for the shape of a prelude declaration it
    /// has nothing to do with. `Prelude::unary_output` answers about the
    /// identity of the definition and not about its name, which is the same
    /// discipline `items`' `WANTED` list is under for every other prelude name
    /// this crate asks about.
    ///
    /// **What it does not cover.** The `Display` half of §7's row, for a
    /// `print` argument — that is the declaration `builtins` withdrew, and the
    /// f-string's [`Self::requires_display`] is the only place this code asks
    /// the question today.
    fn output_is_unary(&mut self, def: DefId, args: &[hir::Arg], span: Span) {
        let Some(name) = self.decls.prelude().unary_output(def) else {
            return;
        };
        if args.len() == 1 {
            return;
        }
        self.diagnostics.push(print_takes_one_value(span, name, args.len()));
    }

    fn call_signature(
        &mut self,
        def: DefId,
        generics: &[hir::Type],
        args: &[hir::Arg],
        span: Span,
    ) -> Typed {
        let sig = self.decls.signature(def).expect("checked by the caller");
        let params: Vec<(DefId, Ty)> =
            sig.params.iter().map(|param| (param.def, param.ty)).collect();
        let ret = sig.ret;
        let declared = sig.generics.clone();
        let callee = self.body.push_expr(ExprKind::Item(def), Ty::ERROR, span);

        if params.len() != args.len() {
            self.diagnostics.push(wrong_argument_count(span, params.len(), args.len()));
        }

        // The arguments in the order the parameters are declared: a label names
        // a parameter (`docs.sort(by: f)`), and an unlabelled argument takes
        // the position it is in.
        let order = self.argument_order(&params, args);
        let substitution = self.instantiate_call(&declared, generics, &params, args, &order, span);
        self.check_bounds(def, &substitution, span);

        let mut ids: Vec<ExprId> = Vec::with_capacity(args.len());
        for (at, arg) in args.iter().enumerate() {
            match order[at].and_then(|index| params.get(index).copied()) {
                Some((_, param_ty)) => {
                    let param_ty = self.apply(&substitution, param_ty, arg.span);
                    let param_ty = self.instantiate(param_ty, arg.span);
                    ids.push(self.check(&arg.value, param_ty, Site::Argument));
                }
                None => ids.push(self.synth(&arg.value).id),
            }
        }
        let ret = self.apply(&substitution, ret, span);
        let ret = self.instantiate(ret, span);
        let id = self.body.push_expr(ExprKind::Call { callee, args: ids }, ret, span);
        if !self.decls.prelude().is_never(self.types, ret) {
            return Typed { id, ty: InferTy::Known(ret) };
        }
        // `panic` and anything else declared `-> Never`: control does not come
        // back, which is what `SC0140`'s fourth exclusion is written against.
        self.diverged = true;
        Typed { id, ty: InferTy::Known(ret) }
    }

    /// Which parameter each argument fills. §6 does not check arity beyond the
    /// count; this is only the pairing.
    fn argument_order(
        &self,
        params: &[(DefId, Ty)],
        args: &[hir::Arg],
    ) -> Vec<Option<usize>> {
        args.iter()
            .enumerate()
            .map(|(at, arg)| match &arg.name {
                Some(label) => params
                    .iter()
                    .position(|(def, _)| self.defs.get(*def).name == label.name)
                    .or(Some(at)),
                None => Some(at),
            })
            .map(|index| index.filter(|index| *index < params.len()))
            .collect()
    }

    /// §6's root-level instantiation of a generic callee.
    fn instantiate_call(
        &mut self,
        declared: &[hir::GenericParam],
        explicit: &[hir::Type],
        params: &[(DefId, Ty)],
        args: &[hir::Arg],
        order: &[Option<usize>],
        span: Span,
    ) -> Substitution {
        if declared.is_empty() {
            return Substitution::new();
        }
        let mut solved: HashMap<DefId, Ty> = HashMap::new();
        for (param, written) in declared.iter().zip(explicit) {
            if matches!(param.kind, hir::GenericParamKind::Type { .. }) {
                let ty = self.lower_ty(written);
                solved.insert(param.def, ty);
            }
        }
        // A parameter whose type *is* the generic parameter is solved by the
        // argument's synthesised type. Anything deeper is §6's hole.
        for (at, arg) in args.iter().enumerate() {
            let Some(index) = order[at] else { continue };
            let Some((_, param_ty)) = params.get(index) else { continue };
            let Some((def, borrowed)) = self.root_param(*param_ty) else { continue };
            if solved.contains_key(&def) || !declared.iter().any(|p| p.def == def) {
                continue;
            }
            // Synthesising the argument here and again at the check below would
            // build the node twice, so the probe is a *type* question only: it
            // is asked of a literal's default and of a name's declared type,
            // which is everything a root-level match can use.
            if let Some(ty) = self.probe(&arg.value) {
                // `borrowed T` against a `borrowed Doc` and against a `Doc`
                // both solve `T := Doc`: §6.3 is what makes the second spelling
                // the ordinary one, so the borrow is stripped from whichever
                // side wrote it.
                let ty = if borrowed { self.peel_borrow(ty, arg.span) } else { ty };
                solved.insert(def, ty);
            }
        }
        let mut substitution = Substitution::new();
        for param in declared {
            match param.kind {
                hir::GenericParamKind::Type { .. } => {
                    // Unsolved becomes `Ty::ERROR`, which agrees with whatever
                    // it meets, so the arguments are still checked against
                    // something and the call reports nothing it cannot justify.
                    let ty = solved.get(&param.def).copied().unwrap_or(Ty::ERROR);
                    substitution = substitution.with_type(param.def, ty);
                }
                // A const argument at a call has no inference here: the
                // one-variable matching of `crate::matching` is the machinery
                // for it and it wants an obligation to discharge, which is
                // `SC0262` and F1's.
                hir::GenericParamKind::Const { .. } => {}
            }
        }
        let _ = span;
        substitution
    }

    /// §8. Every bound the callee declared, held against what this call solved.
    ///
    /// **Run at the one place a callee's generics are solved** — immediately
    /// after [`BodyChecker::instantiate_call`], at both call forms — so that
    /// the substitution being checked is the one the arguments are about to be
    /// checked against, and not a second solve that could disagree with it.
    ///
    /// The solved type is read by *applying the substitution to the parameter
    /// itself* rather than through an accessor on [`Substitution`]. That is the
    /// same operation every parameter type in the signature is about to
    /// undergo, so a bound cannot be held against a different answer than the
    /// arguments are, and it needs nothing added to `subst`.
    ///
    /// **Three ways a bound is skipped, and each is a stated refusal:**
    ///
    /// - **The parameter is unsolved.** §6 leaves one [`Ty::ERROR`], which
    ///   `ty`'s §5 makes agree with whatever it meets; reporting against it
    ///   would blame a call for a hole the checker left.
    /// - **The substitution did not move it**, which is a const parameter or
    ///   one `instantiate_call` declined — the same case as above, reached by a
    ///   different road, and neither is a claim about the argument.
    /// - **The interface is one this compiler cannot answer for**, which is
    ///   `methods`'s §7: `builtins.rs` declares seventeen interfaces and no
    ///   implementation of any, so `T: Ord` at an `I64` is silence rather than
    ///   a no. [`Methods::answers_for`] draws that line, and §8 says what it
    ///   costs — most bounds in the corpus are at a prelude interface and are
    ///   therefore not checked.
    ///
    /// **What it costs where it does fire** is `methods`'s §4 in full: no
    /// blanket implementation, no supertrait, no bound on a type parameter is
    /// looked through. The third would be the one to miss, and it is not: a
    /// parameter passed on to another generic call has no head, and
    /// [`Methods::implements`] admits a headless type for exactly that reason.
    fn check_bounds(&mut self, callee: DefId, substitution: &Substitution, span: Span) {
        let Some(sig) = self.decls.signature(callee) else { return };
        if sig.bounds.is_empty() {
            return;
        }
        let bounds = sig.bounds.clone();
        for bound in bounds {
            let param = self.types.param(bound.param);
            let solved = self.apply(substitution, param, span);
            if solved == param || self.types.references_error(solved) {
                continue;
            }
            if !self.decls.methods().answers_for(self.defs, self.types, solved, bound.interface) {
                continue;
            }
            if self.decls.methods().implements(self.types, solved, bound.interface) {
                continue;
            }
            let rendered = self.types.render(self.defs, solved);
            let interface = self.defs.get(bound.interface).name.clone();
            let parameter = self.defs.get(bound.param).name.clone();
            self.diagnostics.push(unsatisfied_bound(
                span,
                &rendered,
                &interface,
                &parameter,
                bound.span,
            ));
        }
    }

    /// The generic parameter a callee's parameter type *is*, and whether it is
    /// behind a borrow. §6's root-level match, in one place.
    ///
    /// **`borrowed T` is a root-level match and `Array of T` is not**, and the
    /// difference is §6.3 rather than a depth: a borrow is the spelling the
    /// language *tells* the author to leave out at the call, so `value:
    /// borrowed T` and `value: T` are one parameter written two ways and an
    /// argument solves `T` in both. `Array of T` is a different type, and
    /// solving through it needs the nested representation `infer`'s §2
    /// describes and does not build.
    ///
    /// **What it changes beyond the solve** is what §8's bound check can see.
    /// `def describe of T: Summarize(value: borrowed T)` is how the corpus
    /// writes a bounded generic — every one of the five in `examples/` takes
    /// its subject by borrow — so a match that stopped at [`TyKind::Param`]
    /// left `T` unsolved at every call the bound was written for, and a bound
    /// on an unsolved parameter is skipped. The check and this match land
    /// together because neither is worth anything without the other.
    ///
    /// The mutability is not carried: `mutable borrowed T` solves `T` from the
    /// same argument, and whether the borrow may be taken exclusively is the
    /// question [`BodyChecker::auto_borrow`] asks at the argument itself.
    fn root_param(&self, param_ty: Ty) -> Option<(DefId, bool)> {
        match *self.types.kind(param_ty) {
            TyKind::Param { def } => Some((def, false)),
            TyKind::Borrowed { inner, .. } => match *self.types.kind(inner) {
                TyKind::Param { def } => Some((def, true)),
                _ => None,
            },
            _ => None,
        }
    }

    /// The type of an argument, without building a node for it.
    ///
    /// Only the two shapes a root-level match can use — a name whose type is
    /// declared, and a literal whose suffix fixes it. Everything else is
    /// `None`, which §6 turns into [`Ty::ERROR`].
    fn probe(&mut self, expr: &hir::Expr) -> Option<Ty> {
        match &expr.kind {
            hir::ExprKind::Path { res, .. } => match named(self.defs, *res)? {
                Named::Local(def) => self.lookup_local(def)?.known(),
                Named::Const(def) => self.decls.const_ty(def),
                _ => None,
            },
            hir::ExprKind::Literal(literal) => {
                let name = match literal {
                    Literal::Bool(_) => "Bool",
                    Literal::Str(_) => "String",
                    Literal::Char(_) => "Char",
                    Literal::Int { suffix: Some(suffix), .. }
                    | Literal::Float { suffix: Some(suffix), .. } => suffix_name(*suffix),
                    _ => return None,
                };
                self.decls.prelude().ty(self.types, name)
            }
            _ => None,
        }
    }

    fn apply(&mut self, substitution: &Substitution, ty: Ty, span: Span) -> Ty {
        if substitution.is_empty() {
            return ty;
        }
        match substitution.apply(self.types, ty) {
            Ok(applied) => applied,
            Err(error) => {
                self.diagnostics.push(diagnostics::overflowed(error, span));
                Ty::ERROR
            }
        }
    }

    fn call_variant(&mut self, def: DefId, args: &[hir::Arg], span: Span) -> Typed {
        let variant = self.decls.variant(def).expect("checked by the caller");
        let (choice, payload, declared) =
            (variant.choice, variant.payload.clone(), variant.generics.clone());
        let callee = self.body.push_expr(ExprKind::Item(def), Ty::ERROR, span);
        if payload.len() != args.len() {
            self.diagnostics.push(wrong_argument_count(span, payload.len(), args.len()));
        }
        // `Labelled(2, "width")` fixes the `L` of `Tagged of (T, L)`, by the
        // same root-level rule a call and a record literal use. §6.
        let (substitution, generic_args) = self.instantiate_payload(&declared, &payload, args);
        let ids = args
            .iter()
            .enumerate()
            .map(|(at, arg)| match payload.get(at) {
                Some(ty) => {
                    let ty = self.apply(&substitution, *ty, arg.span);
                    let ty = self.instantiate(ty, arg.span);
                    self.check(&arg.value, ty, Site::Argument)
                }
                None => self.synth(&arg.value).id,
            })
            .collect();
        let ty = self.types.named(choice, generic_args);
        let id = self.body.push_expr(ExprKind::Call { callee, args: ids }, ty, span);
        Typed { id, ty: InferTy::Known(ty) }
    }

    /// §6's root-level instantiation, for a variant's positional payload.
    fn instantiate_payload(
        &mut self,
        declared: &[hir::GenericParam],
        payload: &[Ty],
        args: &[hir::Arg],
    ) -> (Substitution, Vec<GenericArg>) {
        if declared.is_empty() {
            return (Substitution::new(), Vec::new());
        }
        let mut solved: HashMap<DefId, Ty> = HashMap::new();
        for (at, arg) in args.iter().enumerate() {
            let Some(param_ty) = payload.get(at) else { continue };
            let TyKind::Param { def } = *self.types.kind(*param_ty) else { continue };
            if solved.contains_key(&def) || !declared.iter().any(|p| p.def == def) {
                continue;
            }
            if let Some(ty) = self.probe(&arg.value) {
                solved.insert(def, ty);
            }
        }
        let mut substitution = Substitution::new();
        let mut generic_args = Vec::with_capacity(declared.len());
        for param in declared {
            match param.kind {
                hir::GenericParamKind::Type { .. } => {
                    let ty = solved.get(&param.def).copied().unwrap_or(Ty::ERROR);
                    substitution = substitution.with_type(param.def, ty);
                    generic_args.push(GenericArg::Type(ty));
                }
                hir::GenericParamKind::Const { .. } => generic_args.push(GenericArg::Error),
            }
        }
        (substitution, generic_args)
    }

    // --- Decision 11, at a call ------------------------------------------

    /// `receiver.method(args)` — the lookup, and the signature it finds.
    ///
    /// **Decision. The receiver is synthesised first and the method is found
    /// from its type**, which is the order Decision 1 forces: a method call is
    /// a synthesis, there is no expected type to push into the receiver, and
    /// `methods`'s §1 keys the index on what the receiver's type *heads*.
    ///
    /// **Four answers, and two of them are diagnostics.** A candidate resolves,
    /// and its arguments are checked against real parameter types.
    /// [`Found::None`] over a type this crate can speak for is `SC0532`, and
    /// [`Found::Ambiguous`] is `SC0531`. [`Found::Instances`] is one method at
    /// several instantiations of one interface and goes to
    /// [`BodyChecker::select`], which resolves it or reports `SC0531` or
    /// `SC0533`. What is left — a prelude receiver, a type parameter, an
    /// already-wrong type, and the one case `methods`'s §5 names — leaves
    /// `method: None` and reports nothing, which is `ty`'s §5 and is what keeps
    /// the hole that remains from manufacturing a cascade.
    fn method_call(
        &mut self,
        receiver: &hir::Expr,
        name: &hir::Ident,
        generics: &[hir::Type],
        args: &[hir::Arg],
        span: Span,
    ) -> Typed {
        // `ConfigError.NotFound("port")`: a choice's *variant*, reached
        // through the name of its type. The parser cannot tell it from a call
        // to an associated function — both are a receiver, a name and an
        // argument list — and the resolver cannot either, because the answer
        // is which child of which definition the two names reach. This is the
        // first phase holding both, and it is the same three questions §4.4
        // hands over one construct along.
        if let Some(variant) = self.variant_receiver(receiver, name) {
            return self.call_variant(variant, args, span);
        }

        // `DefTable.new()`: the receiver names a *type*, so there is no
        // receiver value and the resolved call is an ordinary `Call` at the
        // method's definition. THIR gains no node for a receiver that is not
        // there, which is `thir`'s §4 — a `DefId` standing for *"there is no
        // answer"* is a lie a later pass can dereference.
        if let Some(ty) = self.type_receiver(receiver) {
            return self.associated_call(ty, name, generics, args, span);
        }

        let recv = self.synth(receiver);
        let recv_ty = self.known_or_error(recv.ty);
        let revealed = self.revealed(recv_ty, span);
        let self_ty = self.receiver_self_ty(revealed, span);
        let found = self.lookup(revealed, name, Form::Value, args, self_ty);

        let (candidate, supplied) = match found {
            Callee::Found(candidate, supplied) => (candidate, supplied),
            Callee::Missing(supplied) => {
                // §4 of `narrow`, as it still stands for an unresolved call:
                // with no candidate there is no `SelfKind` to read, so the
                // receiver's narrowing goes.
                if let Some(place) = self.body.place_of(recv.id) {
                    self.facts.invalidate(&place);
                }
                let args = self.argument_ids(args, supplied);
                let id = self.body.push_expr(
                    ExprKind::MethodCall { receiver: recv.id, method: None, args },
                    Ty::ERROR,
                    span,
                );
                return Typed { id, ty: InferTy::Known(Ty::ERROR) };
            }
        };

        // Decision 8, now that the question can be asked: a `mutable self`
        // method is a write to the receiver and invalidates it; every other
        // form is not and does not. `narrow`'s §4.
        //
        // **At a §6 selection the arguments were synthesised before this
        // point**, under the facts that held before the call rather than after
        // it. That is the right order for a reader — the arguments are
        // evaluated before the callee writes anything — and it is a difference
        // from every other call, where the invalidation comes first.
        if candidate.writes_receiver() {
            if let Some(place) = self.body.place_of(recv.id) {
                self.facts.invalidate(&place);
            }
        }

        let (ids, ret) = self.call_method(&candidate, self_ty, generics, args, supplied, span);
        let id = self.body.push_expr(
            ExprKind::MethodCall { receiver: recv.id, method: Some(candidate.method), args: ids },
            ret,
            span,
        );
        if self.decls.prelude().is_never(self.types, ret) {
            self.diverged = true;
        }
        Typed { id, ty: InferTy::Known(ret) }
    }

    /// `DefTable.new()` — a method reached through a type rather than a value.
    ///
    /// It becomes an [`ExprKind::Call`] at the method's own definition, because
    /// that is what it is: a function with no receiver, named by the block that
    /// declares it. The alternative — a `MethodCall` whose receiver node stands
    /// for a type — would put an expression in the tree for something the
    /// author did not evaluate, and MIR would have to know to skip it.
    fn associated_call(
        &mut self,
        receiver_ty: Ty,
        name: &hir::Ident,
        generics: &[hir::Type],
        args: &[hir::Arg],
        span: Span,
    ) -> Typed {
        let revealed = self.revealed(receiver_ty, span);
        let (candidate, supplied) =
            match self.lookup(revealed, name, Form::Type, args, revealed) {
                Callee::Found(candidate, supplied) => (candidate, supplied),
                Callee::Missing(supplied) => {
                    let ids = self.argument_ids(args, supplied);
                    let callee = self.body.push_expr(ExprKind::Error, Ty::ERROR, span);
                    let id =
                        self.body.push_expr(ExprKind::Call { callee, args: ids }, Ty::ERROR, span);
                    return Typed { id, ty: InferTy::Known(Ty::ERROR) };
                }
            };
        let callee = self.body.push_expr(ExprKind::Item(candidate.method), Ty::ERROR, span);
        let (self_ty, supplied) =
            self.receiver_arguments(revealed, &candidate, args, supplied, span);
        let (ids, ret) = self.call_method(&candidate, self_ty, generics, args, supplied, span);
        let id = self.body.push_expr(ExprKind::Call { callee, args: ids }, ret, span);
        if self.decls.prelude().is_never(self.types, ret) {
            self.diverged = true;
        }
        Typed { id, ty: InferTy::Known(ret) }
    }

    /// The receiver of an associated call, with the block's own type arguments
    /// filled in: `Box` becoming `Box of Doc` at `Box.new(doc)`.
    ///
    /// # The decision, and why it has to be taken somewhere
    ///
    /// **Decision. At an associated call whose receiver names a generic type
    /// and writes no arguments, the block's type parameters are solved by §6's
    /// root-level match against the call's arguments, and the arguments are
    /// *synthesised* to do it.**
    ///
    /// [`BodyChecker::block_substitution`] solves a block's parameters by
    /// matching the block's self type against the **receiver's**, which is the
    /// whole answer at a method call: `docs.push(d)` has a receiver whose type
    /// is `Array of Doc` and `T` is read straight off it. An associated call
    /// has no receiver value, so the only arguments its type carries are the
    /// ones the author wrote — `(Array of Int).new()`, which is how
    /// `examples/07_generics.science` spells it and why §4.3 requires the
    /// parentheses. When none were written there is nothing on the receiver's
    /// side at all, and *something* has to happen.
    ///
    /// **What used to happen is the thing this exists to stop.** The pattern
    /// `Box of T` zipped against a receiver carrying no arguments matched
    /// nothing, `T` stayed unsolved, and the block substitution left a
    /// [`TyKind::Param`] standing in the signature — so `Box.new(doc)`
    /// synthesised `Box of T`, and `doc` was checked against a bare `T` bound
    /// in `builtins.rs`. Both halves then reported, and both messages named a
    /// parameter the author cannot see: `expected Box of Doc, found Box of T`,
    /// and `expected T, found Doc`. A type with a free parameter in it is not a
    /// type, and a diagnostic that prints one is the checker leaking its table.
    ///
    /// # Why the arguments are synthesised rather than probed
    ///
    /// [`BodyChecker::instantiate_call`] solves a *callee's* generics by the
    /// same root-level match and asks [`BodyChecker::probe`] instead, because
    /// synthesising there would build every argument node twice. Here it would
    /// not: [`BodyChecker::call_method`] already takes `supplied`, which means
    /// exactly *"these arguments are synthesised already, meet their parameters
    /// at [`BodyChecker::demand`]"*, and that path exists because
    /// [`BodyChecker::select`] needed it.
    ///
    /// The probe would also not have done. It answers for a name whose type is
    /// declared and a literal whose suffix fixes it, and of the **ten
    /// `Box.new(..)` calls in `examples/` it answers for exactly one** —
    /// `19_stdlib`'s `Box.new(record)`. The other nine hand it a record literal
    /// (`Box.new(Doc(title: "a", body: "..."))`, six of them), a variant call
    /// (`Box.new(Leaf(1))`, two) or an ordinary call (`Box.new(produce())`),
    /// and it probes every one of them to `None` — which §6 turns into
    /// [`Ty::ERROR`], and `Box of ERROR` agrees with every `Box` it meets.
    /// That is the same silence this change is about, one constructor down, so
    /// the narrow answer would have looked like a fix and checked nothing.
    ///
    /// # What it costs
    ///
    /// **Decision 1's first sentence, at these call sites, exactly as
    /// [`BodyChecker::select`] already pays it**: the argument is synthesised
    /// before its parameter type is known, so checking mode does not reach into
    /// it and an `if`, a `match`, a block or a closure handed to an associated
    /// function of a generic type is synthesised rather than pushed into. It
    /// meets the solved parameter at [`BodyChecker::demand`] afterwards.
    /// Nothing crosses a function boundary and no inference variable of this
    /// body is unified with anything outside it, so the guarantee Decision 1
    /// actually rests on is untouched.
    ///
    /// **An argument with no type of its own solves nothing.** An unsuffixed
    /// literal and `null` are inference variables until a parameter expects a
    /// type of them, and the parameter here *is* the unsolved one — so
    /// `Wrapper.holding(7)` cannot be solved from its argument and is
    /// [`codes::UNINFERABLE_RECEIVER`], with the instantiation offered as the
    /// fix. That is a real narrowing of what may be written bare, and it is the
    /// direction that can be reversed: an expectation pushed into an associated
    /// call would widen it later, and no note has asked for one.
    ///
    /// **This is a solve and never a check.** A parameter the arguments do not
    /// determine becomes [`Ty::ERROR`] *and* reports, rather than becoming
    /// `Ty::ERROR` in silence; nothing here compares an argument against
    /// anything, so no diagnostic of any other code comes out of it.
    ///
    /// **With one exception, which is `ty`'s §5 and not a softening.** An
    /// argument whose synthesised type *references* an error would have solved
    /// the parameter and could not, so the parameter is filled with
    /// [`Ty::ERROR`] and **nothing is reported for it**. `examples/04_enums`'
    /// `Box.new(Leaf(1))` is the case: `Leaf` is a variant of `Tree of T`, the
    /// unsuffixed `1` probes to `None` under §6, and `Leaf(1)` therefore
    /// synthesises as `Tree of ERROR` before this function is reached. Telling
    /// that author to write `(Box of (Tree of Int)).new(..)` would be advice
    /// that does not help — the argument is still untyped afterwards — and it
    /// would blame the receiver for a hole §6 left at the literal. The silence
    /// there is owned by §6's probe, is older than this function, and is named
    /// rather than inherited.
    fn receiver_arguments(
        &mut self,
        receiver: Ty,
        candidate: &Candidate,
        args: &[hir::Arg],
        supplied: Option<Vec<Typed>>,
        span: Span,
    ) -> (Ty, Option<Vec<Typed>>) {
        let TyKind::Named { def, args: ref written } = *self.types.kind(receiver) else {
            return (receiver, supplied);
        };
        if !written.is_empty() {
            return (receiver, supplied);
        }
        // The block's parameters, and the shape they sit in on its own self
        // type: `Array of T has:` gives `[T]` and `Array of T`. A block on a
        // type with no parameters has neither and leaves through here, which is
        // every `String.new()` and every `Doc.new("scratch")` in the corpus.
        let declared: Vec<hir::GenericParam> = match self.decls.block_generics(candidate.block) {
            Some(generics) if !generics.is_empty() => generics.to_vec(),
            _ => return (receiver, supplied),
        };
        let Some(block_self) = self.decls.self_ty(candidate.block) else {
            return (receiver, supplied);
        };
        let TyKind::Named { def: owner, args: pattern } = self.types.kind(block_self).clone()
        else {
            return (receiver, supplied);
        };
        if owner != def || pattern.is_empty() {
            return (receiver, supplied);
        }

        let supplied: Vec<Typed> = match supplied {
            Some(supplied) => supplied,
            None => args.iter().map(|arg| self.synth(&arg.value)).collect(),
        };

        let mut solved: HashMap<DefId, Ty> = HashMap::new();
        // Parameters an argument would have solved, had that argument had a
        // type. They are filled with [`Ty::ERROR`] like any other unsolved
        // parameter and are the ones the diagnostic below does *not* claim.
        let mut poisoned: HashSet<DefId> = HashSet::new();
        if let Some(sig) = self.decls.signature(candidate.method) {
            let params: Vec<(DefId, Ty)> =
                sig.params.iter().map(|param| (param.def, param.ty)).collect();
            let order = self.argument_order(&params, args);
            for (at, arg) in args.iter().enumerate() {
                let Some(index) = order.get(at).copied().flatten() else { continue };
                let Some((_, param_ty)) = params.get(index).copied() else { continue };
                let Some((param, borrowed)) = self.root_param(param_ty) else { continue };
                if solved.contains_key(&param) || !declared.iter().any(|p| p.def == param) {
                    continue;
                }
                let Some(typed) = supplied.get(at).copied() else { continue };
                let InferTy::Known(ty) = self.infer.resolve(typed.ty) else { continue };
                // An erroneous argument narrows nothing: `ty`'s §5 makes it
                // agree with everything, and solving a parameter to it would
                // spread one mistake into the receiver's type.
                // [`BodyChecker::select`] declines the same argument for the
                // same reason. It also **silences** the report below, which is
                // the other half of the same rule: this argument *does* mention
                // the parameter, so the author has not left it open — the
                // checker failed to type what they wrote, and `Box.new(Leaf(1))`
                // must not be told to instantiate its receiver when the
                // instantiation would not help.
                if self.types.references_error(ty) {
                    poisoned.insert(param);
                    continue;
                }
                let ty = if borrowed { self.peel_borrow(ty, arg.span) } else { ty };
                solved.insert(param, ty);
            }
        }

        let mut filled: Vec<GenericArg> = Vec::with_capacity(pattern.len());
        let mut unsolved: Vec<String> = Vec::new();
        let mut consts = false;
        for argument in &pattern {
            match argument {
                GenericArg::Type(ty) => match *self.types.kind(*ty) {
                    TyKind::Param { def: param } => match solved.get(&param) {
                        Some(ty) => filled.push(GenericArg::Type(*ty)),
                        None => {
                            if !poisoned.contains(&param) {
                                unsolved.push(self.defs.get(param).name.clone());
                            }
                            filled.push(GenericArg::Type(Ty::ERROR));
                        }
                    },
                    // Not a bare parameter: the block wrote a concrete type
                    // into its own self type, so there is nothing to solve and
                    // nothing to report.
                    _ => filled.push(GenericArg::Type(*ty)),
                },
                // A const parameter of the block. No argument solves one —
                // `matching`'s one-variable solve wants an obligation to
                // discharge, which is `SC0262` and F1's — so the written
                // instantiation is the only spelling, and the message says so.
                GenericArg::Const(_) => {
                    consts = true;
                    filled.push(GenericArg::Error);
                }
                // Already erroneous before this function ran: `ty`'s §5 again,
                // and reporting on it would be a second diagnostic for one
                // mistake.
                GenericArg::Error => filled.push(GenericArg::Error),
            }
        }
        if !unsolved.is_empty() || consts {
            let name = self.defs.get(def).name.clone();
            let method = self.defs.get(candidate.method).name.clone();
            self.diagnostics.push(uninferable_receiver(span, &name, &method, &unsolved, consts));
        }
        (self.types.named(def, filled), Some(supplied))
    }

    /// The arguments checked against a resolved method's parameters, and the
    /// type the call has.
    ///
    /// This is [`BodyChecker::call_signature`]'s body with the callee's node
    /// removed and one substitution more: a method's signature is written in a
    /// block, so `Self`, the block's associated types and the block's own
    /// generic parameters all stand in it and all three have to be replaced
    /// before a parameter type is something a value can be compared against.
    /// Reading the signature raw is the bug that says `expected Self, found
    /// Doc` at a `-> Self` that is right.
    ///
    /// **`supplied` is `methods`'s §6 and nothing else.** At a call whose
    /// callee was chosen by its arguments the argument nodes already exist, so
    /// each one meets its parameter at [`BodyChecker::demand`] — which is
    /// `check`'s own default arm — instead of being checked from the HIR a
    /// second time. What the four forms with a genuine checking rule lose by
    /// that is §1's inward push, and [`BodyChecker::select`] prices it.
    fn call_method(
        &mut self,
        candidate: &Candidate,
        self_ty: Ty,
        generics: &[hir::Type],
        args: &[hir::Arg],
        supplied: Option<Vec<Typed>>,
        span: Span,
    ) -> (Vec<ExprId>, Ty) {
        let Some(sig) = self.decls.signature(candidate.method) else {
            let ids = self.argument_ids(args, supplied);
            return (ids, Ty::ERROR);
        };
        let params: Vec<(DefId, Ty)> =
            sig.params.iter().map(|param| (param.def, param.ty)).collect();
        let ret = sig.ret;
        let declared = sig.generics.clone();

        if params.len() != args.len() {
            self.diagnostics.push(wrong_argument_count(span, params.len(), args.len()));
        }

        let block = self.block_substitution(candidate, self_ty, span);
        let order = self.argument_order(&params, args);
        let generic = self.instantiate_call(&declared, generics, &params, args, &order, span);
        self.check_bounds(candidate.method, &generic, span);

        let mut ids: Vec<ExprId> = Vec::with_capacity(args.len());
        for (at, arg) in args.iter().enumerate() {
            let already = supplied.as_ref().and_then(|supplied| supplied.get(at).copied());
            match order[at].and_then(|index| params.get(index).copied()) {
                Some((_, param_ty)) => {
                    let ty = self.apply(&block, param_ty, arg.span);
                    let ty = self.apply(&generic, ty, arg.span);
                    let ty = self.instantiate(ty, arg.span);
                    ids.push(match already {
                        Some(typed) => self.demand(typed, ty, Site::Argument, arg.span),
                        None => self.check(&arg.value, ty, Site::Argument),
                    });
                }
                None => ids.push(match already {
                    Some(typed) => typed.id,
                    None => self.synth(&arg.value).id,
                }),
            }
        }
        let ret = self.apply(&block, ret, span);
        let ret = self.apply(&generic, ret, span);
        let ret = self.instantiate(ret, span);
        (ids, ret)
    }

    /// `Self`, the block's associated types, and the block's generics.
    ///
    /// **`Self` is the *receiver's* type and not the block's written self
    /// type**, which is the more precise of the two and the one the author can
    /// see: inside `Window of (T, const WIDTH: Int) has:` the block's is
    /// `Window of (T, WIDTH)` and the receiver's is `Window of (F32, 8)`, and a
    /// `-> Self` should report as the second.
    ///
    /// **The block's generics are solved by a root-level match** — the same
    /// rule and the same restriction `check`'s §6 states for a call's type
    /// arguments. `Window of (T, WIDTH)` against `Window of (F32, 8)` pairs the
    /// arguments positionally and takes the ones that are a bare parameter;
    /// anything deeper is left unsolved, which leaves a `TyKind::Param`
    /// standing in the signature and compares against it.
    fn block_substitution(
        &mut self,
        candidate: &Candidate,
        self_ty: Ty,
        span: Span,
    ) -> Substitution {
        let mut substitution = self.decls.body_substitution(self.defs, candidate.block);
        substitution = substitution.with_self(candidate.owner, self_ty);
        // `items`' §4a: an inherited method's signature is written in the
        // *interface's* parameters, and the *block* is what says what they are.
        // `def index(self, at: Idx)` reached through
        // `Array of T implements Index of Int:` takes an `Int`.
        //
        // **Bound here and rewritten later.** An argument written in the bound
        // may mention the block's own `T` — `Array of T implements Index of T:`
        // is a thing to write — and the receiver is what solves that `T`, which
        // this function has not read yet. So each argument is bound now, so
        // that a block with no generics at all still gets one, and kept in
        // `interface_args` so that a block with generics can have it rewritten
        // once they are solved.
        let mut interface_args: Vec<(DefId, Ty)> = Vec::new();
        for (param, arg) in self.decls.interface_arguments(candidate.block) {
            match arg {
                GenericArg::Type(ty) => {
                    substitution = substitution.with_type(param, ty);
                    interface_args.push((param, ty));
                }
                GenericArg::Const(form) => substitution = substitution.with_const(param, form),
                GenericArg::Error => {
                    substitution = substitution.with_type(param, Ty::ERROR);
                }
            }
        }
        let declared = match self.decls.block_generics(candidate.block) {
            Some(generics) if !generics.is_empty() => generics.to_vec(),
            _ => return substitution,
        };
        let Some(block_self) = self.decls.self_ty(candidate.block) else { return substitution };
        let (TyKind::Named { args: pattern, .. }, TyKind::Named { args: actual, .. }) =
            (self.types.kind(block_self).clone(), self.types.kind(self_ty).clone())
        else {
            return substitution;
        };
        // The receiver's arguments are collected twice: once into the
        // substitution being built, and once on their own. The second copy is
        // what `subst`'s `with_assocs_through` needs, and the reason it needs
        // one is below.
        let mut solved = Substitution::new();
        for (pattern, actual) in pattern.iter().zip(&actual) {
            match (pattern, actual) {
                (GenericArg::Type(pattern), GenericArg::Type(actual)) => {
                    let TyKind::Param { def } = *self.types.kind(*pattern) else { continue };
                    if declared.iter().any(|param| param.def == def) {
                        substitution = substitution.with_type(def, *actual);
                        solved = solved.with_type(def, *actual);
                    }
                }
                (GenericArg::Const(pattern), GenericArg::Const(actual)) => {
                    // The same root-level rule one kind down: a pattern that is
                    // the parameter itself — `WIDTH`, not `WIDTH + 1` — is
                    // solved by the argument. The general case is
                    // `matching::match_linear`, and it wants an obligation to
                    // discharge, which is `SC0262` and F1's.
                    let Some(def) = bare_const_param(pattern) else { continue };
                    if declared.iter().any(|param| param.def == def) {
                        substitution = substitution.with_const(def, actual.clone());
                        solved = solved.with_const(def, actual.clone());
                    }
                }
                _ => {}
            }
        }
        // **A block's associated type is written in the block's own
        // parameters**, and `subst`'s §1 makes `apply` one pass — so replacing
        // `Self.Item` with `borrowed T` finishes at that node and the `T` in it
        // survives. `Array of T implements Iterate: type Item is borrowed T` is
        // the first declaration in the language where that matters, and without
        // this line `for x in xs` over an `Array of Int` binds `x` at
        // `borrowed T`. Composing the two is safe here for the reason
        // `with_assocs_through` states: the two substitutions are at different
        // levels and cannot name each other's parameters.
        //
        // An interface's *arguments* sit at that same level — `Index of Int`
        // and `Index of T` are both things a block may write — so they take the
        // same rewrite, one line earlier and by hand, because the composition
        // above is the assoc map's and widening it would fold a substitution
        // through itself.
        for (param, ty) in interface_args {
            let rewritten = self.apply(&solved, ty, span);
            substitution = substitution.with_type(param, rewritten);
        }
        match substitution.with_assocs_through(self.types, &solved) {
            Ok(composed) => composed,
            Err(error) => {
                self.diagnostics.push(diagnostics::overflowed(error, span));
                Substitution::new()
            }
        }
    }

    /// The type `Self` means at a call: the receiver's, with a borrow taken off.
    ///
    /// A borrow is transparent to the lookup (`methods`'s §1) and it has to be
    /// transparent here too, or a method on `Doc` called through a `borrowed
    /// Doc` would substitute `Self := borrowed Doc` and every `-> Self` in the
    /// block would come back borrowed.
    fn receiver_self_ty(&mut self, receiver: Ty, span: Span) -> Ty {
        match *self.types.kind(receiver) {
            TyKind::Borrowed { inner, .. } => {
                let inner = self.revealed(inner, span);
                self.receiver_self_ty(inner, span)
            }
            _ => receiver,
        }
    }

    /// A receiver that names a choice type, and a name that is one of its
    /// variants.
    ///
    /// Before [`BodyChecker::type_receiver`], because a variant is not a method
    /// and asking the method index about it would answer `SC0532` for a
    /// construct that is not a method call at all.
    fn variant_receiver(&self, receiver: &hir::Expr, name: &hir::Ident) -> Option<DefId> {
        let hir::ExprKind::Path { res: Res::Def(def), .. } = &receiver.kind else {
            return None;
        };
        if self.defs.get(*def).kind != hir::DefKind::Choice {
            return None;
        }
        let variant = self
            .defs
            .children(*def)
            .find(|child| child.kind == hir::DefKind::Variant && child.name == name.name)?
            .id;
        // Only when the declaration table has it: `call_variant` reads the
        // payload out of that table and asserts it is there.
        self.decls.variant(variant).map(|_| variant)
    }

    /// A receiver that names a type rather than holding a value.
    ///
    /// `DefTable.new()` and `Self.new()`. The parser cannot tell this from
    /// `defs.alloc()` — both are a receiver and a name — and the resolver
    /// cannot either, because whether `DefTable` is a type is a fact about the
    /// definition it resolved to and not about the expression. This is the
    /// first phase that holds both.
    ///
    /// # `DefKind::Primitive` is in the list, and it is why most of the corpus
    /// was unchecked
    ///
    /// **Decision. A prelude type is a receiver.** The list used to be `Record
    /// | Choice | Alias | Interface | Union`, and `builtins.rs` allocates
    /// `Array`, `Map`, `Box`, `String` and `Chars` as `DefKind::Primitive` —
    /// they are §8's library types, with a representation in `science-rt` and
    /// no declaration in any `.science` file, and `Primitive` is the kind that
    /// says so. So `String.new()` was not a call through a type: it fell to the
    /// value path, [`BodyChecker::synth`] was handed a path expression naming a
    /// *type*, and the answer was [`Ty::ERROR`].
    ///
    /// **Nothing reported it**, which is the shape of the cost rather than an
    /// accident. `ty`'s §5 makes an error type agree with everything, so the
    /// call sat in whatever slot it was written into and the slot stopped being
    /// checked. `String.new()`, `(Array of T).new()`, `Map.new()` and
    /// `Box.new(x)` are how every owned collection and every heap indirection
    /// in the language is built, and they appear in fifteen of the
    /// twenty-two files in `examples/`.
    ///
    /// **The kind was never the question the lookup wanted asked.** `methods`'s
    /// §1 keys its index on *"the definition a receiver's type heads"*, and
    /// `ty`'s [`TyKind::Named`] documentation already says a checker's question
    /// about a name is *"which definition"* and never *"which kind of
    /// definition"* — the whole reason a record, a choice, an alias and a
    /// primitive share one variant there. Testing the kind here was the one
    /// place that asked the other question, and a `DefKind` this arm had not
    /// heard of was silently *"not a type"*.
    ///
    /// **What it costs is that the arm now reaches declarations that are
    /// partial.** A prelude block is a transcription of `stdlib-core.md` §9 and
    /// is not finished, so an associated function the prelude has not written
    /// down is reached and found missing. [`Methods::surface_is_closed`] is
    /// what keeps that silent — a builtin head's method set is open — so
    /// `String.bogus()` reports nothing, exactly as `text.bogus()` on a value
    /// receiver reports nothing. The silence is the same one, now reachable
    /// through one more spelling, and it closes when §9 is transcribed whole.
    ///
    /// **And it reaches a second hole one level in**, which is a generic prelude
    /// type named with no arguments: `Box.new(x)` and a bare `Array.new()`.
    /// [`BodyChecker::receiver_arguments`] is that rule and states its own
    /// decision; before this arm accepted `Primitive`, no prelude call ever got
    /// far enough to meet it.
    ///
    /// [`Methods::surface_is_closed`]: crate::methods::Methods::surface_is_closed
    fn type_receiver(&mut self, receiver: &hir::Expr) -> Option<Ty> {
        let hir::ExprKind::Path { res, generics } = &receiver.kind else { return None };
        match res {
            Res::Def(def) => {
                let def = *def;
                if !matches!(
                    self.defs.get(def).kind,
                    hir::DefKind::Record
                        | hir::DefKind::Choice
                        | hir::DefKind::Alias
                        | hir::DefKind::Interface
                        | hir::DefKind::Union
                        // §8's library types — `Array`, `Map`, `Box`, `String`,
                        // `Chars` — and every scalar. See the decision above:
                        // leaving this out made every `String.new()` in the
                        // corpus a `Ty::ERROR` that agreed with its slot.
                        | hir::DefKind::Primitive
                ) {
                    return None;
                }
                // `(Window of (Int, 4)).empty()`: the arguments are lowered as
                // *arguments* and not as types, because one of them is a const
                // expression and lowering it as a type is `SC0523` reported at
                // a receiver that is correct.
                let args: Vec<GenericArg> =
                    generics.iter().map(|ty| self.lower_generic_arg(ty)).collect();
                Some(self.types.named(def, args))
            }
            Res::SelfTy(owner) => self.decls.self_ty(*owner),
            Res::Error => None,
        }
    }

    /// Decision 11's lookup, with its diagnostics and `methods`'s §6
    /// selection.
    ///
    /// [`Callee::Missing`] is *"no answer"* and is not always a diagnostic:
    /// `methods`'s §1 distinguishes a receiver this crate can speak for from
    /// one it cannot, and only the first reports.
    fn lookup(
        &mut self,
        receiver: Ty,
        name: &hir::Ident,
        form: Form,
        args: &[hir::Arg],
        self_ty: Ty,
    ) -> Callee {
        let Some(key) = self.decls.methods().receiver(self.defs, self.types, receiver) else {
            return Callee::Missing(None);
        };
        match self.decls.methods().lookup(key, &name.name, form) {
            Found::One(candidate) => Callee::Found(candidate, None),
            Found::Ambiguous(candidates) => {
                let diagnostic = self.ambiguous(receiver, name, &candidates);
                self.diagnostics.push(diagnostic);
                Callee::Missing(None)
            }
            // The name is there and the form is not: an instance method named
            // through its type, or an associated function called on a value.
            // `methods`'s §5 — F0 has no spelling for either, so a diagnostic
            // about one would be a language decision taken by a checker.
            Found::Mismatched => Callee::Missing(None),
            // One method at several instantiations of one interface, which the
            // arguments choose between: `methods`'s §6.
            Found::Instances(candidates) => {
                self.select(receiver, name, &candidates, args, self_ty)
            }
            Found::None => {
                // `methods`' §8a: a prelude type's method set is open at the
                // names a note gives it and closed everywhere else, so
                // `text.slice(0..4)` is silent and `text.no_such_method()` is
                // not. This used to be `surface_is_closed`, which answered
                // *"builtin"* and exempted the whole standard-library surface.
                if !self.decls.methods().name_is_answerable(self.defs, key, &name.name) {
                    return Callee::Missing(None);
                }
                if !self.types.references_error(receiver) {
                    let rendered = self.types.render(self.defs, receiver);
                    self.diagnostics.push(no_such_method(name.span, &name.name, &rendered));
                }
                Callee::Missing(None)
            }
        }
    }

    /// `methods`'s §6: the argument types choose among the implementations of
    /// one interface.
    ///
    /// **Decision. The arguments are synthesised first, and a candidate
    /// survives if no argument refutes it.** Exactly one survivor resolves the
    /// call; more than one is `SC0531` under a message saying the arguments did
    /// not narrow it; none is `SC0533`, naming what was supplied and what the
    /// implementations accept. All three are answers, and what they replace is
    /// silence — a call with `method: None`, arguments compared against no
    /// signature at all, and nothing said about any of it.
    ///
    /// **The reason it terminates is that this is selection and not
    /// inference.** The candidate set is finite, it is fixed before an argument
    /// is looked at, and it comes from the receiver alone: no argument can add
    /// a candidate, and no candidate can change an argument's type, because the
    /// arguments are *synthesised* and never checked against an expectation
    /// taken from a callee that has not been chosen.
    ///
    /// # The cost, which is Decision 1's first sentence
    ///
    /// > *"every call site knows its callee's types before it looks at the
    /// > arguments"*
    ///
    /// **At these call sites that is now false**, and false in the direction
    /// the sentence was written to exclude: the argument is synthesised first,
    /// and its type is what picks the callee. That sentence is the reason §2 of
    /// the note gives for the regime being bidirectional at all, so this is not
    /// a detail of the implementation and it is not a temporary state.
    ///
    /// **What bounds it**, stated so that the next reader does not take it for
    /// more than it is:
    ///
    /// - The choice is among a **known finite set** computed from the receiver,
    ///   by a linear scan. There is no search and no backtracking: each
    ///   candidate is tested once, against types that are already in hand.
    /// - The argument types are **synthesised locally**, by the same walk any
    ///   other expression gets. Nothing from a candidate's signature flows
    ///   backwards into them, so no expectation crosses the undecided seam.
    /// - Nothing crosses a **function boundary**. Every candidate's parameter
    ///   types are annotations the declaration pass lowered, and no inference
    ///   variable of this body is unified with anything outside it.
    ///
    /// So Decision 1's actual guarantee — *"there is no unifier across function
    /// boundaries"*, and with it both dividends the note counts on: bodies that
    /// check in parallel and in any order, and an error that can name a type a
    /// human wrote. Those survive unchanged. What does not survive is the
    /// sentence above, and two things that sentence implied:
    ///
    /// - **Checking mode does not reach an argument here.** §1's forms with a
    ///   genuine checking rule — an `if`, a `match`, a block, a closure, and
    ///   the tuple arm — are synthesised instead, so the expectation that would
    ///   have been pushed into their sub-expressions is not. They meet the
    ///   chosen parameter at [`BodyChecker::demand`] afterwards, which is a
    ///   comparison and not a push.
    /// - **An argument with no type of its own cannot select.** An unsuffixed
    ///   literal and `null` are inference variables until something expects a
    ///   type of them, and that something is the parameter of the callee this
    ///   selection has not chosen yet. Such an argument refutes no candidate —
    ///   refusing on an unanswerable question is §5's false positive — so it
    ///   narrows nothing, and if more than one candidate is left the author is
    ///   told exactly that, with the literal named as the reason. It is **not**
    ///   left silent: `SC0531` with a note an author can act on is worth more
    ///   than a call nobody checked. The argument itself is still checked, at
    ///   [`BodyChecker::demand`], once a candidate has won.
    ///
    /// **An argument whose type references an error is the one case that stays
    /// silent**, and that is `ty`'s §5 rather than a hole left here: an
    /// erroneous type agrees with everything, so every candidate would survive
    /// it, and the message would be about a mistake that is already reported
    /// somewhere else.
    fn select(
        &mut self,
        receiver: Ty,
        name: &hir::Ident,
        candidates: &[Candidate],
        args: &[hir::Arg],
        self_ty: Ty,
    ) -> Callee {
        let supplied: Vec<Typed> = args.iter().map(|arg| self.synth(&arg.value)).collect();
        let Some(instances) = self.instances(candidates, self_ty, name.span) else {
            return Callee::Missing(Some(supplied));
        };
        let unanswerable = supplied.iter().any(|typed| match self.infer.resolve(typed.ty) {
            InferTy::Known(ty) => self.types.references_error(ty),
            InferTy::Var(_) => false,
        });
        if unanswerable {
            return Callee::Missing(Some(supplied));
        }

        let mut viable: Vec<usize> = Vec::new();
        for (at, instance) in instances.iter().enumerate() {
            if self.accepts(instance, args, &supplied) {
                viable.push(at);
            }
        }
        if viable.len() == 1 {
            return Callee::Found(instances[viable[0]].candidate, Some(supplied));
        }
        let reported = if viable.is_empty() {
            self.no_instance(receiver, name, &instances, &supplied)
        } else {
            let survivors: Vec<Instance> =
                viable.iter().map(|at| instances[*at].clone()).collect();
            self.undetermined(receiver, name, &survivors, &supplied)
        };
        self.diagnostics.push(reported);
        Callee::Missing(Some(supplied))
    }

    /// Every candidate with its parameters substituted, or `None` if one of
    /// them cannot be read.
    ///
    /// A candidate with no signature is *"cannot say"* and not *"takes
    /// nothing"*, and one of those makes the whole selection unanswerable:
    /// choosing among the rest would be choosing against a candidate nothing
    /// was compared to.
    ///
    /// **What the substitution does not reach is the interface's own generic
    /// parameters**, and selection inherits that rather than introducing it:
    /// [`BodyChecker::block_substitution`] solves a block's generics from the
    /// receiver and `Self` from the receiver, and nothing anywhere binds the
    /// `T` of `interface From of T:` to the `ParseError` of `LoadError
    /// implements From of ParseError:` — the arguments of an implemented
    /// interface are not lowered by any pass. It shows only where a method is
    /// *inherited* as a default body, because a block that writes the method
    /// writes concrete parameter types with it, which is every implementation
    /// in the corpus. Where it does show, one implementation already reported
    /// `SC0525` (`expected T, found Io`) before any of this existed; with two,
    /// the same hole comes out as `SC0533`. The fix is the interface's
    /// arguments in `items`'s table, and it is a declaration-pass change.
    fn instances(
        &mut self,
        candidates: &[Candidate],
        self_ty: Ty,
        span: Span,
    ) -> Option<Vec<Instance>> {
        let mut instances = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let sig = self.decls.signature(candidate.method)?;
            let declared: Vec<(DefId, Ty)> =
                sig.params.iter().map(|param| (param.def, param.ty)).collect();
            let block = self.block_substitution(candidate, self_ty, span);
            let params = declared
                .into_iter()
                .map(|(def, ty)| (def, self.substituted(&block, ty)))
                .collect();
            instances.push(Instance { candidate: *candidate, params });
        }
        Some(instances)
    }

    /// Whether this implementation could be the one called.
    ///
    /// Arity first, because a candidate that cannot take this many arguments is
    /// not the callee whatever the types say, and then one comparison per
    /// argument. **The receiver is not compared again**: it is what produced
    /// the candidate set, and every candidate in that set agrees about it.
    fn accepts(&mut self, instance: &Instance, args: &[hir::Arg], supplied: &[Typed]) -> bool {
        if instance.params.len() != args.len() {
            return false;
        }
        let order = self.argument_order(&instance.params, args);
        for (at, index) in order.iter().enumerate() {
            let Some((_, param)) = index.and_then(|index| instance.params.get(index).copied())
            else {
                return false;
            };
            // A literal or `null`: no type until something expects one, so it
            // refutes nothing.
            let InferTy::Known(found) = self.infer.resolve(supplied[at].ty) else {
                continue;
            };
            if !self.fits(found, param) {
                return false;
            }
        }
        true
    }

    /// Whether a value of this type reaches that parameter.
    ///
    /// [`assignable`] at [`Site::Argument`], which is the relation the argument
    /// will actually meet, plus §6.3's auto-borrow asked as a type question:
    /// [`BodyChecker::auto_borrow`] would write the borrow the author was told
    /// to leave out, so a parameter it reaches is a parameter this
    /// implementation accepts. Asking the first and not the second would refuse
    /// an implementation whose `borrowed` parameter the chosen call site takes
    /// happily.
    fn fits(&mut self, supplied: Ty, param: Ty) -> bool {
        let source = self.unreported_reveal(supplied);
        let target = self.unreported_reveal(param);
        let methods = self.decls.methods();
        if assignable(self.types, methods, self.coercions, Site::Argument, source, target)
            .is_some()
        {
            return true;
        }
        let TyKind::Borrowed { mutable, .. } = *self.types.kind(target) else {
            return false;
        };
        let borrowed = self.types.borrowed(mutable, source);
        let methods = self.decls.methods();
        assignable(self.types, methods, self.coercions, Site::Argument, borrowed, target).is_some()
    }

    /// [`BodyChecker::revealed`] with nothing reported.
    ///
    /// Selection asks a type question of every candidate and answers for one of
    /// them, so an overflow reported here would be reported once per candidate.
    /// It is reported once instead, at the comparison the winner's arguments go
    /// through.
    fn unreported_reveal(&mut self, ty: Ty) -> Ty {
        self.aliases.reveal(self.types, ty).unwrap_or(Ty::ERROR)
    }

    /// [`BodyChecker::apply`] with nothing reported, for the same reason.
    fn substituted(&mut self, substitution: &Substitution, ty: Ty) -> Ty {
        substitution.apply(self.types, ty).unwrap_or(Ty::ERROR)
    }

    /// The argument nodes for a call that has no callee to check them against:
    /// the ones selection already synthesised, or a synthesis of each.
    fn argument_ids(&mut self, args: &[hir::Arg], supplied: Option<Vec<Typed>>) -> Vec<ExprId> {
        match supplied {
            Some(supplied) => supplied.iter().map(|typed| typed.id).collect(),
            None => args.iter().map(|arg| self.synth(&arg.value).id).collect(),
        }
    }

    /// How an argument reads in a selection message: its type, or what kind of
    /// literal it is when it does not have one yet.
    fn described(&mut self, typed: Typed) -> String {
        match self.infer.resolve(typed.ty) {
            InferTy::Known(ty) => format!("`{}`", self.types.render(self.defs, ty)),
            InferTy::Var(var) => self.numeric_name(var).to_string(),
        }
    }

    /// The arguments as a message lists them.
    fn described_all(&mut self, supplied: &[Typed]) -> String {
        if supplied.is_empty() {
            return "no arguments".to_string();
        }
        let described: Vec<String> = supplied.iter().map(|typed| self.described(*typed)).collect();
        described.join(", ")
    }

    /// What one implementation takes, as a message lists it.
    fn described_params(&mut self, instance: &Instance) -> String {
        if instance.params.is_empty() {
            return "no arguments".to_string();
        }
        let rendered: Vec<String> = instance
            .params
            .iter()
            .map(|(_, ty)| format!("`{}`", self.types.render(self.defs, *ty)))
            .collect();
        rendered.join(", ")
    }

    /// The interface every candidate of a §6 set came through, by name. The set
    /// is *defined* by them sharing one, so the first answers for all of them.
    fn interface_name(&self, instances: &[Instance]) -> String {
        instances
            .first()
            .and_then(|instance| instance.candidate.interface())
            .map(|interface| self.defs.get(interface).name.clone())
            .unwrap_or_default()
    }

    /// Where each implementation is written, and what it takes.
    fn accepted_by(&mut self, instances: &[Instance]) -> Vec<(Span, String)> {
        instances
            .iter()
            .map(|instance| {
                (self.defs.get(instance.candidate.method).span, self.described_params(instance))
            })
            .collect()
    }

    /// `SC0531` again, for the set the arguments did not narrow. `methods`'s
    /// §6.
    ///
    /// **The same code as Decision 11's ambiguity and a different message**,
    /// because the same thing went wrong — the call names more than one method
    /// and the language has no spelling for saying which — reached by a
    /// different road. A second code would make an author learn two numbers for
    /// one sentence.
    fn undetermined(
        &mut self,
        receiver: Ty,
        name: &hir::Ident,
        viable: &[Instance],
        supplied: &[Typed],
    ) -> Diagnostic {
        let rendered = self.types.render(self.defs, receiver);
        let interface = self.interface_name(viable);
        let supplied_text = self.described_all(supplied);
        let literal =
            supplied.iter().any(|typed| matches!(self.infer.resolve(typed.ty), InferTy::Var(_)));
        let accepts = self.accepted_by(viable);
        instances_not_narrowed(
            name.span,
            &name.name,
            &rendered,
            &interface,
            &supplied_text,
            &accepts,
            literal,
        )
    }

    /// `SC0533` — the arguments fit none of the implementations.
    fn no_instance(
        &mut self,
        receiver: Ty,
        name: &hir::Ident,
        instances: &[Instance],
        supplied: &[Typed],
    ) -> Diagnostic {
        let rendered = self.types.render(self.defs, receiver);
        let interface = self.interface_name(instances);
        let supplied_text = self.described_all(supplied);
        let accepts = self.accepted_by(instances);
        no_matching_instance(name.span, &name.name, &rendered, &interface, &supplied_text, &accepts)
    }

    /// `SC0531`, built where the receiver's type and the candidates are both in
    /// hand. `methods`'s §3 is the argument for every line of it.
    fn ambiguous(&self, receiver: Ty, name: &hir::Ident, candidates: &[Candidate]) -> Diagnostic {
        let rendered = self.types.render(self.defs, receiver);
        let mut diagnostic = Diagnostic::error(
            codes::AMBIGUOUS_METHOD,
            format!("`{}` on `{rendered}` could be {} methods", name.name, candidates.len()),
        )
        .with_label(Label::primary(
            name.span,
            format!("{} implementations declare `{}`", candidates.len(), name.name),
        ));
        for candidate in candidates {
            let source =
                crate::methods::describe_source(self.defs, self.types, receiver, candidate);
            diagnostic = diagnostic.with_label(Label::secondary(
                self.defs.get(candidate.method).span,
                format!("one is declared by {source}"),
            ));
        }
        diagnostic.with_note(
            "method lookup is inherent methods and then interface methods, and an ambiguity is \
             an error and never a priority ordering — a winner picked here would be picked \
             silently, and the program would call a method its author did not mean",
        )
    }

    fn unary(&mut self, op: UnaryOp, operand: &hir::Expr, span: Span) -> Typed {
        match op {
            // `not` is not an operator interface and §4.5 of
            // `stdlib-shape-and-packages.md` says it never will be: *"`and` and
            // `or` are **not** overloadable at all — they are short-circuiting
            // control flow"*, and `Not` is the one whose method name that same
            // decision has to rename. It stays a `Bool` in and a `Bool` out.
            UnaryOp::Not => {
                let bool_ty = self.bool_ty();
                let operand = match bool_ty {
                    Some(ty) => self.check(operand, ty, Site::Elsewhere),
                    None => self.synth(operand).id,
                };
                let ty = bool_ty.unwrap_or(Ty::ERROR);
                let id = self.body.push_expr(ExprKind::Unary { op, operand }, ty, span);
                Typed { id, ty: InferTy::Known(ty) }
            }
            UnaryOp::Neg => {
                let typed = self.synth(operand);
                if let Some(dispatched) = self.operator("-", "Neg", "neg", typed, None, span) {
                    return dispatched;
                }
                let typed = self.read_value(typed, operand.span);
                let ty = typed.ty;
                self.push_typed(ExprKind::Unary { op, operand: typed.id }, ty, span)
            }
        }
    }

    fn binary(&mut self, op: BinaryOp, lhs: &hir::Expr, rhs: &hir::Expr, span: Span) -> Typed {
        match op {
            // The short-circuit pair. The right operand is checked under what
            // the left one established, which is what makes
            // `if doc? and doc.title == t` work — `narrow`'s §2.
            BinaryOp::And | BinaryOp::Or => {
                let bool_ty = self.bool_ty();
                let lhs = match bool_ty {
                    Some(ty) => self.check(lhs, ty, Site::Elsewhere),
                    None => self.synth(lhs).id,
                };
                let outcome = narrow::condition(&self.body, lhs);
                let saved = self.facts.clone();
                let gained =
                    if op == BinaryOp::And { outcome.when_true } else { outcome.when_false };
                self.facts.absorb(&gained);
                let rhs = match bool_ty {
                    Some(ty) => self.check(rhs, ty, Site::Elsewhere),
                    None => self.synth(rhs).id,
                };
                self.facts = saved;
                let ty = bool_ty.unwrap_or(Ty::ERROR);
                let id = self.body.push_expr(ExprKind::Binary { op, lhs, rhs }, ty, span);
                Typed { id, ty: InferTy::Known(ty) }
            }
            BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Le
            | BinaryOp::Ge => {
                let left = self.synth(lhs);
                let right = self.synth(rhs);
                // §6: `is` and `is not` require `Eq`, and `< > <= >=`
                // require `Ord`. Both are the *implementation* check and
                // neither is a dispatch — `implements_operand` says what that
                // buys and what it still leaves open.
                let interface = match op {
                    BinaryOp::Eq | BinaryOp::Ne => "Eq",
                    _ => "Ord",
                };
                self.implements_operand(op.as_str(), interface, left, span);
                self.compare(left, right, span);
                let ty = self.bool_ty().unwrap_or(Ty::ERROR);
                let id = self
                    .body
                    .push_expr(ExprKind::Binary { op, lhs: left.id, rhs: right.id }, ty, span);
                Typed { id, ty: InferTy::Known(ty) }
            }
            // §6, closed: an operator on a user type is the matching
            // interface's method, which is Decision 11's lookup. On the
            // prelude's numerics it is not — nothing declares `I64.add` — so
            // those fall through to the structural answer below.
            _ => {
                let left = self.synth(lhs);
                let right = self.synth(rhs);
                if let Some((interface, method)) = binary_operator(op) {
                    let dispatched = self.operator(
                        op.as_str(),
                        interface,
                        method,
                        left,
                        Some((right, rhs.span)),
                        span,
                    );
                    if let Some(dispatched) = dispatched {
                        return dispatched;
                    }
                }
                // `assign`'s §7 at an operand. `total + n` where `n` came out
                // of a `for` over an `Array of Int` is a `borrowed Int` meeting
                // an `Int`, which is `collections-and-chains.md` §4.3's
                // AMENDMENT 7 — *"`borrowed F32 + borrowed F32` is `F32`"* —
                // read as one coercion instead of as an implementation per
                // primitive per operator.
                let left = self.read_value(left, lhs.span);
                let right = self.read_value(right, rhs.span);
                let ty = match self.infer.unify(self.types, left.ty, right.ty) {
                    Ok(unified) => unified,
                    Err(_) => {
                        let (found, expected) =
                            (self.known_or_error(right.ty), self.known_or_error(left.ty));
                        self.mismatch(found, expected, span);
                        InferTy::Known(Ty::ERROR)
                    }
                };
                let ty = match ty {
                    InferTy::Known(known) if self.is_operand_type(known) => InferTy::Known(known),
                    InferTy::Known(_) => InferTy::Known(Ty::ERROR),
                    InferTy::Var(var) => InferTy::Var(var),
                };
                self.push_typed(ExprKind::Binary { op, lhs: left.id, rhs: right.id }, ty, span)
            }
        }
    }

    /// `assign`'s §7 at an operand: a shared borrow of a `Copy` type reads as
    /// the value.
    ///
    /// **This is the one place a coercion is applied without a slot to apply it
    /// to.** Everywhere else in this file [`assignable`] is asked at
    /// [`BodyChecker::demand`], against a type something else wrote down. An
    /// operand of `+` has no such type: `total + n` compares two synthesised
    /// types against each other, and if neither side is peeled the relation is
    /// never consulted at all. So the peeling is asked for by shape — the
    /// operand is a borrow — and then *answered* by the relation, which is what
    /// keeps the `Copy` question in `assign` where §7 argues it.
    ///
    /// Returns the operand unchanged wherever the relation says no, so a
    /// `borrowed String` stays a `borrowed String` and the mismatch below is
    /// reported about the types the author can see.
    fn read_value(&mut self, typed: Typed, span: Span) -> Typed {
        let InferTy::Known(ty) = self.infer.resolve(typed.ty) else { return typed };
        let source = self.revealed(ty, span);
        let TyKind::Borrowed { inner, .. } = *self.types.kind(source) else { return typed };
        let target = self.revealed(inner, span);
        // `Site::Operand` is what admits §7 for an *exclusive* borrow, and
        // this is the only caller that may: `read_value` is reached from the
        // operands of an operator and from nowhere else, which is the whole of
        // `Site::copies_exclusively`'s domain.
        let verdict = assignable(
            self.types,
            self.decls.methods(),
            self.coercions,
            Site::Operand,
            source,
            target,
        );
        if verdict != Some(Coercion::Copy) {
            return typed;
        }
        let id = self.body.push_expr(
            ExprKind::Coerce { operand: typed.id, coercion: Coercion::Copy },
            target,
            span,
        );
        Typed { id, ty: InferTy::Known(target) }
    }

    /// §6's operator dispatch: `a + b` is `a.add(b)` and `-a` is `a.neg()`.
    ///
    /// > `type-checking-and-mir.md` §9.3, on what a `python:` region buys:
    /// > *"the always-available explicit form (`a.item(k)`, `a.add(b)`)"*.
    ///
    /// **Decision. The operator fixes the method's *name* and nothing else; the
    /// operand's type and the result's are read off the implementation the
    /// receiver actually has.** The name is
    /// `stdlib-shape-and-packages.md` §4.5's Decision 4c — *"an operator
    /// trait's method takes the trait's lowercase name only when that name is
    /// free"* — and for the eight this file dispatches it is free, so `Add` is
    /// `add`, `MatMul` is `matmul` and `Index` is `index`. Nothing else about
    /// the signature is written down here, and the reason is that three
    /// separate documents disagree about what it would be:
    ///
    /// - `examples/06_traits.science` writes
    ///   `Vector2 implements Mul: def mul(self, scale: F64) -> Vector2` — an
    ///   operand that is **not** `Self`, which is the whole of scalar
    ///   multiplication.
    /// - `uncertainty.md` §11 writes `def pow(self, exponent: borrowed Self)` —
    ///   an operand taken by **borrow**.
    /// - `scientific-libraries.md` §12.4 writes a `Quantity implements Mul`
    ///   whose receiver, operand and result are three different instantiations
    ///   of one type, and says F0 cannot express it.
    ///
    /// So a prelude declaration of `def add(self, other: Self) -> Self` would
    /// be a constraint invented here and contradicted by the corpus on the
    /// first line that uses it. `builtins.rs`' `INTERFACE_DECLS` states the
    /// rule this follows — *an interface is declared with its methods only
    /// where a note gives the method's name **and its types***  — and a note
    /// gives the name and not the types, so the name lives here, beside the
    /// operator, and the types stay unwritten.
    ///
    /// **What it costs.** The interfaces are still declared with no methods, so
    /// nothing holds two implementations of `Add` in one crate to the same
    /// shape, and an `implements Add:` block that writes no `add` is
    /// [`codes::NO_OPERATOR_IMPLEMENTATION`] rather than an inherited default.
    /// Both close the day a note writes the signature down.
    ///
    /// **And the method's *own* generics are not solved here**, where
    /// [`BodyChecker::call_method`] solves them at a written call. An operator
    /// has no place to write a type argument and only one argument to infer
    /// from, so `def mul of U(self, other: U)` under an `implements Mul:` would
    /// leave `U` standing and compare the operand against a
    /// [`TyKind::Param`] — which `assign`'s rule 1 admits, so the operand is
    /// unchecked rather than wrongly refused. `scientific-libraries.md` §12.4's
    /// `Quantity implements Mul` is the program that wants it and the same
    /// section says F0 cannot express that signature at all, so nothing
    /// reachable today depends on it. The bound on such a parameter is not
    /// checked either, for the same reason: §8's `check_bounds` runs on a
    /// solved substitution and there is none.
    ///
    /// **`None` is *fall through to the structural answer***, and it is three
    /// cases: the operand's type is already erroneous, its head is one
    /// [`Methods::receiver`] cannot speak for (`methods`' §8 — every prelude
    /// numeric, which is what keeps `1 + 2` structural), or the prelude has no
    /// such interface, which is every test in this crate with no prelude.
    /// `Some` is *handled*, whether that meant a resolved call or a reported
    /// one.
    fn operator(
        &mut self,
        symbol: &str,
        interface: &'static str,
        method: &'static str,
        receiver: Typed,
        operand: Option<(Typed, Span)>,
        span: Span,
    ) -> Option<Typed> {
        let interface_def = self.decls.prelude().get(interface)?;
        let written = self.known_or_error(receiver.ty);
        let revealed = self.revealed(written, span);
        // `ty`'s §5: an erroneous operand agrees with whatever it meets, and a
        // second message about it would be a cascade.
        if self.types.references_error(revealed) {
            return None;
        }
        let head = self.decls.methods().receiver(self.defs, self.types, revealed)?;
        let self_ty = self.receiver_self_ty(revealed, span);
        let candidate = match self.decls.methods().lookup(head, method, Form::Value) {
            Found::One(candidate) if candidate.interface() == Some(interface_def) => candidate,
            // Everything else is *this type does not implement this
            // interface*: no such method, a method of that name from somewhere
            // that is not the operator's interface, or several. Reported where
            // the index can speak for the head and silent where it cannot,
            // which is `methods`' §8 and the restraint `SC0532` is under.
            _ => {
                if self.decls.methods().surface_is_closed(self.defs, head) {
                    // The *peeled* receiver, because `methods`' §1 makes a
                    // borrow transparent to the lookup and a message that said
                    // `borrowed Grid does not implement Index` would be
                    // describing a borrow the author did not write.
                    let rendered = self.types.render(self.defs, self_ty);
                    self.diagnostics.push(no_operator_implementation(
                        span, symbol, &rendered, interface,
                    ));
                    return Some(self.error_expr(span));
                }
                return None;
            }
        };
        // Decision 8 and `narrow`'s §4, as at any other call.
        if candidate.writes_receiver() {
            if let Some(place) = self.body.place_of(receiver.id) {
                self.facts.invalidate(&place);
            }
        }
        let Some(sig) = self.decls.signature(candidate.method) else {
            return Some(self.error_expr(span));
        };
        let params: Vec<Ty> = sig.params.iter().map(|param| param.ty).collect();
        let ret = sig.ret;
        let block = self.block_substitution(&candidate, self_ty, span);
        // The operator supplies exactly as many operands as it has: one for a
        // binary, none for a unary. A method under an operator interface that
        // takes a different number is `SC0527` — the same code a written call
        // gets, because it is the same mistake with the parentheses left off.
        let supplied = usize::from(operand.is_some());
        if params.len() != supplied {
            self.diagnostics.push(wrong_argument_count(span, params.len(), supplied));
        }
        let mut args = Vec::new();
        if let Some((value, operand_span)) = operand {
            args.push(match params.first().copied() {
                Some(param) => {
                    let ty = self.apply(&block, param, operand_span);
                    let ty = self.instantiate(ty, operand_span);
                    // `Site::Argument`, because it is one: §6.3's auto-borrow
                    // is what lets `a + b` reach a `def pow(self, exponent:
                    // borrowed Self)`.
                    self.demand(value, ty, Site::Argument, operand_span)
                }
                None => value.id,
            });
        }
        let ret = self.apply(&block, ret, span);
        let ret = self.instantiate(ret, span);
        let id = self.body.push_expr(
            ExprKind::MethodCall { receiver: receiver.id, method: Some(candidate.method), args },
            ret,
            span,
        );
        Some(Typed { id, ty: InferTy::Known(ret) })
    }

    /// The implementation check with no call made: `is`, `is not`, and the
    /// four order comparisons.
    ///
    /// **Decision. `a is b` requires `Eq` and stays an [`ExprKind::Binary`].**
    /// The requirement is real — `examples/06_traits.science` writes
    /// `Vector2 implements Eq: def eq(self, other: Vector2) -> Bool`, so the
    /// name and the signature are both in the corpus and neither is invented —
    /// and closing it is what `BodyChecker::compare` said it was waiting for:
    /// *"what is missing is a code and the decision behind it"*.
    ///
    /// # `a < b` requires `Ord`, and that is not the refusal being reversed
    ///
    /// **Decision. `< > <= >=` require `Ord` here, through this same check, and
    /// are still not dispatched.**
    ///
    /// Two different questions have been sharing one refusal, and this closes
    /// the half that costs nothing. §5.4 says the four order operators are
    /// `Ord`'s; `builtins.rs` already declares which prelude types implement it
    /// and a user writes `Vector implements Ord:` in their own file. So *"does
    /// this type implement `Ord`"* is answerable today, by the same
    /// [`Methods::declares`](crate::methods::Methods::declares) call `Eq` uses,
    /// and answering it refuses `p < p` on a record that implements nothing —
    /// which is a wrong program the checker used to accept in silence.
    ///
    /// **What stays refused is the *dispatch*, and it is refused for its
    /// original reason, undiminished.** Turning `a < b` into a call needs three
    /// things no note supplies: a method name, a return type — the only sane
    /// one is an `Ordering` that is not in §8's closed library and would arrive
    /// as a Level 1 type invented by an operator — and a rule for how four
    /// operators sit over one `compare`, including what `F64`'s NaN does to a
    /// total order. `binary_operator` has no `Ord` row and this change does not
    /// add one. Nothing here reads `Ord`'s methods, because `Ord` has none.
    ///
    /// **The distinction is the one this function already embodied.** It is
    /// named *the implementation check with no call made*; `Eq` has been using
    /// it since `is` closed, and `Eq`'s *method* is likewise never consulted by
    /// it. Requiring `Ord` is the same question asked about a second interface,
    /// not a decision about a third thing.
    ///
    /// **What it costs, first half.** A type that supports an ordering and has
    /// not written `implements Ord:` now fails to compile where it used to
    /// pass — but there is nothing else such a type could have meant,
    /// `examples/` writes exactly one order comparison on a non-prelude type
    /// and it is `07_generics`' `item > best` under a declared `where T: Ord`,
    /// and the type-parameter case is silent here for
    /// [`Methods::receiver`](crate::methods::Methods::receiver)'s reason. The
    /// restraint is the one the whole of §6 is under: a prelude head this index
    /// cannot speak for is silence and not a no.
    ///
    /// **What it costs, second half, and this one is the language's to pay.**
    /// The diagnostic tells the author to write `Held implements Ord:`, and an
    /// `implements` block **may not be empty** — the parser wants an indented
    /// body, `SC0100` — so the author has to put a method in it and the
    /// language has not said which. `Eq` has the same hole and hides it,
    /// because `examples/06_traits.science` supplies a spelling (`def eq(self,
    /// other: Vector2) -> Bool`) that nothing verifies either; `Ord` has no
    /// such attestation, so the author picks a name and the compiler accepts
    /// whatever it is.
    ///
    /// That is not a reason to keep accepting `p < p` on a record that
    /// implements nothing — a refusal an author can act on beats silence about
    /// a wrong program — but it is the measurement the note that closes `Ord`
    /// should start from: **the ask is one method signature, and an empty
    /// `implements` block is the other half of it.**
    ///
    /// **The node stays a `Binary` because `is not` has no method.** §5.4 makes
    /// `is` and `is not` one operator dispatching to `Eq`, and `Eq` declares
    /// one direction of it; turning `a is not b` into a call would mean
    /// emitting `not (a.eq(b))`, which is a *desugaring* — two THIR nodes where
    /// the author wrote one — and desugaring belongs to the lowering that has
    /// a `Rvalue::Not` to put it in. So this arm reports and types, and
    /// `science-mir` still receives the operator it received before.
    ///
    /// That is the one asymmetry in §6's dispatch and it is deliberate: `+`
    /// becomes `a.add(b)` because `add` *is* the operation, and `is not` does
    /// not because `eq` is only half of it.
    fn implements_operand(
        &mut self,
        symbol: &str,
        interface: &'static str,
        operand: Typed,
        span: Span,
    ) {
        let Some(interface_def) = self.decls.prelude().get(interface) else { return };
        let written = self.known_or_error(operand.ty);
        let revealed = self.revealed(written, span);
        if self.types.references_error(revealed) {
            return;
        }
        let Some(head) = self.decls.methods().receiver(self.defs, self.types, revealed) else {
            return;
        };
        if !self.decls.methods().surface_is_closed(self.defs, head) {
            return;
        }
        if self.decls.methods().declares(self.types, revealed, interface_def) {
            return;
        }
        let self_ty = self.receiver_self_ty(revealed, span);
        let rendered = self.types.render(self.defs, self_ty);
        self.diagnostics.push(no_operator_implementation(span, symbol, &rendered, interface));
    }

    /// `[a, b, c]` and `[]` — `indexing-and-array-literals.md` §3.
    ///
    /// **Decision. The literal is typed here and lowered to
    /// [`ExprKind::Error`] at that type.** Decision 10 fixes the result as
    /// `Array of T`, **always**, where `T` is the *unification* of the element
    /// types; Decision 11 lets an empty `[]` take `T` from an expectation and
    /// makes it [`codes::EMPTY_ARRAY_NO_TYPE`] when there is none; and §3.2
    /// makes elements that do not unify [`codes::ARRAY_ELEMENT_MISMATCH`].
    /// All three are here.
    ///
    /// **Why the THIR node is a hole at a known type, which is the one thing
    /// in this function that looks wrong and is not.** THIR has no
    /// array-literal variant and cannot grow one from this crate: `science-mir`
    /// matches [`crate::thir::ExprKind`] exhaustively in three places, and a
    /// new variant there is a compile error in a crate this change does not
    /// own. So the literal's *type* travels and its *shape* does not, and
    /// [`ExprKind::Error`] is what carries a value this phase declines to
    /// build. MIR turns it into `Rvalue::Error`, and `science-codegen-llvm`
    /// refuses the program **by name** at `SC0400` — measured, it refuses the
    /// local's `Array of I64` as one of §2.6's runtime containers before it
    /// ever reaches the rvalue, which is a better message than the hole would
    /// have produced. `let a be [1]` was already the smallest program that
    /// cannot be built, by that crate's own §2; what changes is that the type
    /// in the refusal is now right.
    ///
    /// **What that costs, stated plainly.** Two things.
    ///
    /// 1. **`ExprKind::Error`'s documented contract said `ty` is
    ///    [`Ty::ERROR`]**, and this is the first node that breaks it. That
    ///    contract is amended where it is written rather than worked around:
    ///    the variant now means *"a value this phase did not build"*, and
    ///    whether it knows the type is a separate question. Nothing downstream
    ///    reads the two together — MIR's arm emits `Rvalue::Error` into a
    ///    destination whose type comes from the *place*, not from the node.
    /// 2. **The element expressions are in the arena and nothing points at
    ///    them.** They are checked, they carry their types, and they are
    ///    unreachable from the root. That is deliberate: dropping them would
    ///    lose the narrowing invalidations and the [`crate::unchecked`] readers
    ///    that a walk over [`crate::thir::Body::exprs`] finds, and `f"{err}"`
    ///    inside a literal is a reader whether or not MIR ever visits it.
    ///
    /// **The bidirectional half is `check`'s arm and not this one.** An
    /// expectation of `Array of E` pushes `E` into every element through
    /// [`BodyChecker::check`], so `let xs: Array of String be [1, 2, 3]`
    /// reports [`codes::MISMATCHED_TYPES`] against the annotation — Decision
    /// 1's shape, one expected type from one annotation — and this function's
    /// [`codes::ARRAY_ELEMENT_MISMATCH`] is left for the case it is named for,
    /// which is elements disagreeing with *each other*.
    fn array_lit(&mut self, elements: &[hir::Expr], span: Span) -> Typed {
        if elements.is_empty() {
            // Decision 11's failure case. Reaching this function at all means
            // `check`'s arm found no `Array of E` to push inward, so there is
            // no expected element type whatever the surrounding annotation
            // said.
            self.diagnostics.push(empty_array_no_type(span));
            return self.error_expr(span);
        }
        // The element nodes go into the arena and nothing keeps their ids: see
        // the second cost above. They are reachable by a walk and not by a
        // pointer, which is what the missing THIR variant costs.
        //
        // The type so far, and the element that last made it concrete — §7.2's
        // *"secondary on the element that fixed the type"*. It starts at the
        // first element whether or not that element knows its own type, so a
        // literal-only array points at its first entry.
        let mut settled: Option<(InferTy, Span)> = None;
        for element in elements {
            let typed = self.synth(element);
            let Some((so_far, fixed_at)) = settled else {
                settled = Some((typed.ty, element.span));
                continue;
            };
            match self.unify_element(so_far, typed.ty, element.span) {
                Some(joined) => {
                    // A class that was a hole and is now a type was fixed
                    // *here*, so this is the element a later disagreement
                    // should point at.
                    let fixed_at = match (so_far, joined) {
                        (InferTy::Var(_), InferTy::Known(_)) => element.span,
                        _ => fixed_at,
                    };
                    settled = Some((joined, fixed_at));
                }
                None => {
                    let found = self.render_infer(typed.ty);
                    let expected = self.render_infer(so_far);
                    self.diagnostics.push(array_element_mismatch(
                        element.span,
                        &expected,
                        &found,
                        fixed_at,
                    ));
                    // The type so far is kept, so a third element disagreeing
                    // with the same first one reports against the first and
                    // not against the wreckage of the second.
                }
            }
        }
        let element_ty = match settled.map(|(ty, _)| ty) {
            Some(InferTy::Known(ty)) => ty,
            // Decision 2's default, run here rather than at
            // [`BodyChecker::finish`], for the reason `demand`'s §5b runs it
            // early one construct over: `infer`'s §2 has no `Array of ?0` to
            // intern, so the element type has to be a `Ty` *now* or the
            // literal has no type at all. An unsuffixed integer literal is
            // `I64` and an unsuffixed float is `F64`, which is §3.2's own
            // sentence about a literal array.
            Some(InferTy::Var(var)) => match self.default_of(var) {
                Some(ty) => {
                    let _ = self.infer.bind(self.types, var, ty);
                    ty
                }
                // `[null]`: a class with no default. `finish` reports
                // `SC0526` for it, so this says nothing and takes the error
                // type, and `ty`'s §5 keeps the count at one.
                None => Ty::ERROR,
            },
            None => Ty::ERROR,
        };
        let ty = self.array_of(element_ty);
        let id = self.body.push_expr(ExprKind::Error, ty, span);
        Typed { id, ty: InferTy::Known(ty) }
    }

    /// `[a, b, c]` with `Array of E` expected: §3.3's bounded push inward.
    ///
    /// Every element is [`BodyChecker::check`]ed at `E`, so a wrong element
    /// reports [`codes::MISMATCHED_TYPES`] against the annotation and an empty
    /// literal reports nothing at all. `Site::Elsewhere` because a literal is
    /// not a call and not a return: Decision 14's boxing has no position here,
    /// and an element that would need it is a mismatch the author fixes by
    /// writing the box.
    fn array_lit_expecting(
        &mut self,
        elements: &[hir::Expr],
        element_ty: Ty,
        expected: Ty,
        span: Span,
    ) -> ExprId {
        for element in elements {
            self.check(element, element_ty, Site::Elsewhere);
        }
        self.body.push_expr(ExprKind::Error, expected, span)
    }

    /// What an `Array of T` holds, when the type is one.
    ///
    /// The head has to be the **prelude's** `Array` — a user is free to declare
    /// a type of that name — which is why `Array` is on `items`' `WANTED` list
    /// rather than found by string comparison here.
    fn array_element(&mut self, ty: Ty) -> Option<Ty> {
        let array = self.decls.prelude().get("Array")?;
        let TyKind::Named { def, args } = self.types.kind(ty) else {
            return None;
        };
        if *def != array || args.len() != 1 {
            return None;
        }
        match args[0] {
            GenericArg::Type(element) => Some(element),
            _ => None,
        }
    }

    /// `Array of T`, interned. [`Ty::ERROR`] where there is no prelude to name
    /// `Array` with, which is `Prelude::is_available`'s admission.
    fn array_of(&mut self, element: Ty) -> Ty {
        match self.decls.prelude().get("Array") {
            Some(array) => self.types.named(array, vec![GenericArg::Type(element)]),
            None => Ty::ERROR,
        }
    }

    /// Two element types made one. §3.1's *"the unification of the element
    /// types"*, and `None` where they do not unify.
    ///
    /// **Unification and not assignability**, which is `infer`'s §5 taken at
    /// its word: there is no slot here, so there is no site, so there is
    /// nothing for a coercion to be a fact about. `[doc, excerpt]` is not an
    /// `Array of any Summarize` and Decision 12 of `assign` is not consulted.
    ///
    /// The two literal cases are the ones [`Inference::unify`] cannot answer on
    /// its own, because a numeric literal's *kind* lives in this checker's side
    /// table and not in the classes:
    ///
    /// - **A literal against a type.** [`Inference::bind`] would accept
    ///   anything, so `["a", 1]` would bind the integer's class to `String`
    ///   and report nothing. [`BodyChecker::literal_admits`] is §5's question
    ///   and is asked first.
    /// - **A literal against a literal.** Two unbound classes union with no
    ///   complaint, so `[1, 2.0]` would join and then default to whichever
    ///   kind `widen_numeric` preferred. §3.2 names that exact array as
    ///   `SC0281` — *"`Int` and `F64` do not unify"* — and the kinds are what
    ///   say so.
    fn unify_element(&mut self, so_far: InferTy, found: InferTy, span: Span) -> Option<InferTy> {
        match (so_far, found) {
            (InferTy::Var(left), InferTy::Var(right)) => {
                if let (Some(a), Some(b)) = (self.literal_kind(left), self.literal_kind(right)) {
                    if a != b {
                        return None;
                    }
                }
                self.infer.unify(self.types, so_far, found).ok()
            }
            (InferTy::Var(var), InferTy::Known(ty))
            | (InferTy::Known(ty), InferTy::Var(var)) => {
                if !self.element_admits(var, ty, span) {
                    return None;
                }
                self.infer.bind(self.types, var, ty).ok().map(InferTy::Known)
            }
            _ => self.infer.unify(self.types, so_far, found).ok(),
        }
    }

    /// [`BodyChecker::literal_admits`] with `null`'s case answered rather than
    /// refused.
    ///
    /// `demand` peels a nullable off the slot before it asks, so by the time
    /// that function sees a `Numeric::Null` the answer is an unconditional no.
    /// There is no slot here: `[null, found]` takes its type *from* `found`, so
    /// the question is whether `found`'s type is one `null` inhabits, which is
    /// §5's rule asked in the only direction this construct has.
    fn element_admits(&mut self, var: InferVar, ty: Ty, span: Span) -> bool {
        if self.literal_kind(var) != Some(Numeric::Null) {
            return self.literal_admits(var, ty, span);
        }
        if !self.decls.prelude().is_available() {
            return true;
        }
        let revealed = self.revealed(ty, span);
        matches!(self.types.kind(revealed), TyKind::Nullable(_)) || is_opaque(self.types, revealed)
    }

    /// Decision 2's default for a literal's class, when it has one.
    fn default_of(&mut self, var: InferVar) -> Option<Ty> {
        match self.literal_kind(var)? {
            Numeric::Integer => self.decls.prelude().default_int(self.types),
            Numeric::Float => self.decls.prelude().default_float(self.types),
            Numeric::Null => None,
        }
    }

    /// A type in progress, rendered for a message.
    ///
    /// A class that is still a hole is named by what it came from —
    /// [`BodyChecker::numeric_name`]'s wording, which is what `demand` prints
    /// in the same situation — so `[1, 2.0]` reads *"expected an integer
    /// literal, found a floating-point literal"* rather than naming two
    /// defaults the author did not write.
    fn render_infer(&mut self, ty: InferTy) -> String {
        match self.infer.resolve(ty) {
            InferTy::Known(known) => format!("`{}`", self.types.render(self.defs, known)),
            InferTy::Var(var) => self.numeric_name(var).to_string(),
        }
    }

    /// `a[i]` — §6's last hole, and the one that keeps its node.
    ///
    /// **Decision. Indexing dispatches to `Index.index` for its *type*, and the
    /// THIR node stays an [`ExprKind::Index`].** `indexing-and-array-literals.md`
    /// §1.1's Decision 2 declares
    /// `interface Index of Idx: type Output; def index(self, at: Idx) ->
    /// borrowed Self.Output`, and the same section makes `a[i] be v` write
    /// through the index — so `a[i]` is a **place**, and a `MethodCall` is not
    /// one. `science-mir` already lowers this node to a `Projection::Index`;
    /// replacing it with a call would delete the only place projection the IR
    /// has, in another crate, to fix a type.
    ///
    /// **So this closes the type and not the lowering**, and that is the honest
    /// extent of it: `element_ty` in `science-mir` still reads the first
    /// generic argument of the base and is right only for an `Array`. What
    /// changes here is that `a[i]` on a user type has the type its `index`
    /// declares instead of [`Ty::ERROR`], that the index operand is checked
    /// against `Idx`, and that a user type implementing no `Index` is reported.
    ///
    /// **`Index` now takes its parameter and declares its method**, which
    /// `builtins.rs`' amendment is about: `interface Index of Idx: type Output;
    /// def index(self, at: Idx) -> borrowed Self.Output`, with
    /// `Array of T implements Index of Int: type Output is T` beside it. So
    /// `xs[0]` on an `Array of I64` is a `borrowed I64` where it was
    /// [`Ty::ERROR`], and a wrong element type is reported instead of agreeing
    /// with whatever it met. Nothing here reads `Idx`: the operand's type comes
    /// off the implementation's own `index`, exactly as `+`'s does.
    ///
    /// # The read and the write are different interfaces, and different types
    ///
    /// **Decision. `a[i]` reads through [`Indexing::Read`] and is the
    /// *reference* `index` returns; `a[i] be v` writes through
    /// [`Indexing::Write`] and its slot is what `index_mutably`'s reference
    /// *points at*.** §1.1's Decision 2 is two interfaces —
    /// `IndexMutably.index_mutably(mutable self, at: Idx) -> mutable borrowed
    /// Self.Output` — and the asymmetry is forced by the two positions rather
    /// than chosen:
    ///
    /// - In a **read**, keeping the borrow is what makes `total + a[i]` work at
    ///   all: `assign`'s §7 reads a value out of a borrow of a `Copy` type, and
    ///   `tests/operators.rs` pins the `Coercion::Copy` that produces. Peeling
    ///   here would take that node away, and would make the borrow invisible
    ///   for a `T` that is *not* `Copy`, where it is the whole of what `a[i]`
    ///   costs.
    /// - In a **write**, `values[i] be values[i] * factor` — §1.1's own example
    ///   — has an `F64` on its right. A slot of `mutable borrowed F64` refuses
    ///   it, and no rule in the language turns a value into an exclusive borrow
    ///   of itself. The value goes *through* the reference, so the slot is the
    ///   referent.
    ///
    /// **A read-only container is refused the write**, which is Decision 2's
    /// content: `IndexMutably` is what `a[i] be v` looks up, and its absence is
    /// [`codes::NO_OPERATOR_IMPLEMENTATION`] naming `IndexMutably` rather than
    /// a sentence about mutability in the abstract. No new code is allocated:
    /// `SC0535` already says *"this type does not implement the interface this
    /// operator dispatches to"*, and that is what happened.
    ///
    /// **What it costs.** Three things, and each is a stated seam rather than a
    /// silence closed badly.
    ///
    /// 1. **Only an index chain is a write.** `a[i] be v` and `a[i][j] be v`
    ///    reach `IndexMutably`; `a[i].field be v` does not, because that target
    ///    is a `Field` whose base this function never sees. The inner read is
    ///    still typed and still checked — it is the *interface* that is the
    ///    shared one. Closing it means threading the write through
    ///    [`BodyChecker::field`] as well, which is a place-mutability walk and
    ///    not a type.
    /// 2. **`science-mir`'s `element_ty` still reads the base's first type
    ///    argument**, so the place it builds for `a[i]` is `T` where this node
    ///    says `borrowed T`. That disagreement predates this change — the
    ///    `Grid implements Index` fixture has it today — and the corpus
    ///    contains no `[`, so nothing measures it. It is `science-mir`'s to
    ///    close, with Decision 11's lookup it now has a reason to run.
    /// 3. **No index obligation is discharged and `SC0286` is not emitted.**
    ///    §1.3's four rules, and the warning that names what the compiler could
    ///    not prove, are a pass over a bounds fact and not a type.
    fn index_expr(
        &mut self,
        base: &hir::Expr,
        index: &hir::Expr,
        span: Span,
        indexing: Indexing,
    ) -> Typed {
        // An index chain under a write is a write at every link: `a[i][j] be v`
        // writes into the row `a[i]` denotes, so that row is reached through
        // `IndexMutably` too.
        let base = match (&base.kind, indexing) {
            (hir::ExprKind::Index { base: inner, index: at }, Indexing::Write) => {
                self.index_expr(inner, at, base.span, Indexing::Write)
            }
            _ => self.synth(base),
        };
        // §2, and [`codes::RANGE_INDEX_NEEDS_SLICE`] is the argument. A range
        // in an index bracket is a *slice*, the type it produces does not
        // exist, and the range expression's own [`Ty::ERROR`] would otherwise
        // agree with whatever `Index.index` declared its parameter to be —
        // handing back one element for an expression that asked for several.
        // Reported before the lookup, because the lookup is not what failed.
        let slicing = matches!(index.kind, hir::ExprKind::Range { .. });
        let index = self.synth(index);
        if slicing {
            let written = self.known_or_error(base.ty);
            let revealed = self.revealed(written, span);
            if !self.types.references_error(revealed) {
                let rendered = self.types.render(self.defs, revealed);
                self.diagnostics.push(range_index_needs_slice(span, &rendered));
            }
            let id = self
                .body
                .push_expr(ExprKind::Index { base: base.id, index: index.id }, Ty::ERROR, span);
            return Typed { id, ty: InferTy::Known(Ty::ERROR) };
        }
        let mut ty = Ty::ERROR;
        let mut index_id = index.id;
        let written = self.known_or_error(base.ty);
        let revealed = self.revealed(written, span);
        let (interface_name, method_name) = indexing.dispatch();
        let interface = self.decls.prelude().get(interface_name);
        let head = if self.types.references_error(revealed) {
            None
        } else {
            self.decls.methods().receiver(self.defs, self.types, revealed)
        };
        if let (Some(interface), Some(head)) = (interface, head) {
            let self_ty = self.receiver_self_ty(revealed, span);
            match self.decls.methods().lookup(head, method_name, Form::Value) {
                Found::One(candidate) if candidate.interface() == Some(interface) => {
                    // Decision 8 and `narrow`'s §4, as at any other call:
                    // `index_mutably` takes `mutable self`.
                    if candidate.writes_receiver() {
                        if let Some(place) = self.body.place_of(base.id) {
                            self.facts.invalidate(&place);
                        }
                    }
                    if let Some(sig) = self.decls.signature(candidate.method) {
                        let param = sig.params.first().map(|param| param.ty);
                        let ret = sig.ret;
                        let block = self.block_substitution(&candidate, self_ty, span);
                        if let Some(param) = param {
                            let want = self.apply(&block, param, span);
                            let want = self.instantiate(want, span);
                            index_id = self.demand(index, want, Site::Argument, span);
                        }
                        let ret = self.apply(&block, ret, span);
                        let ret = self.instantiate(ret, span);
                        ty = match indexing {
                            Indexing::Read => ret,
                            Indexing::Write => self.referent(ret, span),
                        };
                    }
                }
                _ => {
                    if self.decls.methods().surface_is_closed(self.defs, head) {
                        let rendered = self.types.render(self.defs, self_ty);
                        self.diagnostics.push(no_operator_implementation(
                            span,
                            "[]",
                            &rendered,
                            interface_name,
                        ));
                    }
                }
            }
        }
        let id =
            self.body.push_expr(ExprKind::Index { base: base.id, index: index_id }, ty, span);
        Typed { id, ty: InferTy::Known(ty) }
    }

    /// What one borrow points at, or the type unchanged when it is not one.
    ///
    /// Unlike [`BodyChecker::peel_borrow`] this takes exactly one layer off:
    /// `index_mutably`'s return is `mutable borrowed Self.Output`, the slot
    /// `a[i] be v` offers is `Output`, and `Output` may itself be a borrow if
    /// that is what the container holds.
    fn referent(&mut self, ty: Ty, span: Span) -> Ty {
        let revealed = self.revealed(ty, span);
        match *self.types.kind(revealed) {
            TyKind::Borrowed { inner, .. } => inner,
            _ => revealed,
        }
    }

    /// Whether the two sides of a comparison are the same thing.
    ///
    /// **Compared through a borrow, and neither side is coerced.** §5.4 makes
    /// `a is b` and `a == b` one operator dispatching to `Eq`, whose method
    /// takes `borrowed self` and a borrowed argument — so at the call site
    /// §6.3's auto-borrow means the author writes neither `borrowed`, and
    /// `name is ""` compares a `borrowed String` with a `String` in the source
    /// and two `String`s in the call. Demanding that the two *written* types
    /// agree reports on the borrow the language told the author to leave out.
    ///
    /// **What this phase can still say** is that the two values are the same
    /// type once those borrows are stripped. What it still does not say is
    /// whether the type implements `Eq` at all, and that is no longer for want
    /// of a lookup: [`Methods::implements`](crate::methods::Methods::implements)
    /// would answer it. What is missing is a *code* and the decision behind it
    /// — whether comparing two records that implement nothing is an error, and
    /// what it is called — which §5.4 leaves to the interface list and §13
    /// does not number. So a comparison of two records is still silent, and
    /// the blocker is now a diagnostic nobody has specified rather than a
    /// question nobody could ask.
    fn compare(&mut self, left: Typed, right: Typed, span: Span) {
        match (left.ty, right.ty) {
            (InferTy::Known(l), InferTy::Known(r)) => {
                let (peeled_left, peeled_right) =
                    (self.peel_borrow(l, span), self.peel_borrow(r, span));
                if !self.types.compatible(peeled_left, peeled_right) {
                    self.mismatch(r, l, span);
                }
            }
            (InferTy::Var(var), InferTy::Known(known))
            | (InferTy::Known(known), InferTy::Var(var)) => {
                let peeled = self.peel_borrow(known, span);
                if self.literal_admits(var, peeled, span) {
                    let _ = self.infer.bind(self.types, var, peeled);
                } else {
                    let found = self.numeric_name(var);
                    let expected = self.types.render(self.defs, known);
                    self.diagnostics.push(mismatched_types(span, &expected, found));
                }
            }
            (InferTy::Var(_), InferTy::Var(_)) => {
                let _ = self.infer.unify(self.types, left.ty, right.ty);
            }
        }
    }

    /// The type behind however many borrows, revealed at each step.
    fn peel_borrow(&mut self, ty: Ty, span: Span) -> Ty {
        let mut current = self.revealed(ty, span);
        while let TyKind::Borrowed { inner, .. } = *self.types.kind(current) {
            current = self.revealed(inner, span);
        }
        current
    }

    /// Whether an operator's result type is one this phase can name.
    ///
    /// A prelude numeric, or a type it cannot classify — a parameter, `Self`,
    /// an already-erroneous type. A record is not: `a + b` on one is the `Add`
    /// interface and §6 refuses to guess.
    fn is_operand_type(&self, ty: Ty) -> bool {
        self.types.references_error(ty)
            || !self.decls.prelude().is_available()
            || self.decls.prelude().is_numeric(self.types, ty)
            || !is_prelude_named(self.types, ty)
    }

    fn present(&mut self, inner: &hir::Expr, span: Span) -> Typed {
        let typed = self.synth(inner);
        let inner_ty = self.known_or_error(typed.ty);
        let revealed = self.revealed(inner_ty, span);
        if !matches!(self.types.kind(revealed), TyKind::Nullable(_))
            && !self.types.references_error(revealed)
            && is_prelude_named(self.types, revealed)
        {
            let rendered = self.types.render(self.defs, inner_ty);
            self.diagnostics.push(not_nullable(span, &rendered));
        }
        let ty = self.bool_ty().unwrap_or(Ty::ERROR);
        let id = self.body.push_expr(ExprKind::Present(typed.id), ty, span);
        Typed { id, ty: InferTy::Known(ty) }
    }

    fn closure(
        &mut self,
        param: DefId,
        body: &hir::Expr,
        span: Span,
        expected: Option<Ty>,
    ) -> Typed {
        let expected = expected.map(|ty| self.revealed(ty, span));
        let (param_ty, ret) = match expected.map(|ty| self.types.kind(ty).clone()) {
            Some(TyKind::Closure { params, ret }) => (params.first().copied(), Some(ret)),
            // §6: a closure in synthesis mode has no parameter type to take.
            _ => (None, None),
        };
        // `x giving x * 2` binds its parameter the way a pattern does, and
        // there is no place in that syntax for `mutable`.
        self.bind_local(param, InferTy::Known(param_ty.unwrap_or(Ty::ERROR)), BoundAs::Parameter);
        let body_id = match ret {
            Some(ret) => self.check(body, ret, Site::Return),
            None => self.synth(body).id,
        };
        let body_ty = self.body.ty(body_id);
        let ty = match (param_ty, ret) {
            (Some(param_ty), Some(ret)) => self.types.closure(vec![param_ty], ret),
            _ => self.types.closure(vec![param_ty.unwrap_or(Ty::ERROR)], body_ty),
        };
        let id = self.body.push_expr(ExprKind::Closure { param, body: body_id }, ty, span);
        Typed { id, ty: InferTy::Known(ty) }
    }

    // --- control flow, which is where narrowing happens -------------------

    fn if_expr(
        &mut self,
        if_expr: &hir::IfExpr,
        span: Span,
        expected: Option<(Ty, Site)>,
    ) -> Typed {
        let bool_ty = self.bool_ty();
        let cond = match bool_ty {
            Some(ty) => self.check(&if_expr.cond, ty, Site::Elsewhere),
            None => self.synth(&if_expr.cond).id,
        };
        let outcome = narrow::condition(&self.body, cond);
        let entry = self.facts.clone();

        self.facts = entry.clone();
        self.facts.absorb(&outcome.when_true);
        self.diverged = false;
        let then_block = self.body.reserve_block(if_expr.then_branch.span);
        let filled = self.block(&if_expr.then_branch, expected);
        let then_ty = filled.tail.map(|tail| self.body.ty(tail)).unwrap_or(Ty::UNIT);
        self.body.fill_block(then_block, filled);
        let then_diverges = self.diverged;
        let then_facts = std::mem::take(&mut self.facts);

        let (else_branch, else_ty, else_diverges, else_facts) = match &if_expr.else_branch {
            Some(otherwise) => {
                self.facts = entry;
                self.facts.absorb(&outcome.when_false);
                self.diverged = false;
                let id = match expected {
                    Some((ty, site)) => self.check(otherwise, ty, site),
                    None => self.synth(otherwise).id,
                };
                let ty = self.body.ty(id);
                (Some(id), ty, self.diverged, std::mem::take(&mut self.facts))
            }
            None => {
                let mut facts = entry;
                facts.absorb(&outcome.when_false);
                (None, Ty::UNIT, false, facts)
            }
        };

        // §4.2's third rule: a diverging branch contributes nothing, which is
        // what leaves `if err?: return err` with `err` known null afterwards.
        self.facts = Facts::join(then_facts, then_diverges, else_facts, else_diverges);
        self.diverged = then_diverges && else_diverges;

        let ty = match expected {
            Some((ty, _)) => ty,
            None if else_branch.is_some() && !then_diverges => then_ty,
            None if else_branch.is_some() => else_ty,
            None => Ty::UNIT,
        };
        let id = self.body.push_expr(
            ExprKind::If { cond, then_branch: then_block, else_branch },
            ty,
            span,
        );
        Typed { id, ty: InferTy::Known(ty) }
    }

    fn match_expr(
        &mut self,
        match_expr: &hir::MatchExpr,
        span: Span,
        expected: Option<(Ty, Site)>,
    ) -> Typed {
        let scrutinee = self.synth(&match_expr.scrutinee);
        let scrutinee_ty = self.known_or_error(scrutinee.ty);
        let entry = self.facts.clone();
        let mut arms = Vec::with_capacity(match_expr.arms.len());
        let mut arm_ty = None;
        let mut all_diverge = !match_expr.arms.is_empty();
        let mut joined: Option<Facts> = None;
        for arm in &match_expr.arms {
            self.facts = entry.clone();
            self.diverged = false;
            let pattern = self.pattern(&arm.pattern, scrutinee_ty);
            let body = match expected {
                Some((ty, site)) => self.check(&arm.body, ty, site),
                None => self.synth(&arm.body).id,
            };
            if arm_ty.is_none() {
                arm_ty = Some(self.body.ty(body));
            }
            if !self.diverged {
                all_diverge = false;
                let facts = self.facts.clone();
                joined = Some(match joined {
                    Some(existing) => existing.meet(&facts),
                    None => facts,
                });
            }
            arms.push(thir::Arm { pattern, body, span: arm.span });
        }
        self.facts = joined.unwrap_or(entry);
        self.diverged = all_diverge;
        let ty = expected.map(|(ty, _)| ty).or(arm_ty).unwrap_or(Ty::UNIT);
        let id = self
            .body
            .push_expr(ExprKind::Match { scrutinee: scrutinee.id, arms }, ty, span);
        Typed { id, ty: InferTy::Known(ty) }
    }

    fn loop_expr(&mut self, body: &hir::Block, span: Span) -> Typed {
        // §3 of `narrow`: the meet at the loop head, computed by subtracting
        // before the body is entered.
        for root in narrow::clobbered_roots(body) {
            self.facts.invalidate_root(root);
        }
        let entry = self.facts.clone();
        self.breaks.push(false);
        let block = self.body.reserve_block(body.span);
        let filled = self.block(body, None);
        self.body.fill_block(block, filled);
        let saw_break = self.breaks.pop().unwrap_or(true);
        // Facts the body establishes do not survive the back edge. §3.
        self.facts = entry;
        self.diverged = !saw_break;
        // §6: a `loop` has no value.
        let id = self.body.push_expr(ExprKind::Loop { body: block }, Ty::UNIT, span);
        Typed { id, ty: InferTy::Known(Ty::UNIT) }
    }

    fn for_expr(
        &mut self,
        pattern: &hir::Pattern,
        iter: &hir::Expr,
        body: &hir::Block,
        span: Span,
    ) -> Typed {
        let iter = self.synth(iter).id;
        for root in narrow::clobbered_roots(body) {
            self.facts.invalidate_root(root);
        }
        let entry = self.facts.clone();
        // §6, narrowed: the prelude now declares `Iterate` with `Item` and
        // `next`, so a loop over something that implements it binds at a real
        // type. Over anything else — an `Array`, a `Map`, a user type with no
        // `implements Iterate:` — there is still nothing to read and the
        // binding is [`Ty::ERROR`].
        let element = self.iterate_item(iter, span).unwrap_or(Ty::ERROR);
        let pattern = self.pattern(pattern, element);
        self.breaks.push(false);
        let block = self.body.reserve_block(body.span);
        let filled = self.block(body, None);
        self.body.fill_block(block, filled);
        self.breaks.pop();
        self.facts = entry;
        // A `for` may iterate zero times, so it never diverges.
        self.diverged = false;
        let id = self.body.push_expr(ExprKind::For { pattern, iter, body: block }, Ty::UNIT, span);
        Typed { id, ty: InferTy::Known(Ty::UNIT) }
    }

    /// The element type a `for` binds, read off the subject's implementation
    /// of the prelude's `Iterate`.
    ///
    /// **Decision. The element type is `Iterate.next`'s return with its `?`
    /// removed, and nothing else is consulted.** `next` is declared
    /// `def next(mutable self) -> Self.Item?`, so the loop's binding is the
    /// associated type the implementation answered `Item` with, after the same
    /// `block_substitution` an ordinary method call runs. Reading `Item`
    /// directly out of the block would give the same answer today and a
    /// different one the moment an implementation's `next` narrows its return;
    /// the method's signature is the thing a caller is held to.
    ///
    /// **`None` is every case this cannot answer**, and there are three that
    /// matter: the subject's head has no `Iterate` implementation in the index
    /// (`Array` and `Map` are the ones the corpus writes — see the report),
    /// the subject is a type parameter or a tuple, and the candidate named
    /// `next` came from somewhere other than `Iterate`. Each leaves the binding
    /// at [`Ty::ERROR`], which is where every `for` in the language used to be.
    fn iterate_item(&mut self, iter: ExprId, span: Span) -> Option<Ty> {
        let iterate = self.decls.prelude().get("Iterate")?;
        let ty = self.body.ty(iter);
        let revealed = self.revealed(ty, span);
        let self_ty = self.receiver_self_ty(revealed, span);
        let key = self.decls.methods().receiver(self.defs, self.types, revealed)?;
        let Found::One(candidate) = self.decls.methods().lookup(key, "next", Form::Value) else {
            return None;
        };
        if candidate.interface() != Some(iterate) {
            return None;
        }
        let ret = self.decls.signature(candidate.method)?.ret;
        let substitution = self.block_substitution(&candidate, self_ty, span);
        let ret = self.apply(&substitution, ret, span);
        match *self.types.kind(ret) {
            TyKind::Nullable(inner) => Some(inner),
            _ => None,
        }
    }

    fn block(&mut self, block: &hir::Block, expected: Option<(Ty, Site)>) -> Block {
        let mut stmts = Vec::with_capacity(block.stmts.len());
        for stmt in &block.stmts {
            let kind = self.stmt(stmt);
            stmts.push(Stmt { kind, span: stmt.span });
        }
        let tail = block.tail.as_ref().map(|tail| match expected {
            Some((ty, site)) => self.check(tail, ty, site),
            None => self.synth(tail).id,
        });
        // A block with no tail evaluates to `()`. A body whose signature says
        // otherwise and that has not already left is a mismatch, and it is
        // reported here because it is the one place both facts are in hand.
        if tail.is_none() && !self.diverged {
            if let Some((ty, _)) = expected {
                let revealed = self.revealed(ty, block.span);
                if !self.types.compatible(Ty::UNIT, revealed) {
                    self.mismatch(Ty::UNIT, ty, block.span);
                }
            }
        }
        Block { stmts, tail, span: block.span }
    }

    fn stmt(&mut self, stmt: &hir::Stmt) -> StmtKind {
        match &stmt.kind {
            hir::StmtKind::Let(binding) => self.let_stmt(binding),
            hir::StmtKind::Expr(expr) => {
                let typed = self.synth(expr);
                StmtKind::Expr(typed.id)
            }
            hir::StmtKind::Assign { target, value } => {
                // §6's indexing, on the side §6.5 promised: `a[i] be v` is
                // `IndexMutably`'s, and its slot is the referent rather than
                // the reference.
                let target = match &target.kind {
                    hir::ExprKind::Index { base, index } => {
                        self.index_expr(base, index, target.span, Indexing::Write)
                    }
                    _ => self.synth(target),
                };
                // §4.7: *"borrows auto-dereference for field access, method
                // calls, and assignment. There is no dereference operator."*
                // The first two were implemented and this one was not, so
                // `counter be 5` through a `mutable borrowed Int` was
                // `SC0525` — *expected `mutable borrowed Int`, found an
                // integer literal* — against the only spelling §4.7 leaves.
                //
                //
                // **Only an exclusive borrow dereferences, and the shared one
                // is the reason.** §4.7 states the rule without qualifying it,
                // and taken unqualified it makes correct programs unspellable:
                // `largest` in `examples/07_generics.science` narrows a
                // `(borrowed T)?` and writes `best be item`, meaning *rebind
                // the local* — the only thing it can mean, because a shared
                // borrow cannot be written through at all. So a `mutable
                // borrowed` target, which exists precisely to be written
                // through, dereferences; every other target is a rebinding.
                //
                // **The cost, stated.** Rebinding a local that holds a
                // `mutable borrowed` is then unspellable. That is the smaller
                // loss: the language has no dereference operator to recover
                // the write with, and it has `let` to recover the rebinding
                // with.
                let want = self.known_or_error(target.ty);
                let revealed = self.revealed(want, stmt.span);
                let want = match *self.types.kind(revealed) {
                    TyKind::Borrowed { mutable: true, inner } => inner,
                    _ => want,
                };
                let value = self.check(value, want, Site::Elsewhere);
                // The binding's own word decides whether this write is
                // allowed, but the types it reads are not final yet.
                self.assignments.push(target.id);
                // Decision 7: a write invalidates the place and everything
                // projected from it.
                if let Some(place) = self.body.place_of(target.id) {
                    self.facts.invalidate(&place);
                }
                StmtKind::Assign { target: target.id, value }
            }
            hir::StmtKind::Return(value) => {
                let ret = self.ret;
                let value = match value {
                    Some(value) => Some(self.check(value, ret, Site::Return)),
                    None => {
                        let revealed = self.revealed(ret, stmt.span);
                        if !self.types.compatible(Ty::UNIT, revealed) {
                            self.mismatch(Ty::UNIT, ret, stmt.span);
                        }
                        None
                    }
                };
                self.diverged = true;
                StmtKind::Return(value)
            }
            hir::StmtKind::Break(value) => {
                // §6: a `loop`'s value is not inferred, so the operand is
                // synthesised and its type discarded.
                let value = value.as_ref().map(|value| self.synth(value).id);
                if let Some(seen) = self.breaks.last_mut() {
                    *seen = true;
                }
                self.diverged = true;
                StmtKind::Break(value)
            }
            hir::StmtKind::Continue => {
                self.diverged = true;
                StmtKind::Continue
            }
            hir::StmtKind::Error => StmtKind::Error,
        }
    }

    fn let_stmt(&mut self, binding: &hir::Let) -> StmtKind {
        // Revision 2 §3.1's `let value, err be f()` takes one `mutable` for
        // the pair, so the word is the statement's and not each binding's.
        let declared = if binding.mutable { BoundAs::Mutable } else { BoundAs::Let };
        let written: Vec<Option<hir::Type>> =
            binding.bindings.iter().map(|bound| bound.ty.clone()).collect();
        let annotations: Vec<Option<Ty>> =
            written.iter().map(|ty| ty.as_ref().map(|ty| self.lower_ty(ty))).collect();

        let value = if binding.bindings.len() == 1 {
            match annotations[0] {
                Some(ty) => {
                    let id = self.check(&binding.value, ty, Site::Elsewhere);
                    self.bind_local(binding.bindings[0].def, InferTy::Known(ty), declared);
                    id
                }
                None => {
                    let typed = self.synth(&binding.value);
                    self.bind_local(binding.bindings[0].def, typed.ty, declared);
                    typed.id
                }
            }
        } else {
            // Revision 2 §3.1's pair. The resolver knew the binding count and
            // nothing about the initialiser's type, and said in as many words
            // that *"the arity check belongs to `science-types`"*. This is it.
            let expected = if annotations.iter().all(Option::is_some) {
                let elements: Vec<Ty> = annotations.iter().map(|ty| ty.unwrap()).collect();
                Some(self.types.tuple(elements))
            } else {
                None
            };
            let (id, ty) = match expected {
                Some(ty) => (self.check(&binding.value, ty, Site::Elsewhere), ty),
                None => {
                    let typed = self.synth(&binding.value);
                    let ty = self.known_or_error(typed.ty);
                    (typed.id, ty)
                }
            };
            let revealed = self.revealed(ty, binding.span);
            let elements = match self.types.kind(revealed).clone() {
                TyKind::Tuple(elements) => Some(elements),
                _ => None,
            };
            match elements {
                Some(elements) if elements.len() == binding.bindings.len() => {
                    for (at, bound) in binding.bindings.iter().enumerate() {
                        let ty = annotations[at].unwrap_or(elements[at]);
                        self.bind_local(bound.def, InferTy::Known(ty), declared);
                    }
                }
                found => {
                    if !self.types.references_error(revealed) {
                        let rendered = self.types.render(self.defs, ty);
                        self.diagnostics.push(binding_count(
                            binding.span,
                            binding.bindings.len(),
                            found.as_ref().map(|e| e.len()),
                            &rendered,
                        ));
                    }
                    for (at, bound) in binding.bindings.iter().enumerate() {
                        let ty = annotations[at].unwrap_or(Ty::ERROR);
                        self.bind_local(bound.def, InferTy::Known(ty), declared);
                    }
                }
            }
            id
        };
        StmtKind::Let { bindings: binding.bindings.iter().map(|b| b.def).collect(), value }
    }

    /// §9's substitution: what a scrutinee's arguments do to a declaration's
    /// parameters, on the way into a pattern.
    ///
    /// **Decision. A pattern's sub-patterns are checked against the declared
    /// payload or field type with the *scrutinee's* generic arguments
    /// substituted in**, exactly as [`BodyChecker::field`] already does for
    /// `p.left`. `Left(n)` under an `E of (I64, Bool)` binds `n` at `I64`.
    ///
    /// **The reason is that the alternative refuses correct programs.** Without
    /// it a payload reaches the binding as the declaration wrote it — the `L` of
    /// `choice E of (L, R)` — and a [`TyKind::Param`] of a definition the body
    /// is not generic over agrees with nothing, so `Left(n): n` in a function
    /// returning `I64` was `SC0525`, *expected `I64`, found `L`*. That is a
    /// false positive on a program the language has no other way to write, and
    /// it survived because `examples/04_enums.science` declares generic choices
    /// and never matches on one.
    ///
    /// **It is one substitution applied to the whole declared type, so nesting
    /// and arity come for free**: a payload of `Array of (Map of (K, V))` is
    /// rewritten by the same fold that rewrites a bare `T`, and a pattern nested
    /// inside it recurses with the already-substituted type in hand.
    ///
    /// **The cost, stated. The answer is empty wherever the scrutinee is not
    /// this declaration applied to arguments**, and then the payload is the
    /// declared type as before. That covers three cases and none of them is a
    /// new silence: an erroneous scrutinee (`ty`'s §5 — it has already been
    /// reported), a scrutinee that is a type parameter or `Self` (nobody has
    /// instantiated it, so there is nothing to substitute), and a pattern whose
    /// variant belongs to some *other* choice than the scrutinee's — which is a
    /// mismatch the pattern's own check owns, not this one's, and guessing a
    /// substitution for it would report about a type nobody wrote.
    ///
    /// **A borrow is peeled and the binding is not re-borrowed.** `match` over
    /// a `borrowed E of (I64, Bool)` reads its arguments through the borrow,
    /// because a borrow is transparent to a field read for the same reason
    /// [`BodyChecker::field`] gives. What it does *not* do is make the binding
    /// `borrowed I64`: match ergonomics are a decision no note has taken, and
    /// this change is a substitution rather than a new binding mode.
    fn scrutinee_substitution(
        &mut self,
        scrutinee: Ty,
        owner: Option<DefId>,
        generics: &[hir::GenericParam],
        span: Span,
    ) -> Substitution {
        let (Some(owner), false) = (owner, generics.is_empty()) else {
            return Substitution::new();
        };
        let revealed = self.revealed(scrutinee, span);
        let revealed = match *self.types.kind(revealed) {
            TyKind::Borrowed { inner, .. } => self.revealed(inner, span),
            _ => revealed,
        };
        match self.types.kind(revealed).clone() {
            TyKind::Named { def, args } if def == owner => {
                Substitution::of_generics(generics, &args)
            }
            _ => Substitution::new(),
        }
    }

    fn pattern(&mut self, pattern: &hir::Pattern, scrutinee: Ty) -> PatId {
        let span = pattern.span;
        match &pattern.kind {
            hir::PatternKind::Wildcard => self.body.push_pat(PatKind::Wildcard, scrutinee, span),
            hir::PatternKind::Literal(literal) => {
                self.body.push_pat(PatKind::Literal(literal.clone()), scrutinee, span)
            }
            hir::PatternKind::Binding { mutable, def } => {
                let bound = if *mutable { BoundAs::Mutable } else { BoundAs::Pattern };
                self.bind_local(*def, InferTy::Known(scrutinee), bound);
                self.body.push_pat(
                    PatKind::Binding { mutable: *mutable, def: *def },
                    scrutinee,
                    span,
                )
            }
            hir::PatternKind::Variant { res, elems } => {
                let def = res.def_id();
                let declaration = def.and_then(|def| self.decls.variant(def));
                let (payload, owner, generics) = match declaration {
                    Some(variant) => {
                        (variant.payload.clone(), Some(variant.choice), variant.generics.clone())
                    }
                    None => (Vec::new(), None, Vec::new()),
                };
                // §9. `Left(n)` under a scrutinee of `E of (I64, Bool)` binds
                // `n` at `I64`, not at the `L` the declaration wrote.
                let substitution = self.scrutinee_substitution(scrutinee, owner, &generics, span);
                let elems = elems
                    .iter()
                    .enumerate()
                    .map(|(at, elem)| {
                        let ty = payload.get(at).copied().unwrap_or(Ty::ERROR);
                        let ty = self.apply(&substitution, ty, elem.span);
                        self.pattern(elem, ty)
                    })
                    .collect();
                self.body.push_pat(PatKind::Variant { def, elems }, scrutinee, span)
            }
            hir::PatternKind::Struct { res, fields } => {
                let def = res.def_id();
                let declaration = def.and_then(|def| self.decls.record(def));
                let (declared, generics) = match declaration {
                    Some(record) => (record.fields.clone(), record.generics.clone()),
                    None => (Vec::new(), Vec::new()),
                };
                // §9 again, at the other shape a declaration's parameters reach
                // a binding through. `BodyChecker::field` already substitutes
                // for `p.left`; this is the same field read spelled as a
                // pattern.
                let substitution = self.scrutinee_substitution(scrutinee, def, &generics, span);
                let fields = fields
                    .iter()
                    .filter_map(|field| {
                        let id = field.field.def_id()?;
                        let ty = declared
                            .iter()
                            .find(|(known, _)| *known == id)
                            .map(|(_, ty)| *ty)
                            .unwrap_or(Ty::ERROR);
                        let ty = self.apply(&substitution, ty, field.pattern.span);
                        Some((id, self.pattern(&field.pattern, ty)))
                    })
                    .collect();
                self.body.push_pat(PatKind::Record { def, fields }, scrutinee, span)
            }
            hir::PatternKind::Tuple(elements) => {
                let revealed = self.revealed(scrutinee, span);
                let tys = match self.types.kind(revealed).clone() {
                    TyKind::Tuple(tys) if tys.len() == elements.len() => tys,
                    _ => vec![Ty::ERROR; elements.len()],
                };
                let elements = elements
                    .iter()
                    .zip(tys)
                    .map(|(element, ty)| self.pattern(element, ty))
                    .collect();
                self.body.push_pat(PatKind::Tuple(elements), scrutinee, span)
            }
            hir::PatternKind::Unit => self.body.push_pat(PatKind::Unit, Ty::UNIT, span),
            hir::PatternKind::Or(alternatives) => {
                let alternatives =
                    alternatives.iter().map(|p| self.pattern(p, scrutinee)).collect();
                self.body.push_pat(PatKind::Or(alternatives), scrutinee, span)
            }
            hir::PatternKind::Error => self.body.push_pat(PatKind::Error, Ty::ERROR, span),
        }
    }

    // --- bookkeeping ------------------------------------------------------

    /// Brings a binding into scope.
    ///
    /// Takes the [`Form`] rather than defaulting it, so that a binding site
    /// added later cannot quietly inherit "assignable" — which is exactly how
    /// `mutable` came to be accepted everywhere and enforced nowhere.
    fn bind_local(&mut self, def: DefId, ty: InferTy, bound: BoundAs) {
        let stored = self.known_or_error(ty);
        self.locals.push((def, ty));
        self.bound_as.push((def, bound));
        self.body.declare_local(def, stored);
    }

    /// Revision 2 §2.2's rule: `x be v` needs `x` to have been declared
    /// `mutable`.
    ///
    /// Silent until now. `mutable` parsed, resolved, reached
    /// `hir::PatternKind::Binding` — and nothing ever read it, so `let count
    /// be 0` followed by `count be 3` checked, built, and printed `3`. The
    /// word was decoration.
    ///
    /// **The rule governs owned storage, and stops at the first reference.**
    /// `def retitle(doc: mutable borrowed Doc)` writing `doc.title be title`
    /// is not reassigning `doc`; it is writing the referent, which is what
    /// `mutable borrowed` is *for*. So the walk toward the root stops as soon
    /// as it crosses a borrow, and from there the write is the borrow rules'
    /// business — `science-regions`' — and not this one's. Only a path that
    /// reaches a binding without crossing a reference writes that binding's
    /// own storage, and only then does the word at its declaration decide.
    fn check_assignable(&mut self, target: ExprId) {
        let Some((root, whole)) = self.owned_root(target) else { return };
        // A name this body never bound is not a local. Whatever else is wrong
        // with assigning to it, it is not this rule's to report.
        let Some(bound) = self.bound_as(root) else { return };
        if bound == BoundAs::Mutable {
            return;
        }
        let def = self.defs.get(root);
        let name = def.name.clone();
        let span = self.body.expr(target).span;
        // A write through a projection is refused for the root's reason, but
        // the reader is looking at `config.port`, so say which part decided.
        let label = if whole {
            "assigned here".to_string()
        } else {
            format!("this writes through `{name}`")
        };
        let fix = match bound {
            BoundAs::Let => format!("declare it `let mutable {name}`"),
            BoundAs::Pattern => format!("bind it `mutable {name}` in the pattern"),
            // There is nowhere in `name: Type` to put the word, and `self`
            // takes it before the name rather than after.
            BoundAs::Parameter if name == "self" => {
                "take the receiver as `mutable self`".to_string()
            }
            BoundAs::Parameter => {
                format!("a parameter cannot be declared mutable — bind a local from it: `let mutable {name} be {name}`")
            }
            BoundAs::Mutable => unreachable!("returned above"),
        };
        let mut diagnostic = Diagnostic::error(NOT_MUTABLE, format!("`{name}` is not mutable"))
            .with_label(Label::primary(span, label))
            .with_note(fix);
        if !def.is_builtin() {
            // Saying "without `mutable`" at a parameter would be pointing at a
            // place the word cannot go, which reads as a typo rather than a
            // rule.
            let at = match bound {
                BoundAs::Parameter => "declared here",
                _ => "declared here, without `mutable`",
            };
            diagnostic = diagnostic.with_label(Label::secondary(def.span, at));
        }
        self.diagnostics.push(diagnostic);
    }

    /// The binding an assignment writes the storage *of*, and whether the
    /// target was that whole binding rather than a projection from it.
    ///
    /// `None` when the write does not land in a binding's own storage: it goes
    /// through a borrow, or through a call, or the target is not a place at
    /// all. Each of those is somebody else's rule, and answering `None` is how
    /// this one declines to guess.
    fn owned_root(&mut self, target: ExprId) -> Option<(DefId, bool)> {
        let mut id = target;
        let mut whole = true;
        loop {
            // Crossing an exclusive reference ends the walk: past it the
            // storage written belongs to whatever the reference points at,
            // and no binding in this body owns it. A *shared* borrow is not
            // crossed, because a write cannot go through one — assigning to a
            // target of that type is a rebinding, and a rebinding is exactly
            // what the word at the declaration governs. The two halves of
            // `be` therefore split on one question, asked once each.
            let (ty, span) = {
                let expr = self.body.expr(id);
                (expr.ty, expr.span)
            };
            let revealed = self.revealed(ty, span);
            if matches!(self.types.kind(revealed), TyKind::Borrowed { mutable: true, .. }) {
                return None;
            }
            // A type that did not come out says nothing about whether this is
            // a borrow, and §5's absorption means it never will. Reporting
            // over it produces a second, wrong error on a program whose real
            // problem is somewhere else — which is how `let slot be
            // items.get_mut(0)` came to be told its binding was not mutable
            // when the actual fault is that the prelude's method was never
            // looked up.
            if self.types.references_error(revealed) {
                return None;
            }
            match &self.body.expr(id).kind {
                ExprKind::Local(def) | ExprKind::SelfValue(def) => return Some((*def, whole)),
                ExprKind::Field { base, .. } | ExprKind::Index { base, .. } => {
                    id = *base;
                    whole = false;
                }
                // A narrowed read is the same storage at a smaller type, so it
                // is transparent here exactly as it is to `Body::place_of`.
                ExprKind::Narrow(operand) => id = *operand,
                _ => return None,
            }
        }
    }

    /// How a binding was introduced, for the assignment rule.
    ///
    /// A binding this body never introduced is `None`: a static, a function
    /// name, or a name the resolver could not place. None of those is a local
    /// to be reassigned, and none of them is this rule's business.
    fn bound_as(&self, def: DefId) -> Option<BoundAs> {
        self.bound_as.iter().rev().find(|(id, _)| *id == def).map(|(_, bound)| *bound)
    }


    fn lookup_local(&self, def: DefId) -> Option<InferTy> {
        self.locals.iter().rev().find(|(id, _)| *id == def).map(|(_, ty)| *ty)
    }

    fn local_ty(&mut self, def: DefId) -> InferTy {
        match self.lookup_local(def) {
            Some(ty) => self.infer.resolve(ty),
            // A binding this body did not introduce: a capture the resolver
            // admitted, or a name already reported.
            None => InferTy::Known(Ty::ERROR),
        }
    }

    fn known_or_error(&mut self, ty: InferTy) -> Ty {
        match self.infer.resolve(ty) {
            InferTy::Known(known) => known,
            InferTy::Var(_) => Ty::ERROR,
        }
    }

    /// Pushes a node whose type may still be a variable, and records it for
    /// §4's writeback if it is.
    fn push_typed(&mut self, kind: ExprKind, ty: InferTy, span: Span) -> Typed {
        let stored = self.known_or_error(ty);
        let id = self.body.push_expr(kind, stored, span);
        self.record_pending(id, ty);
        Typed { id, ty }
    }

    fn record_pending(&mut self, id: ExprId, ty: InferTy) {
        if let InferTy::Var(var) = ty {
            self.pending.push((id, var));
        }
    }

    fn bool_ty(&mut self) -> Option<Ty> {
        self.decls.prelude().ty(self.types, "Bool")
    }

    fn error_expr(&mut self, span: Span) -> Typed {
        let id = self.body.push_expr(ExprKind::Error, Ty::ERROR, span);
        Typed { id, ty: InferTy::Known(Ty::ERROR) }
    }
}

/// One [`GenericArg`] per declared parameter, all unknown.
///
/// `ty`'s §5 is the whole of why this is right: an erroneous type agrees with
/// whatever it meets, so a `Bound of {unknown}` fits a `Bound of Int` slot and
/// says nothing, where a `Bound` with *no* arguments would be a different type
/// and would say something false.
fn unknown_args(declared: &[hir::GenericParam]) -> Vec<GenericArg> {
    declared
        .iter()
        .map(|param| match param.kind {
            hir::GenericParamKind::Type { .. } => GenericArg::Type(Ty::ERROR),
            hir::GenericParamKind::Const { .. } => GenericArg::Error,
        })
        .collect()
}

/// Whether a type is a prelude-shaped name this phase can classify.
///
/// A [`TyKind::Param`], a `Self`, a closure or a tuple is *not*, and §5 admits a
/// literal at one rather than guessing.
fn is_prelude_named(types: &Types, ty: Ty) -> bool {
    matches!(types.kind(ty), TyKind::Named { .. })
}

/// Whether this phase can say nothing at all about what inhabits a type.
///
/// **Four cases, and they are the whole of §5's admission.** A type parameter
/// and a `Self` stand for a type a *substitution* supplies, which is a call
/// site's fact and not this one's; a `Self.Item` stands for one an
/// implementation block answers, which `subst`'s §2 deliberately leaves
/// standing; and an erroneous type agrees with whatever it meets, which is
/// `ty`'s §5 and the reason one bad annotation stays one diagnostic.
///
/// Every other [`TyKind`] is a shape whose inhabitants the declaration that
/// wrote it fixes, so a judgement about one is a judgement this phase is
/// entitled to make. Both literal rules ask this — a number and a `null` differ
/// in what they accept and not in what they can see — and asking it in one
/// place is what keeps the two lines the same.
fn is_opaque(types: &Types, ty: Ty) -> bool {
    matches!(
        types.kind(ty),
        TyKind::Error
            | TyKind::Param { .. }
            | TyKind::SelfType { .. }
            | TyKind::SelfAssoc { .. }
    ) || types.references_error(ty)
}

/// §1b. An expression whose value is produced inside a scope of its own.
///
/// **The decision. A branch or a block at a `borrowed T` argument is
/// synthesised, and §6.3's auto-borrow is taken above it.**
///
/// **The reason.** §6.3 puts the auto-borrow *"at call sites"*, and the call
/// site of `print(if flag: "yes" else: "no")` is the argument, not the two
/// arms. [`BodyChecker::check`] pushes an expectation inward through `if`,
/// `match` and a block, and pushing `borrowed any Display` inward made each arm
/// take its own borrow — of `"yes"`, a temporary whose storage ends at the
/// close of the arm. `SC0333` was then right about the MIR and wrong about the
/// program: the value the caller sees outlives the arm, and the borrow the
/// checker wrote does not. Synthesising first gives the branch one value, in
/// the argument's own scope, and one borrow above it.
///
/// **Why it is here and not in the region engine.** The temporary's scope is
/// `science-mir`'s, and widening it would be a lowering change made to
/// accommodate a borrow the type checker chose to put in the wrong place. The
/// borrow is this phase's node — [`BodyChecker::auto_borrow`] writes it — so
/// the place it is written is this phase's to get right.
///
/// **The cost, and it is a real one.** `borrow_of(if c: a else: b)`, where `a`
/// and `b` are bindings and the parameter is `borrowed String`, used to borrow
/// `a` or `b` in place; it now moves one of them into the branch's value and
/// borrows that. The move is visible — a later use of `a` is a use after move —
/// and nothing in `examples/` writes that shape, which is how the cost is
/// priced rather than how it is dismissed. Taking the cheaper path would mean
/// deciding per arm whether its tail denotes a place, and the checker does not
/// know that when it descends.
///
/// **A closure and a tuple are not on this list.** A tuple at a borrowed slot
/// never matched [`BodyChecker::check`]'s tuple arm in the first place — that
/// arm requires a tuple *target* — and a closure body is not a place the
/// argument's borrow could have been taken in.
///
/// **`if` is the only one of the four with a surface syntax in argument
/// position today**, and it is the one `examples/13_inline_blocks.science`
/// writes. `match`, a block and an `unsafe` block are on the list because they
/// reach [`BodyChecker::check`] through the same three arms and would
/// distribute the same way the day the grammar admits them inline; leaving them
/// off would be a rule that holds for the form that is written and not for the
/// form it is a case of.
fn branches(expr: &hir::Expr) -> bool {
    matches!(
        expr.kind,
        hir::ExprKind::If(_)
            | hir::ExprKind::Match(_)
            | hir::ExprKind::Block(_)
            | hir::ExprKind::Unsafe(_)
    )
}

/// Two literal kinds in one inference class. An integer literal unified with a
/// float one is a float: `1 + 2.0` is the case, and the integer is the one that
/// can be represented exactly in the other's type.
fn widen_numeric(left: Numeric, right: Numeric) -> Numeric {
    match (left, right) {
        (Numeric::Null, other) | (other, Numeric::Null) => other,
        (Numeric::Float, _) | (_, Numeric::Float) => Numeric::Float,
        _ => Numeric::Integer,
    }
}

fn suffix_name(suffix: NumSuffix) -> &'static str {
    match suffix {
        NumSuffix::I8 => "I8",
        NumSuffix::I16 => "I16",
        NumSuffix::I32 => "I32",
        NumSuffix::I64 => "I64",
        NumSuffix::U8 => "U8",
        NumSuffix::U16 => "U16",
        NumSuffix::U32 => "U32",
        NumSuffix::U64 => "U64",
        NumSuffix::F32 => "F32",
        NumSuffix::F64 => "F64",
    }
}


// --- the diagnostics this phase owns -------------------------------------

/// `SC0525` — the bidirectional checker's central message.
///
/// It names the *declared* type first because Decision 1's first dividend is
/// that *"error messages can name a declared type"*: the expectation came from
/// an annotation a human wrote, and putting it first is what makes the message
/// about that annotation rather than about the compiler's state.
/// Which interface each binary operator dispatches to, and what the interface
/// calls its method.
///
/// **The names are `stdlib-shape-and-packages.md` §4.5's Decision 4c**: *"an
/// operator trait's method takes the trait's lowercase name only when that name
/// is free"*. All eight below are free. The ones that are not — `Not`, `BitAnd`,
/// `BitOr` — are renamed by that same decision and are absent here for a
/// different reason: §5.4 declares no bitwise interfaces at all, which that note
/// records as a hole in the core spec rather than filling.
///
/// `Ord` is the deliberate omission and it stays one: `< > <= >=` would
/// dispatch to a method whose only sane name is `compare`, whose return type is
/// an `Ordering` that no note specifies, and whose relation to four operators —
/// including what `F64`'s NaN does to a total order — nothing has written down.
/// The *implementation* is required all the same, by
/// [`BodyChecker::implements_operand`], which is where the line between the two
/// is argued. `Eq` is off this table for the same reason and through the same
/// function.
fn binary_operator(op: BinaryOp) -> Option<(&'static str, &'static str)> {
    match op {
        BinaryOp::Add => Some(("Add", "add")),
        BinaryOp::Sub => Some(("Sub", "sub")),
        BinaryOp::Mul => Some(("Mul", "mul")),
        BinaryOp::Div => Some(("Div", "div")),
        BinaryOp::Rem => Some(("Rem", "rem")),
        BinaryOp::Pow => Some(("Pow", "pow")),
        BinaryOp::MatMul => Some(("MatMul", "matmul")),
        _ => None,
    }
}

/// `SC0535` — an operator whose operand implements nothing it could dispatch
/// to.
///
/// **The message names the block the author would have to write**, which is
/// [`unsatisfied_bound`]'s shape one construct along and for its reason: the
/// thing that is missing is a declaration, and a diagnostic that says only
/// *"cannot add these"* has refused to say what would fix it.
fn no_operator_implementation(
    span: Span,
    symbol: &str,
    ty: &str,
    interface: &str,
) -> Diagnostic {
    Diagnostic::error(
        codes::NO_OPERATOR_IMPLEMENTATION,
        format!("`{ty}` does not implement `{interface}`"),
    )
    .with_label(Label::primary(span, format!("`{symbol}` needs `{interface}` and `{ty}` has no implementation of it")))
    .with_note(format!(
        "every operator is an interface method (§5.4): the block \n         `{ty} implements {interface}:` is what gives `{ty}` this operator"
    ))
}

/// `SC0275` — a value interpolated into an `f"…"` that cannot be rendered.
///
/// **The message names the interface and not the machinery**, because
/// `Display` is the thing the author can act on: §3.4 decides that arrays,
/// maps, closures and `T?` deliberately do **not** implement it, and each of
/// those refusals has an answer the author can write — narrow the nullable,
/// take the length, name the field. A message about a missing renderer would
/// describe the compiler instead of the program.
/// `SC0275` — a call to `print` or `write` that does not supply one value.
///
/// **The same code as [`not_displayable`], because §7's row is one row.** That
/// row reads *"the argument does not implement the requested interface …
/// covers `print` given more than one argument"*, and the constant it is behind
/// is named for the clause it leads with. Giving the arity its own code would
/// mean `strings-formatting-and-docs.md` §7's table was wrong about the
/// compiler in a second way while being made right about the first.
fn print_takes_one_value(span: Span, name: &str, supplied: usize) -> Diagnostic {
    let note = if supplied == 0 {
        // §4.1: *"a blank line is `print(\"\")`. Two characters, and no
        // language feature."*
        format!("a blank line is `{name}(\"\")`: there are no default arguments (§4.4)")
    } else {
        // §4.1's applicable fix, as prose. It is not an
        // `ApplicableFix` because the rewrite has to interleave the arguments
        // with the separators the author meant, and inventing that text is a
        // guess about the spacing §4.1's second reason is precisely about.
        format!("interpolate instead: `{name}(f\"… {{value}}\")`")
    };
    Diagnostic::error(codes::NOT_DISPLAYABLE, format!("`{name}` takes one value"))
        .with_label(Label::primary(span, format!("{supplied} arguments were supplied")))
        .with_note(note)
}

fn not_displayable(span: Span, ty: &str) -> Diagnostic {
    Diagnostic::error(
        codes::NOT_DISPLAYABLE,
        format!("`{ty}` cannot be interpolated: it does not implement `Display`"),
    )
    .with_label(Label::primary(span, format!("this is `{ty}`")))
    .with_note(
        "an interpolation renders its value through `Display`. A nullable does not implement it \
         until it is narrowed, and a value that implements nothing needs an `implements Display:` \
         block before it can be printed",
    )
}

/// `SC0281` — two elements of an array literal that do not unify.
///
/// **Both spans, which is the whole reason this is not `SC0525`.** §7.2 asks
/// for *"primary on the first element that differs, secondary on the element
/// that fixed the type"*, because with no annotation in sight neither element
/// is more right than the other and a message with one span would have picked
/// a winner silently.
///
/// `expected` and `found` arrive rendered, backticks and all, because one of
/// them may be a phrase — *"an integer literal"* — rather than a type. That is
/// `demand`'s wording for a class that has not settled, and §3.2's own example
/// is exactly that case: `[1, 2.0]` has no two types to print yet.
fn array_element_mismatch(
    span: Span,
    expected: &str,
    found: &str,
    fixed_at: Span,
) -> Diagnostic {
    Diagnostic::error(
        codes::ARRAY_ELEMENT_MISMATCH,
        "the elements of this array literal do not have one type",
    )
    .with_label(Label::primary(span, format!("this is {found}")))
    .with_label(Label::secondary(fixed_at, format!("this is {expected}")))
    .with_note(
        "an array literal is an `Array of T` where `T` is the unification of its elements \
         (§3.1), and there is no implicit numeric conversion (§5.1): write the suffix, the \
         `as`, or the literal you meant",
    )
}

/// `SC0282` — `[]` where nothing says what it holds.
///
/// **The fix is the annotation**, and the message says where it goes rather
/// than naming a candidate type. §7.2 asks for the candidate *"when exactly one
/// is visible"*, and at this point exactly none is: reaching this function
/// means `check`'s arm found no `Array of E` expectation, so there is nothing
/// to name.
fn empty_array_no_type(span: Span) -> Diagnostic {
    Diagnostic::error(codes::EMPTY_ARRAY_NO_TYPE, "this empty array literal has no element type")
        .with_label(Label::primary(span, "nothing here says what `[]` holds"))
        .with_note(
            "an empty literal takes its element type from the expected type at its position \
             (§3.3), and there is none here: annotate the binding — `let xs: Array of F64 be []`",
        )
}

/// `SC0538` — a range written as an index.
///
/// **The message names the missing *type*, not a missing implementation.**
/// There is no block the author can write to make this work and no argument
/// they can pass instead, so the only honest thing to offer is the shape that
/// does exist today.
fn range_index_needs_slice(span: Span, ty: &str) -> Diagnostic {
    Diagnostic::error(
        codes::RANGE_INDEX_NEEDS_SLICE,
        "a range index would produce a slice, and there is no `Slice` type",
    )
    .with_label(Label::primary(span, format!("slicing a `{ty}` is not implemented")))
    .with_note(
        "`a[1..5]` is specified as a `Slice of T` — a borrow and a length, which does not copy \
         (§2.3) — and this compiler declares no such type. Index one element at a time, or \
         iterate: `xs.iterate().skip(1).take(2)`",
    )
}

fn mismatched_types(span: Span, expected: &str, found: &str) -> Diagnostic {
    Diagnostic::error(codes::MISMATCHED_TYPES, format!("expected `{expected}`, found `{found}`"))
        .with_label(Label::primary(span, format!("this is `{found}`")))
}

/// `SC0526` — a class of inference variables that never got a type. §4.
fn cannot_infer(span: Span) -> Diagnostic {
    Diagnostic::error(codes::TYPE_ANNOTATIONS_NEEDED, "the type of this value cannot be inferred")
        .with_label(Label::primary(span, "no annotation and nothing to infer from"))
        .with_note(
            "inference is local to one body (§5.2) and works from the root of a type outward, \
             so a value whose type is a hole inside a known constructor needs the annotation \
             written",
        )
}

/// `SC0527` — a call with the wrong number of arguments.
fn wrong_argument_count(span: Span, expected: usize, found: usize) -> Diagnostic {
    let plural = if expected == 1 { "" } else { "s" };
    Diagnostic::error(
        codes::WRONG_ARGUMENT_COUNT,
        format!("this call takes {expected} argument{plural} and was given {found}"),
    )
    .with_label(Label::primary(span, format!("{found} given")))
}

/// `SC0528` — a field the type does not have.
fn no_such_field(span: Span, name: &str, ty: &str) -> Diagnostic {
    Diagnostic::error(codes::NO_SUCH_FIELD, format!("`{ty}` has no field `{name}`"))
        .with_label(Label::primary(span, "no such field"))
}

/// The const parameter a pattern *is*, when it is one and nothing more.
///
/// `WIDTH` matches and solves; `WIDTH + 1` does not, and `2 * WIDTH` does not.
/// That is the root-level restriction of `check`'s §6 read one kind down, and
/// the general answer is `matching::match_linear` under an obligation nobody
/// discharges yet.
fn bare_const_param(pattern: &crate::normal::NormalForm) -> Option<DefId> {
    if pattern.constant() != 0 || pattern.terms().len() != 1 {
        return None;
    }
    let term = &pattern.terms()[0];
    if term.coefficient() != 1 {
        return None;
    }
    match term.atom() {
        crate::normal::Atom::Param { def, .. } => Some(def),
    }
}

/// `SC0534` — a generic argument that does not satisfy the callee's bound. §8.
///
/// **The message names the type, the interface and the parameter, and the
/// label names the fix.** An interface is not a type a value can have
/// (Decision 13 makes `any Summarize` the type, and the argument is not one),
/// so there is no *"expected"* to put first the way `SC0525` does; what the
/// author has to change is either the argument or the implementation list of
/// its type, and both are named.
///
/// **The secondary label is the bound as written**, which is `diagnostics`'
/// §2's rule — *"the definition span is one lookup away"* — applied to a
/// promise rather than to an atom: the call is being held to something written
/// somewhere else, and a message that does not show where is asking the reader
/// to go and find it.
fn unsatisfied_bound(
    span: Span,
    ty: &str,
    interface: &str,
    parameter: &str,
    bound: Span,
) -> Diagnostic {
    Diagnostic::error(
        codes::UNSATISFIED_BOUND,
        format!("`{ty}` does not implement `{interface}`"),
    )
    .with_label(Label::primary(span, format!("`{parameter}` is `{ty}` here")))
    .with_label(Label::secondary(bound, format!("`{parameter}` was declared `{interface}`")))
    .with_note(format!(
        "an implementation is a block the program writes: `{ty} implements {interface}:`"
    ))
}

/// `SC0532` — a method the receiver's type does not have.
///
/// [`no_such_field`]'s sibling, reported under the restraint `methods`'s §1
/// imposes: only where the receiver is a type this crate holds an
/// implementation table for. The prelude registers no methods at all, so a call
/// on a `String` reaches neither this function nor any other.
/// `SC0536`: an associated call reached through a generic type's bare name,
/// whose own type arguments nothing at the call fixes.
///
/// **The message names the parameters that are open and offers the spelling**,
/// because unlike `SC0526` this one has a fix that is always available and
/// always writable. `examples/07_generics.science` already writes it and says
/// why the parentheses are required — *"`Wrapper of Int.holding(7)` would not
/// say whether `.holding` belongs to `Int` or to the whole type"* — so the
/// second note quotes the corpus rather than inventing a form.
///
/// **No [`Suggestion`] is attached**, although one would be machine-applicable
/// in shape. The replacement needs the type the author meant, which is exactly
/// what nothing at this call determined; a suggestion with a hole in it is a
/// fix a tool applies and gets a second error from.
///
/// [`Suggestion`]: science_diagnostics::Suggestion
fn uninferable_receiver(
    span: Span,
    ty: &str,
    method: &str,
    unsolved: &[String],
    consts: bool,
) -> Diagnostic {
    let plural = if unsolved.len() + usize::from(consts) == 1 { "" } else { "s" };
    let named: Vec<String> = unsolved.iter().map(|name| format!("`{name}`")).collect();
    let label = if named.is_empty() {
        format!("`{ty}`'s const argument{plural} can only come from an instantiation")
    } else {
        format!("nothing here fixes {}", named.join(", "))
    };
    Diagnostic::error(
        codes::UNINFERABLE_RECEIVER,
        format!("the type argument{plural} of `{ty}` cannot be inferred here"),
    )
    .with_label(Label::primary(span, label))
    .with_note(
        "an associated function is reached through a type and not through a value, so the \
         type's arguments come from the instantiation the caller writes or from the arguments \
         of this call, and from nowhere else",
    )
    .with_note(format!(
        "write the instantiation, in parentheses so that the `.` applies to the whole type: \
         `({ty} of ..).{method}(..)`"
    ))
}

fn no_such_method(span: Span, name: &str, ty: &str) -> Diagnostic {
    Diagnostic::error(codes::NO_SUCH_METHOD, format!("`{ty}` has no method `{name}`"))
        .with_label(Label::primary(span, "no such method"))
        .with_note(
            "method lookup finds the inherent methods of the type and the methods of the \
             interfaces it implements, and neither has this name",
        )
}

/// `SC0531`, the second road to it: implementations of one interface that the
/// arguments did not narrow to one. `methods`'s §6.
///
/// **The message says what the author has to change.** Decision 11's other
/// message names two methods and cannot name the fix, because F0 has no
/// qualified-call syntax; this one can, because the thing that chooses here is
/// the argument, and an argument is something an author can give a type to.
/// `literal` is the case where that is the whole story — a bare `1` or a `null`
/// has no type until a signature expects one, and there is no signature yet.
fn instances_not_narrowed(
    span: Span,
    name: &str,
    receiver: &str,
    interface: &str,
    supplied: &str,
    accepts: &[(Span, String)],
    literal: bool,
) -> Diagnostic {
    let mut diagnostic = Diagnostic::error(
        codes::AMBIGUOUS_METHOD,
        format!(
            "`{name}` on `{receiver}` could be {} implementations of `{interface}`",
            accepts.len()
        ),
    )
    .with_label(Label::primary(span, format!("supplied {supplied}, which fits all of them")));
    for (at, params) in accepts {
        diagnostic =
            diagnostic.with_label(Label::secondary(*at, format!("one accepts {params}")));
    }
    diagnostic = diagnostic.with_note(
        "these are one method at several instantiations of one interface, so the argument \
         types are what choose between them — and here they did not narrow it to one",
    );
    if literal {
        diagnostic = diagnostic.with_note(
            "a literal or `null` has no type until a signature expects one, and which \
             signature that is is what this call is trying to decide — give the argument \
             a type first, with a suffix or a binding that is annotated",
        );
    }
    diagnostic
}

/// `SC0533` — the arguments fit none of the implementations of the interface.
///
/// The sibling of [`instances_not_narrowed`] on the other side: there, more
/// than one implementation took what was supplied; here, none did. Both name
/// what each implementation accepts, because that list is the whole of what the
/// author has to choose from and it is spread over as many blocks as there are
/// candidates.
fn no_matching_instance(
    span: Span,
    name: &str,
    receiver: &str,
    interface: &str,
    supplied: &str,
    accepts: &[(Span, String)],
) -> Diagnostic {
    let mut diagnostic = Diagnostic::error(
        codes::NO_MATCHING_IMPLEMENTATION,
        format!("no implementation of `{interface}` for `{receiver}` accepts {supplied}"),
    )
    .with_label(Label::primary(span, format!("`{name}` was given {supplied}")));
    for (at, params) in accepts {
        diagnostic =
            diagnostic.with_label(Label::secondary(*at, format!("this one accepts {params}")));
    }
    diagnostic.with_note(
        "these are one method at several instantiations of one interface, and the argument \
         types choose between them — so an argument that fits none of them names no method",
    )
}

/// `SC0529` — `let a, b be f()` against something that is not a pair.
///
/// The resolver's `LetBinding` documentation hands this check here by name:
/// *"whether the value actually is a tuple of the right width is not checked
/// here … the arity check belongs to `science-types`"*.
fn binding_count(span: Span, wanted: usize, found: Option<usize>, ty: &str) -> Diagnostic {
    let message = match found {
        Some(found) => format!("this `let` binds {wanted} names and the value has {found} parts"),
        None => format!("this `let` binds {wanted} names and the value is not a tuple"),
    };
    Diagnostic::error(codes::BINDING_COUNT_MISMATCH, message)
        .with_label(Label::primary(span, format!("the value is `{ty}`")))
}

/// `SC0530` — `e?` where `e` cannot be absent.
fn not_nullable(span: Span, ty: &str) -> Diagnostic {
    Diagnostic::error(codes::PRESENCE_TEST_ON_NON_NULLABLE, "this value is never absent")
        .with_label(Label::primary(span, format!("`{ty}` is not a nullable type")))
        .with_note("`?` tests a `T?` for a value; on a `T` it is always true")
}
