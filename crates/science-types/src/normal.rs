//! `NORMALISE` and `EQUAL` — §10.1 item 3.
//!
//! # 1. The normal form
//!
//! > **Decision 3.1.** A normalised const expression is `k + Σᵢ cᵢ · aᵢ` where
//! > `k ∈ ℤ`, every `cᵢ ∈ ℤ \ {0}`, and the atoms `aᵢ` are **strictly
//! > increasing** in a total order that is stable across compilations.
//!
//! Three invariants, and every one of them is load-bearing:
//!
//! - **No zero coefficient.** `N - N` is the constant `0` and not `0·N`, or
//!   `N - N` and `0` would be different types.
//! - **Strictly increasing atoms.** No duplicates, so `MERGE` is a linear merge
//!   and `EQUAL` is a pairwise walk. Duplicates would make equality a
//!   multiset comparison, which is sorting at every call site.
//! - **One total order, applied to both sides.** Which order does not affect
//!   *what* `EQUAL` answers — both sides are sorted by the same function — but
//!   it fixes what a diagnostic prints and what the monomorphisation key
//!   serialises to, and those must not move between runs.
//!
//! # 2. The atom order, and the one place this crate disagrees with its own spec
//!
//! `const-expression-arithmetic.md` §3.1 says the order is *"the canonical path
//! of their declaring item and then by their index in its `of` list"*, and adds
//! *"deliberately **not** `DefId`, which is an allocation order and would make
//! a diagnostic's rendering depend on the order files were read."*
//!
//! `type-checking-and-mir.md` §8 Decision 17 overrides it: **the atom order is
//! by `DefId`, and `DefId`s are assigned in source order by the resolver.**
//! This crate implements Decision 17. The argument for it, stated so it can be
//! attacked:
//!
//! - `DefTable::alloc` hands out ids sequentially as the resolver walks, and
//!   the resolver walks declarations in source order within a module and
//!   modules in the order they were handed in. So `DefId` order *is* source
//!   order, not an arbitrary allocation order, and §3.1's objection is to a
//!   property `DefId` does not have here.
//! - It is one integer comparison. §3.1's order requires building a canonical
//!   path per atom — a string, at every comparison, inside the inner loop of
//!   the procedure four other things call.
//! - It gives §8.3 its answer for free. A `with`-bound parameter is introduced
//!   by a statement rather than by a declaration's `of` list, and §8.3 asks
//!   that such parameters sort *after* declaration-bound ones by the source
//!   position of their binder. Under `DefId` that is automatic: the resolver
//!   reaches a statement after it has reached the signature that encloses it.
//!
//! **What it costs, and §3.1 is right that it costs something.** The order
//! depends on declaration order, so moving a `type` above another one in a file
//! renumbers the const parameters below it and reorders the terms a diagnostic
//! *prints*. It never changes an answer: `EQUAL` sorts both sides with the same
//! function, and two forms that were equal stay equal. And it is stable across
//! runs and across incremental rebuilds for as long as declaration order does
//! not change, which is the property §3.1 actually wanted. `tests/atom_order.rs`
//! holds all of that to it, including the part that costs.
//!
//! # 3. Provenance
//!
//! §9.3 places exactly one requirement on this representation: **terms carry
//! provenance spans, carried through `MERGE` and `SCALE` unchanged.** It is
//! what lets a unit diagnostic print
//!
//! ```text
//!     density              length  -3     (kg/m³)
//!     speed                length   1     (m/s)
//!     speed                length   1     (m/s)
//!                          ------------
//!     density * speed * speed      -1
//! ```
//!
//! — one row per contributing operand. That rendering needs *every*
//! contributor, so [`Term::provenance`] is a list and not a single span: when
//! `1·L` and `1·L` merge into `2·L`, a single-span field would have to throw
//! one of the two rows away, and the two rows are the message.
//!
//! **Provenance is not part of equality.** [`Term`] implements `PartialEq` and
//! `Hash` by hand, over the coefficient and the atom only. Two expressions that
//! denote the same function are the same type and the same monomorphisation
//! key however they were spelled and wherever they were written, and a derived
//! `PartialEq` here would make the same program hash differently depending on
//! which file the dimension was written in. That is the bug §10.1 item 4 exists
//! to prevent, arriving through the back door.
//!
//! # 4. Totality, and the one thing that is not total
//!
//! §3.2 calls `NORMALISE` total, structural, one pass, no fixpoint, and it is:
//! no production grows an expression, so there is no possibility of
//! non-termination, and the recursion depth is the expression's syntactic
//! depth. "Total" there means every well-formed expression *has* a normal
//! form, not that the procedure never reports — §3.2's own `DIVIDE` reports
//! `SC0260` on a zero divisor.
//!
//! # 5. Overflow, which §9 does not name
//!
//! `SCALE(f, d)` multiplies every coefficient by `d`, and two `i128` literals
//! can leave `i128`. §9's table does not allocate a code for it. The three
//! available answers are wrapping, saturating, and refusing, and the first two
//! are the same answer: the checker states an equality that is false, which is
//! precisely the failure §4.1 refuses rational coefficients over — *"a
//! type-level value that disagrees with the run-time value it describes is not
//! a conservative approximation, it is a lie."*
//!
//! So [`normalise`] refuses, with `SC0260`, on the reading that *"a const
//! expression the language does not admit"* covers a value its representation
//! does not admit. It follows `ast::ConstExpr::as_i128`, which already chose
//! `None` over *"a wrong number somewhere downstream"* for the same reason one
//! phase earlier.

use std::fmt::Write as _;
use std::hash::{Hash, Hasher};

use science_diagnostics::Span;
use science_resolve::hir::{DefId, DefTable};

use crate::const_expr::{ConstExpr, ConstExprKind};

/// An irreducible symbol of the normal form.
///
/// > §3.1: An atom is either a **const parameter** or a **quotient**
/// > `⌊e / d⌋` where `e` is itself normalised and `d ≥ 2` is a literal.
///
/// **One variant today, and it is an enum anyway.** F0 is the quotient-free
/// fragment (§10.1 item 3), so `Quotient` does not exist yet; this is an enum
/// rather than a newtype over [`DefId`] for the same reason
/// `hir::ConstParamKind` is an enum with a `Shape` nothing implements — so the
/// second case arrives as an added arm rather than as a replaced
/// representation.
///
/// The derived `Ord` is §3.1's order and is the reason the variant order
/// below is not arbitrary: *"parameters first, … quotients after"*. Declaring
/// `Quotient` beneath `Param` is what makes that sentence true, and it is true
/// before the variant exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Atom {
    /// A const generic parameter, or a `with`-bound one (§8.3).
    ///
    /// Ordered by [`DefId`], which is source order — see this module's §2 for
    /// why that overrides §3.1's own sentence on the subject.
    Param(DefId),
}

impl Atom {
    /// The atom as a diagnostic spells it.
    pub fn render(&self, defs: &DefTable) -> String {
        match self {
            Atom::Param(def) => defs.get(*def).name.clone(),
        }
    }

    /// Where the atom was bound, for §9.2's legend.
    pub fn definition_span(&self, defs: &DefTable) -> Span {
        match self {
            Atom::Param(def) => defs.get(*def).span,
        }
    }

    /// Whether the atom's definition site is a readable one.
    ///
    /// A builtin definition carries `hir::BUILTIN_SPAN`, which points at no
    /// text; `hir` is explicit that a diagnostic must never use one as a
    /// label. §9.2's legend is all labels, so it asks first.
    pub fn is_builtin(&self, defs: &DefTable) -> bool {
        match self {
            Atom::Param(def) => defs.get(*def).is_builtin(),
        }
    }
}

/// One `cᵢ · aᵢ` of the normal form.
///
/// `coefficient` is never zero: [`NormalForm`] drops a term whose coefficient
/// reaches zero rather than keeping it, because `N - N` must be the constant
/// `0` and not a form with a term in it.
#[derive(Debug, Clone)]
pub struct Term {
    coefficient: i128,
    atom: Atom,
    provenance: Vec<Span>,
}

impl Term {
    pub fn coefficient(&self) -> i128 {
        self.coefficient
    }

    pub fn atom(&self) -> Atom {
        self.atom
    }

    /// Every operand that contributed to this term, in the order they were
    /// merged. §9.3's derivation block is one row per entry.
    pub fn provenance(&self) -> &[Span] {
        &self.provenance
    }
}

/// Equality over the coefficient and the atom, and **not** over the
/// provenance.
///
/// This is not an optimisation and it is not laziness. §3.3 makes equality of
/// const expressions equality of the functions they denote, and where a
/// dimension was written is not part of what it denotes. A derived
/// `PartialEq` would make `L1 + L2` written in two files two types.
impl PartialEq for Term {
    fn eq(&self, other: &Self) -> bool {
        self.coefficient == other.coefficient && self.atom == other.atom
    }
}

impl Eq for Term {}

/// Hashes what [`PartialEq`] compares, and nothing else.
///
/// `Hash` and `Eq` must agree or a hash map keyed on a monomorphisation key
/// silently grows a second entry for a type it already has — which is §10.1
/// item 4's bug, one layer down.
impl Hash for Term {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.coefficient.hash(state);
        self.atom.hash(state);
    }
}

/// A const expression in normal form: `k + Σ cᵢ·aᵢ`.
///
/// `PartialEq`, `Eq` and `Hash` are derived, and they are correct because
/// [`Term`]'s are written by hand: the constant and the term list are compared
/// structurally, the term list is in strictly increasing atom order, and no
/// term carries a zero coefficient — so structural equality *is* §3.3's
/// `EQUAL`, and §3.4's injectivity argument makes it sound and complete on the
/// quotient-free fragment.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NormalForm {
    constant: i128,
    /// Strictly increasing in [`Atom`]'s order. Never contains a zero
    /// coefficient.
    terms: Vec<Term>,
}

impl NormalForm {
    /// The form `k`, with no terms.
    pub fn literal(constant: i128) -> NormalForm {
        NormalForm { constant, terms: Vec::new() }
    }

    /// The form `1·a`, contributed by `span`.
    pub fn atom(atom: Atom, span: Span) -> NormalForm {
        NormalForm {
            constant: 0,
            terms: vec![Term { coefficient: 1, atom, provenance: vec![span] }],
        }
    }

    /// `k`.
    pub fn constant(&self) -> i128 {
        self.constant
    }

    /// The terms, in strictly increasing atom order.
    pub fn terms(&self) -> &[Term] {
        &self.terms
    }

    /// Whether this form denotes a fixed integer.
    pub fn is_constant(&self) -> bool {
        self.terms.is_empty()
    }

    /// The fixed integer this form denotes, if it denotes one.
    pub fn as_constant(&self) -> Option<i128> {
        self.terms.is_empty().then_some(self.constant)
    }

    /// The coefficient of `atom`, which is zero when the atom is absent.
    ///
    /// Zero and absent are the same thing by the no-zero-coefficient
    /// invariant, so this is total and there is no `Option`.
    pub fn coefficient_of(&self, atom: Atom) -> i128 {
        match self.terms.binary_search_by(|term| term.atom.cmp(&atom)) {
            Ok(at) => self.terms[at].coefficient,
            Err(_) => 0,
        }
    }

    /// The atoms this form mentions, in order.
    pub fn atoms(&self) -> impl Iterator<Item = Atom> + '_ {
        self.terms.iter().map(|term| term.atom)
    }

    /// §3.2's `NEGATE`: negate `k` and every `cᵢ`.
    pub fn negate(&self) -> Result<NormalForm, ConstEvalError> {
        self.scale(-1)
    }

    /// §3.2's `SCALE`.
    ///
    /// > `SCALE(f,0)`: the constant 0. `SCALE(f,d)`: multiply `k` and every
    /// > `cᵢ` by `d`.
    ///
    /// Scaling by zero drops every term *and its provenance*, which is right:
    /// `N * 0` is `0`, and `0` came from nowhere in particular.
    pub fn scale(&self, factor: i128) -> Result<NormalForm, ConstEvalError> {
        if factor == 0 {
            return Ok(NormalForm::literal(0));
        }
        let constant = checked(self.constant.checked_mul(factor))?;
        let mut terms = Vec::with_capacity(self.terms.len());
        for term in &self.terms {
            terms.push(Term {
                coefficient: checked(term.coefficient.checked_mul(factor))?,
                atom: term.atom,
                // "carried through MERGE and SCALE unchanged" — §9.3.
                provenance: term.provenance.clone(),
            });
        }
        Ok(NormalForm { constant, terms })
    }

    /// §3.2's `MERGE`: add the constants, add the coefficients of equal atoms,
    /// drop any term whose coefficient became zero, keep the order.
    ///
    /// A linear merge over two sorted lists, so [`normalise`] is linear in the
    /// size of the expression rather than sorting at every node.
    pub fn merge(&self, other: &NormalForm) -> Result<NormalForm, ConstEvalError> {
        let constant = checked(self.constant.checked_add(other.constant))?;
        let mut terms = Vec::with_capacity(self.terms.len() + other.terms.len());
        let (mut left, mut right) = (self.terms.iter().peekable(), other.terms.iter().peekable());

        loop {
            let order = match (left.peek(), right.peek()) {
                (None, None) => break,
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (Some(a), Some(b)) => a.atom.cmp(&b.atom),
            };
            match order {
                std::cmp::Ordering::Less => terms.push(left.next().expect("peeked").clone()),
                std::cmp::Ordering::Greater => terms.push(right.next().expect("peeked").clone()),
                std::cmp::Ordering::Equal => {
                    let a = left.next().expect("peeked");
                    let b = right.next().expect("peeked");
                    let coefficient = checked(a.coefficient.checked_add(b.coefficient))?;
                    if coefficient == 0 {
                        // The invariant: a cancelled term is not a term. Its
                        // provenance goes with it, because there is nothing
                        // left for a derivation row to be about.
                        continue;
                    }
                    let mut provenance = a.provenance.clone();
                    provenance.extend_from_slice(&b.provenance);
                    terms.push(Term { coefficient, atom: a.atom, provenance });
                }
            }
        }

        Ok(NormalForm { constant, terms })
    }

    /// The form as §9.2's `normalised` column prints it: `-3 + 3*ORDER`.
    ///
    /// The constant comes first because §3.1 writes the form `k + Σ cᵢ·aᵢ`,
    /// and it is printed even when it is zero *only* if there is nothing else
    /// to print — `0 + n` reads as though the zero meant something.
    pub fn render(&self, defs: &DefTable) -> String {
        let mut out = String::new();
        if self.constant != 0 || self.terms.is_empty() {
            let _ = write!(out, "{}", self.constant);
        }
        for term in &self.terms {
            let name = term.atom.render(defs);
            let magnitude = term.coefficient.unsigned_abs();
            let body = if magnitude == 1 { name } else { format!("{magnitude}*{name}") };
            if out.is_empty() {
                if term.coefficient < 0 {
                    out.push('-');
                }
                out.push_str(&body);
            } else {
                out.push_str(if term.coefficient < 0 { " - " } else { " + " });
                out.push_str(&body);
            }
        }
        out
    }
}

/// A const expression whose value the representation cannot hold.
///
/// One variant, and an enum, for the reason [`Atom`] is one: §4's `DIVIDE` adds
/// *division by zero* to this list when quotient atoms arrive, and §9's table
/// already allocates it to `SC0260`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstEvalError {
    /// A constant or a coefficient left the range of `i128`.
    ///
    /// The span is attached by [`normalise`], which knows which node was being
    /// evaluated; the arithmetic itself does not.
    Overflow { span: Option<Span> },
}

impl ConstEvalError {
    /// The same error, blamed on `span`.
    ///
    /// Only the outermost frame that has a span sets one, so the blame lands
    /// on the node whose arithmetic overflowed rather than on the whole
    /// expression.
    fn at(self, span: Span) -> ConstEvalError {
        match self {
            ConstEvalError::Overflow { span: Some(_) } => self,
            ConstEvalError::Overflow { span: None } => {
                ConstEvalError::Overflow { span: Some(span) }
            }
        }
    }

    pub fn span(self) -> Option<Span> {
        match self {
            ConstEvalError::Overflow { span } => span,
        }
    }
}

fn checked(value: Option<i128>) -> Result<i128, ConstEvalError> {
    value.ok_or(ConstEvalError::Overflow { span: None })
}

/// §3.2's `NORMALISE`. Structural, one pass, no fixpoint.
///
/// ```text
/// literal n          ->  (k = n, terms = [])
/// param p            ->  (k = 0, terms = [(1, p)])
/// -e₁                ->  NEGATE(NORMALISE(e₁))
/// e₁ + e₂            ->  MERGE(NORMALISE(e₁), NORMALISE(e₂))
/// e₁ - e₂            ->  MERGE(NORMALISE(e₁), NEGATE(NORMALISE(e₂)))
/// e₁ * d   (d lit)   ->  SCALE(NORMALISE(e₁), d)
/// ```
///
/// There is no arm for a non-linear product because [`ConstExprKind`] has no
/// node for one — §2.1's whole point, and the reason this function returns a
/// form rather than an `Option<Form>` on everything but arithmetic range.
pub fn normalise(expr: &ConstExpr) -> Result<NormalForm, ConstEvalError> {
    let form = match &expr.kind {
        ConstExprKind::Lit(value) => Ok(NormalForm::literal(*value)),
        ConstExprKind::Param(def) => Ok(NormalForm::atom(Atom::Param(*def), expr.span)),
        ConstExprKind::Neg(operand) => normalise(operand)?.negate(),
        ConstExprKind::Add(left, right) => normalise(left)?.merge(&normalise(right)?),
        ConstExprKind::Sub(left, right) => normalise(left)?.merge(&normalise(right)?.negate()?),
        ConstExprKind::Scale { operand, factor, .. } => normalise(operand)?.scale(*factor),
    }
    .map_err(|error| error.at(expr.span))?;
    Ok(form)
}

/// §3.3's `EQUAL`: structural equality on the normal form.
///
/// > **Decision 3.3.** `EQUAL(e₁, e₂)` is `NORMALISE(e₁) = NORMALISE(e₂)`,
/// > structurally: equal constants, equal term counts, and pairwise equal
/// > `(cᵢ, aᵢ)`.
///
/// It is `==`, and it is a named function anyway, because §3.5 has four
/// callers and they must be able to point at one line and say *"that one"*.
/// On the quotient-free fragment it is sound **and complete** (§3.4): two
/// normal forms are structurally equal exactly when they denote the same
/// function. That stops being true the day a quotient atom exists, and §4.3
/// bounds where.
pub fn equal(left: &NormalForm, right: &NormalForm) -> bool {
    left == right
}
