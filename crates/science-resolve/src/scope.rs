//! Lexical scopes, as a stack of ribs.
//!
//! A *rib* is one level of the stack: the generic parameters of an item, the
//! parameters of a function, the bindings of a block, the bindings of one
//! `match` arm. Lookup walks the stack from the innermost rib outwards and
//! stops at the first hit, which is what makes shadowing work.
//!
//! Two rules give this module all of its behaviour.
//!
//! - **A name is visible from the point it is defined onward.** Nothing here
//!   enforces that; it falls out of when the caller calls [`Scopes::define`].
//!   Resolution lowers a `let`'s initializer *before* defining the binding, so
//!   `let x = x` reaches the outer `x` and `let x = 1` is invisible above its
//!   own line. Items are the opposite case, and they are not ribs: a module's
//!   names are collected in a pass of their own so forward references work.
//! - **Whether a redefinition is shadowing or an error is a property of the
//!   rib.** A second `let x` in a block shadows the first, which is legal and
//!   common. A second parameter named `x`, or a second generic parameter `T`,
//!   is a duplicate definition. [`RibKind::allows_shadowing`] is the whole
//!   distinction, and [`Scopes::define`] reports the previous definition so
//!   the diagnostic can point at both.
//!
//! Module-level names are deliberately *not* here. They are unordered, they
//! carry imports and enum variants alongside plain items, and they are looked
//! up after every rib has missed; `resolve.rs` owns that table.

use crate::hir::DefId;

/// Which kind of construct introduced a rib.
///
/// Kept even where the resolver does not branch on it: a scope bug is much
/// easier to read in a dump that says which rib a name landed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RibKind {
    /// Generic parameters of a function, type, interface or implementation.
    Generics,
    /// Function parameters, including the `self` receiver.
    Params,
    /// A block: `let` bindings.
    Block,
    /// The bindings of one pattern — a `match` arm or a `for` loop.
    Pattern,
    /// The subject of a closure: the `doc` of `doc giving doc.title`, or the
    /// `each` that the implicit form leaves unwritten (§4.6). It is its own
    /// kind rather than a [`RibKind::Params`] because a closure's subject is
    /// introduced by an expression, and a dump that says so is how a scope bug
    /// in a nested closure is read.
    Closure,
}

impl RibKind {
    /// Whether defining a name twice in this rib shadows or duplicates.
    ///
    /// Only a block shadows. `(x, x)` in one pattern binds the same name
    /// twice with no way to reach the first, so it is an error, and so is a
    /// repeated parameter or generic parameter. A closure rib holds exactly
    /// one name, so the question never arises there.
    pub fn allows_shadowing(self) -> bool {
        matches!(self, RibKind::Block)
    }
}

#[derive(Debug, Clone)]
struct Rib {
    kind: RibKind,
    /// A `Vec` rather than a map: ribs hold a handful of names, order is what
    /// makes shadowing resolvable, and iteration has to be deterministic.
    bindings: Vec<(String, DefId)>,
}

/// The stack of ribs in scope at one point of the walk.
#[derive(Debug, Clone, Default)]
pub struct Scopes {
    ribs: Vec<Rib>,
}

impl Scopes {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, kind: RibKind) {
        self.ribs.push(Rib { kind, bindings: Vec::new() });
    }

    /// # Panics
    /// If there is no rib to pop, which is a bug in the walk, not in the input.
    pub fn pop(&mut self) {
        self.ribs.pop().expect("popped a rib that was never pushed");
    }

    /// How many ribs are open. For assertions that the walk is balanced.
    pub fn depth(&self) -> usize {
        self.ribs.len()
    }

    /// Introduces a name in the innermost rib.
    ///
    /// Returns `Err` with the definition already holding the name when the rib
    /// does not allow shadowing, and defines nothing in that case: the first
    /// declaration keeps the name, so a duplicate parameter does not silently
    /// take over the uses that follow it.
    ///
    /// # Panics
    /// If no rib is open.
    pub fn define(&mut self, name: &str, def: DefId) -> Result<(), DefId> {
        let rib = self.ribs.last_mut().expect("defined a name with no rib open");
        if !rib.kind.allows_shadowing() {
            if let Some((_, previous)) = rib.bindings.iter().find(|(n, _)| n == name) {
                return Err(*previous);
            }
        }
        rib.bindings.push((name.to_string(), def));
        Ok(())
    }

    /// The innermost definition of `name`, or `None` if no rib has it.
    pub fn lookup(&self, name: &str) -> Option<DefId> {
        for rib in self.ribs.iter().rev() {
            if let Some((_, def)) = rib.bindings.iter().rev().find(|(n, _)| n == name) {
                return Some(*def);
            }
        }
        None
    }

    /// The kind of rib `name` was found in. For diagnostics that want to say
    /// *what* shadowed something.
    pub fn rib_of(&self, name: &str) -> Option<RibKind> {
        for rib in self.ribs.iter().rev() {
            if rib.bindings.iter().any(|(n, _)| n == name) {
                return Some(rib.kind);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::{DefKind, DefTable};
    use science_diagnostics::{FileId, Span};

    /// Ribs only ever hold ids, so the table just has to hand out distinct
    /// ones; the spans are irrelevant here.
    fn table(count: usize) -> (DefTable, Vec<DefId>) {
        let mut defs = DefTable::new();
        let ids = (0..count)
            .map(|i| {
                defs.alloc(DefKind::Local, format!("d{i}"), Span::at(FileId(0), i as u32), None)
            })
            .collect();
        (defs, ids)
    }

    #[test]
    fn a_name_is_not_visible_before_it_is_defined() {
        let (_defs, ids) = table(1);
        let mut scopes = Scopes::new();
        scopes.push(RibKind::Block);

        assert_eq!(scopes.lookup("x"), None, "`x` is not in scope before its `let`");
        scopes.define("x", ids[0]).unwrap();
        assert_eq!(scopes.lookup("x"), Some(ids[0]));
    }

    #[test]
    fn an_inner_block_shadows_an_outer_one_and_the_outer_comes_back() {
        let (_defs, ids) = table(2);
        let mut scopes = Scopes::new();

        scopes.push(RibKind::Block);
        scopes.define("x", ids[0]).unwrap();

        scopes.push(RibKind::Block);
        scopes.define("x", ids[1]).unwrap();
        assert_eq!(scopes.lookup("x"), Some(ids[1]));

        scopes.pop();
        assert_eq!(scopes.lookup("x"), Some(ids[0]));
    }

    #[test]
    fn a_second_let_in_the_same_block_shadows_rather_than_duplicating() {
        let (_defs, ids) = table(2);
        let mut scopes = Scopes::new();
        scopes.push(RibKind::Block);

        scopes.define("x", ids[0]).unwrap();
        assert!(scopes.define("x", ids[1]).is_ok());
        assert_eq!(scopes.lookup("x"), Some(ids[1]));
    }

    #[test]
    fn a_repeated_parameter_is_a_duplicate_and_the_first_one_keeps_the_name() {
        let (_defs, ids) = table(2);
        let mut scopes = Scopes::new();
        scopes.push(RibKind::Params);

        scopes.define("a", ids[0]).unwrap();
        assert_eq!(scopes.define("a", ids[1]), Err(ids[0]));
        assert_eq!(scopes.lookup("a"), Some(ids[0]));
    }

    #[test]
    fn a_repeated_generic_parameter_is_a_duplicate() {
        let (_defs, ids) = table(2);
        let mut scopes = Scopes::new();
        scopes.push(RibKind::Generics);

        scopes.define("T", ids[0]).unwrap();
        assert_eq!(scopes.define("T", ids[1]), Err(ids[0]));
    }

    #[test]
    fn a_name_bound_twice_in_one_pattern_is_a_duplicate() {
        let (_defs, ids) = table(2);
        let mut scopes = Scopes::new();
        scopes.push(RibKind::Pattern);

        scopes.define("x", ids[0]).unwrap();
        assert_eq!(scopes.define("x", ids[1]), Err(ids[0]));
    }

    #[test]
    fn a_parameter_is_visible_inside_the_body_and_a_local_may_shadow_it() {
        let (_defs, ids) = table(2);
        let mut scopes = Scopes::new();

        scopes.push(RibKind::Params);
        scopes.define("a", ids[0]).unwrap();

        scopes.push(RibKind::Block);
        assert_eq!(scopes.lookup("a"), Some(ids[0]));
        scopes.define("a", ids[1]).unwrap();
        assert_eq!(scopes.lookup("a"), Some(ids[1]));
        assert_eq!(scopes.rib_of("a"), Some(RibKind::Block));
    }

    #[test]
    fn a_generic_parameter_is_out_of_scope_once_its_item_closes() {
        let (_defs, ids) = table(1);
        let mut scopes = Scopes::new();

        scopes.push(RibKind::Generics);
        scopes.define("T", ids[0]).unwrap();
        scopes.push(RibKind::Params);
        assert_eq!(scopes.lookup("T"), Some(ids[0]));

        scopes.pop();
        scopes.pop();
        assert_eq!(scopes.depth(), 0);
        assert_eq!(scopes.lookup("T"), None);
    }

    #[test]
    fn lookup_misses_when_nothing_is_open() {
        let scopes = Scopes::new();
        assert_eq!(scopes.lookup("anything"), None);
        assert_eq!(scopes.rib_of("anything"), None);
    }
}
