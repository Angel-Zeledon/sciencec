//! `hir::Type` into [`Ty`] — the one door into the type table.
//!
//! # 1. What this phase decides, and what it refuses to
//!
//! The resolver left three questions open in a type position, and each one is
//! open because answering it needs something the resolver does not have.
//!
//! - **Is this argument a type or a value?** `hir::DefKind::is_type` admits
//!   `ConstParam` into a type position and says why: *"§5.3 puts const
//!   arguments in the same `of (..)` list as type arguments, so `Grid of (T,
//!   ROWS)` reaches a const parameter through a type position"*. Here the
//!   definition's kind settles it, and the answer is recorded once in
//!   [`GenericArg`] so nothing downstream asks again.
//! - **Is `Error?` an interface or an object?** Already settled — the resolver
//!   expands a bare interface under a `?` in `resolve_nullable_inner`, so by
//!   the time this phase sees it, it is a `hir::TypeKind::Any` and there is
//!   nothing here to decide. §3 says why that matters for `T??`.
//! - **Does `T??` mean anything?** Decision 6 says no, and this is the phase
//!   that says so: `SC0520`, in §3.
//!
//! And what it refuses: **arity**. `Map of (String, Int, Bool)` is wrong, and
//! `hir::GenericArity` is the type that says so — but computing it needs a
//! declaration's `GenericParam` list, because variadic-ness is a property of
//! a const parameter's *kind* and a `DefTable` does not record kinds. That
//! list lives on the items, which the checker walks and this function is not
//! given. So every argument written is lowered, the count is whatever was
//! written, and the arity check happens where both sides are in hand. Lowering
//! an argument the declaration cannot take is not a lie — it is a type with
//! too many arguments, which is exactly what the reader wrote.
//!
//! # 2. Failure is a type, not an absence
//!
//! Every arm that cannot produce a real type produces [`Ty::ERROR`] and keeps
//! going, which is the parser's discipline with `ast::TypeKind::Error` and the
//! resolver's with `Res::Error`, one phase further down. `ty`'s §5 is what
//! makes that cheap: an erroneous type is compatible with everything, so the
//! recovery costs one diagnostic rather than one per use.
//!
//! The same discipline covers a const argument that does not lower: it becomes
//! [`GenericArg::Error`], not a `0` that would compare equal to somebody's
//! extent.
//!
//! **And the diagnostic the resolver already gave is not given twice.** A
//! const expression mentioning a name that did not resolve is skipped before
//! `const_expr::lower` is called, because that function's
//! `unresolved_const_param` is documented as *"a silent refusal by design: the
//! caller drops it, because a second diagnostic for one mistake is how a
//! compiler acquires cascades"* — and this is the caller.
//!
//! # 3. `SC0520`, and why it is reported here rather than in the table
//!
//! > **Decision 6.** `T??` does not exist — the parser builds two `Nullable`
//! > nodes and this phase collapses them with `SC0520`, because a presence
//! > test on a presence test is never what anyone meant.
//!
//! The parser's `parse_type` comment says the same from the other side: the
//! `?` suffix is a loop *"so that `T??` produces one node per `?` and is
//! rejected by the phase that can say why — the parser refusing it here would
//! have to explain a type system it cannot see"*. This is that phase.
//!
//! [`Types::nullable`] is idempotent, so the collapsing is the table's and
//! needs no help. What is left for this function is the *report*, and it has
//! to be here for a reason that is not convenience: the table sees one
//! nullable type meeting the `nullable` constructor, and cannot tell a `T??`
//! somebody wrote from a `T?` that a later substitution put where a `T?`
//! already was. Only the first is a mistake. Two `hir::TypeKind::Nullable`
//! nodes with two spans are the evidence, and this is the only phase holding
//! them.
//!
//! The label lands on the **second `?`** rather than on the whole type,
//! because that is the character to delete and the suggestion offers deleting
//! it. The span is the sliver between the inner node's end and the outer
//! node's end, which is what the parser's `start.merge(last_text_span)`
//! leaves; when that sliver is not well formed — a synthesised node, a
//! recovery — the whole type is blamed instead, because a label pointing at
//! nothing is worse than a label pointing at too much.
//!
//! # 4. `SC0523`, which the note did not name
//!
//! `type-checking-and-mir.md` §13 claims `SC0520`–`SC0579` and names three:
//! `SC0520`, `SC0521` and `SC0522`. It does not name the one this phase needs
//! straight away.
//!
//! `parse_type_atom` accepts a const expression wherever it parses a type,
//! because *"nothing else in a type position can be a literal, so there is no
//! ambiguity to resolve"* — which is true inside an `of (..)` list and false
//! outside one. So `def f(x: 4)` parses, resolves without complaint (there is
//! no name in it), and arrives here as a `hir::TypeKind::Const` in a position
//! that wants a type. The same hole admits `def f(x: ROWS)` for a const
//! parameter `ROWS`, by the `is_type` compromise §1 quotes.
//!
//! Nobody upstream has reported either, and lowering them to [`Ty::ERROR`] in
//! silence would produce a program that checks as though the annotation were
//! fine and says nothing at all. So `SC0523` is allocated from the block §13
//! already claims, and it covers both spellings: a const expression, and a
//! const parameter, where a type is expected.

use science_diagnostics::{Diagnostic, Diagnostics, Label, Span, Suggestion};
use science_resolve::hir::{self, DefKind, DefTable, Res};

use crate::codes;
use crate::const_expr;
use crate::diagnostics;
use crate::normal::{normalise, AtomOrder, NormalForm};
use crate::ty::{GenericArg, Ty, TyKind, Types};

/// Lowers resolved types into the interned table.
///
/// Holds `&mut` to the two things it writes — the table and the diagnostics —
/// and `&` to the two it reads. That is four references rather than four
/// arguments at every recursive call, and it is the shape a Science port wants
/// too: the interner is owned elsewhere and borrowed exclusively for the
/// duration, which is `mutable self` seen from the caller's side (Decision
/// 24).
pub struct TypeLowerer<'a> {
    types: &'a mut Types,
    defs: &'a DefTable,
    order: &'a AtomOrder,
    diagnostics: &'a mut Diagnostics,
}

impl<'a> TypeLowerer<'a> {
    /// `order` is the crate's [`AtomOrder`], built once.
    ///
    /// Once, and not once per type: a rank is only meaningful against the
    /// ordering that produced it, so two orders over one crate would intern
    /// `Matrix of (T, N)` twice under two different atoms. That is `normal`'s
    /// §2 failure — a key that moves — reached through this crate instead of
    /// through the command line.
    pub fn new(
        types: &'a mut Types,
        defs: &'a DefTable,
        order: &'a AtomOrder,
        diagnostics: &'a mut Diagnostics,
    ) -> TypeLowerer<'a> {
        TypeLowerer { types, defs, order, diagnostics }
    }

    /// The table being written, for a caller that wants to build a type
    /// directly between two lowerings.
    pub fn types(&mut self) -> &mut Types {
        self.types
    }

    /// Lowers a type in a **type position**: a parameter's annotation, a
    /// field, a return type, an element of a tuple.
    ///
    /// A const expression is not a type, so one here is `SC0523` and
    /// [`Ty::ERROR`]. Inside an `of (..)` list it is an argument and
    /// [`Self::lower_arg`] takes it; the two positions are different and this
    /// is the difference.
    pub fn lower(&mut self, ty: &hir::Type) -> Ty {
        match &ty.kind {
            hir::TypeKind::Path { res, generics } => self.lower_path(*res, generics, ty.span),
            hir::TypeKind::Borrowed { mutable, inner } => {
                let inner = self.lower(inner);
                self.types.borrowed(*mutable, inner)
            }
            hir::TypeKind::Any(bound) => match &bound.kind {
                hir::BoundKind::Interface { res, generics } => {
                    let res = *res;
                    let generics = generics.clone();
                    let args = self.lower_args(&generics);
                    match res {
                        Res::Def(interface) => self.types.object(interface, args),
                        // `any Self` and a bound that did not resolve are both
                        // already reported; neither is an interface object.
                        Res::SelfTy(_) | Res::Error => Ty::ERROR,
                    }
                }
                // `parse_type_atom` reads the bound after `any` in
                // `BoundPosition::Interface`, and that position admits no
                // closure bound — so `any (A) -> B` does not parse and this
                // arm exists because `hir::BoundKind` has two variants, not
                // because the grammar has two. Whoever makes it parse owes a
                // `TyKind` for it; until then there is nothing to lower and
                // nothing to report that the parser has not already said.
                hir::BoundKind::Closure { .. } => Ty::ERROR,
            },
            hir::TypeKind::Tuple(elements) => {
                let elements = elements.iter().map(|element| self.lower(element)).collect();
                self.types.tuple(elements)
            }
            hir::TypeKind::Closure { params, ret } => {
                let params = params.iter().map(|param| self.lower(param)).collect();
                let ret = self.lower(ret);
                self.types.closure(params, ret)
            }
            hir::TypeKind::Unit => Ty::UNIT,
            hir::TypeKind::Nullable(inner) => {
                let lowered = self.lower(inner);
                if matches!(self.types.kind(lowered), TyKind::Nullable(_)) {
                    let diagnostic = double_nullable(ty.span, inner.span);
                    self.diagnostics.push(diagnostic);
                }
                self.types.nullable(lowered)
            }
            hir::TypeKind::SelfType(res) => match res {
                Res::SelfTy(owner) => self.types.self_type(*owner),
                // `Self` outside an implementation is `SC0208`, already said.
                Res::Def(_) | Res::Error => Ty::ERROR,
            },
            hir::TypeKind::SelfAssoc { res, .. } => match res {
                Res::Def(assoc) => self.types.self_assoc(*assoc),
                Res::SelfTy(_) | Res::Error => Ty::ERROR,
            },
            // §4. A value where a type belongs.
            hir::TypeKind::Const(_) => {
                let diagnostic = const_where_type_expected(
                    ty.span,
                    "this is a const expression, which denotes a value",
                );
                self.diagnostics.push(diagnostic);
                Ty::ERROR
            }
            hir::TypeKind::Error => Ty::ERROR,
        }
    }

    /// Lowers one entry of an `of (..)` list, which may be a type or a value.
    ///
    /// The two shapes a const argument arrives in are not alike and both are
    /// here:
    ///
    /// - `hir::TypeKind::Const` — the `4` of `Window of (Int, 4)`, and every
    ///   arithmetic form of §2.1. The parser knew it was not a type.
    /// - `hir::TypeKind::Path` at a `DefKind::ConstParam` — the `ROWS` of
    ///   `Grid of (T, ROWS)`. The parser could not know, the resolver would
    ///   have had to know the declared arity to know, and here the definition
    ///   kind answers it in one lookup.
    pub fn lower_arg(&mut self, ty: &hir::Type) -> GenericArg {
        match &ty.kind {
            hir::TypeKind::Const(expr) => self.lower_const(expr, ty.span),
            hir::TypeKind::Path { res: Res::Def(def), .. }
                if self.defs.get(*def).kind == DefKind::ConstParam =>
            {
                // A bare parameter is the normal form `1·p`. Its own generic
                // arguments, if the author wrote any, are meaningless on a
                // value and are left for the arity check §1 defers.
                GenericArg::Const(NormalForm::atom(self.order.atom(*def), ty.span))
            }
            _ => GenericArg::Type(self.lower(ty)),
        }
    }

    fn lower_args(&mut self, generics: &[hir::Type]) -> Vec<GenericArg> {
        generics.iter().map(|generic| self.lower_arg(generic)).collect()
    }

    /// The same, for a caller outside this module: `items`' §4a lowers the
    /// arguments of an `implements I of Args` bound, which is a list of
    /// [`hir::Type`] in exactly the shape a generic use writes.
    pub fn lower_args_public(&mut self, generics: &[hir::Type]) -> Vec<GenericArg> {
        self.lower_args(generics)
    }

    fn lower_path(&mut self, res: Res, generics: &[hir::Type], span: Span) -> Ty {
        let def = match res {
            Res::Def(def) => def,
            Res::SelfTy(owner) => return self.types.self_type(owner),
            Res::Error => return Ty::ERROR,
        };
        match self.defs.get(def).kind {
            // §14 refuses higher-kinded types, so a type parameter takes no
            // arguments. Any written are still lowered — a bad const literal
            // inside one is a mistake worth reporting — and then dropped,
            // because there is nowhere in `TyKind::Param` for them to go and
            // the arity check that names them is §1's deferral.
            DefKind::TypeParam => {
                self.lower_args(generics);
                self.types.param(def)
            }
            // §4. `def f(x: ROWS)`: a value where a type belongs.
            DefKind::ConstParam => {
                let diagnostic = const_where_type_expected(
                    span,
                    "this is a const generic parameter, which stands for a value",
                );
                self.diagnostics.push(diagnostic);
                Ty::ERROR
            }
            _ => {
                let args = self.lower_args(generics);
                self.types.named(def, args)
            }
        }
    }

    /// Normalises a const argument, or says why it is not one.
    ///
    /// Two failures and two different silences. A const expression naming
    /// something that did not resolve is skipped entirely — §2 — because
    /// resolution has already reported it. Everything else `const_expr::lower`
    /// or [`normalise`] refuses is reported, because nobody else has.
    fn lower_const(&mut self, expr: &hir::ConstExpr, span: Span) -> GenericArg {
        if mentions_unresolved(expr) {
            return GenericArg::Error;
        }
        let lowered = match const_expr::lower(expr, self.order) {
            Ok(lowered) => lowered,
            Err(diagnostic) => {
                self.diagnostics.push(diagnostic);
                return GenericArg::Error;
            }
        };
        match normalise(&lowered) {
            Ok(form) => GenericArg::Const(form),
            Err(error) => {
                self.diagnostics.push(diagnostics::overflowed(error, span));
                GenericArg::Error
            }
        }
    }
}

/// Whether a const expression mentions a name that did not resolve.
///
/// Asked *before* lowering rather than sorted out afterwards, because every
/// refusal `const_expr::lower` produces carries the same code and there is no
/// way to tell them apart once they are diagnostics. The predicate is three
/// lines and the alternative is a second `SC0260` for a name the resolver
/// already reported as `SC0200`.
fn mentions_unresolved(expr: &hir::ConstExpr) -> bool {
    match &expr.kind {
        hir::ConstExprKind::Lit(_) => false,
        hir::ConstExprKind::Param(res) => res.is_error(),
        hir::ConstExprKind::Neg(operand)
        | hir::ConstExprKind::Mul { operand, .. }
        | hir::ConstExprKind::Div { operand, .. } => mentions_unresolved(operand),
        hir::ConstExprKind::Add(left, right) | hir::ConstExprKind::Sub(left, right) => {
            mentions_unresolved(left) || mentions_unresolved(right)
        }
    }
}

/// `SC0520` — `T??`.
///
/// `outer` is the whole `T??`; `inner` is the `T?` underneath it. The label
/// goes on what lies between them, which is the second `?`. §3 says why the
/// fallback exists.
fn double_nullable(outer: Span, inner: Span) -> Diagnostic {
    let redundant = if outer.file == inner.file && inner.end > inner.start && outer.end > inner.end
    {
        Span::new(outer.file, inner.end, outer.end)
    } else {
        outer
    };
    Diagnostic::error(codes::DOUBLE_NULLABLE, "a type is nullable once")
        .with_label(Label::primary(redundant, "this `?` adds nothing the first one did not"))
        .with_label(Label::secondary(inner, "already nullable here"))
        .with_note(
            "`T?` is a distinct type holding a `T` or `null`, not a union, so there is no \
             second absence for a second `?` to describe",
        )
        .with_suggestion(Suggestion {
            span: redundant,
            replacement: String::new(),
            message: "the type is `T?`".to_string(),
        })
}

/// `SC0523` — a value where a type is expected. §4.
fn const_where_type_expected(span: Span, found: &str) -> Diagnostic {
    Diagnostic::error(codes::CONST_WHERE_TYPE_EXPECTED, "a const expression is not a type")
        .with_label(Label::primary(span, found))
        .with_note(
            "const expressions are generic arguments — the `4` of `Window[Int, 4]` — so \
             one belongs inside an `of (..)` list and nowhere else",
        )
}
