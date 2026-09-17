//! Decision 36, tested against machine instructions because it cannot be set.
//!
//! **The decision.** `a * b + c` is compiled to x86-64 assembly at `-O3` for a
//! CPU that *has* an FMA unit, and the assertion is that the output contains
//! `mulsd`/`vmulsd` followed by an add and **no `vfmadd`**.
//!
//! **The reason.** [`science_codegen_llvm::machine`] records the finding that
//! Decision 36 — *"codegen sets `AllowFPOpFusion = Strict` on every target
//! machine it creates"* — is **unimplementable through LLVM-C**, which exposes
//! no `TargetOptions` surface at all. What holds the policy up instead is two
//! facts: this backend never sets a fast-math flag (`LLVMSetFastMathFlags` is
//! not declared, and `tests/symbols.rs` asserts that it stays undeclared), and
//! LLVM's default `FPOpFusion::Standard` does not enable backend contraction on
//! its own. The second fact is about LLVM's behaviour, not about this crate, so
//! it is the one that has to be measured rather than argued. §7.3's own answer
//! to an obligation that is *"correct by not being written, which means nothing
//! tests it"* is **write the test**, and this is it.
//!
//! **`x86-64-v3`, and that is load-bearing.** Decision 38 makes the default CPU
//! the triple's baseline, which is `x86-64` — SSE2, with no FMA unit. A
//! contraction test at the baseline passes on a backend that contracts
//! everything, because there is no instruction to contract into. So the machine
//! here is built at `x86-64-v3` explicitly, and the second test below checks
//! that the CPU name is doing something, by asserting the baseline and v3
//! produce *different* assembly for the same module.
//!
//! **The cost.** This asserts about x86-64 and nothing else. An AArch64 host
//! would need `LLVMInitializeAArch64*` in `crate::sys` first; `machine`'s
//! `initialise` says so.

#![cfg(feature = "llvm")]

use science_codegen::layout::Triple;
use science_codegen::target::{OptLevel, TargetConfig};
use science_codegen_llvm::owned::{Builder, Module, cstr};
use science_codegen_llvm::{machine, sys};

/// `double fused(double a, double b, double c) { return a * b + c; }`, built
/// through [`science_codegen_llvm::sys`] directly.
///
/// **Hand-built rather than lowered from Science, and that is deliberate.** The
/// question is what the *target machine* does with an `fmul` and an `fadd`, and
/// `science_codegen::backend::Operand` has no form for a parameter — so a body
/// lowered through [`science_codegen_llvm::emit`] could only reach three
/// operands as constants, which `-O3` folds to one number before the back end
/// ever sees a multiply. Parameters are what make the arithmetic survive to
/// instruction selection.
fn fused_module(context: &science_codegen_llvm::owned::Context) -> Module {
    let module = Module::new(context, "float_policy");
    let builder = Builder::new(context);
    unsafe {
        let double = sys::LLVMDoubleTypeInContext(context.raw());
        let mut params = [double, double, double];
        let ty = sys::LLVMFunctionType(double, params.as_mut_ptr(), 3, 0);
        let name = cstr("fused");
        let function = sys::LLVMAddFunction(module.raw(), name.as_ptr(), ty);
        let entry = cstr("entry");
        let block = sys::LLVMAppendBasicBlockInContext(context.raw(), function, entry.as_ptr());
        sys::LLVMPositionBuilderAtEnd(builder.raw(), block);
        let a = sys::LLVMGetParam(function, 0);
        let b = sys::LLVMGetParam(function, 1);
        let c = sys::LLVMGetParam(function, 2);
        let product = cstr("product");
        let sum = cstr("sum");
        // No flags on either instruction, here or anywhere: §7.3 obligation 1.
        let mul = sys::LLVMBuildFMul(builder.raw(), a, b, product.as_ptr());
        let add = sys::LLVMBuildFAdd(builder.raw(), mul, c, sum.as_ptr());
        sys::LLVMBuildRet(builder.raw(), add);
    }
    module
}

fn assembly_for(cpu: &str) -> String {
    let triple = Triple::host().expect("a supported host");
    let config = TargetConfig::new(triple, OptLevel::O3);
    let context = machine::context();
    let module = fused_module(&context);
    let target = machine::create(&config, cpu, "").expect("a target machine");
    machine::describe(&module, &target, triple);
    assert_eq!(module.verify(), Ok(()), "the fixture module must verify");
    machine::optimise(&module, &target, OptLevel::O3).expect("the `default<O3>` pipeline");

    // **A counter, not the CPU name.** Two tests in this file ask for
    // `x86-64-v3`, so a directory named after the CPU is the *same* directory
    // for both — and `cargo test` runs them on two threads, so one deletes the
    // scratch directory while the other is reading the file it just wrote.
    // The symptom is an intermittent "path not found" on a test that has
    // nothing to do with paths, which is worth a `static` to be rid of.
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "science-fp-{}-{}-{}",
        std::process::id(),
        cpu.replace('-', "_"),
        unique
    ));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let path = dir.join("fused.s");
    machine::emit_to_file(&module, &target, &path, sys::file_type::ASSEMBLY).expect("assembly");
    let text = std::fs::read_to_string(&path).expect("the assembly is text");
    let _ = std::fs::remove_dir_all(&dir);
    text
}

/// Decision 36's own stated test, run against instructions.
#[test]
fn a_times_b_plus_c_is_not_contracted_on_a_cpu_that_has_an_fma_unit() {
    let assembly = assembly_for("x86-64-v3");
    assert!(
        !assembly.to_lowercase().contains("vfmadd")
            && !assembly.to_lowercase().contains("vfmsub"),
        "an FMA instruction reached the output. `reproducibility.md` Decision 3 forbids every \
         value-changing float transform, and contraction changes the answer by removing one \
         rounding step:\n{assembly}"
    );
    assert!(
        assembly.contains("mulsd") || assembly.contains("vmulsd"),
        "there is no multiply in the output at all, so this test is measuring nothing:\n\
         {assembly}"
    );
    assert!(
        assembly.contains("addsd") || assembly.contains("vaddsd"),
        "there is no add in the output, so the two operations were folded somewhere this test \
         cannot see:\n{assembly}"
    );
}

/// The test above is only meaningful if the CPU name reaches the back end.
///
/// `machine`'s own note says a contraction test at Decision 38's baseline
/// *"passes on a backend that contracts everything, because there is no
/// instruction to contract into"*. This is the check that `x86-64-v3` is not
/// being ignored: the two CPUs must produce different assembly for one module.
/// On x86-64 they do — v3 implies AVX, so the multiply is `vmulsd` rather than
/// `mulsd`.
#[test]
fn the_target_cpu_reaches_instruction_selection() {
    let baseline = assembly_for(Triple::host().expect("a host").baseline_cpu());
    let v3 = assembly_for("x86-64-v3");
    assert_ne!(
        baseline, v3,
        "the baseline CPU and `x86-64-v3` produced identical assembly, so the CPU string is \
         not reaching `LLVMCreateTargetMachine` and the contraction test above proves nothing"
    );
    assert!(
        v3.contains("vmulsd"),
        "`x86-64-v3` did not select the AVX encoding, so the feature level is not what it says:\
         \n{v3}"
    );
}

/// Decision 35's grep, over the IR the pipeline produced rather than over the
/// IR the emitter wrote.
///
/// `emit_file_to` runs this check inside the compiler, on every build. Running
/// it here as well is not duplication: the compiler checks its own output at
/// `-O2`, and this checks the `-O3` pipeline on a module that has floating-point
/// arithmetic in it, which no compiled Science program does yet.
#[test]
fn the_o3_pipeline_adds_no_fast_math_flag_to_float_arithmetic() {
    let triple = Triple::host().expect("a supported host");
    let config = TargetConfig::new(triple, OptLevel::O3);
    let context = machine::context();
    let module = fused_module(&context);
    let target = machine::create(&config, "x86-64-v3", "").expect("a target machine");
    machine::describe(&module, &target, triple);
    machine::optimise(&module, &target, OptLevel::O3).expect("the pipeline");
    let ir = module.print();
    assert_eq!(
        science_codegen::target::first_fast_math_flag(&ir),
        None,
        "the optimiser attached a fast-math flag to IR that had none:\n{ir}"
    );
}
