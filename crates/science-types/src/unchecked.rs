//! `SC0140`, the unchecked error — Decisions 9 and 10.
//!
//! `syntax-revision-2.md` §3.3 is explicit that the Go-style pair loses
//! `Result`'s guarantee that a failure cannot be ignored, and that this
//! diagnostic is what recovers most of it: it *"should ship with the feature,
//! not after it"*. It did not. This is it.
//!
//! **The code is `syntax-revision-2.md`'s and not this crate's.** It sits in
//! the syntax band because the condition is a property of the error *model*,
//! not of the type system that happens to be able to see it, and this module
//! implements it rather than claiming it.
//!
//! # 1. What it is, in the shape §5 specifies
//!
//! > **Decision 9.** If a binding of nullable error type is never tested with
//! > `?` on any path from its definition to a return, that is `SC0140`.
//! >
//! > It is a forward reachability question over THIR, not a full dataflow: mark
//! > every binding whose declared or inferred type is `E?` where `E` implements
//! > `Error`, mark every `?` test that reads it, and report the bindings with no
//! > reader. Linear in the size of the body.
//!
//! So: one pass over [`Body::locals`] to find the candidates, one pass over the
//! expression arena to find their readers, and a report for the difference.
//! **There is no dataflow and no fixed point**, which is the decision and not an
//! economy — a `?` on one path is the author having thought about it, and a
//! diagnostic that asked for one on *every* path would fire on
//! `if urgent?: check(err)` and be wrong about a program that is fine.
//!
//! *"From its definition to a return"* needs no ordering check of its own: a
//! reader is an expression mentioning the binding, and the resolver has already
//! made every such expression come after the `let` — the binding *"is in scope
//! from after this statement onward"*, which its own documentation says. The
//! arena is in visit order, so an earlier reader is not merely unlikely; it is
//! unrepresentable.
//!
//! # 2. What counts as having dealt with it
//!
//! A **reader**, and the list is exactly §5's four exclusions plus the test
//! itself. Each entry below is a test in `tests/unchecked_errors.rs` named for
//! it, because §5 is explicit that *"each of these is a false positive that
//! would make the diagnostic hated"* and §15 records that the list *"came from
//! reading the corpus, not from running the analysis over it"*.
//!
//! | | What | Why it is not a mistake |
//! |---|---|---|
//! | the test | `err?`, anywhere | the whole point |
//! | 1 | `return err`, or the body's tail | *"`return (value, err)` passes the obligation to the caller; that is the model working"* |
//! | 2 | an argument at a parameter of type `E?` | same reason |
//! | 3 | `let missing: Error? be null` | §3.1's own example; there is nothing to check |
//! | 4 | a function that cannot return | nothing downstream of it will ever see the error |
//! | — | `_err` | **Decision 10**: deliberate ignoring, spelled with a leading underscore |
//!
//! And three readers §5 does not name, each of which is a *refusal to guess*
//! rather than an addition:
//!
//! - **A `match` on it.** `match err:` with a `null` arm is a test by another
//!   spelling. Decision 16's exhaustiveness is not built, so this module cannot
//!   confirm the arms are the right ones — and reporting a binding the author
//!   plainly examined would be the fifth entry in the hated list.
//! - **An argument to a *method* whose parameter is of error type**, which is
//!   exclusion 2 again and no longer a refusal to guess. It was one: with no
//!   Decision 11 lookup a method call had no parameter type to compare
//!   against, so *any* argument to *any* method counted and `doc.show(err)`
//!   excused the binding. [`crate::methods`] resolves the call, so the same
//!   question the `Call` arm asks is asked here — *is the parameter at this
//!   position itself an `E?`* — and `doc.show(err)` reports again unless
//!   `show` takes one. A method this crate still cannot resolve falls back to
//!   the old answer, and [`method_takes_error`] says why that is the safe
//!   direction rather than an oversight.
//! - **Anything wrapped in a coercion.** `return err` against an `Error?`
//!   signature may box, and the box is a node ([`ExprKind::Coerce`]); the reader
//!   search looks through it, because the value that reached the `return` is
//!   still the binding.
//!
//! # 3. Decision 10, and what it costs
//!
//! > **Decision 10. Deliberately ignoring an error is spelled by binding it to
//! > a name beginning with an underscore.**
//!
//! One character, no grammar, and greppable. **The cost is the one §13 names**:
//! *"`_` as a prefix acquires meaning it did not have"*, and the resolver does
//! not treat it specially — so `_err` is an ordinary name that can be read,
//! shadowed and returned like any other, and the only thing the underscore does
//! is silence this one diagnostic. Nothing warns that a `_err` was used after
//! all, because the alternative convention (an underscore meaning *unused*)
//! is not the one Decision 10 chose.
//!
//! # 4. What this does *not* rely on
//!
//! **Not the narrowing.** `narrow`'s §4 leaves Decision 8's soundness resting
//! on a fact the region checker proves *later*, and Decision 25 makes that legal
//! only because narrowing's conclusion is not consumed before regions have run.
//! This pass reads the THIR's **structure** — is there a
//! [`Present`](ExprKind::Present) node over this place — and never a narrowed
//! type. So `SC0140` is not the THIR pass §15 warns about, and the phase
//! ordering stays an argument about nothing that has happened yet.
//!
//! # 5. The condition, which is §5's own and no longer narrower
//!
//! §5's condition is *"`E?` where `E` implements `Error`"*. This pass used to
//! recognise `E?` only where `E` was literally `any Error` — the type `Error?`
//! expands to — and a binding of a concrete `ConfigError?` was not reported,
//! because *does `S` implement `Error`* was `assign`'s §3 obligation and
//! nothing could discharge it.
//!
//! **[`Methods::implements`] discharges it, and this is the predicate that
//! section said it would hand over.** [`is_error_nullable`] now answers for
//! `any Error` *and* for any type the crate declares `implements Error:`, which
//! is what §5 wrote and what `examples/09_absence_and_failure.science` returns
//! from half its functions.
//!
//! **What the widening cannot see it does not report.** `methods`'s §4 answers
//! from written implementations only, so an error type reached through a type
//! parameter's bound is still not a candidate. That is the safe direction for
//! this diagnostic and the same one §2's list is written in.

use science_diagnostics::{Diagnostic, Diagnostics, Label, Span};
use science_resolve::hir::{self, DefId, DefTable};

use crate::assign::Coercions;
use crate::items::Declarations;
use crate::thir::{Block, Body, ExprId, ExprKind, Place, StmtKind};
use crate::ty::{Ty, TyKind, Types};

/// `SC0140`. `syntax-revision-2.md`'s code, implemented here.
///
/// Defined in this module and not in [`crate::codes`] because that module's own
/// documentation is about the two bands this crate *owns*, and this code is in
/// neither. A `pub const` here is the record that the code is borrowed.
pub const UNCHECKED_ERROR: science_diagnostics::Code = science_diagnostics::Code(140);

/// Reports every binding of error type the body never dealt with.
pub fn report(
    body: &Body,
    krate: &hir::Crate,
    decls: &Declarations,
    types: &Types,
    diagnostics: &mut Diagnostics,
) {
    let coercions = Coercions::of(&krate.defs);
    // Exclusion 4, both halves: the declared one — `-> Never` — and the one
    // that is a property of the body. A body that diverges on every path has no
    // return for the obligation to travel to.
    let declared_never = decls
        .signature(body.def())
        .map(|sig| !sig.can_return(decls.prelude(), types))
        .unwrap_or(false);
    if declared_never || body.diverges() {
        return;
    }

    for candidate in collect(body, decls, types, coercions, &krate.defs) {
        if !read_anywhere(body, decls, types, coercions, candidate.def) {
            let name = &krate.defs.get(candidate.def).name;
            diagnostics.push(unchecked_error(candidate.span, name));
        }
    }
}

/// A binding that owes a test.
struct Candidate {
    def: DefId,
    span: Span,
}

/// Every `let` binding of error type that is not already excused. §2.
///
/// Over [`Body::blocks`] rather than over the tree, for the reason that method
/// gives: a `let` inside a branch is a binding like any other, and a walk with
/// an arm per node kind is a walk that can forget one.
fn collect(
    body: &Body,
    decls: &Declarations,
    types: &Types,
    coercions: Coercions,
    defs: &DefTable,
) -> Vec<Candidate> {
    let mut out = Vec::new();
    for (_, block) in body.blocks() {
        for stmt in &block.stmts {
            let StmtKind::Let { bindings, value } = &stmt.kind else { continue };
            for (at, def) in bindings.iter().enumerate() {
                let Some(ty) = body.local_ty(*def) else { continue };
                if !is_error_nullable(decls, types, coercions, ty) {
                    continue;
                }
                // Decision 10.
                if defs.get(*def).name.starts_with('_') {
                    continue;
                }
                // Exclusion 3: `let missing: Error? be null`.
                if is_statically_null(body, *value, bindings.len(), at) {
                    continue;
                }
                out.push(Candidate { def: *def, span: defs.get(*def).span });
            }
        }
    }
    out
}

/// §5's condition: `E?` where `E` implements `Error`.
fn is_error_nullable(
    decls: &Declarations,
    types: &Types,
    coercions: Coercions,
    ty: Ty,
) -> bool {
    let TyKind::Nullable(inner) = types.kind(ty) else {
        return false;
    };
    if coercions.is_any_error(types, *inner) {
        return true;
    }
    // The concrete half, which is `assign`'s §3 obligation asked as a
    // question. A table with no prelude has no `Error` interface and answers
    // `false`, which is right for a compilation with no interfaces in it.
    let Some(error) = coercions.error_interface() else {
        return false;
    };
    // `implements` *admits* a type whose head it cannot see — a parameter,
    // `Self` — because that is the direction a coercion has to err in. Here it
    // is the wrong direction: a binding whose type this pass cannot classify
    // is not a binding it should demand a test for. So the head has to be a
    // type the index can answer about.
    matches!(types.kind(*inner), TyKind::Named { .. })
        && decls.methods().implements(types, *inner, error)
}

/// Exclusion 3. `let missing: Error? be null`, and the `err` half of a pair
/// whose initialiser is a literal tuple with `null` in that position.
fn is_statically_null(body: &Body, value: ExprId, bindings: usize, at: usize) -> bool {
    let value = peel(body, value);
    if bindings > 1 {
        if let ExprKind::Tuple(elements) = &body.expr(value).kind {
            return elements
                .get(at)
                .map(|element| is_null_literal(body, *element))
                .unwrap_or(false);
        }
        return false;
    }
    is_null_literal(body, value)
}

fn is_null_literal(body: &Body, id: ExprId) -> bool {
    matches!(&body.expr(peel(body, id)).kind, ExprKind::Literal(hir::Literal::Null))
}

/// Looks through the nodes that wrap a value without being one. §2.
fn peel(body: &Body, id: ExprId) -> ExprId {
    match &body.expr(id).kind {
        ExprKind::Coerce { operand, .. } | ExprKind::Narrow(operand) => peel(body, *operand),
        _ => id,
    }
}

/// Whether anything in the body dealt with this binding. §2's table.
fn read_anywhere(
    body: &Body,
    decls: &Declarations,
    types: &Types,
    coercions: Coercions,
    def: DefId,
) -> bool {
    let wanted = Place::local(def);
    for (id, expr) in body.exprs() {
        let dealt = match &expr.kind {
            // The test itself.
            ExprKind::Present(operand) => mentions(body, *operand, &wanted),
            // Exclusion 2, and the two refusals to guess: a method call has no
            // parameter type to compare against, so any argument to one counts.
            ExprKind::Call { callee, args } => {
                args.iter().enumerate().any(|(at, arg)| {
                    mentions(body, *arg, &wanted)
                        && argument_takes_error(body, decls, types, coercions, *callee, at)
                })
            }
            // Exclusion 2 at a method, which is the same question now that
            // there is a signature to ask it of. §2.
            ExprKind::MethodCall { receiver, method, args } => {
                mentions(body, *receiver, &wanted)
                    || args.iter().enumerate().any(|(at, arg)| {
                        mentions(body, *arg, &wanted)
                            && method_takes_error(decls, types, coercions, *method, at)
                    })
            }
            // A `match` on it. §2.
            ExprKind::Match { scrutinee, .. } => mentions(body, *scrutinee, &wanted),
            _ => false,
        };
        if dealt {
            return true;
        }
        let _ = id;
    }
    // Exclusion 1: returned, either by a `return` statement anywhere in the
    // body or as the body's own trailing expression.
    returned(body, &wanted)
}

/// Whether this expression *is* the place, through the nodes §2 looks past.
fn mentions(body: &Body, id: ExprId, wanted: &Place) -> bool {
    body.place_of(peel(body, id)).as_ref() == Some(wanted)
}

/// Whether the callee's parameter at this position is itself of error type.
///
/// A parameter that is not — `print(err)` — is not the obligation moving on,
/// it is the obligation being printed, and §5's second exclusion is about a
/// function *"taking `E?`"* for exactly that reason.
fn argument_takes_error(
    body: &Body,
    decls: &Declarations,
    types: &Types,
    coercions: Coercions,
    callee: ExprId,
    at: usize,
) -> bool {
    let ExprKind::Item(def) = body.expr(callee).kind else {
        // A closure value: its parameter types are in the callee's type.
        let callee_ty = body.ty(callee);
        return match types.kind(callee_ty) {
            TyKind::Closure { params, .. } => params
                .get(at)
                .map(|ty| is_error_nullable(decls, types, coercions, *ty))
                .unwrap_or(false),
            _ => false,
        };
    };
    decls
        .signature(def)
        .and_then(|sig| sig.params.get(at))
        .map(|param| is_error_nullable(decls, types, coercions, param.ty))
        .unwrap_or(false)
}

/// Whether a resolved method's parameter at this position is of error type.
///
/// **An *unresolved* method still excuses every argument**, and that is the
/// half of §2's old refusal that survives. With no candidate there is no
/// parameter list, so the choice is between excusing an argument that may be
/// being handled and reporting one that may be — and §5 of the note is explicit
/// that a false positive is what would make this diagnostic hated. The domain
/// is now the receivers `methods`'s §1 cannot speak for rather than every
/// method call in the language.
fn method_takes_error(
    decls: &Declarations,
    types: &Types,
    coercions: Coercions,
    method: Option<DefId>,
    at: usize,
) -> bool {
    let Some(method) = method else { return true };
    decls
        .signature(method)
        .and_then(|sig| sig.params.get(at))
        .map(|param| is_error_nullable(decls, types, coercions, param.ty))
        .unwrap_or(false)
}

/// Exclusion 1, over every block in the body.
///
/// The body's trailing expression is a return too (§4.4 makes a block's value
/// its last expression), so the root block's tail counts and an inner block's
/// does not — an inner block's value goes to whatever encloses it, and if that
/// is a `return` the statement arm has already found it.
fn returned(body: &Body, wanted: &Place) -> bool {
    for (id, block) in body.blocks() {
        let block: &Block = block;
        for stmt in &block.stmts {
            // `return (doc, err)` — Decision 14's own example. The obligation
            // travels with the tuple, and the element is the binding.
            if let StmtKind::Return(Some(value)) = &stmt.kind {
                if mentions(body, *value, wanted) || tuple_mentions(body, *value, wanted) {
                    return true;
                }
            }
        }
        if id == body.root() {
            if let Some(tail) = block.tail {
                if mentions(body, tail, wanted) || tuple_mentions(body, tail, wanted) {
                    return true;
                }
            }
        }
    }
    false
}

fn tuple_mentions(body: &Body, id: ExprId, wanted: &Place) -> bool {
    match &body.expr(peel(body, id)).kind {
        ExprKind::Tuple(elements) => {
            elements.iter().any(|element| mentions(body, *element, wanted))
        }
        _ => false,
    }
}

/// `SC0140`.
///
/// The message says what was not done rather than what was wrong, and the help
/// names Decision 10's spelling, because the two things an author does next are
/// *test it* and *say they meant not to* — and only one of them is guessable
/// from the code.
fn unchecked_error(span: Span, name: &str) -> Diagnostic {
    Diagnostic::error(UNCHECKED_ERROR, format!("`{name}` is never checked for an error"))
        .with_label(Label::primary(span, "bound here and never tested"))
        .with_note(format!(
            "test it with `if {name}?:`, pass it on, or return it — or rename it to `_{name}` \
             to say the failure is deliberately ignored"
        ))
}
