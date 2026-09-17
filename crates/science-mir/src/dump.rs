//! A textual MIR, for a test to read and a human to check.
//!
//! # 1. Why a dump and not a `Debug`
//!
//! `science-parser` and `science-resolve` both carry one of these and both give
//! the same reason: a derived `Debug` prints the representation, and what a
//! test wants to assert about is the *shape*. A renamed field breaks every
//! snapshot; a changed lowering breaks the ones about the lowering.
//!
//! **This dump prints no [`science_types::ty::Ty`] and no
//! [`science_diagnostics::Span`].** A type is an interned index whose number
//! depends on how many types were interned first, and a span is a byte offset
//! that moves when a fixture gains a line. Both would make a snapshot fail for
//! a reason that is not about MIR. Where a type genuinely matters — a
//! projection's — a caller renders it itself with
//! [`science_types::ty::Types::render`].
//!
//! # 2. What it does print, and why each
//!
//! Every local with its kind, so that a drop flag is visible as one; every
//! block with every statement; and each borrow's kind with its reservation and
//! activation points, because those two numbers are §10 item 4 and a dump that
//! omitted them would let the two-phase lowering rot unobserved.

use std::fmt::Write;

use science_resolve::hir::DefTable;

use crate::mir::{
    BorrowKind, Callee, Constant, LocalKind, Operand, Place, Projection, Rvalue, StatementKind,
    TerminatorKind,
};
use crate::Body;

/// Renders one body.
pub fn body(defs: &DefTable, body: &Body) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "fn {}:", defs.get(body.def()).name);
    for (local, decl) in body.locals() {
        let kind = match decl.kind {
            LocalKind::Return => "return".to_string(),
            LocalKind::Param(def) => format!("param {}", defs.get(def).name),
            LocalKind::Binding(def) => format!("let {}", defs.get(def).name),
            LocalKind::Temp => "temp".to_string(),
            LocalKind::DropFlag(guarded) => format!("drop flag for {guarded}"),
        };
        let _ = writeln!(out, "    {local}: {kind}");
    }
    for data in body.borrows() {
        let kind = match data.kind {
            BorrowKind::Shared => "shared",
            BorrowKind::Exclusive => "exclusive",
            BorrowKind::TwoPhase => "two-phase",
        };
        let activation = match data.activation {
            Some(point) => format!(", activated {point}"),
            None => String::new(),
        };
        let _ = writeln!(
            out,
            "    borrow {}: {kind} of {}, reserved {}{activation}",
            data.id.index(),
            place(defs, &data.place),
            data.reserved
        );
    }
    for (id, block) in body.blocks() {
        let _ = writeln!(out, "  {id}:");
        for statement in &block.statements {
            let _ = writeln!(out, "    {}", self::statement(defs, &statement.kind));
        }
        let _ = writeln!(out, "    {}", terminator(defs, &block.terminator.kind));
    }
    out
}

fn statement(defs: &DefTable, kind: &StatementKind) -> String {
    match kind {
        StatementKind::Assign { place: target, rvalue: value } => {
            format!("{} = {}", place(defs, target), rvalue(defs, value))
        }
        StatementKind::StorageLive(local) => format!("StorageLive({local})"),
        StatementKind::StorageDead(local) => format!("StorageDead({local})"),
        StatementKind::SetDropFlag { flag, value } => format!("{flag} = {value}"),
        StatementKind::Activate(borrow) => format!("activate borrow {}", borrow.index()),
        StatementKind::Nop => "nop".to_string(),
    }
}

fn terminator(defs: &DefTable, kind: &TerminatorKind) -> String {
    match kind {
        TerminatorKind::Goto { target } => format!("goto {target}"),
        TerminatorKind::If { cond, then_block, else_block } => {
            format!("if {} then {then_block} else {else_block}", operand(defs, cond))
        }
        TerminatorKind::Switch { discr, arms, otherwise } => {
            let arms: Vec<String> = arms
                .iter()
                .map(|(variant, block)| format!("{} -> {block}", defs.get(*variant).name))
                .collect();
            format!("switch {} [{}] otherwise {otherwise}", operand(defs, discr), arms.join(", "))
        }
        TerminatorKind::Call { callee, args, destination, target } => {
            let args: Vec<String> = args.iter().map(|arg| operand(defs, arg)).collect();
            let name = match callee {
                Callee::Def(def) => defs.get(*def).name.to_string(),
                Callee::Indirect(operand) => format!("({})", self::operand(defs, operand)),
                Callee::Runtime(name) => format!("runtime {name}"),
                Callee::Unresolved(which) => format!("<unresolved {which:?}>"),
            };
            let target = match target {
                Some(target) => format!(" -> {target}"),
                None => " -> diverges".to_string(),
            };
            format!("{} = {name}({}){target}", place(defs, destination), args.join(", "))
        }
        TerminatorKind::Drop { place: target, flag, target: next } => {
            let flag = match flag {
                Some(flag) => format!(" if {flag}"),
                None => String::new(),
            };
            format!("drop {}{flag} -> {next}", place(defs, target))
        }
        TerminatorKind::Return => "return".to_string(),
        TerminatorKind::Unreachable => "unreachable".to_string(),
    }
}

fn place(defs: &DefTable, place: &Place) -> String {
    let mut out = place.local.to_string();
    for step in &place.projection {
        match step {
            Projection::Deref { .. } => out = format!("(*{out})"),
            Projection::Field { field, .. } => {
                let _ = write!(out, ".{}", defs.get(*field).name);
            }
            Projection::TupleField { index, .. } => {
                let _ = write!(out, ".{index}");
            }
            Projection::Index { index, .. } => {
                let _ = write!(out, "[{index}]");
            }
            Projection::Downcast { variant, .. } => {
                let _ = write!(out, " as {}", defs.get(*variant).name);
            }
        }
    }
    out
}

fn operand(defs: &DefTable, operand: &Operand) -> String {
    match operand {
        Operand::Copy(target) => format!("copy {}", place(defs, target)),
        Operand::Move(target) => format!("move {}", place(defs, target)),
        Operand::Const(Constant::Unit) => "()".to_string(),
        Operand::Const(Constant::Item(def)) => defs.get(*def).name.to_string(),
        // Suffixed, so a dump reader can tell §1.7's computed capacity from a
        // `57` the author wrote. `Constant::Count`'s own note is the reason the
        // two are different values and not one.
        Operand::Const(Constant::Count(count)) => format!("{count}usize"),
        Operand::Const(Constant::Literal(literal)) => literal_text(literal),
    }
}

fn literal_text(literal: &science_resolve::hir::Literal) -> String {
    use science_resolve::hir::Literal;
    match literal {
        Literal::Int { value, .. } => value.to_string(),
        Literal::Float { value, .. } => format!("{value:?}"),
        Literal::Str(text) => format!("{text:?}"),
        Literal::Char(value) => format!("{value:?}"),
        Literal::Bool(value) => value.to_string(),
        Literal::Null => "null".to_string(),
    }
}

fn rvalue(defs: &DefTable, rvalue: &Rvalue) -> String {
    match rvalue {
        Rvalue::Use(value) => operand(defs, value),
        Rvalue::Ref { kind, place: target, borrow } => {
            let kind = match kind {
                BorrowKind::Shared => "borrowed",
                BorrowKind::Exclusive => "mutable borrowed",
                BorrowKind::TwoPhase => "two-phase borrowed",
            };
            format!("{kind} {} (borrow {})", place(defs, target), borrow.index())
        }
        Rvalue::Unary { op, operand: value } => format!("{} {}", op.as_str(), operand(defs, value)),
        Rvalue::Binary { op, lhs, rhs } => {
            format!("{} {} {}", operand(defs, lhs), op.as_str(), operand(defs, rhs))
        }
        Rvalue::Cast { operand: value, .. } => format!("{} as _", operand(defs, value)),
        Rvalue::Record { def, fields } => {
            let fields: Vec<String> = fields
                .iter()
                .map(|(field, value)| format!("{}: {}", defs.get(*field).name, operand(defs, value)))
                .collect();
            format!("{}({})", defs.get(*def).name, fields.join(", "))
        }
        Rvalue::Variant { variant, payload } => {
            let payload: Vec<String> = payload.iter().map(|value| operand(defs, value)).collect();
            format!("{}({})", defs.get(*variant).name, payload.join(", "))
        }
        Rvalue::Tuple(elements) => {
            let elements: Vec<String> = elements.iter().map(|value| operand(defs, value)).collect();
            format!("({})", elements.join(", "))
        }
        Rvalue::Range { start, end, inclusive } => {
            let dots = if *inclusive { "..=" } else { ".." };
            format!("{}{dots}{}", operand(defs, start), operand(defs, end))
        }
        Rvalue::IsPresent(value) => format!("{}?", operand(defs, value)),
        Rvalue::Discriminant(target) => format!("discriminant({})", place(defs, target)),
        Rvalue::Coerce { operand: value, coercion, .. } => {
            format!("{} as {coercion:?}", operand(defs, value))
        }
        Rvalue::Narrow { operand: value, .. } => format!("narrow {}", operand(defs, value)),
        Rvalue::Closure { captures, .. } => {
            let captures: Vec<String> = captures.iter().map(|it| operand(defs, it)).collect();
            format!("closure[{}]", captures.join(", "))
        }
        Rvalue::Error => "{error}".to_string(),
    }
}
