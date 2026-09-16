//! The normal form is the monomorphisation key — §10.1 item 4.
//!
//! > **§3.5 item 4.** Two instantiations that are `EQUAL` must produce one
//! > symbol, or `a * b` and `b * a` link to two copies of the same function.
//! > The key is the normal form, serialised in atom order.
//!
//! # Why this is a type and not a `Vec<NormalForm>`
//!
//! Because the bug it prevents is a bug of *forgetting*. A monomorphiser that
//! keys on `Vec<ast::ConstExpr>` compiles, passes every test that does not
//! write the same extent two ways, and emits two symbols for one function —
//! §10.1's *"a codegen bug discovered late and fixed by rewriting the key"*.
//! Putting the key in its own type with its own `Hash` and `Eq` means the
//! wrong thing does not typecheck, and means the property has one place to be
//! tested.
//!
//! # What would make two `EQUAL` expressions hash differently
//!
//! Nothing, and that is a claim with four named supports rather than a hope:
//!
//! 1. **`Hash` agrees with `Eq`, which is [`NormalForm`]'s.** This type derives
//!    both and adds no field of its own. [`crate::Term`] implements them by
//!    hand over the coefficient and the atom only, so provenance — the one
//!    field that differs between two spellings of one expression — is outside
//!    both.
//! 2. **No zero coefficient survives.** `MERGE` drops a cancelled term, so
//!    `N - N + 3` and `3` are one key rather than `0·N + 3` and `3`.
//! 3. **Atoms are strictly increasing in one total order.** `a + b` and
//!    `b + a` produce the same list, not two orderings of one multiset.
//! 4. **There is no `-0` in `i128`.** A sign-magnitude coefficient would have
//!    two representations of zero and this claim would be false; the
//!    two's-complement one has one.
//!
//! The support that can actually break is (3), and it breaks by the atom order
//! ceasing to be compilation-stable. That is why `tests/atom_order.rs` exists
//! and why it resolves the same source twice rather than asserting a
//! comparison function against itself.
//!
//! # Serialisation is not mangling
//!
//! [`MonoKey`]'s `Display` is the stable textual form §3.5 item 4 asks for:
//! injective, deterministic, and readable in a dump. It is **not** a symbol
//! name — it contains `+`, `*` and `#`, which no object format accepts — and
//! escaping it into one belongs to codegen, which owns the mangling scheme.
//! Putting the escape here would put half of a mangling rule in a crate that
//! does not know the other half.

use std::fmt;

use crate::const_expr::ConstExpr;
use crate::normal::{normalise, ConstEvalError, NormalForm};

/// The const-argument half of one instantiation's identity.
///
/// The type-argument half is the type checker's and is not here: this crate
/// has no notion of a type. A monomorphiser's key is a pair of the two, and
/// this is the side that needs a normal form to be correct.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct MonoKey {
    args: Vec<NormalForm>,
}

impl MonoKey {
    /// The key for a list of already-normalised const arguments, in the order
    /// the declaration's `of` list gives them.
    ///
    /// Argument *position* is part of the key: `Matrix of (T, 2, 3)` and
    /// `Matrix of (T, 3, 2)` are two types, so the list is a list and not a
    /// set.
    pub fn new(args: impl IntoIterator<Item = NormalForm>) -> MonoKey {
        MonoKey { args: args.into_iter().collect() }
    }

    /// The key for a list of const arguments as written, normalising each.
    pub fn of(args: &[ConstExpr]) -> Result<MonoKey, ConstEvalError> {
        args.iter().map(normalise).collect::<Result<Vec<_>, _>>().map(MonoKey::new)
    }

    pub fn args(&self) -> &[NormalForm] {
        &self.args
    }

    pub fn len(&self) -> usize {
        self.args.len()
    }

    pub fn is_empty(&self) -> bool {
        self.args.is_empty()
    }
}

/// The serialised key, in atom order.
///
/// One argument renders as its constant followed by its terms:
/// `1+1*#7-2*#9`. Arguments are separated by `;`, and an empty key renders as
/// the empty string. Atoms are written as `#` and the [`DefId`] index rather
/// than as a name, because two parameters in two scopes may share a name and
/// the key must not confuse them — the same reason
/// [`crate::ConstExprKind::Param`] holds a `DefId`.
///
/// [`DefId`]: science_resolve::hir::DefId
impl fmt::Display for MonoKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, arg) in self.args.iter().enumerate() {
            if index > 0 {
                f.write_str(";")?;
            }
            write!(f, "{}", arg.constant())?;
            for term in arg.terms() {
                let sign = if term.coefficient() < 0 { '-' } else { '+' };
                let magnitude = term.coefficient().unsigned_abs();
                match term.atom() {
                    crate::Atom::Param(def) => {
                        write!(f, "{sign}{magnitude}*#{}", def.index())?;
                    }
                }
            }
        }
        Ok(())
    }
}
