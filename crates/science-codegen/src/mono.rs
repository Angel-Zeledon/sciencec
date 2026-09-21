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
//!    **`let a be identity(1)` used to be unsolved and now is not.** The note
//!    that stood here called it *"a defect rather than a design limit"* and
//!    said it was `science-types`' — it was, and `check`'s `instantiate_call`
//!    has since been given the third state it needed: a parameter whose only
//!    evidence is an argument still carrying an unresolved literal class is
//!    **deferred** to that argument's own inference variable rather than
//!    forced to `Ty::ERROR`, so Decision 2's default settles the call and the
//!    binding together. Nothing in this module changed; the hole it was
//!    reading is filled.
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
//! names hash-map iteration order as the unknown source behind Gate J. Three
//! things carry that here, and the third is the one that was nearly missed:
//!
//! 1. **The output is a [`BTreeMap`] keyed by the mangled symbol.**
//!    [`MonoSet::emission_order`] is its iterator, and no `HashMap`'s iteration
//!    order reaches it. There are three `HashMap`s in this module — [`Solution`]'s
//!    two and [`function_values`]'s — and two of them are read by key and never
//!    iterated. The third is iterated exactly once, at the end of
//!    [`function_values`], to build **another `HashMap`**, so the order it is
//!    walked in changes nothing about what comes out. That sentence is the kind
//!    of claim that goes stale, which is why `tests/mono.rs` runs the whole
//!    pipeline twice and compares bytes instead of believing it.
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
use science_types::{Form, Found, MonoKey, NormalForm, Substitution};

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
    /// The concrete type `Self` is bound to, for an instance of a method
    /// `interface` declares. `None` for every other definition — which
    /// includes a method on an `implements`/`has` block, where `Self` is
    /// already the concrete self type the block was written for and no
    /// second binding is needed.
    ///
    /// **Why this is a field beside `args` and not one more [`GenericArg`] in
    /// it.** `args` is zipped against [`Mono::generics_of`], which lists a
    /// definition's *declared* parameters — the block's and the method's own
    /// — and `Self` is not one of those: `interface Summarize:`'s `preview`
    /// declares zero parameters and is still a different function for every
    /// implementor, because its `self` is `Self` and Self is not a
    /// declaration anywhere a `hir::GenericParam` could be minted for it.
    /// Folding it into `args` would mean inventing one, and every consumer of
    /// `args` — the zip in [`Mono::generics_of`]'s caller,
    /// [`Substitution::of_generics`] — would have to special-case position
    /// zero for exactly one kind of definition. A field beside `args` is a
    /// second axis of one instance's identity, named for what it is, and
    /// [`Mono::symbol_of`] and [`Mono::describe`] both read it beside `args`
    /// rather than inside it for the same reason.
    pub self_ty: Option<Ty>,
}

impl Instance {
    /// An instance with no generic arguments and no `Self` to bind.
    pub fn plain(def: DefId) -> Instance {
        Instance { def, args: Vec::new(), self_ty: None }
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

/// One instance, with the body a backend emits for it.
///
/// **The symbol is the identity and the [`DefId`] is not**, which is the whole
/// of what monomorphisation changes for a backend: `identity[Int]` and
/// `identity[F64]` share a [`Body::def`] and must not share a function. Every
/// map a backend keys by definition has to be re-keyed by this `symbol`, and
/// [`MonoSet::callee_at`] is how a call site gets from its block to the symbol
/// it calls.
#[derive(Debug, Clone)]
pub struct MonoBody {
    /// The mangled symbol — the same string as the [`MonoItem`] this came from.
    pub symbol: String,
    /// The definition and the arguments it was reached with.
    pub instance: Instance,
    /// The body, with every type substituted. It still carries the *generic*
    /// definition's [`Body::def`], because that is what it is a body of; the
    /// `symbol` is what distinguishes this copy from the others.
    pub body: Body,
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
    /// Which instance each call site calls, keyed by the **calling instance's**
    /// symbol and the block the call terminates.
    ///
    /// **This is the half of monomorphisation a set of items cannot carry.** A
    /// [`science_mir::Callee::Def`] names a definition, and after this walk one
    /// definition is several functions; `identity[Int]` and `identity[F64]`
    /// are two symbols behind one [`DefId`], so a backend holding only the item
    /// set can tell that both exist and not which of them a given call wants.
    ///
    /// **Recorded here rather than recovered there**, because the answer is
    /// [`Mono::solve_call`]'s and it is already computed: the walk has to solve
    /// every call's instantiation to know what to enqueue. A backend that
    /// re-derived it would be a second copy of the unification, in a crate that
    /// cannot see [`Substitution`], discovering a disagreement at link time —
    /// which is the hazard this codebase closes everywhere else by having one
    /// spelling of a rule.
    ///
    /// Keyed on the *caller's symbol* and not its [`DefId`] for the reason the
    /// map exists at all: the same call site in a generic body calls a
    /// different instance in each instantiation of that body. `f[T]` calling
    /// `g[T]` calls `g[Int]` from `f[Int]` and `g[F64]` from `f[F64]`, out of
    /// one block of one MIR body.
    ///
    /// A [`BTreeMap`] for Decision 4's reason, one level down from
    /// [`MonoSet::emission_order`]: the iteration order of this map reaches the
    /// image through nothing today, and `self-hosting.md` §15 names hash-map
    /// order as the unknown behind Gate J, so it is closed by construction
    /// before something depends on it.
    calls: BTreeMap<(String, u32), String>,
    /// A generic record's fields, or a generic `choice`'s payloads, with the
    /// aggregate's parameters bound to this use's arguments and every member
    /// substituted to a concrete [`Ty`] — interned here, above Decision 42's
    /// line, and read back by the backend instead of applied by it.
    ///
    /// **The gap this closes.** `science-codegen-llvm`'s `Lowerer` computes a
    /// generic aggregate's layout by walking its **declared** field list with
    /// its parameters bound at each use, because substituting a field type
    /// would intern a `Ty` and that crate is handed a `&Types` on purpose —
    /// `BuildInput`'s own documentation says a backend must not be able to
    /// invent a type. That walk is a lookup when a member's declared type
    /// *is* a parameter and a refusal when it is a **compound** mentioning
    /// one — `Holder[A]`'s field of type `Array[A]`, or a field whose own
    /// type is `Inner[Array[A]]` — because building `Array[Int]` to bind into
    /// the walk's own environment is exactly the interning `&Types` forbids.
    ///
    /// This map is the fix, spent where it is affordable: [`Mono`] already
    /// holds `&mut Types` to substitute a *body*, and every generic aggregate
    /// a monomorphised body's locals mention is discovered and substituted
    /// here the same way, by [`Substitution::apply`] on each declared member
    /// — the same function `Mono::instantiate` already calls on a body, run
    /// over a field list instead. A member that is itself a new generic
    /// aggregate (`Holder[Int]`, discovered inside `Nest[Int]`'s own
    /// substituted fields) is walked in turn, so the table is closed under
    /// nesting rather than one level deep.
    ///
    /// **Keyed by `(DefId, Vec<GenericArg>)` and not by `Ty`.** The concrete
    /// use this is computed for is the pair a backend already has in hand at
    /// [`TyKind::Named`](science_types::TyKind::Named)'s `def` and `args`,
    /// so the lookup costs it nothing it was not already holding; keying by
    /// the interned `Ty` of the whole aggregate would work too, but would
    /// make this table one more place a `Ty`'s allocation order could leak
    /// out, which is the hazard `normal.rs` §2 is about.
    ///
    /// **A [`HashMap`] and not a [`BTreeMap`], and that is not Decision 4's
    /// hazard.** Every other `HashMap` this module was warned off of is one
    /// whose iteration order could reach emitted output; this one is never
    /// iterated; it is only ever looked up by a key the caller already holds,
    /// the same way [`MonoSet::get`] looks up `items` by symbol. Decision 4
    /// is about the *order things are emitted in*, and nothing here is
    /// emitted at all.
    aggregate_fields: HashMap<(DefId, Vec<GenericArg>), AggregateLayout>,
    holes: Holes,
    diagnostics: Vec<Diagnostic>,
}

/// One generic aggregate's members, substituted at one use. §10.
///
/// A record's fields are one list, in declaration order; a `choice`'s
/// payloads are one list **per variant**, in the `choice`'s own declaration
/// order, because `science-codegen-llvm`'s `Lowerer::emit_choice_glue` needs
/// to know which variant each payload belongs to and a flat list would have
/// thrown that away.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AggregateLayout {
    /// A record's fields, in declaration order.
    Record(Vec<Ty>),
    /// A `choice`'s payloads, one list per variant, in declaration order.
    Choice(Vec<Vec<Ty>>),
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

    /// The instance the call terminating `block` of `caller` calls.
    ///
    /// `None` for a call the walk did not follow — a runtime entry point, an
    /// `extern` function, an unresolved method, an instantiation it could not
    /// make concrete — each of which [`MonoSet::holes`] has already counted.
    /// A backend meeting one falls back to whatever it did before there was a
    /// map, which for every one of those is the right answer: none of them is
    /// an instance.
    pub fn callee_at(&self, caller: &str, block: science_mir::mir::BlockId) -> Option<&str> {
        self.calls.get(&(caller.to_string(), block.index() as u32)).map(String::as_str)
    }

    /// Every call the walk followed, as `(caller symbol, block, callee
    /// symbol)`. For a dump and for a test that wants the whole map.
    pub fn call_sites(&self) -> impl Iterator<Item = (&str, u32, &str)> {
        self.calls.iter().map(|((caller, block), callee)| (caller.as_str(), *block, callee.as_str()))
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

    /// A generic record's fields, or a `choice`'s payloads, substituted at
    /// `args` — `None` when this instantiation is not one [`Mono::instantiate`]
    /// discovered a monomorphised body use, in which case the caller falls
    /// back to whatever it did before this table existed.
    pub fn aggregate_fields(&self, def: DefId, args: &[GenericArg]) -> Option<&AggregateLayout> {
        self.aggregate_fields.get(&(def, args.to_vec()))
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

/// The entry point of one crate: a module-level `def main` with a body.
///
/// **The decision: the rule lives here, as a free function, and
/// [`Mono::entry_point`] calls it.** It used to be a method and nothing but
/// the walk could ask the question, so the one caller that needed the answer
/// *before* the walk — `sciencec`'s `build`, deciding whether the file it was
/// handed is a program at all — could not have it, and the refusal for a file
/// with no `main` came out of the LLVM backend as `SC0400` instead of the
/// `SC0403` §11 defines for exactly that case.
///
/// **The reason it is not copied into the driver instead.** §7's own note on
/// `RootSet::EveryBody` is about what happens when a visibility rule exists in
/// two places at once, and *"which function is the program"* is a rule of the
/// same kind: a second copy in `sciencec` would be a second answer the day
/// `script-mode.md`'s `tool` form becomes an entry point in its own right.
/// One function, two callers.
///
/// **The lowest [`DefId`] when there are several**, which is the one
/// command-line order picked, and that is the correct dependence rather than
/// the hazard `normal.rs` §2 is about: *which file is the program* is
/// genuinely a fact about the invocation, and `script-mode.md`'s driver rule
/// is already *"the first module is the one the user named"*. Nothing derived
/// from this reaches a symbol.
///
/// **The cost.** The caller supplies the set of defs that have bodies, because
/// a `main` without one is a declaration and not a program. `script-mode.md`'s
/// synthesised script `main` is in that set like any other, which is what
/// makes a file with top-level statements a binary.
pub fn entry_point(defs: &DefTable, bodies: impl IntoIterator<Item = DefId>) -> Option<DefId> {
    let mut found: Option<DefId> = None;
    for def in bodies {
        let entry = defs.get(def);
        if entry.kind != DefKind::Fn || entry.name != "main" {
            continue;
        }
        let module = entry.parent.map(|parent| defs.get(parent).kind);
        if module != Some(DefKind::Module) {
            continue;
        }
        if found.is_none_or(|current| def.index() < current.index()) {
            found = Some(def);
        }
    }
    found
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
    /// The module the entry point is in, whose name [`Mono::path_of`] leaves
    /// out. `None` for a crate with no entry — a library build, where every
    /// module is named by a `use` somewhere and none is privileged.
    entry_module: Option<DefId>,
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
        // Computed once, because `path_of` is called per definition and
        // per component and this is a walk up the definition tree.
        let entry_module = entry_point(defs, index.keys().copied()).and_then(|entry| {
            let mut cursor = defs.get(entry).parent;
            while let Some(id) = cursor {
                if defs.get(id).kind == DefKind::Module {
                    return Some(id);
                }
                cursor = defs.get(id).parent;
            }
            None
        });
        Mono { defs, decls, types, bodies: index, entry_module }
    }

    /// Every item in `set` that this crate has a body for, as a **concrete**
    /// body: `science_mir::instantiate` applied with the instance's own
    /// substitution.
    ///
    /// # Why it is a method here and not a step in the driver
    ///
    /// Substituting a type interns types that did not exist before —
    /// `identity[Int]`'s return is a `Ty` nothing had built while the body was
    /// still generic — so this needs [`Types`] mutably, and a [`Mono`] is
    /// holding it. The driver cannot take it back until the walk is dropped,
    /// and by then [`Mono::substitution_of`] is gone with it. Splitting the two
    /// would mean a second copy of the substitution rule outside the only type
    /// that knows the argument order [`Instance`] documents.
    ///
    /// # What is left out, and why each is right
    ///
    /// An item with `defined_here == false` is an `extern` declaration or a
    /// prelude signature: there is no body to instantiate and the backend
    /// emits a declaration. An item whose substitution does not evaluate is
    /// dropped with its [`ConstEvalError`] turned into an [`Unsolved`] on the
    /// set's holes, for `ty`'s §5 reason the rest of this file follows — an
    /// instance that vanished silently is a link error with no author.
    pub fn instantiate(&mut self, set: &mut MonoSet) -> Vec<MonoBody> {
        let mut out = Vec::with_capacity(set.len());
        let items: Vec<(String, Instance)> = set
            .emission_order()
            .filter(|item| item.defined_here)
            .map(|item| (item.symbol.clone(), item.instance.clone()))
            .collect();
        for (symbol, instance) in items {
            let Some(body) = self.bodies.get(&instance.def).copied() else { continue };
            let subst = self.substitution_of(&instance);
            match science_mir::instantiate(body, &subst, self.types) {
                Ok(body) => out.push(MonoBody { symbol, instance, body }),
                Err(_) => set.holes.unsolved.push(Unsolved::Residual {
                    def: instance.def,
                    param: 0,
                }),
            }
        }
        self.intern_aggregate_fields(&out, set);
        out
    }

    /// [`MonoSet::aggregate_fields`]'s table, built from every generic
    /// record or `choice` `out`'s bodies mention.
    ///
    /// # The walk
    ///
    /// The roots are every distinct `Named { def, args }` reachable from a
    /// local's declared type in any concrete body `out` carries — concrete
    /// because `out` is `instantiate`'s own output, built one statement
    /// above this call. From each root this substitutes the aggregate's
    /// declared members at `args` with [`Substitution::apply`], the same
    /// function that substitutes a body, and then walks the **result** for
    /// more roots: a member that is itself a generic aggregate — `Holder[A]`
    /// inside `Nest[A]`, substituted to `Holder[Int]` — is not necessarily a
    /// local's type anywhere in the program, so it would never be found if
    /// the walk stopped at what a local declares. `seen` closes the walk
    /// under this nesting and stops it from repeating an aggregate already
    /// entered.
    ///
    /// # What a failed substitution means
    ///
    /// [`Substitution::apply`] fails only on the const half's arithmetic
    /// range (`subst.rs` §4), which a type-only aggregate like `Pair[A, B]`
    /// never exercises — but a const generic aggregate can, and when it does
    /// the member is recorded as [`Ty::ERROR`] rather than dropped, so the
    /// list stays the length the declaration has and the backend's own
    /// `TyKind::Error` refusal is what a reader sees, with [`Unsolved::Residual`]
    /// counted on `set.holes` for `ty`'s §5 reason: a hole is a thing a
    /// consumer can count, and a member that silently vanished would not be.
    fn intern_aggregate_fields(&mut self, out: &[MonoBody], set: &mut MonoSet) {
        let mut seen: HashSet<(DefId, Vec<GenericArg>)> = HashSet::new();
        let mut worklist: Vec<(DefId, Vec<GenericArg>)> = Vec::new();
        let mut in_ty: HashSet<Ty> = HashSet::new();
        for body in out {
            for (_, local) in body.body.locals() {
                collect_generic_aggregates(self.defs, self.types, local.ty, &mut in_ty, &mut worklist);
            }
        }
        while let Some((def, args)) = worklist.pop() {
            if !seen.insert((def, args.clone())) {
                continue;
            }
            let layout = match self.defs.get(def).kind {
                DefKind::Record => {
                    let Some(record) = self.decls.record(def) else { continue };
                    let subst = Substitution::of_generics(&record.generics, &args);
                    let mut fields = Vec::with_capacity(record.fields.len());
                    for (index, (_, field_ty)) in record.fields.iter().enumerate() {
                        let substituted = match subst.apply(self.types, *field_ty) {
                            Ok(ty) => ty,
                            Err(_) => {
                                set.holes.unsolved.push(Unsolved::Residual { def, param: index });
                                Ty::ERROR
                            }
                        };
                        collect_generic_aggregates(
                            self.defs,
                            self.types,
                            substituted,
                            &mut in_ty,
                            &mut worklist,
                        );
                        fields.push(substituted);
                    }
                    AggregateLayout::Record(fields)
                }
                DefKind::Choice => {
                    let variants: Vec<DefId> = self
                        .defs
                        .children(def)
                        .filter(|child| child.kind == DefKind::Variant)
                        .map(|child| child.id)
                        .collect();
                    let generics = variants
                        .iter()
                        .find_map(|variant| self.decls.variant(*variant).map(|v| v.generics.clone()))
                        .unwrap_or_default();
                    let subst = Substitution::of_generics(&generics, &args);
                    let mut payloads = Vec::with_capacity(variants.len());
                    for variant in &variants {
                        let Some(declared) = self.decls.variant(*variant) else {
                            payloads.push(Vec::new());
                            continue;
                        };
                        let mut variant_fields = Vec::with_capacity(declared.payload.len());
                        for (index, field_ty) in declared.payload.iter().enumerate() {
                            let substituted = match subst.apply(self.types, *field_ty) {
                                Ok(ty) => ty,
                                Err(_) => {
                                    set.holes.unsolved.push(Unsolved::Residual { def, param: index });
                                    Ty::ERROR
                                }
                            };
                            collect_generic_aggregates(
                                self.defs,
                                self.types,
                                substituted,
                                &mut in_ty,
                                &mut worklist,
                            );
                            variant_fields.push(substituted);
                        }
                        payloads.push(variant_fields);
                    }
                    AggregateLayout::Choice(payloads)
                }
                _ => continue,
            };
            set.aggregate_fields.insert((def, args), layout);
        }
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
                    // **`DefTable::path_of` and not `describe`**, and only
                    // here. A description deliberately leaves the module out,
                    // because it is compared byte-for-byte by
                    // `the_file_id_does_not_reach_a_symbol` to prove the
                    // file's stem reaches neither a symbol nor a dump. That
                    // makes it exactly the wrong thing to print in *this*
                    // message, where the two sides differ only by the module
                    // they are in: it read "one is `value`, the other is
                    // `value`". The full path is a diagnostic string, it
                    // reaches no symbol, and it is the only part of the
                    // message a reader can act on.
                    set.diagnostics.push(symbol_collision(
                        &symbol,
                        &self.defs.path_of(existing.instance.def),
                        &self.defs.path_of(task.instance.def),
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
                    if !self.generics_of(def).is_empty() {
                        // A generic body cannot be a root: there is nothing to
                        // instantiate it *at*. It is emitted once some caller
                        // reaches it, and if nothing does it is dead code that
                        // a library build of a language with no exported
                        // generics has no way to hand out.
                        set.holes.unsolved.push(Unsolved::Parameter { def, param: 0 });
                        continue;
                    }
                    if self.needs_self_ty(def) {
                        // The same argument, one axis over: a method
                        // `interface` declares has nothing to instantiate its
                        // `Self` at until some caller supplies an implementor,
                        // and a root set has no implementor to invent one
                        // from. `Instance::self_ty` is exactly the parameter
                        // [`hir::GenericParam`] cannot name, so it is counted
                        // the same way rather than silently rooted at
                        // whatever `Self` happened to mean when the interface
                        // was checked.
                        set.holes.unsolved.push(Unsolved::Parameter { def, param: 0 });
                        continue;
                    }
                    out.push(Instance::plain(def));
                }
                out
            }
        }
    }

    /// The entry point: a module-level `def main`. The walk's view of the free
    /// [`entry_point`], which carries the rule and the argument for it.
    pub fn entry_point(&self) -> Option<DefId> {
        entry_point(self.defs, self.bodies.keys().copied())
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

        for (block_id, block) in body.blocks() {
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
                    // **The map is written here and not at the pop**, because
                    // this is the only point that knows *which call site* the
                    // instance came from; by the time the task is dequeued the
                    // block is gone and only the caller's symbol survives.
                    //
                    // A symbol that will not assemble is left out rather than
                    // guessed at. `symbol_of`'s [`Unsolved`] is the same one
                    // `assemble_instance` has already pushed onto
                    // `holes.unsolved` for this instance, so the count is not
                    // doubled and the backend's fallback arm is reached with
                    // the refusal it would have produced anyway.
                    if let Ok(callee_symbol) = self.symbol_of(&instance) {
                        set.calls
                            .insert((symbol.to_string(), block_id.index() as u32), callee_symbol);
                    }
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
    ///
    /// **`Self`-dispatch happens first, and everything after it is unchanged.**
    /// [`Mono::redirect_self_call`] turns a call to a method `interface`
    /// declares into the definition the receiver's *concrete* type actually
    /// answers it with — an implementor's override, or the same interface
    /// method again when it is inherited unwritten — and returns the `Self`
    /// this instance is bound at alongside it. The rest of this function reads
    /// declarations and matches them against arguments exactly as it always
    /// did, now for whichever definition that was.
    fn solve_call(
        &mut self,
        caller_subst: &Substitution,
        body: &Body,
        callee: DefId,
        args: &[Operand],
        destination: &Place,
        set: &mut MonoSet,
    ) -> Option<Instance> {
        let (callee, self_ty) = self.redirect_self_call(caller_subst, body, callee, args);

        let generics = self.generics_of(callee);
        if generics.is_empty() {
            if self.decls.signature(callee).is_none() {
                set.holes.undeclared_callees += 1;
            }
            return Some(Instance { def: callee, args: Vec::new(), self_ty });
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

        let instance = self.assemble_instance(callee, &generics, &solution, set)?;
        Some(Instance { self_ty, ..instance })
    }

    /// A call to a method `interface` declares, resolved at the receiver's
    /// concrete type: `(the real definition, the `Self` it is bound at)`.
    ///
    /// **`(callee, None)` unchanged is the answer for almost every call**:
    /// `callee`'s owner is not an interface block, or it is one but has no
    /// receiver (an associated function takes no `Self` to resolve), or the
    /// receiver's type could not be read concretely at this call site. Every
    /// one of those is the ordinary path this function did not used to exist
    /// to change.
    ///
    /// **Why a lookup is run again here, at all.** `interface Summarize:`'s
    /// `preview` is checked exactly once, with `self`'s type
    /// `TyKind::SelfType` — *"the block is the owner and there is no concrete
    /// type to substitute"*, `items.rs`'s own words for why. A call inside
    /// that body to `self.summarize()` is therefore resolved once, against
    /// the *interface's own* method index, and the `DefId` MIR records for it
    /// is fixed at that resolution forever, whichever implementor eventually
    /// calls `preview`. That is correct for the checker's job and wrong for a
    /// backend's: `Doc.preview()` and `Row.preview()` must call `Doc.summarize`
    /// and `Row.summarize` respectively, and nothing between the checker and
    /// here ever asks that question again. This is the one place a caller's
    /// concrete `Self` is in hand — [`Mono::actual_ty`] reads it straight off
    /// the operand, through `caller_subst`, which already carries the
    /// enclosing instance's own `self_ty` binding — so this is where the
    /// question gets asked a second time, at a receiver the checker did not
    /// have.
    ///
    /// **`Declarations::methods()` is asked rather than a private index**,
    /// because it is the same rule `BodyChecker::method_call` used to answer
    /// this exact question for every *other* receiver, and a second lookup
    /// built here would be the hazard this codebase names everywhere: wrong
    /// the day the two disagree about who wins an override.
    ///
    /// **`Found::Ambiguous` is narrowed to `owner`'s interface before it is
    /// given up on.** Two different interfaces contributing a method of the
    /// same name to one implementor is not an ambiguity `Doc` itself has to
    /// answer for *this* call: the checker resolved `self.summarize()`
    /// through `Summarize`'s own index, never through `Doc`'s, so it never
    /// had to be told which of the two `Doc` meant. Re-running the lookup
    /// through `Doc`'s index can surface an ambiguity the original call site
    /// does not have, and filtering to the candidate whose
    /// [`Candidate::interface`](science_types::Candidate::interface) is the
    /// one this call already came through recovers exactly the answer the
    /// checker would have given if asked.
    fn redirect_self_call(
        &mut self,
        caller_subst: &Substitution,
        body: &Body,
        callee: DefId,
        args: &[Operand],
    ) -> (DefId, Option<Ty>) {
        let none = (callee, None);
        let Some(signature) = self.decls.signature(callee) else { return none };
        let Some(owner) = signature.owner else { return none };
        if signature.self_param.is_none() || !self.owner_is_interface(owner) {
            return none;
        }
        let Some(receiver) = args.first() else { return none };
        let Some(actual_self) = self.actual_ty(caller_subst, body, receiver) else { return none };
        let actual_self = self.peel_borrow(actual_self);
        // **Decision 13's vtables are refused here, not answered wrong.**
        // `Methods::receiver` answers a *definition* for `any Summarize`
        // too — the interface's own, which is exactly right for the checker
        // asking "what can I call on this value" and exactly wrong for this
        // question, which is "which implementor is this": a receiver still at
        // `TyKind::Object` or still at `TyKind::SelfType` (an enclosing
        // instance with no `self_ty` of its own — a caller this walk has not
        // bound one for) has no single implementor to redirect to, and
        // treating the interface as if it were one produces an instance whose
        // own `self.summarize()` cannot be redirected either, one level
        // further in: a specialisation at nothing, rather than at `Doc`. This
        // crate does not model a vtable's slots — `lower_crate`'s own comment
        // says the rule lives one crate down — so this is where that gap
        // surfaces rather than where it gets papered over: `none` here leaves
        // `callee` at the interface's own default body with no `self_ty`,
        // which is exactly the refusal the backend already gives a default
        // body it cannot specialise.
        if !self.is_concrete_implementor(actual_self) {
            return none;
        }
        let Some(head) = self.decls.methods().receiver(self.defs, self.types, actual_self) else {
            return none;
        };
        let name = self.defs.get(callee).name.clone();
        let candidate = match self.decls.methods().lookup(head, &name, Form::Value) {
            Found::One(candidate) => candidate,
            Found::Ambiguous(candidates) => {
                let mut narrowed =
                    candidates.into_iter().filter(|candidate| candidate.interface() == Some(owner));
                match (narrowed.next(), narrowed.next()) {
                    (Some(candidate), None) => candidate,
                    _ => return none,
                }
            }
            // Mismatched or absent: the checker already answered this name on
            // this receiver once, through a different index, and a different
            // answer here is this pass looking at a type the checker never
            // saw rather than a program with a new mistake. Left as `none`,
            // exactly as an unrecoverable ordinary argument is left in §2.
            Found::Instances(_) | Found::Mismatched | Found::None => return none,
        };
        let self_ty = self.owner_is_interface(candidate.owner).then_some(actual_self);
        (candidate.method, self_ty)
    }

    /// Whether `owner`'s `Self` is itself — `Declarations::self_ty(owner)` is
    /// a [`TyKind::SelfType`] rather than a concrete type. True for an
    /// `interface` block and false for an `implements`/`has` block, which is
    /// `items.rs`'s own distinction and the one [`Mono::redirect_self_call`]
    /// and [`Mono::substitution_of`] both read it for.
    fn owner_is_interface(&self, owner: DefId) -> bool {
        match self.decls.self_ty(owner) {
            Some(ty) => matches!(self.types.kind(ty), TyKind::SelfType { .. }),
            None => false,
        }
    }

    /// Whether `def` is a method whose `Self` a root cannot supply: one
    /// `interface` declares, taking a receiver. [`Mono::roots`]'s
    /// `RootSet::EveryBody` is this function's one caller.
    fn needs_self_ty(&self, def: DefId) -> bool {
        let Some(signature) = self.decls.signature(def) else { return false };
        signature.self_param.is_some()
            && signature.owner.is_some_and(|owner| self.owner_is_interface(owner))
    }

    /// The type under every `Borrowed` layer. `Declarations::self_ty` is
    /// always bare — an implementation block is written `Doc implements I:`
    /// and not `&Doc implements I:` — so a `Self` this pass binds has to be
    /// bare too, or [`Mono::substitution_of`]'s `with_self` would be binding
    /// `Self` to a type no ordinary implementation block's `self_ty` is ever
    /// found to be.
    fn peel_borrow(&self, mut ty: Ty) -> Ty {
        while let TyKind::Borrowed { inner, .. } = *self.types.kind(ty) {
            ty = inner;
        }
        ty
    }

    /// Whether `ty` — already borrow-peeled — names one implementor rather
    /// than the interface itself.
    ///
    /// **`TyKind::Object` and `TyKind::SelfType` are the two shapes
    /// `Methods::receiver` answers *a* definition for that are not this
    /// one.** Both name the interface, not an implementation of it — an
    /// `any I` value could be any implementor at run time, which is Decision
    /// 13's vtable and not a fact this pass can read off a `Ty`, and a
    /// `SelfType` still standing means the enclosing instance's own `Self`
    /// was never bound, so there is nothing concrete to hand down either. A
    /// `TyKind::Named` at an interface's own `def` — `Error?`'s shorthand is
    /// one — is the same fact spelled the other way and refused for it.
    fn is_concrete_implementor(&self, ty: Ty) -> bool {
        match self.types.kind(ty) {
            TyKind::SelfType { .. } | TyKind::Object { .. } => false,
            TyKind::Named { def, .. } => self.defs.get(*def).kind != DefKind::Interface,
            // Every other shape `Methods::receiver` already answers `None`
            // for through `head`, so there is nothing here for this check to
            // exclude.
            _ => true,
        }
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
        // `self_ty` is `solve_call`'s to fill in, from `redirect_self_call`'s
        // answer rather than this function's: `assemble_instance` solves the
        // method's *own* declared generics and knows nothing about the axis
        // beside them.
        Some(Instance { def: callee, args, self_ty: None })
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
    ///
    /// **`instance.self_ty` overrides `body_substitution`'s own answer for
    /// `Self`, and that is the whole of what makes a default body's `Self`
    /// concrete.** `Declarations::body_substitution`'s own documentation
    /// records what an interface block's `self_ty` is: *"`Self` inside an
    /// interface is itself: the block is the owner and there is no concrete
    /// type to substitute"* — an identity substitution, made when the body was
    /// checked once for every implementor at once. [`Instance::self_ty`] is
    /// the implementor this *instance* was reached through, and `with_self`
    /// replaces the identity with it — `Substitution`'s own `self_ty` field is
    /// one `(DefId, Ty)` pair, not a map, so the second `with_self` call
    /// overwrites rather than adds. For every other instance — `self_ty` is
    /// `None` — `body_substitution`'s own answer stands, because an
    /// implementation block's `Self` was already concrete and there is
    /// nothing here to override.
    pub fn substitution_of(&self, instance: &Instance) -> Substitution {
        let owner = self.decls.signature(instance.def).and_then(|signature| signature.owner);
        let mut subst = match owner {
            Some(owner) => self.decls.body_substitution(self.defs, owner),
            None => Substitution::new(),
        };
        if let (Some(owner), Some(self_ty)) = (owner, instance.self_ty) {
            subst = subst.with_self(owner, self_ty);
        }
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
    ///
    /// **`self_ty`, when there is one, is encoded first — before every
    /// declared argument.** It is read the same way an implementation
    /// block's own generics already are: [`Instance`]'s own documentation puts
    /// the block's arguments ahead of the method's for the reason a reader
    /// counts `of` lists left to right, and `Self` is one block further out
    /// than that — the thing the method is even *reached through* — so it
    /// goes first of all. Two implementors of one interface calling the same
    /// default body would otherwise share every encoded argument the method
    /// itself declares (`preview` declares none) and collide on one symbol,
    /// which is the failure this axis exists to rule out.
    pub fn symbol_of(&self, instance: &Instance) -> Result<String, Unsolved> {
        let path = self.path_of(instance.def);
        let mut encoded = Vec::with_capacity(instance.args.len() + 1);
        if let Some(self_ty) = instance.self_ty {
            let mut out = String::new();
            self.encode_ty(self_ty, &mut out)
                .map_err(|()| Unsolved::Untypeable { def: instance.def, param: 0 })?;
            encoded.push(out);
        }
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
    /// **An implementation block used to have no name of its own**, and this
    /// function used to carry a special case for it — reading
    /// `Declarations::self_ty` and appending an ordinal this module computed
    /// itself, in `impl_component`/`impl_ordinal`/`impl_ordinals`, so that two
    /// blocks on the same type in the same module did not share a path. That
    /// was two copies of one rule: `science-resolve`'s `DefTable::alloc_impl`
    /// writes the identical name — the self type's, with a `#n` for the second
    /// and later block on that type in that module — onto `hir::Def::name`
    /// itself, which is finding 25's fix rather than this crate's. Once the
    /// name is on the `Def`, the special case is dead code with a second
    /// spelling of a rule the resolver already enforces, which is exactly the
    /// hazard this codebase warns about everywhere else, so it is deleted
    /// here in favour of the one line below that reads any other kind's name:
    /// an `impl`'s `entry.name` already **is** `Doc` or `Doc#1`.
    fn path_of(&self, def: DefId) -> Vec<String> {
        let mut components = Vec::new();
        let mut current = Some(def);
        while let Some(id) = current {
            let entry = self.defs.get(id);
            match entry.kind {
                // **The entry module contributes nothing; every other
                // module contributes its name.**
                //
                // The decision resolves a conflict that looked unresolvable
                // and was not, because it put two different things under one
                // word. Both of these are true:
                //
                // - A symbol must not depend on a **file's name**. Decision
                //   16 says *"deterministic from the source alone"*, and
                //   `tests/mono.rs`'s `the_file_id_does_not_reach_a_symbol`
                //   compiles one source as `a.science` and as `z.science`
                //   and demands the same symbols.
                // - Two same-named definitions in different modules of one
                //   crate must be distinguishable, or one silently replaces
                //   the other in a map keyed by the symbol and both call
                //   sites reach whichever survived. That was a program that
                //   ran, exited 0 and printed the wrong number.
                //
                // They conflict only while "a module's name" means one
                // thing. It means two. The **entry** module is named after
                // the file on the command line: the author did not write
                // that name anywhere, renaming the file changes nothing
                // else, and it is exactly what Decision 16 refuses to let
                // reach a symbol. Every **other** module is named by the
                // `use` that reaches it — `use deep.inner` is source the
                // author wrote, and renaming that file without editing the
                // `use` does not compile. So the first is excluded and the
                // second is included, and both requirements hold.
                //
                // The cost is that a symbol now depends on which file is the
                // entry, so compiling `helper.science` directly gives its
                // definitions different symbols from compiling a
                // `main.science` that imports it. That is the same fact as a
                // crate having an entry at all, and
                // `science-codegen-llvm`'s `path_components` follows the
                // identical rule so the two manglings cannot drift.
                DefKind::Module if Some(id) == self.entry_module => {}
                DefKind::Module if entry.name.is_empty() => {}
                // **The prelude's module contributes nothing either, by the
                // same test.** The rule above admits a module because the
                // author named it in a `use`; nobody writes `use core`. It
                // is synthesised by `science-resolve`'s `builtins.rs`, which
                // is what `is_builtin` asks.
                //
                // Left in, every type argument of every generic symbol grew
                // a `core.` — `identity[F64]` mangled `EN8core.F640_` rather
                // than `EN3F640_`, putting a `.` *inside* a length-prefixed
                // component, which is not the scheme Decision 16 describes.
                // That was an unintended consequence of admitting modules at
                // all, found by reading a `nm` dump rather than by a test,
                // and it is narrowed here rather than special-cased in the
                // type encoder: it is one rule about which modules are
                // nameable, and a type's path and a function's path should
                // not answer it differently.
                DefKind::Module if entry.is_builtin() => {}
                _ => components.push(entry.name.clone()),
            }
            current = entry.parent;
        }
        components.reverse();
        components
    }

    /// How an instance reads in a diagnostic and in a dump. Not a symbol.
    pub fn describe(&self, instance: &Instance) -> String {
        let path = self.path_of(instance.def).join(".");
        let mut args: Vec<String> = Vec::new();
        if let Some(self_ty) = instance.self_ty {
            args.push(format!("Self={}", self.types.render(self.defs, self_ty)));
        }
        args.extend(instance.args.iter().map(|arg| match arg {
            GenericArg::Type(ty) => self.types.render(self.defs, *ty),
            GenericArg::Const(form) => form.render(self.defs),
            GenericArg::Error => "?".to_string(),
        }));
        if args.is_empty() {
            return path;
        }
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

/// Every generic record or `choice` reachable from `ty`, pushed onto `work`.
///
/// `ty` itself is pushed when it is `Named { def, args }` at a non-empty
/// `args` whose `def` is a record or a `choice` — a builtin generic
/// (`Array`, `Map`, `Box`) has no declared field list for
/// [`Mono::intern_aggregate_fields`] to substitute and is left out for the
/// same reason `science-codegen-llvm`'s `cg_ty_in` gives it its own arm
/// rather than falling into the record and `choice` cases. Either way the
/// walk continues into every type argument, because `Array[Pair[Int, F64]]`
/// has to surface `Pair[Int, F64]` even though `Array` itself is not an
/// aggregate this table describes.
///
/// `seen` is a set of whole types rather than of the pairs pushed, so a type
/// with no aggregate anywhere inside it — `Int`, `String`, `&Doc` — is not
/// re-walked the next time the same local type is met, which is the common
/// case for every scalar in a program.
fn collect_generic_aggregates(
    defs: &DefTable,
    types: &Types,
    ty: Ty,
    seen: &mut HashSet<Ty>,
    work: &mut Vec<(DefId, Vec<GenericArg>)>,
) {
    if !seen.insert(ty) {
        return;
    }
    match types.kind(ty) {
        TyKind::Named { def, args } => {
            let def = *def;
            if !args.is_empty() && matches!(defs.get(def).kind, DefKind::Record | DefKind::Choice) {
                work.push((def, args.clone()));
            }
            for arg in args.clone() {
                if let GenericArg::Type(inner) = arg {
                    collect_generic_aggregates(defs, types, inner, seen, work);
                }
            }
        }
        TyKind::Object { args, .. } => {
            for arg in args.clone() {
                if let GenericArg::Type(inner) = arg {
                    collect_generic_aggregates(defs, types, inner, seen, work);
                }
            }
        }
        TyKind::Tuple(elements) => {
            for element in elements.clone() {
                collect_generic_aggregates(defs, types, element, seen, work);
            }
        }
        TyKind::Nullable(inner) | TyKind::Borrowed { inner, .. } => {
            collect_generic_aggregates(defs, types, *inner, seen, work);
        }
        TyKind::Closure { params, ret } => {
            for param in params.clone() {
                collect_generic_aggregates(defs, types, param, seen, work);
            }
            collect_generic_aggregates(defs, types, *ret, seen, work);
        }
        TyKind::Unit
        | TyKind::Error
        | TyKind::Param { .. }
        | TyKind::SelfType { .. }
        | TyKind::SelfAssoc { .. } => {}
    }
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
            self_ty: None,
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
