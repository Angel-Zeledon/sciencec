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
//! **What is not true.** Stage 2 is not started: no `extern` block is lowered,
//! nothing has been linked against a C library, and the five notes resting on
//! *"a declaration becomes an LLVM `declare` and the system linker resolves
//! it"* are still untested. Stage 3's control flow exists in the instruction set
//! and is not reachable from any source program, because [`lower`] refuses
//! everything but a script body, a string literal and `print`. The refusal is
//! `SC0400` and it names the construct.
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
//! Nine things that reading could not have established, each recorded where it
//! bites. The first four were found by writing the crate; the rest were found by
//! *running* it, which is the difference §10's staging exists to force.
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
    use science_codegen::backend::Backend;

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

    let mut lowerer = lower::Lowerer::new(triple, input.defs, input.types);
    let lowered = match lowerer.lower_crate(input.bodies) {
        Ok(lowered) => lowered,
        Err(unlowered) => {
            diagnostics.push(unlowered.to_diagnostic());
            return Err(diagnostics);
        }
    };

    let mut backend = emit::LlvmBackend::new();
    let module_name = input
        .request
        .inputs
        .first()
        .map(|name| name.as_str())
        .unwrap_or("science");
    let mut run = || -> Result<(), science_codegen::backend::BackendError> {
        backend.begin_module(module_name, &config)?;
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

    let object = object_path(&input.output);
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
    if let Err(error) = link::link(&driver, &object, &runtime, &input.output, triple) {
        diagnostics.push(error.to_diagnostic());
        return Err(diagnostics);
    }
    let _ = std::fs::remove_file(&object);

    Ok(Built {
        executable: input.output.clone(),
        config,
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
