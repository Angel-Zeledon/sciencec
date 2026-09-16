//! The const-expression grammar of §2.1, as the checker's own tree.
//!
//! # Why this is not `hir::ConstExpr`
//!
//! `hir::ConstExpr` is the parser's node re-exported, and it is deliberately
//! the *syntax*: it carries [`Literal`], so it carries floats, strings and
//! suffixes, because §2.3's house pattern is that the parser accepts whatever
//! it finds in a const-argument position and a later phase says what is wrong
//! with it. This tree is what is left after that phase has run. Every node in
//! it denotes an integer, every literal is already an `i128`, and every name
//! is already a [`DefId`]. [`lower`] is the only way in, and it is where
//! `SC0260`'s literal clause is reported.
//!
//! # The shape of the tree is the linearity argument
//!
//! §2.1's `ConstTerm` productions require an `IntLiteral` on one side of `*`
//! and `/`. [`ConstExprKind::Scale`] honours that by holding its factor as an
//! `i128` field rather than as a second operand. A `Mul(Box, Box)` node would
//! parse the same programs and would move §2.1's central claim — *"linearity
//! is enforced by the productions, not by a check after parsing"* — into a
//! check run after parsing. The cost of the faithful node is that `2 * N` and
//! `N * 2` are the same node, which is not a loss: they are the same
//! expression, and §3.3 requires them to be the same type.
//!
//! # What the AST cannot express yet
//!
//! `ast::ConstExprKind` has two variants today, `Lit` and `Neg`: the unary
//! half of §10.1 item 1, and nothing else. There is no `Param`, no `Add`, no
//! `Mul`. So [`lower`]'s match has two arms, and [`ConstExprKind`] has six.
//! That gap is not a defect in this module — it is the rest of item 1, and the
//! match in [`lower`] is exhaustive on the AST, so the day the AST grows an
//! `Add` this file stops compiling and points at the line that has to answer
//! for it. That is the forcing function, and it is why the match is written
//! out rather than ending in a wildcard.

use std::fmt::Write as _;

use science_diagnostics::{Diagnostic, Span};
use science_resolve::hir::{self, DefId, DefTable, Literal};

use crate::diagnostics;

/// One node of a const expression, with where it came from.
///
/// The `X { kind, span }` / `XKind` split is the parser's and the HIR's, kept
/// for the reason they both give: the span lives in one place, and every
/// traversal can ask any node where it came from without matching on it first.
/// §9.3 makes that load-bearing rather than tidy — the derivation block in a
/// unit diagnostic prints one row per contributing operand *and its span*, so
/// a node that lost its span is a row that cannot be printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstExpr {
    pub kind: ConstExprKind,
    pub span: Span,
}

/// §2.1's six forms, and no seventh.
///
/// > **Decision 2.2.** No `%`, no `**`, no `max`, no `min`, no comparison, no
/// > conditional, no function call, no `const fn`, no recursion, no
/// > user-defined const operation of any kind. There is no escape hatch and
/// > none is planned.
///
/// Every addition to this enum is an addition to [`crate::normalise`], to
/// [`crate::equal`], to the renderer below, and to the monomorphisation key —
/// which is the reason §2.2 spends a table on who asked for each exclusion and
/// why each was refused.
///
/// The one form of §2.1 that is missing is `/`. It is missing because its
/// result is a quotient atom (§4.1 treatment (b)), quotient atoms are F1
/// (§10.2), and a `Div` node in F0 would be a node whose normal form does not
/// exist yet. When it arrives it is one variant here and one variant on
/// [`crate::Atom`]; nothing else moves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstExprKind {
    /// An integer literal, already signed.
    ///
    /// Signed because §10.1 item 2 says so: `Literal::Int`'s `u128` cannot
    /// hold the `-1` that half of every SI dimension vector is made of, and
    /// [`lower`] folds a negated literal into this variant at the boundary so
    /// that `-2^127` — representable although `+2^127` is not — survives.
    Lit(i128),
    /// A const generic parameter, or a `with`-bound one (§8.3).
    ///
    /// This is the atom of the normal form. It is a [`DefId`] and not a name,
    /// because two parameters called `N` in two scopes are two parameters and
    /// §3.3 must not call them equal.
    Param(DefId),
    /// `-e`.
    Neg(Box<ConstExpr>),
    /// `e₁ + e₂`.
    Add(Box<ConstExpr>, Box<ConstExpr>),
    /// `e₁ - e₂`.
    ///
    /// A separate variant rather than `Add(a, Neg(b))` so that a diagnostic can
    /// point at the `-` the programmer wrote. §3.2 normalises it as
    /// `MERGE(NORMALISE(e₁), NEGATE(NORMALISE(e₂)))`, which is the same thing,
    /// one line later.
    Sub(Box<ConstExpr>, Box<ConstExpr>),
    /// `e * d` and `d * e`, which are one operation (§2.1's two `ConstTerm`
    /// productions for `*`).
    ///
    /// `factor_span` is the literal's own span, kept because "this factor is
    /// where the wrong exponent came from" is a sentence §9.3's derivation
    /// block has to be able to write.
    Scale { operand: Box<ConstExpr>, factor: i128, factor_span: Span },
}

impl ConstExpr {
    pub fn lit(value: i128, span: Span) -> ConstExpr {
        ConstExpr { kind: ConstExprKind::Lit(value), span }
    }

    pub fn param(def: DefId, span: Span) -> ConstExpr {
        ConstExpr { kind: ConstExprKind::Param(def), span }
    }

    pub fn neg(operand: ConstExpr, span: Span) -> ConstExpr {
        ConstExpr { kind: ConstExprKind::Neg(Box::new(operand)), span }
    }

    pub fn add(left: ConstExpr, right: ConstExpr, span: Span) -> ConstExpr {
        ConstExpr { kind: ConstExprKind::Add(Box::new(left), Box::new(right)), span }
    }

    pub fn sub(left: ConstExpr, right: ConstExpr, span: Span) -> ConstExpr {
        ConstExpr { kind: ConstExprKind::Sub(Box::new(left), Box::new(right)), span }
    }

    /// `operand * factor`. `factor_span` points at the literal.
    pub fn scale(operand: ConstExpr, factor: i128, factor_span: Span, span: Span) -> ConstExpr {
        ConstExpr {
            kind: ConstExprKind::Scale { operand: Box::new(operand), factor, factor_span },
            span,
        }
    }

    /// The expression as the programmer wrote it, near enough to print.
    ///
    /// This is the left-hand column of §9.2's block — *"left `n` normalised
    /// `n`"* — and it is rendered from the tree rather than sliced out of the
    /// source, because a const argument can reach a comparison through a type
    /// alias and then there is no source span that contains the whole of it.
    /// Parentheses are emitted only where precedence needs them, so the
    /// rendering of a round-trip is the expression and not a thicket.
    ///
    /// One thing it does not recover: §2.1 admits `d * e` and `e * d` and
    /// [`ConstExprKind::Scale`] is deliberately one node for both, so a
    /// product always prints operand-first — `ORDER * 3` for a source that
    /// said `3 * ORDER`. Keeping the side the literal was written on would
    /// mean a field that equality has to ignore, which is the field
    /// `Term::provenance` already is and one of those is enough.
    pub fn render(&self, defs: &DefTable) -> String {
        let mut out = String::new();
        self.render_at(defs, Precedence::Lowest, &mut out);
        out
    }

    fn render_at(&self, defs: &DefTable, outer: Precedence, out: &mut String) {
        let mine = self.precedence();
        let wrap = mine < outer;
        if wrap {
            out.push('(');
        }
        match &self.kind {
            ConstExprKind::Lit(value) => {
                let _ = write!(out, "{value}");
            }
            ConstExprKind::Param(def) => out.push_str(&defs.get(*def).name),
            ConstExprKind::Neg(operand) => {
                out.push('-');
                operand.render_at(defs, Precedence::Unary, out);
            }
            ConstExprKind::Add(left, right) => {
                left.render_at(defs, Precedence::Additive, out);
                out.push_str(" + ");
                // The right operand of a left-associative operator needs the
                // next level up, or `a - (b - c)` prints as `a - b - c`.
                right.render_at(defs, Precedence::Multiplicative, out);
            }
            ConstExprKind::Sub(left, right) => {
                left.render_at(defs, Precedence::Additive, out);
                out.push_str(" - ");
                right.render_at(defs, Precedence::Multiplicative, out);
            }
            ConstExprKind::Scale { operand, factor, .. } => {
                operand.render_at(defs, Precedence::Multiplicative, out);
                let _ = write!(out, " * {factor}");
            }
        }
        if wrap {
            out.push(')');
        }
    }

    fn precedence(&self) -> Precedence {
        match &self.kind {
            ConstExprKind::Lit(_) | ConstExprKind::Param(_) => Precedence::Atom,
            ConstExprKind::Neg(_) => Precedence::Unary,
            ConstExprKind::Add(..) | ConstExprKind::Sub(..) => Precedence::Additive,
            ConstExprKind::Scale { .. } => Precedence::Multiplicative,
        }
    }
}

/// §4.6's table, restricted to the five operators §2.1 admits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Precedence {
    Lowest,
    Additive,
    Multiplicative,
    Unary,
    Atom,
}

/// Turns a resolved const argument into a const expression, or says why it is
/// not one.
///
/// This is `SC0260`'s literal clause and the only place it is reported. The
/// three refusals:
///
/// - a literal that is not an integer — `Window of (Int, "a")` parses, per
///   `ast::ConstExprKind::Lit`, and is refused here;
/// - an integer literal with a suffix — `4i32` names a *type* in a position
///   that has no types in it, and the fix is to delete the suffix, which is
///   offered;
/// - an integer literal whose magnitude leaves `i128`.
///
/// The third one defers to [`hir::ConstExpr::as_i128`] rather than reading the
/// `u128` itself, because that function is the single place the project says
/// what a const argument's signed value is, and `-2^127` is exactly the case
/// where reading the magnitude and negating it afterwards gets a different
/// answer.
pub fn lower(expr: &hir::ConstExpr) -> Result<ConstExpr, Diagnostic> {
    match &expr.kind {
        hir::ConstExprKind::Lit(literal) => {
            check_integer_literal(literal, expr.span)?;
            let value = expr.as_i128().ok_or_else(|| diagnostics::literal_too_large(expr.span))?;
            Ok(ConstExpr::lit(value, expr.span))
        }
        hir::ConstExprKind::Neg(operand) => {
            // A negated literal folds here rather than becoming `Neg(Lit(..))`,
            // because the magnitude of `i128::MIN` is one past `i128::MAX` and
            // the fold is the only reading that keeps it.
            if let hir::ConstExprKind::Lit(literal) = &operand.kind {
                check_integer_literal(literal, operand.span)?;
                let value =
                    expr.as_i128().ok_or_else(|| diagnostics::literal_too_large(expr.span))?;
                return Ok(ConstExpr::lit(value, expr.span));
            }
            Ok(ConstExpr::neg(lower(operand)?, expr.span))
        }
        // The rest landed when the parser grew §2.1's grammar. This match was
        // written exhaustive on purpose so that day would fail to compile
        // here rather than silently lower half a language.
        hir::ConstExprKind::Param(res) => match res.def_id() {
            Some(def) => Ok(ConstExpr::param(def, expr.span)),
            // The name resolved to nothing, and resolution said so. Lowering
            // a second diagnostic for the same mistake is how a compiler
            // acquires cascades.
            None => Err(diagnostics::unresolved_const_param(expr.span)),
        },
        hir::ConstExprKind::Add(lhs, rhs) => {
            Ok(ConstExpr::add(lower(lhs)?, lower(rhs)?, expr.span))
        }
        hir::ConstExprKind::Sub(lhs, rhs) => {
            Ok(ConstExpr::sub(lower(lhs)?, lower(rhs)?, expr.span))
        }
        hir::ConstExprKind::Mul { operand, factor, factor_span } => {
            check_integer_literal(factor, *factor_span)?;
            let value = literal_value(factor, *factor_span)?;
            Ok(ConstExpr::scale(lower(operand)?, value, *factor_span, expr.span))
        }
        // `e / k` is §4's quotient and F0 is the quotient-free fragment, so
        // this is refused rather than lowered into a node the normaliser has
        // no rule for. Accepting it and normalising it wrongly would be the
        // worse failure: the checker would assert an equality that is false.
        hir::ConstExprKind::Div { divisor_span, .. } => {
            Err(diagnostics::division_is_f1(*divisor_span))
        }
    }
}

/// The signed value of an integer literal, once it is known to be one.
fn literal_value(literal: &Literal, span: Span) -> Result<i128, Diagnostic> {
    match literal {
        Literal::Int { value, .. } => {
            i128::try_from(*value).map_err(|_| diagnostics::literal_too_large(span))
        }
        _ => Err(diagnostics::non_integer_literal(span, "not an integer")),
    }
}

/// Refuses every literal that is not an unsuffixed integer.
fn check_integer_literal(literal: &Literal, span: Span) -> Result<(), Diagnostic> {
    match literal {
        Literal::Int { suffix: None, .. } => Ok(()),
        Literal::Int { suffix: Some(_), .. } => Err(diagnostics::suffixed_literal(span)),
        Literal::Float { .. } => Err(diagnostics::non_integer_literal(span, "a float")),
        Literal::Str(_) => Err(diagnostics::non_integer_literal(span, "a string")),
        Literal::Char(_) => Err(diagnostics::non_integer_literal(span, "a character")),
        Literal::Bool(_) => Err(diagnostics::non_integer_literal(span, "a boolean")),
        Literal::Null => Err(diagnostics::non_integer_literal(span, "`null`")),
    }
}
