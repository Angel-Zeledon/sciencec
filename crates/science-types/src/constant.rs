//! Evaluating a module-level `const`'s initialiser (§4.4).
//!
//! # 1. What this closes
//!
//! `const WIDTH be 768` used to reach `crate::items::Declarations::item`'s
//! `Const` arm, which lowered the annotation if the author wrote one and
//! stored [`crate::ty::Ty::ERROR`] otherwise — over a comment claiming *"the
//! body walk is what learns it"*, although no such walk existed anywhere in
//! this crate. An unannotated constant's type was never learned, and
//! `science-mir`'s lowering emitted [`Constant::Item`] for every *reference*
//! to a constant,
//! annotated or not, which is the same node a reference to a function gets and
//! which every backend refuses to build: a constant "has no storage of its
//! own" (`examples/02_bindings.science`'s own comment on the construct) and a
//! backend that reaches [`Constant::Item`] finds no function to call and no
//! data-section symbol to load. [`codes::CONST_INITIALISER_NOT_A_LITERAL`]'s
//! doc comment in `lib.rs` has the full history and the diagnostic-numbering
//! argument; this module is the fix.
//!
//! # 2. Why a literal walk of the syntax, and not the bidirectional checker
//!
//! A constant's value has to be known before the first *body* in the crate is
//! checked, because any body may name it — `items.rs`'s own §1 states the
//! general rule this is the one exception to: *"a signature cannot depend on
//! anything a body computes."* Running `crate::check`'s `BodyChecker` on a
//! const's initialiser would mean resolving calls and methods through
//! [`crate::items::Declarations`] — the very table
//! [`crate::items::Declarations::of`] is still building when it reaches the
//! `Const` arm. There is no ordering that lets the ordinary checker go first.
//!
//! [`eval`] sidesteps the cycle instead of breaking it, by only accepting
//! §4.4's literal forms: a bare [`Literal`] needs no declaration to type, so
//! evaluating one is safe at the exact point every other declaration is
//! lowered, and [`natural_ty`] gives the same default an unsuffixed literal
//! would get from `BodyChecker::literal` — [`Prelude`]'s
//! `default_int`/`default_float`, and a suffix's own name otherwise — so an
//! unannotated `const WIDTH be 768` and a `let` binding of the same literal
//! agree on its type without this module re-deriving Decision 2 on its own.
//!
//! # 3. What is refused, and why that is not a narrower feature than it looks
//!
//! `const AREA be WIDTH * HEIGHT`, `const N be other_const`, and `const BAD be
//! side_effecting_call()` are refused identically: none is a bare [`Literal`]
//! node. The corpus this phase has to unblock —
//! `examples/02_bindings.science`'s `WIDTH` and `GREETING` — needs only the
//! literal case, and folding arithmetic or const-to-const references needs a
//! real evaluator with its own cycle detection (`const A be B` next to `const
//! B be A`), which is future work and a second slice. A code that admits more
//! than this module can evaluate is a code whose message is written against a
//! guess, which `const_expr.rs`'s own introduction makes the same argument
//! against for a different grammar.
//!
//! # 4. Why `lower` checks the literal against the type twice
//!
//! [`lower`] returns a value to substitute only when the literal's *kind*
//! agrees with the constant's *type* — an integer literal for one of §5.1's
//! integer primitives, a string literal for `String`, and so on
//! ([`literal_matches`]). Without this, `const X: F64 be 5` would substitute
//! [`Literal::Int`] wherever `X` is read, and
//! `science-codegen-llvm`'s `lower_operand` reads a [`Literal::Int`] as
//! [`Operand::ConstInt`] and a [`Literal::Float`] as
//! [`Operand::ConstFloat`](../../science_codegen_llvm/index.html) — two
//! different bit patterns placed at the same F64 slot, which is silent
//! miscompilation and not a diagnostic. Refusing to substitute in that case is
//! not a regression: [`Constant::Item`] is still what MIR emits, and the
//! backend's existing, honest refusal — *"this backend cannot build a constant
//! of a type this backend cannot build"* — is what the author sees, exactly as
//! it was before this module existed. **What does not need the check** is an
//! integer literal against a narrower or wider integer type than
//! [`Prelude::default_int`] would have chosen — `const X: I32 be 5` — because
//! an integer constant's width is supplied by the destination slot at codegen
//! and not carried on the literal (`science-codegen-llvm`'s `lower_operand`:
//! *"its width is the other operand's or the slot's"*), so [`Literal::Int`]
//! substitutes safely under any integer annotation.
//!
//! [`Constant::Item`]: ../../science_mir/mir/enum.Constant.html#variant.Item
//! [`Operand::ConstInt`]: ../../science_codegen_llvm/index.html
//! [`Operand::ConstFloat`]: ../../science_codegen_llvm/index.html

use science_diagnostics::{Diagnostic, Diagnostics, Label, Span};
use science_resolve::hir::{self, Literal};

use crate::codes;
use crate::items::Prelude;
use crate::ty::Ty;
use crate::ty::Types;

/// What a `const`'s declaration-time lowering settled: its type, and the
/// value MIR substitutes at every use if the initialiser turned out to be one.
pub struct Evaluated {
    /// The annotation's type if the author wrote one, otherwise the type
    /// [`natural_ty`] gives the initialiser's own literal (§4.4's inference).
    pub ty: Ty,
    /// The literal to substitute, present only when [`eval`] succeeded and
    /// [`literal_matches`] confirms it denotes a value of `ty`.
    pub value: Option<Literal>,
}

/// Lowers one `const`'s initialiser: §2's evaluation, §4.4's inference where
/// there is no annotation, and §4's compatibility check that guards
/// substitution.
///
/// `annotation` is the caller's own lowering of `konst.ty` — `crate::items`'s
/// `Const` arm already has its own private `lower` in scope for that, and
/// duplicating the call here would mean two diagnostics for one malformed
/// annotation.
pub fn lower(
    value: &hir::Expr,
    annotation: Option<Ty>,
    prelude: &Prelude,
    types: &mut Types,
    diagnostics: &mut Diagnostics,
) -> Evaluated {
    let literal = eval(value);
    let natural = literal.as_ref().ok().and_then(|literal| natural_ty(literal, prelude, types));
    let ty = annotation.unwrap_or(natural.unwrap_or(Ty::ERROR));
    let value = match &literal {
        Ok(literal) if literal_matches(literal, ty, prelude, types) => Some(literal.clone()),
        _ => None,
    };
    if let Err(diagnostic) = literal {
        diagnostics.push(diagnostic);
    }
    Evaluated { ty, value }
}

/// Turns a `const`'s initialiser into the literal it denotes, or says why it
/// is not one yet. §3's admitted set: a bare literal, and nothing else.
fn eval(value: &hir::Expr) -> Result<Literal, Diagnostic> {
    match &value.kind {
        hir::ExprKind::Literal(literal) => Ok(literal.clone()),
        _ => Err(not_a_literal(value.span)),
    }
}

/// The type an unannotated literal would be given, by the same rule
/// `crate::check`'s `BodyChecker::literal` applies inside a body: a suffix
/// names its own type, and a suffix-less number takes Decision 2's default.
///
/// `Bool`, `String` and `Char` literals carry no suffix and have exactly one
/// type each, so they are not "defaulted" in Decision 2's sense — they are
/// just named, the same one line further down that function takes for them.
///
/// `Literal::Null` answers `None`: its type is `T?` for whichever `T` the
/// context supplies, and a module-level `const` gives it no context to read
/// one from. `const X be null` therefore still reaches this phase as
/// [`Ty::ERROR`] when unannotated — the pre-existing admission, unchanged —
/// and even when annotated it is never substituted, because
/// [`literal_matches`] answers `false` for every type.
fn natural_ty(literal: &Literal, prelude: &Prelude, types: &mut Types) -> Option<Ty> {
    match literal {
        Literal::Bool(_) => prelude.ty(types, "Bool"),
        Literal::Str(_) => prelude.ty(types, "String"),
        Literal::Char(_) => prelude.ty(types, "Char"),
        Literal::Int { suffix: Some(suffix), .. } | Literal::Float { suffix: Some(suffix), .. } => {
            prelude.ty(types, crate::check::suffix_name(*suffix))
        }
        Literal::Int { suffix: None, .. } => prelude.default_int(types),
        Literal::Float { suffix: None, .. } => prelude.default_float(types),
        Literal::Null => None,
    }
}

/// Whether `literal` is a value of `ty` — §4's guard on substitution.
///
/// Kind-level, not width-level: every integer literal matches every one of
/// §5.1's integer types (the module doc's §4 says why the width does not have
/// to agree here), and likewise every float literal matches every float type.
/// `Literal::Null` matches nothing, for [`natural_ty`]'s reason.
fn literal_matches(literal: &Literal, ty: Ty, prelude: &Prelude, types: &Types) -> bool {
    match literal {
        Literal::Bool(_) => prelude.is(types, ty, "Bool"),
        Literal::Str(_) => prelude.is(types, ty, "String"),
        Literal::Char(_) => prelude.is(types, ty, "Char"),
        Literal::Int { .. } => prelude.is_integer(types, ty),
        Literal::Float { .. } => prelude.is_float(types, ty),
        Literal::Null => false,
    }
}

/// `SC0542` — an initialiser [`eval`] cannot yet turn into a value.
fn not_a_literal(span: Span) -> Diagnostic {
    Diagnostic::error(
        codes::CONST_INITIALISER_NOT_A_LITERAL,
        "this `const` initialiser is not a literal",
    )
    .with_label(Label::primary(span, "not a literal this compiler can evaluate yet"))
    .with_note(
        "a `const`'s value has to be known before any body in the crate is checked, so today \
         only §4.4's literal forms are admitted — a call, an operator, or another `const` \
         named here is refused for the same reason a `const` cannot recurse into itself",
    )
}
