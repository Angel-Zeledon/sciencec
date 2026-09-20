//! One generic body, at one set of arguments.
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
//! This is that application. Given a body and a [`Substitution`], it returns a
//! body in which every [`Ty`] has been substituted and nothing else has
//! changed.
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
    let mut out = body.clone();
    for local in &mut out.locals {
        instantiate_local(local, subst, types)?;
    }
    for block in &mut out.blocks {
        instantiate_block(block, subst, types)?;
    }
    for borrow in &mut out.borrows {
        instantiate_borrow(borrow, subst, types)?;
    }
    Ok(out)
}

/// One type. Every other function here is a walk down to a call of this.
fn ty(target: &mut Ty, subst: &Substitution, types: &mut Types) -> Result<(), ConstEvalError> {
    *target = subst.apply(types, *target)?;
    Ok(())
}

fn instantiate_local(
    local: &mut LocalDecl,
    subst: &Substitution,
    types: &mut Types,
) -> Result<(), ConstEvalError> {
    // `kind` is a `DefId` or nothing and `span` is source text; neither is a
    // type and neither moves when the arguments do.
    ty(&mut local.ty, subst, types)
}

fn instantiate_block(
    block: &mut BasicBlock,
    subst: &Substitution,
    types: &mut Types,
) -> Result<(), ConstEvalError> {
    for statement in &mut block.statements {
        instantiate_statement(statement, subst, types)?;
    }
    instantiate_terminator(&mut block.terminator, subst, types)
}

fn instantiate_statement(
    statement: &mut Statement,
    subst: &Substitution,
    types: &mut Types,
) -> Result<(), ConstEvalError> {
    match &mut statement.kind {
        StatementKind::Assign { place, rvalue } => {
            instantiate_place(place, subst, types)?;
            instantiate_rvalue(rvalue, subst, types)
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

fn instantiate_terminator(
    terminator: &mut Terminator,
    subst: &Substitution,
    types: &mut Types,
) -> Result<(), ConstEvalError> {
    match &mut terminator.kind {
        TerminatorKind::If { cond, .. } => instantiate_operand(cond, subst, types),
        TerminatorKind::Switch { discr, .. } => instantiate_operand(discr, subst, types),
        TerminatorKind::Call { callee, args, destination, .. } => {
            instantiate_callee(callee, subst, types)?;
            for arg in args {
                instantiate_operand(arg, subst, types)?;
            }
            instantiate_place(destination, subst, types)
        }
        TerminatorKind::Drop { place, .. } => instantiate_place(place, subst, types),
        TerminatorKind::Goto { .. } | TerminatorKind::Return | TerminatorKind::Unreachable => {
            Ok(())
        }
    }
}

/// **A [`Callee::Def`] is left alone on purpose**; the module header says why.
fn instantiate_callee(
    callee: &mut Callee,
    subst: &Substitution,
    types: &mut Types,
) -> Result<(), ConstEvalError> {
    match callee {
        Callee::Indirect(operand) => instantiate_operand(operand, subst, types),
        Callee::Def(_) | Callee::Runtime(_) | Callee::Unresolved(_) => Ok(()),
    }
}

fn instantiate_place(
    place: &mut Place,
    subst: &Substitution,
    types: &mut Types,
) -> Result<(), ConstEvalError> {
    for step in &mut place.projection {
        match step {
            Projection::Deref { ty: t }
            | Projection::Field { ty: t, .. }
            | Projection::TupleField { ty: t, .. }
            | Projection::Index { ty: t, .. }
            | Projection::Downcast { ty: t, .. } => ty(t, subst, types)?,
        }
    }
    Ok(())
}

fn instantiate_operand(
    operand: &mut Operand,
    subst: &Substitution,
    types: &mut Types,
) -> Result<(), ConstEvalError> {
    match operand {
        Operand::Copy(place) | Operand::Move(place) => instantiate_place(place, subst, types),
        // A constant's type is on the statement beside it, which
        // `Constant::Literal`'s own documentation states and this relies on:
        // there is nothing in here to substitute.
        Operand::Const(
            Constant::Literal(_) | Constant::Item(_) | Constant::Count(_) | Constant::Unit,
        ) => Ok(()),
    }
}

fn instantiate_rvalue(
    rvalue: &mut Rvalue,
    subst: &Substitution,
    types: &mut Types,
) -> Result<(), ConstEvalError> {
    match rvalue {
        Rvalue::Use(operand) | Rvalue::Unary { operand, .. } | Rvalue::IsPresent(operand) => {
            instantiate_operand(operand, subst, types)
        }
        Rvalue::Ref { place, .. } | Rvalue::Discriminant(place) => {
            instantiate_place(place, subst, types)
        }
        Rvalue::Binary { lhs, rhs, .. } => {
            instantiate_operand(lhs, subst, types)?;
            instantiate_operand(rhs, subst, types)
        }
        // **Both** types, and `from` is the one a walk writes by hand and
        // forgets: `Rvalue::Cast`'s own documentation says the signedness of
        // the *source* decides `sext` against `zext`, so a `from` left at `T`
        // is a sign extension chosen from a parameter.
        Rvalue::Cast { operand, from, ty: t } => {
            instantiate_operand(operand, subst, types)?;
            ty(from, subst, types)?;
            ty(t, subst, types)
        }
        Rvalue::Record { fields, .. } => {
            for (_, operand) in fields {
                instantiate_operand(operand, subst, types)?;
            }
            Ok(())
        }
        Rvalue::Variant { payload: operands, .. } | Rvalue::Tuple(operands) => {
            for operand in operands {
                instantiate_operand(operand, subst, types)?;
            }
            Ok(())
        }
        Rvalue::Range { start, end, .. } => {
            instantiate_operand(start, subst, types)?;
            instantiate_operand(end, subst, types)
        }
        Rvalue::Coerce { operand, ty: t, .. } | Rvalue::Narrow { operand, ty: t } => {
            instantiate_operand(operand, subst, types)?;
            ty(t, subst, types)
        }
        Rvalue::Closure { captures, ty: t, .. } => {
            for capture in captures {
                instantiate_operand(capture, subst, types)?;
            }
            ty(t, subst, types)
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
fn instantiate_borrow(
    borrow: &mut BorrowData,
    subst: &Substitution,
    types: &mut Types,
) -> Result<(), ConstEvalError> {
    instantiate_place(&mut borrow.place, subst, types)?;
    instantiate_place(&mut borrow.destination, subst, types)
}
