//! The whole pipeline, for the tests that need an expression.
//!
//! `tests/common` builds const-expression trees by hand and says why: *"the
//! tree is the contract"* for an algebra whose surface syntax is half landed.
//! This harness is the opposite case and needs the opposite answer. The
//! contract of the checking layer is *"what the author wrote checks"*, and the
//! only way to have what the author wrote is to run the lexer, the parser and
//! the resolver over it. A hand-built `hir::Expr` would test the checker
//! against a tree shape the resolver may never produce — and the three
//! questions §4.4 hands the resolver (binding or variant, call or construction,
//! positional or named) are settled *in* that tree, so a hand-built one would
//! be testing against guesses about all three.
//!
//! `tests/relations.rs` made the same choice for the same reason and this is
//! its harness one layer up.

#![allow(dead_code)]

use science_diagnostics::{Diagnostics, FileId};
use science_resolve::hir::{self, DefId, DefKind};
use science_types::items::Declarations;
use science_types::thir::{Body, ExprId, ExprKind};
use science_types::{check_crate, Aliases, AtomOrder, Ty, Types};

/// One program, lexed, parsed, resolved, and checked.
pub struct Checked {
    pub krate: hir::Crate,
    pub types: Types,
    pub decls: Declarations,
    pub bodies: Vec<Body>,
    /// Everything the alias pass, the declaration pass and the bodies
    /// reported. Resolution's own diagnostics are [`Checked::resolution`].
    pub diagnostics: Diagnostics,
    pub resolution: Diagnostics,
}

/// Checks a program that must lex, parse and resolve cleanly.
///
/// The assertions are the ones `tests/relations.rs` makes and for its reason: a
/// `Res::Error` hiding in a fixture makes every type compatible with every
/// other one (`ty`'s §5), so a fixture that does not resolve is a fixture whose
/// assertions all pass for the wrong reason.
pub fn check(source: &str) -> Checked {
    let checked = check_allowing_resolution_errors(source);
    assert!(
        !checked.resolution.has_errors(),
        "the fixture must resolve: {:?}",
        checked.resolution.iter().map(|d| (d.code.0, d.message.clone())).collect::<Vec<_>>()
    );
    checked
}

/// The same, for a fixture that deliberately does not resolve.
pub fn check_allowing_resolution_errors(source: &str) -> Checked {
    let file = FileId(0);
    let (tokens, lexed) = science_lexer::lex(file, source);
    assert!(
        !lexed.has_errors(),
        "the fixture must lex: {:?}",
        lexed.iter().map(|d| (d.code.0, d.message.clone())).collect::<Vec<_>>()
    );
    let (ast, parsed) = science_parser::parse_module(&tokens, file);
    assert!(
        !parsed.has_errors(),
        "the fixture must parse: {:?}",
        parsed.iter().map(|d| (d.code.0, d.message.clone())).collect::<Vec<_>>()
    );
    let (krate, resolution) = science_resolve::resolve_module(file, "checking.science", &ast);

    let order = AtomOrder::of(&krate.defs);
    let mut types = Types::new();
    let mut diagnostics = Diagnostics::new();
    let mut aliases = Aliases::of(&krate, &mut types, &order, &mut diagnostics);
    let decls = Declarations::of(&krate, &mut types, &order, &mut diagnostics);
    let bodies =
        check_crate(&krate, &decls, &mut types, &mut aliases, &order, &mut diagnostics);
    Checked { krate, types, decls, bodies, diagnostics, resolution }
}

impl Checked {
    /// Every code the checker reported, in order.
    pub fn codes(&self) -> Vec<u16> {
        self.diagnostics.iter().map(|diagnostic| diagnostic.code.0).collect()
    }

    /// Every message, for a test that wants to read one.
    pub fn messages(&self) -> Vec<String> {
        self.diagnostics.iter().map(|diagnostic| diagnostic.message.clone()).collect()
    }

    /// Asserts the checker was silent, and says what it said if it was not.
    pub fn assert_clean(&self) {
        assert!(
            self.diagnostics.is_empty(),
            "expected no diagnostics, got {:?}",
            self.diagnostics
                .iter()
                .map(|d| (d.code.0, d.message.clone()))
                .collect::<Vec<_>>()
        );
    }

    /// The body of the named function.
    pub fn body(&self, name: &str) -> &Body {
        self.bodies
            .iter()
            .find(|body| self.krate.defs.get(body.def()).name == name)
            .unwrap_or_else(|| panic!("the fixture declares no body for `{name}`"))
    }

    /// A type as a diagnostic spells it.
    pub fn render(&self, ty: Ty) -> String {
        self.types.render(&self.krate.defs, ty)
    }

    /// The definition with this name and kind.
    pub fn def(&self, name: &str, kind: DefKind) -> DefId {
        self.krate
            .defs
            .iter()
            .find(|def| def.name == name && def.kind == kind)
            .unwrap_or_else(|| panic!("the fixture declares no {kind:?} named `{name}`"))
            .id
    }

    /// Every node in the body, rendered as `kind : type`.
    ///
    /// A shape test rather than a snapshot: the point of Decision 3's first
    /// clause is that *every* node has a type, and the cheapest way to assert
    /// that is to look at all of them.
    pub fn nodes(&self, name: &str) -> Vec<(String, String)> {
        let body = self.body(name);
        body.exprs()
            .map(|(_, expr)| (describe(&expr.kind), self.render(expr.ty)))
            .collect()
    }

    /// The first node of the body whose kind matches the predicate.
    pub fn find(&self, name: &str, mut matches: impl FnMut(&ExprKind) -> bool) -> ExprId {
        let body = self.body(name);
        body.exprs()
            .find(|(_, expr)| matches(&expr.kind))
            .map(|(id, _)| id)
            .expect("no node of that shape in the body")
    }
}

/// A node kind's name, for a readable assertion.
pub fn describe(kind: &ExprKind) -> String {
    match kind {
        ExprKind::Literal(_) => "literal",
        ExprKind::Local(_) => "local",
        ExprKind::SelfValue(_) => "self",
        ExprKind::Item(_) => "item",
        ExprKind::Call { .. } => "call",
        ExprKind::MethodCall { .. } => "method",
        ExprKind::Field { .. } => "field",
        ExprKind::Index { .. } => "index",
        ExprKind::Record { .. } => "record",
        ExprKind::Tuple(_) => "tuple",
        ExprKind::Unit => "unit",
        ExprKind::Unary { .. } => "unary",
        ExprKind::Binary { .. } => "binary",
        ExprKind::Cast { .. } => "cast",
        ExprKind::Present(_) => "present",
        ExprKind::Borrow { .. } => "borrow",
        ExprKind::Range { .. } => "range",
        ExprKind::Closure { .. } => "closure",
        ExprKind::If { .. } => "if",
        ExprKind::Match { .. } => "match",
        ExprKind::Loop { .. } => "loop",
        ExprKind::For { .. } => "for",
        ExprKind::Block(_) => "block",
        ExprKind::Unsafe(_) => "unsafe",
        ExprKind::Coerce { .. } => "coerce",
        ExprKind::Narrow(_) => "narrow",
        ExprKind::Error => "error",
    }
    .to_string()
}
