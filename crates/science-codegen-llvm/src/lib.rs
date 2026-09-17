//! `science-codegen-llvm` — the half of the code generator below Decision 42's
//! line, and the first thing in this repository that produces a program.
//!
//! # 0. What this crate is, and what it makes true
//!
//! `science-codegen` is the half **above** the line: layout, the C ABI,
//! mangling, descriptors, the runtime boundary, monomorphisation. Its own
//! documentation states the cost of stopping there — *"no Science program has
//! been compiled. Nothing here has emitted an object file, driven a linker or
//! run"* — and names the missing crate, this one.
//!
//! With `--features llvm` on a machine with LLVM 18.1,
//! `sciencec build hello.science` now produces an executable that prints
//! `hello, world` and exits 0. That is §10's stage 1 and its gate.
//! `self-hosting.md` §1.2's sentence — *"Science cannot produce an executable"* —
//! is the thing this removes, for one program.
//!
//! **And the exit status is now the language's, not the platform's.** The
//! emitted `main` implements `script-mode.md` §2.3's table: a null `Error?` is
//! `science_exit(0)`, a non-null one is a message on stderr and
//! `science_exit(1)`. It used to be `science_panic_bytes` and an abort on the
//! failing edge, because §9.3's finding 5 was that `science-rt` had no stderr
//! writer that returned and no chosen exit status; it has both now.
//! [`lower`]'s own documentation is the account, including **what the failing
//! edge prints and how far short of §2.3 that falls** — the note asks for the
//! error's `Display`, the prelude declares `Display` with no method, and a
//! placeholder that says so is what a user gets.
//!
//! **Stages 2 and 3 are now emitted too.** An `extern "C"` declaration becomes
//! an LLVM `declare`, its `library` clause becomes a link decision, and a call
//! to it is Decision 40's direct `call`: `cos(0.0)` through `library "m"`
//! builds, links and returns `1.0`, which is §10's stage 2 gate, and a
//! misspelled symbol is `SC0461` against its declaration rather than the
//! linker's raw text. **The five notes resting on *"a declaration becomes an
//! LLVM `declare` and the system linker resolves it"* have been tested by
//! execution, and the proposition holds.** Stage 3's CFG is reachable from
//! source: `loop:`, `break`, `if`/`else`, and §4.6's operators on scalars at
//! their own width and signedness.
//!
//! **A program can print a number now, and this paragraph used to say it could
//! not.** §10's stage 2 and stage 3 each write their program with
//! `print(f"{x}")`; `f"…"` lexes, parses, resolves, checks, and — since
//! `science-mir`'s `lower`'s `Builder::lower_fstring` — lowers to §1.7's
//! builder: `science_string_new` into the destination, then one
//! `science_string_push_bytes` per text run and one `science_string_push_*` per
//! hole, each through a `mutable borrowed String`. This crate's part of it was
//! three arms — a `mir::Callee::Runtime`, an `Rvalue::Ref`, and
//! `StatementKind::Activate` as the `Nop` `science-mir`'s §6 always said it
//! was. `let n be 42` … `print(f"n es {n}")` builds, links, runs and prints
//! `n es 42`; `tests/interpolation.rs` is that and seven more, each built,
//! linked, run, and asked what it printed and what status it exited with.
//!
//! **What is still not true.** §10's stage 3 program is a `for` loop, and **a
//! `for` loop does not reach this backend**: its `next` is
//! `science_mir::mir::Unresolved::IterateNext`, which
//! `science_types::thir::ExprKind::For` has no field to carry. That is a hole
//! above this crate; [`lower`]'s own documentation is the account and
//! `tests/stage_two_and_three.rs` is what was built instead. And **an `f"…"`
//! hole renders only the seven types `science-rt` has an entry point for** —
//! `Int`/`I64`, `U64`, `F64`, `F32`, `Bool`, `Char`, `String`. An `I32` hole is
//! `science_mir::mir::Unresolved::Display` and an `SC0400` naming the type, not
//! a call to `science_string_push_i64`: that entry point's own note says
//! *"`I8`…`I64` are sign-extended by codegen before the call"* and **no phase
//! does that**, because `Rvalue::Cast` is in the paragraph below.
//!
//! **A `choice`, a second function and a record are emitted too.** A `match`
//! over a `choice` is §3.3's tagged layout and §2.2's `switch`, with the
//! variant numbering taken from declaration order because MIR branches on a
//! `DefId` and not on a number. A function with parameters is
//! `science_codegen::backend::Operand::Param`, which is above the line now.
//! A record is Decision 17's offsets reached through a `getelementptr`, and
//! `science-codegen` grew no new layout rule for any of the three: §3 already
//! said all of it and what was missing was the `Ty -> CgTy` lowering that reads
//! it. Two more came with them because nothing could run without them —
//! `TerminatorKind::Drop`, which is a `br` when the value owns nothing and
//! `science_string_free` when it is a `String`, and Decision 6's `T?` in both
//! of its representations. `tests/past_stage_three.rs` is twenty-eight programs
//! that build, link, run and are asked what they printed.
//!
//! **The smallest program that still cannot be built is `let t be (1, 2)`** — a
//! tuple, which `lower::Lowerer::cg_ty` has no arm for, exactly as a `choice`
//! had none before this pass. Behind it, in the order they were measured: a
//! **cast** (`Rvalue::Cast`, which for integers is one `trunc`/`sext`/`zext`
//! and for a float is four more decisions nobody has made); a **method call**,
//! whose receiver is a `Self` this crate cannot resolve and which is refused at
//! the *signature* rather than at the call site; a **generic** function, which
//! nothing monomorphises; **drop glue** for anything that owns something other
//! than a bare `String`, which is Decision 12's emitted function; an **array**
//! and an **index**, whose literal reaches MIR untyped and whose index needs
//! §2.4's bounds check; and integer `/`, `%`, `<<` and `>>`, which are refused
//! **deliberately** rather than for want of an instruction — see
//! [`lower::Lowerer::lower_binary`]. Every refusal is `SC0400` and names the
//! construct.
//!
//! # 1. Why it is a separate crate, and how the workspace builds without LLVM
//!
//! `sciencec/src/driver.rs` already gives the rule: *"a workspace that cannot be
//! built without one is a workspace nobody can contribute to."* So:
//!
//! - **This crate is a workspace member and builds with no LLVM at all.** Its
//!   default feature set is empty; [`available`] is then `false`, [`BACKENDS`]
//!   is empty, and [`build`] returns the same `SC0400`
//!   `science_codegen::driver::build` does.
//! - **`--features llvm` compiles [`sys`], [`owned`], [`machine`], [`emit`],
//!   [`lower`] and [`link`]**, and `build.rs` fails loudly, at build time, with
//!   the name of the missing directory if LLVM is not where it said.
//! - **`sciencec` depends on this crate unconditionally** and forwards its own
//!   `llvm` feature to this one's, which is off. The dependency is not optional
//!   because it does not need to be: without the feature this crate is six
//!   hundred lines with no LLVM anywhere in them, and an unconditional
//!   dependency keeps `sciencec` free of `#[cfg]` — there is one call site for
//!   [`build`] and the feature decides what it does rather than whether it
//!   exists. A default `cargo build` of the compiler produces a compiler that
//!   reports `SC0400` with its install note, exactly as before.
//! - So `cargo test --workspace` and `cargo clippy --workspace` need no LLVM,
//!   which is the configuration CI has.
//!
//! **The cost, and it is the ordinary one for a feature-gated backend.** The
//! default `cargo clippy --workspace` does not lint the LLVM half; linting it
//! takes a second invocation with `--features llvm`, and a contributor without
//! LLVM cannot run it at all. That is the same bargain `llvm-sys` would have
//! imposed and it is smaller, because here the *crate* still compiles — only
//! six modules are absent — so a change that breaks this crate's interface with
//! `science-codegen` still breaks the default build. Two test files run in that
//! configuration and are not nothing: `tests/symbols.rs` reads [`sys`] as text
//! and holds the shape of the `extern` block, and `tests/diagnostics.rs` holds
//! `SC0400`.
//!
//! **A second cost, and it is Windows-specific and real.** The backend links
//! `LLVM-C.dll` dynamically, and Windows resolves a DLL through the executable's
//! directory and `PATH`. The LLVM installer does **not** put its `bin` on
//! `PATH`, so a `sciencec` built with `--features llvm` will not start at all —
//! not fail, not print a diagnostic, fail to load — unless
//! `C:\Program Files\LLVM\bin` is on `PATH`. [`dll_directory`] is what a
//! launcher script needs and `build.rs` prints it as a warning at build time.
//! This is §1.5's broken promise arriving two stages early: *"the self-hosted
//! compiler is the one Science program in existence that is not self-contained
//! … it has a hard dynamic dependency on a 100-megabyte shared library."* Here
//! it is the Rust compiler that has it.
//!
//! # 2. What this crate does not decide
//!
//! Decision 42's line, enforced by reading: no layout is computed here, no
//! return is classified, no symbol is mangled, no parameter attribute is chosen.
//! Every one of those arrives in a `Layout`, an `AbiSignature` or a
//! `TargetConfig` from `science-codegen`, and the dependency points one way.
//!
//! Two places strain it and both are named rather than hidden:
//!
//! - [`emit::ExtInst`] — the interface above the line cannot name the address of
//!   a local, the hidden return slot, or one field of a local, and §10's stage 1
//!   needs all three. `emit`'s §2 is the account, variant by variant.
//! - [`lower`] — MIR-to-`Inst` is an above-the-line job and `science-codegen`
//!   has nowhere to put it, so it is written here against that crate's
//!   vocabulary and should move.
//!
//! # 3. What was found by running it
//!
//! Nineteen things that reading could not have established, each recorded
//! where it bites. The first four were found by writing the crate; the rest
//! were found by *running* it, which is the difference §10's staging exists to
//! force.
//!
//! 1. **Decision 36 is unimplementable through LLVM-C**, which has no
//!    `TargetOptions` surface at all. [`machine`] is the account and the
//!    replacement obligation, and `tests/float_policy.rs` is the empirical
//!    check that the policy holds anyway — `a * b + c` at `-O3` on an
//!    FMA-capable CPU emits `vmulsd` and `vaddsd` and no `vfmadd`.
//! 2. **`science_codegen::runtime::RtParam` erases `*const` from `*mut`**, so
//!    Decision 24's `readonly`/`noalias` cannot be emitted on any runtime
//!    declaration without guessing. [`lower::runtime_signature`] emits none.
//! 3. **`science_codegen::backend` cannot name a local's address, a parameter,
//!    or one field of a local.** Three holes; [`emit`] §2 is the account and
//!    [`emit::ExtInst`] is the minimum repair for each.
//! 4. **Decision 25's `link.exe` is not reachable** without reimplementing the
//!    MSVC environment discovery that Decision 25 gives as the reason to use a C
//!    driver in the first place. [`link`] uses `clang` and says what that costs.
//! 5. **Decision 8 and the `sret` convention contradict each other on `_0`.**
//!    MIR's return local *is* the caller's slot when the return is `Indirect`,
//!    and an `alloca` for it is a private copy nothing returns. The emitted
//!    `main` read an untouched stack slot and took the error branch.
//!    [`emit::ExtInst::ReturnSlot`].
//! 6. **`Reloc::Static` does not link on `x86_64-pc-windows-msvc`.** It makes
//!    LLVM address a global with a 32-bit absolute relocation and every Windows
//!    x64 image is large-address-aware, so the linker refuses the first string
//!    literal in `hello, world` with `LNK2017`. [`machine::create`] now uses
//!    `PIC` on all three targets, which is what `clang` itself does here.
//! 7. **`print` and `write` have no declared signature**, so a call to one types
//!    as `Ty::ERROR` **with no diagnostic** and `hello, world` arrives at the
//!    backend with an untypeable local in it. `science-resolve`'s `builtins.rs`
//!    wrote the signature, measured seven corpus false positives and withdrew
//!    it; [`lower`] skips the dead slot and refuses anything that touches it.
//! 8. **A constant operand took its default width rather than the other
//!    operand's**, so `x + 1` on an `I32` built `add i32 %x, i64 1`. A verifier
//!    failure, and unreachable from any program this compiler can compile, which
//!    is exactly why it survived. `tests/emitter.rs` is where the arithmetic is
//!    exercised at all.
//! 9. **`Error?` reaches codegen as `Nullable(Named)`, not `Nullable(Object)`.**
//!    The `(any Error)?` expansion is `assign`'s subtyping rather than a rewrite
//!    of the type, so [`lower::Lowerer::cg_ty`] has to read the definition's
//!    kind. Decision 13's representation is the same either way, and it has to
//!    be: one arm writes `_S4main`'s `sret` slot and the other reads it.
//! 10. **`script-mode.md` §2.4's *"bare `return` is sugar for `return null`"* is
//!     not implemented.** A script whose body is a lone `return` does not
//!     typecheck: `sciencec check` reports `SC0525`, *expected `any Error?`,
//!     found `()`*. Found while writing `tests/exit_code.rs`, which wanted the
//!     three null-returning spellings of §2.3's table and could compile one of
//!     them — `return null`. Not this crate's to fix, and recorded here because
//!     it is the second row of a table this crate now implements the rest of.
//!
//! 11. **`science_exit` did not flush C's buffered output, so the whole of
//!     stage 2 was silently discarded.** `putchar(65)` through an `extern`
//!     block emitted `call i32 @putchar(i32 65)`, verified, linked, exited 0
//!     and **printed nothing**: the byte was in the C runtime's `stdout` when
//!     `std::process::exit` ended the process. `exit.rs` claimed *"`atexit`
//!     handlers and C stdio flushing happen"*; the first half was true.
//!     Invisible at a terminal, because a console is line-buffered and a pipe
//!     is not — so it appears only under a harness that captures output, which
//!     is the only kind §10's discipline permits. `science-rt`'s `flush_all` is
//!     the repair and the account.
//! 12. **A store whose value is wider than its slot is legal IR, and it was
//!     being emitted.** `let b be 0i8 - 128i8` is two constants;
//!     `Operand::ConstInt` carries no type, `emit`'s `width_hint` reads the
//!     *other* operand and there was none, so both took the default `i64` and
//!     the result was stored into a one-byte `alloca`. **Opaque pointers
//!     removed the only thing that related a store's width to its
//!     destination's**, so `LLVMVerifyModule` passed it — which makes §2 of
//!     [`emit`] wrong where it says this class is *"verifier failures rather
//!     than miscompiles"*. [`emit::ExtInst::Const`] gives every constant its
//!     own type on the way in, and `Inst::Store` now refuses a width it was not
//!     expecting, so the class is closed from both ends.
//! 13. **`link.exe` is localised, and `SC0461` was matching English prose.** On
//!     this machine an unresolved symbol reads *"símbolo externo cosinus sin
//!     resolver"*, so a search for *"unresolved external"* finds nothing and
//!     Decision 29's diagnostic never fires — on every non-English Windows
//!     install, invisibly, and undetectably from an English one. `LNK2019` and
//!     `LNK2001` are not translated and are what is matched;
//!     [`link::undefined_symbols`] is the account, including why forcing the
//!     linker into English was the wrong repair.
//!
//! 14. **Decision 24's parameter attributes cannot be emitted from where the
//!     signature is built.** §4.4 calls `noalias` *"the single place in the
//!     language where a bug in region inference produces a wrong answer rather
//!     than a missed error"* and names `--no-noalias` as the mitigation that
//!     *"should be taken"*. It was: `TargetConfig` carries the flag. But a
//!     [`lower::Lowerer`] is built from a `Triple` and nothing else, and the
//!     `TargetConfig` reaches [`emit`] one layer below — so the function that
//!     classifies a Science parameter cannot see whether the escape hatch is
//!     open. Emitting the attribute with its mitigation unreachable is the
//!     worst of the three options, so **no parameter attributes are emitted on
//!     any Science signature**, and the cost is an optimisation rather than an
//!     answer. It is the same trade [`lower::runtime_signature`] takes for
//!     finding 2's reason.
//! 15. **`science_codegen::descriptor::needs_drop` answers `false` for a
//!     `String`.** It is a predicate over `CgTy`, and `CgTy` is *"the set of
//!     distinctions that change a layout or an ABI classification, and nothing
//!     else"* — so a `String` arrives as `{ Ptr(Raw), Usize, Usize }` and
//!     Decision 19's table makes `Raw` the one pointer kind that owns nothing.
//!     The function is exactly right about Science's own types and blind to all
//!     four of the runtime's owning aggregates, because the model it reads
//!     erases what separates them. **Nothing had called it on one**, so nothing
//!     was wrong; a `TerminatorKind::Drop` lowered through it would have turned
//!     every dropped `String` into a leak with no diagnostic anywhere.
//!     [`lower::Lowerer::drop_runs_something`] walks `Ty` instead and says why.
//! 16. **Decision 16's mangled symbols carry the source file's stem.**
//!     `DefTable::path_of` says *"the crate root is unnamed and contributes
//!     nothing"*, and the file's module is not the crate root — `resolve_module`
//!     names it after the file — so a script's `main` mangles as
//!     `_S7fixture4main` and the same source copied to `prog.science` gets a
//!     different symbol. That is precisely the property Decision 16 gives as
//!     its own reason for refusing hashes: *"deterministic from the source
//!     alone"*. It is **not** repaired here, because whether a file's stem is
//!     part of its module path is `science-resolve`'s answer and hiding it in
//!     the mangling would make two modules' functions share a symbol. The one
//!     case another note already fixes is fixed: `script-mode.md` §2.3 and
//!     [`lower::Lowerer::lower_c_main`] both name the entry `_S4main`, so the
//!     entry keeps that and every other definition carries the path it has.
//! 17. **`science-mir`'s `needs_drop` answers `true` for every `choice`**, so
//!     MIR emits a `Drop` terminator for the scrutinee of every `match` in the
//!     language — *"§4's true where it cannot tell"*. The last pass measured
//!     the smallest unbuildable program as *"a `match` over a two-variant
//!     payload-free `choice`"* and located it at `cg_ty`'s missing arm; the arm
//!     was necessary and was not sufficient, because a `Drop` of a `Colour`
//!     stood behind it. Two of the four things that pass listed as separate
//!     items were one program.
//! 18. **A runtime call's arguments are checked by `LLVMVerifyModule` and by
//!     nothing else, and the verifier cannot see the mistake that matters
//!     here.** [`lower::Lowerer::lower_runtime_call`] matches each MIR operand
//!     to a `RUNTIME` parameter and then reads a place operand with
//!     [`lower::Lowerer::lower_operand`], which emits a **whole-local load at
//!     the local's own layout** and ignores the `expected` layout it was
//!     handed. So the agreement between a MIR operand and a C parameter rests
//!     on the verifier.
//!
//!     For an integer that is enough, and it was measured: making `science-mir`
//!     render an `I32` through `science_string_push_i64` produces
//!     `call void @science_string_push_i64(ptr %v6, i32 %v7)` against a
//!     `declare` that says `i64`, and the verifier rejects the module with
//!     *"Call parameter type does not match function signature"*. **For a
//!     pointer it is not**, because opaque pointers make every pointer the same
//!     type. An `f"…"` hole whose type is `borrowed String` is a pointer and
//!     `science_string_push_str`'s second parameter is a pointer; with
//!     `science-mir`'s §4 dereference removed from the hole's borrow, the
//!     argument becomes the address of *this frame's parameter slot* — a
//!     pointer to a pointer — where a `{ ptr, len, cap }` was wanted. It
//!     compiles, links, verifies, and aborts at run time with
//!     `panic: science-rt: out of memory`, from the byte pattern of a pointer
//!     read as a length.
//!
//!     That is the same shape as finding 12 — a wrongness the IR cannot
//!     express and the verifier therefore cannot see — and the second instance
//!     of it: there, a width the opaque pointer erased; here, an indirection it
//!     erased. The IR reads `call void @science_string_push_str(ptr %a, ptr %b)`
//!     either way. `tests/interpolation.rs` is the execution test
//!     that separates them, and `science-mir`'s `tests/fstring.rs` asserts the
//!     dereference is there, because by the time the operand reaches this crate
//!     there is nothing left to assert it against.
//! 19. **`Rvalue::Ref` had no arm although every ingredient of one did.**
//!     [`lower::Lowerer::place_address`] has had §2.3's `deref` row — *"it
//!     loads a pointer and that pointer becomes the new base"* — since records
//!     landed, so this crate could *read through* a reference and could not
//!     *take* one. The arm is a `LocalAddr` and a `Store`, and the refusal it
//!     replaced said *"a borrow"*, which names the construct correctly and
//!     tells a reader nothing about how close the crate was to lowering it.
//!
//!     The reason it went unnoticed is worth the line: nothing that reached
//!     this backend took a borrow. `print` is undeclared so its argument is
//!     moved, a receiver borrow needs a method call and a method call is
//!     refused at the signature, and the corpus never got this far. `f"…"` is
//!     the first construct in the language whose *lowering* takes one —
//!     the accumulator, once per fragment — so it arrived needing the arm and
//!     needing `StatementKind::Activate`, which was also a refusal and which
//!     `science-mir`'s §6 had always described as *"one statement per two-phase
//!     borrow, which codegen treats as a `Nop`"*.
//!
//! **And nine was itself found this way**, which is the point of the list: the
//! numbering has grown seven times and each entry is something the notes did
//! not say. Eleven, twelve, thirteen and eighteen were all found by *running* a
//! program — none of them changes the IR in a way that looks wrong, and twelve
//! and eighteen both pass the verifier, which is the pair that says opaque
//! pointers cost this crate two kinds of check it cannot get back. Fifteen and seventeen are the same shape one
//! level up: a predicate that is right about the model it was written for and
//! wrong about the caller that arrived later. Nineteen is the other recurring
//! shape: a refusal that names a construct correctly and hides how little was
//! missing.

#![warn(missing_docs)]

use std::path::PathBuf;

use science_codegen::driver::BuildRequest;
// Only the `llvm` build names a target triple; the stub's `build` reports
// `SC0400` and never gets as far as asking what machine it is on.
#[cfg(feature = "llvm")]
use science_codegen::layout::Triple;
use science_codegen::target::{OptLevel, TargetConfig};
use science_diagnostics::Diagnostics;
use science_mir::mir::Body as MirBody;
use science_resolve::hir::DefTable;
use science_types::Types;

#[cfg(feature = "llvm")]
pub mod emit;
#[cfg(feature = "llvm")]
pub mod link;
#[cfg(feature = "llvm")]
pub mod lower;
#[cfg(feature = "llvm")]
pub mod machine;
#[cfg(feature = "llvm")]
pub mod owned;
#[cfg(feature = "llvm")]
pub mod sys;

/// The backends this build of the crate provides.
///
/// A slice rather than a boolean for the reason
/// `science_codegen::driver::BACKENDS`'s own note gives: Decision 42 is
/// explicitly about there being more than one eventually.
pub const BACKENDS: &[&str] = if cfg!(feature = "llvm") { &["llvm"] } else { &[] };

/// Whether this build can produce an executable.
///
/// Constant within any one build, which is what clippy objects to and is the
/// point: the answer is fixed by a `cfg!` at compile time, so a `sciencec`
/// built without the feature can say so without probing the machine. The
/// alternative — deciding at run time whether `LLVM-C.dll` can be found — is a
/// different question, and [`dll_directory`] is where it is asked.
#[allow(clippy::const_is_empty)]
pub fn available() -> bool {
    !BACKENDS.is_empty()
}

/// Where `LLVM-C.dll` lives, for a `PATH` a Windows user has to set.
///
/// `None` when the crate was built without LLVM. See §1's second cost.
pub fn dll_directory() -> Option<PathBuf> {
    llvm_prefixes().into_iter().map(|prefix| prefix.join("bin")).find(|bin| bin.is_dir())
}

/// Candidate LLVM installation roots, in search order.
///
/// `SCIENCE_LLVM_PREFIX` first, then `LLVM_SYS_181_PREFIX` — which is the
/// variable Decision 1's cost item 5 names and which a contributor who followed
/// `science-codegen`'s install note will already have set — then the one
/// `build.rs` resolved, then the platform default.
pub fn llvm_prefixes() -> Vec<PathBuf> {
    let mut prefixes: Vec<PathBuf> = Vec::new();
    for variable in ["SCIENCE_LLVM_PREFIX", "LLVM_SYS_181_PREFIX"] {
        if let Some(value) = std::env::var_os(variable) {
            prefixes.push(PathBuf::from(value));
        }
    }
    if let Some(resolved) = option_env!("SCIENCE_LLVM_PREFIX_RESOLVED") {
        prefixes.push(PathBuf::from(resolved));
    }
    prefixes.extend(default_prefixes().iter().map(PathBuf::from));
    prefixes.dedup();
    prefixes
}

fn default_prefixes() -> &'static [&'static str] {
    if cfg!(windows) {
        &[r"C:\Program Files\LLVM"]
    } else if cfg!(target_os = "macos") {
        &["/opt/homebrew/opt/llvm@18", "/usr/local/opt/llvm@18"]
    } else {
        &["/usr/lib/llvm-18", "/usr"]
    }
}

/// Every `extern` block in a crate, in source order.
///
/// **Here rather than in `science-resolve`, and outside the feature gate.**
/// The *order* is codegen's question — Decision 28 makes the source order of
/// the `library` clauses the link order — and nothing above the line has asked
/// for the list. It sits in this module rather than in [`lower`] because
/// `sciencec` builds the list whether or not it has a backend to hand it to,
/// and a `sciencec` compiled without `--features llvm` still has to compile.
pub fn extern_blocks(krate: &science_resolve::hir::Crate) -> Vec<&science_resolve::hir::ExternBlock> {
    krate
        .modules
        .iter()
        .flat_map(|module| module.items.iter())
        .filter_map(|item| match &item.kind {
            science_resolve::hir::ItemKind::Extern(block) => Some(block),
            _ => None,
        })
        .collect()
}

/// Everything a build needs that this crate does not compute.
///
/// The front end is `sciencec`'s to run — it owns the file I/O, the module
/// collection and the diagnostic reporting — so what crosses here is its
/// output: the definitions, the types, and the MIR bodies region inference has
/// already approved.
pub struct BuildInput<'a> {
    /// What was asked for.
    pub request: &'a BuildRequest,
    /// The crate's definitions.
    pub defs: &'a DefTable,
    /// The crate's types.
    pub types: &'a Types,
    /// The crate's declared signatures.
    ///
    /// **Added for §10's stage 2, and the alternative was worse.** An
    /// `extern "C"` function has no MIR body, so its parameter and return
    /// types reach codegen through nothing else: `DefTable` carries a name, a
    /// kind and a span, and `hir::ExternFn` carries the *syntax* of a
    /// parameter list, which this crate would have to resolve —
    /// `type BlasInt is I32` is an alias — to get a type out of. That is a
    /// second, worse copy of the type checker living in the backend.
    /// [`science_types::items::Declarations`] is the table the checker built
    /// and the one the call site was checked against.
    pub decls: &'a science_types::items::Declarations,
    /// The crate's `extern` blocks, in source order — which Decision 28 makes
    /// significant, because it is the order the `library` clauses reach the
    /// linker in. [`lower::Lowerer::extern_blocks`] collects them.
    pub externs: &'a [&'a science_resolve::hir::ExternBlock],
    /// Every MIR body in the crate.
    pub bodies: &'a [MirBody],
    /// Where the executable goes.
    pub output: PathBuf,
}

/// What a successful build produced.
pub struct Built {
    /// The executable.
    pub executable: PathBuf,
    /// The configuration it was built with, for the `[build]` table of
    /// `package-manager.md` Decision 3.
    pub config: TargetConfig,
    /// The module's LLVM IR, after the pipeline. Kept because `--emit=llvm-ir`
    /// wants it and because every test in this crate reads it.
    pub ir: String,
    /// The linker driver that ran, resolved — §12 item 12 asks for the pipeline
    /// and the CPU to be in the build record, and the driver belongs beside
    /// them.
    pub linker: String,
}

/// Build a program.
///
/// Returns diagnostics rather than a string, for
/// `science_codegen::driver::build`'s reason: *"a build failure is reported the
/// way every other phase reports one … a compiler that reported one kind of
/// failure differently from the others is a compiler whose CI cannot gate on the
/// difference."*
#[cfg(not(feature = "llvm"))]
pub fn build(input: &BuildInput) -> Result<Built, Diagnostics> {
    let _ = input;
    let mut diagnostics = Diagnostics::new();
    diagnostics.push(no_backend());
    Err(diagnostics)
}

/// The `SC0400` a `sciencec` built without `--features llvm` reports.
///
/// **The decision.** This crate emits its own `SC0400` rather than
/// `science_codegen::driver::no_backend`'s, and the only thing that differs is
/// the `how` half — the sentence telling the reader what to do.
///
/// **The reason.** That sentence is a set of instructions, and
/// `science-codegen`'s version instructs the reader to install `llvm-config`
/// and set **`LLVM_SYS_181_PREFIX`**. Both belong to `llvm-sys`, and
/// [`sys`]'s §0 is the record of why this crate does not and cannot use it: the
/// Windows binary release of LLVM 18.1.8 ships neither `llvm-config.exe` nor
/// the ~100 static archives `llvm-sys` links, so a contributor who followed the
/// old note would install LLVM, set the variable, and still have no backend —
/// because the variable was never the thing that was missing. What was missing
/// is `--features llvm`, which no version of that message mentions.
/// `science-codegen`'s own text is left alone because that crate is above
/// Decision 42's line and has no feature flag of its own to name; it is this
/// crate that knows what turns the backend on.
///
/// **The cost.** Two `SC0400` texts exist in the workspace and only one of them
/// is reachable from `sciencec`, so the other can rot without anything failing.
/// `tests/diagnostics.rs` pins this one against [`llvm_prefixes`], which is the
/// half that would actually go stale — the variable names — and
/// `science-codegen`'s own tests still pin its.
///
/// `SCIENCE_LLVM_PREFIX` is named first because it is this crate's own;
/// `LLVM_SYS_181_PREFIX` is still read, and is still mentioned, because a
/// contributor who followed the older note will already have set it and should
/// be told it works rather than left to discover it does.
pub fn no_backend() -> science_diagnostics::Diagnostic {
    science_codegen::diagnostics::backend_not_compiled_in("llvm", install_instructions())
}

/// How to obtain a `sciencec` with a backend, on this host.
///
/// Three steps and they are all required: install LLVM 18.1, tell the build
/// where it is if it is not in the default place, and **turn the feature on** —
/// which is the step the message this replaces did not have.
pub fn install_instructions() -> &'static str {
    if cfg!(target_os = "windows") {
        "install LLVM 18.1.8 — `winget install LLVM.LLVM --version 18.1.8` — and rebuild with \
         `cargo build -p sciencec --features llvm`. The backend links `lib/LLVM-C.lib` and loads \
         `bin/LLVM-C.dll` at run time, so the installation's `bin` directory has to be on `PATH`; \
         set `SCIENCE_LLVM_PREFIX` (or `LLVM_SYS_181_PREFIX`) to the install root if it is not \
         the default one under Program Files. This backend does not use `llvm-sys`, so no \
         `llvm-config` and no static archives are needed"
    } else if cfg!(target_os = "macos") {
        "install LLVM 18.1 — `brew install llvm@18` — and rebuild with \
         `cargo build -p sciencec --features llvm`, setting `SCIENCE_LLVM_PREFIX` (or \
         `LLVM_SYS_181_PREFIX`) to `$(brew --prefix llvm@18)` if it is not \
         `/opt/homebrew/opt/llvm@18`. The backend links `libLLVM.dylib` directly, so no \
         `llvm-sys` and no `llvm-config` are needed"
    } else {
        "install LLVM 18.1 — `apt install llvm-18` or your distribution's equivalent — and \
         rebuild with `cargo build -p sciencec --features llvm`, setting `SCIENCE_LLVM_PREFIX` \
         (or `LLVM_SYS_181_PREFIX`) to the install root if it is not `/usr/lib/llvm-18`. The \
         backend links `libLLVM-18.so` directly, so no `llvm-sys`, no `llvm-config` and no \
         `-dev` package are needed"
    }
}

/// Build a program: MIR to an object file to an executable.
///
/// The order is §10's stage 0 and stage 1 in one function, and every step is one
/// of the note's:
///
/// 1. [`lower`] turns MIR into the backend's instruction set, refusing anything
///    beyond stage 1 as `SC0400`.
/// 2. [`emit::LlvmBackend`] builds the module: the target machine, the string
///    literals, the `declare`s derived from `science_codegen::runtime::RUNTIME`,
///    and the two functions.
/// 3. `LLVMVerifyModule` (Decision 34), then Decision 33's pipeline by name,
///    then Decision 35's fast-math grep over the result, then
///    `LLVMTargetMachineEmitToFile`.
/// 4. [`link`] drives `clang` over the object and `science_rt.lib`.
#[cfg(feature = "llvm")]
pub fn build(input: &BuildInput) -> Result<Built, Diagnostics> {
    let mut diagnostics = Diagnostics::new();
    let host = Triple::host();
    let Some(triple) = host else {
        diagnostics.push(science_codegen::diagnostics::cross_compilation_unsupported(
            "the host",
            "a target this compiler has no layout rules for",
        ));
        return Err(diagnostics);
    };
    if let Some(requested) = input.request.target {
        if requested != triple {
            diagnostics.push(science_codegen::diagnostics::cross_compilation_unsupported(
                requested.as_str(),
                triple.as_str(),
            ));
            return Err(diagnostics);
        }
    }
    let config = TargetConfig::new(triple, input.request.opt)
        .with_no_noalias(input.request.no_noalias);

    let mut lowerer = lower::Lowerer::with_externs(
        triple,
        input.defs,
        input.types,
        input.decls,
        input.externs,
    );
    let lowered = match lowerer.lower_crate(input.bodies) {
        Ok(lowered) => lowered,
        Err(unlowered) => {
            diagnostics.push(unlowered.to_diagnostic());
            return Err(diagnostics);
        }
    };
    let module_name = input
        .request
        .inputs
        .first()
        .map(|name| name.as_str())
        .unwrap_or("science");
    emit_and_link(&lowered, module_name, &config, &input.output)
}

/// Steps 2 to 4 of [`build`]: a lowered module to an executable.
///
/// **Split out for one caller and it is a test.** `tests/exit_code.rs` has to
/// build a module whose `_S4main` returns a **non-null** `Error?`, which no
/// Science program this compiler can compile produces — stage 1 cannot
/// construct a concrete error, box it, or fill a vtable — so it writes that one
/// function itself and pairs it with the real
/// [`lower::Lowerer::lower_c_main`]. Everything from here down is then the same
/// code `build` runs, rather than a second copy of it in a test: the same
/// declaration order, the same verifier, the same Decision 33 pipeline, the
/// same linker. A test that duplicated this would be asserting against its own
/// copy of the thing under test.
#[cfg(feature = "llvm")]
pub fn emit_and_link(
    lowered: &lower::Lowered,
    module_name: &str,
    config: &TargetConfig,
    output: &std::path::Path,
) -> Result<Built, Diagnostics> {
    use science_codegen::backend::Backend;

    let mut diagnostics = Diagnostics::new();
    let triple = config.triple();
    let mut backend = emit::LlvmBackend::new();
    let mut run = || -> Result<(), science_codegen::backend::BackendError> {
        backend.begin_module(module_name, config)?;
        for literal in &lowered.literals {
            backend.define_string_bytes(literal)?;
        }
        for declaration in &lowered.declarations {
            backend.declare_function(declaration)?;
        }
        // Declared before any is defined, so that `main` can call `_S4main`
        // whichever order Decision 4's sort put them in.
        let mut ids = Vec::with_capacity(lowered.definitions.len());
        for (signature, _) in &lowered.definitions {
            ids.push(backend.declare_function(signature)?);
        }
        for (id, (signature, body)) in ids.into_iter().zip(&lowered.definitions) {
            backend.define_function_ext(id, signature, body)?;
        }
        backend.verify()
    };
    if let Err(error) = run() {
        diagnostics.push(internal_error(&error));
        return Err(diagnostics);
    }

    let object = object_path(output);
    if let Err(error) = backend.emit_object_to(&object) {
        diagnostics.push(internal_error(&error));
        return Err(diagnostics);
    }
    let ir = backend.ir();

    let driver = match link::find_driver() {
        Ok(driver) => driver,
        Err(error) => {
            diagnostics.push(error.to_diagnostic());
            return Err(diagnostics);
        }
    };
    let runtime = match link::find_runtime() {
        Ok(path) => path,
        Err(error) => {
            diagnostics.push(error.to_diagnostic());
            return Err(diagnostics);
        }
    };
    if let Err(error) = link::link(&driver, &object, &runtime, output, triple, &lowered.libraries)
    {
        // Decision 29: `SC0461` for an undefined symbol codegen's own table can
        // attribute to an `extern` declaration, and `SC0402` for everything
        // else. The table is `lowered.foreign` and it exists as of stage 2; the
        // attribution is `link::undefined_symbols`, which requires both a
        // marker word meaning "not found" and the symbol as a whole
        // identifier, so a linker failure that merely mentions the name is
        // still §5.5's raw text.
        //
        // **Both are reported when one is attributed.** The `SC0461`s say which
        // declarations failed; the `SC0402` still carries the command line and
        // the linker's output, because a reader whose library was the wrong one
        // needs to see the flags that were passed. A codegen that swallowed the
        // linker's text on the strength of a substring match would be hiding
        // the evidence for its own guess.
        if let link::LinkError::Failed { output: text, .. } = &error {
            let symbols: Vec<String> =
                lowered.foreign.iter().map(|entry| entry.symbol.clone()).collect();
            for symbol in link::undefined_symbols(text, &symbols) {
                let entry = lowered
                    .foreign
                    .iter()
                    .find(|entry| entry.symbol == *symbol)
                    .expect("the symbol came from this list");
                let flags = entry
                    .library
                    .as_deref()
                    .map(|library| link::library_flags(triple, library))
                    .unwrap_or_default();
                diagnostics.push(science_codegen::diagnostics::undefined_foreign_symbol(
                    &entry.name,
                    &entry.symbol,
                    entry.library.as_deref(),
                    &flags,
                    entry.span,
                ));
            }
        }
        diagnostics.push(error.to_diagnostic());
        return Err(diagnostics);
    }
    let _ = std::fs::remove_file(&object);

    Ok(Built {
        executable: output.to_path_buf(),
        config: config.clone(),
        ir,
        linker: driver.program.display().to_string(),
    })
}

/// The object file beside the executable.
///
/// Beside rather than in a temporary directory, because
/// `package-manager.md` Decision 7 removes environment-dependent paths from
/// output and a temporary directory is one. It is removed after a successful
/// link and **kept after a failed one**, so that the command `SC0402` printed
/// can be re-run by hand.
#[cfg(feature = "llvm")]
fn object_path(output: &std::path::Path) -> PathBuf {
    output.with_extension(if cfg!(windows) { "obj" } else { "o" })
}

/// A backend failure, as a diagnostic.
///
/// `SC0402` — which §11 defines for a linker failure that cannot be attributed —
/// is the closest claimed code, and the shape is the same: something below the
/// compiler failed and the honest thing is to hand over what it said. Decision
/// 34 makes a verifier failure *"an internal compiler error that prints the
/// offending function's IR"*, and that is what this carries.
#[cfg(feature = "llvm")]
fn internal_error(error: &science_codegen::backend::BackendError) -> science_diagnostics::Diagnostic
{
    science_codegen::diagnostics::linker_failed("(the backend, before the linker)", &error.to_string())
}

/// The default optimisation level, re-exported so `sciencec` does not have to
/// name two crates to spell `-O2`.
pub const DEFAULT_OPT: OptLevel = OptLevel::O2;
