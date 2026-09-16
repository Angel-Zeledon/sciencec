//! Builders that hand the const layer a tree, and a `DefTable` to read names
//! from.
//!
//! These tests do not go through the parser for the algebra, for the reason
//! `science-resolve`'s own harness gives: a test that went through the parser
//! would be a test of two phases at once, and it could only reach the trees the
//! current surface syntax happens to produce — which, for const expressions, is
//! a literal and a negated literal and nothing else (§10.1 item 1 is half
//! landed). The tree is the contract, so the tests build one.
//!
//! `tests/atom_order.rs` is the deliberate exception: the claim it checks is
//! about the *resolver's* numbering, so it has to be the real resolver.
//!
//! Spans come from a counter that hands out a fresh, non-overlapping range per
//! node. The numbers are arbitrary; what matters is that no two nodes share
//! one, so a test that expects provenance to be carried can tell which operand
//! it came from.

#![allow(dead_code)]

use std::cell::Cell;

use science_diagnostics::{FileId, Span};
use science_resolve::hir::{DefId, DefKind, DefTable};
use science_types::ConstExpr;

pub const FILE: FileId = FileId(0);

/// A definition table plus a span counter: everything a const expression needs
/// around it.
pub struct Scope {
    pub defs: DefTable,
    root: DefId,
    next: Cell<u32>,
}

impl Default for Scope {
    fn default() -> Self {
        Self::new()
    }
}

impl Scope {
    pub fn new() -> Scope {
        let mut defs = DefTable::new();
        // Every table has a root; a const parameter with no parent would be a
        // shape the resolver never produces.
        let root = defs.alloc(DefKind::Module, "", Span::at(FILE, 0), None);
        Scope { defs, root, next: Cell::new(1) }
    }

    /// A fresh span, one byte wide, sharing no byte with any other.
    pub fn span(&self) -> Span {
        let start = self.next.get();
        self.next.set(start + 2);
        Span::new(FILE, start, start + 1)
    }

    /// Declares a const parameter and hands back its id.
    ///
    /// Ids come out in declaration order, which is what the resolver does and
    /// what the atom order depends on.
    pub fn param(&mut self, name: &str) -> DefId {
        let span = self.span();
        let root = self.root;
        self.defs.alloc(DefKind::ConstParam, name, span, Some(root))
    }

    pub fn lit(&self, value: i128) -> ConstExpr {
        ConstExpr::lit(value, self.span())
    }

    pub fn param_expr(&self, def: DefId) -> ConstExpr {
        ConstExpr::param(def, self.span())
    }

    pub fn neg(&self, operand: ConstExpr) -> ConstExpr {
        ConstExpr::neg(operand, self.span())
    }

    pub fn add(&self, left: ConstExpr, right: ConstExpr) -> ConstExpr {
        ConstExpr::add(left, right, self.span())
    }

    pub fn sub(&self, left: ConstExpr, right: ConstExpr) -> ConstExpr {
        ConstExpr::sub(left, right, self.span())
    }

    pub fn scale(&self, operand: ConstExpr, factor: i128) -> ConstExpr {
        ConstExpr::scale(operand, factor, self.span(), self.span())
    }
}
