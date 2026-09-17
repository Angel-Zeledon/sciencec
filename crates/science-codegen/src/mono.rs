//! The monomorphisation walk: which copies of which functions a backend emits.
//!
//! # 0. What this is, and the one sentence in the corpus that puts it here
//!
//! `type-checking-and-mir.md` Decision 25 fixes the order as *"types → THIR
//! analyses → MIR → regions → **mono** → codegen"*, and
//! `codegen-and-linking.md` opens by saying it is *"everything **below**
//! monomorphisation"*. Between those two sentences the pass has no home. The
//! sentence that gives it one is Decision 42's table, which puts *"mangling and
//! the monomorphisation walk"* in the target-independent half of
//! `science-codegen` — this crate — and everything that can be read off an
//! instantiation without naming a backend is here with it.
//!
//! **What the corpus does not contain, and what this module therefore had to
//! decide rather than implement:** the roots, the worklist, the termination
//! rule, and what happens when a generic instantiates itself at a larger type.
//! `type-checking-and-mir.md` names the *key* (§8 item 4) and nothing else;
//! `codegen-and-linking.md` names the *order* (Decision 4) and nothing else.
//! Each decision below says so where it is taken.
//!
//! # 1. The shape: a worklist, and a fixed point that is a reachability closure
//!
//! ```text
//!   roots  ──►  queue  ──►  pop an instance
//!                 ▲              │
//!                 │              ├─ already emitted?  ──►  drop it
//!                 │              │
//!                 │              ├─ emit it into `BTreeMap<symbol, item>`
//!                 │              │
//!                 └──────────────┴─ walk its MIR; every call it makes,
//!                                   instantiated at *this* instance's
//!                                   arguments, is pushed
//! ```
//!
//! The fixed point is reached when the queue empties, and it empties because
//! nothing is pushed twice: the `BTreeMap` keyed by mangled symbol is both the
//! dedup set and the output. That is one structure doing two jobs on purpose —
//! a separate `seen` set is a second thing that can disagree with the emitted
//! set, and the disagreement is silent in exactly the direction §15 warns
//! about.
//!
//! It is a *reachability* closure and not a transitive-closure computation over
//! the call graph, and [`science_mir::CallGraph`] is deliberately not used
//! here. The graph is keyed on [`DefId`] and the walk is keyed on
//! `(DefId, arguments)`: `f` calling `f` is one edge in the graph and either one
//! instance or infinitely many here, so the graph cannot answer the question
//! this pass asks. What the graph is for is §6's *diagnosis* — a chain the walk
//! refused is easier to read next to the SCC it lives in — and the pass reports
//! its own chain instead, for the reason §6 gives.
//!
//! # 2. What a call site does not carry, and how the arguments are recovered
//!
//! **MIR does not record a call's generic arguments, and neither does THIR.**
//! [`science_mir::TerminatorKind::Call`] carries a [`science_mir::Callee`], a
//! list of operands and a destination; `science_types`' `ExprKind::Call`
//! carries a callee expression and a list of arguments. The substitution the
//! checker solved lives in `BodyChecker::instantiate_call`, is applied to the
//! argument and return types, and is then dropped.
//!
//! So this pass **recovers** the instantiation by matching the callee's
//! declared signature against the types MIR already records at the call site:
//! the argument places' types and the destination place's type, each first
//! rewritten through the *caller's* own instantiation. `def largest of T(items:
//! borrowed Array of T)` called with a `borrowed Array of Int` gives `T := Int`
//! by structural match.
//!
//! **This recovers strictly more than the checker solved.** `check`'s
//! `instantiate_call` is root-level only — its own comment says *"a parameter
//! whose type **is** the generic parameter is solved by the argument's
//! synthesised type; anything deeper is §6's hole"* — so `T` under an
//! `Array of` is `Ty::ERROR` to the checker and `Int` here. That is not this
//! pass being clever; it is this pass having the argument's *lowered* type
//! rather than a probe of its syntax.
//!
//! **What it costs, and it is the honest cost of recovering rather than
//! recording:**
//!
//! 1. **A type parameter mentioned nowhere in the signature cannot be
//!    recovered.** There is no such function today — Science has no phantom
//!    parameter and no turbofish — but the day either arrives, the recovery has
//!    no site to read and [`Unsolved`] is what the walk reports.
//! 2. **A parameter solved only by a literal argument is solved from the return
//!    type or not at all**, because [`science_mir::Constant::Literal`] carries
//!    no type — *"the value a `1` denotes is a fact about its type, and the
//!    type is on the statement beside it"*, and the statement is not at the
//!    call. `let a: Int be identity(1)` is solved, from the destination.
//!
//!    **`let a be identity(1)` is not, and the reason is upstream and worth
//!    recording.** The checker leaves an unannotated numeric binding's type at
//!    `Ty::ERROR` and reports nothing — `let n be 1` does it on its own, with
//!    no generic call anywhere near — so the destination this recovery would
//!    read is a hole. Decision 2's defaulting is written and `check`'s §4 has a
//!    writeback for it, so this reads like a defect rather than a design
//!    limit; it is `science-types`' either way and is reported rather than
//!    worked around. Measured on the corpus it costs nothing, because every
//!    example annotates. What it costs *here* is that this module's own
//!    fixtures annotate too, which is a fixture that is less like real code
//!    than it should be.
//! 3. **It is work proportional to the signature at every call**, where a
//!    recorded substitution would have been a clone.
//!
//! **The fix is one field**, and it is named here rather than worked around
//! quietly: `ExprKind::Call` and `ExprKind::MethodCall` should carry the
//! `Vec<GenericArg>` the checker solved, and [`science_mir::TerminatorKind::Call`]
//! should carry it down. That crate is not this one's to edit. Until then the
//! recovery is the whole of what this pass knows, and every hole it leaves is
//! counted in [`Holes`] rather than assumed away.
//!
//! # 3. The key is `MonoKey`, and the type-argument half beside it
//!
//! `type-checking-and-mir.md` §8 item 4: *"two instantiations that are `EQUAL`
//! must produce one symbol, or `a * b` and `b * a` link to two copies of the
//! same function. The key is the normal form, serialised in atom order."*
//!
//! [`science_types::MonoKey`] is that, and it says of itself that it is *"the
//! const-argument half of one instantiation's identity"* — the type half being
//! the checker's. [`Instance`] is the pair. The const half is not re-derived
//! here: an [`Instance`]'s const arguments are the [`NormalForm`]s the checker
//! produced, carried through [`Instance::key`], and the symbol's const fields
//! are encoded from that key and from nothing else. A second normalisation in
//! this crate is precisely the *"two normalisers that disagree"* that
//! [`crate::mangle`] refuses, one level up.
//!
//! **Why the key had to be reproducible, and what this pass spends.**
//! `normal.rs` §2 is the record: the atom order was `DefId`, `DefId` is
//! allocation order, `sciencec` hands out `FileId`s in command-line order, and
//! so *"the same program could produce different symbol names depending on how
//! it was invoked"*. `AtomOrder` replaced the id with a rank computed from the
//! canonical path. **This is the pass that spends that.** A const argument
//! reaching a symbol here is `k` with no terms — a plain integer, invocation-
//! independent — and a const argument that still has terms is a residual, which
//! is [`Unsolved::Residual`] and not a symbol.
//!
//! # 4. The symbol is encoded from the checker's `Ty`, not from `CgTy`, and
//!    that is a correction
//!
//! [`crate::mangle::mangle`] encodes a [`crate::layout::CgTy`]. A `CgTy` is the
//! *layout* model and is lossy by design: [`crate::layout::CgTy::Ptr`] carries
//! a kind and no pointee, and [`crate::layout::CgTy::Interface`] carries no
//! interface. So through that encoder:
//!
//! - `f of (Box of Int)` and `f of (Box of String)` are one symbol, and
//! - `g of (any Summarize)` and `g of (any Report)` are one symbol.
//!
//! Both are correct for a layout — a pointer is a word whatever it points at —
//! and both are `SC0404` raised against a program with nothing wrong with it.
//! §15's *"one key producing two symbols"* is the undetectable direction; this
//! is the detectable one, and it fires on ordinary code.
//!
//! **The decision: this module encodes the checker's [`Ty`] directly, nominally
//! by canonical path, and shares with [`crate::mangle`] only
//! [`crate::mangle::assemble`] — the `_S`, the length prefixes and the `E`.**
//!
//! **The reason** is that the mono key must be injective over *instantiations*
//! and `CgTy` is injective only over *layouts*, and those are different
//! equivalence relations. Lowering `Ty` to `CgTy` first and mangling that would
//! make the key coarser than the thing it identifies, which is the one property
//! a key may not have.
//!
//! **The cost is two type encoders in one crate**, which is the shape of defect
//! §15 warns about, so the frame they share is exactly one function and the
//! difference between them is written down at both ends —
//! [`crate::mangle::assemble`] says it here, and `encode_ty` there says what it
//! erases and that the walk does not use it. The `CgTy` encoder still has a
//! caller coming: the descriptors of Decision 20 and the drop glue of
//! Decision 12 are keyed by a monomorphised *type*, and when they are built
//! they inherit the defect and should take the same fix.
//!
//! # 5. Reproducibility: what makes the output the same on any machine
//!
//! `codegen-and-linking.md` Decision 4: *"every monomorphised item is emitted in
//! sorted order of its mangled symbol name"*, because `self-hosting.md` §15
//! names hash-map iteration order as the unknown source behind Gate J. Four
//! things carry that here, and the fourth is the one that was nearly missed:
//!
//! 1. **The output is a [`BTreeMap`] keyed by the mangled symbol.**
//!    [`MonoSet::emission_order`] is its iterator, and no `HashMap`'s iteration
//!    order reaches it. There are four `HashMap`s in this module — [`Solution`]'s
//!    two, [`Mono::impl_ordinal`] and [`function_values`]'s — and three of them
//!    are read by key and never iterated. The fourth is iterated exactly once,
//!    at the end of [`function_values`], to build **another `HashMap`**, so the
//!    order it is walked in changes nothing about what comes out. That sentence
//!    is the kind of claim that goes stale, which is why `tests/mono.rs` runs
//!    the whole pipeline twice and compares bytes instead of believing it.
//! 2. **The symbol contains no `DefId` and no `Ty`.** Both are allocation
//!    order. A path component is a name the author wrote; a const argument is
//!    an integer; a type argument is a structure over names. Nothing in a
//!    symbol moves when a file moves on the command line.
//! 3. **The symbol contains no hash**, which is Decision 16 and is inherited.
//! 4. **The *queue* order does not reach the output.** It is a `VecDeque` and
//!    it is FIFO, so the walk is breadth-first and the shortest instantiation
//!    chain to an instance is the one recorded — but even that is only for
//!    [`MonoItem::reached_from`] and for §6's message. The set and its order
//!    are the `BTreeMap`'s, so a walk that visited the same instances in a
//!    different order produces the same output bytes. That is why the
//!    determinism test can lower the same crate twice and compare, and why it
//!    would still pass if the queue were a stack.
//!
//! # 6. Termination, which is the part nothing in the corpus specifies
//!
//! `def f of T(x: T)` whose body calls `f(array_of(x))` needs `f[Int]`,
//! `f[Array of Int]`, `f[Array of (Array of Int)]`, and does not terminate.
//! Rust answers with `recursion_limit` and an error; nothing in this project
//! answers at all. `matching.rs` §4 is the corpus noticing the gap from the
//! other side: it declines to build `SC0262` because *"§9.4 makes the
//! instantiation chain the whole of the message, and there is no monomorphiser
//! to walk for a chain yet"*.
//!
//! > **Decision. Two rules, and the diagnostic is the chain.** An instance is
//! > refused when an ancestor in its own instantiation chain has the same
//! > [`DefId`] and arguments that are *contained* in this one's, at least one
//! > of them properly. That is the **containment rule**, it is the cause rather
//! > than a symptom, and it fires at the third link of the example above. A
//! > **depth backstop** at [`Mono::LIMIT`] catches what containment does not.
//! > Both report `SC0407` and both print the chain.
//!
//! **The reason for containment first.** A depth limit is a number the compiler
//! chose, and a message built on one can only say *"this got too deep"*. The
//! chain says which call grew and by what; a reader who sees `f[Int]`,
//! `f[Array of Int]`, `f[Array of (Array of Int)]` printed in order does not
//! need to be told what the rule is. It also fires at depth three rather than
//! at depth `LIMIT`, so the compiler does no work it is going to throw away.
//!
//! **The reason for the backstop, which is that containment is not complete.**
//! Containment is a sufficient condition for unbounded growth and not a
//! necessary one. The counterexample, written down rather than left to be
//! found: `f of (Pair of (Int, T))` calling `f of (Pair of (Bool, Array of T))`
//! calling `f of (Pair of (Int, Array of (Array of T)))` grows without any
//! ancestor's argument being a subterm of a descendant's, because the first
//! component alternates. The backstop is what stops that, and its message says
//! that the containment rule did not fire so the reader knows the chain is
//! evidence rather than explanation.
//!
//! **What neither rule does is bound the *size* of a terminating program's
//! output.** If no chain grows, the reachable set is finite, because the
//! instantiations reachable from a finite program with no growth are types of
//! bounded size over a finite alphabet. That is the termination argument, and
//! it is why "no growth" is the right thing to test for rather than "few
//! instances".
//!
//! **The cost.** The containment test is `O(chain × arguments × type size)` at
//! every push, and the chain is carried per queued instance rather than
//! reconstructed, so a wide program pays in allocation. Both are bounded by
//! [`Mono::LIMIT`] and neither is measurable next to the type checker. `LIMIT`
//! is 64 rather than Rust's 128 because the containment rule is what catches
//! the real cases and the backstop only has to be larger than any honest
//! program's instantiation depth.
//!
//! # 7. What a root is, and what the answer costs
//!
//! > **Decision. The default root set is the entry point and nothing else, and
//! > a library build is [`RootSet::EveryBody`], which is every body that takes
//! > no generic arguments.**
//!
//! **The reason the default is the entry point.** `codegen-and-linking.md`
//! Decision 27 says `science-rt`'s symbols *"are not re-exported from the
//! resulting binary"*, §5.6 says there are no dynamic Science libraries because
//! *"there is no stable Science ABI, so a `.so` of Science code has no consumer
//! that could survive a compiler upgrade"*, and §14 does not plan one. In a
//! world with no consumer for an exported Science symbol, a root set larger
//! than the entry point emits code into every binary that nothing can call.
//! The `SC0403` case — *"a binary needs either top-level statements or a
//! `def main`"* — is the same sentence from the other side: a file with no
//! entry point is a library, and `sciencec build` already refuses it.
//!
//! **The cost of the default, which is real:** an address-taken function is a
//! root the entry point does not name, and the walk has to find it. It does —
//! a [`science_mir::Constant::Item`] naming a function with a body is queued
//! wherever it is read, not only where it is called — but that is a rule that
//! has to be right rather than a consequence of the root set. A closure's body
//! is the case it cannot cover, and §8 says why.
//!
//! **What `EveryBody` costs, and why it is not `public`.** The right root set
//! for a library is *"every `public` item and everything it reaches"*, and it
//! cannot be written: `public` does not survive the resolver.
//! `ast::FnDecl::is_pub` exists, `ast::FnForm::Tool` exists,
//! [`science_resolve::hir::Fn`] carries neither, and
//! [`science_resolve::hir::Def`] has `id`, `kind`, `name`, `span` and `parent`
//! and no visibility. So the two answers available are *"the entry point"* and
//! *"everything"*, and `EveryBody` is the second: it emits every non-generic
//! body in the crate, including the ones a `public` root set would have left
//! out. Measured over `examples/`: `19_stdlib.science` is 34 items from the
//! entry point and 41 from every body, and `10_loops.science` is 15 and 31. The
//! whole corpus is 361 items under `EveryBody`, 41 of them instances of a
//! generic.
//!
//! **What is needed to close it is one `bool` and one enum**, and it is named
//! precisely because a workaround here would be a second visibility rule:
//! `hir::Fn` should carry `is_pub: bool` and the `FnForm` the parser already
//! read, and [`RootSet`] gains the variant that reads them. `tool` matters
//! separately from `public` — `script-mode.md` makes a `tool` an entry point in
//! its own right, so it is a root even in a binary build — and neither fact is
//! reachable from here today.
//!
//! **There is already one workaround for this in the workspace, and it is the
//! evidence that the field is missing rather than merely unused.**
//! `sciencec`'s `tools.rs` answers *"which functions are tools"* by matching
//! `ast::FnForm::Tool` on the **AST**, one phase above the resolver, because
//! that is the last place the answer exists. A monomorphiser cannot do the
//! same: it has no AST, and reaching for one would put the parser in Decision
//! 42's target-independent half.
//!
//! # 8. What the walk cannot follow, and why each one is a hole rather than an
//!    assumption
//!
//! Every one of these is counted in [`Holes`] rather than skipped, because a
//! monomorphisation set that is quietly incomplete is a link error at the end
//! of a long build.
//!
//! - **A closure's body is not lowered.** `science-mir`'s §5 says so —
//!   [`science_mir::Rvalue::Closure`] carries a `thir_body` and not a `DefId`,
//!   *"because the closure's body has no `DefId` to be a `Body` of"*. A closure
//!   is a function a backend must emit, and this pass cannot name it. This is
//!   the largest hole in the module and it closes when `science-mir`'s §8.5
//!   does.
//! - **An indirect call has no target.** [`science_mir::Callee::Indirect`]
//!   holds an operand. The functions it could reach are exactly the
//!   address-taken ones, which the walk roots anyway, so the *set* is still a
//!   superset of what is needed — but it is a superset by way of a different
//!   rule, which is worth knowing when the numbers are read.
//! - **An unresolved callee is Decision 11's hole.**
//!   [`science_mir::Unresolved`] names which. Every `Array` and `Map` method in
//!   the corpus is one of these, so on today's examples the walk stops at the
//!   container boundary and says how often.
//! - **A runtime entry point is not a mono item.**
//!   [`science_mir::Callee::Runtime`] is Decision 5's, and `science-rt` is C:
//!   Decision 20's descriptor convention exists *"so that monomorphisation does
//!   not multiply the container"*. The runtime is called, never instantiated.
//! - **An `extern` callee is a declaration, not a definition.**
//!   [`science_resolve::hir::DefKind::ExternFn`]'s own documentation says
//!   *"codegen emits a declaration rather than a definition"*. It is counted
//!   and not emitted — and it is where Decision 18 is checked; see §9.
//!
//! # 9. Decision 18's boundary, which was reserved and unenforced
//!
//! `type-checking-and-mir.md` §9.1: *"an `extern "C"` function is not generic
//! and cannot be. **Decision 18:** a generic Science function may be *called*
//! from an FFI wrapper, but a function declared in an `extern` block is
//! monomorphic, and a generic function may not be passed as a C callback
//! without an explicit instantiation. The error is `SC0522` and it names the
//! instantiation to write."*
//!
//! **It was enforced nowhere.** `science-types`' `codes` module reserves
//! `SC0522`, declines to define it, and says why: *"`SC0522` is a declaration
//! check — a generic function reached through an `extern` block — and needs the
//! monomorphiser's view of which instantiations cross"*. That crate has a test
//! asserting the code is absent from its own list. Nothing else in the
//! workspace mentions the number.
//!
//! **It is enforced here**, in the one shape the walk can see: an argument to a
//! [`science_resolve::hir::DefKind::ExternFn`] call that is a
//! [`science_mir::Constant::Item`] naming a function with generic parameters.
//! [`crate::diagnostics::generic_across_c_boundary`] is the message and
//! [`crate::diagnostics::code::SC0522`] says why the code is emitted from this
//! crate and not that one.
//!
//! **What that does not cover, stated because the check reads as if it covers
//! more:** a generic function bound to a local and passed through it, a generic
//! reaching an `extern` through a closure, and a generic function pointer
//! stored in a record that crosses. All three need the address-taken set to
//! carry *where each address flows*, which is a call-target analysis and is the
//! same thing [`science_mir::callgraph`]'s §3 says is missing for indirect
//! edges. Decision 18's *"names the instantiation to write"* is also only half
//! met, and the half that fails is not this pass's: **Science has no syntax for
//! an explicit type argument at a use site**, so there is no instantiation to
//! name. The message says so.
//!
//! # 10. What is not here
//!
//! - **Drop glue (Decision 12) and type-info descriptors (Decision 20).** Both
//!   are monomorphised items keyed by the mangling of §2.7, and both need a
//!   `Ty → CgTy` lowering and a layout to decide *"does this type need a
//!   destructor"*. That lowering does not exist, is a piece of work in its own
//!   right, and would be the third thing in this crate to answer "what is this
//!   type" — so it is named rather than begun. [`MonoSet::types_reached`] is
//!   the input it will want: every distinct type argument the walk saw, which
//!   is what Decision 20's *"one descriptor per monomorphised element type"*
//!   is a function of.
//! - **Codegen units.** Decision 4 is one module per crate and Decision 43 makes
//!   units F1.
//! - **`science-db`'s `mono_items` query.** `pending.rs` stubs it with
//!   `unimplemented!("science-codegen: monomorphization")`, and its inputs —
//!   `mir`, `signature`, `thir` — are stubbed the same way, so a body written
//!   there today would be a call into stubs. The placeholder
//!   `MonoItem` in that file is this module's [`MonoItem`] and should move here
//!   when the query below it is real.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use science_diagnostics::{Diagnostic, Span};
use science_mir::mir::{Body, Callee, Constant, Local, Operand, Place, Rvalue, StatementKind};
use science_mir::TerminatorKind;
use science_resolve::hir::{self, DefId, DefKind, DefTable, GenericParamKind};
use science_types::items::Declarations;
use science_types::matching::match_linear;
use science_types::ty::{GenericArg, Ty, TyKind, Types};
use science_types::{MonoKey, NormalForm, Substitution};

use crate::diagnostics::{cyclic_instantiation, generic_across_c_boundary, symbol_collision};
use crate::mangle::{assemble, encode_const};

// --- the instance ---------------------------------------------------------

/// One monomorphised item: a definition and the arguments it was reached with.
///
/// The arguments are in declaration order, **the enclosing implementation
/// block's first**. A method of `Doc of T has:` declaring `of U` is reached as
/// `[T's argument, U's argument]`, because that is the order
/// [`Substitution::of_generics`] would zip them in and because a reader of a
/// symbol should be able to count the `of` lists left to right.
///
/// [`GenericArg`] rather than a type of this crate's own: it is what the
/// checker's substitution consumes, so an [`Instance`] can be turned back into
/// a [`Substitution`] without a translation step that could disagree with the
/// original. The [`Ty`] inside it is an index into a [`Types`] table and is
/// meaningless without one — which is why §5 is careful that no [`Ty`] reaches
/// a symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instance {
    /// The function.
    pub def: DefId,
    /// Its arguments, in declaration order.
    pub args: Vec<GenericArg>,
}

impl Instance {
    /// An instance with no generic arguments.
    pub fn plain(def: DefId) -> Instance {
        Instance { def, args: Vec::new() }
    }

    /// Whether this instance has any arguments at all.
    pub fn is_generic(&self) -> bool {
        !self.args.is_empty()
    }

    /// The const half of the key, in `of`-list order. §3.
    ///
    /// **This is [`science_types::MonoKey`] and not a list of integers**, so
    /// that the property §8 item 4 asks for — two `EQUAL` instantiations are
    /// one key — is inherited from the type that was built to hold it rather
    /// than restated here. A monomorphiser that kept `Vec<i128>` instead would
    /// be correct today and wrong the first time a const argument is an
    /// expression.
    pub fn key(&self) -> MonoKey {
        MonoKey::new(self.args.iter().filter_map(|arg| match arg {
            GenericArg::Const(form) => Some(form.clone()),
            _ => None,
        }))
    }
}

/// Why an instance could not be turned into a symbol.
///
/// A value rather than a panic or a silently dropped item, for `ty`'s §5
/// reason one phase down: a hole is a thing a consumer can count, and an
/// instance that vanished is a link error with no author.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unsolved {
    /// A generic parameter the call site did not determine.
    ///
    /// Either the parameter is mentioned nowhere in the signature, or the one
    /// place it is mentioned was [`Ty::ERROR`] because the checker had already
    /// reported something.
    Parameter {
        /// The callee whose instantiation failed.
        def: DefId,
        /// The parameter's position in the declaration order of [`Instance`].
        param: usize,
    },
    /// A const argument that still mentions a parameter.
    ///
    /// `Matrix of (T, N + 1)` reached from a caller that is itself generic in
    /// `N`. It is not an error — it means this instance is not yet concrete —
    /// and it can only happen when a generic body was made a root, which
    /// [`RootSet::EveryBody`] refuses to do.
    Residual {
        /// The callee whose instantiation failed.
        def: DefId,
        /// The parameter's position in the declaration order of [`Instance`].
        param: usize,
    },
    /// A type argument mentioning [`Ty::ERROR`], or a type whose head is not a
    /// definition a symbol can name.
    Untypeable {
        /// The callee whose instantiation failed.
        def: DefId,
        /// The parameter's position in the declaration order of [`Instance`].
        param: usize,
    },
}

impl Unsolved {
    /// The definition whose instantiation failed.
    pub fn def(&self) -> DefId {
        match self {
            Unsolved::Parameter { def, .. }
            | Unsolved::Residual { def, .. }
            | Unsolved::Untypeable { def, .. } => *def,
        }
    }
}

// --- the output -----------------------------------------------------------

/// One item a backend must emit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonoItem {
    /// The mangled symbol. This is the key the set is ordered by (Decision 4).
    pub symbol: String,
    /// What it is an instance of.
    pub instance: Instance,
    /// How it reads in a dump or a diagnostic — `stats.mean[F64]`.
    pub description: String,
    /// The symbol of the item that first reached this one, or `None` for a
    /// root. The *first* and not every one: the walk is breadth-first, so this
    /// is the shortest path from a root, which is the one a reader wants.
    pub reached_from: Option<String>,
    /// Whether this crate has MIR for it — *define* rather than *declare*.
    ///
    /// **A set without this field is ambiguous and the ambiguity is a link
    /// error.** A backend emits a definition for an item with a body and a
    /// declaration for one without, and the two callees that have none look
    /// identical from a symbol: a function declared in an `extern` block
    /// (`DefKind::ExternFn`'s own documentation says *"codegen emits a
    /// declaration rather than a definition"*) and a prelude signature the
    /// declaration table carries with `has_body: false`. Asking the caller to
    /// re-derive it would mean handing them the `DefTable` as well.
    pub defined_here: bool,
}

/// What the walk could not follow. §8.
///
/// Counts rather than lists for the four that are structural and a list for the
/// one that is not: an [`Unsolved`] names a definition somebody can go and look
/// at, and the other four name a language feature that is not built.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Holes {
    /// [`science_mir::Callee::Indirect`] — a call through a value.
    pub indirect_calls: usize,
    /// [`science_mir::Callee::Unresolved`] — Decision 11's method lookup, a
    /// `for` loop's `next`, an operator on a user type.
    pub unresolved_callees: usize,
    /// [`science_mir::Callee::Runtime`] — not a mono item; see §8.
    pub runtime_calls: usize,
    /// Calls to a function declared in an `extern` block. Emitted as a
    /// declaration, never instantiated.
    pub extern_calls: usize,
    /// [`science_mir::Rvalue::Closure`] — a body that was not lowered.
    pub closures: usize,
    /// A callee with no signature at all: a prelude name the declaration table
    /// does not carry.
    ///
    /// **It is still given a symbol**, because a call to it is a call a backend
    /// has to emit and dropping it would turn a missing declaration into a
    /// missing *call*. What the reader gets instead is an undefined symbol at
    /// link time, which `codegen-and-linking.md` Decision 29 already has a
    /// diagnostic for, and this count, which says how many to expect.
    pub undeclared_callees: usize,
    /// Instantiations the walk could not make concrete, in the order it met
    /// them.
    pub unsolved: Vec<Unsolved>,
}

impl Holes {
    /// Whether the walk followed everything it met.
    ///
    /// A monomorphisation set over a program this is false of is a *lower
    /// bound* on what a backend needs, and the caller should know which.
    pub fn is_empty(&self) -> bool {
        *self == Holes::default()
    }
}

/// Every instance the program needs, in emission order.
#[derive(Debug, Clone, Default)]
pub struct MonoSet {
    items: BTreeMap<String, MonoItem>,
    /// Every distinct type argument the walk saw, rendered. §10: this is what
    /// Decision 20's descriptor emission will be a function of.
    types_reached: BTreeSet<String>,
    holes: Holes,
    diagnostics: Vec<Diagnostic>,
}

impl MonoSet {
    /// Decision 4's order: sorted by mangled symbol name.
    ///
    /// **The order is the map's, not a sort applied afterwards.** A `sort_by`
    /// at the end is a line somebody can delete; a [`BTreeMap`] is a type that
    /// cannot hold an unsorted answer. `self-hosting.md` §15 names hash-map
    /// iteration order as the unknown source behind Gate J, and this closes it
    /// by construction rather than by testing — which is Decision 4's own
    /// phrasing.
    pub fn emission_order(&self) -> impl Iterator<Item = &MonoItem> {
        self.items.values()
    }

    /// The symbols, in emission order.
    pub fn symbols(&self) -> impl Iterator<Item = &str> {
        self.items.keys().map(String::as_str)
    }

    /// How many items a backend would emit.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether the program needs nothing emitted.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The item with this symbol.
    pub fn get(&self, symbol: &str) -> Option<&MonoItem> {
        self.items.get(symbol)
    }

    /// What the walk could not follow. §8.
    pub fn holes(&self) -> &Holes {
        &self.holes
    }

    /// Every distinct monomorphised type the walk met as an argument, sorted.
    pub fn types_reached(&self) -> impl Iterator<Item = &str> {
        self.types_reached.iter().map(String::as_str)
    }

    /// `SC0404`, `SC0407`, `SC0522` — everything the walk reported.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// The whole set as one deterministic block of text, for a dump and for the
    /// determinism test.
    ///
    /// **A rendering rather than a comparison of the structure**, because the
    /// claim being tested is *"the same bytes"*, and a structural comparison
    /// can agree while a rendering that walks a `HashMap` disagrees. This is
    /// the thing the test compares.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for item in self.emission_order() {
            out.push_str(&item.symbol);
            out.push_str("  # ");
            out.push_str(&item.description);
            out.push('\n');
        }
        out
    }
}

// --- roots ----------------------------------------------------------------

/// Where the walk starts. §7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RootSet {
    /// A binary: the entry point, and everything it reaches.
    #[default]
    EntryPoint,
    /// A library: every body that takes no generic arguments.
    ///
    /// Not *"every `public` body"*, which is the right answer and is
    /// unavailable — §7 says what is missing and how much it costs.
    EveryBody,
}

// --- the walk -------------------------------------------------------------

/// The monomorphisation walk.
///
/// Built over one crate's lowered bodies and the tables they were lowered
/// against, then run once. It holds `&mut Types` because substituting a
/// caller's arguments into a callee's signature interns types that did not
/// exist before — which is the same reason [`science_mir::Context`] holds one.
pub struct Mono<'a> {
    defs: &'a DefTable,
    decls: &'a Declarations,
    types: &'a mut Types,
    bodies: BTreeMap<DefId, &'a Body>,
    /// Which of its parent's implementation blocks each `impl` is, among those
    /// with the same self-type name. See [`Mono::path_of`].
    impl_ordinal: HashMap<DefId, usize>,
}

impl<'a> Mono<'a> {
    /// The instantiation depth the walk refuses to go past. §6.
    ///
    /// 64 rather than Rust's 128 because the containment rule is what catches
    /// the real cases and this only has to exceed any honest program's
    /// instantiation depth. A program that legitimately nests 64 generic calls
    /// each at a new type argument does not exist; if one does, the diagnostic
    /// says which rule fired, which is the information needed to raise it.
    pub const LIMIT: usize = 64;

    /// A walk over one crate.
    pub fn new(
        defs: &'a DefTable,
        decls: &'a Declarations,
        types: &'a mut Types,
        bodies: &'a [Body],
    ) -> Mono<'a> {
        let mut index = BTreeMap::new();
        for body in bodies {
            index.insert(body.def(), body);
        }
        let impl_ordinal = impl_ordinals(defs, decls);
        Mono { defs, decls, types, bodies: index, impl_ordinal }
    }

    /// Every instance the program needs, from the given roots. §1.
    pub fn collect(&mut self, roots: RootSet) -> MonoSet {
        let mut set = MonoSet::default();
        let seeds = self.roots(roots, &mut set);
        let mut queue: VecDeque<Task> = VecDeque::new();
        for seed in seeds {
            queue.push_back(Task { instance: seed, chain: Vec::new(), from: None });
        }

        while let Some(task) = queue.pop_front() {
            let symbol = match self.symbol_of(&task.instance) {
                Ok(symbol) => symbol,
                Err(unsolved) => {
                    set.holes.unsolved.push(unsolved);
                    continue;
                }
            };
            if let Some(existing) = set.items.get(&symbol) {
                if existing.instance != task.instance {
                    // `SC0404`: two distinct keys, one symbol. The check is
                    // free here because the emitted map is keyed by the symbol
                    // — Decision 4's *"a single pass over one symbol table"*
                    // turns out to be no pass at all.
                    set.diagnostics.push(symbol_collision(
                        &symbol,
                        &existing.description,
                        &self.describe(&task.instance),
                    ));
                }
                continue;
            }

            let description = self.describe(&task.instance);
            for arg in &task.instance.args {
                if let GenericArg::Type(ty) = arg {
                    set.types_reached.insert(self.types.render(self.defs, *ty));
                }
            }
            set.items.insert(
                symbol.clone(),
                MonoItem {
                    symbol: symbol.clone(),
                    instance: task.instance.clone(),
                    description,
                    reached_from: task.from.clone(),
                    defined_here: self.bodies.contains_key(&task.instance.def),
                },
            );

            let chain = {
                let mut chain = task.chain;
                chain.push(task.instance.clone());
                chain
            };
            self.walk_body(&task.instance, &chain, &symbol, &mut queue, &mut set);
        }
        set
    }

    // --- roots ------------------------------------------------------------

    fn roots(&mut self, policy: RootSet, set: &mut MonoSet) -> Vec<Instance> {
        match policy {
            RootSet::EntryPoint => self.entry_point().into_iter().map(Instance::plain).collect(),
            RootSet::EveryBody => {
                let mut out = Vec::new();
                for def in self.bodies.keys().copied() {
                    if self.generics_of(def).is_empty() {
                        out.push(Instance::plain(def));
                    } else {
                        // A generic body cannot be a root: there is nothing to
                        // instantiate it *at*. It is emitted once some caller
                        // reaches it, and if nothing does it is dead code that
                        // a library build of a language with no exported
                        // generics has no way to hand out.
                        set.holes.unsolved.push(Unsolved::Parameter { def, param: 0 });
                    }
                }
                out
            }
        }
    }

    /// The entry point: a module-level `def main`.
    ///
    /// **The lowest [`DefId`] when there are several**, which is the one
    /// command-line order picked, and that is the correct dependence rather
    /// than the hazard `normal.rs` §2 is about: *which file is the program* is
    /// genuinely a fact about the invocation, and `script-mode.md`'s driver
    /// rule is already *"the first module is the one the user named"*. Nothing
    /// derived from this reaches a symbol.
    pub fn entry_point(&self) -> Option<DefId> {
        let mut found: Option<DefId> = None;
        for def in self.bodies.keys().copied() {
            let entry = self.defs.get(def);
            if entry.kind != DefKind::Fn || entry.name != "main" {
                continue;
            }
            let module = entry.parent.map(|parent| self.defs.get(parent).kind);
            if module != Some(DefKind::Module) {
                continue;
            }
            if found.is_none_or(|current| def.index() < current.index()) {
                found = Some(def);
            }
        }
        found
    }

    // --- walking one body -------------------------------------------------

    fn walk_body(
        &mut self,
        caller: &Instance,
        chain: &[Instance],
        symbol: &str,
        queue: &mut VecDeque<Task>,
        set: &mut MonoSet,
    ) {
        let Some(body) = self.bodies.get(&caller.def).copied() else {
            // A callee with no body here: an `extern` declaration, an
            // interface's required method, a prelude signature. Counted at the
            // call site rather than here, where the reason is not known.
            return;
        };
        let caller_subst = self.substitution_of(caller);
        let function_values = function_values(body);

        for (_, block) in body.blocks() {
            for statement in &block.statements {
                if let StatementKind::Assign { rvalue, .. } = &statement.kind {
                    self.walk_rvalue(rvalue, symbol, queue, set, statement.span);
                }
            }
            let span = block.terminator.span;
            let TerminatorKind::Call { callee, args, destination, .. } = &block.terminator.kind
            else {
                continue;
            };
            for arg in args {
                self.note_address_taken(arg, symbol, queue, set);
            }
            match callee {
                Callee::Indirect(_) => set.holes.indirect_calls += 1,
                Callee::Unresolved(_) => set.holes.unresolved_callees += 1,
                Callee::Runtime(_) => set.holes.runtime_calls += 1,
                Callee::Def(def) => {
                    if self.defs.get(*def).kind == DefKind::ExternFn {
                        set.holes.extern_calls += 1;
                        self.check_c_boundary(*def, args, &function_values, span, set);
                        continue;
                    }
                    let solved =
                        self.solve_call(&caller_subst, body, *def, args, destination, set);
                    let Some(instance) = solved else { continue };
                    self.enqueue(instance, chain, symbol, span, queue, set);
                }
            }
        }
    }

    /// An `rvalue` that reads a function as a value, or builds a closure.
    fn walk_rvalue(
        &mut self,
        rvalue: &Rvalue,
        symbol: &str,
        queue: &mut VecDeque<Task>,
        set: &mut MonoSet,
        _span: Span,
    ) {
        match rvalue {
            Rvalue::Closure { .. } => set.holes.closures += 1,
            Rvalue::Use(operand)
            | Rvalue::Unary { operand, .. }
            | Rvalue::Cast { operand, .. }
            | Rvalue::Coerce { operand, .. }
            | Rvalue::Narrow { operand, .. }
            | Rvalue::IsPresent(operand) => self.note_address_taken(operand, symbol, queue, set),
            Rvalue::Binary { lhs, rhs, .. } => {
                self.note_address_taken(lhs, symbol, queue, set);
                self.note_address_taken(rhs, symbol, queue, set);
            }
            Rvalue::Tuple(operands) | Rvalue::Variant { payload: operands, .. } => {
                for operand in operands {
                    self.note_address_taken(operand, symbol, queue, set);
                }
            }
            Rvalue::Record { fields, .. } => {
                for (_, operand) in fields {
                    self.note_address_taken(operand, symbol, queue, set);
                }
            }
            Rvalue::Range { start, end, .. } => {
                self.note_address_taken(start, symbol, queue, set);
                self.note_address_taken(end, symbol, queue, set);
            }
            Rvalue::Ref { .. } | Rvalue::Discriminant(_) | Rvalue::Error => {}
        }
    }

    /// A function named as a value is a root the entry point does not name. §7.
    ///
    /// **Only a non-generic one.** A generic function read as a value has no
    /// instantiation at the read — the arrow type `(A) -> B` that
    /// `collections-and-chains.md` §1.2 settled on carries no arguments — so
    /// there is nothing to key an instance on. It is [`Unsolved::Parameter`],
    /// and it is the same gap Decision 18 refuses at the C boundary, met here
    /// at the Science one.
    fn note_address_taken(
        &mut self,
        operand: &Operand,
        symbol: &str,
        queue: &mut VecDeque<Task>,
        set: &mut MonoSet,
    ) {
        let Operand::Const(Constant::Item(def)) = operand else { return };
        let def = *def;
        if !matches!(self.defs.get(def).kind, DefKind::Fn) || !self.bodies.contains_key(&def) {
            return;
        }
        if !self.generics_of(def).is_empty() {
            set.holes.unsolved.push(Unsolved::Parameter { def, param: 0 });
            return;
        }
        queue.push_back(Task {
            instance: Instance::plain(def),
            chain: Vec::new(),
            from: Some(symbol.to_string()),
        });
    }

    /// Decision 18, at the one shape the walk can see. §9.
    fn check_c_boundary(
        &self,
        extern_fn: DefId,
        args: &[Operand],
        function_values: &HashMap<Local, DefId>,
        span: Span,
        set: &mut MonoSet,
    ) {
        for arg in args {
            let Some(def) = named_function(arg, function_values) else { continue };
            if !matches!(self.defs.get(def).kind, DefKind::Fn) {
                continue;
            }
            if self.generics_of(def).is_empty() {
                continue;
            }
            set.diagnostics.push(generic_across_c_boundary(
                &self.defs.get(def).name,
                &self.defs.get(extern_fn).name,
                span,
            ));
        }
    }

    // --- termination ------------------------------------------------------

    fn enqueue(
        &mut self,
        instance: Instance,
        chain: &[Instance],
        symbol: &str,
        span: Span,
        queue: &mut VecDeque<Task>,
        set: &mut MonoSet,
    ) {
        if let Some(growth) = self.growth_in(&instance, chain) {
            let mut links: Vec<String> =
                chain.iter().map(|link| self.describe(link)).collect();
            links.push(self.describe(&instance));
            set.diagnostics.push(cyclic_instantiation(&links, Some(growth), span));
            return;
        }
        if chain.len() >= Mono::LIMIT {
            let mut links: Vec<String> =
                chain.iter().map(|link| self.describe(link)).collect();
            links.push(self.describe(&instance));
            set.diagnostics.push(cyclic_instantiation(&links, None, span));
            return;
        }
        queue.push_back(Task {
            instance,
            chain: chain.to_vec(),
            from: Some(symbol.to_string()),
        });
    }

    /// The index of the chain link this instance strictly contains. §6.
    ///
    /// The *last* such link rather than the first, because the chain is printed
    /// outermost first and the reader is looking for the shortest cycle.
    fn growth_in(&self, instance: &Instance, chain: &[Instance]) -> Option<usize> {
        for (at, link) in chain.iter().enumerate().rev() {
            if link.def != instance.def || link.args.len() != instance.args.len() {
                continue;
            }
            if self.strictly_contains(&instance.args, &link.args) {
                return Some(at);
            }
        }
        None
    }

    /// Whether `outer` contains `inner` positionwise, properly somewhere.
    fn strictly_contains(&self, outer: &[GenericArg], inner: &[GenericArg]) -> bool {
        let mut proper = false;
        for (out, inn) in outer.iter().zip(inner) {
            match (out, inn) {
                (GenericArg::Type(a), GenericArg::Type(b)) => {
                    if a == b {
                        continue;
                    }
                    if !self.is_subterm(*b, *a) {
                        return false;
                    }
                    proper = true;
                }
                (GenericArg::Const(a), GenericArg::Const(b)) => {
                    // A const argument does not nest, so it can only be equal
                    // or different. Different is not growth: `f[1]` calling
                    // `f[2]` calling `f[3]` is unbounded and is the backstop's
                    // case, not containment's, because nothing here is a
                    // subterm of anything.
                    if a != b {
                        return false;
                    }
                }
                _ => return false,
            }
        }
        proper
    }

    /// Whether `needle` occurs inside `hay`.
    fn is_subterm(&self, needle: Ty, hay: Ty) -> bool {
        if needle == hay {
            return true;
        }
        match self.types.kind(hay) {
            TyKind::Named { args, .. } | TyKind::Object { args, .. } => args.iter().any(|arg| {
                matches!(arg, GenericArg::Type(inner) if self.is_subterm(needle, *inner))
            }),
            TyKind::Borrowed { inner, .. } | TyKind::Nullable(inner) => {
                self.is_subterm(needle, *inner)
            }
            TyKind::Tuple(elements) => {
                elements.iter().any(|element| self.is_subterm(needle, *element))
            }
            TyKind::Closure { params, ret } => {
                params.iter().any(|param| self.is_subterm(needle, *param))
                    || self.is_subterm(needle, *ret)
            }
            TyKind::Error
            | TyKind::Unit
            | TyKind::Param { .. }
            | TyKind::SelfType { .. }
            | TyKind::SelfAssoc { .. } => false,
        }
    }

    // --- solving one call -------------------------------------------------

    /// The callee's instantiation, recovered from the call site. §2.
    fn solve_call(
        &mut self,
        caller_subst: &Substitution,
        body: &Body,
        callee: DefId,
        args: &[Operand],
        destination: &Place,
        set: &mut MonoSet,
    ) -> Option<Instance> {
        let generics = self.generics_of(callee);
        if generics.is_empty() {
            if self.decls.signature(callee).is_none() {
                set.holes.undeclared_callees += 1;
            }
            return Some(Instance::plain(callee));
        }
        let Some(signature) = self.decls.signature(callee) else {
            set.holes.undeclared_callees += 1;
            return None;
        };

        // The callee's declared types, with `Self` replaced by the block's own
        // self type, so that a method's signature is written in the block's
        // generic parameters and nothing else.
        let owner = signature.owner;
        let self_subst = match owner.and_then(|owner| self.decls.self_ty(owner)) {
            Some(ty) => Substitution::new().with_self(owner.expect("checked"), ty),
            None => Substitution::new(),
        };
        let receiver_ty = owner.and_then(|owner| self.decls.self_ty(owner));
        let has_receiver = signature.self_param.is_some();
        let declared: Vec<Ty> = signature.params.iter().map(|param| param.ty).collect();
        let declared_ret = signature.ret;

        let unknowns: HashSet<DefId> = generics.iter().map(|param| param.def).collect();
        let mut solution = Solution::default();

        // The receiver, when there is one: MIR puts it at `args[0]`.
        let mut offset = 0;
        if has_receiver {
            offset = 1;
            if let (Some(declared_self), Some(actual)) = (receiver_ty, args.first()) {
                let actual = self.actual_ty(caller_subst, body, actual);
                if let Some(actual) = actual {
                    self.match_ty(declared_self, actual, &unknowns, &mut solution);
                }
            }
        }

        for (index, declared) in declared.iter().enumerate() {
            let Some(actual) = args.get(index + offset) else { continue };
            let Some(actual) = self.actual_ty(caller_subst, body, actual) else { continue };
            let declared = self.apply(&self_subst, *declared);
            self.match_ty(declared, actual, &unknowns, &mut solution);
        }

        let destination_ty = destination.ty(body);
        let destination_ty = self.apply(caller_subst, destination_ty);
        let declared_ret = self.apply(&self_subst, declared_ret);
        self.match_ty(declared_ret, destination_ty, &unknowns, &mut solution);

        self.assemble_instance(callee, &generics, &solution, set)
    }

    fn assemble_instance(
        &mut self,
        callee: DefId,
        generics: &[hir::GenericParam],
        solution: &Solution,
        set: &mut MonoSet,
    ) -> Option<Instance> {
        let mut args = Vec::with_capacity(generics.len());
        for (position, param) in generics.iter().enumerate() {
            match param.kind {
                GenericParamKind::Type { .. } => match solution.types.get(&param.def) {
                    Some(ty) if *ty != Ty::ERROR && !self.types.references_error(*ty) => {
                        args.push(GenericArg::Type(*ty));
                    }
                    Some(_) => {
                        set.holes
                            .unsolved
                            .push(Unsolved::Untypeable { def: callee, param: position });
                        return None;
                    }
                    None => {
                        set.holes
                            .unsolved
                            .push(Unsolved::Parameter { def: callee, param: position });
                        return None;
                    }
                },
                GenericParamKind::Const { .. } => match solution.consts.get(&param.def) {
                    Some(form) if form.is_constant() => {
                        args.push(GenericArg::Const(form.clone()));
                    }
                    Some(_) => {
                        set.holes
                            .unsolved
                            .push(Unsolved::Residual { def: callee, param: position });
                        return None;
                    }
                    None => {
                        set.holes
                            .unsolved
                            .push(Unsolved::Parameter { def: callee, param: position });
                        return None;
                    }
                },
            }
        }
        Some(Instance { def: callee, args })
    }

    /// An operand's type at the call site, in the caller's instantiation.
    ///
    /// `None` for a literal or a unit: [`science_mir::Constant::Literal`]
    /// carries no type — *"the value a `1` denotes is a fact about its type,
    /// and the type is on the statement beside it"* — and the statement is not
    /// here. §2 cost 2.
    fn actual_ty(
        &mut self,
        caller_subst: &Substitution,
        body: &Body,
        operand: &Operand,
    ) -> Option<Ty> {
        let place = operand.place()?;
        let ty = place.ty(body);
        Some(self.apply(caller_subst, ty))
    }

    /// One-way structural matching: the declared type is the pattern.
    ///
    /// **One-way and not unification.** The actual type is concrete by
    /// construction — it is a type in an already-monomorphised caller — so
    /// there is nothing on that side to bind, and a two-sided unifier here
    /// would be a second inference engine in a crate with no business having
    /// one. A mismatch is *silence*: the checker has already reported whatever
    /// made the two disagree, and a monomorphiser that reported it again would
    /// be the *"second spelling of one mistake, from the level with the worst
    /// span to say it from"* that `science-mir`'s §3 refuses.
    fn match_ty(
        &self,
        declared: Ty,
        actual: Ty,
        unknowns: &HashSet<DefId>,
        solution: &mut Solution,
    ) {
        if declared == Ty::ERROR || actual == Ty::ERROR {
            return;
        }
        if let TyKind::Param { def } = *self.types.kind(declared) {
            if unknowns.contains(&def) {
                solution.bind_type(def, actual);
            }
            return;
        }
        match (self.types.kind(declared), self.types.kind(actual)) {
            (
                TyKind::Named { def: left, args: left_args },
                TyKind::Named { def: right, args: right_args },
            )
            | (
                TyKind::Object { interface: left, args: left_args },
                TyKind::Object { interface: right, args: right_args },
            ) => {
                if left != right || left_args.len() != right_args.len() {
                    return;
                }
                let pairs: Vec<(GenericArg, GenericArg)> =
                    left_args.iter().cloned().zip(right_args.iter().cloned()).collect();
                for (left, right) in pairs {
                    self.match_arg(&left, &right, unknowns, solution);
                }
            }
            (
                TyKind::Borrowed { mutable: left_mut, inner: left },
                TyKind::Borrowed { mutable: right_mut, inner: right },
            ) => {
                if left_mut == right_mut {
                    self.match_ty(*left, *right, unknowns, solution);
                }
            }
            (TyKind::Nullable(left), TyKind::Nullable(right)) => {
                self.match_ty(*left, *right, unknowns, solution);
            }
            (TyKind::Tuple(left), TyKind::Tuple(right)) if left.len() == right.len() => {
                let pairs: Vec<(Ty, Ty)> =
                    left.iter().copied().zip(right.iter().copied()).collect();
                for (left, right) in pairs {
                    self.match_ty(left, right, unknowns, solution);
                }
            }
            (
                TyKind::Closure { params: left, ret: left_ret },
                TyKind::Closure { params: right, ret: right_ret },
            ) if left.len() == right.len() => {
                let pairs: Vec<(Ty, Ty)> =
                    left.iter().copied().zip(right.iter().copied()).collect();
                let (left_ret, right_ret) = (*left_ret, *right_ret);
                for (left, right) in pairs {
                    self.match_ty(left, right, unknowns, solution);
                }
                self.match_ty(left_ret, right_ret, unknowns, solution);
            }
            // A declared `borrowed T` against an owned actual, and the reverse.
            // `check`'s AMENDMENT 2 auto-borrows at call sites, so the two
            // should already agree — and when they do not, the one that peels
            // is the declared one, because `def largest of T(items: borrowed
            // Array of T)` called on an owned array is the ordinary spelling
            // and §6.3 is why.
            (TyKind::Borrowed { inner, .. }, _) => {
                self.match_ty(*inner, actual, unknowns, solution);
            }
            (_, TyKind::Borrowed { inner, .. }) => {
                self.match_ty(declared, *inner, unknowns, solution);
            }
            _ => {}
        }
    }

    fn match_arg(
        &self,
        declared: &GenericArg,
        actual: &GenericArg,
        unknowns: &HashSet<DefId>,
        solution: &mut Solution,
    ) {
        match (declared, actual) {
            (GenericArg::Type(left), GenericArg::Type(right)) => {
                self.match_ty(*left, *right, unknowns, solution);
            }
            (GenericArg::Const(pattern), GenericArg::Const(value)) => {
                self.match_const(pattern, value, unknowns, solution);
            }
            _ => {}
        }
    }

    /// §8 item 7's one-variable linear matching, at its first caller.
    ///
    /// **`match_linear` rather than a second inversion.** `matching.rs` is that
    /// algorithm with its remainder check and its overflow check already
    /// written, and its §4 says the reason the diagnostic was left unbuilt is
    /// that *"§9.4 makes the instantiation chain the whole of the message, and
    /// there is no monomorphiser to walk for a chain yet"*. There is one now,
    /// and [`Holes::unsolved`] is where an indivisible remainder lands —
    /// as data, not as `SC0262`, because that code is `science-types`' and the
    /// obligation it reports is a *shape* obligation this pass does not check.
    fn match_const(
        &self,
        pattern: &NormalForm,
        value: &NormalForm,
        unknowns: &HashSet<DefId>,
        solution: &mut Solution,
    ) {
        let Some(value) = value.as_constant() else { return };
        if pattern.is_constant() {
            // A concrete position: a *check*, and one the checker already made.
            return;
        }
        match match_linear(pattern, value) {
            Ok(solved) if unknowns.contains(&solved.param) => {
                solution.bind_const(solved.param, NormalForm::literal(solved.value));
            }
            Ok(_) => {}
            // `NotAnInferenceSite` with two or more atoms is §7.1's refusal and
            // `Indivisible` is `SC0262`'s condition. Neither is reported here:
            // the first leaves the parameter unsolved and `assemble_instance`
            // says so, and the second is a *shape* obligation whose diagnostic
            // belongs to the crate that owns the code.
            Err(_) => {}
        }
    }

    // --- naming -----------------------------------------------------------

    /// The generic parameters of a definition, block first. See [`Instance`].
    fn generics_of(&self, def: DefId) -> Vec<hir::GenericParam> {
        let Some(signature) = self.decls.signature(def) else { return Vec::new() };
        let mut out: Vec<hir::GenericParam> = signature
            .owner
            .and_then(|owner| self.decls.block_generics(owner))
            .map(<[hir::GenericParam]>::to_vec)
            .unwrap_or_default();
        out.extend(signature.generics.iter().cloned());
        out
    }

    /// The substitution a body is walked under: `Self`, the block's associated
    /// types, and the instance's own arguments.
    fn substitution_of(&self, instance: &Instance) -> Substitution {
        let owner = self.decls.signature(instance.def).and_then(|signature| signature.owner);
        let mut subst = match owner {
            Some(owner) => self.decls.body_substitution(self.defs, owner),
            None => Substitution::new(),
        };
        for (param, arg) in self.generics_of(instance.def).iter().zip(&instance.args) {
            match arg {
                GenericArg::Type(ty) => subst = subst.with_type(param.def, *ty),
                GenericArg::Const(form) => subst = subst.with_const(param.def, form.clone()),
                GenericArg::Error => {}
            }
        }
        subst
    }

    /// `subst.apply`, with the arithmetic failure folded into [`Ty::ERROR`].
    ///
    /// The only way [`Substitution::apply`] fails is `SC0260`'s `i128` range,
    /// through the const half, and that diagnostic is `science-types`'. An
    /// overflow here makes the instance untypeable, which is what
    /// [`Unsolved::Untypeable`] is, so the failure is not lost — it is renamed
    /// to the thing this pass can say about it.
    fn apply(&mut self, subst: &Substitution, ty: Ty) -> Ty {
        subst.apply(self.types, ty).unwrap_or(Ty::ERROR)
    }

    /// The mangled symbol. §4, §5.
    pub fn symbol_of(&self, instance: &Instance) -> Result<String, Unsolved> {
        let path = self.path_of(instance.def);
        let mut encoded = Vec::with_capacity(instance.args.len());
        // The const half goes through `MonoKey`, so the key §8 item 4 specifies
        // is the thing the symbol is built from and not a parallel copy of it.
        let key = instance.key();
        let mut consts = key.args().iter();
        for (position, arg) in instance.args.iter().enumerate() {
            match arg {
                GenericArg::Type(ty) => {
                    let mut out = String::new();
                    self.encode_ty(*ty, &mut out).map_err(|()| Unsolved::Untypeable {
                        def: instance.def,
                        param: position,
                    })?;
                    encoded.push(out);
                }
                GenericArg::Const(_) => {
                    let form = consts.next().expect("one per `GenericArg::Const`");
                    let value = form.as_constant().ok_or(Unsolved::Residual {
                        def: instance.def,
                        param: position,
                    })?;
                    encoded.push(encode_const(value));
                }
                GenericArg::Error => {
                    return Err(Unsolved::Untypeable { def: instance.def, param: position })
                }
            }
        }
        Ok(assemble(&path, &encoded))
    }

    /// A type, encoded for a symbol. §4.
    ///
    /// Nominal by **canonical path** and structural for the shapes that have no
    /// name. `Err(())` is a type no symbol can name: [`Ty::ERROR`], a
    /// [`TyKind::Param`] that survived substitution, or an unresolved `Self`.
    /// Every one of those means the instance is not concrete, so refusing is
    /// the answer and a placeholder would be a symbol that two different
    /// instantiations could share.
    fn encode_ty(&self, ty: Ty, out: &mut String) -> Result<(), ()> {
        match self.types.kind(ty) {
            TyKind::Error | TyKind::Param { .. } | TyKind::SelfType { .. } => Err(()),
            TyKind::SelfAssoc { .. } => Err(()),
            TyKind::Unit => {
                out.push('u');
                Ok(())
            }
            TyKind::Named { def, args } => {
                let (def, args) = (*def, args.clone());
                out.push('N');
                self.encode_path(def, out);
                self.encode_args(&args, out)
            }
            TyKind::Object { interface, args } => {
                let (interface, args) = (*interface, args.clone());
                out.push('D');
                self.encode_path(interface, out);
                self.encode_args(&args, out)
            }
            TyKind::Borrowed { mutable, inner } => {
                let (mutable, inner) = (*mutable, *inner);
                out.push(if mutable { 'm' } else { 'r' });
                self.encode_ty(inner, out)
            }
            TyKind::Nullable(inner) => {
                let inner = *inner;
                out.push('O');
                self.encode_ty(inner, out)
            }
            TyKind::Tuple(elements) => {
                let elements = elements.clone();
                out.push('T');
                out.push_str(&elements.len().to_string());
                out.push('_');
                for element in elements {
                    self.encode_ty(element, out)?;
                }
                Ok(())
            }
            TyKind::Closure { params, ret } => {
                let (params, ret) = (params.clone(), *ret);
                out.push('F');
                out.push_str(&params.len().to_string());
                out.push('_');
                for param in params {
                    self.encode_ty(param, out)?;
                }
                self.encode_ty(ret, out)
            }
        }
    }

    fn encode_args(&self, args: &[GenericArg], out: &mut String) -> Result<(), ()> {
        out.push_str(&args.len().to_string());
        out.push('_');
        for arg in args {
            match arg {
                GenericArg::Type(inner) => self.encode_ty(*inner, out)?,
                GenericArg::Const(form) => {
                    out.push_str(&encode_const(form.as_constant().ok_or(())?))
                }
                GenericArg::Error => return Err(()),
            }
        }
        Ok(())
    }

    fn encode_path(&self, def: DefId, out: &mut String) {
        let path = self.path_of(def).join(".");
        out.push_str(&path.len().to_string());
        out.push_str(&path);
    }

    /// A definition's canonical path, outermost first.
    ///
    /// **Names, never numbers**, for `normal.rs` §2's reason: a [`DefId`] is
    /// allocation order and allocation order is command-line order.
    ///
    /// An implementation block has no name of its own — `hir::Def::name` is
    /// `""` for one — so its component is the **self type's** name. Two blocks
    /// on the same type in the same module would then share a path, so the
    /// second and later get a `#n` suffix in source order. That is the one
    /// number in a symbol, it is an index within one module rather than across
    /// files, and it moves only when the author adds an implementation block
    /// above an existing one — which is the same trade `normal.rs` §2 took when
    /// it made the atom order depend on names.
    fn path_of(&self, def: DefId) -> Vec<String> {
        let mut components = Vec::new();
        let mut current = Some(def);
        while let Some(id) = current {
            let entry = self.defs.get(id);
            match entry.kind {
                // The crate root and the module carry the file, which is a path
                // and therefore a reproducibility hazard — `package-manager.md`
                // Decision 7 removes absolute paths from output and a module
                // name derived from one is the same hazard. F0 has no `mod`
                // declaration, so a module has no name the author wrote.
                DefKind::Module => {}
                DefKind::Impl => components.push(self.impl_component(id)),
                _ => components.push(entry.name.clone()),
            }
            current = entry.parent;
        }
        components.reverse();
        components
    }

    fn impl_component(&self, def: DefId) -> String {
        let name = self
            .decls
            .self_ty(def)
            .and_then(|ty| match self.types.kind(ty) {
                TyKind::Named { def, .. } => Some(self.defs.get(*def).name.clone()),
                _ => None,
            })
            .unwrap_or_else(|| "impl".to_string());
        match self.impl_ordinal.get(&def).copied().unwrap_or(0) {
            0 => name,
            n => format!("{name}#{n}"),
        }
    }

    /// How an instance reads in a diagnostic and in a dump. Not a symbol.
    pub fn describe(&self, instance: &Instance) -> String {
        let path = self.path_of(instance.def).join(".");
        if instance.args.is_empty() {
            return path;
        }
        let args: Vec<String> = instance
            .args
            .iter()
            .map(|arg| match arg {
                GenericArg::Type(ty) => self.types.render(self.defs, *ty),
                GenericArg::Const(form) => form.render(self.defs),
                GenericArg::Error => "?".to_string(),
            })
            .collect();
        format!("{path}[{}]", args.join(", "))
    }
}

/// One entry in the worklist.
struct Task {
    instance: Instance,
    /// The instantiation chain that reached it, outermost first, **not
    /// including** the instance itself. §6.
    chain: Vec<Instance>,
    from: Option<String>,
}

/// What the match recovered.
///
/// Two `HashMap`s, and §5 item 1 is why that is safe: they are read by key and
/// never iterated. The order the instance's arguments come out in is the
/// declaration's, taken from [`Mono::generics_of`].
#[derive(Debug, Default)]
struct Solution {
    types: HashMap<DefId, Ty>,
    consts: HashMap<DefId, NormalForm>,
}

impl Solution {
    /// **First binding wins.** A parameter matched at two positions that
    /// disagree is a call the checker already rejected, and choosing the later
    /// one would mean the arguments a symbol is built from depend on the order
    /// this function happened to visit the parameter list.
    fn bind_type(&mut self, param: DefId, ty: Ty) {
        self.types.entry(param).or_insert(ty);
    }

    fn bind_const(&mut self, param: DefId, form: NormalForm) {
        self.consts.entry(param).or_insert(form);
    }
}

/// Which function each local holds, for the locals that hold one.
///
/// **A call site does not name a function; it names a temporary.** MIR lowers
/// `install(identity)` to `_2 = identity` and then `install(move _2)`, because
/// `identity` is an [`Operand::Const`] and an argument is a place. So a check
/// about *which function crosses a boundary* — §9's — has to look one statement
/// back, and this is that lookup, built once per body.
///
/// **A local assigned twice is dropped from the table rather than resolved to
/// the last write.** Two writes mean the answer depends on the path taken, and
/// a boundary check that guessed would report `SC0522` against one branch of an
/// `if`. Conservative in the direction that misses a diagnostic, which is the
/// direction §9's *"what that does not cover"* already admits to.
fn function_values(body: &Body) -> HashMap<Local, DefId> {
    let mut out: HashMap<Local, Option<DefId>> = HashMap::new();
    for (_, block) in body.blocks() {
        for statement in &block.statements {
            let StatementKind::Assign { place, rvalue } = &statement.kind else { continue };
            if !place.is_local() {
                continue;
            }
            let named = match rvalue {
                Rvalue::Use(Operand::Const(Constant::Item(def))) => Some(*def),
                _ => None,
            };
            out.entry(place.local)
                .and_modify(|slot| {
                    if *slot != named {
                        *slot = None;
                    }
                })
                .or_insert(named);
        }
    }
    out.into_iter().filter_map(|(local, def)| def.map(|def| (local, def))).collect()
}

/// The function an operand names, directly or through a local. See
/// [`function_values`].
fn named_function(operand: &Operand, values: &HashMap<Local, DefId>) -> Option<DefId> {
    match operand {
        Operand::Const(Constant::Item(def)) => Some(*def),
        Operand::Copy(place) | Operand::Move(place) if place.is_local() => {
            values.get(&place.local).copied()
        }
        _ => None,
    }
}

/// Which of its parent's implementation blocks each `impl` is, among those with
/// the same self-type name. See [`Mono::path_of`].
fn impl_ordinals(defs: &DefTable, decls: &Declarations) -> HashMap<DefId, usize> {
    let mut seen: HashMap<(Option<DefId>, String), usize> = HashMap::new();
    let mut out = HashMap::new();
    for entry in defs.iter() {
        if entry.kind != DefKind::Impl {
            continue;
        }
        let name = decls
            .self_ty(entry.id)
            .map(|ty| format!("{ty:?}"))
            .unwrap_or_else(|| "impl".to_string());
        let slot = seen.entry((entry.parent, name)).or_insert(0);
        out.insert(entry.id, *slot);
        *slot += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use science_diagnostics::FileId;

    /// A `DefId` with no meaning beyond being one. `DefId` has no public
    /// constructor, which is right — it is a table index — so a unit test that
    /// needs one mints it from a table.
    fn a_def() -> DefId {
        let mut defs = DefTable::new();
        defs.alloc(DefKind::Fn, "f", Span::new(FileId(0), 0, 1), None)
    }

    #[test]
    fn an_instance_with_no_arguments_has_an_empty_key() {
        let instance = Instance::plain(a_def());
        assert!(instance.key().is_empty());
        assert!(!instance.is_generic());
    }

    #[test]
    fn the_key_is_the_const_half_and_only_the_const_half() {
        // §3: the type-argument half is not `MonoKey`'s, and an instance whose
        // arguments are all types has an empty one. A key that quietly
        // included types would make `f of Int` and `f of Bool` one key.
        let instance = Instance {
            def: a_def(),
            args: vec![GenericArg::Const(NormalForm::literal(3)), GenericArg::Error],
        };
        let key = instance.key();
        assert_eq!(key.len(), 1);
        assert_eq!(key.args()[0].as_constant(), Some(3));
    }

    #[test]
    fn holes_are_empty_only_when_nothing_was_skipped() {
        let mut holes = Holes::default();
        assert!(holes.is_empty());
        holes.closures += 1;
        assert!(!holes.is_empty());
    }

    #[test]
    fn the_default_root_set_is_the_entry_point() {
        // §7. A default that emitted every body would put dead code in every
        // binary, and it is the kind of default nobody revisits.
        assert_eq!(RootSet::default(), RootSet::EntryPoint);
    }
}
