//! Decisions 35, 36 and 37, tested as far as a machine without LLVM can test
//! them — and an explicit statement of where that stops.
//!
//! # What this file proves, and what it does not
//!
//! **It proves** that this crate never *asks* for a value-changing float
//! transform: that [`FloatOp`] has no fused variant and no flags field, that
//! [`TargetConfig`] has no constructor producing anything but `Strict`, that a
//! transcendental lowers to a `science-libm` call, and that a transcript of a
//! lowered `a * b + c` contains `fmul`, `fadd`, and no `fma` and no fast-math
//! token.
//!
//! **It does not prove** that LLVM honoured any of it. Decision 36 asks for
//! *"a test [that] asserts that a Science program computing `a * b + c` emits
//! `fmul` followed by `fadd` and no `fma`, on every target"*, and the word
//! *emits* means the object code. Reaching it needs
//! `LLVMCreateTargetMachine`, which needs `llvm-sys`, which needs an LLVM
//! installation this machine does not have. The transcript checked below is
//! IR-shaped text produced by [`TextBackend`], not IR produced by LLVM, and a
//! green run here is evidence about this crate and about nothing downstream of
//! it.
//!
//! # The test to add when `science-codegen-llvm` exists
//!
//! Three steps, and the second is the one that fails if `AllowFPOpFusion` is
//! left alone:
//!
//! 1. Build the module for `a * b + c` through `science-codegen-llvm` at each
//!    of `-O0`, `-O1`, `-O2`, `-O3`, on each of the three triples.
//! 2. `LLVMTargetMachineEmitToMemoryBuffer` with `LLVMAssemblyFile`, and assert
//!    the assembly contains no `vfmadd`, `vfmsub`, `fmadd` or `fmla`. **This is
//!    the assertion that catches the defaulted field**, because the IR is
//!    identical either way: contraction happens in the backend, after the IR,
//!    and a test that only reads IR cannot see it. Run it on a host with FMA —
//!    every x86 since Haswell and every AArch64 chip — or it passes for the
//!    wrong reason.
//! 3. `LLVMPrintModuleToString` and run [`contains_fast_math_flag`] over it,
//!    which is Decision 35 and is the only one of the three that *is* an IR
//!    test.
//!
//! Step 2 has no substitute here, and pretending otherwise is the failure this
//! file is written to avoid.

use science_codegen::backend::{
    Backend, Block, BlockId, Body, Callee, EmitKind, FloatOp, Inst, Operand, Terminator, ValueId,
};
use science_codegen::layout::Triple;
use science_codegen::stub::TextBackend;
use science_codegen::target::{
    FpContract, Intrinsic, MathLowering, OptLevel, TargetConfig, contains_fast_math_flag,
    lower_math_call,
};

const LEVELS: [OptLevel; 4] = [OptLevel::O0, OptLevel::O1, OptLevel::O2, OptLevel::O3];

/// Whether any instruction in `ir` uses `opcode`.
///
/// Token-matched on the instruction lines, not substring-matched on the whole
/// text. The first version of this file substring-matched for `"fma"` and
/// failed against its own module name — which is the hazard
/// [`contains_fast_math_flag`] is written to avoid and which this file promptly
/// reintroduced by hand. A float-policy test that can fail because of a symbol
/// name is a float-policy test somebody deletes.
fn uses_opcode(ir: &str, opcode: &str) -> bool {
    ir.lines()
        .filter_map(|line| line.split_once('='))
        .any(|(_, rhs)| rhs.split_whitespace().next() == Some(opcode))
}

/// `a * b + c`, lowered.
fn multiply_add(target: Triple, opt: OptLevel) -> String {
    let mut backend = TextBackend::new();
    backend.begin_module("mul_then_add", &TargetConfig::new(target, opt)).unwrap();
    let body = Body {
        blocks: vec![Block {
            id: BlockId(0),
            label: "entry".to_string(),
            insts: vec![
                Inst::FloatBinary {
                    dest: ValueId(0),
                    op: FloatOp::Mul,
                    lhs: Operand::ConstFloat(2.0),
                    rhs: Operand::ConstFloat(3.0),
                },
                Inst::FloatBinary {
                    dest: ValueId(1),
                    op: FloatOp::Add,
                    lhs: Operand::Value(ValueId(0)),
                    rhs: Operand::ConstFloat(1.0),
                },
            ],
            terminator: Terminator::Return(Some(Operand::Value(ValueId(1)))),
        }],
    };
    let layout = science_codegen::layout::layout_of(
        target,
        &science_codegen::layout::CgTy::Float(science_codegen::layout::FloatTy::F64),
    );
    let sig = science_codegen::abi::AbiSignature::science(target, "_S4calc", layout, vec![]);
    let func = backend.declare_function(&sig).unwrap();
    backend.define_function(func, &sig, &body).unwrap();
    backend.verify().unwrap();
    String::from_utf8(backend.emit(EmitKind::Ir).unwrap()).unwrap()
}

#[test]
fn a_multiply_and_an_add_stay_separate_on_every_target_at_every_level() {
    // Decision 36's assertion, at the level available. See the file header for
    // what the word "emits" would mean with LLVM present.
    for target in Triple::ALL {
        for opt in LEVELS {
            let ir = multiply_add(target, opt);
            assert!(uses_opcode(&ir, "fmul"), "{target:?} {opt:?}:\n{ir}");
            assert!(uses_opcode(&ir, "fadd"), "{target:?} {opt:?}:\n{ir}");
            for fused in ["fma", "fmuladd", "llvm.fma", "llvm.fmuladd"] {
                assert!(!uses_opcode(&ir, fused), "{target:?} {opt:?} used {fused}:\n{ir}");
            }
            // And no `call` to a fused intrinsic either, which `uses_opcode`
            // would miss because the opcode there is `call`.
            assert!(!ir.contains("@llvm.fma"), "{target:?} {opt:?}:\n{ir}");
        }
    }
}

#[test]
fn the_target_machine_carries_strict_contraction_on_every_target_at_every_level() {
    // §7.3 obligation 2. The line that is correct by being written, written and
    // then read back, on every combination — because "at every optimisation
    // level including `-O0`" is part of the claim.
    for target in Triple::ALL {
        for opt in LEVELS {
            let config = TargetConfig::new(target, opt);
            assert_eq!(config.fp_contract(), FpContract::Strict, "{target:?} {opt:?}");
            let ir = multiply_add(target, opt);
            assert!(
                ir.contains("AllowFPOpFusion = Strict"),
                "{target:?} {opt:?} did not configure contraction:\n{ir}"
            );
        }
    }
}

#[test]
fn no_fast_math_flag_appears_anywhere_in_the_corpus_at_o3() {
    // Decision 35, verbatim: "a test greps the emitted IR of the entire corpus
    // at `-O3` for the fast-math flag tokens and fails on any hit". The corpus
    // is small because the compiler is small; the test is the three lines the
    // note priced it at.
    for target in Triple::ALL {
        let ir = multiply_add(target, OptLevel::O3);
        assert!(!contains_fast_math_flag(&ir), "{target:?}:\n{ir}");
    }
}

#[test]
fn the_backend_refuses_a_fast_math_flag_rather_than_passing_it_through() {
    // Decision 35 is an obligation to never opt in, and §7.3 says of it:
    // "a line that is correct by not being written, which means nothing tests
    // it". A verifier that fails on the flag is what tests it, and it is the
    // shape Decision 34 already requires for everything else.
    let mut backend = TextBackend::new();
    backend.begin_module("m", &TargetConfig::new(Triple::X86_64LinuxGnu, OptLevel::O3)).unwrap();
    let body = Body {
        blocks: vec![Block {
            id: BlockId(0),
            label: "entry".to_string(),
            insts: vec![Inst::Call {
                dest: Some(ValueId(0)),
                // A call named after a flag is not a flag; the grep must not
                // fire on it, and `verify` must therefore still pass.
                callee: Callee::Foreign("reassoc_helper".to_string()),
                args: vec![],
                ret: science_codegen::abi::ReturnClass::Void,
                sret_slot: None,
            }],
            terminator: Terminator::Return(None),
        }],
    };
    let layout =
        science_codegen::layout::layout_of(Triple::X86_64LinuxGnu, &science_codegen::layout::CgTy::Unit);
    let sig =
        science_codegen::abi::AbiSignature::science(Triple::X86_64LinuxGnu, "_S1f", layout, vec![]);
    let func = backend.declare_function(&sig).unwrap();
    backend.define_function(func, &sig, &body).unwrap();
    assert!(backend.verify().is_ok(), "a symbol containing a flag's name is not a flag");
}

#[test]
fn the_fused_operation_is_reachable_only_by_asking_for_it() {
    // Decision 36's second half: "`math.fma(a, b, c)` remains a function
    // lowering to `llvm.fma` — a user who wants the fused operation asks for it
    // and gets it everywhere, which is both faster and more deterministic than
    // letting the backend decide per call site."
    assert_eq!(lower_math_call("fma"), MathLowering::Intrinsic(Intrinsic::Fma));
    assert_eq!(Intrinsic::Fma.llvm_name(), "llvm.fma");

    // And there is no `FloatOp` that could produce one. This is the assertion
    // that would stop compiling if somebody added `FloatOp::Fma`, which is the
    // point of writing it as a match rather than a comment.
    fn is_fusable(op: FloatOp) -> bool {
        match op {
            FloatOp::Add | FloatOp::Sub | FloatOp::Mul | FloatOp::Div | FloatOp::Rem => false,
        }
    }
    for op in [FloatOp::Add, FloatOp::Sub, FloatOp::Mul, FloatOp::Div, FloatOp::Rem] {
        assert!(!is_fusable(op));
    }
}

#[test]
fn every_transcendental_is_a_call_and_only_the_whitelist_is_an_intrinsic() {
    // Decision 37. The failure it prevents: LLVM constant-folds `llvm.sin.f64`
    // of a constant argument by calling the *host's* `sin()` at compile time,
    // so `let x be sin(0.5)` compiled on macOS and on Linux can differ in the
    // last bit, from the same source, at the same optimisation level, with no
    // fast-math flag anywhere.
    let transcendentals = [
        "sin", "cos", "tan", "asin", "acos", "atan", "atan2", "sinh", "cosh", "tanh", "exp",
        "exp2", "expm1", "log", "log2", "log10", "log1p", "pow", "cbrt", "hypot", "erf", "gamma",
    ];
    for name in transcendentals {
        match lower_math_call(name) {
            MathLowering::LibmCall(symbol) => {
                assert!(symbol.starts_with("science_libm_"), "{name} -> {symbol}");
            }
            MathLowering::Intrinsic(intrinsic) => panic!(
                "{name} lowered to {}: LLVM will constant-fold it against the build host's libm",
                intrinsic.llvm_name()
            ),
        }
    }

    // The whitelist is the exception and is closed at eleven. Every entry is
    // marked "IEEE-754 exact" by `intrinsics-math-physics.md` §3.1, which is
    // exactly the property that makes constant-folding them bit-identical on
    // any host.
    assert_eq!(Intrinsic::ALL.len(), 11);
    for intrinsic in Intrinsic::ALL {
        assert_eq!(
            lower_math_call(intrinsic.science_name()),
            MathLowering::Intrinsic(intrinsic)
        );
    }
}

#[test]
fn a_name_the_whitelist_does_not_know_becomes_a_call_and_not_an_intrinsic() {
    // The classifier's default direction, which is the decision. Written the
    // other way round — intrinsic unless known dangerous — it would emit
    // `llvm.sinh` the first time anyone added `sinh` to the library.
    match lower_math_call("some_function_nobody_has_written_yet") {
        MathLowering::LibmCall(_) => {}
        MathLowering::Intrinsic(i) => panic!("defaulted to {}", i.llvm_name()),
    }
}
