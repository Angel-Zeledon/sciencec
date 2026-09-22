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
//!   `T` into `T?` is `Coercion::Widen` ([`Lowerer::lower_widen`]).
//! - **`assign`'s §7**, in two of its three shapes: a borrow of a `Copy` type
//!   read as a value is one load through the pointer
//!   ([`Lowerer::copy_out_of_borrow`]), and `borrowed T` into `T?` is that load
//!   and then the widening above, in that order. The third —
//!   `(borrowed T)?` into `T?` — runs the copy only when the value is present
//!   and is refused, because a test and two edges are three basic blocks where
//!   MIR has one and Decision 5 says there is exactly one.
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
//! **Two refusals were not about effort, and neither is a refusal any more.**
//! Integer `/` and `%` and the two shifts are constructs this crate could
//! always emit an instruction for and could not do so *unguarded*: LLVM's
//! `sdiv` is **undefined** at a zero divisor rather than a trap, and a shift
//! by at least the operand's width is poison — not a wrong value, no value —
//! and each refusal said the guard was specified nowhere and emitted by
//! nothing above this crate. `science_codegen::backend::IntOp`'s own note
//! says *"division by zero is a panic the caller has already guarded"*, and
//! for a long time no caller did. `science-mir`'s `division_check` and
//! `shift_check` are that caller now, in that order, both built
//! `bounds_check`'s shape: the guard is basic blocks and MIR is where basic
//! blocks are made, so [`Lowerer::lower_binary`] receives an already-proven
//! operand and lowers it exactly as unconditionally as `add`.
//!
//! # Decision 5 holds here, and one block is invented rather than merged
//!
//! Decision 5 says *"every MIR basic block becomes exactly one LLVM basic
//! block"*. Every block of the *user's* `main` does, with one named exception:
//! [`Lowerer::lower_body`] walks `body.blocks()` and emits one [`ExtBlock`] per
//! MIR block, and nothing merges or splits — except a **flagged drop**, which
//! [`Lowerer::emit_flagged_drop`] turns into two: the MIR block's own
//! terminator becomes the test, and the call it used to end in moves to a
//! block this crate invents. `science-mir`'s §4 item 2 predicted exactly this
//! — *"a flagged drop breaks the decision by becoming three blocks"*, MIR's
//! one plus this crate's one plus the existing `target` it reuses as
//! "continue" — and `codegen-and-linking.md` Decision 5 carries the amendment
//! that note asked for. An **unflagged** drop is still one block: a drop that
//! runs nothing is a `br`, and a drop of a `String` is one call and a `br`.
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
use science_codegen::descriptor::{DescriptorTable, StringLiteral, TypeInfo, Vtable};
use science_codegen::diagnostics::construct_not_lowered;
use science_codegen::layout::{
    CgTy, Field as CgField, IntTy, Layout, Niche, PtrKind, Repr, Scalar, Triple,
    Variant as CgVariant, layout_of,
};
use science_codegen::mangle::{MonoKey, mangle};
use science_codegen::runtime::{
    OwnedNullableReturn, RUNTIME, RtAggregate, RtParam, RtRet, RuntimeFn, owned_nullable_return,
    runtime_fn,
};
use science_diagnostics::{Diagnostic, Span};
use science_mir::mir::{self, Body as MirBody, Constant, Rvalue, StatementKind, TerminatorKind};
use science_parser::ast::{BinaryOp, UnaryOp};
use science_resolve::hir::{self, DefId, DefKind, DefTable, Literal};
use science_types::Types;
use science_types::items::Declarations;
use science_types::methods::{Form, Found};
use science_types::ty::{GenericArg, Ty, TyKind};

use crate::emit::{ConvOp, ExtBlock, ExtBody, ExtInst};

/// A generic aggregate's parameters, bound to the arguments of one use.
///
/// **A map and not a `Substitution`.** `science_types::Substitution` is the
/// real thing and it is what every phase above Decision 42's line uses, but
/// applying one interns a `Ty` and interning needs `&mut Types`, which this
/// crate deliberately does not have. What a layout needs is structure, so the
/// binding is carried down the walk and read at each [`TyKind::Param`].
/// [`Lowerer::aggregate_env`] is where one is built and states the whole
/// argument.
type TyEnv = BTreeMap<DefId, Ty>;


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
                       diagnostic was reported for it. The four this compiler has met are a call \
                       to `print` or `write`, whose signatures `science-resolve`'s `builtins.rs` \
                       withdrew; a `for` loop's range and iterator temporaries; a binding whose \
                       value is an `if` expression over integer literals — `let n be if f: 1 \
                       else: 0`, which `let n be if f: 1i64 else: 0i64` fixes and which is §3's \
                       finding 20 met at an `if` rather than at a tuple; and a discriminant \
                       temporary, which is handled rather than refused";

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
             of what comes after — a script body, string literals and `print` of anything the \
             `f\"…\"` builder renders, `extern \"C\"` declarations and the calls to them, the \
             CFG, scalar arithmetic, functions with parameters, records, `choice`s and `match`, \
             `T?`, methods and associated functions on a concrete type, the prelude's `String` \
             methods, and drops of values that own nothing — and refuses everything else rather \
             than lowering a construct no execution test has ever run",
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
        RtRet::F64 => CgTy::Float(science_codegen::layout::FloatTy::F64),
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
    /// Decision 13's vtables, one per `(interface, concrete type)` pair any
    /// coercion in this module needed, in the order they were interned.
    pub vtables: Vec<Vtable>,
    /// Decision 20's `ScienceTypeInfo` descriptors, one per type a box in this
    /// module allocates for, in the order they were interned.
    pub descriptors: Vec<(String, TypeInfo)>,
    /// Decision 20's `ScienceMapInfo`s, in emission order.
    ///
    /// **A second list and not a second kind of entry**, because a
    /// `ScienceMapInfo` is a different *type* — two nested descriptors and two
    /// function pointers where a `ScienceTypeInfo` is two integers and one —
    /// and `Backend` has had `define_map_info` beside `define_type_info` since
    /// before either had a caller. This is the caller for the second.
    pub map_descriptors: Vec<(String, science_codegen::descriptor::MapInfo)>,
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

/// Where the payload of a [`Lowerer::lower_widen`] comes from.
///
/// Decision 6's widening has two callers that agree about everything except
/// this. `Coercion::Widen` widens a MIR operand, which is materialised at the
/// payload's own layout by [`Lowerer::typed_operand`];
/// `Coercion::CopyThenWiden` widens the result of §7's load, which already
/// exists as a value and was produced at the *referent's* layout. Naming the
/// difference is what lets the tag, the payload offset and Decision 19's
/// asymmetry be decided once.
enum Widened<'a> {
    /// The operand the coercion was applied to.
    Operand(&'a mir::Operand),
    /// §7's copy, already loaded, with the layout it was loaded at.
    Copied { value: ValueId, layout: Layout },
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
    /// The vtables interned so far, keyed by symbol through
    /// [`Lowerer::intern_vtable`]'s own search — the same shape `literals`
    /// uses, and for the same reason: two coercion sites that need one table
    /// must land on one global.
    vtables: Vec<Vtable>,
    /// Decision 20's descriptors, interned by monomorphisation key so that two
    /// boxes of one type name one global — that decision's *"never constructed
    /// twice"*, enforced by the table rather than by this crate remembering.
    descriptors: DescriptorTable,
    /// The symbols of every drop-glue function already emitted, which is both
    /// the dedup set and the recursion guard — [`Lowerer::intern_drop_glue`]
    /// claims a symbol before it builds the body, so a record reaching itself
    /// through a field finds the claim rather than recursing.
    glue: std::collections::BTreeSet<String>,
    /// The glue bodies, appended to the module's definitions by
    /// [`Lowerer::finish`].
    glue_definitions: Vec<(AbiSignature, ExtBody)>,
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
    /// **Keyed by symbol and no longer by definition, which is the whole of
    /// what monomorphisation changes here.** After `science_codegen::mono`
    /// runs, one [`DefId`] is several functions: `identity[Int]` and
    /// `identity[F64]` are two symbols, two signatures — their parameters are
    /// classified differently, an `i64` in a register against an `sret` slot —
    /// and one body. A map keyed by definition can hold exactly one of them,
    /// and the one it holds decides how *both* call sites pass their
    /// arguments. That is §9.2's finding with generics behind it, and it links
    /// cleanly.
    science: BTreeMap<String, AbiSignature>,
    /// Which symbol each definition had, for the callers that still only know
    /// a [`DefId`]: a vtable slot, and the fallback when the walk recorded no
    /// instance for a call site.
    ///
    /// **A lower bound on purpose.** A generic definition has several symbols
    /// and this keeps whichever was entered last, so it is only ever consulted
    /// where the definition is known to have one — which is every place a
    /// [`DefId`] is still enough. [`Lowerer::symbol_for_call`] states the rule
    /// and prefers the call map wherever there is one.
    by_def: BTreeMap<DefId, String>,
    /// `science_codegen::mono`'s call map, for the whole of
    /// [`Lowerer::lower_crate`]. `None` for a lowerer built for a stage the
    /// walk does not run in, where [`Lowerer::symbol_for_call`] falls back to
    /// the definition.
    calls: Option<&'a science_codegen::mono::MonoSet>,
}

/// Every definition `entry` can reach, through [`MirBody::callees`] **and
/// through every vtable a coercion in a reached body builds**.
///
/// A breadth-first walk over the bodies present, so a callee with no body — an
/// `extern "C"` declaration, a builtin — contributes nothing and is not an
/// error here; the call site is where those are resolved.
///
/// # The vtable half, and why `callees` alone is not enough
///
/// [`MirBody::callees`] reports the targets of `TerminatorKind::Call` with a
/// [`mir::Callee::Def`] in it — every call whose callee is known statically.
/// A method reached **only** through `any I` is never one of those: the call
/// site knows the interface and the slot, and the concrete implementation is
/// exactly the thing it does not know. So a walk over `callees` alone stops
/// one step short of every method a vtable holds, and the methods would be
/// left out of `Lowerer::science`, never lowered, and named by a vtable
/// global as functions the module does not define.
///
/// **That failure is caught rather than silent** — `define_vtable` refuses a
/// table naming a function the module has no definition for — which is why
/// this walk can be a fixed point over an easy-to-state rule rather than a
/// proof: what fills a vtable is a *coercion*, coercions are statements in a
/// body, and a body is only walked if something reaches it. The closure is
/// therefore over the same queue as the calls, and a method that itself
/// coerces something into an interface contributes its own tables on the pass
/// that reaches it.
///
/// **A pair this cannot resolve contributes nothing and is not an error
/// here.** `vtable_slots` refuses a default body and an unresolvable lookup
/// with a message naming both the method and the type; this walk would have
/// to invent a worse one, and the coercion that needs the table is lowered
/// later in the same run, where the refusal is in hand and has a span behind
/// it.
fn reachable_from(
    lowerer: &Lowerer<'_>,
    bodies: &[MirBody],
    entry: DefId,
) -> std::collections::BTreeSet<DefId> {
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
        for method in lowerer.vtable_methods_of(body) {
            if !reached.contains(&method) {
                queue.push(method);
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
            vtables: Vec::new(),
            descriptors: DescriptorTable::new(),
            glue: std::collections::BTreeSet::new(),
            glue_definitions: Vec::new(),
            declarations: Vec::new(),
            declared_foreign: Vec::new(),
            entry: None,
            science: BTreeMap::new(),
            by_def: BTreeMap::new(),
            calls: None,
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

    /// The interface an `any I` type names, seeing through a `T?` and a `Box`.
    ///
    /// `Error?` is `(any Error)?` — the nullable is the outside — so a
    /// `BoxThenWiden` coercion's destination arrives here wrapped and a `Box`
    /// coercion's does not. One function for both, because every caller wants
    /// the same answer and none of them wants to know which spelling it got.
    ///
    /// **`Box of any I` is peeled the same way, and for the same reason
    /// `Coercion::UnsizeInBox`'s destination needs one.** `assign.rs`'s §4a
    /// makes `Box of any I` a type a signature can name, and
    /// [`Lowerer::lower_unsize`] is reached with exactly that as `ty` — a
    /// coercion this function could not name is the whole of the refusal this
    /// change replaces.
    fn object_interface(&self, ty: Ty) -> Option<DefId> {
        if let Some(element) = self.box_element(ty) {
            return self.object_interface(element);
        }
        match self.types.kind(ty) {
            TyKind::Object { interface, .. } => Some(*interface),
            TyKind::Nullable(inner) => self.object_interface(*inner),
            // `borrowed any I` is the ordinary parameter spelling and
            // `Coercion::Unsize`'s destination; the borrow is ownership and
            // not representation — `cg_ty_at`'s own arm for it is the account
            // — so what the pair describes is the same interface.
            TyKind::Borrowed { inner, .. } => self.object_interface(*inner),
            // `Error?` also reaches this crate as a `Named` at the interface's
            // own definition — `cg_ty_at` has the same two arms for the same
            // reason, and its note is the account.
            TyKind::Named { def, .. } if self.defs.get(*def).kind == DefKind::Interface => {
                Some(*def)
            }
            _ => None,
        }
    }

    /// The definition a concrete type is headed by: the `Doc` of a `Doc`, of a
    /// `borrowed Doc`, and of a `Box of Doc`.
    ///
    /// `None` for anything with no single head — a parameter, a tuple, a
    /// closure — which is a coercion this backend cannot build a table for and
    /// which [`Lowerer::vtable_slots`] refuses by name rather than guessing.
    ///
    /// **`Box of Doc` is peeled to `Doc` rather than answering the box's own
    /// `DefId`,** which is what this function's own opening sentence always
    /// promised and what `Coercion::UnsizeInBox`'s operand — `Box of C`, read
    /// off [`Lowerer::lower_unsize`]'s `source` — is the first caller to need:
    /// the vtable is keyed on the concrete type `C`, never on `Box`. `Box of
    /// any I` peels to `any I`, which has no single head either, so the
    /// recursion answers `None` for it exactly as it must — a fat pointer's
    /// vtable is chosen by the coercion that built it, not read back out.
    fn concrete_head(&self, ty: Ty) -> Option<DefId> {
        if let Some(element) = self.box_element(ty) {
            return self.concrete_head(element);
        }
        match self.types.kind(ty) {
            TyKind::Named { def, .. } => Some(*def),
            TyKind::Borrowed { inner, .. } => self.concrete_head(*inner),
            _ => None,
        }
    }

    /// Decision 13's slot order: every method `interface` declares, in
    /// declaration order, resolved to the implementation `concrete` answers it
    /// with.
    ///
    /// **The order is read off the interface's children and nowhere else**, for
    /// `science_codegen::descriptor::Vtable`'s reason: the slot index is the
    /// only thing a call site through `any I` knows, so two implementations of
    /// one interface have to agree about it, and the only thing they share is
    /// the interface's own declaration.
    ///
    /// **This is now the fallback and no longer the only path — see
    /// [`Lowerer::vtable_method_symbols`], its one caller.** A default body
    /// used to be refused here outright: *"putting it in a vtable would need
    /// the body monomorphised at the implementor's `Self`, and this backend
    /// monomorphises nothing"* — true when this function was written and false
    /// since `science_codegen::mono::Instance::self_ty` gave a defaulted
    /// method one body per implementor. That repair lives in
    /// `science_codegen::mono::Mono::vtable_instances`, one crate up, where the
    /// instance a defaulted slot needs can be *enqueued* as well as named, so
    /// this function is reached at all only for a pair `mono`'s own walk never
    /// saw a coercion for — a program with a construct that walk misses for
    /// some other reason, which `Lowerer::reachable_from`'s own fallback union
    /// exists to catch. This function's refusal stays exactly as conservative
    /// as it always was for that residual case, because it has no way to tell
    /// an inherited default from an override the way `vtable_instances` does —
    /// it never needed to, until now, and giving it that rule too would be the
    /// second copy of one this codebase warns against everywhere else.
    fn vtable_slots(&self, interface: DefId, concrete: DefId) -> Result<Vec<DefId>, Unlowered> {
        let Some(decls) = self.decls else {
            return Err(Unlowered::new(
                "a vtable with no declaration table to read the interface's methods from",
            ));
        };
        let declared: Vec<(String, DefId)> = self
            .defs
            .children(interface)
            .filter(|def| def.kind == DefKind::Fn)
            .map(|def| (def.name.clone(), def.id))
            .collect();
        if declared.is_empty() {
            return Err(Unlowered::new(format!(
                "a value coerced into `any {}`, an interface this compiler declares no methods \
                 for: `builtins.rs` leaves fourteen of the prelude's interfaces as names with no \
                 method set, and a table with no slots is a table nothing can dispatch through",
                self.defs.get(interface).name
            )));
        }
        let mut slots = Vec::with_capacity(declared.len());
        for (name, declared_def) in &declared {
            if decls.signature(*declared_def).is_some_and(|sig| sig.has_body) {
                return Err(Unlowered::new(format!(
                    "`{}`'s default body in a vtable slot for `{}`: Decision 15's defaulted \
                     method has to be monomorphised at the implementor's `Self` before it has an \
                     address, and this backend monomorphises nothing",
                    name,
                    self.defs.get(concrete).name
                )));
            }
            match decls.methods().lookup(concrete, name, Form::Value) {
                Found::One(candidate) => slots.push(candidate.method),
                _ => {
                    return Err(Unlowered::new(format!(
                        "no single `{}` on `{}` to fill `any {}`'s slot for it: conformance is \
                         `science-types`' `conform` to report, and a table filled from an \
                         unresolved lookup would dispatch to whichever candidate came first",
                        name,
                        self.defs.get(concrete).name,
                        self.defs.get(interface).name
                    )));
                }
            }
        }
        Ok(slots)
    }

    /// The `(interface, concrete)` pair a coercion needs a table for, when it
    /// is one of the four that builds an interface object.
    ///
    /// `ty` is the coercion's *destination* — `any I`, or `(any I)?` for the
    /// widening pair — and the source comes off the operand, through one
    /// borrow if there is one. `None` for every other coercion, and for one
    /// whose ends this crate cannot name: a coercion out of a type parameter
    /// has no concrete head, which is monomorphisation's to supply.
    fn vtable_pair(
        &self,
        body: &MirBody,
        coercion: science_types::assign::Coercion,
        operand: &mir::Operand,
        ty: Ty,
    ) -> Option<(DefId, DefId)> {
        use science_types::assign::Coercion;
        if !matches!(
            coercion,
            Coercion::Box | Coercion::BoxThenWiden | Coercion::Unsize | Coercion::UnsizeInBox
        ) {
            return None;
        }
        let interface = self.object_interface(ty)?;
        let source = self.operand_ty(body, operand)?;
        let concrete = self.concrete_head(source)?;
        // A coercion whose source is already the interface is a widening of an
        // interface object, not the construction of one, and has no table of
        // its own to build.
        if concrete == interface {
            return None;
        }
        Some((interface, concrete))
    }

    /// Every method a vtable built by this body would hold.
    ///
    /// [`reachable_from`]'s second half. Pairs whose slots do not resolve are
    /// skipped here and refused at the coercion, which is the only place a
    /// message about them can say where the coercion was.
    fn vtable_methods_of(&self, body: &MirBody) -> Vec<DefId> {
        let mut out = Vec::new();
        for (_, block) in body.blocks() {
            for statement in &block.statements {
                let StatementKind::Assign { rvalue, .. } = &statement.kind else { continue };
                let Rvalue::Coerce { operand, coercion, ty } = rvalue else { continue };
                let Some((interface, concrete)) =
                    self.vtable_pair(body, *coercion, operand, *ty)
                else {
                    continue;
                };
                if let Ok(slots) = self.vtable_slots(interface, concrete) {
                    out.extend(slots);
                }
            }
        }
        out
    }

    /// The glue for a `T?` whose payload owns something: one test, one branch.
    ///
    /// **The present arm drops the payload and the absent arm does nothing**,
    /// which is what Decision 18's tagged layout makes a `T?` mean. The
    /// payload is released through whatever releases a bare `T` — a runtime
    /// call for a `String`, another glue function for a record — so this
    /// function knows nothing about payloads beyond where one sits.
    ///
    /// The cost: a branch in the hot path of releasing an option, where a
    /// niched one costs nothing at all. That is Decision 19's whole argument
    /// for the niche, and it applies to the options this is *not* reached for.
    fn emit_nullable_glue(
        &mut self,
        ty: Ty,
        payload_ty: Ty,
        name: String,
        symbol: String,
        depth: u32,
    ) -> Result<Option<String>, Unlowered> {
        let layout = self.layout_of_ty(ty)?;
        // **A niched `T?` is its payload, with one reserved bit pattern**, so
        // its glue is a null test and the payload's own release over the same
        // address — there is no separate payload to project to.
        //
        // This arm was the refusal this function's own message named
        // (*"neither Decision 18's tagged pair nor Decision 19's niche"*), and
        // until `any I` grew drop glue nothing could reach it: the five
        // niched payloads are `Box of T`, two borrows, a function pointer and
        // `any I`, and a borrow releases nothing, a function pointer owns
        // nothing, and the other two had no glue to call. `Error?` — which is
        // `(any Error)?` by `syntax-revision-2.md` §3.4 — is the shape that
        // arrives first, because Decision 14's implicit boxing is the one way
        // a program can own an interface object today.
        if let Repr::Niched { niche, niche_variants, .. } = &layout.repr {
            let (offset, variants) = (niche.offset, niche_variants.clone());
            return self.emit_niched_glue(payload_ty, name, symbol, offset, &variants, depth);
        }
        let Repr::Tagged { tag, payload_offset, variants } = &layout.repr else {
            return Err(Unlowered::new(format!(
                "drop glue for `{name}`, whose layout is neither Decision 18's tagged pair nor \
                 Decision 19's niche"
            )));
        };
        let (tag, payload_offset) = (*tag, *payload_offset);
        let present = variants
            .iter()
            .find(|variant| variant.payload.is_some())
            .ok_or_else(|| {
                Unlowered::new(format!(
                    "drop glue for `{name}`, whose variants all carry nothing: it is not a `T?`"
                ))
            })?
            .discriminant;

        let inner = self.intern_drop_glue(payload_ty, depth + 1)?;
        let direct = match &inner {
            Some(_) => None,
            None => self.direct_release(payload_ty)?,
        };
        let (callee, ret) = match (&inner, &direct) {
            (Some(glue), _) => (Callee::Science(glue.clone()), ReturnClass::Void),
            (None, Some((runtime, _))) => {
                (Callee::Runtime(runtime), self.declare(runtime)?.ret.clone())
            }
            (None, None) => {
                return Err(Unlowered::new(format!(
                    "drop glue for `{name}`: its payload owns something that is neither a \
                     runtime aggregate nor a type with glue"
                )));
            }
        };

        // **A `switch` and not a comparison**, because the discriminant is
        // already the value to branch on and `IntOp` has no equality: §3.3
        // puts the tag at offset 0, so one `LoadAt` of the tag's own width is
        // the whole test. The `default` arm is the absent case rather than an
        // `unreachable`, since an option has exactly the two.
        let tag_layout = layout_of(self.target, &CgTy::Int(tag));
        let loaded = ValueId(0);
        let address = ValueId(1);
        let entry = vec![ExtInst::LoadAt {
            dest: loaded,
            address: Operand::Param(0),
            layout: tag_layout,
        }];
        let mut release = vec![ExtInst::FieldAddr {
            dest: address,
            base: Operand::Param(0),
            offset: payload_offset,
        }];
        let mut next_value = 2u32;
        let args = match &direct {
            Some(d) => self.release_args(
                d,
                address,
                &mut || {
                    let id = ValueId(next_value);
                    next_value += 1;
                    id
                },
                &mut release,
            ),
            None => vec![Operand::Value(address)],
        };
        release.push(ExtInst::Above(Inst::Call {
            dest: None,
            callee,
            args,
            ret,
            sret_slot: None,
        }));

        let signature = AbiSignature::science(
            self.target,
            symbol.clone(),
            layout_of(self.target, &CgTy::Unit),
            vec![(
                "self".to_string(),
                layout_of(self.target, &CgTy::Ptr(PtrKind::MutBorrow)),
                ParamAttrs::default(),
            )],
        );
        let body = ExtBody {
            blocks: vec![
                ExtBlock {
                    id: BlockId(0),
                    label: "entry".to_string(),
                    insts: entry,
                    terminator: Terminator::Switch {
                        value: Operand::Value(loaded),
                        arms: vec![(present, BlockId(1))],
                        default: BlockId(2),
                    },
                },
                ExtBlock {
                    id: BlockId(1),
                    label: "present".to_string(),
                    insts: release,
                    terminator: Terminator::Goto(BlockId(2)),
                },
                ExtBlock {
                    id: BlockId(2),
                    label: "done".to_string(),
                    insts: Vec::new(),
                    terminator: Terminator::Return(None),
                },
            ],
        };
        self.glue_definitions.push((signature, body));
        Ok(Some(symbol))
    }

    /// One field's release inside a `choice`'s glue: the address at `offset`
    /// from `Param(0)`, and the call that runs `field_ty`'s glue — or its
    /// runtime free, for a `String`, an `Array`, a `Map` — over it.
    ///
    /// **The same shape `emit_field_glue`'s loop runs for a record's field**,
    /// pulled out on its own because [`Lowerer::emit_choice_glue`] runs it
    /// twice over: once per owning variant, and — for a variant whose payload
    /// has more than one field — once per owning field inside that payload
    /// too. A free function rather than a second copy of the loop keeps the
    /// two call sites from drifting the way the two refusals this feature
    /// replaces already had.
    fn emit_variant_release(
        &mut self,
        name: &str,
        field_ty: Ty,
        offset: u64,
        next_value: &mut u32,
        depth: u32,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        let inner = self.intern_drop_glue(field_ty, depth + 1)?;
        let direct = match &inner {
            Some(_) => None,
            None => self.direct_release(field_ty)?,
        };
        let address = ValueId(*next_value);
        *next_value += 1;
        insts.push(ExtInst::FieldAddr { dest: address, base: Operand::Param(0), offset });
        let (callee, ret) = match (&inner, &direct) {
            (Some(glue), _) => (Callee::Science(glue.clone()), ReturnClass::Void),
            (None, Some((runtime, _))) => {
                (Callee::Runtime(runtime), self.declare(runtime)?.ret.clone())
            }
            // `drop_runs_something` said this field owns something and
            // neither path claimed it — a hole in this crate rather than a
            // program the user wrote, so it is named rather than silently
            // releasing nothing. `emit_field_glue`'s own arm says the same.
            (None, None) => {
                return Err(Unlowered::new(format!(
                    "drop glue for `{name}`: a variant's payload of type `{}` owns something \
                     that is neither a runtime aggregate nor a type with glue",
                    self.types.render(self.defs, field_ty)
                )));
            }
        };
        let args = match &direct {
            Some(d) => self.release_args(
                d,
                address,
                &mut || {
                    let id = ValueId(*next_value);
                    *next_value += 1;
                    id
                },
                insts,
            ),
            None => vec![Operand::Value(address)],
        };
        insts.push(ExtInst::Above(Inst::Call { dest: None, callee, args, ret, sret_slot: None }));
        Ok(())
    }

    /// The glue for a `choice` with any number of variants, when at least one
    /// owns something: switch on the discriminant, and drop only the active
    /// variant's payload.
    ///
    /// # The decision
    ///
    /// **One arm per variant that owns something, not one arm per variant.**
    /// Decision 18's `switch` already has a `default`, and every payload-free
    /// variant — and every variant whose payload owns nothing, such as
    /// `Loaded(Int)` beside a `String`-carrying arm — falls through it to
    /// `done` directly. That is the note this task was given: a variant with
    /// nothing to release is worth skipping rather than building a block that
    /// runs no instruction and jumps on. `emit_nullable_glue` is this same
    /// shape at its smallest — one test, one branch, one `default` — and a
    /// general `choice` is the same `switch`, sized to however many variants
    /// turn out to own something.
    ///
    /// # The reason
    ///
    /// This is the shape the refusal named — *"one block per arm where this
    /// builds one block"* — and the sentence was wrong about the emitter, not
    /// about the requirement: `ExtBody::blocks` is a `Vec` and
    /// `Terminator::Switch` takes an arm list, so the arm count was never
    /// fixed at one and the refusal outlived the limit it described.
    ///
    /// # The cost
    ///
    /// **A variant's payload can own more than one field**
    /// (`Loaded(String, Int)`). §3.3 un-wraps a single-field payload onto
    /// `payload_offset` directly — [`Lowerer::choice_ty`]'s note is the
    /// account — but a multi-field payload is `choice_ty`'s own struct, so
    /// releasing it is a nested loop over that struct's `FieldPlace`s in
    /// reverse declaration order, the same order [`Lowerer::emit_field_glue`]
    /// runs for a record, with every offset shifted by `payload_offset`.
    fn emit_choice_glue(
        &mut self,
        def: DefId,
        args: &[GenericArg],
        name: String,
        symbol: String,
        layout: Layout,
        depth: u32,
    ) -> Result<Option<String>, Unlowered> {
        let Repr::Tagged { tag, payload_offset, variants } = layout.repr else {
            return Err(Unlowered::new(format!(
                "drop glue for `{name}`, whose layout is neither Decision 18's tagged union nor \
                 Decision 19's niche: a general `choice`'s glue only knows how to switch on \
                 Decision 18's shape"
            )));
        };
        // The type's parameters, bound to this use's arguments, so that a
        // variant declared `Loaded(T)` is released as the `String` or the
        // `Int` it actually is. `name` already carries the instantiation —
        // `intern_drop_glue` built the symbol from it — so the glue emitted
        // here is this instantiation's and no other's.
        let generics = self
            .choice_variants(def)
            .iter()
            .find_map(|variant| self.declarations().ok()?.variant(*variant).map(|v| v.generics.clone()))
            .unwrap_or_default();
        let (_, env) = self.aggregate_env(def, &generics, args, &TyEnv::new())?;
        let variant_defs = self.choice_variants(def);
        if variant_defs.len() != variants.len() {
            return Err(Unlowered::new(format!(
                "drop glue for `{name}`: its layout has {} variant(s) and its declaration has {}",
                variants.len(),
                variant_defs.len()
            )));
        }
        let decls = self.declarations()?;
        // `science_codegen::mono`'s substituted payloads, when this use is in
        // its table — [`Lowerer::choice_ty`]'s own doc comment says why a hit
        // is walked with a fresh `TyEnv` and a miss falls back to the
        // declared list bound by `env`.
        let mono_payloads =
            self.mono_aggregate_fields(def, args).and_then(|layout| match layout {
                science_codegen::mono::AggregateLayout::Choice(payloads) => Some(payloads),
                science_codegen::mono::AggregateLayout::Record(_) => None,
            });
        let no_binding = TyEnv::new();

        // ValueId(0) is the loaded tag; every `FieldAddr` after it, across
        // every arm, claims the next — `BodyState::values` is one map for the
        // whole function and not one per block, so numbering has to be too.
        let mut next_value = 1u32;
        let tag_layout = layout_of(self.target, &CgTy::Int(tag));
        let loaded = ValueId(0);
        let entry_insts =
            vec![ExtInst::LoadAt { dest: loaded, address: Operand::Param(0), layout: tag_layout }];

        // Built before any `BlockId` exists, because the `done` block's index
        // depends on how many variants end up with an arm at all — which is
        // known only after this loop decides which ones own something.
        let mut arm_bodies: Vec<(u64, Vec<ExtInst>)> = Vec::new();
        for (index, (place, variant_def)) in variants.iter().zip(variant_defs.iter()).enumerate() {
            let field_tys = match mono_payloads.and_then(|p| p.get(index)) {
                Some(substituted) => substituted
                    .iter()
                    .map(|ty| self.member_ty(*ty, &no_binding, &name))
                    .collect::<Result<Vec<Ty>, Unlowered>>()?,
                None => decls
                    .variant(*variant_def)
                    .ok_or_else(|| {
                        Unlowered::new(format!(
                            "the variant `{name}.{}`, which the declaration table has no lowered \
                             payload for",
                            place.name
                        ))
                    })?
                    .payload
                    .iter()
                    .map(|ty| self.member_ty(*ty, &env, &name))
                    .collect::<Result<Vec<Ty>, Unlowered>>()?,
            };
            // Whether *this* variant owns anything — not whether the choice
            // does, which `intern_drop_glue` already asked before this
            // function was reached. A payload-free variant's `field_tys` is
            // empty and this is trivially `false`; a `Loaded(Int)` beside a
            // `Loaded(String)` is the case that makes the check necessary.
            let mut owns = false;
            for field_ty in &field_tys {
                if self.drop_runs_something(*field_ty, 0)? {
                    owns = true;
                    break;
                }
            }
            if !owns {
                continue;
            }
            let mut insts = Vec::new();
            if field_tys.len() == 1 {
                // §3.3's un-wrapped payload: the one field sits at
                // `payload_offset` itself, with nothing of `choice_ty`'s
                // struct in between.
                self.emit_variant_release(
                    &name,
                    field_tys[0],
                    payload_offset,
                    &mut next_value,
                    depth,
                    &mut insts,
                )?;
            } else {
                // `choice_ty`'s struct for a multi-field payload: a nested
                // aggregate whose own `FieldPlace` offsets are relative to
                // where the payload starts, so the absolute offset is
                // `payload_offset` plus the field's own.
                let payload_layout = place.payload.as_ref().ok_or_else(|| {
                    Unlowered::new(format!(
                        "drop glue for `{name}.{}`: its declaration has a payload but its \
                         layout has none",
                        place.name
                    ))
                })?;
                let Repr::Aggregate { fields: field_places } = &payload_layout.repr else {
                    return Err(Unlowered::new(format!(
                        "drop glue for `{name}.{}`, whose payload layout is not Decision 17's \
                         aggregate one",
                        place.name
                    )));
                };
                if field_places.len() != field_tys.len() {
                    return Err(Unlowered::new(format!(
                        "drop glue for `{name}.{}`: its payload layout has {} field(s) and its \
                         declaration has {}",
                        place.name,
                        field_places.len(),
                        field_tys.len()
                    )));
                }
                // Reverse declaration order, `emit_field_glue`'s order and
                // for the same reason: nothing in F0 can observe the
                // difference today, and the day a `Drop` implementation
                // prints something is the day this would be wrong if it
                // were not followed.
                for (index, field_ty) in field_tys.iter().enumerate().rev() {
                    if !self.drop_runs_something(*field_ty, 0)? {
                        continue;
                    }
                    let offset = payload_offset + field_places[index].offset;
                    self.emit_variant_release(
                        &name,
                        *field_ty,
                        offset,
                        &mut next_value,
                        depth,
                        &mut insts,
                    )?;
                }
            }
            arm_bodies.push((place.discriminant, insts));
        }

        let done_id = BlockId((arm_bodies.len() + 1) as u32);
        let mut blocks = Vec::with_capacity(arm_bodies.len() + 2);
        let mut arms = Vec::with_capacity(arm_bodies.len());
        for (index, (discriminant, insts)) in arm_bodies.into_iter().enumerate() {
            let id = BlockId((index + 1) as u32);
            arms.push((discriminant, id));
            blocks.push(ExtBlock {
                id,
                label: format!("variant{index}"),
                insts,
                terminator: Terminator::Goto(done_id),
            });
        }
        blocks.insert(
            0,
            ExtBlock {
                id: BlockId(0),
                label: "entry".to_string(),
                insts: entry_insts,
                terminator: Terminator::Switch {
                    value: Operand::Value(loaded),
                    arms,
                    default: done_id,
                },
            },
        );
        blocks.push(ExtBlock {
            id: done_id,
            label: "done".to_string(),
            insts: Vec::new(),
            terminator: Terminator::Return(None),
        });

        let signature = AbiSignature::science(
            self.target,
            symbol.clone(),
            layout_of(self.target, &CgTy::Unit),
            vec![(
                "self".to_string(),
                layout_of(self.target, &CgTy::Ptr(PtrKind::MutBorrow)),
                ParamAttrs::default(),
            )],
        );
        self.glue_definitions.push((signature, ExtBody { blocks }));
        Ok(Some(symbol))
    }

    /// The glue for a type whose parts are **all always present**: a record's
    /// fields, or a tuple's elements.
    ///
    /// # The decision
    ///
    /// One function serves both, because releasing them is the same thing in
    /// the same order — reverse declaration order, one call per part that owns
    /// something — and the only difference is where the part *types* come
    /// from.
    ///
    /// # The reason
    ///
    /// A tuple had no glue at all, so nothing in the language could let
    /// `(a, b)` of two strings go out of scope, and the refusal it met spoke
    /// about a `choice`'s discriminant — a sentence about a different type.
    /// Its layout is Decision 17's aggregate exactly as a record's is; what it
    /// lacks is a definition for `concrete_head` to name, which is a fact
    /// about the symbol and not about the release.
    ///
    /// # The cost
    ///
    /// The symbol is keyed on the type's **rendering** rather than on a
    /// mangled path, because a tuple has no path. `Types::render` is injective
    /// over the types that reach here, which is the same argument
    /// `intern_element_descriptor` already makes for the same reason.
    fn emit_field_glue(
        &mut self,
        ty: Ty,
        name: &str,
        symbol: String,
        field_tys: Vec<Ty>,
        depth: u32,
    ) -> Result<Option<String>, Unlowered> {
        let layout = self.layout_of_ty(ty)?;
        let Repr::Aggregate { fields: places } = &layout.repr else {
            return Err(Unlowered::new(format!(
                "drop glue for `{name}`, whose layout is not Decision 17's aggregate one"
            )));
        };
        let places = places.clone();
        
        if field_tys.len() != places.len() {
            return Err(Unlowered::new(format!(
                "drop glue for `{name}`: its layout has {} field(s) and its declaration has {}",
                places.len(),
                field_tys.len()
            )));
        }

        let pointer = layout_of(self.target, &CgTy::Ptr(PtrKind::MutBorrow));
        let mut insts: Vec<ExtInst> = Vec::new();
        let mut next_value = 0u32;
        // Reverse declaration order.
        for (index, field_ty) in field_tys.iter().enumerate().rev() {
            if !self.drop_runs_something(*field_ty, 0)? {
                continue;
            }
            let inner = self.intern_drop_glue(*field_ty, depth + 1)?;
            let direct = match &inner {
                Some(_) => None,
                None => self.direct_release(*field_ty)?,
            };
            let address = ValueId(next_value);
            next_value += 1;
            insts.push(ExtInst::FieldAddr {
                dest: address,
                base: Operand::Param(0),
                offset: places[index].offset,
            });
            let (callee, ret) = match (&inner, &direct) {
                (Some(glue), _) => (Callee::Science(glue.clone()), ReturnClass::Void),
                (None, Some((symbol, _))) => {
                    (Callee::Runtime(symbol), self.declare(symbol)?.ret.clone())
                }
                // `drop_runs_something` said the field owns something, and
                // neither path claimed it. That is a hole in this crate and
                // not a program the user can be told about, so it is refused
                // by name rather than releasing nothing.
                (None, None) => {
                    return Err(Unlowered::new(format!(
                        "drop glue for `{name}`: its field of type `{}` owns something that is \
                         neither a runtime aggregate nor a record with glue",
                        self.types.render(self.defs, *field_ty)
                    )));
                }
            };
            let args = match &direct {
                Some(d) => self.release_args(
                    d,
                    address,
                    &mut || {
                        let id = ValueId(next_value);
                        next_value += 1;
                        id
                    },
                    &mut insts,
                ),
                None => vec![Operand::Value(address)],
            };
            insts.push(ExtInst::Above(Inst::Call {
                dest: None,
                callee,
                args,
                ret,
                sret_slot: None,
            }));
        }

        let signature = AbiSignature::science(
            self.target,
            symbol.clone(),
            layout_of(self.target, &CgTy::Unit),
            vec![("self".to_string(), pointer, ParamAttrs::default())],
        );
        let body = ExtBody {
            blocks: vec![ExtBlock {
                id: BlockId(0),
                label: "entry".to_string(),
                insts,
                terminator: Terminator::Return(None),
            }],
        };
        self.glue_definitions.push((signature, body));
        Ok(Some(symbol))
    }


    /// The glue for a **niched** `T?`: one null test, one call, same address.
    ///
    /// `emit_nullable_glue`'s niche arm carries the argument for when this is
    /// reached; this is its body.
    ///
    /// **The payload's release is given `Param(0)` unchanged**, and that is
    /// the whole difference from the tagged case. Decision 19 makes a niched
    /// option *"its payload, which is also the whole type's layout"* — there
    /// is no tag beside the value and no offset to project past — so the
    /// present arm releases the very bytes that were tested.
    fn emit_niched_glue(
        &mut self,
        payload_ty: Ty,
        name: String,
        symbol: String,
        niche_offset: u64,
        niche_variants: &[(usize, u64)],
        depth: u32,
    ) -> Result<Option<String>, Unlowered> {
        // The same two conditions `lower_is_present`'s niche arm states, for
        // the same reason: F0's niche offers exactly one value and it is the
        // null pointer, so the test is a comparison against `null` rather
        // than against an integer. A second value would be a different
        // instruction, and refusing here keeps the two readers of a niche
        // agreeing about what one holds.
        if niche_offset != 0 {
            return Err(Unlowered::new(format!(
                "drop glue for `{name}`, whose niche is not at offset 0"
            )));
        }
        if niche_variants.iter().any(|(_, value)| *value != 0) {
            return Err(Unlowered::new(format!(
                "drop glue for `{name}`, whose niche encodes a value that is not the null pointer"
            )));
        }

        let inner = self.intern_drop_glue(payload_ty, depth + 1)?;
        let direct = match &inner {
            Some(_) => None,
            None => self.direct_release(payload_ty)?,
        };
        let (callee, ret) = match (&inner, &direct) {
            (Some(glue), _) => (Callee::Science(glue.clone()), ReturnClass::Void),
            (None, Some((runtime, _))) => {
                (Callee::Runtime(runtime), self.declare(runtime)?.ret.clone())
            }
            (None, None) => {
                return Err(Unlowered::new(format!(
                    "drop glue for `{name}`: its payload owns something that is neither a \
                     runtime aggregate nor a type with glue"
                )));
            }
        };

        let pointer = layout_of(self.target, &CgTy::Ptr(PtrKind::Box));
        let loaded = ValueId(0);
        let present = ValueId(1);
        let entry = vec![
            ExtInst::LoadAt {
                dest: loaded,
                address: Operand::Param(0),
                layout: pointer,
            },
            ExtInst::Above(Inst::Cmp {
                dest: present,
                op: CmpOp::Ne,
                signed: false,
                lhs: Operand::Value(loaded),
                rhs: Operand::Null,
            }),
        ];

        let mut release: Vec<ExtInst> = Vec::new();
        let mut next_value = 2u32;
        // `Param(0)` and not a projection: the payload *is* the value.
        let address = ValueId(next_value);
        next_value += 1;
        release.push(ExtInst::FieldAddr {
            dest: address,
            base: Operand::Param(0),
            offset: 0,
        });
        let args = match &direct {
            Some(d) => self.release_args(
                d,
                address,
                &mut || {
                    let id = ValueId(next_value);
                    next_value += 1;
                    id
                },
                &mut release,
            ),
            None => vec![Operand::Value(address)],
        };
        release.push(ExtInst::Above(Inst::Call {
            dest: None,
            callee,
            args,
            ret,
            sret_slot: None,
        }));

        let signature = AbiSignature::science(
            self.target,
            symbol.clone(),
            layout_of(self.target, &CgTy::Unit),
            vec![(
                "self".to_string(),
                layout_of(self.target, &CgTy::Ptr(PtrKind::MutBorrow)),
                ParamAttrs::default(),
            )],
        );
        self.glue_definitions.push((
            signature,
            ExtBody {
                blocks: vec![
                    ExtBlock {
                        id: BlockId(0),
                        label: "entry".to_string(),
                        insts: entry,
                        terminator: Terminator::Branch {
                            cond: Operand::Value(present),
                            then_block: BlockId(1),
                            else_block: BlockId(2),
                        },
                    },
                    ExtBlock {
                        id: BlockId(1),
                        label: "present".to_string(),
                        insts: release,
                        terminator: Terminator::Goto(BlockId(2)),
                    },
                    ExtBlock {
                        id: BlockId(2),
                        label: "done".to_string(),
                        insts: Vec::new(),
                        terminator: Terminator::Return(None),
                    },
                ],
            },
        ));
        Ok(Some(symbol))
    }

    /// Decision 12's glue for an `any I`: release through the table.
    ///
    /// `intern_drop_glue`'s `Object` arm carries the argument; this is the
    /// four loads and the call it describes.
    ///
    /// **Why the data word is loaded rather than passed by address.**
    /// `science_box_free` takes its pointer **by value** — `release_args`
    /// already says so for the `Box[T]` case and this is the same entry point
    /// — so what it wants is the heap pointer the fat pointer's first word
    /// holds, not the address of that word.
    fn emit_object_glue(
        &mut self,
        interface: DefId,
        rendered: &str,
        symbol: String,
    ) -> Result<Option<String>, Unlowered> {
        let pointer = layout_of(self.target, &CgTy::Ptr(PtrKind::Box));
        let word = pointer.size;
        // Every method the interface declares, which is how long each of its
        // tables is and therefore where the descriptor sits.
        let methods = self
            .defs
            .children(interface)
            .filter(|def| def.kind == DefKind::Fn)
            .count();
        if methods == 0 {
            return Err(Unlowered::new(format!(
                "drop glue for `{rendered}`, an interface this compiler declares no methods for: \
                 a table with no slots has nowhere to put the descriptor, and `vtable_slots` \
                 refuses to build one for the same reason"
            )));
        }

        let mut insts: Vec<ExtInst> = Vec::new();
        let mut next = 0u32;
        let mut fresh = || {
            let id = ValueId(next);
            next += 1;
            id
        };

        // The table, out of the pair's second word.
        let table_at = fresh();
        insts.push(ExtInst::FieldAddr { dest: table_at, base: Operand::Param(0), offset: word });
        let table = fresh();
        insts.push(ExtInst::LoadAt {
            dest: table,
            address: Operand::Value(table_at),
            layout: pointer.clone(),
        });

        // The descriptor, out of the table's slot after the methods.
        let descriptor_at = fresh();
        insts.push(ExtInst::FieldAddr {
            dest: descriptor_at,
            base: Operand::Value(table),
            offset: word * methods as u64,
        });
        let descriptor = fresh();
        insts.push(ExtInst::LoadAt {
            dest: descriptor,
            address: Operand::Value(descriptor_at),
            layout: pointer.clone(),
        });

        // The data pointer, out of the pair's first word.
        let data_at = fresh();
        insts.push(ExtInst::FieldAddr { dest: data_at, base: Operand::Param(0), offset: 0 });
        let data = fresh();
        insts.push(ExtInst::LoadAt {
            dest: data,
            address: Operand::Value(data_at),
            layout: pointer.clone(),
        });

        let ret = self.declare("science_box_free")?.ret.clone();
        let callee = Callee::Runtime("science_box_free");
        insts.push(ExtInst::Above(Inst::Call {
            dest: None,
            callee,
            args: vec![Operand::Value(descriptor), Operand::Value(data)],
            ret,
            sret_slot: None,
        }));

        let signature = AbiSignature::science(
            self.target,
            symbol.clone(),
            layout_of(self.target, &CgTy::Unit),
            vec![(
                "self".to_string(),
                layout_of(self.target, &CgTy::Ptr(PtrKind::MutBorrow)),
                ParamAttrs::default(),
            )],
        );
        self.glue_definitions.push((
            signature,
            ExtBody {
                blocks: vec![ExtBlock {
                    id: BlockId(0),
                    label: "entry".to_string(),
                    insts,
                    terminator: Terminator::Return(None),
                }],
            },
        ));
        Ok(Some(symbol))
    }

    /// Decision 12's glue for one type, emitted once and interned by symbol.
    ///
    /// **The decision. Glue is an ordinary definition this crate appends to
    /// the module, not a new kind of thing the backend has to know about.**
    /// A glue function takes a pointer to the value and releases what the
    /// value owns, in reverse declaration order; that is a signature and a
    /// body, which is what [`Lowered::definitions`] already carries and what
    /// [`Lowerer::lower_c_main`] already builds by hand. Adding a `define_*`
    /// for it would be a second path to the same emitter.
    ///
    /// **Reverse declaration order, because `science_codegen::descriptor`'s
    /// `drop_glue` says so** and because it is the order a reader of the
    /// source would run destructors in if the fields were bindings. Nothing
    /// in F0 can observe the difference today — a `String`'s release has no
    /// side effect a program can see — and the order is followed anyway,
    /// because the first type whose `Drop` implementation prints something
    /// would observe it and the glue would be wrong in a way no test written
    /// before that day could catch.
    ///
    /// **`None` is an answer and not a failure**: a type that owns nothing
    /// needs no glue, which is the case Decision 20's `drop_fn: null` exists
    /// for and the case that keeps this out of the hot path for an
    /// `Array of F64`.
    ///
    /// # What it does not do
    ///
    /// **A `choice` whose payload owns something used to be refused here**,
    /// with the sentence *"releasing one means switching on the discriminant
    /// and dropping only the active variant, which is a `switch` and one
    /// block per arm inside a function this builds as a single block"*. The
    /// sentence was wrong about this function rather than about `choice`:
    /// [`Lowerer::emit_choice_glue`] is that `switch`, and this function now
    /// reaches it for `DefKind::Choice` the same way it reaches
    /// [`Lowerer::emit_field_glue`] for `DefKind::Record`.
    /// `science_codegen::descriptor::drop_glue` still answers `fields: vec![]`
    /// for anything that is not `Repr::Aggregate`, which is a hole in that
    /// table and not in this one — nothing above the line reads a `choice`'s
    /// descriptor for its glue today, so nothing yet depends on the answer
    /// being wrong.
    ///
    /// **The linkage is external where Decision 12 asks for `internal`.**
    /// `sys::linkage::INTERNAL` exists and `declare_function` has no way to
    /// be told, so glue is visible in the symbol table. That costs an
    /// optimisation and a tidy symbol table; it costs no correctness, and the
    /// alternative today is a second definition path.
    fn intern_drop_glue(&mut self, ty: Ty, depth: u32) -> Result<Option<String>, Unlowered> {
        if depth > MAX_TYPE_DEPTH {
            return Err(Unlowered::new(
                "a type nested past this crate's depth bound while building its drop glue",
            ));
        }
        if !self.drop_runs_something(ty, 0)? {
            return Ok(None);
        }
        // The owning types with no glue of their own: a direct runtime call,
        // which every caller emits inline rather than through a function. The
        // `Ok(None)` is why every caller has to ask `direct_release` too.
        if self.direct_release(ty)?.is_some() {
            return Ok(None);
        }
        // **A tuple is a record whose fields have no names**, and that is the
        // whole of what it needed. Its layout is Decision 17's aggregate, its
        // elements are all always present, and dropping it is the same loop in
        // the same order — so the only thing standing between `let t be (a, b)`
        // and a drop was that `concrete_head` answers `None` for a type with no
        // definition behind it, and the refusal below spoke about a `choice`.
        //
        // Nothing in this language could release a tuple of two strings, which
        // means nothing could let one go out of scope. `field_value` made
        // `("alpha", "beta")` constructible earlier today and that was only
        // half a feature: every value eventually goes out of scope.
        // **A `T?` whose payload owns something, which is one branch.**
        //
        // The decision. The glue tests the discriminant, drops the payload on
        // the present arm, and joins. A niched option needs none of this: its
        // payload is a borrow, which owns nothing, so `drop_runs_something`
        // has already answered `false` and this is never reached for one.
        //
        // The reason. §5.3's convention materialises a `V?` for **every**
        // `Map.insert` and `Map.remove`, so a `Map[String, String]` could not
        // be inserted into at all — not even an empty one with no previous
        // value to free, and not even when the result was discarded. The
        // refusal said the concrete type could not be named, which was true of
        // `concrete_head` and beside the point: what was missing was the
        // branch.
        //
        // **The branch was always available.** `ExtBody::blocks` is a `Vec`
        // and `Terminator::Branch` has been there since the CFG was; the
        // sentence *"one block per arm where this builds one block"* described
        // how this function was written, not what the emitter can do.
        if let TyKind::Nullable(payload) = *self.types.kind(ty) {
            let rendered = self.types.render(self.defs, ty);
            let symbol = format!("{}.drop", mangle(&MonoKey::plain(&[rendered.as_str()])));
            if self.glue.contains(&symbol) {
                return Ok(Some(symbol));
            }
            self.glue.insert(symbol.clone());
            return self.emit_nullable_glue(ty, payload, rendered, symbol, depth);
        }
        if let TyKind::Tuple(elements) = self.types.kind(ty) {
            let elements: Vec<Ty> = elements.clone();
            let rendered = self.types.render(self.defs, ty);
            let symbol = format!("{}.drop", mangle(&MonoKey::plain(&[rendered.as_str()])));
            if self.glue.contains(&symbol) {
                return Ok(Some(symbol));
            }
            self.glue.insert(symbol.clone());
            return self.emit_field_glue(ty, &rendered, symbol, elements, depth);
        }
        // **An `any I` releases what it holds through its own table**, which
        // is the one thing a release of an interface object can reach the
        // concrete type through.
        //
        // The decision. Decision 13's fat pointer is `{ data, vtable }`, and
        // the vtable now carries the concrete type's `ScienceTypeInfo` in a
        // slot after every method. So the glue loads the table out of the
        // pair, loads the descriptor out of the table, loads the data
        // pointer, and makes the same `science_box_free(descriptor, data)`
        // call an ordinary `Box[T]`'s glue already makes — with the
        // descriptor read at run time instead of known when the glue was
        // emitted.
        //
        // The reason it is one function per **interface** and not per
        // concrete type: the concrete type is exactly what this glue does not
        // know, and the indirection is the point. A glue per implementor
        // would need the caller to choose between them, which is dispatch
        // again, one layer down.
        //
        // The slot index is the interface's method count, which is a property
        // of the interface alone — `vtable_slots` derives the order from the
        // interface's children and every implementor's table is that long —
        // so it is read here without naming a concrete type. `descriptor.rs`
        // puts the descriptor *after* the methods for exactly this reason: a
        // method's own index must not move because this slot exists.
        //
        // The cost is one word per vtable, once per (interface, concrete)
        // pair, whether or not anything ever drops one.
        if let TyKind::Object { interface, .. } = *self.types.kind(ty) {
            let rendered = self.types.render(self.defs, ty);
            let symbol = format!("{}.drop", mangle(&MonoKey::plain(&[rendered.as_str()])));
            if self.glue.contains(&symbol) {
                return Ok(Some(symbol));
            }
            self.glue.insert(symbol.clone());
            return self.emit_object_glue(interface, &rendered, symbol);
        }
        // **`Box of any I` releases through the interface's own glue, not a
        // glue of its own.** `assign.rs`'s §4a and `science-rt`'s `boxed`
        // module agree that `Box of any I` is `{ data, vtable }` — the exact
        // layout [`Lowerer::cg_ty_in`]'s `Box`-of-object arm now gives it —
        // and not a second heap allocation wrapping that pair: the allocation
        // is the one `Box.new`'s own call made, and freeing it is
        // `science_box_free(descriptor, data)` read out of the vtable, which
        // is precisely what the `Object` arm above already builds. Delegating
        // rather than emitting a second function is what keeps a program that
        // drops both a bare owned `any I` (through `Error?`) and a `Box of any
        // I` from getting two copies of one body.
        if let Some(element) = self.box_element(ty) {
            if self.object_interface(element).is_some() {
                return self.intern_drop_glue(element, depth);
            }
        }
        let Some(def) = self.concrete_head(ty) else {
            return Err(Unlowered::new(format!(
                "drop glue for `{}`, whose concrete type this crate cannot name",
                self.types.render(self.defs, ty)
            )));
        };
        // **The name is the instantiated one, and one symbol per
        // instantiation is the whole of what generics change here.**
        // `Holder[String]` owns a `String` and `Holder[Int]` owns nothing, so
        // they need different glue; keyed on the bare `Holder` the first to
        // arrive would have answered for both, and the one that got the other's
        // glue would either leak or free an integer.
        let args: Vec<GenericArg> = match self.types.kind(ty) {
            TyKind::Named { args, .. } => args.clone(),
            _ => Vec::new(),
        };
        let (name, _) = self.aggregate_members(def, &args)?;
        let symbol = format!("{}.drop", mangle(&MonoKey::plain(&[name.as_str()])));
        if self.glue.contains(&symbol) {
            return Ok(Some(symbol));
        }
        // Recorded *before* the body is built, so a record or a `choice` that
        // reaches itself through a field or a variant asks for a symbol that
        // is already claimed rather than recursing until the stack runs out.
        // The depth bound above is the second guard and this is the first.
        self.glue.insert(symbol.clone());

        // **A `choice` whose payload owns something, which is `emit_choice_glue`'s
        // `switch` over however many variants own something.** This used to be
        // the refusal this function's own doc comment quoted — *"one block per
        // arm where this builds one block"* — and the sentence was wrong about
        // the emitter rather than the requirement, the same mistake
        // `emit_nullable_glue` and `emit_field_glue` each found half of already.
        if self.defs.get(def).kind == DefKind::Choice {
            let layout = self.layout_of_ty(ty)?;
            return self.emit_choice_glue(def, &args, name, symbol, layout, depth);
        }
        self.emit_field_glue(ty, &name, symbol, self.record_field_types(def, &args)?, depth)
    }

    /// A record's field types, in declaration order.
    ///
    /// `science_codegen::mono`'s table answers this directly when this use is
    /// in it — [`Lowerer::record_ty`]'s own doc comment says why a hit needs
    /// no further binding — and the declared list bound by [`member_ty`]
    /// survives as the fallback.
    ///
    /// [`member_ty`]: Lowerer::member_ty
    fn record_field_types(&self, def: DefId, args: &[GenericArg]) -> Result<Vec<Ty>, Unlowered> {
        if let Some(science_codegen::mono::AggregateLayout::Record(fields)) =
            self.mono_aggregate_fields(def, args)
        {
            return Ok(fields.clone());
        }
        let name = self.defs.get(def).name.clone();
        let decls = self.declarations()?;
        let record = decls.record(def).ok_or_else(|| {
            Unlowered::new(format!(
                "drop glue for `{name}`, which the declaration table has no lowered field list for"
            ))
        })?;
        let (_, env) = self.aggregate_env(def, &record.generics, args, &TyEnv::new())?;
        record.fields.iter().map(|(_, ty)| self.member_ty(*ty, &env, &name)).collect()
    }

    /// The name of one use of a record or `choice`, and **every type it
    /// contains**, with its parameters bound: a record's fields in
    /// declaration order, or a `choice`'s payloads across every variant.
    ///
    /// # Why the two are one function
    ///
    /// Everything that asks this asks it structurally — *does anything in
    /// here need releasing*, *what is the layout of what is inside* — and for
    /// those questions a record's fields and a `choice`'s payloads are the
    /// same list. The one caller that needs them kept apart is
    /// [`Lowerer::emit_choice_glue`], which needs to know which variant each
    /// payload belongs to so it can put it behind the right `switch` arm, and
    /// that function reads the variants itself.
    ///
    /// # Why it returns a list of `Ty` and not an environment
    ///
    /// [`Lowerer::aggregate_env`] carries a binding down a walk without ever
    /// interning, which is what a *layout* needs. The drop path is different:
    /// it hands member types to half a dozen functions —
    /// `intern_drop_glue`, `drop_runs_something`, `emit_field_glue`,
    /// `emit_choice_glue` — and threading an environment through all of them
    /// would put the same parameter in every signature to serve two callers.
    ///
    /// So the binding is *resolved here instead*, which is possible because a
    /// member type that is exactly a parameter resolves to a `Ty` that
    /// already exists. What that cannot do is resolve a member whose type is
    /// a compound mentioning a parameter — `Array[T]` inside `Holder[T]` —
    /// because that `Ty` does not exist until something interns it.
    /// [`Lowerer::member_ty`] refuses that by name rather than laying it out
    /// wrong, and the cost is stated there.
    ///
    /// **That refusal is now a fallback and not the last word.** When this use
    /// is one `science_codegen::mono`'s walk reached,
    /// [`Lowerer::mono_aggregate_fields`] already has every member
    /// substituted — interned above Decision 42's line, where `&mut Types`
    /// still is — and this function returns that list directly, never
    /// calling [`Lowerer::member_ty`] at all. The declared-list walk through
    /// `member_ty` survives for a use the walk did not reach, which is the
    /// same fallback [`Lowerer::record_ty`] keeps and for the same reason.
    fn aggregate_members(
        &self,
        def: DefId,
        args: &[GenericArg],
    ) -> Result<(String, Vec<Ty>), Unlowered> {
        let name = self.defs.get(def).name.clone();
        let decls = self.declarations()?;
        match self.defs.get(def).kind {
            DefKind::Record => {
                let Some(record) = decls.record(def) else { return Ok((name, Vec::new())) };
                let (rendered, env) =
                    self.aggregate_env(def, &record.generics, args, &TyEnv::new())?;
                if let Some(science_codegen::mono::AggregateLayout::Record(fields)) =
                    self.mono_aggregate_fields(def, args)
                {
                    return Ok((rendered, fields.clone()));
                }
                let members = record
                    .fields
                    .iter()
                    .map(|(_, ty)| self.member_ty(*ty, &env, &name))
                    .collect::<Result<Vec<Ty>, Unlowered>>()?;
                Ok((rendered, members))
            }
            _ => {
                let variants = self.choice_variants(def);
                let generics = variants
                    .iter()
                    .find_map(|variant| decls.variant(*variant).map(|v| v.generics.clone()))
                    .unwrap_or_default();
                let (rendered, env) = self.aggregate_env(def, &generics, args, &TyEnv::new())?;
                if let Some(science_codegen::mono::AggregateLayout::Choice(payloads)) =
                    self.mono_aggregate_fields(def, args)
                {
                    let members = payloads.iter().flatten().copied().collect();
                    return Ok((rendered, members));
                }
                let mut members = Vec::new();
                for variant in variants {
                    // A variant the declaration table has no entry for is a
                    // payload nobody lowered, and the caller must not read
                    // "no payload" out of it — `choice_ty` makes the same
                    // distinction for the same reason.
                    let Some(declared) = decls.variant(variant) else {
                        return Err(Unlowered::new(format!(
                            "the variant `{name}.{}`, which the declaration table has no lowered \
                             payload for",
                            self.defs.get(variant).name
                        )));
                    };
                    for payload in &declared.payload {
                        members.push(self.member_ty(*payload, &env, &name)?);
                    }
                }
                Ok((rendered, members))
            }
        }
    }

    /// One declared member type, with the aggregate's parameters bound.
    ///
    /// **Only ever reached for a use `science_codegen::mono`'s table has no
    /// entry for** — [`Lowerer::aggregate_members`] and
    /// [`Lowerer::record_field_types`] both ask
    /// [`Lowerer::mono_aggregate_fields`] first and return its answer without
    /// calling this at all when it has one. So a member reaching this
    /// function is a member of an aggregate the monomorphisation walk never
    /// instantiated, and the three cases below are what is still true without
    /// that table: a type mentioning no parameter is itself; a type that
    /// **is** a parameter is whatever the environment bound it to, which is a
    /// lookup; a compound mentioning a parameter would have to be built, and
    /// building a `Ty` is interning, and this crate holds a `&Types`
    /// precisely so that it cannot — so it is refused, by a message that
    /// names the type and says where the work belongs.
    fn member_ty(&self, ty: Ty, env: &TyEnv, owner: &str) -> Result<Ty, Unlowered> {
        if !self.mentions_a_parameter(ty, 0) {
            return Ok(ty);
        }
        if let TyKind::Param { def } = self.types.kind(ty) {
            if let Some(bound) = env.get(def) {
                return Ok(*bound);
            }
        }
        Err(Unlowered::new(format!(
            "a member of `{owner}` whose type is `{}` — a compound mentioning one of the type's \
             own parameters. Binding it would mean interning a new `Ty`, and this crate is \
             handed a `&Types` so that it cannot; the substitution belongs above Decision 42's \
             line, and `science_mir::instantiate` reaches a body's types rather than a \
             declaration's field list",
            self.types.render(self.defs, ty)
        )))
    }

    /// Intern Decision 20's descriptor for a concrete type boxed into an
    /// interface object (Decision 14), returning the global every call site
    /// must name.
    ///
    /// **`drop_fn` is real now, and this is the function that used to refuse
    /// instead.** It read *"a descriptor claiming there is nothing to drop
    /// would leak the value every time the box is freed"* — true when nothing
    /// downstream of this call could reach the concrete type again, which was
    /// every downstream at the time: `TerminatorKind::Drop` refused an
    /// interface object outright, and [`Lowerer::intern_vtable`] built a table
    /// with nowhere to put this descriptor's address even if one existed.
    /// Both gaps are `Lowerer::emit_interface_drop_glue`'s and
    /// `science_codegen::descriptor::Vtable::descriptor`'s to close, and
    /// closing them is what makes the leak this comment warned about
    /// impossible rather than merely unlikely: the descriptor this function
    /// builds is the *only* one the vtable's own slot will ever name, so a
    /// caller that reaches this function twice for one concrete type gets the
    /// one glue function both times.
    ///
    /// **`source` is stripped of its one borrow, or its one `Box`, before
    /// anything is asked of it.** [`Lowerer::lower_unsize`]'s operand is `&C`
    /// for `Coercion::Unsize` and `Box of C` for `Coercion::UnsizeInBox`, and
    /// [`Lowerer::lower_box`]'s is `C` itself; asking
    /// [`Lowerer::drop_runs_something`] about the borrowed or boxed spelling
    /// answers a different question from the one this function has to —
    /// *"does releasing this pointer run something"* rather than *"does every
    /// value of the concrete type this vtable names"* — and for a borrow it is
    /// `false` unconditionally, which is correct about the reference and wrong
    /// about the type. [`Lowerer::box_element`] peels the one `Box`
    /// `Coercion::UnsizeInBox` ever puts around a source this function sees,
    /// the same way [`Lowerer::concrete_head`] does for the `DefId` beside it;
    /// [`Lowerer::referent`] peels the one borrow left when it does not, so
    /// one or the other is always enough.
    fn intern_descriptor(&mut self, concrete: DefId, source: Ty) -> Result<String, Unlowered> {
        let name = self.defs.get(concrete).name.clone();
        let cg = self.record_or_choice_ty(concrete)?;
        let referent = self.box_element(source).unwrap_or_else(|| self.referent(source));
        let drop_fn = self.intern_drop_glue(referent, 0)?;
        let info = science_codegen::descriptor::type_info(self.target, &cg, drop_fn);
        Ok(self.descriptors.intern(&MonoKey::plain(&[name.as_str()]), info))
    }

    /// `Array of T`'s element, when the type is one.
    ///
    /// The head has to be the **prelude's** `Array`, for the reason
    /// `science-types`' `array_element` gives: a user may declare a type of
    /// that name. This crate has no `WANTED` list of its own, so the check is
    /// the name plus the arity plus `DefKind::Record`'s absence — every
    /// builtin generic is none of the kinds a user's `Array` could be.
    fn array_element(&self, ty: Ty) -> Option<Ty> {
        let TyKind::Named { def, args } = self.types.kind(self.referent(ty)) else {
            return None;
        };
        if self.defs.get(*def).name != "Array" || args.len() != 1 {
            return None;
        }
        match args[0] {
            GenericArg::Type(element) => Some(element),
            _ => None,
        }
    }

    /// The element type a runtime call's descriptor should describe.
    ///
    /// **Read off whichever operand is the array, and the destination is the
    /// fallback.** `science_array_push(array, info, value)` has the array in
    /// front of it; `science_array_with_capacity(info, n)` has no array
    /// operand at all, because the array is what it *returns* — so the
    /// destination is where the element type lives for that one. Asking the
    /// operands first and the destination second covers both without either
    /// call site being named here.
    fn array_operand_element(
        &self,
        body: &MirBody,
        args: &[mir::Operand],
        destination: &mir::Place,
    ) -> Option<Ty> {
        for arg in args {
            if let Some(place) = arg.place() {
                if let Some(element) = self.array_element(place.ty(body)) {
                    return Some(element);
                }
            }
        }
        self.array_element(destination.ty(body))
    }

    /// The element type `science_box_new`'s descriptor should describe.
    ///
    /// **The destination and nothing else, unlike
    /// [`Lowerer::array_operand_element`]'s two-step search.** `Box.new`'s one
    /// argument is `T` itself and not `Box of T` — that is the entire point of
    /// an associated function that builds the indirection rather than
    /// receiving one — so no operand is ever `Box`-shaped for
    /// [`Lowerer::box_element`] to find. `T` is nowhere but the destination
    /// Decision 42's checker already solved this call's return type into.
    fn box_operand_element(&self, body: &MirBody, destination: &mir::Place) -> Option<Ty> {
        self.box_element(destination.ty(body))
    }

    /// Decision 20's descriptor for an element type, whatever kind it is.
    ///
    /// **Wider than [`Lowerer::intern_descriptor`] on purpose.** That one
    /// answers for the concrete type behind a box, which is a record or a
    /// `choice` by construction. An array's element is any type at all —
    /// `Array of Int`, `Array of Doc`, `Array of String` — and what a
    /// descriptor needs of it is a size, an alignment and whether it owns
    /// something, all three of which `cg_ty` and `drop_runs_something` answer
    /// for every type this backend lays out.
    ///
    /// **The key is the element's rendered name**, which is what makes two
    /// `Array of Int` literals in one module share one global. It is not a
    /// mangled path because an element may be a type with no path — a tuple,
    /// a `T?` — and Decision 16's mangling has no spelling for those; the
    /// rendering does, and it is injective over the types that reach here.
    fn intern_element_descriptor(&mut self, element: Ty) -> Result<String, Unlowered> {
        // **Decision 20's `drop_fn`, filled in.** This used to refuse any
        // element that owned something, on the ground that *"this backend emits
        // glue for a record and names it in no descriptor yet"* — the first
        // half of which was already false when it was written, and the second
        // half is what this fills. `science_array_free` runs `drop_fn` over the
        // live prefix of the buffer, and `drop_fn`'s signature is a pointer to
        // one element and nothing back, which is exactly the signature of both
        // things that can go there: `intern_drop_glue`'s glue function for a
        // record, and `science_string_free` for a `String`.
        //
        // **An element whose release needs a descriptor of its own is still
        // refused, and `Array of (Array of T)` is the whole of that set.**
        // `science_array_free(P, D)` takes two arguments where `drop_fn` calls
        // with one, so a nested array's element cannot be named here at all;
        // closing it means a one-argument thunk per element type, which is a
        // function this crate would emit for no program that has run.
        let drop_fn = match self.direct_release(element)? {
            Some((_, Some(_))) => {
                return Err(Unlowered::new(format!(
                    "an `Array of {}`, whose element is released by a runtime call that takes a \
                     descriptor of its own: Decision 20's `drop_fn` is called with the element's \
                     address and nothing else, and there is no one-argument symbol to name",
                    self.types.render(self.defs, element)
                )));
            }
            Some((symbol, None)) => Some(symbol.to_string()),
            None => self.intern_drop_glue(element, 0)?,
        };
        let cg = self.cg_ty(element)?;
        let info = science_codegen::descriptor::type_info(self.target, &cg, drop_fn);
        let rendered = self.types.render(self.defs, element);
        Ok(self.descriptors.intern(&MonoKey::plain(&[rendered.as_str()]), info))
    }

    /// `Box of T`'s element, when the type is one.
    ///
    /// [`Lowerer::array_element`]'s reason, one container over: the head has
    /// to be the **prelude's** `Box`, because a user may declare a type of
    /// that name, and every builtin generic is none of the kinds a user's
    /// `Box` could be.
    fn box_element(&self, ty: Ty) -> Option<Ty> {
        let TyKind::Named { def, args } = self.types.kind(self.referent(ty)) else {
            return None;
        };
        if self.defs.get(*def).name != "Box" || args.len() != 1 {
            return None;
        }
        match args[0] {
            GenericArg::Type(element) => Some(element),
            _ => None,
        }
    }

    /// `Map of (K, V)`'s key and value, when the type is one.
    fn map_key_value(&self, ty: Ty) -> Option<(Ty, Ty)> {
        let TyKind::Named { def, args } = self.types.kind(self.referent(ty)) else {
            return None;
        };
        if self.defs.get(*def).name != "Map" || args.len() != 2 {
            return None;
        }
        match (&args[0], &args[1]) {
            (GenericArg::Type(key), GenericArg::Type(value)) => Some((*key, *value)),
            _ => None,
        }
    }

    /// The key and value a map call's descriptor should describe.
    ///
    /// [`Lowerer::array_operand_element`]'s rule, one container over: ask the
    /// operands first and the destination second. `science_map_new(D)` has no
    /// map operand at all — the map is what it *returns* — so the destination
    /// is where the types live for that one, and every other entry point has
    /// the map in front of it.
    fn map_operand_kv(
        &self,
        body: &MirBody,
        args: &[mir::Operand],
        destination: &mir::Place,
    ) -> Option<(Ty, Ty)> {
        for arg in args {
            if let Some(place) = arg.place() {
                if let Some(pair) = self.map_key_value(place.ty(body)) {
                    return Some(pair);
                }
            }
        }
        self.map_key_value(destination.ty(body))
    }

    /// Decision 20's `ScienceMapInfo` for one key/value pair.
    ///
    /// # The decision
    ///
    /// Two nested `ScienceTypeInfo`s, built by the same
    /// [`Lowerer::intern_element_descriptor`] an array's element goes through,
    /// plus the key's `hash_fn` and `eq_fn` read from
    /// [`science_codegen::runtime::map_key_support`].
    ///
    /// # The reason the pair is asked for rather than derived
    ///
    /// **The tempting default is wrong exactly where it would be reached.**
    /// Hashing the key's bytes and comparing the key's bytes looks like it
    /// works for every key; it does not, because a record with padding has
    /// bytes that take no part in equality, so two values that *are* equal can
    /// hash differently. A table cannot detect that — it loses entries as a
    /// function of what the allocator last left in the padding, which is a bug
    /// that reproduces on one machine and not the next. Refusing costs a
    /// diagnostic; defaulting costs that.
    ///
    /// So `map_key_support` names a pair for `String` and for the eight-byte
    /// integers and refuses everything else, and this turns that refusal into
    /// a sentence naming the key type.
    ///
    /// # The cost
    ///
    /// A `Map` keyed by `U8` or by a record is refused today although both are
    /// perfectly sensible keys. Each integer width needs its own symbol pair,
    /// because a `ScienceHashFn` is handed a `*const u8` and no size and so
    /// cannot ask how wide its key is — an eight-byte read from a one-byte
    /// slot reads past the key array.
    fn intern_map_descriptor(&mut self, key: Ty, value: Ty) -> Result<String, Unlowered> {
        let key_cg = self.cg_ty(key)?;
        let Some((hash, eq)) = science_codegen::runtime::map_key_support(&key_cg) else {
            return Err(Unlowered::new(format!(
                "a `Map` keyed by `{}`, for which no `hash_fn`/`eq_fn` pair exists:                  `runtime::map_key_support` names one for `String` and for the eight-byte                  integers and refuses the rest, because a byte-wise default hashes a record's                  padding and would lose entries as a function of what the allocator left there",
                self.types.render(self.defs, key)
            )));
        };
        // Declared so the module has a `declare` for each: a `ScienceMapInfo`
        // holding the address of a symbol nothing declared is a global that
        // fails to link, which is `define_vtable`'s lesson one descriptor over.
        self.declare(hash)?;
        self.declare(eq)?;
        let key_symbol = self.intern_element_descriptor(key)?;
        let value_symbol = self.intern_element_descriptor(value)?;
        let key_info = self
            .descriptors
            .type_infos()
            .find(|(symbol, _)| *symbol == &key_symbol)
            .map(|(_, info)| info.clone())
            .expect("just interned");
        let value_info = self
            .descriptors
            .type_infos()
            .find(|(symbol, _)| *symbol == &value_symbol)
            .map(|(_, info)| info.clone())
            .expect("just interned");
        let rendered = format!(
            "{},{}",
            self.types.render(self.defs, key),
            self.types.render(self.defs, value)
        );
        let info = science_codegen::descriptor::MapInfo {
            key: key_info,
            value: value_info,
            hash_fn: hash.to_string(),
            eq_fn: eq.to_string(),
        };
        Ok(self.descriptors.intern_map(&MonoKey::plain(&[rendered.as_str()]), info))
    }

    /// The [`CgTy`] of a record or choice definition, for a descriptor.
    fn record_or_choice_ty(&self, def: DefId) -> Result<CgTy, Unlowered> {
        match self.defs.get(def).kind {
            // No arguments and no environment: this reaches a *definition*
            // from a descriptor, not a use, so there is nothing bound and a
            // generic one is refused inside — which is the same answer it
            // gave before generic aggregates had a layout at all.
            DefKind::Record => self.record_ty(def, &[], 0, &TyEnv::new()),
            DefKind::Choice => self.choice_ty(def, &[], 0, &TyEnv::new()),
            _ => Err(Unlowered::new(format!(
                "a box of `{}`, which is neither a record nor a `choice`: a descriptor needs a \
                 size and an alignment, and this crate lays out neither for it",
                self.defs.get(def).name
            ))),
        }
    }

    /// Intern the vtable for `(interface, concrete)`, returning the global
    /// every coercion to that pair must name.
    ///
    /// **`source` is the coercion's own operand type**, threaded through to
    /// [`Lowerer::intern_descriptor`] for the drop slot every table now
    /// carries. It is not an extra fact about the pair: `concrete` already
    /// determines it up to a borrow, and `source` is only here because
    /// stripping that borrow ([`Lowerer::referent`]) is [`Lowerer::intern_descriptor`]'s
    /// job and not this function's to repeat. A pair interned once from a
    /// borrowing call site (`Lowerer::lower_unsize`) and never from an owning
    /// one still gets a real descriptor, because the table is a fact about the
    /// concrete type and not about which coercion happened to build it first.
    fn intern_vtable(
        &mut self,
        interface: DefId,
        concrete: DefId,
        source: Ty,
    ) -> Result<Vtable, Unlowered> {
        let symbol = Vtable::symbol_for(
            &MonoKey::plain(&[self.defs.get(interface).name.as_str()]),
            &MonoKey::plain(&[self.defs.get(concrete).name.as_str()]),
        );
        if let Some(existing) = self.vtables.iter().find(|v| v.symbol == symbol) {
            return Ok(existing.clone());
        }
        let methods = self.vtable_method_symbols(interface, concrete)?;
        let descriptor = self.intern_descriptor(concrete, source)?;
        let vtable = Vtable { symbol, methods, descriptor };
        self.vtables.push(vtable.clone());
        Ok(vtable)
    }

    /// The method symbols one `(interface, concrete)` vtable's slots hold, in
    /// declaration order.
    ///
    /// **Read from `science_codegen::mono::MonoSet` when the walk built this
    /// pair, and resolved the old way — [`Lowerer::vtable_slots`]'s `DefId`s,
    /// each mapped through [`Lowerer::symbol_of`] — when it did not.**
    /// `MonoSet::vtable` is `Mono::collect`'s own answer to the *same*
    /// coercion this function was called for, computed while the concrete
    /// operand type — and therefore the instance a defaulted method's slot
    /// needs — was still in reach to enqueue. That is not a convenience: a
    /// defaulted method reached this way is `Instance { self_ty: Some(_), .. }`,
    /// one body per implementor, and this crate has no `&mut
    /// science_types::ty::Types` to build that substitution with — reading the
    /// mono-computed symbol is the only way this function can name the right
    /// one rather than refuse it, which is what it did before `MonoSet` carried
    /// an answer.
    ///
    /// The fallback exists for exactly the gap `Lowerer::lower_crate`'s own
    /// comment still names: a pair `science_codegen::mono`'s walk never saw a
    /// coercion for, because the body it is in reached this crate through
    /// `Lowerer::reachable_from`'s union and not through the walk. Every slot
    /// such a pair can name is a required method — an inherited default would
    /// already have made `mono` enqueue an instance for it the moment any
    /// instantiation of that default body exists anywhere in the reachable
    /// program — so [`Lowerer::vtable_slots`]'s older refusal is still the
    /// honest answer there and not a second, silently weaker copy of the rule
    /// `science_codegen::mono::Mono::vtable_instances` now owns.
    fn vtable_method_symbols(&self, interface: DefId, concrete: DefId) -> Result<Vec<String>, Unlowered> {
        if let Some(mono) = self.calls {
            if let Some(result) = mono.vtable(interface, concrete) {
                return result.clone().map_err(Unlowered::new);
            }
        }
        Ok(self.vtable_slots(interface, concrete)?.into_iter().map(|m| self.symbol_of(m)).collect())
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

    /// [`Lowerer::cg_ty_at`] with no type parameters bound, which is every
    /// caller outside the two generic-aggregate arms.
    fn cg_ty_at(&self, ty: science_types::ty::Ty, depth: u32) -> Result<CgTy, Unlowered> {
        self.cg_ty_in(ty, depth, &TyEnv::new())
    }

    /// [`Lowerer::cg_ty`], counting how deep the nesting has gone.
    ///
    /// **The depth is a guard against a hang, not against a wrong answer.** A
    /// record that contains itself has no finite layout and the type checker is
    /// what rejects it; a code generator that met one anyway would recurse until
    /// the stack ran out, which is a compiler that dies with no diagnostic. The
    /// bound is far above anything a program writes and the refusal names both
    /// causes, because from here they are indistinguishable.
    fn cg_ty_in(
        &self,
        ty: science_types::ty::Ty,
        depth: u32,
        env: &TyEnv,
    ) -> Result<CgTy, Unlowered> {
        if depth > MAX_TYPE_DEPTH {
            return Err(Unlowered::new(format!(
                "a type nested more than {MAX_TYPE_DEPTH} deep, or a type that contains itself \
                 and therefore has no size"
            )));
        }
        match self.types.kind(ty) {
            TyKind::Unit => Ok(CgTy::Unit),
            TyKind::Nullable(inner) => Ok(CgTy::nullable(self.cg_ty_in(*inner, depth + 1, env)?)),
            // `any I` — Decision 13's two-word fat pointer, with the null niche
            // in the data pointer (§3.4).
            TyKind::Object { .. } => Ok(CgTy::Interface),
            // **A borrow of an interface object is itself two words, and this
            // arm used to say one.** `borrowed any I` is the ordinary way an
            // interface object is passed — `def report(err: borrowed any
            // Error)` in `examples/09_absence_and_failure.science`, `def
            // describe_any(value: borrowed any Summarize)` in
            // `examples/08_dyn_dispatch.science` — and a dispatch through it
            // needs the vtable the borrow carries. A one-word `Ptr` has
            // nowhere to put it, so `Coercion::Unsize` would have had nothing
            // to write and the call site nothing to read.
            //
            // It was one word because nothing could build one: all four
            // coercions that produce an interface object were refused, so the
            // arm was never reached with an `Object` inside it and the
            // shorthand held. `Box of any I` is the same shape for the same
            // reason and is reached through the `Named` arms below.
            //
            // The ownership difference between `any I` and `borrowed any I` is
            // real and is not a representation difference: both are
            // `{ data, vtable }`, and which one frees the data is
            // `TerminatorKind::Drop`'s question, exactly as it is for
            // `Box of C` against `borrowed C`.
            TyKind::Borrowed { inner, .. } if self.object_interface(*inner).is_some() => {
                Ok(CgTy::Interface)
            }
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
            TyKind::Named { def, args } if self.defs.get(*def).kind == DefKind::Record => {
                self.record_ty(*def, args, depth, env)
            }
            // §3.3's Decision 18 and §3.4's Decision 19, both of which
            // `layout_of` already implements: what is needed here is the list
            // of variants in declaration order, because **declaration order is
            // what fixes the discriminant values** and nothing below this
            // function can recover it.
            TyKind::Named { def, args } if self.defs.get(*def).kind == DefKind::Choice => {
                self.choice_ty(*def, args, depth, env)
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
            // `Array of T` — §2.6's runtime aggregate, whose *value* is the
            // three-word `ScienceArray` and whose element travels beside every
            // operation as a `ScienceTypeInfo`. The descriptor is not part of
            // the value and so not part of this: `Lowerer::intern_descriptor`
            // interns one per element type and
            // `Lowerer::lower_runtime_call` puts it in the call.
            TyKind::Named { def, args }
                if args.len() == 1 && self.defs.get(*def).name == "Array" =>
            {
                Ok(RtAggregate::Array.cg_ty())
            }
            // `Map of (K, V)` — §2.6's other runtime aggregate. Its *value* is
            // the six-word `ScienceMap`; its key and value travel beside every
            // operation inside one `ScienceMapInfo`, which carries two nested
            // `ScienceTypeInfo`s **and** the key's `hash_fn` and `eq_fn`. That
            // pair is the whole of what makes a map harder than an array: the
            // runtime has no fallback for either, and the descriptor is the
            // only place they can come from.
            TyKind::Named { def, args }
                if args.len() == 2 && self.defs.get(*def).name == "Map" =>
            {
                Ok(RtAggregate::Map.cg_ty())
            }
            // `Box of T` — §2.6's third runtime container, and the one whose
            // *value* is not an aggregate at all: Decision 19's null niche
            // makes it one word, `CgTy::Ptr(PtrKind::Box)`, with no
            // `ScienceTypeInfo` inside the type the way `Array`'s three words
            // and `Map`'s six carry none either. The descriptor a `Box.new` or
            // a drop needs is built at the *use* and not read back out of this
            // arm — `Lowerer::intern_element_descriptor` interns one per
            // element type, the same table `Array`'s and `Map`'s elements
            // share, because what a box frees is exactly what an array frees
            // one element of.
            //
            // **`Box of any I` is the one `Box` this arm must not answer this
            // way.** `assign.rs`'s §4a and `science-rt`'s `boxed` module both
            // say `Box of any I` is wider — "a pointer to the value and a
            // pointer to the vtable" — which is `CgTy::Interface`'s own two
            // words and not a one-word `Ptr(Box)`. `object_interface` is
            // already the test the line above it reuses for `borrowed any I`
            // against a `Named` at the interface itself, and asking it of the
            // argument rather than of `ty` is what tells a `Box of Doc` from a
            // `Box of any Summarize` here.
            TyKind::Named { def, args }
                if args.len() == 1
                    && self.defs.get(*def).name == "Box"
                    && matches!(args[0], GenericArg::Type(inner) if self.object_interface(inner).is_some()) =>
            {
                Ok(CgTy::Interface)
            }
            TyKind::Named { def, args }
                if args.len() == 1 && self.defs.get(*def).name == "Box" =>
            {
                Ok(CgTy::Ptr(PtrKind::Box))
            }
            TyKind::Named { args, .. } if !args.is_empty() => Err(Unlowered::new(format!(
                "a value of type `{}`, one of §2.6's runtime containers: its value is a \
                 `science-rt` aggregate reached through a `ScienceTypeInfo` descriptor, and this \
                 backend emits a descriptor for an element type and a vtable for an interface, \
                 and has no lowering for this one",
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
                    // Half precision, which this language is not entitled to
                    // treat as an exotic corner: `F16` and `BF16` are what a
                    // model's weights are stored in. A value in a local, a
                    // record field or an array element is an ordinary scalar
                    // here; what stays refused is **crossing the `extern "C"`
                    // boundary by value**, which `science_codegen::abi`'s
                    // `SC0431` now actually fires for — it had a rendered
                    // diagnostic and no reachable path, because the layout model
                    // had no half-precision variant to match on.
                    "F16" => Ok(CgTy::Float(science_codegen::layout::FloatTy::F16)),
                    "BF16" => Ok(CgTy::Float(science_codegen::layout::FloatTy::Bf16)),
                    "F32" => Ok(CgTy::Float(science_codegen::layout::FloatTy::F32)),
                    "F64" => Ok(CgTy::Float(science_codegen::layout::FloatTy::F64)),
                    // **`Float` is to `F64` what `Int` is to `I64`, and only
                    // one of the pair was here.** Decision 2 defaults an
                    // integer literal to `I64` and a floating-point literal to
                    // `F64`, and the prelude spells the two defaults `Int` and
                    // `Float`; `Int` has had its row since stage 1 and `Float`
                    // never did, so every program that wrote the word was
                    // refused as *"a value of type `Float`"* — a sentence about
                    // a type the language does have.
                    "Float" => Ok(CgTy::Float(science_codegen::layout::FloatTy::F64)),
                    "Bool" => Ok(CgTy::Bool),
                    "Char" => Ok(CgTy::Char),
                    "String" => Ok(RtAggregate::String.cg_ty()),
                    // `Chars`, §8's one iterator: `{ ptr, len, offset }`. It
                    // **borrows** the string it walks and owns nothing, so it
                    // needs no `drop_fn` and no descriptor — which is what
                    // makes it the one runtime aggregate that is an ordinary
                    // value here.
                    "Chars" => Ok(RtAggregate::Chars.cg_ty()),
                    "IoError" => Ok(RtAggregate::IoError.cg_ty()),
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
            // **A parameter bound by the aggregate this field belongs to.**
            // `Pair[A, B]`'s field list is written in `A` and `B`, and
            // [`Lowerer::record_ty`] binds them to the arguments the use
            // wrote before walking the fields. So a `Param` reached with an
            // environment that knows it is not an un-monomorphised type — it
            // is `Int`, spelled the way the *declaration* spells it.
            TyKind::Param { def } if env.contains_key(def) => {
                self.cg_ty_in(env[def], depth + 1, &TyEnv::new())
            }
            TyKind::Param { .. } | TyKind::SelfType { .. } | TyKind::SelfAssoc { .. } => {
                Err(Unlowered::new(
                    "a value whose type is still a type parameter, which nothing has \
                     monomorphised: Decision 42 puts the walk above this crate and no phase runs \
                     it yet",
                ))
            }
            // A closure's *type* says nothing about what it captures —
            // `collections-and-chains.md` §1.2 makes it a bare arrow,
            // `(A) -> B` — so this layout cannot depend on which closure
            // literal produced a given value; it has to be one answer for
            // every closure of this type. `CgTy::Ptr(PtrKind::Fn)`
            // unconditionally is exactly right for the only closure this
            // crate can build a *value* of: one with nothing captured,
            // `science-mir`'s `Rvalue::Closure` with an empty `captures`
            // (that crate's `lower.rs` §8.5). A captured closure needs the
            // aggregate its own doc comment describes, `{ fn ptr, captures }`,
            // and this type has no room to say how many or of what — the same
            // gap `science-mir`'s `lib.rs` §7 item 5 names. Nothing builds a
            // value of a captured closure's type yet, so this layout being
            // wrong for one is not reachable; [`Lowerer::lower_rvalue`]'s own
            // `Rvalue::Closure` arm is where a captured closure is refused, by
            // name, rather than here.
            TyKind::Closure { .. } => Ok(CgTy::Ptr(PtrKind::Fn)),
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
    ///
    /// **A field's type comes from `science_codegen::mono`'s table when this
    /// use is in it, and from the declared list bound by `env` otherwise.**
    /// `MonoSet::aggregate_fields` is filled with every generic record's
    /// members already substituted at `args`, interned above Decision 42's
    /// line — see its own doc comment — and a field found there needs no
    /// further binding, which is why it is walked with a fresh, empty
    /// [`TyEnv`] rather than `inner`. The declared-list path survives as the
    /// fallback for a use the walk did not reach — a definition emitted only
    /// through [`Lowerer::lower_crate`]'s reachability union, or a nested
    /// aggregate whose argument is exactly an *enclosing* parameter, which
    /// [`Lowerer::aggregate_env`]'s first arm already resolves by lookup and
    /// which `mono` never had a reason to intern a fresh `Ty` for.
    fn record_ty(
        &self,
        def: DefId,
        args: &[GenericArg],
        depth: u32,
        env: &TyEnv,
    ) -> Result<CgTy, Unlowered> {
        let record = self.declarations()?.record(def).ok_or_else(|| {
            Unlowered::new(format!(
                "a value of type `{}`, which the declaration table has no lowered field list for",
                self.defs.get(def).name
            ))
        })?;
        let (name, inner) = self.aggregate_env(def, &record.generics, args, env)?;
        let substituted = self.mono_aggregate_fields(def, args).and_then(|layout| match layout {
            science_codegen::mono::AggregateLayout::Record(fields) => Some(fields),
            science_codegen::mono::AggregateLayout::Choice(_) => None,
        });
        let mut fields = Vec::with_capacity(record.fields.len());
        if let Some(substituted) = substituted {
            let no_binding = TyEnv::new();
            for ((field, _), ty) in record.fields.iter().zip(substituted) {
                fields.push(CgField::new(
                    self.defs.get(*field).name.clone(),
                    self.cg_ty_in(*ty, depth + 1, &no_binding)?,
                ));
            }
        } else {
            for (field, ty) in &record.fields {
                fields.push(CgField::new(
                    self.defs.get(*field).name.clone(),
                    self.cg_ty_in(*ty, depth + 1, &inner)?,
                ));
            }
        }
        Ok(CgTy::strukt(name, fields))
    }

    /// `science_codegen::mono`'s substituted member list for one use of a
    /// generic record or `choice`, when the monomorphisation walk reached it.
    ///
    /// `None` whenever `self.calls` is unset — a caller with no `MonoSet` at
    /// all, which today is only this crate's own tests that build a
    /// [`Lowerer`] directly — or when `def`/`args` names a use the walk did
    /// not enqueue. Either is the same fallback: bind the declared list by
    /// hand, exactly as every use resolved before this table existed.
    fn mono_aggregate_fields(
        &self,
        def: DefId,
        args: &[GenericArg],
    ) -> Option<&'a science_codegen::mono::AggregateLayout> {
        self.calls.and_then(|mono| mono.aggregate_fields(def, args))
    }

    /// The name and the parameter bindings for one use of a record or a
    /// `choice`.
    ///
    /// # The decision
    ///
    /// A generic aggregate's layout is computed by walking its **declared**
    /// field list with its parameters bound to the arguments of this use,
    /// rather than by substituting the field types into new ones and walking
    /// those.
    ///
    /// # The reason, and it is the same wall Decision 42 draws
    ///
    /// Substituting a `Ty` interns a `Ty`, and interning needs `&mut Types`.
    /// This crate is handed a `&Types` **on purpose** — `BuildInput`'s own
    /// documentation says a backend must not be able to invent a type — and
    /// that is exactly why `science_mir::instantiate` runs above the line and
    /// hands down concrete bodies. But a body's *locals* being concrete does
    /// not make a generic record's declared field list concrete: `Pair[A, B]`
    /// is declared once, in `A` and `B`, and no instantiation of a body
    /// rewrites the declaration.
    ///
    /// A layout needs structure and never a `Ty`, so the binding is carried
    /// down the walk instead of applied to the type table. Nothing is
    /// interned and the `&Types` holds.
    ///
    /// # The name, which is load-bearing
    ///
    /// `science_codegen::layout::LayoutCache` is keyed by a struct's name and
    /// answers `SC0404` when one name gets two layouts, so `Pair[Int, Int]`
    /// and `Pair[Int, F64]` must not share one. The name is therefore built
    /// from the arguments, exactly as [`Lowerer::tuple_ty`] builds a tuple's
    /// from its elements and for the same reason. A use with no arguments
    /// keeps the bare name it has always had, so no existing layout record
    /// moves.
    ///
    /// # The cost, stated as the refusal it produces
    ///
    /// An argument that is itself a compound mentioning an outer parameter —
    /// a field of type `Inner[Array[A]]` inside `Outer[A]` — cannot be bound,
    /// because binding it would mean building the `Ty` for `Array[Int]`,
    /// which is the interning this function exists to avoid. It is refused by
    /// name rather than laid out wrong. The plain nesting, `Inner[A]` inside
    /// `Outer[A]`, is bound and works, because the argument *is* a parameter
    /// and resolving one is a lookup.
    fn aggregate_env(
        &self,
        def: DefId,
        generics: &[science_resolve::hir::GenericParam],
        args: &[GenericArg],
        outer: &TyEnv,
    ) -> Result<(String, TyEnv), Unlowered> {
        let name = self.defs.get(def).name.clone();
        if args.is_empty() {
            return Ok((name, TyEnv::new()));
        }
        let mut inner = TyEnv::new();
        let mut rendered = Vec::with_capacity(args.len());
        for (param, arg) in generics.iter().zip(args) {
            let GenericArg::Type(ty) = arg else { continue };
            // An argument written as one of the *enclosing* aggregate's
            // parameters is resolved through the environment that brought us
            // here; anything else is already concrete and stands.
            let ty = match self.types.kind(*ty) {
                TyKind::Param { def } => *outer.get(def).ok_or_else(|| {
                    Unlowered::new(format!(
                        "a value of the generic type `{name}`, whose argument `{}` is a type \
                         parameter nothing bound: a generic aggregate's layout is computed by \
                         binding its parameters at each use, and this use is inside a body \
                         nothing monomorphised",
                        self.defs.get(*def).name
                    ))
                })?,
                _ if self.mentions_a_parameter(*ty, 0) => {
                    return Err(Unlowered::new(format!(
                        "a value of the generic type `{name}`, whose argument `{}` is a compound \
                         type mentioning a parameter: binding it would mean interning a new \
                         `Ty`, and this crate is handed a `&Types` so that it cannot. The \
                         substitution belongs above Decision 42's line, in \
                         `science_mir::instantiate`, which does not reach a *declaration*'s field \
                         list",
                        self.types.render(self.defs, *ty)
                    )));
                }
                _ => *ty,
            };
            inner.insert(param.def, ty);
            rendered.push(self.types.render(self.defs, ty));
        }
        if rendered.is_empty() {
            return Ok((name, inner));
        }
        Ok((format!("{name}[{}]", rendered.join(", ")), inner))
    }

    /// Whether `ty` mentions a [`TyKind::Param`] anywhere inside it.
    ///
    /// The depth bound is [`Lowerer::cg_ty_at`]'s and for the same reason: a
    /// self-containing type would otherwise recurse until the stack ran out.
    /// Answering `false` at the bound is safe here, because the caller's only
    /// use of a `true` is to produce a refusal, and a type this deep is
    /// refused by `cg_ty_in` a moment later anyway.
    fn mentions_a_parameter(&self, ty: Ty, depth: u32) -> bool {
        if depth > MAX_TYPE_DEPTH {
            return false;
        }
        match self.types.kind(ty) {
            TyKind::Param { .. } | TyKind::SelfType { .. } | TyKind::SelfAssoc { .. } => true,
            TyKind::Nullable(inner) => self.mentions_a_parameter(*inner, depth + 1),
            TyKind::Borrowed { inner, .. } => self.mentions_a_parameter(*inner, depth + 1),
            TyKind::Named { args, .. } => args.iter().any(|arg| match arg {
                GenericArg::Type(ty) => self.mentions_a_parameter(*ty, depth + 1),
                _ => false,
            }),
            TyKind::Tuple(elements) => {
                elements.iter().any(|ty| self.mentions_a_parameter(*ty, depth + 1))
            }
            _ => false,
        }
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
    fn choice_ty(
        &self,
        def: DefId,
        args: &[GenericArg],
        depth: u32,
        env: &TyEnv,
    ) -> Result<CgTy, Unlowered> {
        let decls = self.declarations()?;
        // A `choice`'s parameters are carried on each of its *variants* —
        // `items::Variant`'s own comment says why: a use writes `Left(1)` and
        // never names the type. The list is the same on every variant, so the
        // first one that has an entry answers for the type, and a `choice`
        // with no variants is refused just below.
        let generics: Vec<science_resolve::hir::GenericParam> = self
            .choice_variants(def)
            .iter()
            .find_map(|variant| decls.variant(*variant).map(|v| v.generics.clone()))
            .unwrap_or_default();
        let (name, env) = self.aggregate_env(def, &generics, args, env)?;
        let env = &env;
        let order = self.choice_variants(def);
        if order.is_empty() {
            return Err(Unlowered::new(format!(
                "a value of type `{name}`, a `choice` with no variants: it has no value and no \
                 discriminant to hold one"
            )));
        }
        // `science_codegen::mono`'s substituted payload list, one per variant
        // in [`Lowerer::choice_variants`]'s own order — the same order
        // `Mono::intern_aggregate_fields` walked the `choice`'s variants in,
        // since both read `DefTable::children` filtered to `DefKind::Variant`.
        // `record_ty`'s own doc comment says why a hit is walked with a fresh
        // `TyEnv` and a miss falls back to `declared` bound by `env`.
        let mono_payloads =
            self.mono_aggregate_fields(def, args).and_then(|layout| match layout {
                science_codegen::mono::AggregateLayout::Choice(payloads) => Some(payloads),
                science_codegen::mono::AggregateLayout::Record(_) => None,
            });
        let no_binding = TyEnv::new();
        let mut variants = Vec::with_capacity(order.len());
        for (index, variant) in order.iter().enumerate() {
            let variant_name = self.defs.get(*variant).name.clone();
            // Absent is **not** the same as empty: a variant the declaration
            // table has no entry for is a variant whose payload nobody lowered,
            // and treating it as payload-free would put the wrong number of
            // bytes in the union.
            let declared = decls.variant(*variant).ok_or_else(|| {
                Unlowered::new(format!(
                    "the variant `{name}.{variant_name}`, which the declaration table has no \
                     lowered payload for"
                ))
            })?;
            let (payload, use_env): (&[Ty], &TyEnv) = match mono_payloads.and_then(|p| p.get(index))
            {
                Some(substituted) => (substituted.as_slice(), &no_binding),
                None => (declared.payload.as_slice(), env),
            };
            variants.push(match payload {
                [] => CgVariant::unit(variant_name),
                [only] => CgVariant::with(variant_name, self.cg_ty_in(*only, depth + 1, use_env)?),
                many => {
                    let mut fields = Vec::with_capacity(many.len());
                    for (index, ty) in many.iter().enumerate() {
                        fields.push(CgField::new(
                            index.to_string(),
                            self.cg_ty_in(*ty, depth + 1, use_env)?,
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
                let args = args.clone();
                match self.defs.get(def).kind {
                    // **A generic record or `choice` is asked the same
                    // question as a plain one, with its parameters bound.**
                    //
                    // This arm used to answer `true` for any aggregate with
                    // arguments, which was the conservative reading of *"this
                    // crate lays out no generic type"*: a `Pair[Int, F64]`
                    // owns nothing, but nothing could prove it, and claiming
                    // a value needs no drop when it does is a leak. So it
                    // claimed the opposite and paid a refusal one step later,
                    // at glue it could not emit.
                    //
                    // Now the parameters can be bound —
                    // [`Lowerer::aggregate_members`] does it — so the honest
                    // answer is available: `Pair[Int, F64]` owns nothing and
                    // `Pair[Int, String]` owns a `String`. The conservative
                    // answer survives only where a member genuinely cannot be
                    // bound, where `aggregate_members` refuses rather than
                    // guessing.
                    DefKind::Record | DefKind::Choice => {
                        for member in self.aggregate_members(def, &args)?.1 {
                            if self.drop_runs_something(member, depth + 1)? {
                                return Ok(true);
                            }
                        }
                        Ok(false)
                    }
                    _ if !args.is_empty() => Ok(true),
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
                            | "F16"
                            | "BF16"
                            | "IoError"
                            // **`Chars` owns nothing, which is the whole
                            // reason it can be an iterator at all.** It is
                            // `{ ptr, len, offset }` *into a string somebody
                            // else owns*, so there is no buffer to free and no
                            // `drop_fn` to name — unlike `String`, `Array` and
                            // `Map` three lines up, which are on the other side
                            // of this list for exactly that difference.
                            | "Chars"
                    )),
                }
            }
            // A closure never owns anything under this backend, and this is
            // the one arm of this match that is not conservative about a type
            // it cannot see behind — because it does not have to. `cg_ty_in`'s
            // `TyKind::Closure` arm gives every value of this type the same
            // layout, a bare function pointer: a closure that captures
            // something is refused where it would be *built*
            // (`Lowerer::lower_rvalue`'s `Rvalue::Closure` arm), so nothing of
            // this type in a place this backend lowers is ever anything else.
            // A function pointer releases nothing on its own —
            // `science_codegen::descriptor::needs_drop` already says so of
            // `CgTy::Ptr(PtrKind::Fn)` — and this is that fact, read off the
            // Science type before it reaches a layout.
            TyKind::Closure { .. } => Ok(false),
            // A type parameter, `Self`, an associated type: nothing here can
            // see what is behind either.
            _ => Ok(true),
        }
    }

    /// The owning types this backend releases with a direct runtime call,
    /// and the descriptor that call needs.
    ///
    /// **Decision 12's `DropGlue::RuntimeCall`, as one predicate instead of
    /// two call sites' worth of special cases.** A `String` is released by
    /// `science_string_free(address)` and an `Array of T` by
    /// `science_array_free(address, descriptor)`; neither has — or wants — a
    /// glue function, because the runtime already owns the knowledge of how
    /// to free the buffer. Both the drop terminator and the field loop inside
    /// [`Lowerer::intern_drop_glue`] ask here, so the two never disagree about
    /// which release a type gets.
    ///
    /// **The second element is the descriptor's symbol and not the descriptor**,
    /// for [`Lowerer::lower_runtime_call`]'s reason: only this crate interns
    /// globals, so MIR passes the array and codegen supplies the `ScienceTypeInfo`
    /// beside it. `science_string_free` takes none, which is exactly what
    /// separates the two signatures.
    ///
    /// **`science_box_free` is in this table too, and its call is built
    /// differently from the other three's.** [`Lowerer::release_args`] is
    /// where that shows up: every caller of this function builds its call
    /// through that one function rather than reading the tuple apart itself,
    /// which is what keeps the four call sites from having to know
    /// `science_box_free` takes its descriptor first and its pointer by value
    /// and not by address.
    ///
    /// `None` means the type is not one of these — either it owns nothing, or
    /// it owns something through fields and wants glue.
    fn direct_release(
        &mut self,
        ty: Ty,
    ) -> Result<Option<(&'static str, Option<String>)>, Unlowered> {
        if self.is_string(ty) {
            return Ok(Some(("science_string_free", None)));
        }
        if let Some(element) = self.array_element(ty) {
            let descriptor = self.intern_element_descriptor(element)?;
            return Ok(Some(("science_array_free", Some(descriptor))));
        }
        // A `Map` releases through `science_map_free(P, D)`, which runs the
        // key's and the value's `drop_fn` over the live slots. Its descriptor
        // takes two arguments like an array's, so a `Map` nested inside either
        // container meets the same refusal `Array of (Array of T)` does, for
        // the same reason: Decision 20's `drop_fn` is called with one address.
        if let Some((key, value)) = self.map_key_value(ty) {
            let descriptor = self.intern_map_descriptor(key, value)?;
            return Ok(Some(("science_map_free", Some(descriptor))));
        }
        // A `Box` releases through `science_box_free(D, P)`, whose element is
        // `Lowerer::intern_element_descriptor`'s general descriptor and not
        // [`Lowerer::intern_descriptor`]'s narrower one: a boxed value can own
        // something — Decision 14's boxing into an interface object cannot say
        // that yet, and §2.6's plain `Box of T` is exactly where it can, since
        // its `drop_fn` is filled in by [`Lowerer::intern_drop_glue`] the same
        // way an array's element is.
        //
        // **Not `Box of any I`, and the reason is the descriptor this branch
        // would have to name.** `assign.rs`'s §4a: releasing `Box of any
        // Summarize` reads the descriptor out of the vtable at run time — the
        // concrete type behind it is exactly what this backend cannot see —
        // so there is no *static* `ScienceTypeInfo` for this branch to intern
        // and pass as `D`. `None` here sends `ty` on to
        // [`Lowerer::intern_drop_glue`], whose own `Box`-of-object arm reaches
        // the vtable the same four loads [`Lowerer::emit_object_glue`] already
        // makes for a bare owned `any I` — `Box of any I` is that value's
        // layout exactly, so it is that value's release too.
        if let Some(element) = self.box_element(ty) {
            if self.object_interface(element).is_none() {
                let descriptor = self.intern_element_descriptor(element)?;
                return Ok(Some(("science_box_free", Some(descriptor))));
            }
        }
        Ok(None)
    }

    /// The call [`Lowerer::direct_release`]'s answer becomes, given the
    /// field's or local's own address.
    ///
    /// **Three of the four take `address` itself and `science_box_free` does
    /// not, and the difference is what `CgTy` each one is.** `String`,
    /// `Array` and `Map` are aggregates — `{ ptr, len, cap }` and wider — so
    /// the field's own address is already the pointer a C signature expects.
    /// `Box of T` lowers to `CgTy::Ptr(PtrKind::Box)`: **the field itself
    /// holds the heap pointer**, not a struct to point at, so `address` names
    /// the *slot* and the runtime call wants what is stored in it — one
    /// `LoadAt` further than the other three need. `science_box_free`'s
    /// descriptor is the *first* argument and not the second for the same
    /// reason `science_codegen::descriptor::DropGlue`'s own comment gives:
    /// `science_box_free(@typeinfo.T, %p)` reads left to right where the
    /// other three read `(%p, @typeinfo.T)`.
    ///
    /// **`fresh` and not a `ValueId` this function invents itself**, because
    /// every caller already has its own numbering — `BodyCtx::value` in
    /// [`Lowerer::lower_terminator`], a `u32` counter in
    /// [`Lowerer::emit_field_glue`] and [`Lowerer::emit_variant_release`], two
    /// fixed constants in [`Lowerer::emit_nullable_glue`] — and a fifth
    /// scheme here would be a second counter for exactly one of the values
    /// those already hand out.
    fn release_args(
        &self,
        direct: &(&'static str, Option<String>),
        address: ValueId,
        fresh: &mut dyn FnMut() -> ValueId,
        insts: &mut Vec<ExtInst>,
    ) -> Vec<Operand> {
        let (symbol, descriptor) = direct;
        if *symbol == "science_box_free" {
            let descriptor = descriptor
                .clone()
                .expect("direct_release always pairs science_box_free with a descriptor");
            let loaded = fresh();
            insts.push(ExtInst::LoadAt {
                dest: loaded,
                address: Operand::Value(address),
                layout: layout_of(self.target, &CgTy::Ptr(PtrKind::Box)),
            });
            return vec![Operand::GlobalAddr(descriptor), Operand::Value(loaded)];
        }
        let mut args = vec![Operand::Value(address)];
        if let Some(descriptor) = descriptor {
            args.push(Operand::GlobalAddr(descriptor.clone()));
        }
        args
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
    pub fn lower_crate(
        &mut self,
        bodies: &[MirBody],
        instances: &'a [science_codegen::mono::MonoBody],
        mono: &'a science_codegen::mono::MonoSet,
    ) -> Result<Lowered, Unlowered> {
        self.calls = Some(mono);
        let entry = bodies
            .iter()
            .find(|body| self.defs.get(body.def()).name == "main")
            .ok_or_else(|| Unlowered::new("a program with no `main`"))?;
        self.entry = Some(entry.def());
        // **The emission set is the monomorphisation walk's, not this crate's
        // own reachability.**
        //
        // The decision. Which bodies get emitted comes from
        // [`science_codegen::mono::MonoSet`], computed by `sciencec`'s driver
        // and handed in. The `reachable_from` walk this replaced is gone from
        // the decision and kept only as the fallback for a caller that has no
        // set — see below.
        //
        // The reason. `reachable_from` answered *"which definitions can `main`
        // reach"*, and the question a backend actually has to answer is *"which
        // **instances** does this program need"*. For a program with no
        // generics the two sets coincide, which is why this compiler got as far
        // as it did on the weaker one; they stop coinciding at the first
        // `identity of T` called at two types, where the right answer is two
        // functions and reachability can only ever say *one definition*.
        // `science_codegen::mono` has answered the stronger question the whole
        // time — 1753 lines and forty tests — and **nothing in the compiler
        // called it**. Decision 42 already put the walk above this crate; this
        // makes that placement load-bearing instead of aspirational.
        //
        // The cost, and it is the honest limit of this change. Emission is
        // driven by the set, but a *generic* instance is still refused one
        // layer down in [`Lowerer::science_signature`], because lowering one
        // body twice at two argument types needs the substitution applied to
        // every type the body mentions and this crate does not apply it yet.
        // So today the set and the old walk produce the same answer on every
        // program that compiles, which is exactly what makes the swap
        // verifiable: the whole test suite is the assertion that nothing moved.
        // **The union, and the second half is a gap in the walk that connecting
        // it is what found.**
        //
        // `science_codegen::mono` does not model Decision 13's vtables. Its
        // `walk_rvalue` meets an `Rvalue::Coerce` and asks only
        // `note_address_taken`, which looks for a function named as a *value*;
        // an unsize coercion to `any I` names no function anywhere in the MIR,
        // and yet it puts the address of every one of that interface's methods
        // into a table. So a method reached **only** through a vtable is absent
        // from the set, and the symptom is not a refusal — it is `define_vtable`
        // naming a symbol nothing defined, and the linker saying so. Five
        // execution tests in this crate failed exactly that way the moment the
        // set became the emission set, which is five more than had ever
        // exercised the walk before.
        //
        // **Why it is a union here rather than a fix there.** The rule that
        // decides a vtable's slots is [`Lowerer::vtable_pair`] and
        // [`Lowerer::vtable_slots`], and it lives in *this* crate;
        // `science-codegen` cannot call it, because the dependency runs the
        // other way. Writing a second copy of it inside `mono` is the hazard
        // this codebase warns about everywhere else — two spellings of one rule,
        // discovered by a linker a stage too late. The real repair is to move
        // that rule down into `science-codegen`, where `descriptor::Vtable`
        // already lives, so one rule serves the walk and the emitter; until
        // then the walk is an honest **lower bound** and this says so in the
        // only way that cannot rot, by taking the union and naming what the
        // second half is for.
        //
        // The cost: a definition reachable from `main` but genuinely not
        // needed by any instance is still emitted, which is what was happening
        // before this change for every definition. Nothing regresses; what the
        // set adds is the ability to name instances, which is what generics
        // will need.
        // **What is emitted is one function per *instance*, and the list of
        // instances is `science_codegen::mono`'s own.**
        //
        // The decision. `instances` — a `MonoBody` each, carrying the mangled
        // symbol and a body with every type already substituted — replaces the
        // `&[MirBody]` this loop used to walk. The old reachability walk
        // survives only for the union below.
        //
        // The reason. The comment this replaces said the honest thing: *"a
        // generic instance is still refused one layer down in
        // `science_signature`, because lowering one body twice at two argument
        // types needs the substitution applied to every type the body mentions
        // and this crate does not apply it yet"*. It still does not apply it —
        // `science_mir::instantiate` does, above the line Decision 42 draws —
        // and what arrives here is concrete MIR. So the refusal has nothing
        // left to refuse, and this crate did not learn about generics; it
        // stopped being handed any.
        //
        // The cost: a body per instance rather than per definition, which is
        // what monomorphisation costs by definition and is paid in compile
        // time and image size. `identity` called at three types is three
        // functions, and the alternative — one function and a dictionary — is
        // a different language with a different performance story, which
        // Decision 42 already settled.
        //
        // **The union with the reachability walk stays, and for the unchanged
        // reason**: `mono` does not model Decision 13's vtables, so a method
        // reached only through one is absent from its set and the symptom is a
        // linker error rather than a refusal. That gap is a rule living in
        // this crate that `science-codegen` cannot call; the union is the
        // honest lower bound until the rule moves down. A definition reached
        // that way is emitted at its plain instance, which is correct for
        // every method a vtable can hold — an interface method is not generic
        // in the receiver, it is one entry per concrete implementor.
        //
        // **The collision check survives the re-keying, and it had to be
        // rewritten rather than kept.** It used to read the `science` map and
        // ask whether a *different definition* had already claimed this
        // symbol; now the map is keyed by symbol, so a second claim would
        // simply overwrite the first and the `define` would be emitted twice
        // under one name, which is finding 25 exactly. So the owner of each
        // symbol is tracked here, in `claimed`, and the two cases are
        // separated by what they mean: the same definition reaching one symbol
        // twice is monomorphisation meeting the same instance by two paths and
        // is a *deduplication*; two definitions reaching one symbol is the
        // mangling failing to separate them and is a refusal.
        let mut emitted: Vec<(String, &MirBody)> = Vec::new();
        let mut claimed: std::collections::BTreeMap<String, DefId> =
            std::collections::BTreeMap::new();
        let mut claim = |symbol: &str, def: DefId, defs: &DefTable| -> Result<bool, Unlowered> {
            match claimed.get(symbol) {
                Some(other) if *other == def => Ok(false),
                Some(other) => Err(Unlowered::new(format!(
                    "two definitions that mangle to one symbol, `{symbol}`: `{}` and `{}`. \
                     Decision 16 builds a symbol out of the definition's path, and the path of a \
                     method runs through a block `science-resolve` leaves unnamed",
                    defs.path_of(*other),
                    defs.path_of(def),
                ))),
                None => {
                    claimed.insert(symbol.to_string(), def);
                    Ok(true)
                }
            }
        };
        for instance in instances {
            if claim(&instance.symbol, instance.body.def(), self.defs)? {
                emitted.push((instance.symbol.clone(), &instance.body));
            }
        }
        let reachable = reachable_from(self, bodies, entry.def());
        let instantiated: std::collections::BTreeSet<DefId> =
            instances.iter().map(|instance| instance.body.def()).collect();
        for body in bodies {
            if !reachable.contains(&body.def()) {
                continue;
            }
            // **A definition the walk already reached is not re-emitted under
            // this crate's own mangling.** The two manglings agree today —
            // both are `science_codegen::mangle` over the definition's path —
            // but they are two pieces of code and nothing forces them to, and
            // a disagreement would put the same function in the module twice
            // under two names, the second silently dead. The fallback's job is
            // the definitions `mono` *missed*, which is the vtable-only
            // methods the comment above names, so it asks exactly that.
            if instantiated.contains(&body.def()) {
                continue;
            }
            let symbol = self.symbol_of(body.def());
            if claim(&symbol, body.def(), self.defs)? {
                emitted.push((symbol, body));
            }
        }

        for (symbol, body) in &emitted {
            let signature = self.science_signature(body, symbol)?;
            self.by_def.insert(body.def(), symbol.clone());
            self.science.insert(symbol.clone(), signature);
        }

        let mut definitions = Vec::new();
        for (symbol, body) in &emitted {
            definitions.push(self.lower_body(body, symbol)?);
        }
        // `main` is never generic — it takes no parameters — so it has exactly
        // one instance and `by_def` is enough to name it.
        let main_signature = self
            .by_def
            .get(&entry.def())
            .and_then(|symbol| self.science.get(symbol))
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
    /// **`Declarations` is not consulted at all, and it was until this
    /// function's one refusal closed.** It read *"a call to `{name}`, the
    /// default body a method declared on `interface {interface}` carries: its
    /// `self` is `Self`, which is a different concrete type in every
    /// implementation …"* — true the day it was written, because nothing gave
    /// that body a second copy per implementor. `science_codegen::mono` does
    /// now: [`science_codegen::mono::Instance::self_ty`] is the axis, its own
    /// module documentation is where the decision and its cost are written
    /// down, and `science_mir::instantiate` substitutes `Self` in each copy
    /// exactly as it substitutes any other type parameter. What arrives here
    /// is a [`MirBody`] whose `self` already reads `Doc` or `Row`, never
    /// `Self` — asking `Declarations` whether the *definition* is a default
    /// body would be asking the wrong table, for the same reason asking it
    /// whether the definition is generic would be: both questions are the
    /// declaration's and this is one of its instances.
    ///
    /// **What still catches a body that slipped through unsubstituted** is
    /// `layout_of_ty`, which refuses a `TyKind::Param` or a `TyKind::SelfType`
    /// by name. That is the property `science_mir::instantiate`'s header
    /// relies on: a missed substitution site is a refusal at a boundary
    /// already built to report one, not a wrong layout that runs. **This is
    /// also where Decision 13's vtable gap surfaces for a default body reached
    /// only through one**, rather than through a concrete receiver:
    /// `science_codegen::mono`'s own documentation says its walk does not
    /// model a vtable's slots, so a call through `&any Summarize` never learns
    /// a concrete `Self` to bind, `Instance::self_ty` stays `None`, and the
    /// body `mono` hands back still has `TyKind::SelfType` where `self` was
    /// written — caught by the same fallback, with a plainer message than the
    /// one this replaced, because this boundary no longer knows *why* the
    /// type survived unsubstituted, only that it did.
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
    fn science_signature(
        &mut self,
        body: &MirBody,
        symbol: &str,
    ) -> Result<AbiSignature, Unlowered> {
        let def = body.def();
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
        // **The symbol is the instance's, except for the entry point.**
        // `science_codegen::mono` mangles `main` from its path like any other
        // definition, and [`Lowerer::symbol_of`]'s own documentation is why
        // that is the one name this crate does not take: `script-mode.md`
        // §2.3, `lower_c_main` and `science-codegen`'s `tests/stage_one.rs`
        // all say the entry point is `_S4main` whatever module it is in. The
        // map key stays the instance's symbol — it is an identity, not a name
        // — and only the emitted name is overridden.
        let symbol = match self.entry == Some(def) {
            true => self.symbol_of(def),
            false => symbol.to_string(),
        };
        Ok(AbiSignature::science(self.target, symbol, ret_layout, params))
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
        let parts = self.path_components(def);
        if parts.is_empty() {
            return mangle(&MonoKey::plain(&[self.defs.get(def).name.as_str()]));
        }
        let components: Vec<&str> = parts.iter().map(|part| part.as_str()).collect();
        mangle(&MonoKey::plain(&components))
    }

    /// `DefTable::path_of`'s walk.
    ///
    /// **This used to be finding 25 and a symbol collision that ran**, and this
    /// function used to carry the fix as a special case: `path_of` skips a
    /// definition whose name is empty, `science-resolve` gave an implementation
    /// block exactly that, so `Left has: def value` and `Right has: def value`
    /// both had the path `fixture.value`, both mangled to `_S7fixture5value`,
    /// and the module gained two definitions of one symbol — which links, runs,
    /// and calls the first of them for both.
    ///
    /// **The special case is gone because the hole it patched is gone.**
    /// `DefTable::alloc_impl` now writes a name onto the block's own
    /// `hir::Def` — the self type's, with a `#n` for the second and later block
    /// on that type in that module — so `entry.name` is never empty for an
    /// `impl` and this function is `path_of`'s walk with nothing special about
    /// any one `DefKind`. `block_type_name`, which read
    /// `Declarations::self_ty` to reconstruct the same name this crate cannot
    /// see the resolver already wrote, is deleted rather than kept as a second
    /// way to ask; two spellings of one rule is the hazard this codebase warns
    /// about everywhere else, and `science-codegen`'s `Mono::path_of` took the
    /// same fix for the same reason.
    ///
    /// **What separates two blocks on one type now, and what still does not.**
    /// `Doc has:` beside `Doc implements Sized:` are `Doc` and `Doc#1`, so the
    /// bug this finding names cannot happen again. Two blocks that are
    /// ambiguous on their own terms — two `implements` blocks for the same
    /// interface — still contribute the same component, but that program is an
    /// ambiguity `science-types`' `methods`' §6 already reports at the call
    /// site, so nothing that checks clean can reach it; [`Lowerer::lower_crate`]'s
    /// duplicate check stays as the guard for whatever this walk gets wrong
    /// rather than for this finding specifically.
    fn path_components(&self, def: DefId) -> Vec<String> {
        let entry_module = self.entry_module();
        let mut parts: Vec<String> = Vec::new();
        let mut cursor = Some(def);
        while let Some(current) = cursor {
            let entry = self.defs.get(current);
            // **The entry module's name is left out, and every other
            // module's is kept.** `science_codegen::mono`'s `path_of` states
            // the whole argument; the short version is that the entry file's
            // name is not source — nobody wrote it and renaming it changes
            // nothing else, which is Decision 16's *"deterministic from the
            // source alone"* — while a module reached by `use deep.inner` is
            // named by source that would have to change with it.
            //
            // The rule is duplicated here rather than shared because the
            // dependency runs the wrong way: `science-codegen` cannot call
            // into this crate. What keeps the two from drifting is
            // `tests/mono.rs`'s `the_walk_and_the_backend_agree_on_a_plain_
            // symbol`, which compares them directly and fails the moment
            // either side moves.
            // The prelude's module is skipped for the same reason the
            // entry's is: nobody writes `use core`, so its name is not
            // source. `mono`'s `path_of` carries the argument and the cost
            // of having got this wrong once.
            let skip = entry.name.is_empty()
                || Some(current) == entry_module
                || (entry.kind == DefKind::Module && entry.is_builtin());
            if !skip {
                parts.push(entry.name.clone());
            }
            cursor = entry.parent;
        }
        parts.reverse();
        parts
    }

    /// The module [`Lowerer::entry`] is in, whose name no symbol carries.
    fn entry_module(&self) -> Option<DefId> {
        let mut cursor = self.defs.get(self.entry?).parent;
        while let Some(id) = cursor {
            if self.defs.get(id).kind == DefKind::Module {
                return Some(id);
            }
            cursor = self.defs.get(id).parent;
        }
        None
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
        // Decision 12's glue, which is emitted while bodies are lowered and
        // so is not in the list the caller handed over. Appended before the
        // sort, so Decision 4's order covers it too.
        definitions.extend(std::mem::take(&mut self.glue_definitions));
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
        // Taken once: the table owns both lists and reading the second after
        // moving it out is the mistake this shape exists to make impossible.
        let descriptors = std::mem::take(&mut self.descriptors);
        Lowered {
            literals: std::mem::take(&mut self.literals),
            vtables: std::mem::take(&mut self.vtables),
            descriptors: descriptors
                .type_infos()
                .map(|(symbol, info)| (symbol.clone(), info.clone()))
                .collect(),
            map_descriptors: descriptors
                .map_infos()
                .map(|(symbol, info)| (symbol.clone(), info.clone()))
                .collect(),
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

    fn lower_body(
        &mut self,
        body: &MirBody,
        symbol: &str,
    ) -> Result<(AbiSignature, ExtBody), Unlowered> {
        let return_local = mir::Local::from_index(0);
        let return_id = LocalId(return_local.index() as u32);
        let cached = self.science.get(symbol).cloned();
        let sig = match cached {
            Some(existing) => existing,
            None => self.science_signature(body, symbol)?,
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
            symbol: symbol.to_string(),
            block: mir::BlockId::from_index(0),
            next_value: 0,
            next_temp: body.local_count() as u32,
            next_block: body.block_count() as u32,
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
            ctx.block = id;
            let mut insts: Vec<ExtInst> = Vec::new();
            for statement in &block.statements {
                self.lower_statement(body, &mut ctx, &statement.kind, &mut insts)?;
            }
            // `extra` is Decision 26's flagged drop and nothing else: every
            // other terminator lowers to one `Terminator` and leaves it
            // empty, which is `lower_terminator`'s own §Decision-5 note.
            let mut extra: Vec<ExtBlock> = Vec::new();
            let terminator = self.lower_terminator(
                body,
                &mut ctx,
                &block.terminator.kind,
                &mut insts,
                &mut extra,
            )?;
            blocks.push(ExtBlock {
                id: BlockId(id.index() as u32),
                label: format!("bb{}", id.index()),
                insts,
                terminator,
            });
            blocks.extend(extra);
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
            // Decision 26's drop flag, written: a store of a constant `Bool`
            // into the flag's own slot, the same `Inst::Store` an ordinary
            // whole-local `Bool` assignment ends in
            // ([`Lowerer::lower_rvalue`]'s `Rvalue::Use` arm) — because a
            // flag *is* an ordinary local, laid out and given an `alloca` by
            // `lower_body`'s per-local loop like any other
            // [`science_mir::mir::LocalKind::DropFlag`]. `flag: Option<Local>`
            // is read back at [`Lowerer::emit_flagged_drop`], the only reader.
            StatementKind::SetDropFlag { flag, value } => {
                let local = LocalId(flag.index() as u32);
                if ctx.untyped.contains(&local) {
                    return Err(Unlowered::new(format!("{UNTYPED} (local _{})", local.0)));
                }
                ctx.layout(local)?;
                insts.push(ExtInst::Above(Inst::Store {
                    local,
                    value: Operand::ConstInt(i128::from(*value)),
                }));
                Ok(())
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
                    return Err(Unlowered::new(format!("{UNTYPED} (local _{})", local.0)));
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
            return Err(Unlowered::new(format!("{UNTYPED} (local _{})", local.0)));
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
                // `Projection::Payload`: a `T?`'s payload, at whatever offset
                // *this* option's layout puts it — the fact `science-mir` was
                // written not to know. Decision 19's niche has no discriminant
                // and no separate storage for its payload: the payload *is*
                // the whole value, so the offset is zero and its layout is the
                // whole layout. Decision 18's tagged pair keeps the payload at
                // `payload_offset`, behind the discriminant, exactly where
                // `Projection::Downcast` above already reads a `choice`'s.
                mir::Projection::Payload { ty } => match &layout.repr {
                    Repr::Niched { payload, .. } => (0, (**payload).clone()),
                    Repr::Tagged { payload_offset, .. } => {
                        (*payload_offset, self.layout_of_ty(*ty)?)
                    }
                    _ => {
                        return Err(Unlowered::new(
                            "a payload projection of a value whose layout is neither Decision \
                             18's tagged pair nor Decision 19's niche",
                        ));
                    }
                },
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
                    // **A niched layout whose payload is a pointer dereferences
                    // like the pointer it is, and that is Decision 19 rather
                    // than a relaxation of this check.** `(borrowed T)?` is
                    // `Repr::Niched` over a `Scalar(Pointer)` with null as the
                    // niche value — the *whole* layout is the payload's, which
                    // is what `Repr::Niched`'s own doc says — so the slot holds
                    // a pointer and nothing else. MIR reaches here only where a
                    // narrowing already established the value is not null:
                    // `f"{found}"` inside `if found?:`. The guard still refuses
                    // everything that is not pointer-shaped, which is what it
                    // was written for.
                    let pointee_repr = match &layout.repr {
                        Repr::Niched { payload, .. } => &payload.repr,
                        other => other,
                    };
                    if !matches!(pointee_repr, Repr::Scalar(Scalar::Pointer(_))) {
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
                // **One call, no branch, and the branch is not missing.** This
                // used to be the refusal *"an index, which §2.4 makes a bounds
                // check and then a `getelementptr`, and neither the check nor
                // an `Array` value reaches this backend"*, which was a true
                // description of the problem and the wrong crate to solve it
                // in: a place projection is not a statement, so there is
                // nowhere here to put three basic blocks. `science-mir`'s
                // `bounds_check` emits them in front of the place, so what is
                // left is the address, and `science_array_get` is the entry
                // point that computes one.
                //
                // **Decision 5, again**: the element address is a runtime call
                // and not a `getelementptr` this crate opens `ScienceArray` to
                // build. The layout of the buffer is the runtime's, the
                // descriptor is what says how wide an element is, and a
                // `getelementptr` here would be this crate asserting a stride
                // it does not own.
                //
                // The returned pointer **is** the next base, exactly as
                // `Projection::Deref`'s loaded pointer is — the difference is
                // that this one is computed rather than loaded, so there is no
                // `LoadAt` and the pointee's layout comes from the
                // projection's own `ty`.
                //
                // **`get_mut` for a read as well as for a write, and that is
                // the sound direction rather than the lazy one.** A place
                // projection does not know which it is in — `place_address` has
                // twelve call sites and no mutability parameter — so one of the
                // two entry points has to serve both. `science_array_get`
                // returns a `*const u8` that the runtime derived from a shared
                // `&ScienceArray`; `xs[1] be 99` would then write through a
                // pointer derived from a shared reference, which is undefined
                // under Rust's aliasing model on the `science-rt` side of the
                // boundary even though every compiler today emits the store.
                // Reading through the `*mut` that `science_array_get_mut`
                // returns has no such problem. The cost is that the read path
                // asks the runtime for an exclusive pointer it does not need;
                // the borrow checker's view of `xs[i]` is `Projection::Index`
                // and `science-regions`, not which symbol this picks, so
                // nothing above is loosened by it.
                mir::Projection::Index { index, ty } => {
                    let element = self.layout_of_ty(*ty)?;
                    let descriptor = self.intern_element_descriptor(*ty)?;
                    let index_local = LocalId(index.index() as u32);
                    let index_value = ctx.value();
                    ctx.layout(index_local)?;
                    insts.push(ExtInst::Above(Inst::Load {
                        dest: index_value,
                        local: index_local,
                    }));
                    let sig = self.declare("science_array_get_mut")?;
                    let result = ctx.value();
                    insts.push(ExtInst::Above(Inst::Call {
                        dest: Some(result),
                        callee: Callee::Runtime("science_array_get_mut"),
                        args: vec![
                            Operand::Value(address),
                            Operand::GlobalAddr(descriptor),
                            Operand::Value(index_value),
                        ],
                        ret: sig.ret.clone(),
                        sret_slot: None,
                    }));
                    address = result;
                    layout = element;
                    continue;
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
        // **The tag is at offset 0, always** — Decision 18 — so a projected
        // source needs the address of the `choice` and nothing more. This used
        // to refuse every one of them, saying *"this instruction set has no
        // typed load from a computed one"*; [`ExtInst::LoadAt`] is exactly
        // that and has been since records landed. What made the refusal
        // reachable is `science-mir`'s §4 dereference on a `match` scrutinee:
        // `match format:` where `format` is a `borrowed Format` is
        // `discriminant((*_1))`, which is the spelling every `def
        // name_of(format: borrowed Format)` in the corpus produces.
        let source_ty = match source.projection.last() {
            None => body.local_decl(source.local).ty,
            Some(mir::Projection::Downcast { .. }) => {
                return Err(Unlowered::new(
                    "a discriminant read of a value that has already been downcast to one \
                     variant: the tag it would load is the one the downcast assumed",
                ));
            }
            Some(projection) => projection.ty(),
        };
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
                ctx.layouts.insert(dest.0, tag_layout.clone());
            }
        }
        ctx.discriminants.insert(dest.0, choice);

        let value = ctx.value();
        if source.projection.is_empty() {
            let source_id = LocalId(source.local.index() as u32);
            ctx.layout(source_id)?;
            insts.push(ExtInst::LoadTag { dest: value, local: source_id });
        } else {
            let (address, at) = self.place_address(ctx, source, insts)?;
            // The same question [`ExtInst::LoadTag`] asks of a local, asked of
            // the projected place instead: a niched layout has no tag to load
            // and reading one would branch a `switch` on an address.
            if !matches!(at.repr, Repr::Tagged { .. }) {
                return Err(Unlowered::new(
                    "a discriminant read through a projection whose layout is not Decision 18's \
                     tagged one",
                ));
            }
            insts.push(ExtInst::LoadAt {
                dest: value,
                address: Operand::Value(address),
                layout: tag_layout.clone(),
            });
        }
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
            // Decision 7's narrowing: the payload of a `T?` the program has
            // already tested.
            //
            // **The decision.** A niched option is the value already and is
            // moved; a tagged one is a load at the payload's offset.
            //
            // **The reason both shapes are here and neither is a branch.** The
            // `If` that established presence has already run — that is what
            // `Rvalue::Narrow` *means*, and `science-types` emits it only where
            // §7's narrowing applies — so this is a representation change and
            // not a test. Decision 19 puts a `(&T)?`'s niche in the pointer, so
            // the option and its payload are the same word and there is nothing
            // to do; Decision 18 lays an `I64?` out as a discriminant and a
            // payload, so the payload has an offset and is read from it.
            //
            // **The cost.** The discriminant is not re-checked. Reading the
            // payload of an absent value would be whatever the slot held, and
            // what stops that is `science-types`' narrowing rather than
            // anything here — the same trust `Projection::Downcast` already
            // places in `match`.
            Rvalue::Narrow { operand, .. } => {
                let source = self.operand_ty(body, operand).ok_or_else(|| {
                    Unlowered::new(
                        "a narrowed read of an operand with no type: the payload's offset is the \
                         option's and there is nothing here to read it from",
                    )
                })?;
                let option = self.layout_of_ty(source)?;
                match &option.repr {
                    // Same bits: `Decision 19`'s niche is in the payload.
                    Repr::Niched { .. } | Repr::Scalar(_) => {
                        let value = self.typed_operand(ctx, operand, layout, insts)?;
                        insts.push(ExtInst::Above(Inst::Store { local: dest, value }));
                        Ok(())
                    }
                    Repr::Tagged { payload_offset, .. } => {
                        let offset = *payload_offset;
                        // **A *copied* payload that owns something is refused,
                        // because reading it here would be a second owner —
                        // and a *moved* one is not, because there is only
                        // ever one.**
                        //
                        // This loads the payload out of the option's slot,
                        // which for an `I64?` is the whole of the
                        // representation change and for a `String?` is a byte
                        // copy of a `{ ptr, len, cap }`. Whether that second
                        // triple is a second *owner* is exactly what
                        // `Operand::Copy` versus `Operand::Move` already
                        // states: a copy leaves the option alive to release
                        // its payload later, so the narrowed value's own
                        // release is a second free of memory the first
                        // returned; a move is `science-mir`'s own drop
                        // elaboration answering *"only one of these two now
                        // owns it"* before this arm ever runs — the flag that
                        // guards the option's drop is already false on every
                        // path this narrow reaches. Refusing the move too
                        // would be refusing a program this crate can already
                        // prove sound, on the grounds that a *different*
                        // program (the copy) is not.
                        //
                        // `print(s)` inside `if s?:` is the program that found
                        // this: `s`'s call-site argument is a plain value and
                        // not a borrow — a `String` prints without needing
                        // `any Display`, `Builder::prints_by_rendering`'s own
                        // note — so `science-mir` narrows it with exactly one
                        // use ahead and moves. Borrowing it instead —
                        // `h.describe()` inside `if h?:`, a method receiver —
                        // is the *other* half of the same bug, and is
                        // `science-mir`'s to fix rather than this crate's:
                        // `Builder::borrow_source` projects
                        // `Projection::Payload` onto the option so the borrow
                        // is of the payload's own storage and never reaches
                        // this arm as a narrow at all.
                        //
                        // It was unreachable until `emit_nullable_glue`
                        // landed, because a `String?` could not be dropped at
                        // all; the glue made the option usable and made this
                        // shape reachable in the same change, and
                        // `Map.insert`'s own round trip found it.
                        let payload = match *self.types.kind(self.referent(source)) {
                            TyKind::Nullable(inner) => inner,
                            _ => Ty::ERROR,
                        };
                        if operand.moved_place().is_none() && self.drop_runs_something(payload, 0)?
                        {
                            return Err(Unlowered::new(format!(
                                "a narrowed read of `{}`, whose payload owns something: reading it \
                                 out of the option copies an owner, and the option is still the \
                                 one that releases it — so the value would be freed twice. Move \
                                 the narrowed value instead of reading it twice, or borrow its \
                                 payload rather than reading it by value",
                                self.types.render(self.defs, source)
                            )));
                        }
                        let place = operand.place().ok_or_else(|| {
                            Unlowered::new(
                                "a narrowed read of a tagged option that is not a place: the \
                                 payload is read at an offset and a constant has no address",
                            )
                        })?;
                        let (base, _) = self.place_address(ctx, place, insts)?;
                        let address = ctx.value();
                        insts.push(ExtInst::FieldAddr {
                            dest: address,
                            base: Operand::Value(base),
                            offset,
                        });
                        let value = ctx.value();
                        insts.push(ExtInst::LoadAt {
                            dest: value,
                            address: Operand::Value(address),
                            layout: layout.clone(),
                        });
                        insts.push(ExtInst::Above(Inst::Store {
                            local: dest,
                            value: Operand::Value(value),
                        }));
                        Ok(())
                    }
                    _ => Err(Unlowered::new(format!(
                        "a narrowed read of `{}`, whose layout is neither Decision 18's tagged \
                         pair nor Decision 19's niche",
                        self.types.render(self.defs, source)
                    ))),
                }
            }
            // Decision 6's `T` into `T?` and `assign`'s §7 copy out of a
            // borrow, in two of its three shapes. The other five box, build a
            // vtable or need a branch, and each is refused by name in
            // [`Lowerer::lower_coercion`].
            Rvalue::Coerce { operand, coercion, .. } => {
                self.lower_coercion(body, ctx, *coercion, operand, dest, layout, ty, insts)
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
                // **A borrow of something already behind a fat pointer copies
                // the fat pointer; it does not take its address.** `def report
                // (err: borrowed any Error)` calling `err.message()` lowers to
                // `_2 = borrowed (*_1)` — a reborrow of the referent — and the
                // referent is `any Error`, which is two words. Storing *the
                // address of* those two words would make `_2` a pointer to a
                // fat pointer while its type says it is one, and the dispatch
                // that reads slot 0 out of it would read the data word as a
                // vtable. The value that a `borrowed any I` holds is the pair
                // itself, so a reborrow is the pair, copied.
                //
                // This is the same rule Decision 13's representation already
                // implies and the same one `&*x` follows for a `&dyn T`: the
                // width of a reference is the width of what it refers to
                // needing, and for an unsized referent that is two words, not
                // one.
                if layout == &layout_of(self.target, &CgTy::Interface) {
                    // The place is a reborrow — `borrowed (*_1)` — and the
                    // pair to copy is what `_1` holds, so the `Deref` comes
                    // off rather than being followed. Following it is what
                    // `place_address` would do and it cannot: its `Deref` arm
                    // refuses a base that is not a scalar pointer, and a fat
                    // pointer is two words. That refusal is right for every
                    // other reader and this is the one place that has to step
                    // around it rather than through it.
                    // A reborrow — `borrowed (*_1)` — reads the pair `_1`
                    // holds, so the `Deref` comes off rather than being
                    // followed; `place_address` cannot follow it at all,
                    // because its `Deref` arm refuses a base that is not a
                    // scalar pointer and a fat pointer is two words. A borrow
                    // of an owned `any I` local has no `Deref` to remove and
                    // the pair is simply where the place says it is. Both are
                    // the same sentence — *the value of a reference to an
                    // interface object is the pair itself* — and the only
                    // difference is whether one indirection has already been
                    // written down.
                    let source = match place.projection.split_last() {
                        Some((mir::Projection::Deref { .. }, head)) => {
                            mir::Place { local: place.local, projection: head.to_vec() }
                        }
                        _ => place.clone(),
                    };
                    let (address, _) = self.place_address(ctx, &source, insts)?;
                    let value = ctx.value();
                    insts.push(ExtInst::LoadAt {
                        dest: value,
                        address: Operand::Value(address),
                        layout: layout.clone(),
                    });
                    insts.push(ExtInst::Above(Inst::Store {
                        local: dest,
                        value: Operand::Value(value),
                    }));
                    return Ok(());
                }
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
            // A closure with nothing captured is a bare function pointer —
            // the value `cg_ty_in`'s `TyKind::Closure` arm now gives a layout
            // to. `param` is the identity `science-mir`'s `lower.rs` §8.5
            // minted the closure's body under, and `by_def` — the same table
            // every direct call's own symbol comes from — names it once
            // `science_codegen::mono`'s `Mono::walk_rvalue` has walked a use
            // of this closure and enqueued it. `Operand::GlobalAddr` already
            // resolves a function as well as a global (its own doc says so:
            // *"a string literal's bytes, a descriptor, a function"*), which
            // is what makes this one `Store` and no new backend primitive.
            Rvalue::Closure { param, captures, .. } if captures.is_empty() => {
                let symbol = self.by_def.get(param).cloned().ok_or_else(|| {
                    Unlowered::new(
                        "a capture-free closure whose body `science_codegen::mono` never \
                         walked: nothing reachable from `main` created a value of it, so it was \
                         never enqueued and no function exists to point at",
                    )
                })?;
                insts.push(ExtInst::Above(Inst::Store {
                    local: dest,
                    value: Operand::GlobalAddr(symbol),
                }));
                Ok(())
            }
            // A closure that captures something is still §8.5's hole,
            // unmoved: its value is `{ fn ptr, captures }`, `cg_ty_in` gives
            // every closure type the bare pointer that is only true of one
            // with nothing captured, and there is nowhere here to put the
            // rest even if there were room for it in the type.
            Rvalue::Closure { .. } => Err(Unlowered::new(
                "a closure that captures something: only a capture-free closure has a concrete \
                 representation today, a bare function pointer, and this one needs the \
                 `{ fn ptr, captures }` aggregate `science-mir`'s `Rvalue::Closure` documents and \
                 no phase builds",
            )),
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
    /// [`ExtInst::LoadNiche`] (or, through a projection, [`ExtInst::LoadAt`]
    /// at the niche's own scalar layout) and not `Inst::Load`, because for
    /// `(any I)?` the whole type is two words and *"the vtable slot of a null
    /// trait object is undefined and codegen must never load it — including
    /// on the path that tests for null"*. That path is this one.
    ///
    /// **A field's `?` is the same question asked of a computed address.**
    /// [`Lowerer::lower_discriminant`] found this first for `match`: a tagged
    /// discriminant is always at offset 0, so `place_address`'s computed
    /// pointer plus [`ExtInst::LoadAt`] at the tag's own width reads it with
    /// no new instruction. The niche is at offset 0 too — Decision 19 puts it
    /// there — so the same trick reads the niche's pointer-sized scalar
    /// through the pointer `place_address` already knows how to build for
    /// `over.host`, `Projection::Payload`'s neighbouring arm and all. Only a
    /// bare local, with no projection, still uses [`ExtInst::LoadTag`] /
    /// [`ExtInst::LoadNiche`], because reading its own address needs no
    /// `place_address` walk at all.
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
        let ty = self
            .operand_ty(body, operand)
            .ok_or_else(|| Unlowered::new("a `?` on a value with no type"))?;
        let layout = self.layout_of_ty(ty)?;
        let result = ctx.value();
        match &layout.repr {
            Repr::Tagged { variants, tag, .. } => {
                let null = variants
                    .iter()
                    .find(|variant| variant.payload.is_none())
                    .ok_or_else(|| {
                        Unlowered::new("a `?` on a tagged type with no payload-free variant")
                    })?
                    .discriminant;
                let value = ctx.value();
                if place.projection.is_empty() {
                    let local = LocalId(place.local.index() as u32);
                    if ctx.untyped.contains(&local) {
                        return Err(Unlowered::new(format!("{UNTYPED} (local _{})", local.0)));
                    }
                    ctx.layout(local)?;
                    insts.push(ExtInst::LoadTag { dest: value, local });
                } else {
                    let (address, at) = self.place_address(ctx, place, insts)?;
                    if !matches!(at.repr, Repr::Tagged { .. }) {
                        return Err(Unlowered::new(
                            "a `?` read through a projection whose layout is not Decision 18's \
                             tagged one",
                        ));
                    }
                    let tag_layout = layout_of(self.target, &CgTy::Int(*tag));
                    insts.push(ExtInst::LoadAt {
                        dest: value,
                        address: Operand::Value(address),
                        layout: tag_layout,
                    });
                }
                insts.push(ExtInst::Above(Inst::Cmp {
                    dest: result,
                    op: CmpOp::Ne,
                    signed: false,
                    lhs: Operand::Value(value),
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
                let value = ctx.value();
                if place.projection.is_empty() {
                    let local = LocalId(place.local.index() as u32);
                    if ctx.untyped.contains(&local) {
                        return Err(Unlowered::new(format!("{UNTYPED} (local _{})", local.0)));
                    }
                    ctx.layout(local)?;
                    insts.push(ExtInst::LoadNiche { dest: value, local });
                } else {
                    let (address, at) = self.place_address(ctx, place, insts)?;
                    let Repr::Niched { niche: at_niche, .. } = &at.repr else {
                        return Err(Unlowered::new(
                            "a `?` read through a projection whose layout is not Decision 19's \
                             niched one",
                        ));
                    };
                    if at_niche.offset != 0 {
                        return Err(Unlowered::new(
                            "a `?` on a nullable whose niche is not at offset 0",
                        ));
                    }
                    // The niche's own scalar type, and not the payload's whole
                    // layout: every niche this crate lays out is a pointer at
                    // offset 0 (`CgTy::niche`'s doc), the same fact
                    // `owned_nullable_return`'s `BranchAndMaterialiseNull` arm
                    // already relies on to build `niche_layout` this way.
                    let niche_layout = layout_of(self.target, &CgTy::Ptr(PtrKind::Raw));
                    insts.push(ExtInst::LoadAt {
                        dest: value,
                        address: Operand::Value(address),
                        layout: niche_layout,
                    });
                }
                insts.push(ExtInst::Above(Inst::Cmp {
                    dest: result,
                    op: CmpOp::Ne,
                    signed: false,
                    lhs: Operand::Value(value),
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

    /// Decision 6's widening, §7's copy, and a name for each of the five this
    /// is not.
    ///
    /// **Three of the eight are lowered.** [`Coercion::Identity`] is the
    /// assignment underneath it, [`Coercion::Widen`] is
    /// [`Lowerer::lower_widen`], and `assign`'s §7 —
    /// *"a borrow of a `Copy` type reads as a value"* — is
    /// [`Lowerer::copy_out_of_borrow`], a load through the pointer, for
    /// [`Coercion::Copy`] and for [`Coercion::CopyThenWiden`].
    ///
    /// **The order in `CopyThenWiden`'s name is the order of the
    /// instructions.** §7 says *"§7's rule then Decision 6's, in that order and
    /// never the reverse"*, and the two orders are different programs: copying
    /// and then widening loads `T` and stores it into the payload of a `T?`,
    /// where widening and then copying would build a `(borrowed T)?` and then
    /// have to read through a pointer that may be absent. So this composes with
    /// [`Lowerer::lower_widen`] rather than writing a second widening — the
    /// tag, the payload offset and Decision 19's asymmetry are decided in one
    /// place, and the only thing [`Coercion::CopyThenWiden`] changes is where
    /// the payload's *value* comes from.
    ///
    /// **[`Coercion::CopyWhenPresent`] is refused and the reason is Decision
    /// 5.** It is the one of the three with a branch in it.
    #[allow(clippy::too_many_arguments)]
    fn lower_coercion(
        &mut self,
        body: &MirBody,
        ctx: &mut BodyCtx,
        coercion: science_types::assign::Coercion,
        operand: &mir::Operand,
        dest: LocalId,
        layout: &Layout,
        ty: Ty,
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
            Coercion::Widen => {
                self.lower_widen(ctx, Widened::Operand(operand), dest, layout, insts)
            }
            Coercion::Box | Coercion::BoxThenWiden => {
                self.lower_box(body, ctx, operand, dest, ty, insts)
            }
            Coercion::Unsize | Coercion::UnsizeInBox => {
                self.lower_unsize(body, ctx, operand, dest, ty, insts)
            }
            // §7, whole: `borrowed T` into `T` is the load and nothing else.
            // The destination's layout is checked against the referent's rather
            // than assumed equal to it, because the two come from different
            // types — `layout` is the `Assign`'s slot and the referent is the
            // `Ty` the borrow points at — and a disagreement between them is
            // finding 12's shape, which opaque pointers make invisible to the
            // verifier.
            Coercion::Copy => {
                // **A `Copy` whose operand is already the value is the
                // identity, and `xs[i]` is how that arises.**
                //
                // The decision. When the operand's own type is not a borrow,
                // this emits the ordinary read that `Rvalue::Use` emits and
                // nothing else.
                //
                // The reason, and it is a disagreement between two true views
                // rather than a bug in either. `science-types` types `xs[i]` as
                // `Index.index`'s return, which is `&T`, and records a `Copy`
                // coercion to get the `T` the expression is used as.
                // `science-mir` lowers the same expression to a **place** —
                // `Projection::Index` carries the *element* type, because a
                // projection to an element is the element and not a reference
                // to one — so by the time the coercion arrives the load it
                // describes has already happened. `copy_out_of_borrow` then
                // refused, correctly by its own lights: there was no pointer to
                // read through.
                //
                // The cost, stated. This makes `Coercion::Copy` accept an
                // operand the coercion was not written for, so the one thing it
                // must not do is accept a *wrong* one: the destination's layout
                // is still checked against the operand's below, which is the
                // check finding 12 exists for and the only one that catches a
                // size disagreement opaque pointers hide.
                //
                // It was found by the most ordinary loop there is —
                // `total be total + xs[i]` — which is worth saying because
                // `print(f"{xs[i]}")` worked the whole time: an f-string hole
                // reads through `value_hole`, which never asks for a coercion.
                //
                // **`TyKind::Nullable(Borrowed(_))` is not this arm's "already
                // the value" case, and looked exactly like one.** `let cell be
                // items.get(index)` types `cell` as `(&Int)?`, and inside `if
                // cell?:` the narrowed `cell` still is not `TyKind::Borrowed`
                // at the top — Decision 19 wraps it in `Nullable` up to the
                // moment codegen picks a layout for it. Reading `!matches!(…,
                // Borrowed)` alone took this for `xs[i]`'s shape and stored the
                // niched pointer where the referent belongs: `local _0 is i64
                // and the value stored into it is ptr`, silent because the two
                // are the same width. `borrowed_referent` answers the one
                // question that tells them apart — is there a pointer to load
                // through at all — for both spellings of "there is one".
                let operand_ty = self.operand_ty(body, operand);
                if operand_ty.is_some_and(|ty| self.borrowed_referent(ty).is_none()) {
                    let value = self.typed_operand(ctx, operand, layout, insts)?;
                    insts.push(ExtInst::Above(Inst::Store { local: dest, value }));
                    return Ok(());
                }
                let (value, referent) = self.copy_out_of_borrow(body, ctx, operand, insts)?;
                if referent != *layout {
                    return Err(Unlowered::new(format!(
                        "a `Copy` out of a borrow of a {}-byte value into a {}-byte slot: §7 \
                         copies the value as it stands and there is no conversion to put between \
                         them",
                        referent.size, layout.size
                    )));
                }
                insts.push(ExtInst::Above(Inst::Store {
                    local: dest,
                    value: Operand::Value(value),
                }));
                Ok(())
            }
            // §7 then Decision 6, in that order. The load happens first and
            // unconditionally — the borrow itself is not nullable here, only
            // the destination is — so this is straight-line code and the
            // widening is the same one `Coercion::Widen` emits.
            Coercion::CopyThenWiden => {
                let (value, referent) = self.copy_out_of_borrow(body, ctx, operand, insts)?;
                let copied = Widened::Copied { value, layout: referent };
                self.lower_widen(ctx, copied, dest, layout, insts)
            }
            // **Refused, and it is Decision 5 rather than a missing
            // instruction.** `(borrowed T)?` into `T?` is §7's copy *under*
            // Decision 6's constructor, and `assign`'s own note says what that
            // costs: *"the copy runs only when the value is present … a
            // conversion with a branch in it rather than a load"*. The branch
            // is not optional and cannot be flattened: the absent case is a
            // null pointer, so a load hoisted above the test dereferences null
            // on exactly the input the test exists to catch.
            //
            // Every ingredient is here — [`Lowerer::lower_is_present`] asks the
            // question for both of Decision 18's and 19's representations,
            // [`Lowerer::copy_out_of_borrow`] is the present edge and
            // [`Lowerer::store_null`] is the absent one — and what is missing is
            // the place to put them. A statement lowers into one block's
            // instruction list; this one needs three blocks where MIR has one,
            // which is the line Decision 5 draws and which
            // [`Lowerer::lower_binary`] refuses integer `/` for as well. Half of
            // it — a load on the present edge and an uninitialised slot on the
            // other — would verify, link and run, which is why it is refused
            // rather than approximated.
            Coercion::CopyWhenPresent => Err(Unlowered::new(
                "a `Copy` out of a `(&T)?`, which `assign`'s §7 runs only when the value \
                 is present: that is a test and two edges, which is three basic blocks where MIR \
                 has one, and Decision 5 says \"every MIR basic block becomes exactly one LLVM \
                 basic block\" — the same line integer `/` is refused at",
            )),
        }
    }

    /// Decision 14's boxing: a concrete value moved to the heap and paired
    /// with a vtable.
    ///
    /// **The decision. One `science_box_new`, and the fat pointer is built
    /// from what it returns.** The runtime entry point takes a descriptor and
    /// a pointer to the value, allocates `size` bytes at `align`, and copies
    /// the value in; the allocation's address is the data word and the table
    /// is the vtable word, exactly as [`Lowerer::lower_unsize`] writes them.
    /// The two coercions differ in where the data word comes from and in
    /// nothing else, which is why they share everything below the allocation.
    ///
    /// **Not §2.6's plain `Box of T`, and the reason is what produces each.**
    /// `Coercion::Box` is `assign`'s rule 8, gated on
    /// `Coercions::is_any_error` — boxing a concrete value into `any Error`
    /// with no `Box.new` written at all, the one case §5 admits without it.
    /// An explicit `Box.new(doc)` is never a coercion: `associated_call`
    /// resolves it to an ordinary call at `builtins.rs`' declared `new`,
    /// which carries no Science body — so it is a [`Lowerer::prelude_method`]
    /// row, reached from [`Lowerer::lower_call`] and lowered through the same
    /// [`Lowerer::lower_runtime_call`] `Array.new` and `Map.new` already go
    /// through (its element read off the destination by
    /// [`Lowerer::box_operand_element`], since `Box.new`'s one argument is `T`
    /// and never `Box of T`), and not a coercion reached from here.
    ///
    /// **The value needs an address and may not have one.** `science_box_new`
    /// copies *from memory*, so an operand that is a constant — no place, no
    /// slot — is materialised into a temporary first and the temporary's
    /// address is what is passed. An operand that is already a place is read
    /// where it is: MIR has moved it, so nothing else will read it again.
    ///
    /// **What it does not do: free anything.** The box outlives this
    /// statement by construction — it is the value the destination holds —
    /// and releasing it is `TerminatorKind::Drop`'s, which refuses an
    /// interface object today because the drop goes through a vtable slot
    /// this backend does not yet emit. So a boxed value is allocated and
    /// never freed, and the shape of that leak is bounded by what
    /// [`Lowerer::intern_descriptor`] admits: a type that owns nothing, whose
    /// box is `size` bytes of plain data. It is a leak and it is named as one.
    fn lower_box(
        &mut self,
        body: &MirBody,
        ctx: &mut BodyCtx,
        operand: &mir::Operand,
        dest: LocalId,
        ty: Ty,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        let Some(interface) = self.object_interface(ty) else {
            return Err(Unlowered::new(format!(
                "a box into `{}`, which is not an interface object",
                self.types.render(self.defs, ty)
            )));
        };
        let Some(source) = self.operand_ty(body, operand) else {
            return Err(Unlowered::new(
                "a box of an operand with no type to read a size from: the descriptor is what \
                 says how many bytes to allocate and a constant carries none",
            ));
        };
        let Some(concrete) = self.concrete_head(source) else {
            return Err(Unlowered::new(format!(
                "a box of `{}`, whose concrete type this crate cannot name",
                self.types.render(self.defs, source)
            )));
        };
        let vtable = self.intern_vtable(interface, concrete, source)?;
        let descriptor = self.intern_descriptor(concrete, source)?;
        let value_layout = self.layout_of_ty(source)?;

        // The address the runtime copies from. A place is read where it is; a
        // constant has no address until this makes one.
        let value_address = match operand.place() {
            Some(place) => {
                let (address, _) = self.place_address(ctx, place, insts)?;
                address
            }
            None => {
                let slot = self.temp(ctx, value_layout.clone());
                let value = self.typed_operand(ctx, operand, &value_layout, insts)?;
                insts.push(ExtInst::Above(Inst::Store { local: slot, value }));
                let address = ctx.value();
                insts.push(ExtInst::LocalAddr { dest: address, local: slot });
                address
            }
        };

        let box_new = self.declare("science_box_new")?;
        let boxed = ctx.value();
        insts.push(ExtInst::Above(Inst::Call {
            dest: Some(boxed),
            callee: Callee::Runtime("science_box_new"),
            args: vec![
                Operand::GlobalAddr(descriptor),
                Operand::Value(value_address),
            ],
            ret: box_new.ret.clone(),
            sret_slot: None,
        }));

        self.store_fat_pointer(ctx, dest, Operand::Value(boxed), &vtable, insts)
    }

    /// The two stores every interface object is built with: the data word,
    /// then the table.
    ///
    /// Shared by [`Lowerer::lower_unsize`] and [`Lowerer::lower_box`] because
    /// the pair is the same pair — `science_codegen::descriptor::Vtable`'s
    /// slot order is meaningless if one of the two writes it at a different
    /// offset than the other reads it from, and one function is how they
    /// cannot.
    fn store_fat_pointer(
        &mut self,
        ctx: &mut BodyCtx,
        dest: LocalId,
        data: Operand,
        vtable: &Vtable,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        let fat = layout_of(self.target, &CgTy::Interface);
        let Repr::Aggregate { fields } = &fat.repr else {
            return Err(Unlowered::new(
                "an interface object whose layout is not the two-word aggregate `CgTy::Interface` \
                 lays out: `science-codegen`'s `layout` and this lowering disagree about \
                 Decision 13's representation",
            ));
        };
        let [data_place, table_place] = fields.as_slice() else {
            return Err(Unlowered::new(format!(
                "an interface object with {} word(s) rather than two",
                fields.len()
            )));
        };
        let (data_place, table_place) = (data_place.clone(), table_place.clone());

        let base = ctx.value();
        insts.push(ExtInst::LocalAddr { dest: base, local: dest });

        let data_address = ctx.value();
        insts.push(ExtInst::FieldAddr {
            dest: data_address,
            base: Operand::Value(base),
            offset: data_place.offset,
        });
        insts.push(ExtInst::StoreAt {
            address: Operand::Value(data_address),
            layout: data_place.layout.clone(),
            value: data,
        });

        let table_address = ctx.value();
        insts.push(ExtInst::FieldAddr {
            dest: table_address,
            base: Operand::Value(base),
            offset: table_place.offset,
        });
        insts.push(ExtInst::StoreAt {
            address: Operand::Value(table_address),
            layout: table_place.layout.clone(),
            value: Operand::GlobalAddr(vtable.symbol.clone()),
        });
        Ok(())
    }

    /// Decision 13's unsizing: an existing pointer paired with a vtable.
    ///
    /// **The decision. `{ data, vtable }` is written at the destination's
    /// offset 0, and that is the whole of it for all four destination
    /// spellings.** `borrowed any I` and `Box of any I` are the fat pointer
    /// itself; `(any I)?` is `Repr::Niched` with the niche in the data word
    /// (§3.4), so the *same two stores* at the same two offsets produce a
    /// present nullable and there is no tag to write. Decision 19's asymmetry,
    /// which [`Lowerer::lower_widen`] has to branch on, does not arise here
    /// because an interface object is never the tagged form.
    ///
    /// **The data word is the operand, unchanged.** `Coercion::Unsize` is
    /// `borrowed C` into `borrowed any I` and `Coercion::UnsizeInBox` is
    /// `Box of C` into `Box of any I`; `assign`'s §4 says of both that they
    /// *"pair an existing pointer with a vtable"* and allocate nothing, so the
    /// pointer that arrives is the pointer that is stored. Whatever owned the
    /// referent before still owns it — this writes no ownership and takes
    /// none.
    ///
    /// **The vtable word is a global's address and never a computed value.**
    /// `science_codegen::descriptor::Vtable` is the account: one table per
    /// `(interface, concrete type)` pair, interned, so two unsizings of one
    /// pair name one global.
    fn lower_unsize(
        &mut self,
        body: &MirBody,
        ctx: &mut BodyCtx,
        operand: &mir::Operand,
        dest: LocalId,
        ty: Ty,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        let Some(interface) = self.object_interface(ty) else {
            return Err(Unlowered::new(format!(
                "an unsizing into `{}`, which is not an interface object: the destination of a \
                 `Coercion::Unsize` is `any I` by construction, so this is a type table and a \
                 coercion that disagree",
                self.types.render(self.defs, ty)
            )));
        };
        let Some(source) = self.operand_ty(body, operand) else {
            return Err(Unlowered::new(
                "an unsizing of an operand with no place to read a type from: a constant cannot \
                 be borrowed, and the concrete type is what names the vtable",
            ));
        };
        let Some(concrete) = self.concrete_head(source) else {
            return Err(Unlowered::new(format!(
                "an unsizing of `{}`, whose concrete type this crate cannot name: a type \
                 parameter's vtable is chosen by the instantiation, which is \
                 `science_codegen::mono`'s and not this crate's",
                self.types.render(self.defs, source)
            )));
        };
        let vtable = self.intern_vtable(interface, concrete, source)?;
        // The data word is the operand itself, materialised at the pointer
        // layout the pair's first field has.
        let pointer = layout_of(self.target, &CgTy::Ptr(PtrKind::Borrow));
        let data = self.typed_operand(ctx, operand, &pointer, insts)?;
        self.store_fat_pointer(ctx, dest, data, &vtable, insts)
    }

    /// Decision 6's widening, with the payload's value left open.
    ///
    /// **It writes the tag and then the payload, in that order, and neither is
    /// optional.** A tagged `T?` is a discriminant at offset 0 and a payload at
    /// `payload_offset`, and a widening that wrote only the payload would leave
    /// the tag holding whatever the slot held — which for a freshly `alloca`'d
    /// slot is a value that is `null` about half the time and is never
    /// reported. A niched `T?` **is** its payload, so there is no tag to write
    /// and the store is the ordinary one; that asymmetry is Decision 19 and is
    /// why the two arms do not share a line.
    ///
    /// **[`Widened`] is what makes [`Coercion::CopyThenWiden`] compose rather
    /// than copy this function.** Everything above is the same for both
    /// coercions and only the payload's value differs, so the value is the
    /// parameter.
    fn lower_widen(
        &mut self,
        ctx: &mut BodyCtx,
        source: Widened<'_>,
        dest: LocalId,
        layout: &Layout,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        match &layout.repr {
            Repr::Tagged { payload_offset, variants, .. } => {
                let present = variants
                    .iter()
                    .find(|variant| variant.payload.is_some())
                    .ok_or_else(|| {
                        Unlowered::new(
                            "a widening into a tagged type with no payload-carrying variant",
                        )
                    })?;
                let payload =
                    present.payload.clone().expect("the variant was found by having a payload");
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
                let value = self.widened_value(ctx, source, &payload, insts)?;
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
                let value = self.widened_value(ctx, source, &payload, insts)?;
                insts.push(ExtInst::Above(Inst::Store { local: dest, value }));
                Ok(())
            }
            _ => Err(Unlowered::new(
                "a widening into a type that is neither Decision 18's tagged nullable nor \
                 Decision 19's niched one",
            )),
        }
    }

    /// The payload a [`Lowerer::lower_widen`] writes, from whichever of the two
    /// places it comes from.
    ///
    /// **The `Copied` arm checks the layout and the `Operand` arm does not have
    /// to.** [`Lowerer::typed_operand`] *builds* its value at the payload's
    /// layout, so the widths agree by construction; a copy out of a borrow was
    /// loaded at the referent's layout, which is a second type, and a
    /// disagreement between the two is a store of the wrong width into a slot
    /// inside another object. [`ExtInst::StoreAt`] catches that in the emitter,
    /// and catching it here as well is the difference between a refusal naming
    /// the construct and a `BackendError` naming two LLVM types.
    fn widened_value(
        &mut self,
        ctx: &mut BodyCtx,
        source: Widened<'_>,
        payload: &Layout,
        insts: &mut Vec<ExtInst>,
    ) -> Result<Operand, Unlowered> {
        match source {
            Widened::Operand(operand) => self.typed_operand(ctx, operand, payload, insts),
            Widened::Copied { value, layout } => {
                if layout != *payload {
                    return Err(Unlowered::new(format!(
                        "a `CopyThenWiden` whose copy is {} bytes and whose `T?` holds a {}-byte \
                         payload: §7's rule and Decision 6's disagree about the type in the \
                         middle",
                        layout.size, payload.size
                    )));
                }
                Ok(Operand::Value(value))
            }
        }
    }

    /// `assign`'s §7: the value behind a borrow, loaded.
    ///
    /// Gives back the loaded value and **the layout it was loaded at**, which
    /// is the referent's and not the destination's — the caller checks the two
    /// against each other, because it is the caller that knows which of §7's
    /// three shapes this is.
    ///
    /// **The operand is a pointer and the result is what it points at**, which
    /// is one [`ExtInst::LoadAt`] and is the same row of §2.3's table
    /// [`Lowerer::place_address`]'s `Projection::Deref` arm reads — including
    /// its guard, that the thing being dereferenced really is
    /// `Repr::Scalar(Scalar::Pointer(_))`. That guard is load-bearing for
    /// finding 18's reason: opaque pointers make a pointer to a `T` and a
    /// pointer to the slot holding a pointer to a `T` the same LLVM type, so a
    /// source operand that was not a borrow at all would load garbage the
    /// verifier accepts.
    ///
    /// **The `Copy` bound is not checked here and is not this crate's to
    /// check.** `science-types`' `Coercions::of` discharges it through
    /// `Methods::declares` before the coercion is ever recorded, and its own
    /// note gives the reason a `Copy` this crate cannot classify must not be
    /// admitted: *"the failure would be a silent double drop"*. What is checked
    /// here is the machine-level precondition — that there is a pointer to load
    /// through — because that is the part a wrong answer above the line turns
    /// into a wrong number rather than a diagnostic.
    fn copy_out_of_borrow(
        &mut self,
        body: &MirBody,
        ctx: &mut BodyCtx,
        operand: &mir::Operand,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(ValueId, Layout), Unlowered> {
        let source = self.operand_ty(body, operand).ok_or_else(|| {
            Unlowered::new(
                "a `Copy` out of an operand with no type: the load's width is the referent's and \
                 there is nothing here to read it from",
            )
        })?;
        let referent = self.borrowed_referent(source).ok_or_else(|| {
            Unlowered::new(format!(
                "a `Copy` out of `{}`, which is not a borrow: §7's rule reads through a pointer \
                 and this operand is not one",
                self.types.render(self.defs, source)
            ))
        })?;
        let borrow = self.layout_of_ty(source)?;
        // A niched `(borrowed T)?`'s own repr is `Niched`, not `Scalar` — its
        // *payload* is the pointer-shaped layout the guard below is checking
        // for, the same distinction `place_address`'s `Deref` arm already
        // draws for the identical reason.
        let pointer_repr = match &borrow.repr {
            Repr::Niched { payload, .. } => &payload.repr,
            other => other,
        };
        if !matches!(pointer_repr, Repr::Scalar(Scalar::Pointer(_))) {
            return Err(Unlowered::new(
                "a `Copy` out of a borrow whose representation is not a pointer",
            ));
        }
        let layout = self.layout_of_ty(referent)?;
        let address = self.typed_operand(ctx, operand, &borrow, insts)?;
        let value = ctx.value();
        insts.push(ExtInst::LoadAt { dest: value, address, layout: layout.clone() });
        Ok((value, layout))
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
            let value = self.field_value(ctx, operand, &place.layout, insts)?;
            insts.push(ExtInst::StoreAt {
                address: Operand::Value(address),
                layout: place.layout.clone(),
                value,
            });
        }
        Ok(())
    }

    /// One field or element's value, with Decision 15's string construction
    /// folded in.
    ///
    /// # The decision
    ///
    /// A `Literal::Str` reaching a field is built into a slot of its own with
    /// [`Lowerer::build_string`] and then loaded; everything else goes to
    /// [`Lowerer::typed_operand`] unchanged.
    ///
    /// # The reason
    ///
    /// `Doc(title: "a", body: "…")` was refused — *"a string literal read as a
    /// value rather than bound or printed: Decision 15 makes it a
    /// `science_string_from_bytes` call, which needs a slot to own the result
    /// and a `science_string_free` to pair with"*. Both halves of that sentence
    /// are answered here rather than being reasons to refuse:
    ///
    /// - **The slot** is one this function invents, exactly as a `let` binding's
    ///   is. `science_string_from_bytes` returns three words through `sret`, so
    ///   it needs somewhere to write; a field address cannot be that somewhere,
    ///   because `Inst::Call`'s `sret_slot` names a local and not an arbitrary
    ///   address. The slot is written once and copied once.
    /// - **The free** is the record's, not this site's, and that is what makes
    ///   the ownership right rather than merely convenient. The literal is
    ///   *moved* into the field, so the record owns it from the store onward
    ///   and Decision 12's glue — which `intern_drop_glue` already emits for a
    ///   record with a `String` field — is the pairing the message asked for.
    ///   A `science_string_free` here would be a double free, which is the
    ///   defect `element_move` was written for one container over.
    ///
    /// **This was never about strings being hard.** A string literal bound to a
    /// `let` and a string literal printed both worked, and so did one passed to
    /// a function; what had no path was every position where the value's owner
    /// is an *aggregate* — a record field, a tuple element, or a `choice`
    /// variant's payload. All three call this, because all three had the same
    /// hole for the same reason and fixing one of them would have left a user
    /// wondering which containers strings are allowed in. `Loaded("…")` is as
    /// ordinary as `Doc(title: "…")`.
    ///
    /// # The cost
    ///
    /// One slot and one aggregate copy per literal field, where a construction
    /// straight into the field would need neither. `sret_slot` taking a
    /// `LocalId` is what stands between the two, and widening it to an address
    /// is a change to the backend's instruction set for an optimisation
    /// `mem2reg` already takes: the slot is written once, read once, and never
    /// escapes.
    fn field_value(
        &mut self,
        ctx: &mut BodyCtx,
        operand: &mir::Operand,
        layout: &Layout,
        insts: &mut Vec<ExtInst>,
    ) -> Result<Operand, Unlowered> {
        let mir::Operand::Const(Constant::Literal(Literal::Str(text))) = operand else {
            return self.typed_operand(ctx, operand, layout, insts);
        };
        let text = text.clone();
        let slot = self.temp(ctx, layout.clone());
        self.build_string(&text, slot, insts)?;
        let loaded = ctx.value();
        insts.push(ExtInst::Above(Inst::Load { dest: loaded, local: slot }));
        Ok(Operand::Value(loaded))
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
            let value = self.field_value(ctx, operand, &place.layout, insts)?;
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
            let value = self.field_value(ctx, &payload[0], &declared, insts)?;
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
        // **`is` and `is not` on a `String` are a runtime call and not an
        // instruction**, and this is the arm that says so before `scalar_of`
        // refuses the aggregate. §4.6 gives the language one spelling of
        // equality and `stdlib-core.md` makes `String implements Eq`, so
        // `name is not ""` — `examples/01_functions.science`'s — is an
        // ordinary comparison whose operands happen to be three words each.
        // `science_string_eq` is `RUNTIME`'s and compares the bytes.
        if self.is_string(self.referent(operand_ty)) {
            return self.lower_string_comparison(ctx, op, lhs, rhs, dest, insts);
        }
        // **A reference as the operand of an operator, and this is where a
        // user meets it.** §3's finding 27 *was* that `science-types` wrapped
        // a shared borrow of a scalar in a `Coercion::Copy` before an operator
        // saw it and did not wrap an exclusive one, so
        // `def bump(counter: mutable borrowed Int): counter be counter + 1` —
        // `examples/01_functions.science`'s, and the only spelling §4.7 leaves
        // — arrived as `_1 + 1` with `_1` holding a pointer. That is closed:
        // `assign`'s §7 no longer consults the borrow's mutability, gated to
        // `Site::Operand` so an exclusive borrow reads as a value where the
        // language offers no alternative and nowhere else.
        //
        // **What reaches here now is a borrow the coercion did not license**,
        // which means the referent does not implement `Copy` as
        // `Methods::declares` can see it — a bound on a type parameter is the
        // case `operators.rs` pins, since §3's discipline refuses what it
        // cannot see.
        //
        // **Refused by name rather than by accident.** Without this arm the
        // pointer reaches `typed_operand`, the constant beside it is given a
        // pointer layout, and the build ends in `SC0402` with *"a constant of a
        // type this backend cannot build"* — a message about a constant, from
        // below the linker, for a mistake two phases up. Decision 19 makes a
        // pointer a scalar, so nothing between here and LLVM would have
        // objected on its own.
        if let TyKind::Borrowed { mutable, .. } = *self.types.kind(operand_ty) {
            let written = if mutable { "mutable borrowed" } else { "borrowed" };
            return Err(Unlowered::new(format!(
                "`{}` applied to a `{written} {}`: an operator reads its operands and this one \n                 is a reference. `science-types` inserts the dereferencing coercion at an \n                 operand when the referent implements `Copy`, so this one's does not — a \n                 bound on a type parameter is the case that cannot be seen — and the operand \n                 reaches this crate as a pointer",
                op.as_str(),
                self.render_referent(operand_ty)
            )));
        }
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

        // **`**` is a call, on both sides of the `float` split, and it has to
        // be decided before that split rather than inside either arm of it.**
        // Neither `Inst::FloatBinary` nor `Inst::IntBinary` has a `Pow`
        // variant to reach for, for two different reasons that land on the
        // same fix: see [`Lowerer::lower_pow`]. `Bool`, `Char` and a pointer
        // are left to fall through to the ordinary arms below and their
        // existing `describe_binary` refusal, unchanged — `Pow` is only ever
        // legitimately dispatched to a `Scalar::Int` or a `Scalar::Float`,
        // and a program that reaches this point with anything else is a
        // checker disagreement this function already knew how to report.
        if op == BinaryOp::Pow && matches!(scalar, Scalar::Int(_) | Scalar::Float(_)) {
            return self.lower_pow(ctx, scalar, &operand_layout, left, right, dest, insts);
        }

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
                // **The caller guards it now, and that sentence used to be the
                // refusal.** `science_codegen::backend::IntOp` says of `SDiv`
                // that *"division by zero is a panic the caller has already
                // guarded, not a trap the backend inserts"*, and this arm
                // existed because **no caller did** — so a bare `sdiv` was
                // being handed an instruction whose behaviour at `b == 0` is
                // *undefined* rather than trapping, which at `-O2` licenses
                // deleting the very branch that was going to check.
                //
                // `science-mir`'s `division_check` is that caller. The guard is
                // emitted where basic blocks are made, in front of the
                // division, exactly as `bounds_check` is emitted in front of an
                // index — so Decision 5's *"every MIR basic block becomes
                // exactly one LLVM basic block"* survives, which is the reason
                // this arm would not emit the guard itself.
                //
                // **Two failing inputs, not one.** `Int.min / -1` is immediate
                // UB for `sdiv` and `srem` just as the zero is, and it is
                // `#DE`/`SIGFPE` on x86-64; MIR guards it for the signed types
                // and skips it for the unsigned ones, where no unrepresentable
                // quotient exists.
                BinaryOp::Div if signed => IntOp::SDiv,
                BinaryOp::Div => IntOp::UDiv,
                BinaryOp::Rem if signed => IntOp::SRem,
                BinaryOp::Rem => IntOp::URem,
                // **The caller guards this one now too.** A shift by at least
                // the operand's width is poison in LLVM — not a wrong value,
                // no value — exactly as `sdiv` is undefined at a zero
                // divisor, and this arm existed because nothing above this
                // crate bounded the amount. `science-mir`'s `shift_check` is
                // that caller now, built `division_check`'s own shape: the
                // guard is emitted where basic blocks are made, in front of
                // the shift, so Decision 5 survives here exactly as it does
                // for `/` and `%` a few lines up.
                BinaryOp::Shl => IntOp::Shl,
                BinaryOp::Shr if signed => IntOp::AShr,
                BinaryOp::Shr => IntOp::LShr,
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

    /// `**`, both operands the same width and signedness (`Self.pow(Self) ->
    /// Self`, §5.4). A call in both cases, and for two different reasons.
    ///
    /// **`Float ** Float` has no instruction and no whitelisted intrinsic.**
    /// `llvm.pow` is not on `intrinsics-math-physics.md` §3.1's table
    /// `codegen-and-linking.md` §7.3's Decision 37 reads as a constant-folding
    /// whitelist, so emitting it would let LLVM fold a constant `**` against
    /// the *build host's* libm — the exact hazard Decision 37 exists to close.
    /// The fix Decision 37 gives is *"a direct call to `science-libm`'s
    /// symbol"*; `science-libm` does not exist yet, so the symbol lives in
    /// `science-rt`'s `math.rs` under the name Decision 37 asks for,
    /// `science_libm_pow`, until it does.
    ///
    /// **`Int ** Int` has no instruction at all.** Exponentiation by squaring
    /// is a loop, and this crate's Decision 8 makes every MIR basic block
    /// exactly one LLVM basic block — the same reason `BinaryOp::Div`'s
    /// zero-guard is refused a few lines up rather than inlined. A call hides
    /// the loop inside a function this instruction sequence never sees, which
    /// is `science_ipow_i64`, beside its sibling in the same module.
    ///
    /// **Narrower than 64 bits widens, calls, then narrows.** One call exists,
    /// at the widest case of each kind, and `I8`…`I32` and `F32` reach it
    /// through the same [`ConvOp`] conversions [`Lowerer::lower_cast`] already
    /// builds — the shape `science_string_push_i64`'s own note describes:
    /// *"`I8`…`I64` are sign-extended by codegen before the call"*. Eight
    /// entry points, one per integer width, would say nothing eight lines here
    /// do not already say once.
    fn lower_pow(
        &mut self,
        ctx: &mut BodyCtx,
        scalar: Scalar,
        operand_layout: &Layout,
        left: Operand,
        right: Operand,
        dest: LocalId,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        let (symbol, wide_ty, is_float, signed): (&'static str, CgTy, bool, bool) = match scalar {
            Scalar::Float(_) => {
                ("science_libm_pow", CgTy::Float(science_codegen::layout::FloatTy::F64), true, false)
            }
            Scalar::Int(int) => ("science_ipow_i64", CgTy::Int(IntTy::I64), false, int.is_signed()),
            // `matches!(scalar, Scalar::Int(_) | Scalar::Float(_))` at the call
            // site is exhaustive against the two arms above; nothing else
            // reaches this function.
            _ => unreachable!("the caller's guard admits only `Int` and `Float`"),
        };
        let wide_layout = layout_of(self.target, &wide_ty);
        // `Int as Int` and `F64 as F64` both take this path with no
        // conversion at all — `Int`, the common case in `examples/12_operators.science`,
        // is already 64 bits wide.
        let widen_op = if operand_layout.size == wide_layout.size {
            None
        } else if is_float {
            Some(ConvOp::FloatExtend)
        } else if signed {
            Some(ConvOp::SignExtend)
        } else {
            Some(ConvOp::ZeroExtend)
        };
        let narrow_op = if is_float { ConvOp::FloatTrunc } else { ConvOp::Trunc };

        let widen = |ctx: &mut BodyCtx, insts: &mut Vec<ExtInst>, value: Operand| -> Operand {
            match widen_op {
                None => value,
                Some(op) => {
                    let converted = ctx.value();
                    insts.push(ExtInst::Convert {
                        dest: converted,
                        op,
                        value,
                        from: operand_layout.clone(),
                        to: wide_layout.clone(),
                    });
                    Operand::Value(converted)
                }
            }
        };
        let wide_left = widen(ctx, insts, left);
        let wide_right = widen(ctx, insts, right);

        let sig = self.declare(symbol)?;
        let call_result = ctx.value();
        insts.push(ExtInst::Above(Inst::Call {
            dest: Some(call_result),
            callee: Callee::Runtime(symbol),
            args: vec![wide_left, wide_right],
            ret: sig.ret.clone(),
            sret_slot: None,
        }));

        let value = match widen_op {
            None => Operand::Value(call_result),
            Some(_) => {
                let narrowed = ctx.value();
                insts.push(ExtInst::Convert {
                    dest: narrowed,
                    op: narrow_op,
                    value: Operand::Value(call_result),
                    from: wide_layout.clone(),
                    to: operand_layout.clone(),
                });
                Operand::Value(narrowed)
            }
        };
        insts.push(ExtInst::Above(Inst::Store { local: dest, value }));
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

    /// A type with one enclosing borrow taken off.
    ///
    /// [`Lowerer::render_referent`] is the same walk for a message; this is it
    /// for a question, and the two are separate because one returns a string a
    /// user reads and the other a `Ty` a predicate asks about.
    fn referent(&self, ty: Ty) -> Ty {
        match self.types.kind(ty) {
            TyKind::Borrowed { inner, .. } => *inner,
            _ => ty,
        }
    }

    /// The type behind a pointer `ty`'s own bytes actually are, whether `ty`
    /// says so as a plain `borrowed T` or as a niched `(borrowed T)?`.
    ///
    /// **Why both spellings answer the same question.** Decision 19 gives
    /// `(borrowed T)?` no discriminant of its own — the niche is the null
    /// pointer, so the whole value *is* its payload, and a narrowed read of it
    /// (`science-mir`'s `as_place` seeing straight through the narrow) hands
    /// back exactly the bytes a plain `borrowed T` would have. `None` is every
    /// other shape, including `T?` where `T` is not itself a borrow — that one
    /// is Decision 18's tagged pair, a different offset and a different
    /// question, and [`Rvalue::Narrow`]'s own arm is where that is answered.
    fn borrowed_referent(&self, ty: Ty) -> Option<Ty> {
        match self.types.kind(ty) {
            TyKind::Borrowed { inner, .. } => Some(*inner),
            TyKind::Nullable(payload) => match self.types.kind(*payload) {
                TyKind::Borrowed { inner, .. } => Some(*inner),
                _ => None,
            },
            _ => None,
        }
    }

    /// `a is b` and `a is not b` on a `String`: `science_string_eq`.
    ///
    /// **The decision. Equality on a `String` is a call, and `is not` is that
    /// call and one `icmp`.** `science_string_eq` takes two `*const
    /// ScienceString` and compares the bytes; there is no
    /// `science_string_ne`, and inverting the answer here is cheaper and
    /// shorter than a second entry point.
    ///
    /// **The reason it is not the scalar path.** `CmpOp::Eq` on a three-word
    /// aggregate is not an instruction LLVM has, and the layout is
    /// `{ ptr, len, cap }` — so a comparison that reached [`Inst::Cmp`] would
    /// be comparing *pointers*, which answers *"are these the same buffer"*
    /// and not *"are these the same text"*. Two strings with equal bytes and
    /// different allocations are `is` and would have compared unequal, and
    /// nothing in the IR or the verifier distinguishes the two questions.
    ///
    /// **Only `is` and `is not`.** `<` and `>` on a `String` are
    /// `stdlib-core.md`'s `Ord` and `science_string_cmp` exists for them, but
    /// the ordering the runtime implements is a byte ordering and no note in
    /// this repository says that is the ordering the language means; a
    /// collation question answered by whichever function was nearest is the
    /// kind of guess §11 exists to refuse.
    ///
    /// **A literal operand is Decision 15's temporary and it is freed here.**
    /// `"" ` has no MIR local, so this call site builds the `ScienceString`,
    /// compares it, and releases it before the result is stored — the same
    /// build/use/free trio [`Lowerer::lower_print`] emits for `print("…")`, and
    /// for the same reason: nothing else in the program can own it.
    #[allow(clippy::too_many_arguments)]
    fn lower_string_comparison(
        &mut self,
        ctx: &mut BodyCtx,
        op: BinaryOp,
        lhs: &mir::Operand,
        rhs: &mir::Operand,
        dest: LocalId,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        if !matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
            return Err(Unlowered::new(format!(
                "`{}` on a `String`: `science_string_cmp` is in `RUNTIME` and orders by bytes, \
                 and no note in this repository says a byte ordering is the ordering \
                 `stdlib-core.md`'s `String implements Ord` means",
                op.as_str()
            )));
        }
        let (left, left_temp) = self.string_pointer(ctx, lhs, insts)?;
        let (right, right_temp) = self.string_pointer(ctx, rhs, insts)?;
        let equal = self.declare("science_string_eq")?;
        let value = ctx.value();
        insts.push(ExtInst::Above(Inst::Call {
            dest: Some(value),
            callee: Callee::Runtime("science_string_eq"),
            args: vec![Operand::Value(left), Operand::Value(right)],
            ret: equal.ret.clone(),
            sret_slot: None,
        }));
        for slot in [left_temp, right_temp].into_iter().flatten() {
            let free = self.declare("science_string_free")?;
            let address = ctx.value();
            insts.push(ExtInst::LocalAddr { dest: address, local: slot });
            insts.push(ExtInst::Above(Inst::Call {
                dest: None,
                callee: Callee::Runtime("science_string_free"),
                args: vec![Operand::Value(address)],
                ret: free.ret.clone(),
                sret_slot: None,
            }));
        }
        let result = if op == BinaryOp::Eq {
            value
        } else {
            // The same negation `UnaryOp::Not` emits, and the only one this
            // instruction set has: a comparison against zero.
            let negated = ctx.value();
            insts.push(ExtInst::Above(Inst::Cmp {
                dest: negated,
                op: CmpOp::Eq,
                signed: false,
                lhs: Operand::Value(value),
                rhs: Operand::ConstInt(0),
            }));
            negated
        };
        insts.push(ExtInst::Above(Inst::Store { local: dest, value: Operand::Value(result) }));
        Ok(())
    }

    /// One operand of a `String` comparison, as the `*const ScienceString` the
    /// entry point wants.
    ///
    /// The second half of the answer is the slot this call site has to free, if
    /// it built one. Three shapes reach here and they are the three
    /// [`Lowerer::lower_print`] already distinguishes: a literal, which is a
    /// temporary this crate owns; a place whose slot *holds* a pointer, which
    /// is a `borrowed String` and whose value is the argument; and a place that
    /// *is* a `String`, whose address is.
    ///
    /// **A `Move` of a whole `String` is refused rather than compared.** MIR
    /// spells a read of a non-`Copy` place as a move, so `s is ""` where `s` is
    /// an owned `String` arrives with its only owner consumed by a comparison
    /// that does not consume anything — drop elaboration has deleted the
    /// binding's `Drop` and nothing here would free it. That is finding 23's
    /// shape with the leak on the other side, and the repair is a `copy` one
    /// phase up rather than a `science_string_free` guessed at here.
    fn string_pointer(
        &mut self,
        ctx: &mut BodyCtx,
        operand: &mir::Operand,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(ValueId, Option<LocalId>), Unlowered> {
        match operand {
            mir::Operand::Const(Constant::Literal(Literal::Str(text))) => {
                let layout = layout_of(self.target, &RtAggregate::String.cg_ty());
                let slot = self.temp(ctx, layout);
                self.build_string(text, slot, insts)?;
                let address = ctx.value();
                insts.push(ExtInst::LocalAddr { dest: address, local: slot });
                Ok((address, Some(slot)))
            }
            mir::Operand::Copy(place) | mir::Operand::Move(place) => {
                let (address, layout) = self.place_address(ctx, place, insts)?;
                if matches!(layout.repr, Repr::Scalar(Scalar::Pointer(_))) {
                    let pointer = ctx.value();
                    insts.push(ExtInst::LoadAt {
                        dest: pointer,
                        address: Operand::Value(address),
                        layout,
                    });
                    return Ok((pointer, None));
                }
                if matches!(operand, mir::Operand::Move(_)) {
                    return Err(Unlowered::new(
                        "a comparison that MIR spells as a `move` of a whole `String`: a \
                         comparison reads and does not consume, so the operand should be a \
                         `copy` and the buffer's release should stay with the binding's `Drop`",
                    ));
                }
                Ok((address, None))
            }
            _ => Err(Unlowered::new(
                "a `String` compared against an operand that is neither a literal nor a place",
            )),
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
    /// becomes a constant. `expected` is the destination's layout, and for
    /// every literal but `null` and a string it is unused.
    ///
    /// **`null` needs `expected` to decide *which* absent value, because the
    /// two representations disagree about what a value even is.** A niched
    /// `T?` is its payload and nothing else, so `null` is the payload's own
    /// null pointer and this function has always built that directly. A
    /// tagged `T?` — Decision 18's discriminant beside a payload union — is
    /// not a scalar at all, and this arm used to refuse it outright: *"a
    /// `null` read as a value of a type whose absent case is not a null
    /// pointer at offset 0"*, which every `Int?`, `Bool?`, `F64?`, record `?`
    /// and `String?` hit the moment `null` reached anywhere but
    /// `Rvalue::Use`'s own destination — a tuple element, a record field, a
    /// choice payload, a call argument.
    ///
    /// **The fix is to build the same bytes [`Lowerer::store_null`] builds,
    /// in a slot invented for the purpose, and read the whole struct back as
    /// one value** — exactly the shape [`Lowerer::field_value`] already uses
    /// for a string literal reaching a field. The tag is
    /// `store_null`'s, so a `null` built here and one built by `let x be
    /// null` write the same discriminant by construction, which is what
    /// keeps this arm from disagreeing with [`Lowerer::lower_is_present`] and
    /// [`Rvalue::Narrow`]'s Tagged arm about which bit pattern absence is.
    /// The payload union is left uninitialised, which is Decision 18's own
    /// rule for the absent case and is never read by either of those two
    /// consumers.
    ///
    /// **Cost.** One `alloca`, one store and one load per tagged `null` that
    /// is not already a `Rvalue::Use`'s whole destination — the same price
    /// `field_value` already pays for a string literal, and `mem2reg` turns
    /// the slot into nothing once optimisation is on.
    ///
    /// **Finding 30: a string literal has the identical shape and was still
    /// refused here.** `field_value` already builds one into an invented
    /// slot for a record field or a single-element `choice` payload; this
    /// function refused the same literal outright everywhere else, which is
    /// [`Lowerer::lower_variant`]'s *second and later* payload elements —
    /// `ConfigError.Malformed("empty input", 1)`'s two-element case calls
    /// [`Lowerer::typed_operand`] per element and not `field_value` — and any
    /// `extern "C"` argument that names `lower_operand` directly. The fix is
    /// the same one `null` already has: `self.temp`, [`Lowerer::build_string`]
    /// into it, and a load of the whole `String` back out. The cost is the
    /// same `alloca`/store/load `field_value` already pays, once more per
    /// site this function is now asked from; `expected` is `None` only at a
    /// condition or a `match` discriminant, neither of which types as
    /// `String`, so the refusal that remains is not one any program reaches.
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
                    Some(Repr::Tagged { .. }) => {
                        let layout = expected
                            .expect("Some(Repr::Tagged) is matched only through Some(layout)")
                            .clone();
                        let slot = self.temp(ctx, layout.clone());
                        self.store_null(slot, &layout, insts)?;
                        let dest = ctx.value();
                        insts.push(ExtInst::Above(Inst::Load { dest, local: slot }));
                        Ok(Operand::Value(dest))
                    }
                    _ => Err(Unlowered::new(
                        "a `null` read as a value of a type whose absent case is not a null \
                         pointer at offset 0",
                    )),
                },
                // Finding 30, this function's own doc comment. The caller
                // owns the result, exactly as `build_string`'s own doc says.
                Literal::Str(text) => match expected {
                    Some(layout) => {
                        let layout = layout.clone();
                        let slot = self.temp(ctx, layout);
                        self.build_string(text, slot, insts)?;
                        let dest = ctx.value();
                        insts.push(ExtInst::Above(Inst::Load { dest, local: slot }));
                        Ok(Operand::Value(dest))
                    }
                    None => Err(Unlowered::new(
                        "a string literal read as a value with no expected layout to build it \
                         at: every position that owns a `String` has one, so this is a caller \
                         this crate has not met",
                    )),
                },
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
                    return Err(Unlowered::new(format!("{UNTYPED} (local _{})", local.0)));
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
        extra: &mut Vec<ExtBlock>,
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
            // **A drop that has to run nothing is a `br`.** See
            // [`Lowerer::drop_runs_something`] for the predicate and for why
            // `science_codegen::descriptor::needs_drop` is not it. **A drop
            // that has to run something and is unconditional is a call and a
            // `br`; a drop that has to run something and carries a flag is
            // the same call, gated.** [`Lowerer::emit_flagged_drop`] is where
            // the second case is built and why it is a second block rather
            // than an instruction.
            TerminatorKind::Drop { place, flag, target } => {
                let ty = place.ty(body);
                let target_block = BlockId(target.index() as u32);
                if !self.drop_runs_something(ty, 0)? {
                    return Ok(Terminator::Goto(target_block));
                }
                match flag {
                    None => {
                        self.emit_drop_release(ctx, place, ty, insts)?;
                        Ok(Terminator::Goto(target_block))
                    }
                    Some(flag_local) => {
                        self.emit_flagged_drop(ctx, place, ty, *flag_local, target_block, insts, extra)
                    }
                }
            }
        }
    }

    /// The address computation and the release call a `Drop` runs, shared
    /// between the unconditional case — which writes them into the block's
    /// own instructions — and [`Lowerer::emit_flagged_drop`], which writes
    /// them into the block it invents. `ty` is already known to own
    /// something: `lower_terminator`'s `drop_runs_something` guard is what
    /// makes that true before either caller reaches here.
    fn emit_drop_release(
        &mut self,
        ctx: &mut BodyCtx,
        place: &mir::Place,
        ty: Ty,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        // Decision 12's `DropGlue::RuntimeCall`, and it is the one owning
        // type this backend can release without emitting a glue function.
        // `lower`'s own §1 already stated the rule for the temporary a
        // `print` makes — *"a `String` does not need [glue] — the temporary
        // is freed by a direct `science_string_free` at the site that made
        // it"* — and a bound `String` is that same call at the site that
        // drops it. `science_string_free` takes the address and no
        // descriptor; `science_array_free` and `science_box_free` each take
        // one, and `direct_release` is what supplies it — the descriptor is
        // a global, so it is interned here and not passed down from MIR.
        // `science_box_free`'s call is shaped differently again —
        // [`Lowerer::release_args`] is where that is built and why.
        // Everything else that owns something goes through Decision 12's
        // glue, which is emitted on demand.
        let glue = self.intern_drop_glue(ty, 0)?;
        let direct = match &glue {
            Some(_) => None,
            None => self.direct_release(ty)?,
        };
        let (address, _) = self.place_address(ctx, place, insts)?;
        let (callee, ret) = match (&glue, &direct) {
            (Some(symbol), _) => (Callee::Science(symbol.clone()), ReturnClass::Void),
            (None, Some((symbol, _))) => {
                (Callee::Runtime(symbol), self.declare(symbol)?.ret.clone())
            }
            (None, None) => {
                return Err(Unlowered::new(format!(
                    "a drop of `{}`, which owns something this crate releases neither by a \
                     runtime call nor by Decision 12's glue",
                    self.types.render(self.defs, ty)
                )));
            }
        };
        let args = match &direct {
            Some(d) => self.release_args(d, address, &mut || ctx.value(), insts),
            None => vec![Operand::Value(address)],
        };
        insts.push(ExtInst::Above(Inst::Call { dest: None, callee, args, ret, sret_slot: None }));
        Ok(())
    }

    /// Decision 26's flagged drop: *"one byte, set where the value is
    /// initialised, cleared where it is moved, tested at the drop point"*.
    /// The byte is read here; `StatementKind::SetDropFlag` is where it is
    /// written.
    ///
    /// **Three basic blocks where MIR has one**, exactly as
    /// `science-mir`'s §4 item 2 and this file's own module doc both already
    /// say the amendment costs. [`Lowerer::emit_niched_glue`] is the model —
    /// test, call, continue — with one difference: that function invents
    /// every block of a standalone glue function, and this one reuses
    /// `target` as "continue" because `target` already exists as a MIR
    /// block. So only one block is invented, not three: the current block
    /// becomes the test, `ctx.invent_block` gives the call a home, and
    /// `target` is unchanged either way.
    ///
    /// **Why a flag reads as `Operand::Copy`, never `Operand::Move`.**
    /// Reading the flag's own byte does not consume the value it describes;
    /// `TerminatorKind::If`'s condition is lowered the identical way, for the
    /// identical reason, one match arm up.
    fn emit_flagged_drop(
        &mut self,
        ctx: &mut BodyCtx,
        place: &mir::Place,
        ty: Ty,
        flag: mir::Local,
        target: BlockId,
        insts: &mut Vec<ExtInst>,
        extra: &mut Vec<ExtBlock>,
    ) -> Result<Terminator, Unlowered> {
        let flag_operand = mir::Operand::Copy(mir::Place::local(flag));
        let cond = self.lower_operand(ctx, &flag_operand, None, insts)?;
        let mut release: Vec<ExtInst> = Vec::new();
        self.emit_drop_release(ctx, place, ty, &mut release)?;
        let do_drop = ctx.invent_block();
        extra.push(ExtBlock {
            id: do_drop,
            label: format!("bb{}", do_drop.0),
            insts: release,
            terminator: Terminator::Goto(target),
        });
        Ok(Terminator::Branch { cond, then_block: do_drop, else_block: target })
    }

    /// Lower a call.
    ///
    /// Three shapes reach here and the third is §10's stage 2:
    ///
    /// 1. **`print` of a string literal**, which Decision 15 makes three calls
    ///    — build, print, free — where MIR has one terminator.
    /// 2. **`print` of a `String` the program is holding**, which is one call
    ///    and — only when MIR says the call is the value's last owner — a free.
    ///    [`Lowerer::lower_print`] is where that is decided and why.
    /// 3. **A call to an `extern "C"` declaration**, which Decision 40 makes
    ///    *"a direct `call` to the declared symbol — no thunk, no wrapper, no
    ///    trampoline"*.
    #[allow(clippy::too_many_arguments)]
    /// Which **instance** the call terminating the current block calls.
    ///
    /// # The two answers, and why the order between them is this way round
    ///
    /// `science_codegen::mono`'s call map is asked first, because it is the
    /// only source that can distinguish `g[Int]` from `g[F64]` at one call
    /// site: it holds [`Mono::solve_call`]'s answer, which unified the
    /// callee's declared signature against the actual argument types in the
    /// *instantiated* caller. Nothing in this crate can recompute that —
    /// `Substitution` lives in `science-types` and the unification in
    /// `science-codegen` — and a second copy of it here, disagreeing at link
    /// time, is the hazard this codebase closes everywhere else by keeping one
    /// spelling of a rule.
    ///
    /// [`Lowerer::by_def`] answers second, for the call the map did not
    /// record: a body reached only through a vtable (the union in
    /// [`Lowerer::lower_crate`] says why the walk misses those), and any
    /// lowerer built for a stage that runs no walk at all. Every such callee
    /// has exactly one symbol, because a definition with more than one is a
    /// generic and a generic is what the walk *does* record — so the fallback
    /// is never the ambiguous case, which is what makes it safe rather than a
    /// guess.
    ///
    /// `None` means neither knows the callee, and the arm below treats that
    /// as it always did: a runtime entry point, an `extern` declaration, or
    /// the refusal that names what is missing.
    /// The symbol this call site's callee names, if one instance answers for
    /// it unambiguously.
    ///
    /// **`by_def` is never consulted for a method `interface` declares, and
    /// that exclusion is new.** `by_def` maps a raw [`DefId`] to *the last
    /// symbol [`Lowerer::lower_crate`] inserted for it* — one entry, because
    /// every definition used to have exactly one instance. A defaulted
    /// method no longer does: `Summarize.twice[Self=Doc]` and
    /// `Summarize.twice[Self=Row]` share one `DefId` and insert into `by_def`
    /// one after the other, so the second silently overwrites the first and
    /// this fallback would answer *whichever implementor's body happened to
    /// be emitted last* for every call it does not otherwise know how to
    /// name — the exact collapse `science_codegen::mono::Instance::self_ty`
    /// exists to rule out, reintroduced one layer down. `mono.callee_at`
    /// already carries the right answer for every call site the walk
    /// resolved, including one redirected to a concrete `Self`; a call
    /// reaching this function with `mono.callee_at` empty and
    /// [`Lowerer::declaring_interface`] naming an interface is exactly the
    /// vtable dispatch [`Lowerer::lower_dispatch`] exists for, and `None`
    /// here is what sends it there rather than to a wrong direct call.
    fn symbol_for_call(&self, ctx: &BodyCtx, def: DefId) -> Option<String> {
        if let Some(mono) = self.calls {
            if let Some(symbol) = mono.callee_at(&ctx.symbol, ctx.block) {
                return Some(symbol.to_string());
            }
        }
        if self.declaring_interface(def).is_some() {
            return None;
        }
        self.by_def.get(&def).cloned()
    }

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
            // Half of `science-mir`'s own refusal — *"a call through a
            // closure"* — closes here. The other half, a call through a
            // *captured* closure's value, never reaches this arm at all: its
            // `Rvalue::Closure` was refused three functions up, at
            // `Lowerer::lower_rvalue`, so there is no register holding one to
            // read `operand` out of.
            mir::Callee::Indirect(operand) => {
                self.lower_indirect_closure_call(body, ctx, operand, args, destination, insts)?;
                return Ok(match target {
                    Some(block) => Terminator::Goto(BlockId(block.index() as u32)),
                    None => Terminator::Unreachable,
                });
            }
            // **A call MIR makes to `science-rt` directly**, which
            // `science-mir` calls *"the array-op hole kept open deliberately"*
            // and which nothing produced until `f"…"` did. This used to be an
            // outright refusal naming *"a whole-array call"*, which was the
            // only kind anyone expected; a string interpolation is the first
            // one that exists.
            mir::Callee::Runtime(symbol) => {
                self.lower_runtime_call(body, ctx, symbol, args, destination, insts)?;
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
        } else if self.defs.get(def).is_builtin()
            && matches!(self.defs.get(def).name.as_str(), "print" | "write")
        {
            // **One function, not two.** `print` and `write` are
            // `strings-formatting-and-docs.md` §4.2's two names for the same
            // call — stdout, rendered through `Display`, differing only in
            // whether a `\n` follows — and `lower_print` below reads which one
            // it is off `function` rather than being copied for the second
            // name. Two spellings of one lowering is the shape this file's own
            // `undisplayable` and the module-level docs elsewhere name as the
            // recurring defect; this arm is the fix applied to itself.
            let function = self.defs.get(def).name.clone();
            self.lower_print(body, ctx, args, insts, &function)?;
        } else if self.defs.get(def).is_builtin()
            && matches!(self.defs.get(def).name.as_str(), "read_file" | "write_file")
            && self.decls.and_then(|d| d.signature(def)).is_some_and(|s| s.owner.is_none())
        {
            let symbol = match self.defs.get(def).name.as_str() {
                "read_file" => "science_read_file",
                _ => "science_write_file",
            };
            self.lower_runtime_call(body, ctx, symbol, args, destination, insts)?;
        } else if self.defs.get(def).is_builtin()
            && self.defs.get(def).name == "panic"
            && self.decls.and_then(|d| d.signature(def)).is_some_and(|s| s.owner.is_none())
        {
            // **Decision, mirrored from `science-mir`'s `lower_assert`.** `panic`
            // is a free builtin (`recv: None` in `builtins.rs`), so it never
            // reaches [`Lowerer::prelude_method`] — that table is keyed on
            // `(Self, method)` and a free function has no `Self`. It reaches
            // here the same way `read_file`/`write_file` do, one arm up.
            //
            // **Reason the symbol is chosen from the argument and not fixed.**
            // `science-rt` exports two entry points for one `Never`-returning
            // call: `science_panic(*const ScienceString) -> !` for a value,
            // `science_panic_bytes(*const u8, usize) -> !` for a literal —
            // `lower_runtime_call`'s own literal-expansion arm only fires when
            // the parameter list is a pointer followed by a length, and
            // `science_panic`'s one parameter is not. `assert`'s failing branch
            // already makes exactly this choice at MIR-build time because it
            // builds a `Callee::Runtime` directly; an ordinary `panic(msg)` call
            // reaches MIR as `Callee::Def`, so this crate makes the same choice
            // here instead of duplicating a MIR-level special case for one
            // builtin.
            //
            // **Cost.** None beyond the match: both entry points are already in
            // `RUNTIME`, and `RtRet::Never` already builds `Terminator::Unreachable`
            // for a call with no successor (Decision 6, `call` then
            // `unreachable`, never `invoke`) — this arm supplies the callee, not
            // the terminator.
            let symbol = match args.first() {
                Some(mir::Operand::Const(Constant::Literal(Literal::Str(_)))) => "science_panic_bytes",
                _ => "science_panic",
            };
            self.lower_runtime_call(body, ctx, symbol, args, destination, insts)?;
        } else if let Some(sig) = self.symbol_for_call(ctx, def).and_then(|symbol| self.science.get(&symbol).cloned()) {
            self.lower_science_call(ctx, &sig, args, destination, insts)?;
        } else if let Some(symbol) = self.owned_nullable_method(def) {
            self.lower_owned_nullable_call(body, ctx, symbol, args, destination, insts)?;
        } else if let Some(symbol) =
            self.prelude_method(def, args.first().and_then(|op| self.operand_ty(body, op)))
        {
            self.lower_runtime_call(body, ctx, symbol, args, destination, insts)?;
        } else {
            let name = self.defs.get(def).name.clone();
            // **Decision 13's vtable, named as itself.** A method the lookup
            // resolved to a declaration inside an `interface` block, with no
            // body anywhere, is a call through `any I` and nothing else: a
            // receiver whose type is concrete selects the implementation's own
            // method (`science-types`' `methods`'s `head` answers the
            // interface only for a `TyKind::Object`), and an interface method
            // that *has* a default body is refused one layer up, at its
            // signature. So this arm is dynamic dispatch, and saying so is
            // worth more than saying there is no body.
            if self.declaring_interface(def).is_some() {
                self.lower_dispatch(ctx, def, args, destination, insts)?;
                return Ok(match target {
                    Some(block) => Terminator::Goto(BlockId(block.index() as u32)),
                    None => Terminator::Unreachable,
                });
            }
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

    /// Decision 13's dispatch: the call a vtable slot answers.
    ///
    /// **The shape, and every step of it is forced.** A `borrowed any I` is
    /// `{ data, vtable }`; the receiver arrives as a place holding those two
    /// words. The vtable word is loaded, the slot is reached by a byte offset
    /// — `slot * pointer_size`, which is what [`ExtInst::FieldAddr`] emits and
    /// what `tests/abi_claims.rs`'s `an_i8_gep_is_a_byte_offset` pins — the
    /// function pointer is loaded out of it, and the call passes the **data**
    /// word as the receiver. The concrete method's own `self` is a
    /// `borrowed C`, which is that same pointer: an implementation is an
    /// ordinary method and the erasure is entirely in the caller.
    ///
    /// **The slot index is the method's position among the interface's
    /// `DefKind::Fn` children, and it is computed here from the same walk
    /// `Lowerer::vtable_slots` fills the table with.** Two walks of one list
    /// in one file is how the writer and the reader of a slot stay in step;
    /// `science_codegen::descriptor::Vtable`'s own note is why a second
    /// ordering rule anywhere would be a call that lands on the wrong method.
    ///
    /// **What it cannot do, named rather than guessed.** A default body has no
    /// slot — `vtable_slots` refuses the table before this is reached — and a
    /// receiver that is not a place has no address to read two words from.
    fn lower_dispatch(
        &mut self,
        ctx: &mut BodyCtx,
        method: DefId,
        args: &[mir::Operand],
        destination: &mir::Place,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        let name = self.defs.get(method).name.clone();
        let interface = self.defs.get(method).parent.ok_or_else(|| {
            Unlowered::new(format!("a dispatch of `{name}`, which is declared inside nothing"))
        })?;
        let slot = self
            .defs
            .children(interface)
            .filter(|def| def.kind == DefKind::Fn)
            .position(|def| def.id == method)
            .ok_or_else(|| {
                Unlowered::new(format!(
                    "a dispatch of `{name}`, which `any {}`'s own declaration does not list",
                    self.defs.get(interface).name
                ))
            })?;
        let signature = self.dispatch_signature(method)?;

        let Some((receiver, rest)) = args.split_first() else {
            return Err(Unlowered::new(format!(
                "a dispatch of `{name}` with no receiver: the fat pointer is the first argument \
                 and there is nothing here to read a vtable out of"
            )));
        };
        let Some(place) = receiver.place() else {
            return Err(Unlowered::new(format!(
                "a dispatch of `{name}` on a receiver that is not a place: `any {}` is two words \
                 in memory and a constant has no address to read them from",
                self.defs.get(interface).name
            )));
        };

        let fat = layout_of(self.target, &CgTy::Interface);
        let Repr::Aggregate { fields } = &fat.repr else {
            return Err(Unlowered::new(
                "an interface object whose layout is not the two-word aggregate `CgTy::Interface` \
                 lays out",
            ));
        };
        let [data_place, table_place] = fields.as_slice() else {
            return Err(Unlowered::new(format!(
                "an interface object with {} word(s) rather than two",
                fields.len()
            )));
        };
        let (data_place, table_place) = (data_place.clone(), table_place.clone());

        let (base, _) = self.place_address(ctx, place, insts)?;

        let data_address = ctx.value();
        insts.push(ExtInst::FieldAddr {
            dest: data_address,
            base: Operand::Value(base),
            offset: data_place.offset,
        });
        let data = ctx.value();
        insts.push(ExtInst::LoadAt {
            dest: data,
            address: Operand::Value(data_address),
            layout: data_place.layout.clone(),
        });

        let table_address = ctx.value();
        insts.push(ExtInst::FieldAddr {
            dest: table_address,
            base: Operand::Value(base),
            offset: table_place.offset,
        });
        let table = ctx.value();
        insts.push(ExtInst::LoadAt {
            dest: table,
            address: Operand::Value(table_address),
            layout: table_place.layout.clone(),
        });

        let slot_address = ctx.value();
        insts.push(ExtInst::FieldAddr {
            dest: slot_address,
            base: Operand::Value(table),
            offset: slot as u64 * table_place.layout.size,
        });
        let function = ctx.value();
        insts.push(ExtInst::LoadAt {
            dest: function,
            address: Operand::Value(slot_address),
            layout: table_place.layout.clone(),
        });

        // The receiver is the data word; every other argument is materialised
        // at the layout the *interface* declared for it, which is the same
        // layout every implementation was classified at — `intern_vtable`
        // checks that rather than assuming it.
        let mut lowered = vec![Operand::Value(data)];
        let declared: Vec<Layout> =
            signature.params.iter().skip(1).map(|param| param.layout.clone()).collect();
        if rest.len() != declared.len() {
            return Err(Unlowered::new(format!(
                "a dispatch of `{name}` passing {} argument(s) where `any {}` declares {}",
                rest.len(),
                self.defs.get(interface).name,
                declared.len()
            )));
        }
        for (arg, layout) in rest.iter().zip(&declared) {
            lowered.push(self.typed_operand(ctx, arg, layout, insts)?);
        }

        let ret = signature.ret.clone();
        self.emit_result(
            ctx,
            Callee::Indirect { function, signature: Box::new(signature) },
            &ret,
            lowered,
            destination,
            insts,
        )
    }

    /// The ABI signature a dispatch through `any I` calls at: the interface's
    /// own declaration, with the receiver erased to a raw pointer.
    ///
    /// **The receiver is `ptr` and not `Self`.** `Self` is a different
    /// concrete type in every implementation and has no layout here; what the
    /// fat pointer carries is the address of the value, and every
    /// implementation's own `self` is a `borrowed C` — the same pointer,
    /// classified the same way. The rest of the signature is the declaration's
    /// verbatim, because `conform` has already checked that every
    /// implementation matches it at the type level.
    ///
    /// **`symbol` is a description and not a symbol**, which is what
    /// `Callee::Indirect` documents it as: there is no symbol for a call whose
    /// callee is a value, and a message about a wrong argument count needs
    /// something a reader recognises.
    fn dispatch_signature(&self, method: DefId) -> Result<AbiSignature, Unlowered> {
        let name = self.defs.get(method).name.clone();
        let Some(decls) = self.decls else {
            return Err(Unlowered::new(format!(
                "a dispatch of `{name}` with no declaration table to read the interface's \
                 signature from"
            )));
        };
        let Some(signature) = decls.signature(method) else {
            return Err(Unlowered::new(format!(
                "a dispatch of `{name}`, which the declaration table has no signature for"
            )));
        };
        if !signature.generics.is_empty() {
            return Err(Unlowered::new(format!(
                "a dispatch of the generic method `{name}`: a generic method has no single \
                 vtable slot — Decision 13's table is one pointer per method and a generic one \
                 needs one per instantiation, which is monomorphisation's"
            )));
        }
        if signature.self_param.is_none() {
            return Err(Unlowered::new(format!(
                "a dispatch of `{name}`, which takes no receiver: an associated function is \
                 reached through a type and there is no value carrying a vtable to find it with"
            )));
        }
        let interface = self.defs.get(method).parent.map(|p| self.defs.get(p).name.clone());
        let description = match interface {
            Some(interface) => format!("any {interface}::{name}"),
            None => name.clone(),
        };
        let ret_layout = self.layout_of_ty(signature.ret)?;
        let mut params = vec![(
            "self".to_string(),
            layout_of(self.target, &CgTy::Ptr(PtrKind::Borrow)),
            ParamAttrs::default(),
        )];
        for param in &signature.params {
            let param_name = self.defs.get(param.def).name.clone();
            params.push((param_name, self.layout_of_ty(param.ty)?, ParamAttrs::default()));
        }
        Ok(AbiSignature::science(self.target, description, ret_layout, params))
    }

    /// A call through a closure value — `science-mir`'s `Callee::Indirect`.
    ///
    /// **The shape is `dispatch_signature`'s, minus the vtable.** A `borrowed
    /// any I` erases its receiver behind a fat pointer and a slot index; a
    /// closure of nothing captured *is* its function pointer (`cg_ty_in`'s
    /// `TyKind::Closure` arm), so there is no field to load one out of and no
    /// receiver to smuggle through the signature — `operand` reads straight
    /// to the value `Callee::Indirect { function, .. }` calls.
    ///
    /// **What this cannot do, named rather than guessed.** A captured closure
    /// never reaches here: its value was refused where it was built
    /// (`Lowerer::lower_rvalue`'s `Rvalue::Closure` arm), so there is no
    /// register for `operand` to name and this function is never asked about
    /// one.
    fn lower_indirect_closure_call(
        &mut self,
        body: &MirBody,
        ctx: &mut BodyCtx,
        operand: &mir::Operand,
        args: &[mir::Operand],
        destination: &mir::Place,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        let closure_ty = self.operand_ty(body, operand).ok_or_else(|| {
            Unlowered::new(
                "a call through a closure value that is not a place: there is no declared type \
                 to read a signature from",
            )
        })?;
        let (params, ret) = match self.types.kind(closure_ty) {
            TyKind::Closure { params, ret } => (params.clone(), *ret),
            other => {
                return Err(Unlowered::new(format!(
                    "a call through a value of type `{other:?}`, which is not a closure"
                )))
            }
        };
        let signature = self.closure_signature(&params, ret)?;

        let ptr_layout = layout_of(self.target, &CgTy::Ptr(PtrKind::Fn));
        let function = match self.lower_operand(ctx, operand, Some(&ptr_layout), insts)? {
            Operand::Value(value) => value,
            _ => {
                return Err(Unlowered::new(
                    "a call through a closure value that is not a loaded register: every \
                     closure reaches a call site through a place, and this one did not",
                ))
            }
        };

        if args.len() != signature.params.len() {
            return Err(Unlowered::new(format!(
                "a call through a closure declaring {} parameter(s) with {} argument(s)",
                signature.params.len(),
                args.len()
            )));
        }
        let mut lowered = Vec::with_capacity(args.len());
        for (arg, param) in args.iter().zip(&signature.params) {
            lowered.push(self.typed_operand(ctx, arg, &param.layout, insts)?);
        }

        let ret_class = signature.ret.clone();
        self.emit_result(
            ctx,
            Callee::Indirect { function, signature: Box::new(signature) },
            &ret_class,
            lowered,
            destination,
            insts,
        )
    }

    /// The ABI signature a call through a closure value calls at: the
    /// closure's own arrow type, `(A) -> B`, with no receiver.
    ///
    /// **`symbol` is a description and not a symbol**, mirroring
    /// [`Lowerer::dispatch_signature`]'s own note: there is no symbol for a
    /// call whose callee is a value.
    fn closure_signature(&self, params: &[Ty], ret: Ty) -> Result<AbiSignature, Unlowered> {
        let ret_layout = self.layout_of_ty(ret)?;
        let mut lowered_params = Vec::with_capacity(params.len());
        for (index, param) in params.iter().enumerate() {
            lowered_params.push((
                format!("_{index}"),
                self.layout_of_ty(*param)?,
                ParamAttrs::default(),
            ));
        }
        Ok(AbiSignature::science(self.target, "a closure".to_string(), ret_layout, lowered_params))
    }

    /// `print` or `write`, of a literal or of a `String` the program holds.
    ///
    /// **`function` names which one** — `"print"` or `"write"` — and is the
    /// only thing that varies between them: `strings-formatting-and-docs.md`
    /// §4.2 gives both the same stream and the same argument shape, and the
    /// only difference, a trailing `\n`, is `science_print`'s and
    /// `science_write`'s own business and not this function's — it picks the
    /// runtime symbol from `function` and otherwise treats the two calls
    /// identically. A second copy of this function for `write` would be the
    /// defect `runtime_reachability.rs`'s module doc catalogues six instances
    /// of, filed a seventh time in the one function best placed to avoid it.
    ///
    /// **The decision. Who frees the buffer is read off the operand, and the
    /// operand is the only thing that can say.** A `Const` is a `String` this
    /// call site built, so this call site frees it. A `Move` says MIR gave up
    /// ownership at the call, so nothing else will free it and this call site
    /// must. A `Copy` says the frame still owns it, so there is a
    /// `TerminatorKind::Drop` further down that frees it and freeing here would
    /// be the first half of a double free.
    ///
    /// **The reason it is three arms and not two.** It used to be two, and the
    /// third — `Copy` — was an outright refusal saying *"MIR's move analysis
    /// says something else still owns it, and this call site frees what it
    /// prints"*. That sentence described the bug rather than avoiding it: the
    /// free was unconditional, so the only safe operand was a `Move`, so MIR
    /// had to be wrong about `print` for anything to lower at all.
    /// `science-mir`'s `lower` §5 now reads `print`'s (and `write`'s) missing
    /// signature as *"unknown argument passing"* and hands over a `copy`,
    /// which is what `strings-formatting-and-docs.md` §4.1's
    /// `def print(value: borrowed any Display)` means at this level. The
    /// `Move` arm is **kept and is not dead**: an `f"…"` bound to nothing is a
    /// temporary whose last use is the call, and if the front half ever
    /// declares the parameter the same arm covers whatever MIR then emits for a
    /// value that really is consumed.
    ///
    /// **What it costs.** The two arms disagree about one `science_string_free`
    /// and nothing in the emitted IR records which one ran, so a future change
    /// that makes MIR emit `move` where it now emits `copy` is a use-after-free
    /// the verifier cannot see — finding 18's shape. `tests/interpolation.rs`
    /// and `tests/printing.rs` are execution tests for exactly that reason:
    /// they run the program and read what it wrote.
    fn lower_print(
        &mut self,
        body: &MirBody,
        ctx: &mut BodyCtx,
        args: &[mir::Operand],
        insts: &mut Vec<ExtInst>,
        function: &str,
    ) -> Result<(), Unlowered> {
        let symbol = match function {
            "write" => "science_write",
            _ => "science_print",
        };
        let string_layout = layout_of(self.target, &RtAggregate::String.cg_ty());
        // **A `borrowed String` is already the pointer `science_print` (or
        // `science_write`) wants**,
        // and it is the one operand shape that is neither a slot this call site
        // built nor a slot it can take the address of: the local holds the
        // address of somebody else's `ScienceString`, so the value in the slot
        // *is* the argument and there is nothing here to free. `def longest(a:
        // borrowed String, …) -> borrowed String` returns one, which is how it
        // reaches `print`.
        //
        // **The type is checked and not the layout**, for [`Lowerer::is_string`]'s
        // reason one level out: a `borrowed Doc` has the same `Ptr` layout and
        // handing its address to `science_print` reads a record's first three
        // words as a `{ ptr, len, cap }`.
        if let [operand @ (mir::Operand::Move(place) | mir::Operand::Copy(place))] = args {
            let borrowed_string = self
                .operand_ty(body, operand)
                .map(|ty| match self.types.kind(ty) {
                    TyKind::Borrowed { inner, .. } => self.is_string(*inner),
                    _ => false,
                })
                .unwrap_or(false);
            if borrowed_string {
                let (address, layout) = self.place_address(ctx, place, insts)?;
                let pointer = ctx.value();
                insts.push(ExtInst::LoadAt {
                    dest: pointer,
                    address: Operand::Value(address),
                    layout,
                });
                let print = self.declare(symbol)?;
                insts.push(ExtInst::Above(Inst::Call {
                    dest: None,
                    callee: Callee::Runtime(symbol),
                    args: vec![Operand::Value(pointer)],
                    ret: print.ret.clone(),
                    sret_slot: None,
                }));
                return Ok(());
            }
        }
        // `frees` is the decision above, taken here where all three operand
        // shapes are in view rather than at the call that emits it.
        //
        // **The place arm takes any projection, not only a bare local.** It
        // used to require `place.projection.is_empty()`, which is right for
        // `let s be "hola"` and wrong for `print(b.label)`: a field read is a
        // `Place` with one more step than a bare local and nothing else about
        // it — `place_address` already follows `Field`, `TupleField` and
        // `Downcast` for [`Lowerer::lower_print`]'s own `borrowed String` arm a
        // few lines up, and a `String` field is exactly as much a `String` as
        // a `String` local is. The guard that used to be "no projection" is
        // now "the projected layout is a `String`'s", which is the fact that
        // was actually load-bearing — `ctx.layout(local)` only ever answered
        // the *local's* layout, so a field of type `String` inside a `Doc`
        // reached the `is_empty()` guard, failed it, and fell to the same
        // `undisplayable` message a record type gets, naming `String` as
        // though it had no renderer.
        //
        // **`f"{b.label}"` never had this bug, and that asymmetry is what
        // pointed at the fix.** `science-mir`'s `lower_fstring` builds every
        // hole through an `Rvalue::Ref` — it takes a reference to the place
        // first, at whatever depth, and hands `print`'s cousin a `borrowed
        // String` it already knows how to read. `print(b.label)` never goes
        // through that builder at all: `science-types`' `call` passes the
        // operand straight through as `unknown argument passing`, MIR reads
        // print's `borrowed` parameter as a `copy` rather than as a
        // reference, and the place that copy names kept its full projection —
        // which this function then discarded down to "empty or refuse". The
        // fix is not a new capability; it is this function reaching for the
        // same [`Lowerer::place_address`] its own neighbouring arm and the
        // `f"…"` path both already use.
        let (address, frees) = match args {
            [mir::Operand::Const(Constant::Literal(Literal::Str(text)))] => {
                // A slot for the temporary `String`. It is not a MIR local —
                // MIR's `print("…")` has the literal as a constant operand,
                // with no local to hold the `ScienceString` the runtime builds
                // — so codegen invents one, and it goes in the entry block with
                // the rest (Decision 8).
                let slot = self.temp(ctx, string_layout.clone());
                self.build_string(text, slot, insts)?;
                let address = ctx.value();
                insts.push(ExtInst::LocalAddr { dest: address, local: slot });
                (address, true)
            }
            [mir::Operand::Move(place) | mir::Operand::Copy(place)] => {
                let (address, layout) = self.place_address(ctx, place, insts)?;
                if layout != string_layout {
                    return Err(Unlowered::new(self.undisplayable(body, &args[0], function)));
                }
                (address, matches!(args[0], mir::Operand::Move(_)))
            }
            [operand] => return Err(Unlowered::new(self.undisplayable(body, operand, function))),
            _ => {
                return Err(Unlowered::new(format!(
                    "a `{function}` of something other than one value: §4.1 declares \
                     `def {function}(value: &any Display)` and this call has a different \
                     number of arguments",
                )));
            }
        };

        let print = self.declare(symbol)?;
        insts.push(ExtInst::Above(Inst::Call {
            dest: None,
            callee: Callee::Runtime(symbol),
            args: vec![Operand::Value(address)],
            ret: print.ret.clone(),
            sret_slot: None,
        }));

        if frees {
            let free = self.declare("science_string_free")?;
            insts.push(ExtInst::Above(Inst::Call {
                dest: None,
                callee: Callee::Runtime("science_string_free"),
                args: vec![Operand::Value(address)],
                ret: free.ret.clone(),
                sret_slot: None,
            }));
        }
        Ok(())
    }

    /// A `print` or `write` of something the one renderer in this compiler
    /// cannot render, named by its **type**.
    ///
    /// **The refusal used to name the construct and it named the wrong one.**
    /// It read *"a `print` of a value that is not a `String`"*, which is true
    /// of `print(42)` — a program that builds now — and tells the author of
    /// `print(doc)` nothing they did not know. What is missing is a renderer,
    /// the renderer is §1.7's builder, and the builder's coverage is a list of
    /// `science_string_push_*` entry points, so the useful sentence names the
    /// type that is not on it. This is the same repair and the same wording
    /// `mir::Unresolved::Display`'s arm already carries one call over.
    ///
    /// **`function` names which call this is**, so `write(doc)` is refused as
    /// a `write` and not reported under the other function's name.
    fn undisplayable(&self, body: &MirBody, operand: &mir::Operand, function: &str) -> String {
        let named = self.operand_ty(body, operand).map(|ty| self.render_referent(ty));
        match named {
            Some(name) => format!(
                "a `{function}` of a value of type `{name}`: §4.1 declares `{function}` as \
                 `def {function}(value: &any Display)` and the only renderer in this compiler \
                 is §1.7's builder, whose entry points cover `Int`/`I64`, `U64`, `F64`, `F32`, \
                 `Bool`, `Char` and `String`. §3.1's `Formatter` — which is what a user type \
                 would render through — is specified by no note and declared by no prelude, and \
                 this backend emits no vtable to reach one with"
            ),
            None => format!(
                "a `{function}` of a value whose type this crate cannot name: `{function}` \
                 renders through `Display`, the only renderer is §1.7's builder, and its entry \
                 points cover the prelude's scalars and `String`"
            ),
        }
    }

    /// The pointer a runtime parameter wants for a MIR *place* argument:
    /// either the place's own address, or the pointer the place holds.
    ///
    /// **Factored out of [`Lowerer::lower_runtime_call`]'s per-argument loop**,
    /// which drew exactly this distinction inline before
    /// [`Lowerer::lower_owned_nullable_call`] needed to draw it too, at a
    /// receiver and at a key rather than at one generic parameter. A pointer
    /// parameter given a place whose slot is itself a pointer — `mutable
    /// self` on a builtin method, which arrives as the auto-borrow's slot —
    /// takes the pointer *stored there*; a place whose slot holds the value
    /// directly takes the slot's own address. [`Lowerer::lower_runtime_call`]'s
    /// own doc comment states the rule in full; this is that rule with no
    /// change of behaviour, so the two callers cannot answer it differently.
    fn pointer_to_place(
        &mut self,
        ctx: &mut BodyCtx,
        place: &mir::Place,
        insts: &mut Vec<ExtInst>,
    ) -> Result<Operand, Unlowered> {
        let (address, layout) = self.place_address(ctx, place, insts)?;
        if matches!(layout.repr, Repr::Scalar(Scalar::Pointer(_))) {
            let value = ctx.value();
            insts.push(ExtInst::LoadAt { dest: value, address: Operand::Value(address), layout });
            Ok(Operand::Value(value))
        } else {
            Ok(Operand::Value(address))
        }
    }

    /// Spill an operand with no place of its own into a fresh slot at
    /// `layout`, and return the slot's address.
    ///
    /// **Also factored out of [`Lowerer::lower_runtime_call`]**, whose own
    /// comment at the one call site this used to be inline at —
    /// *"a value with no place, into a parameter that wants its address"* —
    /// is the account of why this exists at all, unchanged by moving it here.
    /// [`Lowerer::lower_owned_nullable_call`] reaches it through
    /// [`Lowerer::pointer_to_operand`] for `Map.insert`'s `value: V`, which is
    /// owned and by-value exactly the way `xs.push(30)`'s argument is.
    fn spill_to_pointer(
        &mut self,
        ctx: &mut BodyCtx,
        arg: &mir::Operand,
        layout: &Layout,
        insts: &mut Vec<ExtInst>,
    ) -> Result<Operand, Unlowered> {
        let slot = self.temp(ctx, layout.clone());
        // `field_value` and not `typed_operand`, for the one operand that is
        // not a value yet: a string literal. `m.insert("uno", 1)` moves the key
        // into the call, so the key argument is a `String` that has to be
        // *constructed* — Decision 15's `science_string_from_bytes` — and
        // `field_value` is where that construction already lives, written for a
        // record field and true of every position whose owner is not the
        // literal itself. The call takes ownership, so there is no free here,
        // exactly as a record field has none.
        let value = self.field_value(ctx, arg, layout, insts)?;
        insts.push(ExtInst::Above(Inst::Store { local: slot, value }));
        let address = ctx.value();
        insts.push(ExtInst::LocalAddr { dest: address, local: slot });
        Ok(Operand::Value(address))
    }

    /// [`Lowerer::pointer_to_place`] when the operand has one,
    /// [`Lowerer::spill_to_pointer`] when it does not — the same choice
    /// [`Lowerer::lower_runtime_call`]'s loop makes per parameter, made once
    /// here for a caller that already knows the Science type to spill at
    /// rather than reading it off a descriptor's element.
    fn pointer_to_operand(
        &mut self,
        ctx: &mut BodyCtx,
        arg: &mir::Operand,
        ty: Ty,
        insts: &mut Vec<ExtInst>,
    ) -> Result<Operand, Unlowered> {
        if let Some(place) = arg.place() {
            return self.pointer_to_place(ctx, place, insts);
        }
        let layout = self.layout_of_ty(ty)?;
        self.spill_to_pointer(ctx, arg, &layout, insts)
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
    #[allow(clippy::too_many_arguments)]
    fn lower_runtime_call(
        &mut self,
        body: &MirBody,
        ctx: &mut BodyCtx,
        symbol: &str,
        args: &[mir::Operand],
        destination: &mir::Place,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        let sig = self.declare(symbol)?;
        let entry = runtime_fn(symbol).expect("`declare` found it");

        // **§6's descriptor is inserted here and passed by nobody.** An entry
        // point that takes a `ScienceTypeInfo` takes it at a position that is
        // *"not consistent"* across the table — `RuntimeFn::descriptor_index`
        // is that finding, and it is why the position is asked for rather
        // than assumed — and `science-mir` cannot supply the argument at all:
        // a descriptor is a global this crate interns, and MIR has no operand
        // that names a global. So the caller above the line passes the values
        // and this fills the hole from the element type, which it reads off
        // whichever operand is the array.
        let descriptor_at = entry.descriptor_index();
        // The element type, kept past the descriptor it is interned for: it is
        // also the layout of the slot a by-value element has to be spilled
        // into, which is the argument case below that has no place to take an
        // address of.
        let mut element_ty = None;
        let mut map_kv: Option<(Ty, Ty)> = None;
        // Which of a map entry point's `P` parameters this argument is: the
        // key comes before the value, and `science_map_get`/`contains`/`remove`
        // have only the key.
        let mut map_pointer_args = 0usize;
        let descriptor = match descriptor_at {
            None => None,
            Some(_) => {
                // **A `science_map_*` call's descriptor is a `ScienceMapInfo`
                // and not a `ScienceTypeInfo`**, and the symbol is what says
                // which. `RuntimeFn::descriptor_index` answers *where* the
                // descriptor goes and is silent about *what* it is, because
                // every entry point that takes one took the same kind until
                // `Map` arrived. Keying on the prefix rather than adding a
                // second field to `RuntimeFn` keeps the table describing
                // signatures; the day a third descriptor kind exists, that is
                // the trade to revisit.
                if symbol.starts_with("science_map_") {
                    let (key, value) =
                        self.map_operand_kv(body, args, destination).ok_or_else(|| {
                            Unlowered::new(format!(
                                "a call to `{symbol}`, which takes a `ScienceMapInfo`, with no \
                                 operand this crate can read a key and value type off"
                            ))
                        })?;
                    // Remembered for the same reason `element_ty` is: a `P`
                    // parameter with no place behind it needs a layout for the
                    // slot it is spilled into, and a map's arguments are a key
                    // then a value, in that order, after the map itself.
                    map_kv = Some((key, value));
                    Some(self.intern_map_descriptor(key, value)?)
                } else {
                // `science_box_new` tried second and not merged into
                // `array_operand_element` itself: that function's whole point
                // is *"whichever operand is the array, and the destination is
                // the fallback"*, a search `Box.new` has no operand-shaped
                // half of at all — see `Lowerer::box_operand_element`.
                let element = self
                    .array_operand_element(body, args, destination)
                    .or_else(|| self.box_operand_element(body, destination))
                    .ok_or_else(|| {
                        Unlowered::new(format!(
                            "a call to `{symbol}`, which takes a `ScienceTypeInfo`, with no \
                             operand this crate can read an element type off: the descriptor \
                             names the element and there is nothing here to name"
                        ))
                    })?;
                element_ty = Some(element);
                Some(self.intern_element_descriptor(element)?)
                }
            }
        };

        // The expansion above means the arity is checked against the parameters
        // a literal does *not* cover, so it is counted rather than compared.
        let mut lowered: Vec<Operand> = Vec::with_capacity(sig.params.len());
        let mut param = 0usize;
        for arg in args {
            // The descriptor occupies its own parameter and consumes no
            // argument, so it is emitted when the walk reaches its position
            // and the argument being placed goes after it.
            if descriptor_at == Some(param) {
                if let Some(symbol) = &descriptor {
                    lowered.push(Operand::GlobalAddr(symbol.clone()));
                    param += 1;
                }
            }
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
                    lowered.push(self.pointer_to_place(ctx, place, insts)?);
                    param += 1;
                    continue;
                }
                // **A value with no place, into a parameter that wants its
                // address: spill it.** `xs.push(30)` is the shape —
                // `science_array_push(P, D, P)` takes a pointer to the
                // *element*, and MIR hands over `Operand::Const(30)`, which has
                // no slot anywhere because a constant is not stored until
                // something stores it. Without this the argument reached
                // `typed_operand` against a pointer layout and came out as
                // *"a constant of a type this backend cannot build at ptr"*,
                // which is a true sentence about a program the user wrote
                // correctly.
                //
                // **The slot is the caller's and its layout is the element's.**
                // `science_codegen::abi`'s rule for an indirect argument is a
                // caller-owned slot, and this is that with one difference worth
                // naming: the runtime reads the element and copies it in, so
                // the slot is not moved-from and nothing outlives the call.
                // The layout comes from the element type the descriptor was
                // interned for — the same `T`, by construction, since both are
                // read off the same array — and not from the operand, which
                // carries no type at all.
                let spill_ty = match (element_ty, map_kv) {
                    (Some(element), _) => element,
                    (None, Some((key, value))) => {
                        let ty = if map_pointer_args == 0 { key } else { value };
                        map_pointer_args += 1;
                        ty
                    }
                    (None, None) => {
                        return Err(Unlowered::new(format!(
                            "a value passed by address to `{symbol}`, which is not one of the \
                             entry points this crate reads an element type for: there is nothing \
                             to give the spill slot a layout"
                        )));
                    }
                };
                let layout = self.layout_of_ty(spill_ty)?;
                lowered.push(self.spill_to_pointer(ctx, arg, &layout, insts)?);
                param += 1;
                continue;
            }
            lowered.push(self.typed_operand(ctx, arg, &class.layout, insts)?);
            param += 1;
        }
        // A descriptor in the *last* position — `science_array_free(P, D)`,
        // `science_map_free(P, D)` — is reached after the arguments run out
        // rather than between two of them, which is the other half of
        // `descriptor_index` not being a constant.
        if descriptor_at == Some(param) {
            if let Some(symbol) = &descriptor {
                lowered.push(Operand::GlobalAddr(symbol.clone()));
                param += 1;
            }
        }
        if param != sig.params.len() {
            return Err(Unlowered::new(format!(
                "a call to `{symbol}` that fills {param} of its {} parameter(s)",
                sig.params.len()
            )));
        }
        self.emit_result(ctx, Callee::Runtime(entry.symbol), &sig.ret, lowered, destination, insts)
    }

    /// The interface a method was **declared** on, when that is where the call
    /// resolved to.
    ///
    /// `None` for an inherent method, for a method written inside an
    /// `implements` block, and for anything that is not a method at all —
    /// `Signature::owner` is the *block* a method belongs to, and only an
    /// `interface` block has [`DefKind::Interface`].
    fn declaring_interface(&self, def: DefId) -> Option<String> {
        let owner = self.decls?.signature(def)?.owner?;
        (self.defs.get(owner).kind == DefKind::Interface)
            .then(|| self.defs.get(owner).name.clone())
    }

    /// The `science-rt` entry point a **prelude** method is, if it is one.
    ///
    /// # The decision
    ///
    /// A method the prelude declares and gives no body to is lowered to the
    /// [`RUNTIME`] symbol that implements it, from a closed table written out
    /// here.
    ///
    /// # The reason
    ///
    /// `science-resolve`'s `builtins.rs` declares `String has: def length
    /// (self) -> Int` and stops — there is no Science body anywhere, and there
    /// cannot be one, because the answer is a field of a `ScienceString` and
    /// Science has no way to name it. So the call arrives as a
    /// [`mir::Callee::Def`] with nothing behind it, and the two honest answers
    /// are this table or a refusal. `science-rt` already exports every one of
    /// these under `#[no_mangle]`, `science_codegen::runtime::RUNTIME`
    /// already declares each with a classified signature, and
    /// `tests/symbols.rs` already checks the two against each other — so what
    /// is missing is only the sentence that says which Science name means
    /// which symbol.
    ///
    /// **It is keyed on the block's `Self` and on the name, and both halves are
    /// required.** A user may write `type String:` of their own and give it a
    /// `length`; [`Declarations::self_ty`] of the *block* is what says whether
    /// the receiver is the prelude's, and `is_builtin` on the method is what
    /// says the declaration came from the prelude rather than from a user's
    /// `String has:` extension. Keying on the method name alone is
    /// [`Lowerer::cg_ty`]'s restricted name lookup without `cg_ty`'s excuse.
    ///
    /// # The cost
    ///
    /// **This table is hand-maintained and the one in `RUNTIME` is derived**,
    /// which is §9.2's shape with the two halves one crate apart: a prelude
    /// method added to `builtins.rs` is not added here, and the symptom is a
    /// refusal rather than a wrong answer. `tests/methods.rs` pins each row by
    /// running a program that prints what the entry point returned, so a row
    /// that names the wrong symbol fails on the number and not on the shape.
    ///
    /// Every prelude method not here either has no `RUNTIME` entry point at
    /// all, or needs more than this table can say in one symbol —
    /// `Map.insert`/`Map.remove`'s §5.3 convention is
    /// [`Lowerer::owned_nullable_method`]'s table for that reason. A row is
    /// added when a program that runs it is added with it.
    ///
    /// **Two shapes reach here, and only one of them has a `Self` on
    /// `owner`.** `String has: def length(self) -> Int` is `owner` pointing
    /// at the `impl` block itself, and [`Declarations::self_ty`] answers for
    /// it exactly as the original comment below describes. `Clone.clone` is
    /// the other shape: `String implements Clone:` writes no `clone` of its
    /// own, so `methods`' §2 contributes the *interface's* declaration, and
    /// `Signature::owner` for that contributed method is `Clone` itself —
    /// which has no concrete `Self` to answer `self_ty` with, because an
    /// interface is implemented by every type that names it and not by one.
    /// The concrete type is only at the call, in the receiver argument, so
    /// `receiver_ty` is what this reads instead whenever `owner` is an
    /// interface rather than a block — the same fact
    /// `Lowerer::declaring_interface`'s refusal, one arm down, already
    /// states in its own message: *"the receiver is the only thing that
    /// says which"*.
    ///
    /// Measured: without this, `"hi".clone()` on a concrete `String` refused
    /// with *"a call to the method `clone` through `any Clone`"* — a vtable
    /// message about a program that never wrote a trait object. A call
    /// through an actual `any Clone` still falls through correctly: its
    /// `receiver_ty` is a [`TyKind::Object`], the `TyKind::Named` match below
    /// fails, this returns `None`, and `declaring_interface` reports as
    /// before.
    fn prelude_method(&self, def: DefId, receiver_ty: Option<Ty>) -> Option<&'static str> {
        /// `(the receiver's `Self`, the method) -> the entry point`.
        const PRELUDE_METHODS: &[(&str, &str, &str)] = &[
            ("String", "length", "science_string_len"),
            ("String", "is_empty", "science_string_is_empty"),
            ("String", "new", "science_string_new"),
            ("String", "push_str", "science_string_push_str"),
            ("String", "clone", "science_string_clone"),
            // `truncate` joins on the same terms as the four above it:
            // `science_string_truncate` was already in `RUNTIME`, takes the
            // receiver's address and one `Int`, and returns nothing — so the
            // ordinary `lower_runtime_call` path carries it with no new
            // shape.
            ("String", "truncate", "science_string_truncate"),
            // **`starts_with` is a row because both halves already existed.**
            // `RUNTIME` declares `science_string_starts_with(P, P) -> Bool` and
            // `builtins.rs` declares the method; only the sentence saying which
            // is which was missing, which is this table's stated failure mode —
            // *"a prelude method added to `builtins.rs` is not added here, and
            // the symptom is a refusal rather than a wrong answer"*.
            //
            // **Its four siblings are not rows and cannot be.** The prelude
            // also declares `contains`, `ends_with`, `find`, `replace` and
            // `trim`, and `science-rt` exports an entry point for **none** of
            // them — so those are declared-and-unimplemented, which is a
            // different gap from this one and is not closed by guessing a
            // symbol. The refusal a user meets for them names the method, which
            // is the right report.
            ("String", "starts_with", "science_string_starts_with"),
            ("String", "chars", "science_string_chars"),
            // `Array of T`'s two descriptor-free rows. **Only two**, and the
            // line is `RuntimeFn::descriptor_index`: `science_array_len(P)` and
            // `science_array_is_empty(P)` read a header field, so they take the
            // array and nothing else and are the same shape as `String`'s rows
            // one type up. `push`, `get` and `pop` all take a `D`, which
            // `lower_runtime_call` does supply — but `get` hands back a raw
            // pointer that a `borrowed T?` has to be built out of, and that is
            // §3.4's niche rather than a table row. They are added when a
            // program that runs them is.
            ("Array", "length", "science_array_len"),
            ("Array", "is_empty", "science_array_is_empty"),
            // The one row here that *does* take a descriptor, and it costs
            // nothing extra: `science_array_new(D)` has no array operand, so
            // `array_operand_element` reads `T` off the destination — the same
            // fallback `science_array_with_capacity` already relies on, which
            // is why `[1, 2, 3]` and `(Array of Int).new()` are one path.
            ("Array", "new", "science_array_new"),
            ("Array", "push", "science_array_push"),
            // **`get` is a row and `xs[i]` is not, and the difference is a
            // basic block.** §3.6 declares `get(self, index: Int) -> (borrowed
            // T)?` and `science_array_get` returns a null pointer out of
            // bounds — which is not an error path to be branched on, it *is*
            // the value: §3.4's null niche in the data word, the same
            // representation `Lowerer::lower_c_main` already tests an `Error?`
            // with. So the whole of `get` is one call whose result is stored,
            // and nothing about it needs control flow.
            //
            // `xs[i]` is the other half of §2.4 and is refused: `Index.index`
            // returns a `borrowed T` and not an option, so somewhere between
            // the null and the element there has to be a comparison, a branch
            // and a `science_panic_bytes` — three blocks where this backend's
            // runtime-call lowering builds one, and a place projection is not
            // a statement, so the blocks cannot be made where the index is
            // read. The repair is above this crate: `science-mir` already
            // emits exactly that shape for `assert`, and a `Projection::Index`
            // expanded into statements there would arrive here as a call and a
            // branch this already lowers. That is Decision 42's line, and it
            // is left where it is rather than guessed at.
            ("Array", "get", "science_array_get"),
            // `get_mutably` and **not** `get_mut`, which is the prelude's
            // decision and not this table's: `collections-and-chains.md` §5
            // retires the abbreviation, and `indexing-and-array-literals.md`
            // §1.4 Decision 5 writes the signature out in Science. The runtime
            // twin is `science_array_get_mut`, whose name is C's and is not
            // reached by a user.
            ("Array", "get_mutably", "science_array_get_mut"),
            // `Map of (K, V)`'s surface. **Four rows here and not seven**, and
            // the three that are missing are missing for two different
            // reasons. `insert` and `remove` return `V?` through §5.3's
            // bool-plus-out-parameter convention, which is two results where
            // a row in *this* table maps one call to one symbol — they are
            // wired now, in [`Lowerer::owned_nullable_method`] and
            // [`Lowerer::lower_owned_nullable_call`], a second table for the
            // reason that function's own doc comment gives; `is_empty` has no
            // runtime twin at all — `science_map_len` is the only length
            // there is, and comparing it to zero is an instruction this table
            // cannot express, so it stays missing until that shape exists.
            ("Map", "new", "science_map_new"),
            ("Map", "get", "science_map_get"),
            ("Map", "contains", "science_map_contains"),
            ("Map", "length", "science_map_len"),
            // §2.6's third runtime container. `science_box_new(D, P)` takes a
            // descriptor exactly as `science_array_new(D)` does, and it is
            // recovered the same way: `Box.new`'s one argument is `T` and
            // never `Box of T`, so `lower_runtime_call`'s `array_operand_element`
            // finds nothing and `Lowerer::box_operand_element` — tried next,
            // for that stated reason — reads `T` off the destination instead.
            ("Box", "new", "science_box_new"),
        ];
        if !self.defs.get(def).is_builtin() {
            return None;
        }
        let owner = self.decls?.signature(def)?.owner?;
        let self_ty = if self.defs.get(owner).kind == DefKind::Interface {
            self.referent(receiver_ty?)
        } else {
            self.decls?.self_ty(owner)?
        };
        let TyKind::Named { def: receiver, args } = self.types.kind(self_ty) else {
            return None;
        };
        // **The arguments are ignored and the head is not.** The block that
        // declares `length` on an array is `Array of T has:`, so its `Self` is
        // `Array of T` and never argument-free; keying on the head alone is
        // what lets a generic prelude block have a row at all. It is sound for
        // the rows that are here because each of them reads a header field
        // that is the same field whatever `T` is — which is exactly why they
        // are the rows that take no descriptor. A row whose entry point takes a
        // `D` would need `T` back, and `lower_runtime_call` is where that is
        // recovered, not here.
        let _ = args;
        if !self.defs.get(*receiver).is_builtin() {
            return None;
        }
        let receiver = self.defs.get(*receiver).name.as_str();
        let method = self.defs.get(def).name.as_str();
        PRELUDE_METHODS
            .iter()
            .find(|(ty, name, _)| *ty == receiver && *name == method)
            .map(|(_, _, symbol)| *symbol)
    }

    /// The `science-rt` entry point a method reaches through §5.3's
    /// bool-plus-out-parameter convention, if it is one.
    ///
    /// # The decision
    ///
    /// A second closed table, `(Self, method) -> symbol`, read exactly the
    /// way [`Lowerer::prelude_method`]'s is — by the block's `Self` and the
    /// method's name, both required, `is_builtin` filtered on both — but kept
    /// apart from `PRELUDE_METHODS` rather than added to it, because a row in
    /// that table promises [`Lowerer::lower_runtime_call`]'s one-call,
    /// one-symbol, one-result shape, and this convention keeps none of the
    /// three: `Map.insert` passes two Science arguments where
    /// `science_map_insert` takes five, the fifth is a pointer no MIR operand
    /// names, and the `Bool` that comes back is not the call's result — it is
    /// consumed building the `V?` that is.
    ///
    /// # The reason this is a table and not a rule read off [`RUNTIME`]
    ///
    /// This crate's own doc comment on
    /// [`science_codegen::runtime::OwnedNullableReturn`] asks for the
    /// representation question to be *derived*, and the derived half is
    /// derived: once a call site is known to use this convention,
    /// [`science_codegen::runtime::owned_nullable_return`] answers tag-or-
    /// niche from the payload type alone, and
    /// [`Lowerer::lower_owned_nullable_call`] asks it rather than
    /// re-deciding. What is not derivable from `RUNTIME` is *which* entry
    /// points the convention applies to, and a structural guess — "a `Bool`
    /// return with a trailing pointer parameter" — has two false positives
    /// sitting in the same table as the two true ones. `science_chars_next(P,
    /// P) -> Bool` has that exact shape and is not this convention at all: it
    /// is an iterator's "was there another `Char`", and the pointer it writes
    /// through is a loop variable's out-parameter, written on every call
    /// whether or not the `bool` is `true`. `science_map_contains(P, D, P) ->
    /// Bool` has the same shape read the other way: its trailing pointer is
    /// the *key*, an input the call reads, not an output it writes. Telling
    /// the three apart needs to know whether the trailing pointer is written
    /// unconditionally, read, or written only when the `bool` says so — which
    /// is semantics `RtParam::Pointer` does not carry and `RUNTIME`'s table is
    /// not the place to teach it, because every other parameter already means
    /// "a pointer" without qualification. So the table is hand-maintained,
    /// for [`Lowerer::prelude_method`]'s own reason restated: two honest
    /// answers exist for a call this crate must lower — a table, or a
    /// structural rule that answers `science_chars_next` wrong — and the
    /// table is the smaller mistake.
    ///
    /// # The cost
    ///
    /// Three rows now, not two. `Array.pop` was the convention's other user
    /// left out — `science-resolve`'s `builtins.rs` left `pop` undeclared on
    /// purpose, on the ground that the choice between `pop(mutable self) ->
    /// T?` and `pop(mutable self)` was not this crate's to settle. That note
    /// has since settled it, `T?` is the declared signature, and
    /// `science_array_pop(P, D, P) -> Bool` is §5.3's own shape — the
    /// descriptor at index 1, same as `Map`'s two — so it is a row here and
    /// not a fourth table. `array_operand_element`/`intern_element_descriptor`
    /// build its descriptor in [`Lowerer::lower_owned_nullable_call`], the same
    /// pair [`Lowerer::lower_runtime_call`] already uses for `push` and `get`.
    fn owned_nullable_method(&self, def: DefId) -> Option<&'static str> {
        /// `(the block's `Self`, the method) -> the entry point`.
        const OWNED_NULLABLE_METHODS: &[(&str, &str, &str)] = &[
            ("Map", "insert", "science_map_insert"),
            ("Map", "remove", "science_map_remove"),
            // **`Chars.next`, and it is why the convention was worth building
            // once.** `science_chars_next(iter, out) -> Bool` is the same shape
            // as the two above — its own documentation calls it *"the
            // owned-`T?` convention … §5.3"* — so `for c in text.chars():`
            // needed no machinery of its own, only this row. The `for` lowering
            // above it is Decision 7's narrowing and a back edge, both of which
            // already existed.
            ("Chars", "next", "science_chars_next"),
            // **`Array.pop`, the third.** `science_array_pop(array, D, out) ->
            // Bool` moves the array's last element into `out` and decrements
            // `len`; it does not run the element's drop glue, so nothing is
            // released twice when the `T?` this builds is later dropped —
            // `science_array_pop`'s own doc comment and `science_array_free`'s
            // both say so. On an empty array `out` is never written and the
            // `Bool` is `false`, which is `BranchAndMaterialiseNull`'s or
            // `StoreBoolIntoTag`'s existing empty-case handling and needed
            // nothing new here.
            ("Array", "pop", "science_array_pop"),
        ];
        if !self.defs.get(def).is_builtin() {
            return None;
        }
        let owner = self.decls?.signature(def)?.owner?;
        let self_ty = self.decls?.self_ty(owner)?;
        let TyKind::Named { def: receiver, .. } = self.types.kind(self_ty) else {
            return None;
        };
        if !self.defs.get(*receiver).is_builtin() {
            return None;
        }
        let receiver = self.defs.get(*receiver).name.as_str();
        let method = self.defs.get(def).name.as_str();
        OWNED_NULLABLE_METHODS
            .iter()
            .find(|(ty, name, _)| *ty == receiver && *name == method)
            .map(|(_, _, symbol)| *symbol)
    }

    /// `Map.insert` and `Map.remove`: §5.3's convention, lowered.
    ///
    /// # The shape, restated at the call site
    ///
    /// `science_map_insert(P map, D, P key, P value, P out_old) -> Bool` and
    /// `science_map_remove(P map, D, P key, P out_old) -> Bool` both hand a
    /// `V?` back as a `bool` plus an out-parameter this function invents —
    /// there is no MIR operand for it, because Science's `insert`/`remove`
    /// take two or one arguments where the runtime takes five or four. So
    /// this does not call [`Lowerer::lower_runtime_call`] at all: that
    /// function's arity check and its `emit_result` both assume what this
    /// call breaks.
    ///
    /// # Building the pointer arguments
    ///
    /// The receiver and the key are read the way every other builtin
    /// method's are — [`Lowerer::pointer_to_place`] — and the value (`insert`
    /// only) is Science's owned `V`, spilled into a fresh slot when it has no
    /// place of its own exactly the way `xs.push(30)`'s literal is
    /// ([`Lowerer::pointer_to_operand`]). The descriptor is `Map`'s own
    /// `ScienceMapInfo`, read off the receiver the same way
    /// [`Lowerer::lower_runtime_call`] reads it for `get` and `contains`.
    ///
    /// `Array.pop` takes the other branch of the same `if`: its descriptor is
    /// a plain `ScienceTypeInfo` for the element, built by
    /// [`Lowerer::array_operand_element`] and [`Lowerer::intern_element_descriptor`]
    /// — the same pair `lower_runtime_call` already uses for `push` and `get`
    /// — and not [`Lowerer::map_operand_kv`], which has no `Array` to read a
    /// key and value off. `pop` takes no further Science argument, so the loop
    /// below it runs zero times, exactly as it does for `Chars.next`.
    ///
    /// # Building the out-parameter and consuming the `bool`
    ///
    /// **The destination's own storage is the out-parameter, in both
    /// representations, and nothing is copied into it afterwards.** The
    /// destination is already an `alloca` sized for `V?` — every MIR local
    /// is, since Decision 8 — and in each of Decision 19's two shapes that
    /// `alloca` already contains, or *is*, exactly where the payload belongs:
    ///
    /// - **[`OwnedNullableReturn::StoreBoolIntoTag`]**: the payload sits at
    ///   the tag's `payload_offset`, so [`crate::emit::ExtInst::FieldAddr`]
    ///   off the destination's own address is the out-parameter the runtime
    ///   call is given directly. `science_map_insert` then writes `V` there
    ///   itself on `true` and touches nothing on `false` — which is correct
    ///   either way, because the tag written next is what makes the payload
    ///   meaningful or not. That tag write is
    ///   [`crate::emit::ExtInst::StoreTagFromValue`] of the call's own `Bool`
    ///   result: one store, no branch, for the reason that instruction's own
    ///   doc comment gives.
    /// - **[`OwnedNullableReturn::BranchAndMaterialiseNull`]**: the whole
    ///   destination *is* the payload (Decision 19), so its address is the
    ///   out-parameter with no field to project into. On `true` the call has
    ///   already written the answer where it belongs; on `false` the slot
    ///   holds whatever an uninitialised `alloca` holds, and turning that
    ///   into `null` is [`crate::emit::ExtInst::LoadNiche`] to read the
    ///   (possibly garbage) niche scalar back, [`crate::emit::ExtInst::Select`]
    ///   to choose between it and [`Operand::Null`] on the call's `Bool`, and
    ///   one more store to put the chosen value back — three instructions and
    ///   no branch, which is what [`crate::emit::ExtInst::Select`]'s own doc
    ///   comment argues Decision 5 requires here.
    fn lower_owned_nullable_call(
        &mut self,
        body: &MirBody,
        ctx: &mut BodyCtx,
        symbol: &str,
        args: &[mir::Operand],
        destination: &mir::Place,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        let sig = self.declare(symbol)?;
        let entry = runtime_fn(symbol).expect("`declare` found it");

        // **The receiver, then the descriptor if the entry point wants one,
        // then the rest — in that order, read off `RUNTIME` rather than
        // written out per symbol.**
        //
        // This used to name `Map`'s two symbols and refuse everything else,
        // which was honest while they were the only two. `Chars.next` is the
        // third and it takes **no** descriptor: a `ScienceChars` is a pointer,
        // a length and an offset into somebody else's bytes, so there is no
        // element type to describe. Keying the descriptor on
        // `RuntimeFn::descriptor_index` instead of on the symbol is what lets
        // one function serve both shapes, and it is the same question
        // `lower_runtime_call` already asks.
        let Some(self_place) = args.first().and_then(|arg| arg.place()) else {
            return Err(Unlowered::new(format!(
                "a call to `{symbol}` whose receiver is not a place this crate can take the \
                 address of"
            )));
        };
        let receiver = self.pointer_to_place(ctx, self_place, insts)?;
        let mut lowered = vec![receiver];
        // The declared types of the arguments after the receiver, in order.
        //
        // **Declared and not read off the operand**, because an operand need
        // not have a type at all: `m.insert("uno", 1)` passes a
        // `Constant::Literal`, which carries none, and asking it produced a
        // refusal about a program that was perfectly well typed. A map's key
        // and value come from the descriptor that was just interned for them;
        // anything else falls back to the operand, which is right for every
        // shape that passes a place.
        let mut declared: Vec<Ty> = Vec::new();
        if entry.descriptor_index().is_some() {
            // **`symbol`, not the argument shape, says which descriptor this
            // call needs** — the same split `lower_runtime_call` makes between
            // `science_map_*` and everything else, restated here because this
            // function does not share that one's loop.
            if symbol.starts_with("science_map_") {
                let (key_ty, value_ty) =
                    self.map_operand_kv(body, args, destination).ok_or_else(|| {
                        Unlowered::new(format!(
                            "a call to `{symbol}`, which takes a `ScienceMapInfo`, with no \
                             operand this crate can read a key and value type off"
                        ))
                    })?;
                lowered.push(Operand::GlobalAddr(self.intern_map_descriptor(key_ty, value_ty)?));
                declared.push(key_ty);
                declared.push(value_ty);
            } else {
                let element =
                    self.array_operand_element(body, args, destination).ok_or_else(|| {
                        Unlowered::new(format!(
                            "a call to `{symbol}`, which takes a `ScienceTypeInfo`, with no \
                             operand this crate can read an element type off"
                        ))
                    })?;
                lowered.push(Operand::GlobalAddr(self.intern_element_descriptor(element)?));
                // No `declared` push: `pop` takes no further Science argument,
                // so the loop below never consults this position. A future
                // owned-nullable `Array` method that did would need its
                // parameter type declared here the way `insert`'s value is.
            }
        }
        // Every remaining Science argument is passed by address: `insert`'s key
        // and value, `remove`'s key, and — for `Chars.next`, which takes only
        // `self` — none at all.
        for (position, arg) in args.iter().skip(1).enumerate() {
            let ty = match declared.get(position) {
                Some(ty) => *ty,
                None => self.operand_ty(body, arg).ok_or_else(|| {
                    Unlowered::new(format!(
                        "an argument to `{symbol}` with no type: §5.3's convention passes each \
                         by address and a slot needs a layout"
                    ))
                })?,
            };
            lowered.push(self.pointer_to_operand(ctx, arg, ty, insts)?);
        }

        // **A checked assumption, not a guessed one.** `RUNTIME`'s signature
        // for either symbol always has exactly one more parameter than the
        // arguments built above — the out-parameter this call site invents —
        // and if that ever stops being true, `owned_nullable_method`'s table
        // and `RUNTIME` have drifted apart, and guessing which parameter is
        // missing would silently build the wrong call rather than refuse one.
        if sig.params.len() != lowered.len() + 1 {
            return Err(Unlowered::new(format!(
                "`{symbol}` declares {} parameter(s) and this call built {}: §5.3's convention \
                 expects exactly one more, the out-parameter this call site invents",
                sig.params.len(),
                lowered.len()
            )));
        }

        let dest_local = LocalId(destination.local.index() as u32);
        let dest_layout = ctx.layout(dest_local)?.clone();
        // **The payload's type is the destination's, not an argument's.** It
        // used to be read off the map's value type, which is the same thing for
        // `insert` and `remove` and nothing at all for `Chars.next`, whose
        // `Char?` has no argument to read it from. The destination *is* the
        // `T?` this convention exists to build, so its payload is the one type
        // every call of this shape has.
        let dest_ty = destination.ty(body);
        let TyKind::Nullable(payload_ty) = *self.types.kind(self.referent(dest_ty)) else {
            return Err(Unlowered::new(format!(
                "a call to `{symbol}` whose destination is `{}` rather than a `T?`: §5.3's                  convention builds an option out of a `bool` and an out-parameter, and there is                  nothing else for it to build",
                self.types.render(self.defs, dest_ty)
            )));
        };
        let cg_value = self.cg_ty(payload_ty)?;
        let flag = ctx.value();

        match owned_nullable_return(&cg_value) {
            OwnedNullableReturn::StoreBoolIntoTag => {
                let Repr::Tagged { payload_offset, .. } = &dest_layout.repr else {
                    return Err(Unlowered::new(
                        "a `V?` `owned_nullable_return` classified tagged, whose destination's \
                         own layout is Decision 19's niched one instead: the layout engine and \
                         this convention have disagreed about the same type",
                    ));
                };
                let dest_addr = ctx.value();
                insts.push(ExtInst::LocalAddr { dest: dest_addr, local: dest_local });
                let payload_addr = ctx.value();
                insts.push(ExtInst::FieldAddr {
                    dest: payload_addr,
                    base: Operand::Value(dest_addr),
                    offset: *payload_offset,
                });
                lowered.push(Operand::Value(payload_addr));
                insts.push(ExtInst::Above(Inst::Call {
                    dest: Some(flag),
                    callee: Callee::Runtime(entry.symbol),
                    args: lowered,
                    ret: sig.ret.clone(),
                    sret_slot: None,
                }));
                insts.push(ExtInst::StoreTagFromValue {
                    local: dest_local,
                    value: Operand::Value(flag),
                });
            }
            OwnedNullableReturn::BranchAndMaterialiseNull => {
                let dest_addr = ctx.value();
                insts.push(ExtInst::LocalAddr { dest: dest_addr, local: dest_local });
                lowered.push(Operand::Value(dest_addr));
                insts.push(ExtInst::Above(Inst::Call {
                    dest: Some(flag),
                    callee: Callee::Runtime(entry.symbol),
                    args: lowered,
                    ret: sig.ret.clone(),
                    sret_slot: None,
                }));
                let raw = ctx.value();
                insts.push(ExtInst::LoadNiche { dest: raw, local: dest_local });
                // The niche's own scalar type, and not `V`'s whole layout:
                // `ExtInst::LoadNiche` reads one pointer-sized word regardless
                // of how wide `V` is (`any I` is two), and `Operand::Null`
                // materialises as one opaque `ptr` (`crate::emit`'s
                // `operand`), so `Select`'s two operands are always
                // pointer-sized even when `V` is not. Every niche this crate
                // lays out is a pointer at offset 0 (`CgTy::niche`'s doc: "the
                // four never-null pointer kinds and `any I`"), and `scalar_ty`
                // erases `PtrKind` to one opaque type, so `Ptr(Raw)` stands in
                // for whichever kind `V`'s actual niche is without claiming to
                // know which.
                let niche_layout = layout_of(self.target, &CgTy::Ptr(PtrKind::Raw));
                let chosen = ctx.value();
                insts.push(ExtInst::Select {
                    dest: chosen,
                    cond: Operand::Value(flag),
                    if_true: Operand::Value(raw),
                    if_false: Operand::Null,
                    layout: niche_layout,
                });
                insts.push(ExtInst::Above(Inst::Store {
                    local: dest_local,
                    value: Operand::Value(chosen),
                }));
            }
        }
        Ok(())
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
    ///
    /// **`Operand::Const` is not a copy and gets the repair, not the refusal.**
    /// `Doc.new("scratch")` and `defs.alloc(kind, "main", null)` are both
    /// ordinary programs — a string literal into a `String` parameter, `null`
    /// into a `DefId?` one — and `science-mir`'s argument lowering hands each
    /// straight over as a constant, because it has no place to read for a
    /// literal and no reason to invent one. A constant is not "something
    /// else['s]" the way a `Copy` place is: nothing owns it yet, so there is no
    /// second owner to hand the callee a writable alias of. The fix is the
    /// scratch slot [`Lowerer::emit_result_maybe_aliased`] already uses for the
    /// *other* Decision 22 collision — allocate a caller-owned temporary,
    /// materialise the constant into it exactly as an ordinary `let` binding
    /// would ([`Lowerer::store_null`], [`Lowerer::build_string`]), and pass its
    /// address. The callee then owns a slot that never had a second reader.
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
        let mut aliases_destination = false;
        let mut lowered: Vec<Operand> = Vec::with_capacity(args.len());
        for (arg, param) in args.iter().zip(&sig.params) {
            match param.class {
                ArgClass::Ignore => {}
                ArgClass::Direct => {
                    lowered.push(self.typed_operand(ctx, arg, &param.layout, insts)?);
                }
                ArgClass::IndirectByPointer => {
                    // **A move out of a field passes the field's own address.**
                    // Decision 22 wants a caller-owned slot the callee may write
                    // through, and a field of a local the caller still owns is
                    // one: the move is what says nothing else reads it again,
                    // and `science-mir`'s per-field move tracking is what makes
                    // that true of the drop as well — the caller drops the
                    // fields it did not give away and not this one. Inventing a
                    // slot and copying the field into it would be a second
                    // owner of the same bytes for as long as the call lasts,
                    // which is the shape `Operand::Copy` is refused for below.
                    //
                    // `take(o.inner.a)` was refused outright before this, and a
                    // field of a field is an ordinary thing to pass.
                    if let mir::Operand::Move(place) = arg {
                        if !place.projection.is_empty() {
                            let local = LocalId(place.local.index() as u32);
                            if ctx.untyped.contains(&local) {
                                return Err(Unlowered::new(format!(
                                    "{UNTYPED} (local _{})",
                                    local.0
                                )));
                            }
                            // The callee writes through this pointer, so the
                            // result may not land in the same memory — the same
                            // collision `emit_result_maybe_aliased` documents,
                            // asked of a projected argument.
                            if place.local == destination.local {
                                aliases_destination = true;
                            }
                            let (address, _) = self.place_address(ctx, place, insts)?;
                            lowered.push(Operand::Value(address));
                            continue;
                        }
                    }
                    let local = match arg {
                        mir::Operand::Move(place) => {
                            let local = LocalId(place.local.index() as u32);
                            if ctx.untyped.contains(&local) {
                                return Err(Unlowered::new(format!(
                                    "{UNTYPED} (local _{})",
                                    local.0
                                )));
                            }
                            ctx.layout(local)?;
                            // **The one place a by-pointer argument and the
                            // result can be the same memory.** `s be wrap(s)`
                            // moves `s` into the call by address and asks for
                            // the result through `sret` into the same slot, so
                            // the callee builds its answer on top of the
                            // argument it is still reading. It printed the
                            // empty string and exited 0.
                            if place.local == destination.local
                                && destination.projection.is_empty()
                            {
                                aliases_destination = true;
                            }
                            local
                        }
                        // A constant has no owner to alias — see this
                        // function's doc comment. Give it one: an invented
                        // slot nothing else ever reads, filled the same way an
                        // ordinary `let` binding of the same constant would be.
                        mir::Operand::Const(constant) => {
                            let slot = self.temp(ctx, param.layout.clone());
                            match constant {
                                Constant::Literal(Literal::Null) => {
                                    self.store_null(slot, &param.layout, insts)?;
                                }
                                Constant::Literal(Literal::Str(text)) => {
                                    self.build_string(text, slot, insts)?;
                                }
                                Constant::Unit => {}
                                _ => {
                                    return Err(Unlowered::new(format!(
                                        "a constant argument to `{}` with no runtime \
                                         representation to put in a slot: only `null` and a \
                                         string literal have one",
                                        sig.symbol
                                    )));
                                }
                            }
                            slot
                        }
                        mir::Operand::Copy(_) => {
                            return Err(Unlowered::new(format!(
                                "an aggregate argument to `{}` that is not a move: Decision 22 \
                                 passes a pointer to the caller's own slot and the callee may \
                                 write through it, so a copy would hand it a slot something else \
                                 still owns",
                                sig.symbol
                            )));
                        }
                    };
                    let address = ctx.value();
                    insts.push(ExtInst::LocalAddr { dest: address, local });
                    lowered.push(Operand::Value(address));
                }
            }
        }
        self.emit_result_maybe_aliased(
            ctx,
            Callee::Science(sig.symbol.clone()),
            &sig.ret,
            lowered,
            destination,
            aliases_destination,
            insts,
        )
    }

    /// The call instruction and where its result lands, shared by the Science
    /// and the foreign paths.
    ///
    /// The two used to have a copy of this each, and the copies were the same
    /// four lines with the same `sret` question asked twice — which is §9.2's
    /// shape exactly, so there is one of them.
    /// [`Lowerer::emit_result`], with the one case where the result's slot and
    /// an argument's are the same memory.
    ///
    /// # The decision
    ///
    /// When the destination is also a by-pointer argument, the call returns
    /// into a slot this invents and the value is copied over afterwards.
    ///
    /// # The reason
    ///
    /// Decision 22 passes an aggregate argument as *"a pointer to a
    /// caller-owned slot"*, and the return of an aggregate is `sret` into
    /// another. `s be wrap(s)` makes those the same slot, so a callee that
    /// reads its argument to build its answer — which is what a function
    /// taking a `String` and returning one usually does — writes over the
    /// bytes it has not finished reading. The program printed the empty
    /// string, exited 0, and said nothing. It is the worst shape a defect can
    /// have, and `s be transform(s)` is an ordinary thing to write.
    ///
    /// Neither end can be dropped: the argument must be the caller's slot
    /// because the callee may write through it, and the return must be `sret`
    /// because the value does not fit in registers. What can go is their being
    /// the *same* slot.
    ///
    /// # The cost
    ///
    /// One slot and one copy, for calls where the destination is also an
    /// argument and nowhere else. `mem2reg` removes neither, because the slot
    /// escapes into the call — so this is a real copy of an aggregate, paid
    /// only by the shape that was silently wrong before.
    #[allow(clippy::too_many_arguments)]
    fn emit_result_maybe_aliased(
        &mut self,
        ctx: &mut BodyCtx,
        callee: Callee,
        ret: &ReturnClass,
        args: Vec<Operand>,
        destination: &mir::Place,
        aliases_destination: bool,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        if !aliases_destination || !ret.is_sret() {
            return self.emit_result(ctx, callee, ret, args, destination, insts);
        }
        let dest_local = LocalId(destination.local.index() as u32);
        let layout = ctx.layout(dest_local)?.clone();
        let scratch = self.temp(ctx, layout.clone());
        insts.push(ExtInst::Above(Inst::Call {
            dest: None,
            callee,
            args,
            ret: ret.clone(),
            sret_slot: Some(scratch),
        }));
        let value = ctx.value();
        insts.push(ExtInst::Above(Inst::Load { dest: value, local: scratch }));
        insts.push(ExtInst::Above(Inst::Store { local: dest_local, value: Operand::Value(value) }));
        Ok(())
    }

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
        // **An entry point that cannot fail is rows 1 to 3 and no branch.**
        // §2.3's table is written about the *script body*, whose signature that
        // note fixes at `def main() -> Error?`; a file that writes `def main():`
        // out by hand returns `()`, `sciencec check` accepts it, and the core
        // spec's §11 — as §2.3 itself records — *"never says what signatures
        // `main` may have"*. So there is no error to test, the failing row is
        // unreachable by construction rather than by analysis, and the C `main`
        // is the call and `science_exit(0)`.
        //
        // **`science_exit(0)` and not `ret i32 0`**, for the same reason the
        // `ok` block below gives: the process's last `write` may be sitting in
        // `science-rt`'s own `LineWriter`, which the C runtime's exit does not
        // know about.
        if matches!(science_main.ret, ReturnClass::Void) {
            let exit = self.declare("science_exit")?;
            let entry = ExtBlock {
                id: BlockId(0),
                label: "entry".to_string(),
                insts: vec![
                    ExtInst::Above(Inst::Call {
                        dest: None,
                        callee: Callee::Science(science_main.symbol.clone()),
                        args: vec![],
                        ret: science_main.ret.clone(),
                        sret_slot: None,
                    }),
                    ExtInst::Above(Inst::Call {
                        dest: None,
                        callee: Callee::Runtime("science_exit"),
                        args: vec![Operand::ConstInt(0)],
                        ret: exit.ret.clone(),
                        sret_slot: None,
                    }),
                ],
                terminator: Terminator::Unreachable,
            };
            return Ok((sig, ExtBody { blocks: vec![entry] }));
        }
        // `Indirect` and `Direct` are what is left once `Void` has already
        // returned above — `classify_return_for`'s three functions are
        // exhaustive over `ReturnClass` and this is the boundary between the
        // two remaining cases, not a third one.
        let slot = LocalId(0);
        let data = ValueId(0);
        let is_null = ValueId(1);
        let direct = ValueId(2);
        let message = self.intern_literal(ERROR_MESSAGE);
        let write_error = self.declare("science_write_error_bytes")?;
        let exit = self.declare("science_exit")?;

        let mut entry_insts = vec![ExtInst::Above(Inst::Alloca {
            local: slot,
            layout: science_main.ret_layout.clone(),
        })];
        if science_main.ret.is_sret() {
            entry_insts.push(ExtInst::Above(Inst::Call {
                dest: None,
                callee: Callee::Science(science_main.symbol.clone()),
                args: vec![],
                ret: science_main.ret.clone(),
                sret_slot: Some(slot),
            }));
        } else {
            // **Windows is the one convention where `Error?` is always
            // `Indirect`, and hardcoding that shape here — as this function
            // used to, refusing everything else — is a bug the other two
            // hosts this crate targets would have found the first time either
            // one ran this function.** SysV and AAPCS64 return a two-word fat
            // pointer this small in registers, not through a hidden pointer;
            // `emit_result`'s ordinary call path already handles a `Direct`
            // return generically by capturing the callee's result as one
            // (possibly aggregate) SSA value, and this does the same, then
            // stores it into `slot` so the niche read below sees the same
            // bytes in the same place regardless of which convention put them
            // there.
            entry_insts.push(ExtInst::Above(Inst::Call {
                dest: Some(direct),
                callee: Callee::Science(science_main.symbol.clone()),
                args: vec![],
                ret: science_main.ret.clone(),
                sret_slot: None,
            }));
            entry_insts.push(ExtInst::Above(Inst::Store {
                local: slot,
                value: Operand::Value(direct),
            }));
        }
        // §3.4: the data pointer and **only** the data pointer. The vtable
        // word of a null `(any Error)?` is undefined, and it is at offset 8,
        // so this load must not widen — which is what `Inst::Load` does,
        // because it loads the local's whole type. `ExtInst::LoadNiche` is the
        // narrow one and its documentation is the account.
        entry_insts.push(ExtInst::LoadNiche { dest: data, local: slot });
        entry_insts.push(ExtInst::Above(Inst::Cmp {
            dest: is_null,
            op: science_codegen::backend::CmpOp::Eq,
            signed: false,
            lhs: Operand::Value(data),
            rhs: Operand::Null,
        }));

        let entry = ExtBlock {
            id: BlockId(0),
            label: "entry".to_string(),
            insts: entry_insts,
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
    /// The mangled symbol of the **instance** being lowered, and the block the
    /// walk is in.
    ///
    /// **Together they are the key of `science_codegen::mono`'s call map**,
    /// which is the only thing that can say which instance a
    /// [`mir::Callee::Def`] calls: after monomorphisation one definition is
    /// several functions, and the same call site in `f[T]` calls `g[Int]` from
    /// `f[Int]` and `g[F64]` from `f[F64]` out of one block of one MIR body.
    ///
    /// On the context rather than threaded through
    /// [`Lowerer::lower_terminator`] and [`Lowerer::lower_call`] as two more
    /// parameters, because every other per-body fact the call site needs is
    /// already here and a second channel for the same kind of fact is the
    /// thing that gets out of step.
    symbol: String,
    block: mir::BlockId,
    next_value: u32,
    next_temp: u32,
    /// The next address a block Decision 26's flagged drop invents may use.
    ///
    /// Mirrors `next_temp`'s reasoning one field up: MIR's own block indices
    /// run `0..body.block_count()`, so seeding this counter at that count and
    /// only ever incrementing it is what keeps an invented block's address
    /// from colliding with a real one, the same way an invented local's does.
    next_block: u32,
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

    /// A fresh block address, for the one block Decision 26's flagged drop
    /// invents. `lower_body`'s §Decision-5 note is the reason there is ever a
    /// second block for one MIR terminator.
    fn invent_block(&mut self) -> BlockId {
        let id = BlockId(self.next_block);
        self.next_block += 1;
        id
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
