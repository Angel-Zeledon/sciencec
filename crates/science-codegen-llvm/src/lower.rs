//! MIR to the backend's instruction set: §10's stages 0 to 3, and the exact
//! edge of what is not.
//!
//! **Where this belongs, and it is not here.** Decision 42's table puts
//! *"which operations are runtime calls"* above the line and leaves *"emitting
//! the call"* below it, which makes MIR-to-`Inst` an above-the-line job. There
//! is no such module in `science-codegen`: its own §2 says *"there is still no
//! `lower_hir` function"* and *"a `Ty -> CgTy` lowering does not exist"*, and
//! both are exactly what this file needs. So this file is written here, against
//! `science_codegen`'s vocabulary and using none of its own, and **it should
//! move above the line the day that crate grows a place for it.** Nothing in it
//! names LLVM; the test for that is that the module compiles with
//! `crate::sys` removed from scope, and `tests/independence.rs` in
//! `science-codegen` is the model for making that a fact rather than a claim.
//!
//! # What is lowered
//!
//! **Stage 1** — `print("hello, world")` — is §10's list, and two of its five
//! items are not what the emitted module contains. There is no
//! `science_println`: `science-rt` §8 records that the symbol was renamed and
//! the newline-adding form is `science_print`, which is what is called. And
//! there is no *drop glue*: glue is
//! `science_codegen::descriptor::DropGlue::Emitted`, a function this crate
//! would define, and a `String` does not need one — the temporary is freed by
//! a direct `science_string_free` at the site that made it, which is
//! `DropGlue::RuntimeCall`. Both are recorded as amendment 3 in §10 itself.
//!
//! **Stage 2** — a call into a C library — is [`Lowerer::lower_foreign_call`]
//! and [`Lowerer::declare_foreign`]: an `extern "C"` declaration becomes an
//! LLVM `declare` classified by §4.2's
//! [`classify_extern_argument`] and [`classify_extern_return`], a call becomes
//! Decision 40's *"direct `call` to the declared symbol"*, and the block's
//! `library` clause becomes a link decision in [`crate::link`]. `SC0461`
//! attributes an undefined symbol back to its declaration.
//!
//! **Stage 3** — control flow and integers — is the CFG (an `If` becomes a
//! `br`, a `Goto` becomes a `br`, and Decision 5's one-block-per-block holds),
//! `alloca`-per-local **including the slots codegen invents** ([`BodyCtx`]'s
//! note is why that stopped being free the moment a loop existed), and
//! §4.6's operators on scalars at their own width and signedness
//! ([`Lowerer::lower_binary`]).
//!
//! **Past stage 3** — and §10 does not number these together, so what follows
//! is what one program needed rather than what one stage listed:
//!
//! - **A `choice` and a `match`.** [`Lowerer::choice_ty`] computes §3.3's
//!   variants in declaration order, [`Lowerer::lower_discriminant`] reads the
//!   tag, and `TerminatorKind::Switch` translates a variant's **`DefId`** into
//!   a case value — which is the whole of the work, because MIR branches on a
//!   definition and *"the numbering is a layout question"*.
//! - **A function with parameters, and a call to it.** `Operand::Param` is
//!   above the line now; a register parameter is stored into Decision 8's
//!   `alloca` in the prologue, and an aggregate one **is** the caller's slot
//!   ([`crate::emit::ExtInst::ParamSlot`], Decision 22). Only what `main`
//!   reaches is emitted; [`Lowerer::lower_crate`] is the argument and the cost.
//! - **A record.** [`Lowerer::record_ty`] is Decision 17's fields in
//!   declaration order and [`Lowerer::place_address`] is §2.3's projection
//!   table, with `deref` as the one row that is not an offset.
//! - **A drop.** `TerminatorKind::Drop` is a `br` when the value owns nothing
//!   and `science_string_free` when it is a `String`.
//!   [`Lowerer::drop_runs_something`] is the predicate and finding 15 is why it
//!   is not `science_codegen::descriptor::needs_drop`.
//! - **Decision 6's `T?`**, in both representations: `null` is a tag store or a
//!   null pointer, `?` is one comparison ([`Lowerer::lower_is_present`]), and
//!   `T` into `T?` is `Coercion::Widen`.
//!
//! # What §10's stage 2 and stage 3 ask for that this language does not have
//!
//! Both of §10's programs print a computed value with `print(f"{x}")`.
//! **There is no string interpolation in this language.** `f"…"` is not in the
//! lexer, the parser, the AST or the corpus, and `print(f"{x}")` is a parse
//! error; there is also no integer- or float-to-string entry point among
//! `science-rt`'s 47, so there is no other spelling of it either. Both gates
//! are therefore met by a program whose *branch* depends on the value —
//! `if x is 1.0: print("1")` — which exercises everything the gate is about and
//! prints what it asks for. `tests/stage_two_and_three.rs` is the record.
//!
//! §10's stage 3 program is `for i in 0..10:`. **A `for` loop does not reach
//! this backend at all**: its `next` arrives as
//! [`science_mir::mir::Unresolved::IterateNext`], because
//! `science_types::thir::ExprKind::For` has no field for the callee the
//! checker's `iterate_item` found, and its range and iterator temporaries are
//! typed `TyKind::Error` with no diagnostic. That is a hole above this crate
//! and `science-mir`'s own note on the variant already states it. `loop:` with
//! a `break` is the same CFG in a form the front end delivers.
//!
//! # What is refused, and why refusing is the whole discipline
//!
//! Everything not lowered is `SC0400`, which §11 defines as *"a toolchain
//! feature required to build this program is not compiled into this
//! `sciencec`"* — and an unlowered MIR construct is literally that. The refusal
//! names the construct, so that a user who writes a tuple is told "a tuple" and
//! not "internal error".
//!
//! **A refusal whose stated reason has stopped being true is worse than no
//! refusal**, and this file has had one: `store_null` refused a tagged nullable
//! *"because the instruction set has no field projection"*, and the instruction
//! set grew one. A reader who believes a message like that stops looking, so
//! the check when anything is added here is not only *"does this still refuse"*
//! but *"is the reason it gives still the reason"*.
//!
//! **Two of the refusals are not about effort.** Integer `/` and `%` and the
//! two shifts are constructs this crate could emit an instruction for and
//! *must not*: LLVM's `sdiv` is **undefined** at a zero divisor rather than a
//! trap, and a shift by at least the operand's width is poison, and the guard
//! that would stop either is specified nowhere and emitted by nothing above
//! this crate. `science_codegen::backend::IntOp`'s own note says *"division by
//! zero is a panic the caller has already guarded"*; no caller guards it.
//! [`Lowerer::lower_binary`] is the argument in full, including what emitting
//! the guard here would cost Decision 5.
//!
//! # Decision 5 holds here, and one block is invented rather than merged
//!
//! Decision 5 says *"every MIR basic block becomes exactly one LLVM basic
//! block"*. Every block of the *user's* `main` does: [`Lowerer::lower_body`]
//! walks `body.blocks()` and emits one [`ExtBlock`] per MIR block, and nothing
//! merges or splits. `science-mir`'s §4 item 2 records that a **flagged drop**
//! breaks the decision by becoming three blocks; a flagged drop is refused
//! outright — both `StatementKind::SetDropFlag` and a `TerminatorKind::Drop`
//! carrying a flag are `SC0400` — so this crate has not met the contradiction
//! yet and the amendment that note asks for is still owed. An **unflagged**
//! drop is one block, which is why it could be lowered without meeting it: a
//! drop that runs nothing is a `br`, and a drop of a `String` is one call and a
//! `br`.
//!
//! **What a reader of `Built::ir` sees is not quite one-to-one, and the reason
//! is not this crate.** That field holds the module *after* Decision 33's
//! pipeline, and even at `-O0` the pipeline drops a basic block with no
//! predecessors — which `loop:` with a `break` produces, because MIR leaves an
//! unreachable arm behind on purpose (`science-mir`'s §2: *"a `Goto` chain that
//! a peephole would collapse is left alone, because … Decision 5 wants a MIR
//! dump and an IR dump to be diffable"*). So the claim is exact for the
//! emitter's output and holds on *reachable* blocks for anything downstream of
//! the pipeline, and `tests/stage_two_and_three.rs` asserts the second because
//! the second is what a reader can observe.
//!
//! What this crate does add is a whole function with no MIR behind it —
//! [`Lowerer::lower_c_main`]'s three blocks — which Decision 5 does not cover
//! because Decision 5 is about *lowering* a body and C's `main` is not one.
//!
//! **The cost of refusing rather than half-lowering** is that the set of
//! programs this compiler builds is tiny and the boundary is sharp. §10's own
//! discipline is the argument: *"every stage below produces a program that runs
//! and prints something, and no stage is finished until an execution test
//! asserts its output and exit code."* A backend that lowered a loop it had not
//! tested would produce a program that runs and prints the wrong thing.
//!
//! # The entry point, and §9.3's finding 5 met from the other side
//!
//! `science-rt` has no program entry point, so this module emits `main` itself,
//! as §9.3 says it must. What it emits is `script-mode.md` §2.3's exit table,
//! row for row:
//!
//! - the Science entry is `_S4main`, mangled by Decision 16, returning `Error?`
//!   through `sret` because `Error?` is `(any Error)?`, two words, and Windows
//!   x64 returns anything that is not 1, 2, 4 or 8 bytes by hidden pointer;
//! - C's `main` calls it, then tests **the data pointer and only the data
//!   pointer** — §3.4: *"the vtable slot of a null trait object is undefined and
//!   codegen must never load it — including on the path that tests for null"*;
//! - null is rows 1 to 3 (falling off the end, `return`, `return` of a null
//!   error, which are one value and one edge): `science_exit(0)`;
//! - non-null is row 4: `science_write_error_bytes` with a static message, then
//!   `science_exit(1)`;
//! - row 5, `panic(…)`, does not come through here at all. It is
//!   `science_panic_bytes` at the panicking call site and it still aborts, and
//!   `panic.rs`'s argument for that is untouched by anything here.
//!
//! **This used to abort, and the record of why is worth keeping.** Until
//! `science-rt` grew [`exit.rs`'s two symbols][exit], the fourth row had nothing
//! to lower to: the only stderr writer was `science_panic_bytes`, which aborts,
//! and `abort()` is `SIGABRT` on POSIX and 3 on Windows and is not 1. So the
//! emitted `main` called it with a message explaining that it could not do what
//! §2.3 requires — the closest reachable behaviour, and reachable only by a
//! program the compiler could not produce. [`EXIT_CONTRACT`] was the record of
//! the gap; it now names the two symbols and
//! [`ExitContract::is_satisfiable`](science_codegen::runtime::ExitContract::is_satisfiable)
//! is true.
//!
//! [exit]: https://docs.rs/science-rt
//!
//! # What the failing row actually prints, and how far short it falls
//!
//! §2.3 asks for `error: ` and then **the `Display` of the error**. What is
//! emitted is [`ERROR_MESSAGE`]: the required prefix, then a fixed phrase, then
//! a newline. The error's identity is not in it.
//!
//! **The reason is that `Display` is not reachable, not that rendering it is
//! hard.** `science-resolve`'s `builtins.rs` declares `Display` as an interface
//! **with no methods**, and says why: naming `Display.display(Formatter)` would
//! invent `Formatter`, a Level 1 type no note specifies, as a side effect of a
//! bound check. There is therefore no method name, no vtable slot, and nothing
//! for a `call` here to go through. `Error.message(shared self) -> String` *is*
//! declared — it is `stdlib-core.md` §7.5 verbatim — and is no better off: no
//! implementation of it exists anywhere in `science-rt`, this backend emits no
//! vtables at all, and calling through a slot nothing fills is a jump to
//! whatever the second word of the fat pointer holds.
//!
//! **The cost, stated plainly: a user is told that something failed and is not
//! told what.** That is a real loss and it is the smaller one. The alternative
//! is for a code generator to invent `Formatter` — to pick a signature for a
//! type two design notes decline to specify — which is the failure
//! `science-types`'s `assign.rs` §3 names: a decision nobody argued for,
//! arriving as a side effect of something else, and arriving from the component
//! with the least standing to make it. The placeholder is deleted the day
//! `Display` has a method; `EXIT_CONTRACT.display_is_renderable` is `false`
//! until then and a test asserts it, so the deletion is prompted rather than
//! remembered.

use std::collections::BTreeMap;

use science_codegen::abi::{
    AbiParam, AbiSignature, ArgClass, ParamAttrs, ReturnClass, classify_extern_argument,
    classify_extern_return,
};
use science_codegen::backend::{
    BlockId, Callee, CmpOp, Inst, IntOp, FloatOp, LocalId, Operand, Terminator, ValueId,
};
use science_codegen::descriptor::StringLiteral;
use science_codegen::diagnostics::construct_not_lowered;
use science_codegen::layout::{
    CgTy, Field as CgField, IntTy, Layout, PtrKind, Repr, Scalar, Triple,
    Variant as CgVariant, layout_of,
};
use science_codegen::mangle::{MonoKey, mangle};
use science_codegen::runtime::{RUNTIME, RtAggregate, RtParam, RtRet, RuntimeFn, runtime_fn};
use science_diagnostics::{Diagnostic, Span};
use science_mir::mir::{self, Body as MirBody, Constant, Rvalue, StatementKind, TerminatorKind};
use science_parser::ast::{BinaryOp, UnaryOp};
use science_resolve::hir::{self, DefId, DefKind, DefTable, Literal};
use science_types::Types;
use science_types::items::Declarations;
use science_types::ty::{Ty, TyKind};

use crate::emit::{ConvOp, ExtBlock, ExtBody, ExtInst};

/// What the emitted `main` writes to standard error when the script body
/// returns a non-null error.
///
/// **It opens with `script-mode.md` §2.3's required prefix and then says
/// something true and smaller than §2.3 asks for.** The note asks for the
/// error's `Display`; the prelude's `Display` has no method, so there is
/// nothing to call and no vtable slot to call it through. The module
/// documentation is the full argument, including why inventing `Formatter` to
/// close the gap would be the worse trade.
///
/// The prefix is asserted against
/// [`science_codegen::runtime::EXIT_CONTRACT`]`.required_prefix` by
/// `tests/exit_code.rs` rather than spliced in here: a `const` built by
/// concatenation reads worse than the bytes it produces, and the bytes are what
/// a user sees.
///
/// The trailing newline is this constant's and not the runtime's.
/// `science_write_error_bytes` is `stdlib-core.md` §4.1's `write_error` — the
/// form that adds nothing — so the line terminator has to be here. It is `\n`
/// on every platform, for `science_print`'s reason: Science text is UTF-8 and
/// its terminator is `\n`, and translating it would make output depend on where
/// the compiler was built.
pub const ERROR_MESSAGE: &str = "error: the script returned an error, and this compiler cannot \
                                 say which one — `script-mode.md` §2.3 asks for the error's \
                                 `Display`, and the prelude declares `Display` with no method to \
                                 call\n";

/// What a local with no type is refused as.
///
/// **One sentence for two causes, because the compiler cannot tell them
/// apart.** A local whose `Ty` is `TyKind::Error` reaches this backend with
/// **no diagnostic having been reported** — §5's *"the mistake has already been
/// reported"* rule firing on a mistake nobody made — so there is nothing here
/// that says which expression produced it. Two are known and both are named:
/// a call to `print` or `write`, whose signatures `science-resolve`'s
/// `builtins.rs` wrote, measured seven corpus false positives on, and withdrew;
/// and a `for` loop, whose range and iterator temporaries the checker leaves
/// untyped for the same reason its `next` callee is
/// [`science_mir::mir::Unresolved::IterateNext`].
///
/// An earlier version of this message named `print` alone, so a `for` loop over
/// a range was refused with a sentence about a function it does not call. A
/// refusal that names the wrong construct is worse than one that names two.
const UNTYPED: &str = "a value the front end left untyped: its `Ty` is `TyKind::Error` and no \
                       diagnostic was reported for it. The three this compiler has met are a call \
                       to `print` or `write`, whose signatures `science-resolve`'s `builtins.rs` \
                       withdrew; a `for` loop's range and iterator temporaries; and a \
                       discriminant temporary, which is handled rather than refused";

/// How deep [`Lowerer::cg_ty`] follows a type before it gives up.
///
/// See that function's note: this stops a compiler that met a self-containing
/// type from dying with no diagnostic, and it is far above anything a program
/// writes.
const MAX_TYPE_DEPTH: u32 = 32;

/// Why a program could not be lowered.
///
/// One variant, carrying the construct's name, because every refusal is the
/// same refusal: this compiler does not implement that yet. §11's `SC0400` is
/// the code and its contract is *"names the feature and how to obtain a build
/// that has it"* — here the second half is "a later stage of the backend", which
/// is not a package anyone can install, and the note says so rather than
/// inventing an install command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unlowered {
    /// The construct, named the way a user would recognise it.
    pub construct: String,
}

impl Unlowered {
    fn new(construct: impl Into<String>) -> Unlowered {
        Unlowered { construct: construct.into() }
    }

    /// The `SC0400` this becomes.
    ///
    /// **[`construct_not_lowered`] and not
    /// `science_codegen::diagnostics::backend_not_compiled_in`**, which this
    /// used and which is shaped for the *name of a backend*: it rendered every
    /// refusal here as *"no `a tuple` backend is compiled into this
    /// `sciencec`"*. Same code, same contract, a sentence the construct fits
    /// into.
    pub fn to_diagnostic(&self) -> Diagnostic {
        construct_not_lowered(
            &self.construct,
            "this `sciencec` implements §10's stages 0 to 3 of `codegen-and-linking.md` and some \
             of what comes after — a script body, string literals and `print`, `extern \"C\"` \
             declarations and the calls to them, the CFG, scalar arithmetic, functions with \
             parameters, records, `choice`s and `match`, `T?`, and drops of values that own \
             nothing — and refuses everything else rather than lowering a construct no execution \
             test has ever run",
        )
    }
}

/// The signature of a runtime entry point, derived from
/// [`science_codegen::runtime::RUNTIME`].
///
/// **Derived and never written down**, which is `runtime.rs`'s whole argument:
/// §9.2's finding was a hand-maintained list of the entry points that need
/// `sret`, and `science_array_with_capacity` had fallen off it. A declaration
/// built from the signature cannot fall off anything.
///
/// **No pointer attributes are emitted, and that is a finding rather than
/// laziness.** Decision 24 wants `readonly nocapture` on a shared borrow and
/// `noalias nocapture` on an exclusive one, and
/// [`science_codegen::runtime::RtParam`] has **one** pointer variant: it does
/// not distinguish `*const ScienceString` from `*mut ScienceString`. Every one
/// of the signatures makes the distinction in Rust —
/// `science_print(text: *const ScienceString)` against
/// `science_string_free(value: *mut ScienceString)` — and the table that codegen
/// reads erases it. Emitting `readonly` on a guess is §4.4's failure mode with
/// the compiler on the wrong end of it, so nothing is emitted and the
/// optimisation Decision 24 buys is left on the table until `RtParam` splits.
pub fn runtime_signature(target: Triple, entry: &RuntimeFn) -> AbiSignature {
    let ret_ty = match entry.ret {
        RtRet::Void | RtRet::Never => CgTy::Unit,
        RtRet::Bool => CgTy::Bool,
        RtRet::I32 => CgTy::Int(IntTy::I32),
        RtRet::Int => CgTy::Int(IntTy::I64),
        RtRet::U64 => CgTy::Int(IntTy::U64),
        RtRet::Ptr => CgTy::Ptr(PtrKind::Raw),
        RtRet::Aggregate(aggregate) => aggregate.cg_ty(),
    };
    let ret_layout = layout_of(target, &ret_ty);
    let params = entry
        .params
        .iter()
        .enumerate()
        .map(|(index, param)| {
            let ty = match param {
                RtParam::Descriptor | RtParam::Pointer => CgTy::Ptr(PtrKind::Raw),
                RtParam::Usize => CgTy::Int(IntTy::Usize),
                RtParam::Int => CgTy::Int(IntTy::I64),
                RtParam::I32 => CgTy::Int(IntTy::I32),
                RtParam::U64 => CgTy::Int(IntTy::U64),
                RtParam::F64 => CgTy::Float(science_codegen::layout::FloatTy::F64),
                RtParam::F32 => CgTy::Float(science_codegen::layout::FloatTy::F32),
                // §3.1's memory form. Rust's `bool` across `extern "C"` is C's
                // one-byte `_Bool`, and `CgTy::Bool` is that byte — a `Direct`
                // `i8` argument, which is what `scalar_ty` builds and what the
                // runtime's own `#[no_mangle] fn(.., flag: bool)` expects.
                RtParam::Bool => CgTy::Bool,
                RtParam::Char => CgTy::Char,
            };
            AbiParam {
                name: format!("a{index}"),
                class: ArgClass::Direct,
                layout: layout_of(target, &ty),
                attrs: ParamAttrs::default(),
            }
        })
        .collect();
    AbiSignature {
        symbol: entry.symbol.to_string(),
        ret: science_codegen::abi::classify_return_for(target, &ret_layout),
        ret_layout,
        params,
        foreign: true,
        nounwind: true,
    }
}

/// Every runtime entry point, by symbol, for a target.
pub fn runtime_signatures(target: Triple) -> Vec<AbiSignature> {
    RUNTIME.iter().map(|entry| runtime_signature(target, entry)).collect()
}

/// A foreign symbol this module declares, and where it was declared.
///
/// **The span is the whole point.** Decision 29 makes `SC0461` *"an undefined
/// symbol that codegen's own table can attribute to an `extern`
/// declaration"*, and this is that table: without it a missing `cos` is
/// `link.exe`'s `LNK2019` handed to the user verbatim as `SC0402`, which
/// §5.5 calls the honest answer *only* for a failure codegen did not
/// understand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignSymbol {
    /// The linker name — the `symbol "…"` clause when there is one, and the
    /// Science name otherwise.
    pub symbol: String,
    /// The Science name, for the message.
    pub name: String,
    /// The declaration's span.
    pub span: Span,
    /// The `library` clause's name, for §10 stage 2's gate: *"naming the
    /// declaration's span **and the library clause**"*.
    pub library: Option<String>,
}

/// A lowered module: what the backend is asked to emit, in order.
pub struct Lowered {
    /// The string literals, in the order they were interned.
    pub literals: Vec<StringLiteral>,
    /// Declarations for every runtime entry point and foreign symbol the
    /// module calls.
    pub declarations: Vec<AbiSignature>,
    /// Definitions, in Decision 4's sorted-by-symbol order.
    pub definitions: Vec<(AbiSignature, ExtBody)>,
    /// The `library` clauses of every `extern` block that contributed a
    /// declaration, in **source order** — Decision 28, which makes that order
    /// significant and says so.
    pub libraries: Vec<String>,
    /// Every foreign symbol declared, for `SC0461`.
    pub foreign: Vec<ForeignSymbol>,
}

/// One `extern` block's function, resolved to what codegen needs of it.
struct Foreign {
    symbol: String,
    library: Option<String>,
    span: Span,
    variadic: bool,
}

/// The whole of what this backend can lower.
pub struct Lowerer<'a> {
    target: Triple,
    defs: &'a DefTable,
    types: &'a Types,
    decls: Option<&'a Declarations>,
    foreign: BTreeMap<DefId, Foreign>,
    /// Source order of the `library` clauses, and the set of the ones actually
    /// reached. Decision 28's order is the first; the second is what keeps a
    /// program that declares a block it never calls from failing to link
    /// against a library it never needed.
    library_order: Vec<String>,
    libraries_used: Vec<String>,
    literals: Vec<StringLiteral>,
    declarations: Vec<AbiSignature>,
    declared_foreign: Vec<ForeignSymbol>,
    /// The definition of the entry point, once [`Lowerer::lower_crate`] has
    /// found it. See [`Lowerer::symbol_of`]: it is the one definition whose
    /// symbol is not its path.
    entry: Option<DefId>,
    /// Every reachable Science function's classified signature, built before
    /// any body is lowered.
    ///
    /// **One table, and that is the point.** A definition and a call site that
    /// each classified the same function would be two implementations of
    /// §4.2's return rules with nothing comparing them, and §9.2's finding is
    /// what one disagreement costs: *"a call site got `sret` wrong for one
    /// symbol"*, which corrupts a register and links cleanly.
    science: BTreeMap<DefId, AbiSignature>,
}

/// Every definition `entry` can reach, through [`MirBody::callees`].
///
/// A breadth-first walk over the bodies present, so a callee with no body — an
/// `extern "C"` declaration, a builtin — contributes nothing and is not an
/// error here; the call site is where those are resolved.
fn reachable_from(bodies: &[MirBody], entry: DefId) -> std::collections::BTreeSet<DefId> {
    let mut reached = std::collections::BTreeSet::new();
    let mut queue = vec![entry];
    while let Some(def) = queue.pop() {
        if !reached.insert(def) {
            continue;
        }
        let Some(body) = bodies.iter().find(|body| body.def() == def) else { continue };
        for callee in body.callees() {
            if !reached.contains(&callee) {
                queue.push(callee);
            }
        }
    }
    reached
}

impl<'a> Lowerer<'a> {
    /// A lowerer for one crate.
    ///
    /// No `extern` blocks and no declarations: [`Lowerer::with_externs`] is
    /// the form stage 2 needs, and this one is kept because two tests build a
    /// module with no foreign anything in it and asking them for two empty
    /// tables would be asking them to name a thing they do not have.
    pub fn new(target: Triple, defs: &'a DefTable, types: &'a Types) -> Lowerer<'a> {
        Lowerer {
            target,
            defs,
            types,
            decls: None,
            foreign: BTreeMap::new(),
            library_order: Vec::new(),
            libraries_used: Vec::new(),
            literals: Vec::new(),
            declarations: Vec::new(),
            declared_foreign: Vec::new(),
            entry: None,
            science: BTreeMap::new(),
        }
    }

    /// A lowerer that can reach §10 stage 2: the `extern` blocks, and the
    /// declaration table their signatures come from.
    ///
    /// **The signature is read from `Declarations` and not from the HIR**,
    /// although the HIR's `ExternFn` carries a parameter list. The HIR's
    /// parameters are *syntax* — `type BlasInt is I32` is an alias this crate
    /// would have to resolve — and resolving it here is a second, worse copy of
    /// the type checker. `Declarations::signature` is the one the checker
    /// already built and the one the call site was checked against, so a
    /// disagreement between the declaration and the call is impossible by
    /// construction rather than by testing.
    pub fn with_externs(
        target: Triple,
        defs: &'a DefTable,
        types: &'a Types,
        decls: &'a Declarations,
        blocks: &[&hir::ExternBlock],
    ) -> Lowerer<'a> {
        let mut lowerer = Lowerer::new(target, defs, types);
        lowerer.decls = Some(decls);
        for block in blocks {
            let library = block.library.as_ref().map(|library| library.name.clone());
            if let Some(name) = &library {
                if !lowerer.library_order.contains(name) {
                    lowerer.library_order.push(name.clone());
                }
            }
            for item in &block.items {
                let hir::ExternItemKind::Fn(function) = &item.kind else { continue };
                let name = defs.get(function.def).name.clone();
                lowerer.foreign.insert(
                    function.def,
                    Foreign {
                        symbol: function.symbol.clone().unwrap_or(name),
                        library: library.clone(),
                        span: defs.get(function.def).span,
                        variadic: function.variadic,
                    },
                );
            }
        }
        lowerer
    }

    fn declare(&mut self, symbol: &str) -> Result<AbiSignature, Unlowered> {
        if let Some(existing) = self.declarations.iter().find(|s| s.symbol == symbol) {
            return Ok(existing.clone());
        }
        let entry = runtime_fn(symbol)
            .ok_or_else(|| Unlowered::new(format!("a call to `{symbol}`, which is not one of \
                                                   `science-rt`'s entry points")))?;
        let sig = runtime_signature(self.target, entry);
        self.declarations.push(sig.clone());
        Ok(sig)
    }

    /// Intern a string literal, returning the global codegen will emit for it.
    ///
    /// **Public for `tests/exit_code.rs`**, which needs a global whose address
    /// is not null — that is the whole of what a non-null `(any Error)?`'s data
    /// word has to be for `main` to take §2.3's fourth row. Asking the lowerer
    /// for one rather than inventing a symbol name keeps the test's global in
    /// the same [`Lowered::literals`] the emitter defines from, so a test cannot
    /// name a global the module does not have.
    pub fn intern_literal(&mut self, text: &str) -> StringLiteral {
        if let Some(existing) = self.literals.iter().find(|l| l.bytes == text.as_bytes()) {
            return existing.clone();
        }
        let literal = StringLiteral::new(self.literals.len(), text);
        self.literals.push(literal.clone());
        literal
    }

    /// A `science_types::Ty` as a [`CgTy`].
    ///
    /// **This is the `Ty -> CgTy` lowering `science-codegen`'s §2 says does not
    /// exist**, cut down to the types stage 1 can produce. It belongs above the
    /// line beside `layout`, and it is here because there is nowhere above the
    /// line to put it.
    ///
    /// The `Named` case reads the *name*, which is wrong in general — two
    /// modules can each define a `String` — and is right for the prelude types
    /// it is restricted to, all of which are builtins. A real lowering keys on
    /// the `DefId`. Recorded rather than smoothed because the fix is to move the
    /// function, not to patch it.
    fn cg_ty(&self, ty: science_types::ty::Ty) -> Result<CgTy, Unlowered> {
        self.cg_ty_at(ty, 0)
    }

    /// [`Lowerer::cg_ty`], counting how deep the nesting has gone.
    ///
    /// **The depth is a guard against a hang, not against a wrong answer.** A
    /// record that contains itself has no finite layout and the type checker is
    /// what rejects it; a code generator that met one anyway would recurse until
    /// the stack ran out, which is a compiler that dies with no diagnostic. The
    /// bound is far above anything a program writes and the refusal names both
    /// causes, because from here they are indistinguishable.
    fn cg_ty_at(&self, ty: science_types::ty::Ty, depth: u32) -> Result<CgTy, Unlowered> {
        if depth > MAX_TYPE_DEPTH {
            return Err(Unlowered::new(format!(
                "a type nested more than {MAX_TYPE_DEPTH} deep, or a type that contains itself \
                 and therefore has no size"
            )));
        }
        match self.types.kind(ty) {
            TyKind::Unit => Ok(CgTy::Unit),
            TyKind::Nullable(inner) => Ok(CgTy::nullable(self.cg_ty_at(*inner, depth + 1)?)),
            // `any I` — Decision 13's two-word fat pointer, with the null niche
            // in the data pointer (§3.4).
            TyKind::Object { .. } => Ok(CgTy::Interface),
            TyKind::Borrowed { mutable, .. } => Ok(CgTy::Ptr(if *mutable {
                PtrKind::MutBorrow
            } else {
                PtrKind::Borrow
            })),
            // `Error?` is spelled `Nullable(Named { def: Error })` in the type
            // table, not `Nullable(Object)`: `syntax-revision-2.md` §3.4 makes
            // `Error?` shorthand for `(any Error)?`, and the shorthand is
            // expanded by `assign`'s subtyping rather than by a rewrite of the
            // type, so what reaches codegen still names the interface. Decision
            // 13's representation is the same either way — a two-word fat
            // pointer with the niche in the data word — so this arm and the
            // `Object` one above produce the same `CgTy`, and they have to,
            // because `_S4main`'s `sret` slot is written by one and read by the
            // other.
            TyKind::Named { def, args }
                if args.is_empty()
                    && self.defs.get(*def).kind == science_resolve::hir::DefKind::Interface =>
            {
                Ok(CgTy::Interface)
            }
            // §3.2's Decision 17, reached through the *definition* rather than
            // through the name — which the arm below cannot do and says so.
            TyKind::Named { def, args }
                if args.is_empty() && self.defs.get(*def).kind == DefKind::Record =>
            {
                self.record_ty(*def, depth)
            }
            // §3.3's Decision 18 and §3.4's Decision 19, both of which
            // `layout_of` already implements: what is needed here is the list
            // of variants in declaration order, because **declaration order is
            // what fixes the discriminant values** and nothing below this
            // function can recover it.
            TyKind::Named { def, args }
                if args.is_empty() && self.defs.get(*def).kind == DefKind::Choice =>
            {
                self.choice_ty(*def, depth)
            }
            // A generic record or choice, named rather than swallowed by the
            // arm below. Decision 42 puts the monomorphisation walk above this
            // crate and `science_codegen::mono` is where it will live; until
            // something runs it, a `Pair of (Int, Int)` reaches here with its
            // arguments still on it and there is no substituted field list to
            // lay out.
            TyKind::Named { def, .. }
                if matches!(self.defs.get(*def).kind, DefKind::Record | DefKind::Choice) =>
            {
                Err(Unlowered::new(format!(
                    "a value of the generic type `{}`, which nothing has monomorphised: Decision \
                     42 puts the walk above this crate and no phase runs it yet",
                    self.defs.get(*def).name
                )))
            }
            // A **builtin** generic: `Array of Int`, `Map of (K, V)`, `Box of
            // T`. Not the arm above, because these are not records or choices
            // and there is no field list to substitute into — they are §2.6's
            // runtime aggregates, reached through a `ScienceTypeInfo` this
            // backend does not emit.
            //
            // **It is named rather than left to the fallback**, which rendered
            // it as `Named { def: DefId(20), args: [Type(Ty(17))] }` — an
            // interned index a reader cannot look up, which is the exact
            // complaint the `Tuple` arm below used to carry about the same
            // fallback.
            TyKind::Named { args, .. } if !args.is_empty() => Err(Unlowered::new(format!(
                "a value of type `{}`, one of §2.6's runtime containers: its value is a \
                 `science-rt` aggregate reached through a `ScienceTypeInfo` descriptor, and this \
                 backend emits no descriptor",
                self.types.render(self.defs, ty)
            ))),
            TyKind::Named { def, args } if args.is_empty() => {
                let name = self.defs.get(*def).name.as_str();
                match name {
                    "Int" => Ok(CgTy::Int(IntTy::I64)),
                    "I8" => Ok(CgTy::Int(IntTy::I8)),
                    "I16" => Ok(CgTy::Int(IntTy::I16)),
                    "I32" => Ok(CgTy::Int(IntTy::I32)),
                    "I64" => Ok(CgTy::Int(IntTy::I64)),
                    "U8" => Ok(CgTy::Int(IntTy::U8)),
                    "U16" => Ok(CgTy::Int(IntTy::U16)),
                    "U32" => Ok(CgTy::Int(IntTy::U32)),
                    "U64" => Ok(CgTy::Int(IntTy::U64)),
                    "F32" => Ok(CgTy::Float(science_codegen::layout::FloatTy::F32)),
                    "F64" => Ok(CgTy::Float(science_codegen::layout::FloatTy::F64)),
                    "Bool" => Ok(CgTy::Bool),
                    "Char" => Ok(CgTy::Char),
                    "String" => Ok(RtAggregate::String.cg_ty()),
                    other => Err(Unlowered::new(format!("a value of type `{other}`"))),
                }
            }
            // The remaining kinds, each named the way a user would recognise
            // it. The `{other:?}` fallback below renders `TyKind`'s `Debug` —
            // `Tuple([Ty(0), Ty(0)])` — which names an interned index a reader
            // cannot look up and a construct they did not write.
            // §3.2's Decision 17 again, with positions where a record has
            // names. **No layout rule was added for this**: a tuple is a
            // `CgTy::Struct` whose fields are called `0`, `1`, … and
            // `layout_of` lays it out by the same C rule it lays a record out
            // by, which is what `science-codegen`'s §3.2 already said applies
            // *"to one unchanged"*. The arm was simply not written.
            TyKind::Tuple(elements) => self.tuple_ty(ty, elements.clone(), depth),
            // **A type the front end left as a hole, met where a *type* was
            // needed rather than a value.** [`UNTYPED`] is the sibling message
            // for a whole local whose `Ty` is this, and the two are different
            // situations: that one is `print`'s undeclared return, which is
            // skipped because nothing reads it, and this one is an erroneous
            // type *inside* a type that is otherwise fine — a tuple element, a
            // record field — where there is nothing to skip.
            //
            // **It is reachable from a program that checks clean**, which is
            // what makes the message worth writing out. `let t be (1, 2)` types
            // as `(TyKind::Error, TyKind::Error)`: `science-types`'s `Tuple`
            // arm calls `known_or_error` on each element as it synthesises the
            // node, and an integer literal's type is still an inference
            // variable at that moment. Nothing reports it, because §5's rule is
            // that an erroneous type means a mistake already reported and here
            // there was no mistake. `let t: (Int, Int) be (1, 2)` and
            // `let t be (1i64, 2i64)` both build; the bare one is refused here,
            // by the only phase that ever looks.
            TyKind::Error => Err(Unlowered::new(
                "a value whose type the front end left as `TyKind::Error` with no diagnostic \
                 beside it — for a tuple this is `science-types`' `ExprKind::Tuple` arm reading \
                 each element's type before inference has defaulted it, so `(1, 2)` is a tuple of \
                 two holes and `(1i64, 2i64)` is not",
            )),
            TyKind::Param { .. } | TyKind::SelfType { .. } | TyKind::SelfAssoc { .. } => {
                Err(Unlowered::new(
                    "a value whose type is still a type parameter, which nothing has \
                     monomorphised: Decision 42 puts the walk above this crate and no phase runs \
                     it yet",
                ))
            }
            TyKind::Closure { .. } => Err(Unlowered::new(
                "a closure, whose body `science-mir` does not lower at all — `Rvalue::Closure` \
                 carries a THIR expression id and its captures, and nothing turns either into a \
                 function",
            )),
            other => Err(Unlowered::new(format!("a value of type `{other:?}`"))),
        }
    }

    /// A `record` as [`CgTy::Struct`]: Decision 17's fields, in declaration
    /// order.
    ///
    /// **The order is read from `Declarations::record` and from nothing else.**
    /// `science_types::items::Record::fields` is *"a record's fields, lowered,
    /// in declaration order"*, and declaration order is what Decision 17 lays
    /// out — *"fields in declaration order … no field reordering, ever"*. MIR's
    /// `Rvalue::Record` carries its fields in the order they were **written**,
    /// which for a record literal with named arguments is any order the author
    /// liked, so a code generator that took MIR's order would lay out
    /// `Doc(body: "b", title: "t")` differently from `Doc(title: "t",
    /// body: "b")` — two layouts for one type, which is `LayoutCache`'s
    /// `SC0404` if anything asked it and silence if nothing does.
    fn record_ty(&self, def: DefId, depth: u32) -> Result<CgTy, Unlowered> {
        let name = self.defs.get(def).name.clone();
        let record = self.declarations()?.record(def).ok_or_else(|| {
            Unlowered::new(format!(
                "a value of type `{name}`, which the declaration table has no lowered field list \
                 for"
            ))
        })?;
        let mut fields = Vec::with_capacity(record.fields.len());
        for (field, ty) in &record.fields {
            fields.push(CgField::new(
                self.defs.get(*field).name.clone(),
                self.cg_ty_at(*ty, depth + 1)?,
            ));
        }
        Ok(CgTy::strukt(name, fields))
    }

    /// A tuple as [`CgTy::Struct`]: §3.2's C layout, with positions for names.
    ///
    /// **The name is the rendered type and that is load-bearing.**
    /// `science_codegen::layout::LayoutCache` is keyed by a `CgTy::Struct`'s
    /// name and answers `SC0404` when one name gets two layouts, so a tuple's
    /// name has to be a function of its element types and of nothing else.
    /// `Types::render` gives `(Int, F64)`, which is the spelling the author
    /// wrote and is different for every different tuple — where a fixed name
    /// like `"tuple"` would make `(Int, Int)` and `(Int, F64)` collide, and a
    /// name derived from the `Ty` index would make two structurally identical
    /// tuples two types to the cache.
    ///
    /// **The field names are `0`, `1`, … and they are never looked up.**
    /// [`Lowerer::place_address`]'s `TupleField` arm indexes the layout's field
    /// list by position, so the strings exist for Decision 31's layout record
    /// and for a `--emit=layout` dump, and a reader who changes them breaks
    /// nothing. The *order* is the source order and it is the thing that
    /// matters: Decision 17's *"no field reordering, ever"* applies here for
    /// the same reason it applies to a record, and there is no second order to
    /// confuse it with because a tuple literal cannot name its elements.
    fn tuple_ty(&self, ty: Ty, elements: Vec<Ty>, depth: u32) -> Result<CgTy, Unlowered> {
        let mut fields = Vec::with_capacity(elements.len());
        for (index, element) in elements.iter().enumerate() {
            fields.push(CgField::new(index.to_string(), self.cg_ty_at(*element, depth + 1)?));
        }
        Ok(CgTy::strukt(self.types.render(self.defs, ty), fields))
    }

    /// A `choice` as [`CgTy::Choice`]: §3.3's variants, in declaration order.
    ///
    /// **A one-element payload is the element and is not wrapped**, and that is
    /// a layout decision rather than a convenience. §3.3 makes the payload union
    /// *"a union of the variant payloads"*, and the member for `Circle(F64)` is
    /// an `F64`. Wrapping it in a one-field struct would give the same size and
    /// the same offset and would cost Decision 19's niche: `CgTy::niche` answers
    /// only for a pointer and for `any I`, so `choice { none, some(Box of T) }`
    /// would stop being one word the moment the payload became a struct. What
    /// the un-wrapping costs is one case in [`Lowerer::place_address`], where
    /// MIR's `TupleField { index: 0 }` off such a variant is the identity rather
    /// than a field lookup, and that function says so.
    fn choice_ty(&self, def: DefId, depth: u32) -> Result<CgTy, Unlowered> {
        let name = self.defs.get(def).name.clone();
        let decls = self.declarations()?;
        let order = self.choice_variants(def);
        if order.is_empty() {
            return Err(Unlowered::new(format!(
                "a value of type `{name}`, a `choice` with no variants: it has no value and no \
                 discriminant to hold one"
            )));
        }
        let mut variants = Vec::with_capacity(order.len());
        for variant in order {
            let variant_name = self.defs.get(variant).name.clone();
            // Absent is **not** the same as empty: a variant the declaration
            // table has no entry for is a variant whose payload nobody lowered,
            // and treating it as payload-free would put the wrong number of
            // bytes in the union.
            let declared = decls.variant(variant).ok_or_else(|| {
                Unlowered::new(format!(
                    "the variant `{name}.{variant_name}`, which the declaration table has no \
                     lowered payload for"
                ))
            })?;
            variants.push(match declared.payload.as_slice() {
                [] => CgVariant::unit(variant_name),
                [only] => CgVariant::with(variant_name, self.cg_ty_at(*only, depth + 1)?),
                many => {
                    let mut fields = Vec::with_capacity(many.len());
                    for (index, ty) in many.iter().enumerate() {
                        fields.push(CgField::new(
                            index.to_string(),
                            self.cg_ty_at(*ty, depth + 1)?,
                        ));
                    }
                    let payload = CgTy::strukt(format!("{name}.{variant_name}"), fields);
                    CgVariant::with(variant_name, payload)
                }
            });
        }
        Ok(CgTy::choice(name, variants))
    }

    /// A `choice`'s variants, in declaration order.
    ///
    /// **Declaration order is the discriminant numbering** — Decision 18:
    /// *"discriminant values follow declaration order from zero"* — so this is
    /// the function that decides what `switch` branches on, and it is a scan of
    /// the definition table rather than a list on the choice because
    /// `hir::DefTable` is flat and a variant's parent is the choice.
    /// `DefTable::children` documents the order it yields and
    /// `science-regions` and `science-types` both read it the same way.
    fn choice_variants(&self, choice: DefId) -> Vec<DefId> {
        self.defs
            .children(choice)
            .filter(|def| def.kind == DefKind::Variant)
            .map(|def| def.id)
            .collect()
    }

    /// Whether dropping a value of `ty` has to run anything.
    ///
    /// **This exists because `science_codegen::descriptor::needs_drop` answers
    /// `false` for a `String`, and that is finding 15.** That function is a
    /// predicate over [`CgTy`], and `CgTy` is *"the set of distinctions that
    /// change a layout or an ABI classification, and nothing else"* — so a
    /// `String` arrives at it as `RtAggregate::String.cg_ty()`, which is
    /// `{ Ptr(Raw), Usize, Usize }`, and Decision 19's table makes `Raw` the one
    /// pointer kind that owns nothing. The predicate is exactly right about
    /// Science's own types and blind to all four of the runtime's owning
    /// aggregates, because the model it reads deliberately erases what
    /// distinguishes them. Nothing called it on one before this function
    /// existed, so nothing was wrong; a code generator that called it on a
    /// `Drop` terminator would have turned every dropped `String` into a leak
    /// with no diagnostic.
    ///
    /// **So the walk is over `Ty` and not over `CgTy`**, which is the level
    /// where a `String` is still a `String` and where a user's
    /// `Doc implements Drop:` is visible at all.
    ///
    /// **It answers `true` where it cannot tell**, which is `science-mir`'s own
    /// rule for the same question, and here the cost of that is a refusal
    /// rather than a missed destructor. The types it will say `false` for are
    /// the ones built only out of §3.1's scalars: a scalar, a record of them, a
    /// `choice` whose payloads are them, and a tuple or nullable of those.
    /// Everything else — a `String`, an `Array`, a `Map`, a `Box`, a trait
    /// object, a type parameter, a closure, a type with its own `Drop` — is a
    /// refusal that names the type.
    ///
    /// This is narrower than `science-mir`'s `needs_drop`, which answers `true`
    /// for **every** `choice` — *"§4's true where it cannot tell"* — and that
    /// difference is the whole reason a `match` over a payload-free `choice`
    /// needs this function at all: MIR emits a `Drop` for the scrutinee of
    /// every `match` in the language, so *"the smallest program that cannot be
    /// built"* was never only about `cg_ty`.
    fn drop_runs_something(&self, ty: Ty, depth: u32) -> Result<bool, Unlowered> {
        if depth > MAX_TYPE_DEPTH {
            return Ok(true);
        }
        // A user `Drop` implementation is a fact about a definition, and it is
        // checked first because it overrides every structural answer below: a
        // record of two `Int`s with a `Drop` of its own still has to run it.
        if let (Some(decls), Some(interface)) = (self.decls, self.drop_interface()) {
            if decls.methods().declares(self.types, ty, interface) {
                return Ok(true);
            }
        }
        match self.types.kind(ty) {
            // `ty`'s §5: an erroneous type must not manufacture work any more
            // than it manufactures a diagnostic.
            TyKind::Error | TyKind::Unit => Ok(false),
            // A borrow releases nothing; the referent's own storage-dead point
            // is what releases it.
            TyKind::Borrowed { .. } => Ok(false),
            TyKind::Nullable(inner) => self.drop_runs_something(*inner, depth + 1),
            TyKind::Tuple(elements) => {
                for element in elements.clone() {
                    if self.drop_runs_something(element, depth + 1)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            // A trait object's drop goes through a vtable this backend does not
            // emit, so it is never inert — which is `descriptor::needs_drop`'s
            // answer too, and right here for the same reason it is too coarse
            // there: `Box of any I` and `borrowed any I` are one type to a
            // layout and two to a destructor.
            TyKind::Object { .. } => Ok(true),
            TyKind::Named { def, args } => {
                let def = *def;
                if !args.is_empty() {
                    return Ok(true);
                }
                match self.defs.get(def).kind {
                    DefKind::Record => {
                        let fields: Vec<Ty> = self
                            .declarations()?
                            .record(def)
                            .map(|record| record.fields.iter().map(|(_, ty)| *ty).collect())
                            .unwrap_or_default();
                        for field in fields {
                            if self.drop_runs_something(field, depth + 1)? {
                                return Ok(true);
                            }
                        }
                        Ok(false)
                    }
                    DefKind::Choice => {
                        let decls = self.declarations()?;
                        for variant in self.choice_variants(def) {
                            let Some(declared) = decls.variant(variant) else { return Ok(true) };
                            for payload in declared.payload.clone() {
                                if self.drop_runs_something(payload, depth + 1)? {
                                    return Ok(true);
                                }
                            }
                        }
                        Ok(false)
                    }
                    // A primitive, or a library type. The list is
                    // [`Lowerer::cg_ty`]'s own and is kept the same shape on
                    // purpose: a name that function can lay out as a scalar is
                    // a name that owns nothing, and every other name — `String`
                    // and `Array` and `Map` among them — is a refusal.
                    _ => Ok(!matches!(
                        self.defs.get(def).name.as_str(),
                        "Int"
                            | "I8"
                            | "I16"
                            | "I32"
                            | "I64"
                            | "U8"
                            | "U16"
                            | "U32"
                            | "U64"
                            | "F32"
                            | "F64"
                            | "Bool"
                            | "Char"
                    )),
                }
            }
            // A type parameter, `Self`, an associated type, a closure: nothing
            // here can see what is behind any of them.
            _ => Ok(true),
        }
    }

    /// Whether a type is the prelude's `String`, exactly.
    ///
    /// **Exactly, and not "a type whose layout is a `ScienceString`".** The
    /// layout of `String` is `{ Ptr(Raw), Usize, Usize }`, which is also the
    /// layout of `ScienceChars` and within one field of `ScienceArray`'s — so a
    /// predicate on the layout would hand `science_string_free` a value that is
    /// not a string, which frees a pointer with the wrong allocator's idea of
    /// its size. The name is the distinction that survives, and it is the same
    /// one [`Lowerer::cg_ty`] keys on for the same restricted reason: these are
    /// prelude builtins, and a real lowering keys on the `DefId`.
    fn is_string(&self, ty: Ty) -> bool {
        matches!(
            self.types.kind(ty),
            TyKind::Named { def, args }
                if args.is_empty()
                    && self.defs.get(*def).is_builtin()
                    && self.defs.get(*def).name == "String"
        )
    }

    /// The prelude's `Drop` interface, if this compilation has one.
    ///
    /// **Found by scanning the definition table rather than asked of
    /// `Prelude`**, because `Prelude`'s lookup is a closed list — *"a closed
    /// list rather than a general lookup, so that adding a dependency on a
    /// prelude name is an edit to this array and shows up in a diff"* — and
    /// `Drop` is not on it. That array is `science-types`', which this crate
    /// must not edit, and the scan asks exactly the question `Prelude::of`
    /// asks: a builtin definition, an interface, named `Drop`.
    ///
    /// `Methods::answers_for` warns that a builtin interface is one the index
    /// cannot speak for, and that is about the **negative** direction: the
    /// prelude registers no implementations of its own, so silence is not a no.
    /// What is read here is the positive direction, where the index is
    /// complete — a user's `Doc implements Drop:` is written in the crate and is
    /// therefore in `Methods::implemented`.
    fn drop_interface(&self) -> Option<DefId> {
        self.defs
            .iter()
            .find(|def| {
                def.is_builtin() && def.kind == DefKind::Interface && def.name == "Drop"
            })
            .map(|def| def.id)
    }

    /// The declaration table, or the refusal for a build that was given none.
    fn declarations(&self) -> Result<&'a Declarations, Unlowered> {
        self.decls.ok_or_else(|| {
            Unlowered::new(
                "a user-defined type: this build was given no declaration table to read its \
                 fields or variants from",
            )
        })
    }

    fn layout_of_ty(&self, ty: science_types::ty::Ty) -> Result<Layout, Unlowered> {
        Ok(layout_of(self.target, &self.cg_ty(ty)?))
    }

    /// Lower one crate's MIR.
    ///
    /// `bodies` is every MIR body in the crate, and **what is emitted is what
    /// `main` reaches**.
    ///
    /// **The decision.** The call graph is walked from `main` over
    /// [`science_mir::mir::Body::callees`] and a body outside the closure is
    /// dropped, not lowered and not refused.
    ///
    /// **The reason.** This used to refuse a program with a second function at
    /// all, which was honest while no second function *could* be emitted. Now
    /// that one can, the question is what to do with a function this backend
    /// still cannot lower — a generic one, a method — that the program declares
    /// and never calls. Refusing the whole program for it would refuse a
    /// program for instructions that contribute nothing to the image, and
    /// `examples/07_generics.science` is what that looks like: one unused
    /// generic makes the file unbuildable. Decision 42 puts the monomorphisation
    /// walk above this crate and `science_codegen::mono` is where a real one
    /// belongs; this is the reachability half of it and no more, so it makes no
    /// decision `mono` will have to unmake.
    ///
    /// **The cost, and it is a real one.** An error in an uncalled function is
    /// not reported by `sciencec build`, so a refusal a user would want to see
    /// is silent until something calls it. That is the ordinary cost of
    /// monomorphisation-on-demand and Rust pays it too; what makes it
    /// acceptable here is that `sciencec check` runs the whole front end over
    /// every function whether or not it is called, so the only diagnostics this
    /// hides are *this crate's* — the `SC0400`s that say a backend feature is
    /// missing.
    ///
    /// **Two signatures for one function is the hazard this avoids next**, so
    /// every reachable body's [`AbiSignature`] is built once, before any body is
    /// lowered, and both the definition and every call site read that one entry.
    /// A call site that classified the callee's return for itself is §9.2's
    /// finding with the compiler on both ends of it.
    pub fn lower_crate(&mut self, bodies: &[MirBody]) -> Result<Lowered, Unlowered> {
        let entry = bodies
            .iter()
            .find(|body| self.defs.get(body.def()).name == "main")
            .ok_or_else(|| Unlowered::new("a program with no `main`"))?;
        self.entry = Some(entry.def());
        let reachable = reachable_from(bodies, entry.def());

        for body in bodies {
            if !reachable.contains(&body.def()) {
                continue;
            }
            let signature = self.science_signature(body)?;
            self.science.insert(body.def(), signature);
        }

        let mut definitions = Vec::new();
        for body in bodies {
            if !reachable.contains(&body.def()) {
                continue;
            }
            definitions.push(self.lower_body(body)?);
        }
        let main_signature = self
            .science
            .get(&entry.def())
            .cloned()
            .expect("`main` is reachable from itself");
        definitions.push(self.lower_c_main(&main_signature)?);
        Ok(self.finish(definitions))
    }

    /// The classified signature of one Science function, derived from its MIR.
    ///
    /// **From MIR and not from `Declarations`, and the asymmetry with
    /// [`Lowerer::declare_foreign`] is deliberate.** An `extern "C"` function
    /// has no body, so its parameter types reach codegen through the
    /// declaration table or through nothing. A Science function has a body, and
    /// the body's locals `_1..=arg_count` *are* its parameters with the types
    /// the checker gave them — already substituted, already in order, and
    /// already what every `Assign` in the body was lowered against. Reading the
    /// declaration instead would introduce a second source for a fact the body
    /// carries, and the two disagreeing is a signature mismatch that links.
    ///
    /// `Declarations` is still consulted, for the two shapes MIR cannot
    /// distinguish and this backend cannot emit: a generic function and a
    /// method.
    ///
    /// **No parameter attributes are emitted, and that is finding 14 rather
    /// than laziness.** Decision 24 wants `readonly nocapture` on a shared
    /// borrow and `noalias nocapture` on an exclusive one, and §4.4 calls
    /// `noalias` *"the single place in the language where a bug in region
    /// inference produces a wrong answer rather than a missed error"*. Its own
    /// mitigation is `--no-noalias`, which `TargetConfig` carries and **this
    /// function cannot see**: a [`Lowerer`] is built from a `Triple`, and the
    /// flag arrives at [`crate::emit`] one layer below. Emitting the attribute
    /// with its escape hatch unreachable is the worst of the three options; the
    /// cost of emitting none is an optimisation, which is the same trade
    /// [`runtime_signature`] already takes for the same kind of reason.
    fn science_signature(&mut self, body: &MirBody) -> Result<AbiSignature, Unlowered> {
        let def = body.def();
        let name = self.defs.get(def).name.clone();
        if let Some(decls) = self.decls {
            if let Some(signature) = decls.signature(def) {
                if !signature.generics.is_empty() {
                    return Err(Unlowered::new(format!(
                        "a call to the generic function `{name}`, which nothing has \
                         monomorphised: Decision 42 puts the walk above this crate and no phase \
                         runs it yet"
                    )));
                }
                if signature.self_param.is_some() {
                    return Err(Unlowered::new(format!(
                        "a call to the method `{name}`: its receiver is a `Self` this crate \
                         cannot resolve to a concrete type, and Decision 11's method lookup does \
                         not put what it found in the tree"
                    )));
                }
            }
        }
        let ret_layout = self.layout_of_ty(body.local_decl(mir::Local::from_index(0)).ty)?;
        let mut params = Vec::new();
        for local in body.params() {
            let decl = body.local_decl(local);
            let param_name = decl
                .def()
                .map(|def| self.defs.get(def).name.clone())
                .unwrap_or_else(|| format!("a{}", local.index()));
            params.push((param_name, self.layout_of_ty(decl.ty)?, ParamAttrs::default()));
        }
        Ok(AbiSignature::science(self.target, self.symbol_of(def), ret_layout, params))
    }

    /// Decision 16's mangled symbol for one definition.
    ///
    /// The path is `DefTable::path_of`'s, split on its separator — **except for
    /// the entry point, which is `_S4main` whatever module it is in**, and the
    /// exception is finding 16 rather than a convenience.
    ///
    /// **What `path_of` actually returns for a script is `hello.main`.** The
    /// crate root is unnamed and contributes nothing, but the *file's* module
    /// is not the crate root: `resolve_module` names it after the file, so the
    /// first path component of every definition in a single-file program is
    /// that file's stem. Mangled, that makes `hello.science` and a byte-identical
    /// copy called `prog.science` produce **different symbols for the same
    /// source** — which is precisely the property Decision 16 gives as its own
    /// reason for refusing hashes: *"a hash of a path … is stable only if the
    /// path is, and the path is the thing that is not"*, and
    /// *"deterministic from the source alone"*.
    ///
    /// **It is not repaired here, because it is not this crate's to repair.**
    /// Whether a file's stem is part of its module path is
    /// `type-checking-and-mir.md`'s question and `science-resolve`'s answer;
    /// changing the mangling to hide it would make two different modules'
    /// functions share a symbol, which is `SC0404` at best and a silently
    /// wrong call at worst. What is done instead is the one case where the
    /// answer is already fixed by another note: `script-mode.md` §2.3 and this
    /// crate's own `lower_c_main` both name the entry point `_S4main`, and
    /// `science-codegen`'s `tests/stage_one.rs` pins it, so the entry keeps
    /// that symbol and every other definition carries the path it really has.
    fn symbol_of(&self, def: DefId) -> String {
        if self.entry == Some(def) {
            return mangle(&MonoKey::plain(&["main"]));
        }
        let path = self.defs.path_of(def);
        let components: Vec<&str> = path.split('.').filter(|part| !part.is_empty()).collect();
        if components.is_empty() {
            return mangle(&MonoKey::plain(&[self.defs.get(def).name.as_str()]));
        }
        mangle(&MonoKey::plain(&components))
    }

    /// The literals and declarations interned so far, with `definitions`,
    /// as one module.
    ///
    /// **Public for the same caller [`Lowerer::intern_literal`] is public for**:
    /// `tests/exit_code.rs` writes a `_S4main` that no Science program can
    /// produce and needs the rest of the module — the literals, the runtime
    /// declarations [`Lowerer::lower_c_main`] added — assembled the way
    /// [`Lowerer::lower_crate`] assembles it. Sharing the function rather than
    /// the recipe is what keeps the test from drifting: a declaration this
    /// lowerer adds and the test forgot is `main` calling a symbol the module
    /// does not declare, which the emitter reports as an internal error at the
    /// end of a build rather than as a missing line in a test.
    pub fn finish(&mut self, mut definitions: Vec<(AbiSignature, ExtBody)>) -> Lowered {
        // Decision 4: *"every monomorphised item is emitted in sorted order of
        // its mangled symbol name."* One line, and it closes Gate J's named
        // hazard by construction rather than by testing.
        definitions.sort_by(|a, b| a.0.symbol.cmp(&b.0.symbol));
        // Decision 28's source order, filtered to the blocks a call actually
        // reached. `library_order` is the order; `libraries_used` is the set.
        let used = std::mem::take(&mut self.libraries_used);
        let libraries: Vec<String> = self
            .library_order
            .iter()
            .filter(|name| used.contains(name))
            .cloned()
            .collect();
        Lowered {
            literals: std::mem::take(&mut self.literals),
            declarations: std::mem::take(&mut self.declarations),
            definitions,
            libraries,
            foreign: std::mem::take(&mut self.declared_foreign),
        }
    }

    /// A slot codegen invents, in the entry block.
    fn temp(&self, ctx: &mut BodyCtx, layout: Layout) -> LocalId {
        let id = LocalId(ctx.next_temp);
        ctx.next_temp += 1;
        ctx.entry.push(ExtInst::Above(Inst::Alloca { local: id, layout: layout.clone() }));
        ctx.layouts.insert(id.0, layout);
        id
    }

    fn lower_body(&mut self, body: &MirBody) -> Result<(AbiSignature, ExtBody), Unlowered> {
        let return_local = mir::Local::from_index(0);
        let return_id = LocalId(return_local.index() as u32);
        let cached = self.science.get(&body.def()).cloned();
        let sig = match cached {
            Some(existing) => existing,
            None => self.science_signature(body)?,
        };
        let params: Vec<mir::Local> = body.params().collect();
        if params.len() != sig.params.len() {
            return Err(Unlowered::new(format!(
                "a function whose MIR declares {} parameter(s) where its signature classifies {}",
                params.len(),
                sig.params.len()
            )));
        }

        // Decision 8: every local is an `alloca` in the entry block. MIR's
        // `StorageLive`/`StorageDead` are not modelled — an `alloca` lives for
        // the frame — which is correct and costs stack: two locals with
        // disjoint live ranges get two slots where LLVM's stack colouring would
        // have shared one. `-O2`'s `StackColoring` pass recovers it only when
        // it is given `llvm.lifetime` intrinsics, which is what those two
        // statements should become and do not yet.
        let mut ctx = BodyCtx {
            layouts: BTreeMap::new(),
            untyped: Vec::new(),
            entry: Vec::new(),
            prologue: Vec::new(),
            discriminants: BTreeMap::new(),
            ret: sig.ret.clone(),
            next_value: 0,
            next_temp: body.local_count() as u32,
        };
        for (local, decl) in body.locals() {
            let id = LocalId(local.index() as u32);
            // **`print`'s result temporary has no type, and that is a finding
            // about the front end rather than about this program.**
            // `science-resolve`'s `builtins.rs` leaves `print` and `write`
            // deliberately undeclared — it wrote the signature, measured seven
            // false positives on the corpus, and withdrew it — so the checker
            // has no return type for the call and gives the temporary
            // `Ty::ERROR`, **with no diagnostic**, which is §5's "the mistake
            // has already been reported" rule firing on a mistake nobody made.
            //
            // `hello, world` is therefore a program that type-checks clean and
            // arrives here with an untypeable local in it. An `alloca` cannot
            // be emitted for it: `layout_of` needs a `CgTy` and there is none.
            //
            // It is *skipped* rather than refused because nothing reads it —
            // `print` returns `void`, the temporary is dead on arrival, and
            // refusing would make the one program stage 1 exists to compile
            // uncompilable over a slot with nothing in it. Any statement that
            // does touch it is refused below, by name, so the skip cannot
            // become a silent wrong answer. The fix is above this crate: a
            // signature for `print`, or a `Ty::UNIT` for a call to a builtin
            // that has none.
            if matches!(self.types.kind(decl.ty), TyKind::Error) {
                ctx.untyped.push(id);
                continue;
            }
            let layout = self.layout_of_ty(decl.ty)?;
            // `_0` of an `sret` function is the **caller's** slot, not a slot of
            // this frame. `crate::emit::ExtInst::ReturnSlot` is the account and
            // the bug it closes; the short version is that an `alloca` here is a
            // private copy nothing ever returns.
            if id == return_id && sig.ret.is_sret() {
                ctx.entry.push(ExtInst::ReturnSlot { local: id, layout: layout.clone() });
            } else if let Some(index) = params.iter().position(|param| *param == local) {
                // A parameter's local, and the two classes are two different
                // bindings rather than one binding and a copy.
                match sig.params[index].class {
                    // Decision 22: *"by pointer to a caller-owned slot"*, so
                    // the local **is** that slot. An `alloca` here would be a
                    // private copy the caller never sees written, which is
                    // `ExtInst::ReturnSlot`'s bug on the argument side.
                    ArgClass::IndirectByPointer => ctx.entry.push(ExtInst::ParamSlot {
                        local: id,
                        index: index as u32,
                        layout: layout.clone(),
                    }),
                    // A register parameter gets Decision 8's `alloca` and one
                    // store in the prologue, which is what every C compiler
                    // emits at `-O0` and what `mem2reg` removes at `-O2`.
                    ArgClass::Direct => {
                        ctx.entry.push(ExtInst::Above(Inst::Alloca {
                            local: id,
                            layout: layout.clone(),
                        }));
                        ctx.prologue.push(ExtInst::Above(Inst::Store {
                            local: id,
                            value: Operand::Param(index as u32),
                        }));
                    }
                    // §3.2: a zero-sized argument is *"passed as nothing"*, so
                    // there is nothing to store. The slot still exists, because
                    // the body may still name the local.
                    ArgClass::Ignore => ctx.entry.push(ExtInst::Above(Inst::Alloca {
                        local: id,
                        layout: layout.clone(),
                    })),
                }
            } else {
                ctx.entry
                    .push(ExtInst::Above(Inst::Alloca { local: id, layout: layout.clone() }));
            }
            ctx.layouts.insert(id.0, layout);
        }

        let mut blocks: Vec<ExtBlock> = Vec::new();
        for (id, block) in body.blocks() {
            let mut insts: Vec<ExtInst> = Vec::new();
            for statement in &block.statements {
                self.lower_statement(body, &mut ctx, &statement.kind, &mut insts)?;
            }
            let terminator =
                self.lower_terminator(body, &mut ctx, &block.terminator.kind, &mut insts)?;
            blocks.push(ExtBlock {
                id: BlockId(id.index() as u32),
                label: format!("bb{}", id.index()),
                insts,
                terminator,
            });
        }
        // Every `alloca` in front of the entry block's own instructions, in one
        // place, however late in the walk the slot was invented — and then the
        // prologue, which stores each register parameter into the slot the
        // loop above reserved for it. The order is load-bearing in one
        // direction only: a store needs its `alloca`, and an `alloca` invented
        // late by `Lowerer::temp` still lands in front of every store.
        let mut entry = ctx.entry;
        entry.extend(ctx.prologue);
        if let Some(first) = blocks.first_mut() {
            let body_insts = std::mem::replace(&mut first.insts, entry);
            first.insts.extend(body_insts);
        }
        Ok((sig, ExtBody { blocks }))
    }

    fn lower_statement(
        &mut self,
        body: &MirBody,
        ctx: &mut BodyCtx,
        kind: &StatementKind,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        match kind {
            // Not modelled; see `lower_body`.
            StatementKind::StorageLive(_) | StatementKind::StorageDead(_) => Ok(()),
            // Decision 26's drop flags and two-phase activations are statements
            // a code generator may ignore only because nothing reachable here
            // has a drop that is conditional or a two-phase borrow. Refused
            // rather than ignored, because ignoring a `SetDropFlag` is a double
            // free.
            StatementKind::SetDropFlag { .. } => {
                Err(Unlowered::new("a conditionally moved value, which needs a drop flag"))
            }
            // **A `Nop`, which is what `science-mir`'s §6 says it is**:
            // *"the cost is one statement per two-phase borrow, which codegen
            // treats as a `Nop`"*. The activation point exists so that region
            // inference can say what may happen between a reservation and the
            // call that consumes it; by the time a body reaches this crate that
            // question has been answered and there is no instruction to emit
            // for the answer.
            //
            // This was a refusal until `f"…"` produced one. The refusal was
            // right while nothing did — an unrun shape is an unrun shape — and
            // it is not the same case as `SetDropFlag` above, which is refused
            // because *ignoring* it is a double free. Ignoring an activation
            // changes no machine state at all.
            StatementKind::Activate(_) => Ok(()),
            StatementKind::Nop => Ok(()),
            // **Before the untyped check, not after it.** A discriminant read
            // writes a local `lower_body` skipped — `science-mir` gives the
            // temporary `Ty::ERROR` on purpose, because *"which variant a value
            // holds has no Science type at all"* — so the ordinary path would
            // refuse every `match` in the language with `UNTYPED`, naming a
            // front-end hole for something the front end did deliberately.
            StatementKind::Assign { place, rvalue: Rvalue::Discriminant(source) } => {
                self.lower_discriminant(body, ctx, place, source, insts)
            }
            StatementKind::Assign { place, rvalue } => {
                if !place.projection.is_empty() {
                    return self.lower_projected_assign(body, ctx, place, rvalue, insts);
                }
                let local = LocalId(place.local.index() as u32);
                if ctx.untyped.contains(&local) {
                    // The skipped slot of `lower_body`, now written to. See the
                    // comment there: the skip is only sound while nothing
                    // touches the local, and this is where that stops being
                    // true.
                    return Err(Unlowered::new(UNTYPED));
                }
                let layout = ctx.layout(local)?.clone();
                let ty = body.local_decl(place.local).ty;
                self.lower_rvalue(body, ctx, rvalue, local, &layout, ty, insts)
            }
        }
    }

    /// An assignment whose destination is a field, a tuple element or a
    /// variant's payload.
    ///
    /// **The value is computed into a slot codegen invents and then copied in,
    /// and the copy is the cost.** [`Lowerer::lower_rvalue`] writes into a
    /// `LocalId`, and every one of its ten cases ends in `Inst::Store` to that
    /// local; giving each of them a second, address-shaped spelling would be
    /// ten more places for a width to be wrong, in a crate whose §3 records two
    /// findings that were exactly that. One extra `alloca` and one extra
    /// load-store pair per projected assignment is what the uniformity costs,
    /// and `-O2`'s `SROA` removes both — the slot has one store and one load and
    /// never has its address escape, which is the shape that pass exists for.
    /// At `-O0` it stands, and `-O0` is alloca-heavy by Decision 8 anyway.
    fn lower_projected_assign(
        &mut self,
        body: &MirBody,
        ctx: &mut BodyCtx,
        place: &mir::Place,
        rvalue: &Rvalue,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        let ty = place
            .projection
            .last()
            .map(|projection| projection.ty())
            .expect("a projected place has at least one projection");
        let (address, layout) = self.place_address(ctx, place, insts)?;
        let slot = self.temp(ctx, layout.clone());
        self.lower_rvalue(body, ctx, rvalue, slot, &layout, ty, insts)?;
        let value = ctx.value();
        insts.push(ExtInst::Above(Inst::Load { dest: value, local: slot }));
        insts.push(ExtInst::StoreAt {
            address: Operand::Value(address),
            layout,
            value: Operand::Value(value),
        });
        Ok(())
    }

    /// §2.3's projection table, as a chain of addresses.
    ///
    /// Returns the address of the place and the layout of what is there. The
    /// base is [`ExtInst::LocalAddr`] and every step after it is
    /// [`ExtInst::FieldAddr`] at an offset `science-codegen`'s layout computed —
    /// **except `deref`, which is the one row of that table that is not an
    /// offset**: it loads a pointer and that pointer becomes the new base.
    ///
    /// Decision 9 applies and is visible here: *"codegen lowers each place
    /// expression once per use and performs no common-subexpression
    /// elimination"*, so two reads of `doc.title` build two GEPs and LLVM's GVN
    /// is what merges them.
    fn place_address(
        &mut self,
        ctx: &mut BodyCtx,
        place: &mir::Place,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(ValueId, Layout), Unlowered> {
        let local = LocalId(place.local.index() as u32);
        if ctx.untyped.contains(&local) {
            return Err(Unlowered::new(UNTYPED));
        }
        let mut layout = ctx.layout(local)?.clone();
        let mut address = ctx.value();
        insts.push(ExtInst::LocalAddr { dest: address, local });

        for projection in &place.projection {
            // Every arm answers with a byte offset and the layout of what is
            // there, and the two are applied together below. `deref` is the one
            // that cannot: it replaces the base rather than adding to it, so it
            // does its own work and leaves the loop early.
            let (offset, next) = match projection {
                mir::Projection::Field { field, .. } => {
                    let (index, _) = self.field_index(*field)?;
                    let Repr::Aggregate { fields } = &layout.repr else {
                        return Err(Unlowered::new(
                            "a field of a value whose representation is not Decision 17's \
                             aggregate one",
                        ));
                    };
                    let found = fields.get(index).ok_or_else(|| {
                        Unlowered::new(
                            "a field whose index the layout does not have: the declaration and \
                             the layout disagree about how many fields this type has",
                        )
                    })?;
                    (found.offset, found.layout.clone())
                }
                mir::Projection::Downcast { variant, .. } => {
                    let (index, choice) = self.variant_index(*variant)?;
                    let Repr::Tagged { payload_offset, variants, .. } = &layout.repr else {
                        return Err(Unlowered::new(format!(
                            "a downcast to `{}.{}`, whose layout is Decision 19's niched one \
                             rather than Decision 18's tagged one: its payload is the whole value \
                             and not a field at an offset",
                            self.defs.get(choice).name,
                            self.defs.get(*variant).name
                        )));
                    };
                    let payload = variants
                        .get(index)
                        .and_then(|found| found.payload.clone())
                        .ok_or_else(|| {
                            Unlowered::new(format!(
                                "a downcast to `{}`, which carries no payload to project into",
                                self.defs.get(*variant).name
                            ))
                        })?;
                    (*payload_offset, payload)
                }
                mir::Projection::TupleField { index, .. } => {
                    let index = *index as usize;
                    match &layout.repr {
                        Repr::Aggregate { fields } => {
                            let found = fields.get(index).ok_or_else(|| {
                                Unlowered::new(format!(
                                    "element {index} of a value that has {} of them",
                                    fields.len()
                                ))
                            })?;
                            (found.offset, found.layout.clone())
                        }
                        // **A one-element payload is its element**, by
                        // `Lowerer::choice_ty`'s decision not to wrap it, so
                        // `Number(v)`'s `.0` is the value itself at offset 0.
                        // The `index == 0` guard is what keeps that from
                        // becoming "any element of anything is the whole
                        // thing".
                        _ if index == 0 => (0, layout.clone()),
                        _ => {
                            return Err(Unlowered::new(format!(
                                "element {index} of a value that is not an aggregate"
                            )));
                        }
                    }
                }
                // The row of §2.3's table that is not an offset: `load ptr`,
                // and the loaded pointer *is* the next base. The pointee's
                // layout comes from the projection's own `ty`, which
                // `science-mir` takes from the declaration rather than from the
                // expression.
                mir::Projection::Deref { ty } => {
                    if !matches!(layout.repr, Repr::Scalar(Scalar::Pointer(_))) {
                        return Err(Unlowered::new(
                            "a dereference of a value that is not a pointer",
                        ));
                    }
                    let pointee = self.layout_of_ty(*ty)?;
                    let loaded = ctx.value();
                    insts.push(ExtInst::LoadAt {
                        dest: loaded,
                        address: Operand::Value(address),
                        layout: layout.clone(),
                    });
                    address = loaded;
                    layout = pointee;
                    continue;
                }
                mir::Projection::Index { .. } => {
                    return Err(Unlowered::new(
                        "an index, which §2.4 makes a bounds check and then a \
                         `getelementptr`, and neither the check nor an `Array` value reaches \
                         this backend",
                    ));
                }
            };
            layout = next;
            let stepped = ctx.value();
            insts.push(ExtInst::FieldAddr {
                dest: stepped,
                base: Operand::Value(address),
                offset,
            });
            address = stepped;
        }
        Ok((address, layout))
    }

    /// A field's index in its record's declaration order, and the record.
    fn field_index(&self, field: DefId) -> Result<(usize, DefId), Unlowered> {
        let name = self.defs.get(field).name.clone();
        let owner = self.defs.get(field).parent.ok_or_else(|| {
            Unlowered::new(format!("the field `{name}`, which belongs to no record"))
        })?;
        let record = self.declarations()?.record(owner).ok_or_else(|| {
            Unlowered::new(format!(
                "the field `{name}` of `{}`, which the declaration table has no lowered field \
                 list for",
                self.defs.get(owner).name
            ))
        })?;
        let index = record
            .fields
            .iter()
            .position(|(def, _)| *def == field)
            .ok_or_else(|| Unlowered::new(format!("the field `{name}`, which its record's field \
                                                   list does not contain")))?;
        Ok((index, owner))
    }

    /// A variant's discriminant value, and its `choice`.
    ///
    /// Decision 18: *"discriminant values follow declaration order from zero"*,
    /// so the index **is** the value and [`Lowerer::choice_variants`] is what
    /// fixes it.
    fn variant_index(&self, variant: DefId) -> Result<(usize, DefId), Unlowered> {
        let name = self.defs.get(variant).name.clone();
        let choice = self.defs.get(variant).parent.ok_or_else(|| {
            Unlowered::new(format!("the variant `{name}`, which belongs to no `choice`"))
        })?;
        let index = self
            .choice_variants(choice)
            .iter()
            .position(|def| *def == variant)
            .ok_or_else(|| {
                Unlowered::new(format!(
                    "the variant `{name}`, which `{}` does not declare",
                    self.defs.get(choice).name
                ))
            })?;
        Ok((index, choice))
    }

    /// `_n = discriminant(place)`: §3.3's tag, loaded.
    ///
    /// **This is where the slot for the result is invented.** MIR's discriminant
    /// temporary is typed `Ty::ERROR` deliberately, so [`Lowerer::lower_body`]
    /// skipped it along with the genuinely untyped locals and there is no
    /// `alloca` for it. The width it needs is not a Science type at all — it is
    /// `IntTy::discriminant_for(variant count)`, which is a *layout* fact — so
    /// the slot cannot be reserved until the choice being read is known, which
    /// is here and nowhere earlier. The `alloca` goes into the entry block's
    /// list whatever block this statement is in, which is `BodyCtx::entry`'s
    /// whole reason for existing.
    ///
    /// **A niched layout is refused rather than read.** Decision 19's enum *is*
    /// its payload, so its "discriminant" is a comparison against the niche
    /// values and not a field; answering with the raw pointer would make every
    /// `switch` branch on an address, which verifies — a pointer is not an
    /// integer, so in fact it does not, and the refusal is here so the message
    /// names the construct rather than the instruction.
    fn lower_discriminant(
        &mut self,
        body: &MirBody,
        ctx: &mut BodyCtx,
        place: &mir::Place,
        source: &mir::Place,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        if !place.projection.is_empty() {
            return Err(Unlowered::new("a discriminant read written through a field"));
        }
        if !source.projection.is_empty() {
            return Err(Unlowered::new(
                "a discriminant read through a field or an index: the tag is loaded from a \
                 local's own address and this instruction set has no typed load from a computed \
                 one",
            ));
        }
        let source_ty = body.local_decl(source.local).ty;
        let choice = match self.types.kind(source_ty) {
            TyKind::Named { def, .. } if self.defs.get(*def).kind == DefKind::Choice => *def,
            _ => {
                return Err(Unlowered::new(
                    "a discriminant read of a value that is not a `choice`: a `T?`'s presence \
                     test is `Rvalue::IsPresent` and a niched layout has no tag to load",
                ));
            }
        };
        let layout = self.layout_of_ty(source_ty)?;
        let Repr::Tagged { tag, .. } = &layout.repr else {
            return Err(Unlowered::new(format!(
                "a `match` over `{}`, whose layout is Decision 19's niched one rather than \
                 Decision 18's tagged one: its discriminant is a comparison against the niche \
                 and not a field to load",
                self.defs.get(choice).name
            )));
        };
        let tag_layout = layout_of(self.target, &CgTy::Int(*tag));
        let dest = LocalId(place.local.index() as u32);
        match ctx.layouts.get(&dest.0) {
            Some(existing) if *existing == tag_layout => {}
            Some(_) => {
                return Err(Unlowered::new(
                    "one local holding the discriminants of two `choice`s of different widths",
                ));
            }
            None => {
                ctx.untyped.retain(|untyped| *untyped != dest);
                ctx.entry.push(ExtInst::Above(Inst::Alloca {
                    local: dest,
                    layout: tag_layout.clone(),
                }));
                ctx.layouts.insert(dest.0, tag_layout);
            }
        }
        ctx.discriminants.insert(dest.0, choice);

        let source_id = LocalId(source.local.index() as u32);
        ctx.layout(source_id)?;
        let value = ctx.value();
        insts.push(ExtInst::LoadTag { dest: value, local: source_id });
        insts.push(ExtInst::Above(Inst::Store { local: dest, value: Operand::Value(value) }));
        Ok(())
    }

    /// One `Assign`'s right-hand side, ending in a store into `dest`.
    #[allow(clippy::too_many_arguments)]
    fn lower_rvalue(
        &mut self,
        body: &MirBody,
        ctx: &mut BodyCtx,
        rvalue: &Rvalue,
        dest: LocalId,
        layout: &Layout,
        ty: Ty,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        match rvalue {
            Rvalue::Use(mir::Operand::Const(Constant::Literal(Literal::Null))) => {
                self.store_null(dest, layout, insts)
            }
            Rvalue::Use(mir::Operand::Const(Constant::Unit)) => Ok(()),
            // A string literal is Decision 15's call, written into the
            // destination's own slot rather than into an invented one: the
            // binding *is* the `String`.
            Rvalue::Use(mir::Operand::Const(Constant::Literal(Literal::Str(text)))) => {
                self.build_string(text, dest, insts)
            }
            Rvalue::Use(operand) => {
                let value = self.lower_operand(ctx, operand, Some(layout), insts)?;
                insts.push(ExtInst::Above(Inst::Store { local: dest, value }));
                Ok(())
            }
            Rvalue::Unary { op, operand } => {
                let operand_ty = self.operand_ty(body, operand).unwrap_or(ty);
                let scalar = self.scalar_of(operand_ty)?;
                let operand_layout = layout_of(self.target, &self.cg_ty(operand_ty)?);
                let value = self.typed_operand(ctx, operand, &operand_layout, insts)?;
                let result = ctx.value();
                match op {
                    UnaryOp::Neg => {
                        if !matches!(scalar, Scalar::Int(_) | Scalar::Float(_)) {
                            return Err(Unlowered::new("a negation of something not numeric"));
                        }
                        insts.push(ExtInst::Neg { dest: result, operand: value });
                    }
                    // **`not x` is `x == 0` and needs no instruction of its
                    // own.** §3.1 gives `Bool` two forms, `i1` in a register
                    // and `i8` in memory, and a bitwise complement is wrong on
                    // the second: `not` of a stored `false` would be `0xff`,
                    // which is a bit pattern no `Bool` may hold, which every
                    // later `trunc` reads as `true`, and which nothing reports.
                    // A comparison against zero is right at both widths and is
                    // total.
                    UnaryOp::Not => {
                        if !matches!(scalar, Scalar::Bool) {
                            return Err(Unlowered::new("a `not` of something that is not `Bool`"));
                        }
                        insts.push(ExtInst::Above(Inst::Cmp {
                            dest: result,
                            op: CmpOp::Eq,
                            signed: false,
                            lhs: value,
                            rhs: Operand::ConstInt(0),
                        }));
                    }
                }
                insts.push(ExtInst::Above(Inst::Store {
                    local: dest,
                    value: Operand::Value(result),
                }));
                Ok(())
            }
            Rvalue::Binary { op, lhs, rhs } => {
                self.lower_binary(body, ctx, *op, lhs, rhs, dest, ty, insts)
            }
            // §5.5's `?`: *"`?` evaluates to a `Bool` and never to the
            // value"*, so this is one comparison and no branch.
            Rvalue::IsPresent(operand) => {
                self.lower_is_present(body, ctx, operand, dest, insts)
            }
            // Decision 6's `T` into `T?`. The other seven coercions box, build
            // a vtable or copy through a borrow, and each is refused by name in
            // [`Lowerer::lower_coercion`].
            Rvalue::Coerce { operand, coercion, .. } => {
                self.lower_coercion(body, ctx, *coercion, operand, dest, layout, insts)
            }
            Rvalue::Record { def, fields } => {
                self.lower_record(ctx, *def, fields, dest, layout, insts)
            }
            Rvalue::Tuple(elements) => self.lower_tuple(ctx, elements, dest, layout, insts),
            Rvalue::Variant { variant, payload } => {
                self.lower_variant(ctx, *variant, payload, dest, layout, insts)
            }
            // **A borrow is an address, stored.** §2.3's projection table is
            // already a chain of addresses and `Projection::Deref` is already
            // the row that loads one back, so the only thing missing was the
            // statement that *takes* one — which is `LocalAddr` plus the
            // `FieldAddr`s the place asks for, and then one store.
            //
            // **The destination's slot must be a pointer, and it is checked
            // rather than assumed.** `science-mir` gives the temporary the type
            // `borrowed T`, which `cg_ty` makes a `CgTy::Ptr`; a destination
            // whose layout is anything else means the reference and its slot
            // disagree about width, and storing an eight-byte address into a
            // narrower slot is §3's finding 12 — *"a store whose value is wider
            // than its slot is legal IR"* — with a different value in it. It
            // verifies. It is refused here instead.
            //
            // **Nothing about the loan's *kind* reaches the IR.** Shared,
            // exclusive and two-phase are one machine instruction, and the
            // difference between them is a question `science-regions` answered
            // before this crate ran. `noalias` on a `mutable borrowed`
            // parameter is the optimisation that would read the kind, and
            // Decision 22 does not ask for it.
            // §5.1's `as`. [`Lowerer::lower_cast`] is the whole of it and
            // its note is where the core spec's four sentences about numeric
            // conversion are read out.
            Rvalue::Cast { operand, from, ty: target } => {
                self.lower_cast(ctx, operand, *from, *target, dest, insts)
            }
            Rvalue::Ref { place, .. } => {
                if !matches!(layout.repr, Repr::Scalar(Scalar::Pointer(_))) {
                    return Err(Unlowered::new(
                        "a borrow stored into a slot that is not a pointer: the reference's own \
                         type and the slot laid out for it disagree",
                    ));
                }
                let (address, _) = self.place_address(ctx, place, insts)?;
                insts.push(ExtInst::Above(Inst::Store {
                    local: dest,
                    value: Operand::Value(address),
                }));
                Ok(())
            }
            other => Err(Unlowered::new(describe_rvalue(other))),
        }
    }

    /// §5.1's `as`, for every pair of scalars the language has.
    ///
    /// # What the notes say, and what they do not
    ///
    /// The core spec §5.1 is the authority and it decides three of the four
    /// questions outright:
    ///
    /// - *"No implicit numeric conversion; `as` is always written, including
    ///   where it loses precision."* So a narrowing cast is **legal**, and the
    ///   language has already decided that the loss is the author's to accept.
    /// - *"`as` from float to integer saturates, and NaN becomes zero."* That
    ///   is [`ConvOp::FloatToIntSat`], and the reason it is a call to an
    ///   intrinsic rather than one instruction is at
    ///   [`crate::emit::LlvmBackend::saturating_float_to_int`].
    /// - *"Integer overflow panics in debug builds and wraps in release, as
    ///   Rust does."* That sentence is about **arithmetic** and not about `as`;
    ///   `codegen-and-linking.md` has no cast section at all, and §4.6's
    ///   operator table stops at the operators.
    ///
    /// # The decision, where nothing said
    ///
    /// **A narrowing integer cast truncates, silently, in two's complement.**
    /// `300 as U8` is `44` and `-1i32 as U8` is `255`, with no check and no
    /// diagnostic.
    ///
    /// **The reason** is that §5.1 names Rust twice in the same paragraph and
    /// says of `as` that it is written *"including where it loses precision"* —
    /// a sentence that only means anything if a lossy cast is a thing the
    /// language performs rather than refuses. The alternatives were a panic
    /// (which would make `as` fallible, and §5.1 gives it no error type and no
    /// second return) and saturation (which is the float rule, and applying it
    /// to integers would make `-1i32 as U8` be `0`, silently turning a bit
    /// pattern the author was probably reinterpreting into a different number).
    ///
    /// **The cost, and it is the ordinary one.** A narrowing cast is the one
    /// place in F0 where a value changes with nothing in the source to mark it
    /// beyond the word `as` itself: `f"{total as U8}"` on a total of 300 prints
    /// `44`, and no phase says anything. That is the price of the sentence
    /// above, it is Rust's price, and the mitigation the language has is that
    /// the conversion cannot happen without the author writing `as`.
    ///
    /// **The signedness of an extension is the *source's*.** `-1i32 as I64` is
    /// `-1` and `4294967295u32 as I64` is `4294967295`, and those are the same
    /// thirty-two bits. LLVM integers carry no sign, so this is the one fact
    /// about a cast that cannot be recovered below this function, which is why
    /// `science-mir`'s `Rvalue::Cast` carries the source type at all.
    ///
    /// # What is refused, and why each one
    ///
    /// | Cast | Answer |
    /// |---|---|
    /// | integer or `Char` to `Bool` | refused: §3.1 makes `Bool` a byte with **two** valid values, and a `trunc` from `i64` to `i8` produces 254 others — a `Bool` holding `2` is a value every later `trunc i8 to i1` reads as `true` and no phase reports |
    /// | `Bool` or `Char` to a float | refused: Rust does not have it either, and inventing `true as F64 == 1.0` is a decision about what a `Bool` *is* that §5.1 does not take |
    /// | a float to `Char` | refused: a `Char` is a Unicode scalar value, and *"saturating"* has no meaning on a set with a hole in it — `0xD800`…`0xDFFF` are not scalar values |
    /// | an integer wider than `U8` to `Char` | refused: `U8 as Char` is total, and every other width can produce a surrogate or a value above `0x10FFFF`, which needs a validity check §5.1 does not specify and this crate must not invent |
    /// | anything non-scalar | refused, naming both types |
    ///
    /// `U8 as Char` **is** accepted, as a zero extension, because it is the one
    /// integer-to-`Char` conversion that cannot fail: every byte is a Unicode
    /// scalar value.
    fn lower_cast(
        &mut self,
        ctx: &mut BodyCtx,
        operand: &mir::Operand,
        from: Ty,
        target: Ty,
        dest: LocalId,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        let from_layout = self.layout_of_ty(from)?;
        let to_layout = self.layout_of_ty(target)?;
        let describe = || {
            format!(
                "a cast from `{}` to `{}`",
                self.types.render(self.defs, from),
                self.types.render(self.defs, target)
            )
        };
        let (Repr::Scalar(source), Repr::Scalar(destination)) =
            (&from_layout.repr, &to_layout.repr)
        else {
            return Err(Unlowered::new(format!(
                "{}, where §5.1's `as` is defined on scalars and one of these is an aggregate",
                describe()
            )));
        };
        let op = conversion(*source, *destination, self.target)
            .ok_or_else(|| Unlowered::new(format!("{}, which §5.1 does not define", describe())))?;
        // The operand is materialised at the **source's** layout, always, which
        // is finding 12's repair one instruction earlier: a constant carries no
        // width, and `ExtInst::Convert` refuses one for that reason rather than
        // giving it a default and truncating from a width nobody chose.
        let value = self.typed_operand(ctx, operand, &from_layout, insts)?;
        let value = match op {
            // The same bits under a different name: `I64 as U64`, `Int as I64`.
            // LLVM integers are signless, so there is no instruction to emit
            // and a `bitcast` between two `i64`s is not one either.
            None => value,
            Some(op) => {
                let converted = ctx.value();
                insts.push(ExtInst::Convert {
                    dest: converted,
                    op,
                    value,
                    from: from_layout.clone(),
                    to: to_layout.clone(),
                });
                Operand::Value(converted)
            }
        };
        insts.push(ExtInst::Above(Inst::Store { local: dest, value }));
        Ok(())
    }

    /// §5.5's `?`, which is one comparison.
    ///
    /// **Both of Decision 6's representations, and they ask different
    /// questions.** A tagged nullable is present when its discriminant is not
    /// the payload-free variant's; a niched one is present when its niche
    /// scalar is not the encoded value, which in F0 is always the null pointer.
    /// The discriminant is read off the layout in both cases rather than
    /// written as a constant, so the two agree with [`Lowerer::store_null`] by
    /// construction rather than by matching numbers.
    ///
    /// **§3.4's rule holds here too.** The niched read is
    /// [`ExtInst::LoadNiche`] and not `Inst::Load`, because for `(any I)?` the
    /// whole type is two words and *"the vtable slot of a null trait object is
    /// undefined and codegen must never load it — including on the path that
    /// tests for null"*. That path is this one.
    fn lower_is_present(
        &mut self,
        body: &MirBody,
        ctx: &mut BodyCtx,
        operand: &mir::Operand,
        dest: LocalId,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        let place = operand.place().ok_or_else(|| {
            Unlowered::new(
                "a `?` on a constant: the presence test reads a discriminant or a niche and needs \
                 a slot to read it from",
            )
        })?;
        if !place.projection.is_empty() {
            return Err(Unlowered::new(
                "a `?` on a field: the tag and the niche are read from a local's own address and \
                 this instruction set has no typed load from a computed one",
            ));
        }
        let ty = self
            .operand_ty(body, operand)
            .ok_or_else(|| Unlowered::new("a `?` on a value with no type"))?;
        let layout = self.layout_of_ty(ty)?;
        let local = LocalId(place.local.index() as u32);
        if ctx.untyped.contains(&local) {
            return Err(Unlowered::new(UNTYPED));
        }
        ctx.layout(local)?;
        let result = ctx.value();
        match &layout.repr {
            Repr::Tagged { variants, .. } => {
                let null = variants
                    .iter()
                    .find(|variant| variant.payload.is_none())
                    .ok_or_else(|| {
                        Unlowered::new("a `?` on a tagged type with no payload-free variant")
                    })?
                    .discriminant;
                let tag = ctx.value();
                insts.push(ExtInst::LoadTag { dest: tag, local });
                insts.push(ExtInst::Above(Inst::Cmp {
                    dest: result,
                    op: CmpOp::Ne,
                    signed: false,
                    lhs: Operand::Value(tag),
                    rhs: Operand::ConstInt(null as i128),
                }));
            }
            Repr::Niched { niche, niche_variants, .. } => {
                if niche.offset != 0 {
                    return Err(Unlowered::new("a `?` on a nullable whose niche is not at offset 0"));
                }
                // F0's niche offers exactly one value and it is the null
                // pointer. §3.4 generalises the rule — *"the `n` payload-free
                // variants take the first `n` values of the niche"* — and a
                // second value would need a comparison against an integer
                // rather than against `null`, which is a different instruction
                // and not one this has been asked for.
                if niche_variants.iter().any(|(_, value)| *value != 0) {
                    return Err(Unlowered::new(
                        "a `?` on a niche encoding a value that is not the null pointer",
                    ));
                }
                let data = ctx.value();
                insts.push(ExtInst::LoadNiche { dest: data, local });
                insts.push(ExtInst::Above(Inst::Cmp {
                    dest: result,
                    op: CmpOp::Ne,
                    signed: false,
                    lhs: Operand::Value(data),
                    rhs: Operand::Null,
                }));
            }
            _ => {
                return Err(Unlowered::new(format!(
                    "a `?` on `{}`, which is not a nullable type",
                    self.types.render(self.defs, ty)
                )));
            }
        }
        insts.push(ExtInst::Above(Inst::Store { local: dest, value: Operand::Value(result) }));
        Ok(())
    }

    /// Decision 6's widening, and a name for each of the seven this is not.
    ///
    /// **`Widen` writes the tag and then the payload, in that order, and
    /// neither is optional.** A tagged `T?` is a discriminant at offset 0 and a
    /// payload at `payload_offset`, and a widening that wrote only the payload
    /// would leave the tag holding whatever the slot held — which for a freshly
    /// `alloca`'d slot is a value that is `null` about half the time and is
    /// never reported. A niched `T?` **is** its payload, so there is no tag to
    /// write and the store is the ordinary one; that asymmetry is Decision 19
    /// and is why the two arms do not share a line.
    #[allow(clippy::too_many_arguments)]
    fn lower_coercion(
        &mut self,
        body: &MirBody,
        ctx: &mut BodyCtx,
        coercion: science_types::assign::Coercion,
        operand: &mir::Operand,
        dest: LocalId,
        layout: &Layout,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        use science_types::assign::Coercion;
        match coercion {
            // The types already agree and `assign`'s own note says *"nothing is
            // emitted"* — so what is left is the assignment underneath it.
            Coercion::Identity => {
                let value = self.typed_operand(ctx, operand, layout, insts)?;
                insts.push(ExtInst::Above(Inst::Store { local: dest, value }));
                Ok(())
            }
            Coercion::Widen => match &layout.repr {
                Repr::Tagged { payload_offset, variants, .. } => {
                    let present = variants
                        .iter()
                        .find(|variant| variant.payload.is_some())
                        .ok_or_else(|| {
                            Unlowered::new(
                                "a widening into a tagged type with no payload-carrying variant",
                            )
                        })?;
                    let payload = present
                        .payload
                        .clone()
                        .expect("the variant was found by having a payload");
                    let discriminant = present.discriminant;
                    let payload_offset = *payload_offset;
                    insts.push(ExtInst::StoreTag { local: dest, discriminant });
                    let base = ctx.value();
                    insts.push(ExtInst::LocalAddr { dest: base, local: dest });
                    let address = ctx.value();
                    insts.push(ExtInst::FieldAddr {
                        dest: address,
                        base: Operand::Value(base),
                        offset: payload_offset,
                    });
                    let value = self.typed_operand(ctx, operand, &payload, insts)?;
                    insts.push(ExtInst::StoreAt {
                        address: Operand::Value(address),
                        layout: payload,
                        value,
                    });
                    Ok(())
                }
                // Decision 19: the enum *is* the payload, and a present value
                // is that payload written as itself. The niche value is the one
                // bit pattern the payload cannot hold, so writing the payload
                // is what makes it present — there is nothing else to say.
                Repr::Niched { payload, .. } => {
                    let payload = (**payload).clone();
                    let value = self.typed_operand(ctx, operand, &payload, insts)?;
                    insts.push(ExtInst::Above(Inst::Store { local: dest, value }));
                    Ok(())
                }
                _ => Err(Unlowered::new(
                    "a widening into a type that is neither Decision 18's tagged nullable nor \
                     Decision 19's niched one",
                )),
            },
            Coercion::Box | Coercion::BoxThenWiden => Err(Unlowered::new(
                "a concrete error boxed into `any Error`, which allocates and fills a vtable: \
                 this backend emits no vtables at all",
            )),
            Coercion::Unsize | Coercion::UnsizeInBox => Err(Unlowered::new(
                "a borrow or a `Box` unsized into an interface object, which pairs the pointer \
                 with a vtable this backend does not emit",
            )),
            Coercion::Copy | Coercion::CopyThenWiden | Coercion::CopyWhenPresent => {
                let _ = body;
                Err(Unlowered::new(
                    "a `Copy` out of a borrow, which is a load through a pointer whose `Copy` \
                     bound nothing here checks",
                ))
            }
        }
    }

    /// A record literal: Decision 17's fields, written one at a time.
    ///
    /// **The order the fields are written in is MIR's and the order they are
    /// *placed* in is the declaration's**, and keeping the two apart is the
    /// whole of this function. `Rvalue::Record` carries `(field, operand)` pairs
    /// in the order the literal spelled them, which for named-argument
    /// construction is any order the author liked;
    /// `Declarations::record().fields` is declaration order, which is what
    /// Decision 17 lays out and what [`Lowerer::record_ty`] built the layout
    /// from. Each field is looked up by its `DefId`, so the literal's order
    /// changes the order of the stores and nothing else.
    ///
    /// **Every field must be named.** A literal that names fewer is refused
    /// rather than partially written: the bytes of an unwritten field are
    /// whatever the stack held, and a record with one of those in it is a wrong
    /// answer that nothing reports. The type checker already requires all of
    /// them, so this is a guard on a disagreement rather than a diagnostic a
    /// user will see.
    #[allow(clippy::too_many_arguments)]
    fn lower_record(
        &mut self,
        ctx: &mut BodyCtx,
        def: DefId,
        fields: &[(DefId, mir::Operand)],
        dest: LocalId,
        layout: &Layout,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        let name = self.defs.get(def).name.clone();
        let order: Vec<DefId> = self
            .declarations()?
            .record(def)
            .ok_or_else(|| {
                Unlowered::new(format!(
                    "a `{name}` literal, whose record the declaration table has no lowered field \
                     list for"
                ))
            })?
            .fields
            .iter()
            .map(|(field, _)| *field)
            .collect();
        let Repr::Aggregate { fields: places } = &layout.repr else {
            return Err(Unlowered::new(format!(
                "a `{name}` literal whose layout is not Decision 17's aggregate one"
            )));
        };
        if places.len() != order.len() {
            return Err(Unlowered::new(format!(
                "a `{name}` literal: its layout has {} field(s) and its declaration has {}",
                places.len(),
                order.len()
            )));
        }
        if fields.len() != order.len() {
            return Err(Unlowered::new(format!(
                "a `{name}` literal that names {} of its {} field(s): the rest of the slot would \
                 be whatever the stack held, which is a value nothing reports",
                fields.len(),
                order.len()
            )));
        }
        let places = places.clone();
        let base = ctx.value();
        insts.push(ExtInst::LocalAddr { dest: base, local: dest });
        for (field, operand) in fields {
            let index = order.iter().position(|def| def == field).ok_or_else(|| {
                Unlowered::new(format!(
                    "a `{name}` literal naming `{}`, which is not one of its fields",
                    self.defs.get(*field).name
                ))
            })?;
            let place = &places[index];
            let address = ctx.value();
            insts.push(ExtInst::FieldAddr {
                dest: address,
                base: Operand::Value(base),
                offset: place.offset,
            });
            let value = self.typed_operand(ctx, operand, &place.layout, insts)?;
            insts.push(ExtInst::StoreAt {
                address: Operand::Value(address),
                layout: place.layout.clone(),
                value,
            });
        }
        Ok(())
    }

    /// A tuple literal: each element written at its own offset.
    ///
    /// **One position, one order, and that is the whole difference from
    /// [`Lowerer::lower_record`].** A record literal has two orders — the
    /// order the author spelled the named arguments in and the order the
    /// declaration lays the fields out in — and that function's entire body is
    /// keeping them apart. `(1, 2.5)` has one: MIR's element order *is* the
    /// source order *is* the layout order, so this is a walk down the layout's
    /// field list with MIR's operands beside it and there is no lookup to get
    /// wrong.
    ///
    /// **The counts are checked rather than assumed**, for the reason
    /// `lower_record` gives: an element the layout has and the literal does not
    /// leaves those bytes holding whatever the stack held, and a tuple with one
    /// of those in it is a wrong answer nothing reports.
    fn lower_tuple(
        &mut self,
        ctx: &mut BodyCtx,
        elements: &[mir::Operand],
        dest: LocalId,
        layout: &Layout,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        let Repr::Aggregate { fields: places } = &layout.repr else {
            // The one tuple that is not an aggregate is the empty one, which
            // has no elements to write and which `Rvalue::Tuple` cannot even
            // spell: `()` is `Constant::Unit`.
            if elements.is_empty() {
                return Ok(());
            }
            return Err(Unlowered::new(
                "a tuple literal whose layout is not §3.2's aggregate one",
            ));
        };
        if places.len() != elements.len() {
            return Err(Unlowered::new(format!(
                "a tuple literal with {} element(s) whose layout has {}",
                elements.len(),
                places.len()
            )));
        }
        let places = places.clone();
        let base = ctx.value();
        insts.push(ExtInst::LocalAddr { dest: base, local: dest });
        for (place, operand) in places.iter().zip(elements) {
            let address = ctx.value();
            insts.push(ExtInst::FieldAddr {
                dest: address,
                base: Operand::Value(base),
                offset: place.offset,
            });
            let value = self.typed_operand(ctx, operand, &place.layout, insts)?;
            insts.push(ExtInst::StoreAt {
                address: Operand::Value(address),
                layout: place.layout.clone(),
                value,
            });
        }
        Ok(())
    }

    /// A `choice` variant, constructed: §3.3's discriminant, then its payload.
    ///
    /// **The payload union is left alone for a payload-free variant**, which
    /// Decision 18 permits — *"a variant with no payload contributes nothing to
    /// the union"* — so `Format.Plain` is one store of one byte and the rest of
    /// the slot keeps whatever it held. That is not a leak of anything: no
    /// reader of a `choice` may look at a payload the discriminant does not
    /// select, and `Projection::Downcast` is emitted only under a `match` arm
    /// that tested the tag.
    #[allow(clippy::too_many_arguments)]
    fn lower_variant(
        &mut self,
        ctx: &mut BodyCtx,
        variant: DefId,
        payload: &[mir::Operand],
        dest: LocalId,
        layout: &Layout,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        let (index, choice) = self.variant_index(variant)?;
        let Repr::Tagged { payload_offset, variants, .. } = &layout.repr else {
            return Err(Unlowered::new(format!(
                "a `{}.{}` value whose layout is Decision 19's niched one: constructing it means \
                 writing the payload or the niche value, and not a discriminant field",
                self.defs.get(choice).name,
                self.defs.get(variant).name
            )));
        };
        let payload_offset = *payload_offset;
        let declared = variants.get(index).and_then(|place| place.payload.clone());
        insts.push(ExtInst::StoreTag { local: dest, discriminant: index as u64 });
        if payload.is_empty() {
            return Ok(());
        }
        let declared = declared.ok_or_else(|| {
            Unlowered::new(format!(
                "a `{}` value given a payload its declaration does not have",
                self.defs.get(variant).name
            ))
        })?;
        let base = ctx.value();
        insts.push(ExtInst::LocalAddr { dest: base, local: dest });
        let start = ctx.value();
        insts.push(ExtInst::FieldAddr {
            dest: start,
            base: Operand::Value(base),
            offset: payload_offset,
        });
        // One element is the element, by `Lowerer::choice_ty`'s decision; more
        // than one is the struct that function built, in positional order.
        if payload.len() == 1 {
            let value = self.typed_operand(ctx, &payload[0], &declared, insts)?;
            insts.push(ExtInst::StoreAt {
                address: Operand::Value(start),
                layout: declared,
                value,
            });
            return Ok(());
        }
        let Repr::Aggregate { fields } = &declared.repr else {
            return Err(Unlowered::new(format!(
                "a `{}` value with {} payload elements whose layout is not an aggregate",
                self.defs.get(variant).name,
                payload.len()
            )));
        };
        if fields.len() != payload.len() {
            return Err(Unlowered::new(format!(
                "a `{}` value given {} payload element(s) where its layout has {}",
                self.defs.get(variant).name,
                payload.len(),
                fields.len()
            )));
        }
        let fields = fields.clone();
        for (element, field) in payload.iter().zip(&fields) {
            let address = ctx.value();
            insts.push(ExtInst::FieldAddr {
                dest: address,
                base: Operand::Value(start),
                offset: field.offset,
            });
            let value = self.typed_operand(ctx, element, &field.layout, insts)?;
            insts.push(ExtInst::StoreAt {
                address: Operand::Value(address),
                layout: field.layout.clone(),
                value,
            });
        }
        Ok(())
    }

    /// §4.6's binary operators, on scalars.
    ///
    /// # Which type the operands have, and where it comes from
    ///
    /// MIR's `Operand` carries no type: `copy _1 > 0` is a place and a
    /// constant, and the constant's width and signedness are the *other*
    /// operand's. So the type is read off whichever side is a place, and only
    /// when neither is does it fall back to the destination's — which is right
    /// for arithmetic, where `_1 = 1 + 2` has `_1`'s type, and **wrong for a
    /// comparison**, where the destination is `Bool` and says nothing about
    /// what was compared. `1 > 0` is therefore refused rather than compared at
    /// a width nothing chose. It is a degenerate expression and the refusal is
    /// one line; comparing two constants at a guessed width is a wrong answer
    /// for `1u64 > 0` the day the guess is `i64`.
    ///
    /// # Overflow, and what §10 does not say
    ///
    /// The core spec says *"integer overflow panics in debug builds and wraps
    /// in release, as Rust does"*. `codegen-and-linking.md` does not mention
    /// overflow anywhere — not in §2.6's operation table, not in §10's stage 3,
    /// which is where integers arrive.
    ///
    /// **What is emitted is the release half: `add`, `sub`, `mul` with neither
    /// `nsw` nor `nuw`.** LLVM defines those as two's-complement wrapping, so
    /// the emitted program's answer is the one the spec names for a release
    /// build, exactly. What is *not* emitted is the debug half — there is no
    /// `llvm.sadd.with.overflow` and no panic on the overflowing edge — and
    /// `sciencec` has no notion of a debug build to key one off, since
    /// `BuildRequest` carries an optimisation level and not a profile.
    ///
    /// **The flags are the part that would have been silent.** `nsw` is what a
    /// C or Rust front end emits for signed arithmetic it has already checked,
    /// and it makes signed overflow *undefined* rather than wrapping — so a
    /// backend that added it "for the optimiser" would turn a spec-defined
    /// wrap into a wrong answer that only appears at `-O2`. They are not set
    /// and there is nowhere in `science_codegen::backend::IntOp` to put them,
    /// which is the same structural refusal `FloatOp` uses for fast-math.
    #[allow(clippy::too_many_arguments)]
    fn lower_binary(
        &mut self,
        body: &MirBody,
        ctx: &mut BodyCtx,
        op: BinaryOp,
        lhs: &mir::Operand,
        rhs: &mir::Operand,
        dest: LocalId,
        dest_ty: Ty,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        let comparison = matches!(
            op,
            BinaryOp::Eq
                | BinaryOp::Ne
                | BinaryOp::Lt
                | BinaryOp::Le
                | BinaryOp::Gt
                | BinaryOp::Ge
        );
        let operand_ty = self
            .operand_ty(body, lhs)
            .or_else(|| self.operand_ty(body, rhs))
            .or(if comparison { None } else { Some(dest_ty) })
            .ok_or_else(|| {
                Unlowered::new(
                    "a comparison of two constants, whose width and signedness no operand and \
                     no destination names",
                )
            })?;
        let scalar = self.scalar_of(operand_ty)?;
        let operand_layout = layout_of(self.target, &self.cg_ty(operand_ty)?);
        let signed = match scalar {
            Scalar::Int(int) => int.is_signed(),
            // `Bool`, `Char` and a pointer compare unsigned, and none of the
            // three has arithmetic.
            _ => false,
        };
        let float = matches!(scalar, Scalar::Float(_));
        let left = self.typed_operand(ctx, lhs, &operand_layout, insts)?;
        let right = self.typed_operand(ctx, rhs, &operand_layout, insts)?;
        let result = ctx.value();

        if comparison {
            let cmp = match op {
                BinaryOp::Eq => CmpOp::Eq,
                BinaryOp::Ne => CmpOp::Ne,
                BinaryOp::Lt => CmpOp::Lt,
                BinaryOp::Le => CmpOp::Le,
                BinaryOp::Gt => CmpOp::Gt,
                BinaryOp::Ge => CmpOp::Ge,
                _ => unreachable!("the `comparison` guard is this same list"),
            };
            insts.push(ExtInst::Above(Inst::Cmp {
                dest: result,
                op: cmp,
                signed,
                lhs: left,
                rhs: right,
            }));
        } else if float {
            let float_op = match op {
                BinaryOp::Add => FloatOp::Add,
                BinaryOp::Sub => FloatOp::Sub,
                BinaryOp::Mul => FloatOp::Mul,
                BinaryOp::Div => FloatOp::Div,
                // IEEE 754 remainder, which `frem` is and which is total:
                // there is no division by zero to guard, because the answer is
                // a NaN.
                BinaryOp::Rem => FloatOp::Rem,
                other => return Err(Unlowered::new(describe_binary(other))),
            };
            insts.push(ExtInst::Above(Inst::FloatBinary {
                dest: result,
                op: float_op,
                lhs: left,
                rhs: right,
            }));
        } else {
            let int_op = match op {
                BinaryOp::Add => IntOp::Add,
                BinaryOp::Sub => IntOp::Sub,
                BinaryOp::Mul => IntOp::Mul,
                BinaryOp::BitAnd => IntOp::And,
                BinaryOp::BitOr => IntOp::Or,
                BinaryOp::BitXor => IntOp::Xor,
                // **Refused, and the reason is that the guard does not
                // exist.** `science_codegen::backend::IntOp` says of `SDiv`:
                // *"division by zero is a panic the caller has already
                // guarded, not a trap the backend inserts."* No caller guards
                // it. Nothing in MIR, THIR or the checker emits a test against
                // zero before a `/`, so lowering `a / b` to a bare `sdiv`
                // hands LLVM an instruction whose behaviour at `b == 0` is
                // *undefined* — not a trap, not a panic, undefined, which at
                // `-O2` licenses deleting the branch that was going to check.
                // `Int.min / -1` is the same hazard with a second operand.
                //
                // Emitting the guard here is possible and is not a lowering:
                // it is two extra basic blocks per division, which breaks
                // Decision 5's *"every MIR basic block becomes exactly one
                // LLVM basic block"* and the MIR-dump-to-IR-dump diff Gate I
                // and Gate J rest on. That is a decision for the note to take
                // and §2.6's table does not take it. So this refuses and says
                // which of the two halves is missing.
                BinaryOp::Div | BinaryOp::Rem => {
                    return Err(Unlowered::new(
                        "integer `/` or `%`, whose divide-by-zero check nothing above this \
                         crate emits — `IntOp`'s own note calls it \"a panic the caller has \
                         already guarded\" and no caller does, and a bare `sdiv` is undefined \
                         at zero rather than a trap",
                    ));
                }
                // The same shape: a shift by at least the operand's width is
                // poison in LLVM, and the mask or the panic that would stop it
                // is specified nowhere.
                BinaryOp::Shl | BinaryOp::Shr => {
                    return Err(Unlowered::new(
                        "a shift, whose amount nothing above this crate bounds — a shift by at \
                         least the operand's width is poison in LLVM and neither the mask nor \
                         the panic that would stop it is specified",
                    ));
                }
                other => return Err(Unlowered::new(describe_binary(other))),
            };
            insts.push(ExtInst::Above(Inst::IntBinary {
                dest: result,
                op: int_op,
                lhs: left,
                rhs: right,
            }));
        }
        insts.push(ExtInst::Above(Inst::Store { local: dest, value: Operand::Value(result) }));
        Ok(())
    }

    /// The Science type of an operand that reads a place, if it reads one.
    ///
    /// **A projected place has a type too, and it is the projection's.**
    /// `science-mir` carries a `Ty` on every projection step and takes it from
    /// the *declaration* rather than from the expression, which is exactly what
    /// makes it usable here. This used to answer `None` for anything projected,
    /// which was right while no projection was lowered and became a wrong
    /// answer the moment one was: `m.a is 80u8` is a place against a constant,
    /// and with no type from the place [`Lowerer::lower_binary`] refused it as
    /// *"a comparison of two constants"* — a message naming something the
    /// program does not contain.
    ///
    /// **`Downcast` is the one step whose `ty` is not the value's**: it is the
    /// *choice's* own type, because a downcast names a variant of it rather
    /// than producing one. A place ending in a downcast is not a readable value
    /// — MIR always follows one with a `TupleField` — so answering `None` is
    /// right and answering with the choice would give a comparison the width of
    /// the whole tagged union.
    fn operand_ty(&self, body: &MirBody, operand: &mir::Operand) -> Option<Ty> {
        let place = operand.place()?;
        match place.projection.last() {
            None => Some(body.local_decl(place.local).ty),
            Some(mir::Projection::Downcast { .. }) => None,
            Some(projection) => Some(projection.ty()),
        }
    }

    /// A type as a user would name it, with one enclosing borrow stripped.
    ///
    /// Used by the refusals that name the type of something the lowering
    /// *borrowed* on the user's behalf. `f"{doc}"` borrows `doc` because
    /// §1.6 says an interpolation does; a message that then said
    /// *"a hole of type `borrowed Doc`"* would be naming a construct the
    /// author did not write, which is [`Lowerer::operand_ty`]'s own rule about
    /// `Debug` output one level up.
    fn render_referent(&self, ty: Ty) -> String {
        let named = match self.types.kind(ty) {
            TyKind::Borrowed { inner, .. } => *inner,
            _ => ty,
        };
        self.types.render(self.defs, named)
    }

    /// A type's scalar, for the two questions the instruction set asks about
    /// one: signed or not, integer or float.
    fn scalar_of(&self, ty: Ty) -> Result<Scalar, Unlowered> {
        let cg = self.cg_ty(ty)?;
        match layout_of(self.target, &cg).repr {
            Repr::Scalar(scalar) => Ok(scalar),
            _ => Err(Unlowered::new("an operator on a value that is not a scalar")),
        }
    }

    /// One MIR operand, **with its width fixed at `layout`'s type**.
    ///
    /// **Not an optimisation, and the thing it replaces was a wrong answer.**
    /// `science_codegen::backend::Operand::ConstInt` carries an `i128` and no
    /// type, so a constant reaching `Inst::IntBinary` takes whatever width the
    /// emitter can infer from the *other* operand — and when the other operand
    /// is also a constant there is nothing to infer from and the default is
    /// `i64`. `let b be 0i8 - 128i8` is that case: `sub i64 0, 128` into a
    /// one-byte slot, which opaque pointers make legal IR, which verifies, and
    /// which answers `false` to `b is -128i8`. `1i8 + 2i8` and `-128i8` are the
    /// same shape.
    ///
    /// So every constant this crate puts into an arithmetic instruction is
    /// materialised at the layout the type checker gave the expression, and
    /// `crate::emit`'s `width_hint` is never load-bearing for anything lowered
    /// here. A place operand is already typed by its `alloca` and passes
    /// through unchanged.
    fn typed_operand(
        &mut self,
        ctx: &mut BodyCtx,
        operand: &mir::Operand,
        layout: &Layout,
        insts: &mut Vec<ExtInst>,
    ) -> Result<Operand, Unlowered> {
        let lowered = self.lower_operand(ctx, operand, Some(layout), insts)?;
        match lowered {
            Operand::Value(_) => Ok(lowered),
            constant => {
                let dest = ctx.value();
                insts.push(ExtInst::Const {
                    dest,
                    layout: layout.clone(),
                    value: constant,
                });
                Ok(Operand::Value(dest))
            }
        }
    }

    /// One MIR operand as something the instruction set can read.
    ///
    /// A place becomes an `Inst::Load` — **a whole-local load, because
    /// `science_codegen::backend::Inst` has no projection** — and a constant
    /// becomes a constant. `expected` is the destination's layout and is used
    /// only to refuse a `null` that is not going into a niche at offset 0.
    fn lower_operand(
        &mut self,
        ctx: &mut BodyCtx,
        operand: &mir::Operand,
        expected: Option<&Layout>,
        insts: &mut Vec<ExtInst>,
    ) -> Result<Operand, Unlowered> {
        match operand {
            mir::Operand::Const(Constant::Literal(literal)) => match literal {
                // The literal's *value*; its width is the other operand's or
                // the slot's, and `crate::emit`'s `width_hint` is where that is
                // applied. `u128 as i128` is a reinterpretation and not a
                // truncation for every literal the lexer admits, because the
                // widest integer type is 64 bits.
                Literal::Int { value, .. } => Ok(Operand::ConstInt(*value as i128)),
                Literal::Float { value, .. } => Ok(Operand::ConstFloat(*value)),
                Literal::Bool(value) => Ok(Operand::ConstInt(i128::from(*value))),
                Literal::Char(value) => Ok(Operand::ConstInt(*value as i128)),
                Literal::Null => match expected.map(|layout| &layout.repr) {
                    Some(Repr::Niched { niche, .. }) if niche.offset == 0 => Ok(Operand::Null),
                    _ => Err(Unlowered::new(
                        "a `null` read as a value of a type whose absent case is not a null \
                         pointer at offset 0",
                    )),
                },
                Literal::Str(_) => Err(Unlowered::new(
                    "a string literal read as a value rather than bound or printed: Decision 15 \
                     makes it a `science_string_from_bytes` call, which needs a slot to own the \
                     result and a `science_string_free` to pair with",
                )),
            },
            mir::Operand::Const(Constant::Unit) => Err(Unlowered::new("a unit value read")),
            // §1.7's capacity, and the one constant in the IR the program does
            // not contain. It reaches [`ExtInst::Const`] like any other and is
            // given the *parameter's* layout there — a `usize` — which is why
            // `Constant::Count` carries no type of its own: there is no Science
            // type for it to disagree with.
            mir::Operand::Const(Constant::Count(count)) => Ok(Operand::ConstInt(*count as i128)),
            mir::Operand::Const(Constant::Item(def)) => {
                Err(Unlowered::new(format!("`{}` named as a value", self.defs.get(*def).name)))
            }
            mir::Operand::Copy(place) | mir::Operand::Move(place) => {
                if !place.projection.is_empty() {
                    let (address, layout) = self.place_address(ctx, place, insts)?;
                    let dest = ctx.value();
                    insts.push(ExtInst::LoadAt {
                        dest,
                        address: Operand::Value(address),
                        layout,
                    });
                    return Ok(Operand::Value(dest));
                }
                let local = LocalId(place.local.index() as u32);
                if ctx.untyped.contains(&local) {
                    return Err(Unlowered::new(UNTYPED));
                }
                ctx.layout(local)?;
                let dest = ctx.value();
                insts.push(ExtInst::Above(Inst::Load { dest, local }));
                Ok(Operand::Value(dest))
            }
        }
    }

    /// Store `null` into a `T?`.
    ///
    /// Two representations and both put the answer at offset 0, which is why
    /// this needs no `getelementptr`. Decision 19's niche form is a null
    /// pointer; Decision 18's tagged form is `SCIENCE_NULLABLE_NULL`, which
    /// `science-rt`'s `abi.rs` fixes at **zero** — *"the absent case is then the
    /// all-zero byte pattern in both representations"*. The niche's own offset
    /// is checked rather than assumed, because §3.4 generalises the rule and a
    /// future niche that is not at 0 would otherwise be written to the wrong
    /// place.
    fn store_null(
        &self,
        local: LocalId,
        layout: &Layout,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        match &layout.repr {
            Repr::Niched { niche, .. } if niche.offset == 0 => {
                insts.push(ExtInst::Above(Inst::Store { local, value: Operand::Null }));
                Ok(())
            }
            // **This used to be a refusal and its stated reason stopped being
            // true.** The sentence was *"which needs a store to the tag byte
            // and the instruction set has no field projection"*; the
            // instruction set has one now, and a refusal that cites a missing
            // thing that is present is worse than no refusal at all.
            //
            // The discriminant is read off the layout rather than written as
            // `0`. `layout_of` desugars `T?` into a two-variant `choice` whose
            // variant 0 is `null` and variant 1 is `present` — which are
            // `science_rt`'s `SCIENCE_NULLABLE_NULL` and
            // `SCIENCE_NULLABLE_PRESENT`, *"in that order"* — and asking the
            // layout which variant carries no payload gets the same answer
            // without a second copy of the convention. The payload union is
            // left undefined, which is Decision 18's rule and is also what
            // `abi.rs` means by *"the absent case is the all-zero byte pattern
            // in both representations"*: the tag is the part that has to be
            // written.
            Repr::Tagged { variants, .. } => {
                let null = variants
                    .iter()
                    .find(|variant| variant.payload.is_none())
                    .ok_or_else(|| {
                        Unlowered::new(
                            "a `null` of a tagged type whose variants all carry a payload: it is \
                             not a nullable",
                        )
                    })?;
                insts.push(ExtInst::StoreTag { local, discriminant: null.discriminant });
                Ok(())
            }
            Repr::Niched { .. } => Err(Unlowered::new("a `null` whose niche is not at offset 0")),
            _ => Err(Unlowered::new("a `null` of a type that is not nullable")),
        }
    }

    /// Decision 15's string literal: the bytes as a global, and a call that
    /// builds a `String` in `slot`.
    ///
    /// The caller owns the result and has to free it. *Where* that free goes
    /// differs: a `print`'s temporary is freed at the call site, and a `let`'s
    /// binding is freed where MIR drops it — which this crate does not lower,
    /// so a bound string literal leaks until `TerminatorKind::Drop` does.
    fn build_string(
        &mut self,
        text: &str,
        slot: LocalId,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        let literal = self.intern_literal(text);
        let from_bytes = self.declare("science_string_from_bytes")?;
        insts.push(ExtInst::Above(Inst::Call {
            dest: None,
            callee: Callee::Runtime("science_string_from_bytes"),
            args: vec![
                Operand::GlobalAddr(literal.bytes_symbol.clone()),
                Operand::ConstInt(literal.len() as i128),
            ],
            ret: from_bytes.ret.clone(),
            sret_slot: Some(slot),
        }));
        Ok(())
    }

    fn lower_terminator(
        &mut self,
        body: &MirBody,
        ctx: &mut BodyCtx,
        kind: &TerminatorKind,
        insts: &mut Vec<ExtInst>,
    ) -> Result<Terminator, Unlowered> {
        match kind {
            // **What `ret` carries is decided by the classification and by
            // nothing else.** An `sret` function returns nothing — the value
            // went through the hidden pointer, and `_0` *is* that pointer's
            // slot — and a `()` function returns nothing because there is
            // nothing. A `Direct` return is the one that has to load `_0`, and
            // it did not before because stage 1's only function was `main` and
            // `main` returns `Error?` through `sret`. A `ret void` from a
            // function LLVM declared as returning an `i64` is a verifier
            // failure, so this one is loud; it is written out rather than
            // guessed because the three cases are three different terminators.
            TerminatorKind::Return => match &ctx.ret {
                ReturnClass::Void | ReturnClass::Indirect => Ok(Terminator::Return(None)),
                ReturnClass::Direct { .. } => {
                    let local = LocalId(0);
                    ctx.layout(local)?;
                    let dest = ctx.value();
                    insts.push(ExtInst::Above(Inst::Load { dest, local }));
                    Ok(Terminator::Return(Some(Operand::Value(dest))))
                }
            },
            TerminatorKind::Goto { target } => Ok(Terminator::Goto(BlockId(target.index() as u32))),
            TerminatorKind::Unreachable => Ok(Terminator::Unreachable),
            TerminatorKind::Call { callee, args, destination, target } => {
                self.lower_call(body, ctx, callee, args, destination, *target, insts)
            }
            // Decision 5 holds: one MIR block, one LLVM block, and an `If`
            // becomes the `br` that ends it. The condition is a `Bool`, so it
            // arrives as §3.1's `i8` memory form and `crate::emit` narrows it.
            TerminatorKind::If { cond, then_block, else_block } => {
                let value = self.lower_operand(ctx, cond, None, insts)?;
                Ok(Terminator::Branch {
                    cond: value,
                    then_block: BlockId(then_block.index() as u32),
                    else_block: BlockId(else_block.index() as u32),
                })
            }
            // §2.2's `switch`, and the translation it needs is from a `DefId`
            // to a number. MIR branches on a variant's definition because
            // *"the numbering is a layout question and `science-codegen` owns
            // layout"*; Decision 18 makes the number the variant's position in
            // declaration order, and [`Lowerer::variant_index`] is the one
            // place that says so.
            //
            // **The arms are checked against the choice the discriminant came
            // from, not merely translated.** An arm naming a variant of some
            // other `choice` would otherwise become a case value that is a
            // valid discriminant of *this* one — a `match` that branches to the
            // wrong arm, verifies, links and runs.
            TerminatorKind::Switch { discr, arms, otherwise } => {
                let place = discr.place().ok_or_else(|| {
                    Unlowered::new("a `switch` on a constant rather than on a discriminant read")
                })?;
                if !place.projection.is_empty() {
                    return Err(Unlowered::new("a `switch` on a projected place"));
                }
                let local = LocalId(place.local.index() as u32);
                let choice = *ctx.discriminants.get(&local.0).ok_or_else(|| {
                    Unlowered::new(
                        "a `switch` on a value no `Rvalue::Discriminant` in this body produced: \
                         the variant numbering is the `choice`'s layout and there is nothing here \
                         to read it off",
                    )
                })?;
                let order = self.choice_variants(choice);
                let value = self.lower_operand(ctx, discr, None, insts)?;
                let mut lowered = Vec::with_capacity(arms.len());
                for (variant, target) in arms {
                    let index = order.iter().position(|def| def == variant).ok_or_else(|| {
                        Unlowered::new(format!(
                            "a `match` arm for `{}`, which is not a variant of `{}`",
                            self.defs.get(*variant).name,
                            self.defs.get(choice).name
                        ))
                    })?;
                    lowered.push((index as u64, BlockId(target.index() as u32)));
                }
                Ok(Terminator::Switch {
                    value,
                    arms: lowered,
                    default: BlockId(otherwise.index() as u32),
                })
            }
            // **A drop that has to run nothing is a `br`, and a drop that has
            // to run something is still refused.** See
            // [`Lowerer::drop_runs_something`] for the predicate and for why
            // `science_codegen::descriptor::needs_drop` is not it.
            TerminatorKind::Drop { place, flag, target } => {
                if flag.is_some() {
                    return Err(Unlowered::new(
                        "a conditionally moved value, which needs a drop flag: Decision 26's \
                         flagged drop is three basic blocks where MIR has one, and no execution \
                         test has run the shape",
                    ));
                }
                let ty = place.ty(body);
                if self.drop_runs_something(ty, 0)? {
                    // Decision 12's `DropGlue::RuntimeCall`, and it is the one
                    // owning type this backend can release without emitting a
                    // glue function. `lower`'s own §1 already stated the rule
                    // for the temporary a `print` makes — *"a `String` does not
                    // need [glue] — the temporary is freed by a direct
                    // `science_string_free` at the site that made it"* — and a
                    // bound `String` is that same call at the site that drops
                    // it. `science_string_free` takes the address and no
                    // descriptor, which is what separates it from
                    // `science_array_free` and `science_map_free`; those take
                    // one and this backend emits none, so they are refused
                    // below with the rest.
                    if !self.is_string(ty) {
                        return Err(Unlowered::new(format!(
                            "a drop of a value of type `{}`, which owns something Decision 12's \
                             glue would have to release: glue is an emitted `internal` function \
                             per monomorphised type, this backend emits none, and the only \
                             release it can make without one is `science_string_free`",
                            self.types.render(self.defs, ty)
                        )));
                    }
                    let (address, _) = self.place_address(ctx, place, insts)?;
                    let free = self.declare("science_string_free")?;
                    insts.push(ExtInst::Above(Inst::Call {
                        dest: None,
                        callee: Callee::Runtime("science_string_free"),
                        args: vec![Operand::Value(address)],
                        ret: free.ret.clone(),
                        sret_slot: None,
                    }));
                }
                Ok(Terminator::Goto(BlockId(target.index() as u32)))
            }
        }
    }

    /// Lower a call.
    ///
    /// Three shapes reach here and the third is §10's stage 2:
    ///
    /// 1. **`print` of a string literal**, which Decision 15 makes three calls
    ///    — build, print, free — where MIR has one terminator.
    /// 2. **`print` of a `String` the program is holding**, which is one call
    ///    and a free.
    /// 3. **A call to an `extern "C"` declaration**, which Decision 40 makes
    ///    *"a direct `call` to the declared symbol — no thunk, no wrapper, no
    ///    trampoline"*.
    #[allow(clippy::too_many_arguments)]
    fn lower_call(
        &mut self,
        body: &MirBody,
        ctx: &mut BodyCtx,
        callee: &mir::Callee,
        args: &[mir::Operand],
        destination: &mir::Place,
        target: Option<mir::BlockId>,
        insts: &mut Vec<ExtInst>,
    ) -> Result<Terminator, Unlowered> {
        let def = match callee {
            mir::Callee::Def(def) => *def,
            mir::Callee::Indirect(_) => return Err(Unlowered::new("a call through a closure")),
            // **A call MIR makes to `science-rt` directly**, which
            // `science-mir` calls *"the array-op hole kept open deliberately"*
            // and which nothing produced until `f"…"` did. This used to be an
            // outright refusal naming *"a whole-array call"*, which was the
            // only kind anyone expected; a string interpolation is the first
            // one that exists.
            mir::Callee::Runtime(symbol) => {
                self.lower_runtime_call(ctx, symbol, args, destination, insts)?;
                return Ok(match target {
                    Some(block) => Terminator::Goto(BlockId(block.index() as u32)),
                    None => Terminator::Unreachable,
                });
            }
            // **The one `Unresolved` whose message can name a type**, because
            // the argument is in front of it. `science-mir`'s
            // `Unresolved::Display` is a hole *below* that crate rather than
            // above it — the shape of the call is known and what is missing is
            // a `science_string_push_*` for this one type — so the refusal that
            // helps is the one that says which type, and it is read off the
            // hole operand the lowering already borrowed.
            mir::Callee::Unresolved(mir::Unresolved::Display) => {
                let named = args
                    .get(1)
                    .and_then(|operand| self.operand_ty(body, operand))
                    .map(|ty| self.render_referent(ty));
                return Err(Unlowered::new(match named {
                    Some(name) => format!(
                        "an `f\"…\"` hole of type `{name}`, which `science-rt` has no \
                         `science_string_push_*` entry point for: the seven that exist render \
                         `Int`/`I64`, `U64`, `F64`, `F32`, `Bool`, `Char` and `String`, and \
                         §3.1's `Formatter` — which is what a user type would render through — \
                         is specified by no note and declared by no prelude"
                    ),
                    None => describe_unresolved(mir::Unresolved::Display).to_string(),
                }));
            }
            mir::Callee::Unresolved(unresolved) => {
                return Err(Unlowered::new(describe_unresolved(*unresolved)));
            }
        };
        if self.defs.get(def).kind == DefKind::ExternFn {
            self.lower_foreign_call(ctx, def, args, destination, insts)?;
        } else if self.defs.get(def).is_builtin() && self.defs.get(def).name == "print" {
            self.lower_print(ctx, args, insts)?;
        } else if let Some(sig) = self.science.get(&def).cloned() {
            self.lower_science_call(ctx, &sig, args, destination, insts)?;
        } else {
            let name = self.defs.get(def).name.clone();
            // Reachable and not in the table means the walk did not see it,
            // which for a `Callee::Def` means the crate has no body for it: a
            // builtin with no signature, or a declaration `science-mir`
            // resolved and nothing lowered.
            return Err(Unlowered::new(format!(
                "a call to `{name}`, which this crate was given no MIR body for"
            )));
        }
        match target {
            Some(block) => Ok(Terminator::Goto(BlockId(block.index() as u32))),
            // A call with no successor is a panic (§2.2), and neither `print`
            // nor anything else this crate emits is one.
            None => Ok(Terminator::Unreachable),
        }
    }

    /// `print`, of a literal or of a `String` the program holds.
    ///
    /// The free is not optional in either case. For a literal the temporary is
    /// owned by the call site and nothing else frees it, so omitting
    /// `science_string_free` leaks twenty-four bytes per evaluation; §2.6's
    /// cost note for Decision 15 is the other half of the same fact — *"a
    /// literal in a loop allocates on every iteration"* — and it also frees on
    /// every iteration.
    ///
    /// For a **moved** local the free is right for a different reason, and it
    /// is worth stating because it looks like a double free and is not.
    /// `print` has no declared signature (crate §3 item 7), so the checker
    /// cannot know it borrows; MIR therefore records `move` of the local into
    /// the call, drop elaboration treats the local as moved-out, and **no
    /// `Drop` terminator is emitted for it**. The call is the value's last
    /// owner, so the call frees it. A `copy` would mean something else still
    /// owns it, and is refused rather than guessed, because a guess here is a
    /// double free.
    fn lower_print(
        &mut self,
        ctx: &mut BodyCtx,
        args: &[mir::Operand],
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        let string_layout = layout_of(self.target, &RtAggregate::String.cg_ty());
        let slot = match args {
            [mir::Operand::Const(Constant::Literal(Literal::Str(text)))] => {
                // A slot for the temporary `String`. It is not a MIR local —
                // MIR's `print("…")` has the literal as a constant operand,
                // with no local to hold the `ScienceString` the runtime builds
                // — so codegen invents one, and it goes in the entry block with
                // the rest (Decision 8).
                let slot = self.temp(ctx, string_layout.clone());
                self.build_string(text, slot, insts)?;
                slot
            }
            [mir::Operand::Move(place)] if place.projection.is_empty() => {
                let local = LocalId(place.local.index() as u32);
                let layout = ctx.layout(local)?.clone();
                if layout != string_layout {
                    return Err(Unlowered::new(
                        "a `print` of a value that is not a `String`: `print` takes \
                         `borrowed any Display`, this backend emits no vtables, and there is no \
                         `display` method on `Display` to call through even if it did",
                    ));
                }
                local
            }
            [mir::Operand::Copy(_)] => {
                return Err(Unlowered::new(
                    "a `print` of a copied `String`: MIR's move analysis says something else \
                     still owns it, and this call site frees what it prints",
                ));
            }
            _ => return Err(Unlowered::new("a `print` of anything but a `String`")),
        };

        // The address of the slot, which is the operand the interface above the
        // line cannot spell. `crate::emit`'s §2 is the account.
        let address = ctx.value();
        insts.push(ExtInst::LocalAddr { dest: address, local: slot });

        let print = self.declare("science_print")?;
        insts.push(ExtInst::Above(Inst::Call {
            dest: None,
            callee: Callee::Runtime("science_print"),
            args: vec![Operand::Value(address)],
            ret: print.ret.clone(),
            sret_slot: None,
        }));

        let free = self.declare("science_string_free")?;
        insts.push(ExtInst::Above(Inst::Call {
            dest: None,
            callee: Callee::Runtime("science_string_free"),
            args: vec![Operand::Value(address)],
            ret: free.ret.clone(),
            sret_slot: None,
        }));
        Ok(())
    }

    /// A call MIR makes to a `science-rt` entry point by name.
    ///
    /// **The signature is [`RUNTIME`]'s and the arguments are matched to it,
    /// not to the call.** `science_codegen::runtime` is §2.6's *"whole list"*
    /// and [`runtime_signature`] derives the classification from it, so a call
    /// site cannot disagree with a declaration about `sret` or about a width —
    /// which is §9.2's finding and the reason the table exists at all. A symbol
    /// that is not in the table is refused by name rather than declared on
    /// trust, because a `declare` invented here is a symbol the linker will
    /// happily fail on with `SC0402` instead of `SC0400`.
    ///
    /// # The two places a MIR operand and a C parameter are not one to one
    ///
    /// **A pointer parameter given an aggregate takes that aggregate's
    /// address.** Every entry point on the runtime boundary takes an aggregate
    /// as `*const`/`*mut` and never by value — that is Decision 22's *"one rule
    /// … and it is 'pass a pointer'"* — so `science_string_push_i64(s, 42)`
    /// has a MIR operand naming the `String` and a C parameter wanting its
    /// address. [`Lowerer::lower_print`] has done exactly this since stage 1
    /// with [`crate::emit::ExtInst::LocalAddr`]; this generalises it and makes
    /// the rule explicit: **a pointer parameter given a place whose slot is not
    /// itself a pointer is passed that slot's address.** A place whose slot
    /// *is* a pointer passes its value, which is what `science_chars_next`'s
    /// second argument and `science_string_push_bytes`'s already need.
    ///
    /// **A string literal spans two parameters.** `science_string_push_bytes`
    /// takes `(ptr, len)`, and MIR has no operand that names a global — a
    /// literal reaches here as `Constant::Literal(Literal::Str)` and nothing
    /// else. So a literal landing on a pointer parameter whose successor is a
    /// `usize` is interned and expanded into
    /// [`science_codegen::backend::Operand::GlobalAddr`] and its length, which
    /// is the same pair [`Lowerer::build_string`] has always emitted for
    /// `science_string_from_bytes`. It is the one place the argument count and
    /// the parameter count differ, it is stated here, and the arity check below
    /// accounts for it rather than being skipped.
    fn lower_runtime_call(
        &mut self,
        ctx: &mut BodyCtx,
        symbol: &str,
        args: &[mir::Operand],
        destination: &mir::Place,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        let sig = self.declare(symbol)?;
        let entry = runtime_fn(symbol).expect("`declare` found it");

        // The expansion above means the arity is checked against the parameters
        // a literal does *not* cover, so it is counted rather than compared.
        let mut lowered: Vec<Operand> = Vec::with_capacity(sig.params.len());
        let mut param = 0usize;
        for arg in args {
            let Some(class) = sig.params.get(param) else {
                return Err(Unlowered::new(format!(
                    "a call to `{symbol}` with more arguments than its {} parameter(s)",
                    sig.params.len()
                )));
            };
            if matches!(class.class, ArgClass::IndirectByPointer) {
                return Err(Unlowered::new(format!(
                    "a call to `{symbol}`, one of whose parameters is classified indirect: no \
                     entry point in `RUNTIME` is, so this is a table that has changed under a \
                     lowering that assumed it had not"
                )));
            }
            if matches!(class.class, ArgClass::Ignore) {
                param += 1;
                continue;
            }
            let pointer = matches!(class.layout.repr, Repr::Scalar(Scalar::Pointer(_)));
            // A literal into `(ptr, len)`.
            if let mir::Operand::Const(Constant::Literal(Literal::Str(text))) = arg {
                let follows_a_length = sig
                    .params
                    .get(param + 1)
                    .is_some_and(|next| next.layout == layout_of(self.target, &CgTy::Int(IntTy::Usize)));
                if !pointer || !follows_a_length {
                    return Err(Unlowered::new(format!(
                        "a string literal passed to `{symbol}` where its parameters are not a \
                         pointer followed by a length: a literal has no other spelling in MIR and \
                         no other lowering here"
                    )));
                }
                let literal = self.intern_literal(text);
                lowered.push(Operand::GlobalAddr(literal.bytes_symbol.clone()));
                lowered.push(Operand::ConstInt(literal.len() as i128));
                param += 2;
                continue;
            }
            if pointer {
                if let Some(place) = arg.place() {
                    let (address, layout) = self.place_address(ctx, place, insts)?;
                    if matches!(layout.repr, Repr::Scalar(Scalar::Pointer(_))) {
                        // The slot holds a pointer: the value is the argument.
                        let value = ctx.value();
                        insts.push(ExtInst::LoadAt {
                            dest: value,
                            address: Operand::Value(address),
                            layout,
                        });
                        lowered.push(Operand::Value(value));
                    } else {
                        lowered.push(Operand::Value(address));
                    }
                    param += 1;
                    continue;
                }
            }
            lowered.push(self.typed_operand(ctx, arg, &class.layout, insts)?);
            param += 1;
        }
        if param != sig.params.len() {
            return Err(Unlowered::new(format!(
                "a call to `{symbol}` that fills {param} of its {} parameter(s)",
                sig.params.len()
            )));
        }
        self.emit_result(ctx, Callee::Runtime(entry.symbol), &sig.ret, lowered, destination, insts)
    }

    /// A call to another Science function: Decision 22's private convention.
    ///
    /// **The signature is the callee's own, read from the one table
    /// [`Lowerer::lower_crate`] built.** Not re-derived here, and not read from
    /// `Declarations`: §9.2's finding is a call site that classified one
    /// symbol's return differently from its definition, and the only structural
    /// defence against it is that there be one classification.
    ///
    /// **An aggregate argument is the caller's own slot, passed by address.**
    /// Decision 22: *"every aggregate argument is passed by pointer to a
    /// caller-owned slot"*, and [`science_codegen::abi::indirect_argument_attrs`]
    /// says what that pointer means — a **move** into the callee, `nocapture`
    /// and not `readonly`, so the callee may write through it and the caller
    /// must treat the slot as moved-from. That makes `Operand::Copy` of an
    /// aggregate the one shape this must refuse rather than lower: a copy is
    /// MIR saying something else still owns the value, and handing the callee a
    /// writable pointer to a slot somebody else still reads is a wrong answer
    /// with no diagnostic anywhere. The repair, when a program needs it, is a
    /// copy into an invented slot — which is what a real argument lowering does
    /// and which is not guessed at here.
    fn lower_science_call(
        &mut self,
        ctx: &mut BodyCtx,
        sig: &AbiSignature,
        args: &[mir::Operand],
        destination: &mir::Place,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        if args.len() != sig.params.len() {
            return Err(Unlowered::new(format!(
                "a call to `{}` with {} argument(s) where it declares {}",
                sig.symbol,
                args.len(),
                sig.params.len()
            )));
        }
        let mut lowered: Vec<Operand> = Vec::with_capacity(args.len());
        for (arg, param) in args.iter().zip(&sig.params) {
            match param.class {
                ArgClass::Ignore => {}
                ArgClass::Direct => {
                    lowered.push(self.typed_operand(ctx, arg, &param.layout, insts)?);
                }
                ArgClass::IndirectByPointer => {
                    let mir::Operand::Move(place) = arg else {
                        return Err(Unlowered::new(format!(
                            "an aggregate argument to `{}` that is not a move: Decision 22 passes \
                             a pointer to the caller's own slot and the callee may write through \
                             it, so a copy would hand it a slot something else still owns",
                            sig.symbol
                        )));
                    };
                    if !place.projection.is_empty() {
                        return Err(Unlowered::new(format!(
                            "an aggregate argument to `{}` that is a field rather than a whole \
                             local: Decision 22 needs a slot to point at",
                            sig.symbol
                        )));
                    }
                    let local = LocalId(place.local.index() as u32);
                    if ctx.untyped.contains(&local) {
                        return Err(Unlowered::new(UNTYPED));
                    }
                    ctx.layout(local)?;
                    let address = ctx.value();
                    insts.push(ExtInst::LocalAddr { dest: address, local });
                    lowered.push(Operand::Value(address));
                }
            }
        }
        self.emit_result(
            ctx,
            Callee::Science(sig.symbol.clone()),
            &sig.ret,
            lowered,
            destination,
            insts,
        )
    }

    /// The call instruction and where its result lands, shared by the Science
    /// and the foreign paths.
    ///
    /// The two used to have a copy of this each, and the copies were the same
    /// four lines with the same `sret` question asked twice — which is §9.2's
    /// shape exactly, so there is one of them.
    fn emit_result(
        &self,
        ctx: &mut BodyCtx,
        callee: Callee,
        ret: &ReturnClass,
        args: Vec<Operand>,
        destination: &mir::Place,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        if !destination.projection.is_empty() {
            return Err(Unlowered::new("a call whose result goes through a field"));
        }
        let dest_local = LocalId(destination.local.index() as u32);
        // A `()`-returning function writes nowhere, and its MIR destination is
        // a unit temporary with no slot of its own.
        let has_slot = ctx.layouts.contains_key(&dest_local.0);
        if ret.is_sret() {
            if !has_slot {
                return Err(Unlowered::new(
                    "a call returning an aggregate into a destination with no slot",
                ));
            }
            insts.push(ExtInst::Above(Inst::Call {
                dest: None,
                callee,
                args,
                ret: ret.clone(),
                sret_slot: Some(dest_local),
            }));
            return Ok(());
        }
        let value = ctx.value();
        insts.push(ExtInst::Above(Inst::Call {
            dest: Some(value),
            callee,
            args,
            ret: ret.clone(),
            sret_slot: None,
        }));
        if has_slot && !matches!(ret, ReturnClass::Void) {
            insts.push(ExtInst::Above(Inst::Store {
                local: dest_local,
                value: Operand::Value(value),
            }));
        }
        Ok(())
    }

    /// §10's stage 2: a call into a C library.
    ///
    /// Decision 40 makes this *"a direct `call` to the declared symbol"*. What
    /// this adds beyond the call is the declaration — classified by §4.2's
    /// [`classify_extern_argument`] and [`classify_extern_return`], which
    /// **refuse** a by-value aggregate rather than guess a classification — and
    /// the record that the block's `library` clause was reached, which is what
    /// puts `-lm` on the link line and nothing else.
    fn lower_foreign_call(
        &mut self,
        ctx: &mut BodyCtx,
        def: DefId,
        args: &[mir::Operand],
        destination: &mir::Place,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        let sig = self.declare_foreign(def)?;
        if args.len() != sig.params.len() {
            return Err(Unlowered::new(format!(
                "a call to `{}` with {} argument(s) where it declares {}",
                sig.symbol,
                args.len(),
                sig.params.len()
            )));
        }
        let mut lowered: Vec<Operand> = Vec::with_capacity(args.len());
        for (arg, param) in args.iter().zip(&sig.params) {
            if matches!(param.class, ArgClass::Ignore) {
                continue;
            }
            lowered.push(self.lower_operand(ctx, arg, Some(&param.layout), insts)?);
        }

        self.emit_result(
            ctx,
            Callee::Foreign(sig.symbol.clone()),
            &sig.ret,
            lowered,
            destination,
            insts,
        )
    }

    /// The `declare` for one `extern "C"` function, interned.
    ///
    /// **The signature comes from `Declarations` and not from the HIR**,
    /// although `hir::ExternFn` carries a parameter list. Those parameters are
    /// syntax — `type BlasInt is I32` is an alias this crate would have to
    /// resolve — and resolving them here is a second, worse copy of the type
    /// checker. `Declarations::signature` is what the checker built and what
    /// the *call site* was checked against, so a declaration that disagrees
    /// with its calls is impossible by construction rather than by testing.
    fn declare_foreign(&mut self, def: DefId) -> Result<AbiSignature, Unlowered> {
        let name = self.defs.get(def).name.clone();
        let Some(foreign) = self.foreign.get(&def) else {
            return Err(Unlowered::new(format!(
                "a call to `{name}`, which is a foreign function this build was not given the \
                 `extern` block of"
            )));
        };
        let symbol = foreign.symbol.clone();
        let library = foreign.library.clone();
        let span = foreign.span;
        if foreign.variadic {
            return Err(Unlowered::new(format!(
                "a call to `{name}`, which is variadic — §4.5 refuses a Science call to a `...` \
                 function because the C default-argument-promotion rules are not modelled"
            )));
        }
        if let Some(existing) = self.declarations.iter().find(|s| s.symbol == symbol) {
            return Ok(existing.clone());
        }
        let Some(decls) = self.decls else {
            return Err(Unlowered::new(format!(
                "a call to `{name}`: this build was given no declaration table to read its \
                 signature from"
            )));
        };
        let Some(signature) = decls.signature(def) else {
            return Err(Unlowered::new(format!("a call to `{name}`, which has no signature")));
        };
        let abi = self.target.c_abi();
        let ret_ty = self.cg_ty(signature.ret)?;
        let ret_layout = layout_of(self.target, &ret_ty);
        let ret = classify_extern_return(&ret_ty, &ret_layout, abi)
            .map_err(|refusal| Unlowered::new(describe_refusal(&refusal, &name)))?;
        let params: Vec<_> = signature.params.clone();
        let mut abi_params = Vec::with_capacity(params.len());
        for param in &params {
            let ty = self.cg_ty(param.ty)?;
            let layout = layout_of(self.target, &ty);
            let class = classify_extern_argument(&ty, &layout)
                .map_err(|refusal| Unlowered::new(describe_refusal(&refusal, &name)))?;
            abi_params.push(AbiParam {
                name: self.defs.get(param.def).name.clone(),
                class,
                layout,
                attrs: ParamAttrs::default(),
            });
        }
        let sig = AbiSignature {
            symbol: symbol.clone(),
            ret,
            ret_layout,
            params: abi_params,
            foreign: true,
            // Decision 6, and here it is a claim about the C library rather
            // than about Science: a C function that unwinds through a
            // `nounwind` frame is undefined. `panic.rs` already makes the whole
            // binary landing-pad-free, so the attribute records what is true of
            // every frame in the image rather than adding an assumption.
            nounwind: true,
        };
        self.declarations.push(sig.clone());
        if let Some(library) = &library {
            if !self.libraries_used.contains(library) {
                self.libraries_used.push(library.clone());
            }
        }
        self.declared_foreign.push(ForeignSymbol { symbol, name, span, library });
        Ok(sig)
    }

    /// C's `main`, which `science-rt` does not provide: `script-mode.md` §2.3's
    /// exit table in three blocks. See the module documentation for the table
    /// and for what the failing block prints.
    ///
    /// **Public, and it is the acceptance test that makes it so.** §2.3's
    /// failing row is reachable from no Science program this compiler can build
    /// — a non-null `Error?` needs a concrete error, a box and a vtable, and
    /// stage 1 has none of the three — so `tests/exit_code.rs` pairs this
    /// function with a `_S4main` it writes itself, links the result and runs it.
    /// A test that built its own `main` beside this one would assert the table
    /// against a copy of the table; this way it asserts the emitted `main`, and
    /// the only invented half is the value that `main` reads.
    pub fn lower_c_main(
        &mut self,
        science_main: &AbiSignature,
    ) -> Result<(AbiSignature, ExtBody), Unlowered> {
        let i32_layout = layout_of(self.target, &CgTy::Int(IntTy::I32));
        let sig = AbiSignature {
            symbol: "main".to_string(),
            ret: science_codegen::abi::classify_return_for(self.target, &i32_layout),
            ret_layout: i32_layout,
            params: vec![],
            foreign: false,
            nounwind: true,
        };
        if !science_main.ret.is_sret() {
            return Err(Unlowered::new(
                "an entry point whose `Error?` does not come back through `sret`; stage 1 only \
                 knows the two-word interface object",
            ));
        }
        let slot = LocalId(0);
        let data = ValueId(0);
        let is_null = ValueId(1);
        let message = self.intern_literal(ERROR_MESSAGE);
        let write_error = self.declare("science_write_error_bytes")?;
        let exit = self.declare("science_exit")?;

        let entry = ExtBlock {
            id: BlockId(0),
            label: "entry".to_string(),
            insts: vec![
                ExtInst::Above(Inst::Alloca {
                    local: slot,
                    layout: science_main.ret_layout.clone(),
                }),
                ExtInst::Above(Inst::Call {
                    dest: None,
                    callee: Callee::Science(science_main.symbol.clone()),
                    args: vec![],
                    ret: science_main.ret.clone(),
                    sret_slot: Some(slot),
                }),
                // §3.4: the data pointer and **only** the data pointer. The
                // vtable word of a null `(any Error)?` is undefined, and it is
                // at offset 8, so this load must not widen — which is what
                // `Inst::Load` does, because it loads the local's whole type.
                // `ExtInst::LoadNiche` is the narrow one and its documentation
                // is the account.
                ExtInst::LoadNiche { dest: data, local: slot },
                ExtInst::Above(Inst::Cmp {
                    dest: is_null,
                    op: science_codegen::backend::CmpOp::Eq,
                    signed: false,
                    lhs: Operand::Value(data),
                    rhs: Operand::Null,
                }),
            ],
            terminator: Terminator::Branch {
                cond: Operand::Value(is_null),
                then_block: BlockId(1),
                else_block: BlockId(2),
            },
        };
        // Rows 1 to 3 of §2.3, which are one edge: falling off the end is an
        // implicit `return null`, a bare `return` is sugar for `return null`,
        // and `return err` with a null `err` is that same value written out.
        // Three rows in the note because three things a user types; one block
        // here because the compiler cannot tell them apart and must not.
        //
        // **`science_exit(0)` rather than `ret i32 0`, and the difference is a
        // flush.** A Science binary's entry point is this function, so Rust's
        // `lang_start` never runs and nothing is registered to flush the
        // `LineWriter` inside `science-rt`'s `std::io::stdout()`. Returning
        // would end the process through the C runtime, which flushes C's stdio
        // and knows nothing about that buffer. `print` appends a newline and a
        // `LineWriter` flushes on one, so today nothing is lost; `write` does
        // not, and the first program that ends with one would lose its last
        // line. `exit.rs` is the account.
        let ok = ExtBlock {
            id: BlockId(1),
            label: "ok".to_string(),
            insts: vec![ExtInst::Above(Inst::Call {
                dest: None,
                callee: Callee::Runtime("science_exit"),
                args: vec![Operand::ConstInt(0)],
                ret: exit.ret.clone(),
                sret_slot: None,
            })],
            // Decision 6: `call` then `unreachable`, never `invoke`.
            terminator: Terminator::Unreachable,
        };
        // Row 4: `error: ` and something true on stderr, then status 1. The two
        // calls are in that order and the order is the contract — a status
        // without the message is a program that fails silently, and a message
        // without the status is one a shell believes.
        let failed = ExtBlock {
            id: BlockId(2),
            label: "failed".to_string(),
            insts: vec![
                ExtInst::Above(Inst::Call {
                    dest: None,
                    callee: Callee::Runtime("science_write_error_bytes"),
                    args: vec![
                        Operand::GlobalAddr(message.bytes_symbol.clone()),
                        Operand::ConstInt(message.len() as i128),
                    ],
                    ret: write_error.ret.clone(),
                    sret_slot: None,
                }),
                ExtInst::Above(Inst::Call {
                    dest: None,
                    callee: Callee::Runtime("science_exit"),
                    args: vec![Operand::ConstInt(
                        science_codegen::runtime::EXIT_CONTRACT.required_status as i128,
                    )],
                    ret: exit.ret.clone(),
                    sret_slot: None,
                }),
            ],
            terminator: Terminator::Unreachable,
        };
        Ok((sig, ExtBody { blocks: vec![entry, ok, failed] }))
    }
}

/// Everything one body needs that the MIR does not carry.
///
/// **`entry` is separate from the block being lowered, and that is the stage-3
/// change.** Stage 1 had one basic block, so "the entry block" and "here" were
/// the same place and a temporary's `alloca` could be pushed where it was
/// needed. Stage 3 has loops, and an `alloca` inside one runs on every
/// iteration: the stack grows without bound, which is a program that works for
/// ten iterations and dies at a million, and which nothing in the IR looks
/// wrong. Decision 8 already says every local is *"an `alloca` in the
/// function's entry block"*; this field is what makes the sentence true for the
/// slots codegen invents as well as for the ones MIR declares.
struct BodyCtx {
    /// Every slot with a layout: MIR's locals by index, then the invented ones.
    layouts: BTreeMap<u32, Layout>,
    /// The locals `lower_body` skipped because their type is `Ty::ERROR`.
    untyped: Vec<LocalId>,
    /// The entry block's `alloca`s, collected wherever they are invented.
    entry: Vec<ExtInst>,
    /// The stores that bind each register parameter to its slot, emitted after
    /// every `alloca` and before the first instruction of the user's `main`.
    prologue: Vec<ExtInst>,
    /// Which `choice` each discriminant slot was read from.
    ///
    /// **The table `TerminatorKind::Switch` cannot do without.** MIR branches on
    /// a **`DefId`**, not on a number — `science-mir` says why: *"the numbering
    /// is a layout question and `science-codegen` owns layout"* — and the local
    /// the discriminant was read into is typed `Ty::ERROR`, because *"which
    /// variant a value holds has no Science type at all"*. So by the time a
    /// `switch` is reached there is nothing in the MIR that says which choice
    /// the number came from, and the variant ordering is what turns a `DefId`
    /// into a case value. This is filled in by
    /// [`Lowerer::lower_discriminant`], which is the one place that still knows.
    discriminants: BTreeMap<u32, DefId>,
    /// How this function's return value comes back, for `TerminatorKind::Return`.
    ret: ReturnClass,
    next_value: u32,
    next_temp: u32,
}

impl BodyCtx {
    fn value(&mut self) -> ValueId {
        let id = ValueId(self.next_value);
        self.next_value += 1;
        id
    }

    fn layout(&self, local: LocalId) -> Result<&Layout, Unlowered> {
        self.layouts.get(&local.0).ok_or_else(|| {
            Unlowered::new(format!("a use of local _{}, which has no slot", local.0))
        })
    }
}

/// An `extern` signature this crate cannot classify, as a refusal.
///
/// **`SC0429` and `SC0431` are `ffi-c-boundary.md`'s codes and this is not
/// them.** `AbiRefusal::to_diagnostic` renders those two properly and needs a
/// span to do it; what reaches here is a call site inside a body, and the span
/// that matters is the *declaration's*, which this crate has and `Unlowered`
/// has nowhere to put. So the refusal is `SC0400` carrying the sentence, and
/// the day `Unlowered` carries a span the two collapse into one.
fn describe_refusal(refusal: &science_codegen::abi::AbiRefusal, function: &str) -> String {
    match refusal {
        science_codegen::abi::AbiRefusal::AggregateByValue { position } => format!(
            "a call to `{function}`, whose {position} is a by-value aggregate: F0 implements no              argument classifier, and System V, AAPCS64 and Windows x64 classify aggregates by              three different rules, so guessing links cleanly and corrupts the stack at run time"
        ),
        science_codegen::abi::AbiRefusal::HalfPrecision => format!(
            "a call to `{function}`, which passes `F16` or `BF16` by value across the              `extern \"C\"` boundary"
        ),
    }
}

/// What a [`mir::Unresolved`] callee is, named the way a user would recognise
/// it.
///
/// **The `for` loop row is the boundary this crate reports and does not own.**
/// `science-mir`'s own note on `Unresolved::IterateNext` says `thir::ExprKind::For`
/// *"has no field for a callee"*, so every range- and collection-driven `for`
/// in the language arrives here with a hole where its `next` should be. That is
/// §10's stage 3 program — `for i in 0..10:` — and it cannot be lowered by
/// anything this crate does.
fn describe_unresolved(unresolved: mir::Unresolved) -> &'static str {
    match unresolved {
        mir::Unresolved::Method => {
            "a method call the front end could not resolve: Decision 11's method lookup does not              put what it found in the tree"
        }
        mir::Unresolved::IterateNext => {
            "a `for` loop: its `next` call reaches MIR unresolved, because              `science_types::thir::ExprKind::For` has no field for the callee the checker's              `iterate_item` found. Every `for` in the language stops here, including              `for i in 0..10:`"
        }
        mir::Unresolved::Operator => {
            "an operator or an index on a user type, which is Decision 11's method lookup again"
        }
        // The typed spelling is at the call site, which has the argument this
        // one does not. This is what is left when the operand names no place.
        mir::Unresolved::Display => {
            "an `f\"…\"` hole whose type `science-rt` has no `science_string_push_*` entry point for"
        }
    }
}

/// How a binary operator is named in a refusal.
fn describe_binary(op: BinaryOp) -> String {
    match op {
        BinaryOp::Pow => "`**`, which needs `llvm.pow` and Decision 37's intrinsic whitelist"
            .to_string(),
        BinaryOp::MatMul => "`@`, the matrix product, which is a whole-array operation and                              Decision 5 makes one a runtime call that does not exist"
            .to_string(),
        BinaryOp::And | BinaryOp::Or => "`and` or `or` as a value: MIR turns both into blocks                                          and edges, so one reaching here is a MIR that did not"
            .to_string(),
        other => format!("`{}` on this type", other.as_str()),
    }
}

/// Which [`ConvOp`] takes `source` to `destination`, if §5.1 defines one.
///
/// `None` is *"§5.1 does not define this cast"* and `Some(None)` is *"it is
/// defined and it is no instruction"* — the same bits under another name, which
/// `I64 as U64` and `Int as I64` both are. Two levels rather than a third enum
/// variant, because the caller does exactly two different things with them and
/// a `ConvOp::Nop` would be a ninth operation the emitter would have to know
/// emits nothing.
///
/// **A pointer is not here at all**, in either position. Science has no cast
/// between a reference and an integer and no `as` that produces one, so a
/// [`Scalar::Pointer`] on either side is a disagreement between this function
/// and the type checker rather than a conversion to define, and it falls
/// through to the refusal with both type names in it.
///
/// [`Lowerer::lower_cast`]'s note is where each row is argued.
fn conversion(source: Scalar, destination: Scalar, target: Triple) -> Option<Option<ConvOp>> {
    // `Char` is a `u32` and `Bool` is a byte whose only valid values are 0 and
    // 1, so both behave as unsigned integers on the *reading* side and neither
    // may be written to by a conversion that could produce anything else.
    let as_unsigned_source = |scalar: Scalar| -> Option<u64> {
        match scalar {
            Scalar::Char => Some(4),
            Scalar::Bool => Some(1),
            _ => None,
        }
    };
    match (source, destination) {
        (Scalar::Int(from), Scalar::Int(to)) => {
            let (wide, narrow) = (from.width(target), to.width(target));
            Some(match narrow.cmp(&wide) {
                std::cmp::Ordering::Equal => None,
                std::cmp::Ordering::Less => Some(ConvOp::Trunc),
                std::cmp::Ordering::Greater if from.is_signed() => Some(ConvOp::SignExtend),
                std::cmp::Ordering::Greater => Some(ConvOp::ZeroExtend),
            })
        }
        (Scalar::Int(from), Scalar::Float(_)) => Some(Some(if from.is_signed() {
            ConvOp::IntToFloat
        } else {
            ConvOp::UintToFloat
        })),
        (Scalar::Float(_), Scalar::Int(to)) => Some(Some(if to.is_signed() {
            ConvOp::FloatToIntSat
        } else {
            ConvOp::FloatToUintSat
        })),
        (Scalar::Float(from), Scalar::Float(to)) => Some(match to.width().cmp(&from.width()) {
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Less => Some(ConvOp::FloatTrunc),
            std::cmp::Ordering::Greater => Some(ConvOp::FloatExtend),
        }),
        // `Bool as Int`, `Char as Int`. Zero-extending, narrowing or neither,
        // by width — and never sign-extending, because `sext i8 1` of a `true`
        // byte is still 1 but `sext` is the wrong statement about a value whose
        // type has no sign.
        (from, Scalar::Int(to)) => {
            let wide = as_unsigned_source(from)?;
            let narrow = to.width(target);
            Some(match narrow.cmp(&wide) {
                std::cmp::Ordering::Equal => None,
                std::cmp::Ordering::Less => Some(ConvOp::Trunc),
                std::cmp::Ordering::Greater => Some(ConvOp::ZeroExtend),
            })
        }
        // `U8 as Char`, and nothing else: every byte is a Unicode scalar value
        // and no wider integer is.
        (Scalar::Int(from), Scalar::Char) if !from.is_signed() && from.width(target) == 1 => {
            Some(Some(ConvOp::ZeroExtend))
        }
        (Scalar::Char, Scalar::Char) | (Scalar::Bool, Scalar::Bool) => Some(None),
        _ => None,
    }
}

fn describe_rvalue(rvalue: &Rvalue) -> &'static str {
    match rvalue {
        Rvalue::Use(_) => "an assignment",
        Rvalue::Ref { .. } => "a borrow",
        Rvalue::Unary { .. } => "a unary operator",
        Rvalue::Binary { .. } => "arithmetic",
        Rvalue::Cast { .. } => "a cast",
        Rvalue::Record { .. } => "a record literal",
        Rvalue::Variant { .. } => "a choice variant",
        Rvalue::Tuple(_) => "a tuple",
        Rvalue::Range { .. } => "a range",
        Rvalue::IsPresent(_) => "a `?` presence test",
        Rvalue::Discriminant(_) => "a discriminant read",
        Rvalue::Coerce { .. } => "a coercion",
        Rvalue::Narrow { .. } => "a narrowed read",
        Rvalue::Closure { .. } => "a closure",
        // **This used to say *"an expression the front end could not check"*,
        // and it went into `SC0400`'s slot — so `sciencec build` on a program
        // that `sciencec check` had just passed printed *"no `an expression the
        // front end could not check` backend is compiled into this
        // `sciencec`"*, which blames the front end for a program the front end
        // approved and blames the reader's source for a hole in the compiler.
        //
        // The occasion was a string interpolation: `f"{n}"` lexes, parses,
        // resolves, type-checks and formats, and reaches MIR as
        // `Rvalue::Error` because `science-mir`'s arm for it is not written.
        // That is `UNTYPED`'s situation one level up — §5's *"the mistake has
        // already been reported"* firing on a mistake nobody made — and the
        // message now says which of the two it is, because from here they are
        // the same value and only the wording can tell them apart.
        Rvalue::Error => {
            "an expression the front end replaced with a hole: its MIR is `Rvalue::Error`, and \
             if `sciencec check` passed the same program then nothing was wrong with it and the \
             gap is in a lowering rather than in the source"
        }
    }
}
