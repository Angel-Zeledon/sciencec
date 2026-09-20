//! The type representation — `type-checking-and-mir.md` §2 and Decision 24.
//!
//! This is the thing inference and checking both stand on, and nothing above
//! it. There is no inference variable, no coercion, no subtyping and no method
//! lookup here; §8 below says where each of them stops.
//!
//! # 1. A type is an index, and [`Types`] is the only thing that can make one
//!
//! **Decision. [`Ty`] is a `u32` index into a flat table, interned on
//! construction, and structural equality on types is `==` on the index.**
//!
//! The reason is that equality is the checker's inner loop, and it is the
//! inner loop twice over:
//!
//! - Decision 1 makes checking bidirectional with no unifier across function
//!   boundaries, so *every* call site compares a declared parameter type
//!   against a synthesised argument type. There is no other algorithm; the
//!   comparison is the algorithm.
//! - §8 item 4 makes type equality the monomorphisation key, and
//!   [`crate::MonoKey`]'s own documentation says what a key that compares the
//!   wrong thing costs: two symbols for one function, *"not a type error, not
//!   a link error on most platforms, and a performance mystery long after the
//!   cause"*.
//!
//! Comparing trees is `O(size)` at every one of those sites and allocates a
//! stack frame per node. Comparing indices is one instruction, and — this is
//! the part that is not about speed — it is *one* instruction that cannot get
//! the answer wrong by forgetting a field, which is the failure mode a
//! hand-written `PartialEq` over ten variants has.
//!
//! **What it costs, in the order it will bite:**
//!
//! 1. **A [`Ty`] means nothing without the [`Types`] that made it.** Two
//!    tables number their types differently, so a `Ty` from one compared
//!    against a `Ty` from another is a silently wrong answer and not a panic.
//!    Nothing in the type system catches it. F0 has one table per compilation
//!    and that is the whole defence; a tagged index would cost a word and a
//!    branch on every comparison, which is the loop this decision exists to
//!    keep short.
//! 2. **Every [`TyKind`] is stored twice** — once in the table and once as the
//!    key of the map that finds it. The alternative is hashing through the
//!    index, which needs the table inside the hasher and therefore needs the
//!    table borrowed while it is being mutated, which is §2's refusal.
//! 3. **Construction is no longer free.** Building `Array[Int]` is a hash
//!    lookup, not a `Box::new`. That is the right side of the trade only
//!    because a checker builds a type once and compares it many times.
//!
//! # 2. `mutable self`, not interior mutability
//!
//! **Decision 24.** The checker is index-not-pointer throughout and union-find
//! takes `mutable self`, *"recorded now, because discovering it during the
//! port means rewriting the inference context"*. [`Types::intern`] takes
//! `&mut self` for the same reason and it is literally the same reason:
//! `region-inference.md` §9 says `RefCell` has no Science spelling, so an
//! interner that mutates through a shared borrow is an interner that cannot be
//! ported to the language it is checking.
//!
//! **What that costs.** A caller cannot hold a borrow of the table across an
//! interning call, so code that reads a kind and then builds from it clones
//! the parts it needs first. That clone is the price of the decision, and it
//! is visible at every such site rather than hidden inside a cell.
//!
//! **And one tension, stated rather than resolved.** Decision 1's second
//! dividend is that *"bodies check in parallel and in any order"*. A single
//! `mutable self` interner is shared mutable state, and shared mutable state
//! is exactly what parallel checking cannot have. F0 checks sequentially, so
//! the tension is not yet real. The fix — an immutable prefix shared between
//! threads with a per-body extension merged afterwards, or one table per body
//! keyed on a hash — is a decision the parallel checker should make against a
//! measurement, and a guess made here would be a shape it has to undo.
//!
//! # 3. A const generic argument is a [`NormalForm`], not a number
//!
//! **Decision.** [`GenericArg::Const`] holds a normal form.
//!
//! `Matrix of (T, a + 1)` and `Matrix of (T, 1 + a)` are one type, and
//! [`crate::normalise`] is the only thing that makes them one. So interning is
//! **not syntactic**: two different `hir::ConstExpr` trees reach one [`Ty`],
//! and they reach it because normalisation runs *before* the hash key is built
//! rather than after. An interner keyed on the syntax would compile, pass
//! every test that does not write one extent two ways, and emit two symbols
//! for one function — §10.1 item 4's bug, arriving through the type table
//! instead of through the monomorphiser.
//!
//! **What it costs: provenance.** `normal`'s §3 puts `Term::provenance`
//! deliberately outside equality and hashing, so the interner cannot see it —
//! which is what makes two spellings one type, and also means **the table
//! keeps the spans of whichever spelling it interned first**. Reading
//! provenance off an interned type therefore reads some other occurrence's
//! source positions, which is a diagnostic pointing into the wrong function.
//! A diagnostic blames the `hir::Type` span it was lowering, which
//! [`crate::lowering::TypeLowerer`] has in hand and this table does not have at
//! all. The alternative — stripping provenance on the way in — was rejected
//! because it destroys what the *first* occurrence legitimately needs, and
//! because the caller that has a span is the caller that should use it.
//!
//! # 4. `T?` is a node, and it cannot be built twice
//!
//! **Decision 6.** `T?` is a distinct type, not a union and not a subtype of
//! `T`. `T` coerces into `T?`; `T?` never coerces to `T`. **Neither coercion
//! is here** — a coercion is a fact about an assignment, and this file has no
//! assignments. What is here is the type.
//!
//! `T??` does not exist, so [`Types::nullable`] is **idempotent**: making a
//! nullable type nullable hands back the same type. A representation that can
//! hold a type the language does not have is a representation every match arm
//! downstream has to pretend about, and the pretending is where the arm that
//! forgot lives.
//!
//! The *diagnostic* is not here either, and that split is deliberate. This
//! table cannot tell a `T??` the author wrote from a `T?` that a later
//! substitution put where a `T?` already was, and only the first is `SC0520`.
//! The lowering knows which it has, because it is looking at two
//! `hir::TypeKind::Nullable` nodes with two spans.
//!
//! # 5. `Error` is a type, and it is why one mistake is one diagnostic
//!
//! **Decision.** [`Ty::ERROR`] is an ordinary interned type, every type that
//! mentions it anywhere is flagged at interning time, and
//! [`Types::compatible`] is:
//!
//! ```text
//! a == b  ||  references_error(a)  ||  references_error(b)
//! ```
//!
//! Three things fall out of that, and the third is the argument for §1:
//!
//! - **One bad annotation is one diagnostic.** `def f(x: Bogus)` is reported
//!   once by the resolver; every call to `f` then compares against a type that
//!   is compatible with whatever it is given, so the checker has nothing to
//!   add. This is the discipline the parser already follows with
//!   `ast::TypeKind::Error` and the resolver with `Res::Error`.
//! - **The flag is computed once, on the way in.** A type's `has_error` is the
//!   disjunction of its operands', which are already interned and already
//!   flagged, so it is `O(arity)` at construction and `O(1)` forever after. No
//!   walk, no memo table, no cycle to worry about.
//! - **There is nothing else in `compatible`.** Because interning already made
//!   structural equality into `==`, the error rule is the *entire* remaining
//!   content of the relation. That is the clearest statement of what interning
//!   bought that this file can make.
//!
//! [`Types::compatible`] is emphatically **not** assignability: it does not
//! widen `T` into `T?` (Decision 6), it does not box a concrete error type
//! into `any Error` (Decision 14), and it knows nothing about interfaces.
//! Those are three separate relations and each belongs to the phase that has
//! an expression in hand.
//!
//! # 6. `Self` arrives unsubstituted, and so do aliases
//!
//! [`TyKind::SelfType`] and [`TyKind::SelfAssoc`] carry the block that wrote
//! them and nothing more, and a `type Embedding is Array[F32]` interns as
//! its own [`TyKind::Named`] rather than as `Array[F32]`. Both are holes and
//! both are the same hole: expanding either needs the crate's *items*, needs a
//! substitution over [`Ty`], and needs a cycle check with a diagnostic of its
//! own.
//!
//! The cost is stated plainly: until then, `Embedding` and `Array[F32]` are
//! two `Ty`s denoting one type, and `compatible` says they differ. Nothing in
//! F0 relies on them agreeing, and the first thing that does is the change
//! that has to land expansion.
//!
//! # 7. What is deliberately not in [`TyKind`]
//!
//! - **No `Infer` variant.** Decision 1's cost is that *"local inference
//!   variables still exist … so there is a small unification engine inside one
//!   body"*, and Decision 24 says that engine is a flat array with `mutable
//!   self` on `find`. Adding the variant is one arm here, one arm in
//!   [`Types::compatible`] and one arm in [`Types::render`] — three arms, and
//!   they are cheap. It is left out because *what an inference variable is* —
//!   a union-find index scoped to one body and discarded when the body is done
//!   — is the inference context's first decision, and a variant added before
//!   that decision fixes its representation from outside the file that owns
//!   it.
//! - **No region and no lifetime.** `region-inference.md` owns them and its
//!   lattice is over MIR points, not over types.
//! - **No "callable" beyond [`TyKind::Closure`]'s shape.** A closure type is
//!   its parameters and its return type, structurally, exactly as
//!   `collections-and-chains.md` §1.2 spells it and as the resolver hands it
//!   over. Whether a particular function *has* that shape is a question about
//!   a definition, and it is asked where the definition is in hand.
//!
//! # 8. The seam
//!
//! Everything above is the representation. The next phase owns: inference
//! variables and the union-find over them; assignability, which is
//! `compatible` plus Decision 6's widening plus Decision 14's boxing;
//! substitution, including the const half, which rewrites atoms *inside* a
//! [`NormalForm`] and has an overflow failure mode of its own; alias
//! expansion; `Self` resolution; method lookup (Decision 11); and every
//! `SC05xx` this crate has not claimed.

use std::collections::HashMap;
use std::fmt::Write as _;

use science_resolve::hir::{DefId, DefTable};

use crate::normal::NormalForm;

/// An interned type: an index into the [`Types`] that made it.
///
/// `Copy`, and one word. Two `Ty`s from one table are equal exactly when the
/// types are structurally equal, which is what §1 buys and what makes
/// [`Types::compatible`] a single line.
///
/// A `Ty` from a *different* table is a different numbering of a different
/// program, and comparing the two is a wrong answer with no symptom. §1 states
/// that cost rather than defending it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ty(u32);

impl Ty {
    /// The type of an annotation whose mistake has already been reported.
    ///
    /// Interned first by [`Types::new`], so its index is fixed and a `const`
    /// is honest rather than a hope. §5 says what it means.
    pub const ERROR: Ty = Ty(0);

    /// `()` — the type of a function that returns nothing (§4.4).
    ///
    /// Interned second, for the same reason: unit is what every signature that
    /// omits a return type means, so it is worth not looking up.
    pub const UNIT: Ty = Ty(1);

    /// The table index. For a dump, and for a side table keyed on types.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// One argument in an `of (..)` list.
///
/// §5.3 puts type arguments and const arguments in the same list, and
/// `hir::DefKind::is_type` admits a const parameter into a type position for
/// exactly that reason — the resolver cannot tell them apart and says so in
/// its own doc comment. This is the phase that can, and this enum is where the
/// answer is recorded so that no later phase has to ask again.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GenericArg {
    /// A type argument: the `T` of `Array[T]`.
    Type(Ty),
    /// A const argument: the `4` of `Window[Int, 4]`, the `a + 1` of
    /// `Matrix[T, a + 1]`, and a bare const parameter used as one.
    ///
    /// Already normalised. §3 is why it has to be, and why the provenance it
    /// carries is the first spelling's.
    Const(NormalForm),
    /// An argument whose lowering failed, of either kind.
    ///
    /// The const analogue of [`Ty::ERROR`], and a variant of its own rather
    /// than `Type(Ty::ERROR)` because a monomorphiser matching on this enum
    /// wants *"do not key on me"* to be a case it was made to handle, not a
    /// type sitting in a slot that should hold a value.
    Error,
}

impl GenericArg {
    /// The type this argument is, when it is one.
    pub fn as_type(&self) -> Option<Ty> {
        match self {
            GenericArg::Type(ty) => Some(*ty),
            _ => None,
        }
    }

    /// The const expression this argument is, when it is one.
    pub fn as_const(&self) -> Option<&NormalForm> {
        match self {
            GenericArg::Const(form) => Some(form),
            _ => None,
        }
    }
}

/// What a [`Ty`] is.
///
/// Every operand is a [`Ty`] or a [`DefId`] — an index, never a pointer
/// (Decision 24). A recursive type is a cycle through this table, which is not
/// a cycle in anybody's ownership graph, which is §11's reason for saying the
/// checker is expressible in Science at all.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TyKind {
    /// A type the compiler does not know, because the mistake that produced it
    /// has already been reported. §5.
    Error,
    /// `()`.
    Unit,
    /// A named type applied to its arguments: a record, a choice, an alias, a
    /// primitive, a foreign union, an associated type reached by path.
    ///
    /// One variant for all of them, because a checker's question about a name
    /// is *"which definition"* and never *"which kind of definition"* — the
    /// kind is one lookup away in the [`DefTable`], and a copy of it here
    /// would be a second thing to keep in step with the resolver.
    Named { def: DefId, args: Vec<GenericArg> },
    /// A generic type parameter standing for itself: the `T` inside
    /// `def largest[T](..)`.
    ///
    /// A variant of its own rather than a [`TyKind::Named`] at a `TypeParam`
    /// definition, because substitution asks *"is this a thing I replace?"* at
    /// every node, and that answer should be the discriminant rather than a
    /// table lookup.
    Param { def: DefId },
    /// `&T` and `&mut T`.
    Borrowed { mutable: bool, inner: Ty },
    /// Two or more elements. `(T)` is `T`, and the parser builds no
    /// one-element tuple.
    Tuple(Vec<Ty>),
    /// `(A) -> B` — the closure type of `collections-and-chains.md` §1.2.
    ///
    /// Structural: it names no definition, so there is nothing here but the
    /// parameters and the return type. `params` is empty for `() -> B`. The
    /// §1.2 loss survives into this table unchanged — `(T)` collapsed in the
    /// parser with no node, so a closure over a single tuple is not
    /// representable here either.
    Closure { params: Vec<Ty>, ret: Ty },
    /// `T?` — Decision 6. Never directly inside another [`TyKind::Nullable`];
    /// see [`Types::nullable`].
    Nullable(Ty),
    /// `any Summarize` — the interface object of Decision 13.
    Object { interface: DefId, args: Vec<GenericArg> },
    /// `Self`, carrying the implementation or interface that wrote it,
    /// unsubstituted. §6.
    SelfType { owner: DefId },
    /// `Self.Item` (§5.4), carrying the associated-type definition it names,
    /// unresolved. §6.
    SelfAssoc { assoc: DefId },
}

/// The interned type table.
///
/// Flat and append-only, like `hir::DefTable` and for the reason that table
/// gives: a tree is what the phase walks, and this is what it leaves behind.
///
/// [`Types::intern`] takes `&mut self`, and there is no cell anywhere in it.
/// §2.
#[derive(Debug, Clone)]
pub struct Types {
    kinds: Vec<TyKind>,
    /// `true` when the type at that index mentions [`Ty::ERROR`] anywhere
    /// inside it. Computed when the type is interned, out of flags its
    /// operands already carry, so it is `O(arity)` once and `O(1)` after. §5.
    has_error: Vec<bool>,
    index: HashMap<TyKind, Ty>,
}

impl Default for Types {
    fn default() -> Self {
        Types::new()
    }
}

impl Types {
    /// A table holding [`Ty::ERROR`] and [`Ty::UNIT`], in that order.
    ///
    /// The order is asserted rather than assumed. Two associated constants
    /// that claim an index are two claims a later edit can falsify in silence,
    /// and this is the one place that can notice.
    pub fn new() -> Types {
        let mut types = Types { kinds: Vec::new(), has_error: Vec::new(), index: HashMap::new() };
        let error = types.intern(TyKind::Error);
        let unit = types.intern(TyKind::Unit);
        assert_eq!(error, Ty::ERROR, "the error type is interned first");
        assert_eq!(unit, Ty::UNIT, "the unit type is interned second");
        types
    }

    /// The type for a kind, interning it if the table has not seen it.
    ///
    /// The only way a [`Ty`] comes into existence. Every constructor below
    /// funnels through it, which is what makes *"equal types have equal
    /// indices"* a property of the table rather than a rule callers follow.
    pub fn intern(&mut self, kind: TyKind) -> Ty {
        if let Some(&existing) = self.index.get(&kind) {
            return existing;
        }
        let has_error = self.kind_has_error(&kind);
        let ty = Ty(self.kinds.len() as u32);
        self.index.insert(kind.clone(), ty);
        self.kinds.push(kind);
        self.has_error.push(has_error);
        ty
    }

    /// What a type is.
    pub fn kind(&self, ty: Ty) -> &TyKind {
        &self.kinds[ty.index()]
    }

    /// How many distinct types this compilation has built.
    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    // --- constructors ----------------------------------------------------

    /// A named type applied to its arguments.
    pub fn named(&mut self, def: DefId, args: Vec<GenericArg>) -> Ty {
        self.intern(TyKind::Named { def, args })
    }

    /// A generic type parameter, standing for itself.
    pub fn param(&mut self, def: DefId) -> Ty {
        self.intern(TyKind::Param { def })
    }

    /// `&T`, or `&mut T`.
    pub fn borrowed(&mut self, mutable: bool, inner: Ty) -> Ty {
        self.intern(TyKind::Borrowed { mutable, inner })
    }

    /// A tuple of two or more elements.
    ///
    /// The arity invariant is the parser's — `(T)` is `T` and produces no node
    /// — and it is not re-checked here. A one-element tuple arriving would be
    /// a bug upstream, and interning it is how it stays visible instead of
    /// being quietly turned into something else.
    pub fn tuple(&mut self, elements: Vec<Ty>) -> Ty {
        self.intern(TyKind::Tuple(elements))
    }

    /// `(A) -> B`.
    pub fn closure(&mut self, params: Vec<Ty>, ret: Ty) -> Ty {
        self.intern(TyKind::Closure { params, ret })
    }

    /// `T?`, and **idempotent**: `T??` is `T?`.
    ///
    /// Decision 6 says `T??` does not exist, so this is the function that makes
    /// it not exist. It is not the function that reports it; see §4.
    pub fn nullable(&mut self, inner: Ty) -> Ty {
        if matches!(self.kind(inner), TyKind::Nullable(_)) {
            return inner;
        }
        self.intern(TyKind::Nullable(inner))
    }

    /// `any I` — an interface object.
    pub fn object(&mut self, interface: DefId, args: Vec<GenericArg>) -> Ty {
        self.intern(TyKind::Object { interface, args })
    }

    /// `Self`, inside the implementation or interface `owner`.
    pub fn self_type(&mut self, owner: DefId) -> Ty {
        self.intern(TyKind::SelfType { owner })
    }

    /// `Self.Item`, naming the associated type `assoc`.
    pub fn self_assoc(&mut self, assoc: DefId) -> Ty {
        self.intern(TyKind::SelfAssoc { assoc })
    }

    // --- the two questions the representation answers --------------------

    /// Whether this type mentions [`Ty::ERROR`] anywhere inside it.
    ///
    /// `O(1)`: the flag was computed when the type was interned, out of flags
    /// its operands already carried. A phase that wants to stay silent about a
    /// type it cannot trust asks this.
    pub fn references_error(&self, ty: Ty) -> bool {
        self.has_error[ty.index()]
    }

    /// Structural equality, with an erroneous type compatible with everything.
    ///
    /// **This is not assignability.** It does not widen `T` into `T?`
    /// (Decision 6), it does not box a concrete error type into `any Error`
    /// (Decision 14), it does not look through an alias, and it does not know
    /// what an interface is. Every one of those is a relation over an
    /// *expression* in a *context*, and this file has neither. The checker
    /// builds assignability on top of this; it does not replace it.
    ///
    /// What it *is* is the two rules the representation itself owes:
    /// interning turned structural equality into `==`, and §5 makes an
    /// already-reported type agree with whatever it meets, so that one bad
    /// annotation stays one diagnostic.
    pub fn compatible(&self, left: Ty, right: Ty) -> bool {
        left == right || self.references_error(left) || self.references_error(right)
    }

    // --- rendering -------------------------------------------------------

    /// The type as a diagnostic spells it, in surface syntax.
    ///
    /// `Array[T]`, `Map[String, Int]`, `&mut Doc`,
    /// `(any Error)?`. The parentheses around a borrow under a `?` are not
    /// decoration: `&T?` parses as `&(T?)`, so printing a
    /// nullable borrow without them prints a type that reads back as a
    /// different one.
    ///
    /// [`Ty::ERROR`] renders `{unknown}` — braces, because no surface syntax
    /// uses them, so a reader who sees one knows the compiler is admitting
    /// rather than describing.
    pub fn render(&self, defs: &DefTable, ty: Ty) -> String {
        let mut out = String::new();
        self.render_into(defs, ty, &mut out);
        out
    }

    fn render_into(&self, defs: &DefTable, ty: Ty, out: &mut String) {
        match self.kind(ty) {
            TyKind::Error => out.push_str("{unknown}"),
            TyKind::Unit => out.push_str("()"),
            TyKind::Named { def, args } => {
                out.push_str(&name_for_reader(defs, *def));
                self.render_args(defs, args, out);
            }
            TyKind::Param { def } => out.push_str(&defs.get(*def).name),
            TyKind::Borrowed { mutable, inner } => {
                out.push_str(if *mutable { "&mut " } else { "&" });
                self.render_into(defs, *inner, out);
            }
            TyKind::Tuple(elements) => {
                out.push('(');
                for (at, element) in elements.iter().enumerate() {
                    if at > 0 {
                        out.push_str(", ");
                    }
                    self.render_into(defs, *element, out);
                }
                out.push(')');
            }
            TyKind::Closure { params, ret } => {
                out.push('(');
                for (at, param) in params.iter().enumerate() {
                    if at > 0 {
                        out.push_str(", ");
                    }
                    self.render_into(defs, *param, out);
                }
                out.push_str(") -> ");
                self.render_into(defs, *ret, out);
            }
            TyKind::Nullable(inner) => {
                // `&T?` is `&(T?)`, so a nullable borrow needs
                // its parentheses to survive a round trip. Decision 6 writes it
                // `(&T)?` for the same reason.
                let wrap = matches!(self.kind(*inner), TyKind::Borrowed { .. });
                if wrap {
                    out.push('(');
                }
                self.render_into(defs, *inner, out);
                if wrap {
                    out.push(')');
                }
                out.push('?');
            }
            TyKind::Object { interface, args } => {
                let _ = write!(out, "any {}", name_for_reader(defs, *interface));
                self.render_args(defs, args, out);
            }
            TyKind::SelfType { .. } => out.push_str("Self"),
            TyKind::SelfAssoc { assoc } => {
                let _ = write!(out, "Self.{}", defs.get(*assoc).name);
            }
        }
    }

    /// `[T]`, `[T, 4]`, or nothing.
    ///
    /// Brackets always wrap the list now (§4.3): there is no bare,
    /// unbracketed form to weigh against a parenthesised one the way `of T`
    /// against `of (T, 4)` used to be, so one argument and several are
    /// printed the same way.
    fn render_args(&self, defs: &DefTable, args: &[GenericArg], out: &mut String) {
        if args.is_empty() {
            return;
        }
        out.push('[');
        for (at, arg) in args.iter().enumerate() {
            if at > 0 {
                out.push_str(", ");
            }
            match arg {
                GenericArg::Type(ty) => self.render_into(defs, *ty, out),
                GenericArg::Const(form) => out.push_str(&form.render(defs)),
                GenericArg::Error => out.push_str("{unknown}"),
            }
        }
        out.push(']');
    }

    // --- interning internals ---------------------------------------------

    /// Whether a kind about to be interned mentions the error type.
    ///
    /// Every operand is already interned, so this reads flags rather than
    /// walking a tree. That is the whole of §5's *"computed once, on the way
    /// in"*.
    fn kind_has_error(&self, kind: &TyKind) -> bool {
        match kind {
            TyKind::Error => true,
            TyKind::Unit | TyKind::Param { .. } => false,
            TyKind::SelfType { .. } | TyKind::SelfAssoc { .. } => false,
            TyKind::Borrowed { inner, .. } | TyKind::Nullable(inner) => {
                self.references_error(*inner)
            }
            TyKind::Tuple(elements) => {
                elements.iter().any(|element| self.references_error(*element))
            }
            TyKind::Closure { params, ret } => {
                self.references_error(*ret)
                    || params.iter().any(|param| self.references_error(*param))
            }
            TyKind::Named { args, .. } | TyKind::Object { args, .. } => {
                args.iter().any(|arg| self.arg_has_error(arg))
            }
        }
    }

    fn arg_has_error(&self, arg: &GenericArg) -> bool {
        match arg {
            GenericArg::Type(ty) => self.references_error(*ty),
            GenericArg::Const(_) => false,
            GenericArg::Error => true,
        }
    }
}

/// A definition's name as a diagnostic says it.
///
/// **The bare name, not `DefTable::path_of`**, and the choice is the same one
/// `normal`'s [`crate::Atom::render`] already made. A file is a module (§4.4),
/// so `path_of` prefixes every user type with the file it was declared in —
/// `Doc` written and read in one file prints as `documents.Doc`, and a
/// prelude type prints as `core.Array`, where `core` is a module
/// `builtins.rs` is explicit that *"no `use` can reach"*. A message full of
/// paths the reader did not write and mostly cannot write is worse than a
/// message with a name in it.
///
/// **The cost is that two `Token`s in two modules print the same**, and this
/// crate already has the answer for that: `SC0261`'s legend puts a secondary
/// label at each definition site, which is `diagnostics`' §2 — *"it is why
/// the atoms carry `DefId`s rather than names: the definition span is one
/// lookup away, and two parameters that share a name do not share a legend
/// entry"*. A [`TyKind::Named`] carries a `DefId` for exactly the same reason,
/// so a mismatch diagnostic that needs to tell two `Token`s apart points at
/// both rather than lengthening both.
fn name_for_reader(defs: &DefTable, def: DefId) -> String {
    defs.get(def).name.clone()
}
