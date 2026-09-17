//! What the emitter does with an instruction, for the shapes that were wrong.
//!
//! **The decision.** Each test builds one function out of
//! [`science_codegen_llvm::emit::ExtInst`] values, emits it, and reads the IR.
//! Nothing is lowered from Science, because none of these shapes is reachable
//! from a Science program this compiler can compile.
//!
//! **The reason, and it is why the file exists at all.** Stage 1 lowers a script
//! body, a string literal and `print`. Everything else in the instruction set —
//! integer arithmetic, float arithmetic, comparisons, `switch` — is *implemented
//! and unreachable*, which is the state in which a defect survives: the module
//! `hello, world` builds does not contain one integer add, so nothing about the
//! emitter's arithmetic was ever executed. Two defects were found this way and
//! both are pinned below.
//!
//! **The cost.** These bodies are not bodies any front end produces, so a test
//! here can pass while the lowering that would produce the same shape is wrong.
//! `tests/hello.rs` is the other half and it is the one with the exit code in it.

#![cfg(feature = "llvm")]

use science_codegen::abi::AbiSignature;
use science_codegen::backend::{
    Backend, BlockId, CmpOp, FloatOp, Inst, IntOp, LocalId, Operand, Terminator, ValueId,
};
use science_codegen::layout::{CgTy, FloatTy, IntTy, Triple, layout_of};
use science_codegen::target::{OptLevel, TargetConfig};
use science_codegen_llvm::emit::{ExtBlock, ExtBody, ExtInst, LlvmBackend};

fn triple() -> Triple {
    Triple::host().expect("a supported host")
}

/// One `void f()` with the given instructions in its entry block, emitted and
/// printed.
fn emit(insts: Vec<ExtInst>, terminator: Terminator) -> String {
    let config = TargetConfig::new(triple(), OptLevel::O0);
    let mut backend = LlvmBackend::new();
    backend.begin_module("emitter", &config).expect("a module");
    let signature =
        AbiSignature::science(triple(), "_S1f", layout_of(triple(), &CgTy::Unit), vec![]);
    let id = backend.declare_function(&signature).expect("a declaration");
    let body = ExtBody {
        blocks: vec![ExtBlock {
            id: BlockId(0),
            label: "entry".to_string(),
            insts,
            terminator,
        }],
    };
    backend.define_function_ext(id, &signature, &body).expect("a definition");
    backend.verify().expect("the module verifies");
    backend.ir()
}

/// A local of type `ty`, and a value loaded out of it, as the first two
/// instructions of a body. Returns the instructions and the loaded `ValueId`.
fn loaded(ty: CgTy) -> (Vec<ExtInst>, ValueId) {
    let layout = layout_of(triple(), &ty);
    let local = LocalId(0);
    let value = ValueId(0);
    (
        vec![
            ExtInst::Above(Inst::Alloca { local, layout }),
            ExtInst::Above(Inst::Load { dest: value, local }),
        ],
        value,
    )
}

/// **Defect 1.** An integer constant took the width of the other operand, and
/// did not.
///
/// `IntBinary` passed no expectation for either side, so
/// `LlvmBackend::operand` gave every `Operand::ConstInt` its default `i64`.
/// `x + 1` on an `I32` therefore built `add i32 %v0, i64 1`, which the verifier
/// rejects — so it is loud, and it makes every non-`Int` arithmetic expression
/// in the language uncompilable the day one is lowered.
#[test]
fn an_integer_constant_takes_the_width_of_the_value_it_is_combined_with() {
    for (int, width) in
        [(IntTy::I8, "i8"), (IntTy::I16, "i16"), (IntTy::I32, "i32"), (IntTy::U32, "i32")]
    {
        let (mut insts, value) = loaded(CgTy::Int(int));
        insts.push(ExtInst::Above(Inst::IntBinary {
            dest: ValueId(1),
            op: IntOp::Add,
            lhs: Operand::Value(value),
            rhs: Operand::ConstInt(1),
        }));
        let ir = emit(insts, Terminator::Return(None));
        assert!(
            ir.contains(&format!("add {width} %v0, 1")),
            "an `{int:?}` add did not produce `add {width}`:\n{ir}"
        );
        assert!(!ir.contains("i64 1"), "the constant kept its default width:\n{ir}");
    }
}

/// The same defect on the constant's other side, and with the constant first.
#[test]
fn the_hint_is_read_from_whichever_operand_is_a_value() {
    let (mut insts, value) = loaded(CgTy::Int(IntTy::I32));
    insts.push(ExtInst::Above(Inst::IntBinary {
        dest: ValueId(1),
        op: IntOp::Sub,
        lhs: Operand::ConstInt(7),
        rhs: Operand::Value(value),
    }));
    let ir = emit(insts, Terminator::Return(None));
    assert!(ir.contains("sub i32 7, %v0"), "{ir}");
}

/// **Defect 1, the float half.** `FloatBinary` passed `double` for both sides,
/// so an `F32` expression built `fadd float %v0, double 1.0`.
#[test]
fn a_float_constant_takes_the_width_of_the_value_it_is_combined_with() {
    for (float, width) in [(FloatTy::F32, "float"), (FloatTy::F64, "double")] {
        let (mut insts, value) = loaded(CgTy::Float(float));
        insts.push(ExtInst::Above(Inst::FloatBinary {
            dest: ValueId(1),
            op: FloatOp::Add,
            lhs: Operand::Value(value),
            rhs: Operand::ConstFloat(1.0),
        }));
        let ir = emit(insts, Terminator::Return(None));
        assert!(
            ir.contains(&format!("fadd {width} %v0,")),
            "an `{float:?}` add did not produce `fadd {width}`:\n{ir}"
        );
        // §7.3 obligation 1, on the instruction that could carry one.
        assert_eq!(science_codegen::target::first_fast_math_flag(&ir), None, "{ir}");
    }
}

/// **Defect 2.** `Cmp` asked only the **left** operand whether it was a float.
///
/// `science_codegen::backend::Inst::Cmp` carries `signed: bool` and says
/// *"irrelevant for floats"*, leaving the backend to work out which comparison
/// it is building. Reading `lhs` alone answers "integer" for
/// `Cmp { lhs: ConstFloat, .. }` only by accident — the default type of a float
/// constant is `double`, so that case happened to work — and answers wrong the
/// moment the value is on the right of a constant.
#[test]
fn a_comparison_picks_fcmp_from_either_operand() {
    for (float, width) in [(FloatTy::F32, "float"), (FloatTy::F64, "double")] {
        let (mut insts, value) = loaded(CgTy::Float(float));
        insts.push(ExtInst::Above(Inst::Cmp {
            dest: ValueId(1),
            op: CmpOp::Lt,
            signed: true,
            lhs: Operand::ConstFloat(0.0),
            rhs: Operand::Value(value),
        }));
        let ir = emit(insts, Terminator::Return(None));
        assert!(
            ir.contains(&format!("fcmp olt {width}")),
            "a float comparison with the constant on the left built the wrong instruction:\n{ir}"
        );
        assert!(!ir.contains("icmp"), "{ir}");
    }
}

/// The integer comparisons still pick `icmp`, and the signedness still reaches
/// the predicate. The float fix must not have swallowed the integer path.
#[test]
fn an_integer_comparison_still_picks_icmp_and_respects_signedness() {
    for (signed, mnemonic) in [(true, "slt"), (false, "ult")] {
        let (mut insts, value) = loaded(CgTy::Int(IntTy::I32));
        insts.push(ExtInst::Above(Inst::Cmp {
            dest: ValueId(1),
            op: CmpOp::Lt,
            signed,
            lhs: Operand::Value(value),
            rhs: Operand::ConstInt(3),
        }));
        let ir = emit(insts, Terminator::Return(None));
        assert!(ir.contains(&format!("icmp {mnemonic} i32 %v0, 3")), "{ir}");
    }
}

/// A `switch` types its case constants from the value it switches on.
///
/// This was reported as a defect and is not one: `emit_terminator` already read
/// `LLVMTypeOf(discr)` and built each case against it, and the comment beside it
/// gives the reason — Decision 18 makes a discriminant `u8` up to 256 variants,
/// so a hard-coded `i64` would fail the verifier on every `match` in the
/// language. The test is here because "already correct" and "checked" are
/// different states, and nothing had ever built a `switch`.
#[test]
fn a_switch_types_its_cases_from_the_value_it_switches_on() {
    for (int, width) in [(IntTy::U8, "i8"), (IntTy::U16, "i16"), (IntTy::U32, "i32")] {
        let (mut insts, value) = loaded(CgTy::Int(int));
        let config = TargetConfig::new(triple(), OptLevel::O0);
        let mut backend = LlvmBackend::new();
        backend.begin_module("switch", &config).expect("a module");
        let signature =
            AbiSignature::science(triple(), "_S1f", layout_of(triple(), &CgTy::Unit), vec![]);
        let id = backend.declare_function(&signature).expect("a declaration");
        insts.push(ExtInst::Above(Inst::Alloca {
            local: LocalId(1),
            layout: layout_of(triple(), &CgTy::Unit),
        }));
        let body = ExtBody {
            blocks: vec![
                ExtBlock {
                    id: BlockId(0),
                    label: "entry".to_string(),
                    insts,
                    terminator: Terminator::Switch {
                        value: Operand::Value(value),
                        arms: vec![(0, BlockId(1)), (1, BlockId(2))],
                        default: BlockId(2),
                    },
                },
                ExtBlock {
                    id: BlockId(1),
                    label: "a".to_string(),
                    insts: vec![],
                    terminator: Terminator::Return(None),
                },
                ExtBlock {
                    id: BlockId(2),
                    label: "b".to_string(),
                    insts: vec![],
                    terminator: Terminator::Return(None),
                },
            ],
        };
        backend.define_function_ext(id, &signature, &body).expect("a definition");
        backend.verify().expect("the module verifies");
        let ir = backend.ir();
        assert!(ir.contains(&format!("switch {width} %v0")), "{ir}");
        assert!(ir.contains(&format!("{width} 0, label")), "a case kept the wrong width:\n{ir}");
        assert!(ir.contains(&format!("{width} 1, label")), "{ir}");
    }
}

/// A branch on a value that is not `i1` is refused rather than emitted.
///
/// §3.1 makes `Bool` *"`i1` in registers and `i8` in memory"*, so a condition
/// loaded from a `Bool` local is an `i8` and `br i8` is invalid. The repair is a
/// `trunc` and `crate::sys` declares none — *"declare only what you call"* — so
/// the emitter says which line is missing instead of building something the
/// verifier would reject with a less useful message.
#[test]
fn a_branch_on_a_loaded_bool_is_refused_by_name() {
    let (mut insts, value) = loaded(CgTy::Bool);
    insts.push(ExtInst::Above(Inst::Alloca {
        local: LocalId(1),
        layout: layout_of(triple(), &CgTy::Unit),
    }));
    let config = TargetConfig::new(triple(), OptLevel::O0);
    let mut backend = LlvmBackend::new();
    backend.begin_module("branch", &config).expect("a module");
    let signature =
        AbiSignature::science(triple(), "_S1f", layout_of(triple(), &CgTy::Unit), vec![]);
    let id = backend.declare_function(&signature).expect("a declaration");
    let body = ExtBody {
        blocks: vec![
            ExtBlock {
                id: BlockId(0),
                label: "entry".to_string(),
                insts,
                terminator: Terminator::Branch {
                    cond: Operand::Value(value),
                    then_block: BlockId(1),
                    else_block: BlockId(1),
                },
            },
            ExtBlock {
                id: BlockId(1),
                label: "done".to_string(),
                insts: vec![],
                terminator: Terminator::Return(None),
            },
        ],
    };
    let error = backend
        .define_function_ext(id, &signature, &body)
        .expect_err("an `i8` condition is not a branch condition");
    assert!(
        error.to_string().contains("trunc"),
        "the refusal must name the instruction that is missing, and it said: {error}"
    );
}
