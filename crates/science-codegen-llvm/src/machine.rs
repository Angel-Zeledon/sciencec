//! The target machine, and the obligation LLVM-C cannot discharge.
//!
//! **The decision.** This module creates the target machine from a
//! [`TargetConfig`] and **does not set `AllowFPOpFusion`, because LLVM-C has no
//! entry point that sets it.** Decision 36's obligation is discharged by a
//! different mechanism, stated below and tested in `tests/float_policy.rs`.
//!
//! **The reason, which is a finding about the note rather than a choice.**
//! Decision 36 reads:
//!
//! > *Codegen sets `AllowFPOpFusion = Strict` on every target machine it
//! > creates, and a test asserts that a Science program computing `a * b + c`
//! > emits `fmul` followed by `fadd` and no `fma`, on every target.*
//!
//! and §15 calls it *"the single highest-value line in this note … which is
//! exactly why it will be the one that is missing"*.
//!
//! It is missing, and not for the reason §15 predicted. `AllowFPOpFusion` is a
//! field of C++'s `llvm::TargetOptions`, and **`LLVM-C` does not expose
//! `TargetOptions` at all.** The 1226 symbols this installation's `LLVM-C.lib`
//! exports include `LLVMSetFastMathFlags` and `LLVMGetFastMathFlags`, which are
//! the per-*instruction* flags, and nothing whatever for the target's options.
//! LLVM 18 added an options builder — `LLVMCreateTargetMachineOptions`,
//! `LLVMTargetMachineOptionsSetCPU`, `...SetFeatures`, `...SetABI`,
//! `...SetCodeGenOptLevel`, `...SetRelocMode`, `...SetCodeModel` — and it has no
//! FP-fusion setter either.
//!
//! **So Decision 36 is unimplementable through the C API, in either
//! implementation.** That is not a gap this crate can work around by trying
//! harder: `inkwell` cannot set it either, for the same reason, and a Science
//! backend calling `LLVM-C` certainly cannot.
//!
//! # What discharges the obligation instead
//!
//! The policy survives, and it survives for a reason the note did not have.
//! §7.3's *"good news"* paragraph is more load-bearing than it looks:
//!
//! > *LLVM's value-changing float transforms are gated on **per-instruction
//! > fast-math flags** … and the optimisation level does not set them.*
//!
//! In LLVM 18 that is true of the **backend's** contraction as well, not only of
//! the optimiser's. `DAGCombiner`'s contraction predicates test
//! `Options.AllowFPOpFusion == FPOpFusion::Fast` **or** the node's `contract`
//! flag. `Standard` — LLVM's default, and the value Decision 36 is alarmed
//! about — does **not** enable backend fusion; it means "the front end decides",
//! and clang implements `-ffp-contract=on` by setting the `contract` flag on the
//! IR it emits, not by relying on the backend.
//!
//! So the two facts that actually hold the policy up are:
//!
//! 1. **This backend never sets a fast-math flag on any instruction.**
//!    `LLVMSetFastMathFlags` is not declared in [`crate::sys`], which is
//!    §7.3 obligation 1 enforced by absence rather than by discipline — the same
//!    move `science_codegen::backend::FloatOp` makes above the line.
//! 2. **`Standard` is not `Fast`.** Nothing this crate does sets `Fast`, and
//!    there is no way to set it.
//!
//! **This is weaker than what Decision 36 asked for and it should be read as
//! weaker.** Decision 36 wanted a setting that makes the policy true regardless
//! of what the IR says. What is available is a policy that is true because of
//! what the IR says, which is exactly the situation §7.3 calls *"an obligation
//! to never opt in — a line that is correct by not being written, which means
//! nothing tests it"*. The answer is the same as §7.3's: **write the test**.
//! `tests/float_policy.rs` compiles `a * b + c` to x86 assembly at `-O3` for a
//! CPU that has an FMA unit and asserts that no `vfmadd` appears. That is
//! Decision 36's own stated test — *"a test asserts that a Science program
//! computing `a * b + c` emits `fmul` followed by `fadd` and no `fma`"* — run
//! against machine instructions rather than against a field nobody can set.
//!
//! **And the test has to name an FMA-capable CPU, which Decision 38 does not.**
//! Decision 38 makes the default CPU the triple's baseline, `x86-64`, which is
//! SSE2 and has no FMA unit at all. A contraction test at the baseline passes on
//! a backend that contracts everything, because there is no instruction to
//! contract into. The test therefore builds a second machine at `x86-64-v3` and
//! that is the only configuration in which the assertion means anything.
//!
//! # `--target-cpu=native` is still unresolved here
//!
//! Decision 38 requires that `native` be *"resolved to a concrete CPU name at
//! build time, and recorded as that concrete name"*. LLVM-C exposes
//! `LLVMGetHostCPUName`, so this is reachable; it is not called, because
//! `science_codegen::target::TargetCpu::named` takes the resolved name as a
//! parameter and nothing in F0 passes `native` yet. When something does, this is
//! the module that resolves it and the declaration to add is one line.

use science_codegen::layout::Triple;
use science_codegen::target::{OptLevel, TargetConfig};

use crate::owned::{Context, Module, TargetMachine, cstr};
use crate::sys;

/// Register the X86 target.
///
/// Idempotent inside LLVM, but called once per process here anyway, because
/// LLVM's registries are not thread-safe against concurrent registration and
/// `cargo test` runs test functions on several threads.
///
/// **Only X86.** Cross-compilation is `SC0406` (§5.6), so the target is the
/// host; `Triple::host()` returns `None` on any host that is not one of the
/// three; and two of the three are x86-64. A run on `aarch64-apple-darwin`
/// needs `LLVMInitializeAArch64{TargetInfo,Target,TargetMC,AsmPrinter}` added to
/// [`crate::sys`] and to this function, and nothing else.
pub fn initialise() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe {
        sys::LLVMInitializeX86TargetInfo();
        sys::LLVMInitializeX86Target();
        sys::LLVMInitializeX86TargetMC();
        // Not optional for object emission: the object streamer is registered
        // here, and without it `LLVMTargetMachineEmitToFile` reports only that
        // the target does not support this file type.
        sys::LLVMInitializeX86AsmPrinter();
    });
}

/// LLVM's own name for the host triple, for comparison against
/// [`Triple::host`].
pub fn default_triple() -> String {
    let raw = unsafe { sys::LLVMGetDefaultTargetTriple() };
    match unsafe { crate::owned::Message::from_message(raw) } {
        Some(message) => message.to_string_lossy(),
        None => String::new(),
    }
}

fn codegen_level(opt: OptLevel) -> std::ffi::c_uint {
    match opt {
        OptLevel::O0 => sys::codegen_opt::NONE,
        OptLevel::O1 => sys::codegen_opt::LESS,
        OptLevel::O2 => sys::codegen_opt::DEFAULT,
        OptLevel::O3 => sys::codegen_opt::AGGRESSIVE,
    }
}

/// Create a target machine for `config`.
///
/// **`RelocMode` is `PIC` on all three targets, including Windows, and the
/// Windows half is a correction rather than a preference.** This module
/// previously chose `Static` there, on the reasoning that `link.exe` builds a
/// relocatable image from non-PIC objects and that ASLR on Windows is a header
/// flag rather than a code-generation mode. Both sentences are true and the
/// conclusion does not follow: on `x86_64-pc-windows-msvc`, `Reloc::Static`
/// makes LLVM address a global with a **32-bit absolute** relocation, and every
/// Windows x64 image is large-address-aware, so the linker refuses it:
///
/// ```text
/// hello.obj : error LNK2017: 'ADDR32' relocation to '.rdata' invalid
///                            without /LARGEADDRESSAWARE:NO
/// LINK : fatal error LNK1165
/// ```
///
/// That is the first string literal in `hello, world`, and it is a link failure
/// rather than a miscompile only because the address happened to be in
/// `.rdata`. `PIC` emits the RIP-relative form instead, which is what `clang`
/// itself uses for this triple — its default relocation model on Windows x64 is
/// `pic`, not `static`. The cost is nothing on this target: RIP-relative
/// addressing is the x86-64 norm and Windows x64 has no `-fno-pic` ABI to be
/// compatible with.
///
/// `CodeModel::Small` everywhere, which is what every C compiler on these three
/// targets does for an executable.
pub fn create(config: &TargetConfig, cpu: &str, features: &str) -> Result<TargetMachine, String> {
    initialise();
    let triple = cstr(config.triple().as_str());
    let mut target: sys::LLVMTargetRef = std::ptr::null_mut();
    let mut error: *mut std::ffi::c_char = std::ptr::null_mut();
    let failed =
        unsafe { sys::LLVMGetTargetFromTriple(triple.as_ptr(), &mut target, &mut error) };
    if failed != 0 {
        let message = unsafe { crate::owned::Message::from_message(error) };
        return Err(message
            .map(|m| m.to_string_lossy())
            .unwrap_or_else(|| format!("no LLVM target for `{}`", config.triple().as_str())));
    }
    let cpu = cstr(cpu);
    let features = cstr(features);
    let reloc = sys::reloc::PIC;
    let machine = unsafe {
        sys::LLVMCreateTargetMachine(
            target,
            triple.as_ptr(),
            cpu.as_ptr(),
            features.as_ptr(),
            codegen_level(config.opt()),
            reloc,
            sys::code_model::SMALL,
        )
    };
    if machine.is_null() {
        return Err(format!(
            "LLVM refused a target machine for `{}`",
            config.triple().as_str()
        ));
    }
    Ok(unsafe { TargetMachine::from_raw(machine) })
}

/// Create the target machine a build uses: `config`'s triple at `config`'s
/// baseline CPU, with no feature string.
pub fn for_config(config: &TargetConfig) -> Result<TargetMachine, String> {
    create(config, config.cpu().as_str(), "")
}

/// Run Decision 33's pipeline over a module.
///
/// The pipeline is [`OptLevel::pipeline`] and nothing else — *"no custom
/// passes"*. The pass builder options are LLVM's defaults, because every one of
/// them is a way to make the pipeline something other than the string that names
/// it, and the string is what goes in the `[build]` table.
pub fn optimise(module: &Module, machine: &TargetMachine, opt: OptLevel) -> Result<(), String> {
    let options = crate::owned::PassOptions::new();
    let passes = cstr(opt.pipeline());
    let error = unsafe {
        sys::LLVMRunPasses(module.raw(), passes.as_ptr(), machine.raw(), options.raw())
    };
    if error.is_null() {
        return Ok(());
    }
    // `LLVMGetErrorMessage` consumes the error, so this is the only call.
    let raw = unsafe { sys::LLVMGetErrorMessage(error) };
    let message = unsafe { crate::owned::Message::from_error(raw) };
    Err(message
        .map(|m| m.to_string_lossy())
        .unwrap_or_else(|| format!("the `{}` pipeline failed", opt.pipeline())))
}

/// Emit a module to a file.
///
/// `LLVMTargetMachineEmitToFile` returns **1 on failure**, which is the
/// inversion [`sys::LLVMBool`]'s documentation is about; reading it the other
/// way produces a compiler that writes nothing and reports success.
pub fn emit_to_file(
    module: &Module,
    machine: &TargetMachine,
    path: &std::path::Path,
    file_type: std::ffi::c_uint,
) -> Result<(), String> {
    let path_text = path.to_string_lossy().into_owned();
    let filename = cstr(&path_text);
    let mut error: *mut std::ffi::c_char = std::ptr::null_mut();
    let failed = unsafe {
        sys::LLVMTargetMachineEmitToFile(
            machine.raw(),
            module.raw(),
            filename.as_ptr(),
            file_type,
            &mut error,
        )
    };
    let message = unsafe { crate::owned::Message::from_message(error) };
    if failed == 0 {
        return Ok(());
    }
    Err(message
        .map(|m| m.to_string_lossy())
        .unwrap_or_else(|| format!("LLVM could not write `{path_text}`")))
}

/// The `datalayout` string for a machine, and the triple, applied to a module.
pub fn describe(module: &Module, machine: &TargetMachine, triple: Triple) {
    let data = machine.data_layout();
    module.set_target(triple.as_str(), &data.as_string());
}

/// A context with the X86 target registered. A convenience for tests, and the
/// one place `initialise` is called from outside [`create`].
pub fn context() -> Context {
    initialise();
    Context::new()
}
