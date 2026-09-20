//! One body, with every type it mentions rewritten.
//!
//! # What this is for
//!
//! `codegen-and-linking.md` Decision 42 puts the monomorphisation walk above
//! the backend, and `science_codegen::mono` is that walk: it answers *"which
//! **instances** does this program need"* and hands the backend a set. The
//! backend then refused every generic one, and `science-codegen-llvm`'s
//! `lower_crate` says exactly why:
//!
//! > a *generic* instance is still refused one layer down in
//! > `science_signature`, because lowering one body twice at two argument
//! > types needs the substitution applied to every type the body mentions and
//! > this crate does not apply it yet.
//!
//! This is that application. [`instantiate`] takes a body and a
//! [`Substitution`] and returns a body in which every [`Ty`] has been
//! substituted and nothing else has changed.
//!
//! It is written on top of [`map_types`], which is the walk itself, because a
//! second caller wants the same eleven positions and a different rewrite:
//! alias expansion. That function's own comment is the argument for the
//! split.
//!
//! # Decision: substitute the MIR, not the lowering
//!
//! The other way to spend this is to carry a [`Substitution`] inside the
//! backend's lowerer and apply it at every point the lowerer reads a type.
//! That was rejected.
//!
//! **The reason is how each one fails.** A backend that substitutes as it goes
//! fails by *forgetting a site*: one `layout_of_ty` that read the unsubstituted
//! type computes a layout for `T` where `F64` was meant, and a wrong layout is
//! a wrong offset, which is a program that runs and prints the wrong number. A
//! body substituted up front fails by *leaving a `TyKind::Param` in the body* —
//! and `layout_of_ty` already refuses one of those by name. So a missed site
//! here is a refusal at the same boundary that refuses an un-monomorphised
//! generic today, which is the failure the compiler is already built to
//! report.
//!
//! **The second reason is that the walk is closed and the lowering is not.**
//! Eleven syntactic positions in [`crate::mir`] hold a [`Ty`], they are all in
//! one file, and every `match` below is exhaustive — no `..` — so adding a
//! twelfth breaks this file at compile time. The backend's reads of a type are
//! spread over thousands of lines and no compiler error marks a new one.
//!
//! **The cost** is a copy of the body per instance, which is what
//! monomorphisation costs by definition: `identity[Int]` and `identity[F64]`
//! are two functions and two functions need two bodies. It is paid once per
//! instance at compile time and nothing survives into the image.
//!
//! # What is deliberately *not* rewritten
//!
//! A [`Callee::Def`] still names a definition and carries no arguments, and
//! that is not an omission. `science_codegen::mono`'s `solve_call` recovers a
//! callee's instantiation by unifying its declared signature against the
//! **actual** types of the arguments at the call — and after this function has
//! run, those actual types are concrete. So the instance a call needs is
//! recoverable from the instantiated body by the inference that already
//! exists, and adding arguments to [`Callee`] would be a second spelling of a
//! fact the body already carries. `science-mir`'s own §10 item 2 is the same
//! rule one level down.
//!
//! [`Callee`]: crate::mir::Callee
//! [`Callee::Def`]: crate::mir::Callee::Def

use science_types::subst::Substitution;
use science_types::{ConstEvalError, Ty, Types};

use crate::mir::{
    BasicBlock, Body, BorrowData, Callee, Constant, LocalDecl, Operand, Place, Projection, Rvalue,
    Statement, StatementKind, Terminator, TerminatorKind,
};

/// `body`, with `subst` applied to every type it mentions.
///
/// The [`ConstEvalError`] is [`Substitution::apply`]'s own: a const argument
/// that does not evaluate. It is returned rather than swallowed for `ty`'s §5
/// reason — an instance that vanished is a link error with no author — and the
/// caller reports it against the instance it was instantiating.
pub fn instantiate(
    body: &Body,
    subst: &Substitution,
    types: &mut Types,
) -> Result<Body, ConstEvalError> {
    map_types(body, &mut |ty| subst.apply(types, ty))
}

/// `body`, with `f` applied to every type it mentions.
///
/// # Why the walk is separate from what it applies
///
/// [`instantiate`] was the first caller and the walk was written inside it.
/// The second caller wants something else entirely: alias expansion.
/// `crate::alias`'s §1 keeps an alias's own [`Ty`] in the table and computes
/// the alias-free form beside it, deliberately, *"against a diagnostic that
/// cannot say `Embedding`"* — so a `Counter` declared as `type Counter is
/// I64` reaches a backend still spelled `Counter`, and a backend that has
/// never heard of aliases refuses it.
///
/// Both are the same walk over the same eleven positions and neither is the
/// other's business, so the walk is the function and what it applies is the
/// argument. A third caller that has to visit every type in a body — a pass
/// that normalises, or one that counts — gets it for free and, more to the
/// point, gets the **exhaustiveness**: every `match` below is written out
/// with no `..`, so a twelfth type-bearing position breaks this file at
/// compile time rather than being silently skipped by one caller.
pub fn map_types<F>(body: &Body, f: &mut F) -> Result<Body, ConstEvalError>
where
    F: FnMut(Ty) -> Result<Ty, ConstEvalError>,
{
    let mut out = body.clone();
    for local in &mut out.locals {
        instantiate_local(local, f)?;
    }
    for block in &mut out.blocks {
        instantiate_block(block, f)?;
    }
    for borrow in &mut out.borrows {
        instantiate_borrow(borrow, f)?;
    }
    Ok(out)
}

/// One type. Every other function here is a walk down to a call of this.
fn ty<F>(target: &mut Ty, f: &mut F) -> Result<(), ConstEvalError>
where
    F: FnMut(Ty) -> Result<Ty, ConstEvalError>,
{
    *target = f(*target)?;
    Ok(())
}

fn instantiate_local<F>(
    local: &mut LocalDecl,
    f: &mut F,
) -> Result<(), ConstEvalError>
where
    F: FnMut(Ty) -> Result<Ty, ConstEvalError>,
{
    // `kind` is a `DefId` or nothing and `span` is source text; neither is a
    // type and neither moves when the arguments do.
    ty(&mut local.ty, f)
}

fn instantiate_block<F>(
    block: &mut BasicBlock,
    f: &mut F,
) -> Result<(), ConstEvalError>
where
    F: FnMut(Ty) -> Result<Ty, ConstEvalError>,
{
    for statement in &mut block.statements {
        instantiate_statement(statement, f)?;
    }
    instantiate_terminator(&mut block.terminator, f)
}

fn instantiate_statement<F>(
    statement: &mut Statement,
    f: &mut F,
) -> Result<(), ConstEvalError>
where
    F: FnMut(Ty) -> Result<Ty, ConstEvalError>,
{
    match &mut statement.kind {
        StatementKind::Assign { place, rvalue } => {
            instantiate_place(place, f)?;
            instantiate_rvalue(rvalue, f)
        }
        // A local index, a drop flag, a borrow id, a nop: no type in any of
        // them. Listed rather than wildcarded so that a statement kind that
        // grows one fails to compile here.
        StatementKind::StorageLive(_)
        | StatementKind::StorageDead(_)
        | StatementKind::SetDropFlag { .. }
        | StatementKind::Activate(_)
        | StatementKind::Nop => Ok(()),
    }
}

fn instantiate_terminator<F>(
    terminator: &mut Terminator,
    f: &mut F,
) -> Result<(), ConstEvalError>
where
    F: FnMut(Ty) -> Result<Ty, ConstEvalError>,
{
    match &mut terminator.kind {
        TerminatorKind::If { cond, .. } => instantiate_operand(cond, f),
        TerminatorKind::Switch { discr, .. } => instantiate_operand(discr, f),
        TerminatorKind::Call { callee, args, destination, .. } => {
            instantiate_callee(callee, f)?;
            for arg in args {
                instantiate_operand(arg, f)?;
            }
            instantiate_place(destination, f)
        }
        TerminatorKind::Drop { place, .. } => instantiate_place(place, f),
        TerminatorKind::Goto { .. } | TerminatorKind::Return | TerminatorKind::Unreachable => {
            Ok(())
        }
    }
}

/// **A [`Callee::Def`] is left alone on purpose**; the module header says why.
fn instantiate_callee<F>(
    callee: &mut Callee,
    f: &mut F,
) -> Result<(), ConstEvalError>
where
    F: FnMut(Ty) -> Result<Ty, ConstEvalError>,
{
    match callee {
        Callee::Indirect(operand) => instantiate_operand(operand, f),
        Callee::Def(_) | Callee::Runtime(_) | Callee::Unresolved(_) => Ok(()),
    }
}

fn instantiate_place<F>(
    place: &mut Place,
    f: &mut F,
) -> Result<(), ConstEvalError>
where
    F: FnMut(Ty) -> Result<Ty, ConstEvalError>,
{
    for step in &mut place.projection {
        match step {
            Projection::Deref { ty: t }
            | Projection::Field { ty: t, .. }
            | Projection::TupleField { ty: t, .. }
            | Projection::Index { ty: t, .. }
            | Projection::Downcast { ty: t, .. } => ty(t, f)?,
        }
    }
    Ok(())
}

fn instantiate_operand<F>(
    operand: &mut Operand,
    f: &mut F,
) -> Result<(), ConstEvalError>
where
    F: FnMut(Ty) -> Result<Ty, ConstEvalError>,
{
    match operand {
        Operand::Copy(place) | Operand::Move(place) => instantiate_place(place, f),
        // A constant's type is on the statement beside it, which
        // `Constant::Literal`'s own documentation states and this relies on:
        // there is nothing in here to substitute.
        Operand::Const(
            Constant::Literal(_) | Constant::Item(_) | Constant::Count(_) | Constant::Unit,
        ) => Ok(()),
    }
}

fn instantiate_rvalue<F>(
    rvalue: &mut Rvalue,
    f: &mut F,
) -> Result<(), ConstEvalError>
where
    F: FnMut(Ty) -> Result<Ty, ConstEvalError>,
{
    match rvalue {
        Rvalue::Use(operand) | Rvalue::Unary { operand, .. } | Rvalue::IsPresent(operand) => {
            instantiate_operand(operand, f)
        }
        Rvalue::Ref { place, .. } | Rvalue::Discriminant(place) => {
            instantiate_place(place, f)
        }
        Rvalue::Binary { lhs, rhs, .. } => {
            instantiate_operand(lhs, f)?;
            instantiate_operand(rhs, f)
        }
        // **Both** types, and `from` is the one a walk writes by hand and
        // forgets: `Rvalue::Cast`'s own documentation says the signedness of
        // the *source* decides `sext` against `zext`, so a `from` left at `T`
        // is a sign extension chosen from a parameter.
        Rvalue::Cast { operand, from, ty: t } => {
            instantiate_operand(operand, f)?;
            ty(from, f)?;
            ty(t, f)
        }
        Rvalue::Record { fields, .. } => {
            for (_, operand) in fields {
                instantiate_operand(operand, f)?;
            }
            Ok(())
        }
        Rvalue::Variant { payload: operands, .. } | Rvalue::Tuple(operands) => {
            for operand in operands {
                instantiate_operand(operand, f)?;
            }
            Ok(())
        }
        Rvalue::Range { start, end, .. } => {
            instantiate_operand(start, f)?;
            instantiate_operand(end, f)
        }
        Rvalue::Coerce { operand, ty: t, .. } | Rvalue::Narrow { operand, ty: t } => {
            instantiate_operand(operand, f)?;
            ty(t, f)
        }
        Rvalue::Closure { captures, ty: t, .. } => {
            for capture in captures {
                instantiate_operand(capture, f)?;
            }
            ty(t, f)
        }
        Rvalue::Error => Ok(()),
    }
}

/// A loan's two places.
///
/// **The loan set is rewritten and not dropped**, because the instantiated body
/// is what a backend lowers and [`Body::borrows`] is what tells it which
/// assignments are references. Region inference has already run on the generic
/// body — it is a front-end question and its answer does not depend on the
/// arguments — so what survives here is the record, not a second analysis.
fn instantiate_borrow<F>(
    borrow: &mut BorrowData,
    f: &mut F,
) -> Result<(), ConstEvalError>
where
    F: FnMut(Ty) -> Result<Ty, ConstEvalError>,
{
    instantiate_place(&mut borrow.place, f)?;
    instantiate_place(&mut borrow.destination, f)
}
