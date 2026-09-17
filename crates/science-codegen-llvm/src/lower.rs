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
//! names the construct, so that a user who writes a `match` is told "a `match`"
//! and not "internal error".
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
//! outright (`StatementKind::SetDropFlag` is `SC0400`), so this crate has not
//! met the contradiction yet and the amendment that note asks for is still owed.
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
    AbiParam, AbiSignature, ArgClass, ParamAttrs, classify_extern_argument,
    classify_extern_return,
};
use science_codegen::backend::{
    BlockId, Callee, CmpOp, Inst, IntOp, FloatOp, LocalId, Operand, Terminator, ValueId,
};
use science_codegen::descriptor::StringLiteral;
use science_codegen::diagnostics::backend_not_compiled_in;
use science_codegen::layout::{CgTy, IntTy, Layout, PtrKind, Repr, Scalar, Triple, layout_of};
use science_codegen::mangle::{MonoKey, mangle};
use science_codegen::runtime::{RUNTIME, RtAggregate, RtParam, RtRet, RuntimeFn, runtime_fn};
use science_diagnostics::{Diagnostic, Span};
use science_mir::mir::{self, Body as MirBody, Constant, Rvalue, StatementKind, TerminatorKind};
use science_parser::ast::{BinaryOp, UnaryOp};
use science_resolve::hir::{self, DefId, DefKind, DefTable, Literal};
use science_types::Types;
use science_types::items::Declarations;
use science_types::ty::{Ty, TyKind};

use crate::emit::{ExtBlock, ExtBody, ExtInst};

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
                       diagnostic was reported for it. The two this compiler has met are a call \
                       to `print` or `write`, whose signatures `science-resolve`'s `builtins.rs` \
                       withdrew, and a `for` loop's range and iterator temporaries";

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
    pub fn to_diagnostic(&self) -> Diagnostic {
        backend_not_compiled_in(
            &self.construct,
            "this `sciencec` implements §10's stages 0 to 3 of `codegen-and-linking.md` — a \
             script body, string literals and `print`, `extern \"C\"` declarations and the calls \
             to them, the CFG, and scalar arithmetic — and refuses everything else rather than \
             lowering a construct no execution test has ever run",
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
        match self.types.kind(ty) {
            TyKind::Unit => Ok(CgTy::Unit),
            TyKind::Nullable(inner) => Ok(CgTy::nullable(self.cg_ty(*inner)?)),
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
            other => Err(Unlowered::new(format!("a value of type `{other:?}`"))),
        }
    }

    fn layout_of_ty(&self, ty: science_types::ty::Ty) -> Result<Layout, Unlowered> {
        Ok(layout_of(self.target, &self.cg_ty(ty)?))
    }

    /// Lower one crate's MIR.
    ///
    /// `bodies` is every MIR body in the crate. Stage 1 has exactly one and it
    /// is `main`; anything else is refused, because a second function means a
    /// call to it and a call to it means the parameter operand
    /// `science_codegen::backend::Operand` does not have (see
    /// `crate::emit`'s `BodyState::params`).
    pub fn lower_crate(&mut self, bodies: &[MirBody]) -> Result<Lowered, Unlowered> {
        let entry = bodies
            .iter()
            .find(|body| self.defs.get(body.def()).name == "main")
            .ok_or_else(|| Unlowered::new("a program with no `main`"))?;
        if bodies.len() > 1 {
            let other = bodies
                .iter()
                .find(|body| self.defs.get(body.def()).name != "main")
                .map(|body| self.defs.get(body.def()).name.clone())
                .unwrap_or_default();
            return Err(Unlowered::new(format!("a second function, `{other}`")));
        }

        let science_main = self.lower_body(entry)?;
        let c_main = self.lower_c_main(&science_main.0)?;
        Ok(self.finish(vec![science_main, c_main]))
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
        if body.params().next().is_some() {
            return Err(Unlowered::new("a function with parameters"));
        }
        let return_local = mir::Local::from_index(0);
        let return_id = LocalId(return_local.index() as u32);
        let ret_layout = self.layout_of_ty(body.local_decl(return_local).ty)?;
        let key = MonoKey::plain(&["main"]);
        let sig = AbiSignature::science(self.target, mangle(&key), ret_layout.clone(), vec![]);

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
        // place, however late in the walk the slot was invented.
        let entry = ctx.entry;
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
            StatementKind::Activate(_) => Err(Unlowered::new("a two-phase borrow")),
            StatementKind::Nop => Ok(()),
            StatementKind::Assign { place, rvalue } => {
                if !place.projection.is_empty() {
                    return Err(Unlowered::new("an assignment through a field or an index"));
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
                let value = self.lower_operand(body, ctx, operand, Some(layout), insts)?;
                insts.push(ExtInst::Above(Inst::Store { local: dest, value }));
                Ok(())
            }
            Rvalue::Unary { op, operand } => {
                let operand_ty = self.operand_ty(body, operand).unwrap_or(ty);
                let scalar = self.scalar_of(operand_ty)?;
                let operand_layout = layout_of(self.target, &self.cg_ty(operand_ty)?);
                let value = self.typed_operand(body, ctx, operand, &operand_layout, insts)?;
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
            other => Err(Unlowered::new(describe_rvalue(other))),
        }
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
        let left = self.typed_operand(body, ctx, lhs, &operand_layout, insts)?;
        let right = self.typed_operand(body, ctx, rhs, &operand_layout, insts)?;
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
    fn operand_ty(&self, body: &MirBody, operand: &mir::Operand) -> Option<Ty> {
        let place = operand.place()?;
        if !place.projection.is_empty() {
            return None;
        }
        Some(body.local_decl(place.local).ty)
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
        body: &MirBody,
        ctx: &mut BodyCtx,
        operand: &mir::Operand,
        layout: &Layout,
        insts: &mut Vec<ExtInst>,
    ) -> Result<Operand, Unlowered> {
        let lowered = self.lower_operand(body, ctx, operand, Some(layout), insts)?;
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
        body: &MirBody,
        ctx: &mut BodyCtx,
        operand: &mir::Operand,
        expected: Option<&Layout>,
        insts: &mut Vec<ExtInst>,
    ) -> Result<Operand, Unlowered> {
        let _ = body;
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
            mir::Operand::Const(Constant::Item(def)) => {
                Err(Unlowered::new(format!("`{}` named as a value", self.defs.get(*def).name)))
            }
            mir::Operand::Copy(place) | mir::Operand::Move(place) => {
                if !place.projection.is_empty() {
                    return Err(Unlowered::new("a read through a field or an index"));
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
            Repr::Tagged { .. } => Err(Unlowered::new(
                "a `null` of a tagged nullable, which needs a store to the tag byte and the \
                 instruction set has no field projection",
            )),
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
            TerminatorKind::Return => Ok(Terminator::Return(None)),
            TerminatorKind::Goto { target } => Ok(Terminator::Goto(BlockId(target.index() as u32))),
            TerminatorKind::Unreachable => Ok(Terminator::Unreachable),
            TerminatorKind::Call { callee, args, destination, target } => {
                self.lower_call(body, ctx, callee, args, destination, *target, insts)
            }
            // Decision 5 holds: one MIR block, one LLVM block, and an `If`
            // becomes the `br` that ends it. The condition is a `Bool`, so it
            // arrives as §3.1's `i8` memory form and `crate::emit` narrows it.
            TerminatorKind::If { cond, then_block, else_block } => {
                let value = self.lower_operand(body, ctx, cond, None, insts)?;
                Ok(Terminator::Branch {
                    cond: value,
                    then_block: BlockId(then_block.index() as u32),
                    else_block: BlockId(else_block.index() as u32),
                })
            }
            TerminatorKind::Switch { .. } => Err(Unlowered::new(
                "a `match`, which needs §3.3's tagged layout and a `Ty -> CgTy` lowering for a \
                 `choice`",
            )),
            TerminatorKind::Drop { .. } => Err(Unlowered::new(
                "a drop of a named value, which needs Decision 12's drop glue",
            )),
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
            mir::Callee::Runtime(symbol) => {
                return Err(Unlowered::new(format!("a whole-array call to `{symbol}`")));
            }
            mir::Callee::Unresolved(unresolved) => {
                return Err(Unlowered::new(describe_unresolved(*unresolved)));
            }
        };
        if self.defs.get(def).kind == DefKind::ExternFn {
            self.lower_foreign_call(body, ctx, def, args, destination, insts)?;
        } else {
            let name = self.defs.get(def).name.clone();
            if !self.defs.get(def).is_builtin() || name != "print" {
                return Err(Unlowered::new(format!("a call to `{name}`")));
            }
            self.lower_print(ctx, args, insts)?;
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
        body: &MirBody,
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
            lowered.push(self.lower_operand(body, ctx, arg, Some(&param.layout), insts)?);
        }

        if !destination.projection.is_empty() {
            return Err(Unlowered::new("a foreign call whose result goes through a field"));
        }
        let dest_local = LocalId(destination.local.index() as u32);
        // A `()`-returning foreign function writes nowhere, and its MIR
        // destination is a unit temporary with no slot of its own.
        let has_slot = ctx.layouts.contains_key(&dest_local.0);
        if sig.ret.is_sret() {
            if !has_slot {
                return Err(Unlowered::new(
                    "a foreign call returning an aggregate into a destination with no slot",
                ));
            }
            insts.push(ExtInst::Above(Inst::Call {
                dest: None,
                callee: Callee::Foreign(sig.symbol.clone()),
                args: lowered,
                ret: sig.ret.clone(),
                sret_slot: Some(dest_local),
            }));
            return Ok(());
        }
        let value = ctx.value();
        insts.push(ExtInst::Above(Inst::Call {
            dest: Some(value),
            callee: Callee::Foreign(sig.symbol.clone()),
            args: lowered,
            ret: sig.ret.clone(),
            sret_slot: None,
        }));
        if has_slot && !matches!(sig.ret, science_codegen::abi::ReturnClass::Void) {
            insts.push(ExtInst::Above(Inst::Store {
                local: dest_local,
                value: Operand::Value(value),
            }));
        }
        Ok(())
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
        Rvalue::Error => "an expression the front end could not check",
    }
}
