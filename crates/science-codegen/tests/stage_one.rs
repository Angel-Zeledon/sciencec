//! `print("hello, world")` — as a lowering, not as a program.
//!
//! **What this is.** §10's stage 1 lists what hello world requires and calls it
//! *"the whole list"*: the script body lowered to `def main() -> Error?`; a
//! string literal as a `private unnamed_addr constant` plus a
//! `science_string_from_bytes` call (Decision 15); a `science_print` call; drop
//! glue for one `String`; and the `sret` convention, immediately, because
//! `science_string_from_bytes` returns `ScienceString` by value.
//!
//! This file builds that lowering by hand and asserts each item. It is the
//! above-the-line half of stage 1 with the below-the-line half replaced by a
//! transcript.
//!
//! **What it is not.** Stage 1's gate is *"the binary runs, prints
//! `hello, world\n`, exits 0, and the emitted IR at `-O3` contains no fast-math
//! flag"*. Three of those four need a backend and a linker. **Stage 1 is not
//! passed, stage 0 is not passed, and no Science program has been compiled or
//! run.** The fourth is checked in `tests/float_policy.rs`.
//!
//! **Why build it at all.** Because the interface is the deliverable, and an
//! interface nobody has driven end to end is an interface with a hole in it.
//! Writing this found two: that [`AbiSignature::params`] must not contain the
//! `sret` pointer (a backend would have doubled it) and that a `Call` needs a
//! return slot local, not just a classification. Both are fixed above; neither
//! was visible from the classifier's unit tests.
//!
//! **And one thing it cannot do at all**, which is stage 1's other half: `def
//! main() -> Error?` has nothing to lower its failing path to. §9.3's finding 5
//! is that `science-rt` has no symbol that writes to stderr without aborting and
//! none that exits with a chosen status, so `script-mode.md` §2.3's contract —
//! print `error: `, exit **1** — is unimplementable. The last test here asserts
//! that, because a gap nobody asserts is a gap somebody works around.

use science_codegen::abi::{AbiSignature, ArgClass, ReturnClass, classify_return_for};
use science_codegen::backend::{
    Backend, Block, BlockId, Body, Callee, EmitKind, Inst, LocalId, Operand, Terminator, ValueId,
};
use science_codegen::descriptor::{StringLiteral, drop_glue, needs_drop};
use science_codegen::layout::{CgTy, Triple, layout_of};
use science_codegen::mangle::{MonoKey, mangle};
use science_codegen::runtime::{RtAggregate, runtime_fn};
use science_codegen::stub::TextBackend;
use science_codegen::target::{OptLevel, TargetConfig};

fn target() -> Triple {
    Triple::host().unwrap_or(Triple::X86_64LinuxGnu)
}

/// The whole of stage 1's lowering, rendered.
fn hello_world() -> String {
    let t = target();
    let config = TargetConfig::new(t, OptLevel::O2);
    let mut backend = TextBackend::new();
    backend.begin_module("hello", &config).unwrap();

    // 1. The literal's bytes as a `private unnamed_addr constant`.
    let literal = StringLiteral::new(0, "hello, world");
    backend.define_string_bytes(&literal).unwrap();

    // 2. `science_string_from_bytes`, declared with its classified return.
    //    This is the `sret` convention, immediately.
    let string_layout = RtAggregate::String.layout(t);
    let from_bytes = runtime_fn("science_string_from_bytes").unwrap();
    let from_bytes_sig = AbiSignature {
        symbol: from_bytes.symbol.to_string(),
        ret: classify_return_for(t, &string_layout),
        ret_layout: string_layout.clone(),
        params: vec![
            science_codegen::abi::AbiParam {
                name: "bytes".to_string(),
                class: ArgClass::Direct,
                layout: layout_of(t, &CgTy::Ptr(science_codegen::layout::PtrKind::Raw)),
                attrs: science_codegen::abi::ParamAttrs::default(),
            },
            science_codegen::abi::AbiParam {
                name: "len".to_string(),
                class: ArgClass::Direct,
                layout: layout_of(t, &CgTy::Int(science_codegen::layout::IntTy::Usize)),
                attrs: science_codegen::abi::ParamAttrs::default(),
            },
        ],
        foreign: true,
        nounwind: true,
    };
    backend.declare_function(&from_bytes_sig).unwrap();

    // 3. `science_print(borrowed String)`.
    let print_sig = AbiSignature {
        symbol: "science_print".to_string(),
        ret: ReturnClass::Void,
        ret_layout: layout_of(t, &CgTy::Unit),
        params: vec![science_codegen::abi::AbiParam {
            name: "text".to_string(),
            class: ArgClass::Direct,
            layout: layout_of(t, &CgTy::Ptr(science_codegen::layout::PtrKind::Borrow)),
            attrs: science_codegen::abi::borrow_attrs(
                science_codegen::abi::BorrowKind::Shared,
                string_layout.align,
                config.no_noalias(),
            ),
        }],
        foreign: true,
        nounwind: true,
    };
    backend.declare_function(&print_sig).unwrap();

    // 4. Drop glue for the one `String`: a single runtime call.
    let free_sig = AbiSignature {
        symbol: "science_string_free".to_string(),
        ret: ReturnClass::Void,
        ret_layout: layout_of(t, &CgTy::Unit),
        params: vec![science_codegen::abi::AbiParam {
            name: "value".to_string(),
            class: ArgClass::Direct,
            layout: layout_of(t, &CgTy::Ptr(science_codegen::layout::PtrKind::MutBorrow)),
            attrs: science_codegen::abi::borrow_attrs(
                science_codegen::abi::BorrowKind::Exclusive,
                string_layout.align,
                config.no_noalias(),
            ),
        }],
        foreign: true,
        nounwind: true,
    };
    backend.declare_function(&free_sig).unwrap();

    // 5. `main`. §10: "One basic block, no CFG, no generics, no layout beyond
    //    `ScienceString`."
    let main_key = MonoKey::plain(&["main"]);
    let unit = layout_of(t, &CgTy::Unit);
    let main_sig = AbiSignature::science(t, mangle(&main_key), unit, vec![]);
    let slot = LocalId(0);
    let body = Body {
        blocks: vec![Block {
            id: BlockId(0),
            label: "entry".to_string(),
            insts: vec![
                // Decision 8: every local is an `alloca` in the entry block.
                Inst::Alloca { local: slot, layout: string_layout.clone() },
                Inst::Call {
                    dest: None,
                    callee: Callee::Runtime("science_string_from_bytes"),
                    args: vec![
                        Operand::GlobalAddr(literal.bytes_symbol.clone()),
                        Operand::ConstInt(literal.len() as i128),
                    ],
                    ret: classify_return_for(t, &string_layout),
                    sret_slot: Some(slot),
                },
                Inst::Call {
                    dest: None,
                    callee: Callee::Runtime("science_print"),
                    args: vec![Operand::Value(ValueId(0))],
                    ret: ReturnClass::Void,
                    sret_slot: None,
                },
                // The drop, which is not optional: §5.4's "omitting the call is
                // a leak on the error path, which is the path least likely to
                // be exercised" applies to the success path too.
                Inst::Call {
                    dest: None,
                    callee: Callee::Runtime("science_string_free"),
                    args: vec![Operand::Value(ValueId(0))],
                    ret: ReturnClass::Void,
                    sret_slot: None,
                },
            ],
            terminator: Terminator::Return(None),
        }],
    };
    let func = backend.declare_function(&main_sig).unwrap();
    backend.define_function(func, &main_sig, &body).unwrap();

    backend.verify().unwrap();
    String::from_utf8(backend.emit(EmitKind::Ir).unwrap()).unwrap()
}

#[test]
fn stage_ones_whole_list_is_present_in_the_lowering() {
    let ir = hello_world();
    assert!(ir.contains("private unnamed_addr constant [12 x i8]"), "{ir}");
    assert!(ir.contains("@science_string_from_bytes"), "{ir}");
    assert!(ir.contains("@science_print"), "{ir}");
    assert!(ir.contains("@science_string_free"), "{ir}");
    assert!(ir.contains("@_S4main"), "{ir}");
}

#[test]
fn the_literals_construction_is_an_sret_call_and_the_slot_is_passed() {
    // §9.2's failure mode, made visible: an `Indirect` return with no slot
    // renders `<MISSING>` in the transcript, and a run that produced one would
    // be the compiler about to corrupt a register.
    let ir = hello_world();
    assert!(ir.contains("ptr sret %l0"), "{ir}");
    assert!(!ir.contains("MISSING"), "{ir}");
}

#[test]
fn print_takes_a_shared_borrow_and_free_takes_an_exclusive_one() {
    // Decision 24 at a call site. `science_print` reads and `science_string_free`
    // writes, so only the second gets `noalias`.
    let ir = hello_world();
    let print_line = ir.lines().find(|l| l.contains("@science_print")).unwrap();
    assert!(print_line.contains("readonly"), "{print_line}");
    assert!(!print_line.contains("noalias"), "a shared borrow must never be noalias: {print_line}");
    let free_line = ir.lines().find(|l| l.contains("@science_string_free")).unwrap();
    assert!(free_line.contains("noalias"), "{free_line}");
    assert!(!free_line.contains("readonly"), "{free_line}");
}

#[test]
fn a_string_needs_drop_glue_and_it_is_one_runtime_call() {
    // Stage 1's fourth item. A `String` is three words of pointer and lengths
    // in the model, so `needs_drop` is asked about the *nominal* type, which is
    // the seam where a real type checker would supply the `Drop` fact.
    let string = RtAggregate::String.cg_ty();
    assert!(
        !needs_drop(&string, &|_| false),
        "structurally a String owns nothing: its `ptr` is a raw pointer"
    );
    assert!(
        needs_drop(&string, &|name| name == "ScienceString"),
        "and it is the type checker that knows String has a destructor"
    );
    let layout = RtAggregate::String.layout(target());
    let glue = drop_glue(&string, &layout, &MonoKey::plain(&["String"]), &|name| {
        name == "ScienceString"
    });
    assert!(glue.is_some());
}

#[test]
fn the_module_verifies_and_emits_ir_but_refuses_an_executable() {
    // The honest boundary. Stage 0's deliverable is "an executable that exits
    // 0" and this is not it.
    let mut backend = TextBackend::new();
    backend.begin_module("hello", &TargetConfig::new(target(), OptLevel::O2)).unwrap();
    assert!(backend.verify().is_ok());
    assert!(backend.emit(EmitKind::Ir).is_ok());
    assert!(backend.emit(EmitKind::Executable).is_err());
}

#[test]
fn the_lowering_is_byte_identical_across_runs() {
    // Decision 4's sorted emission order and Gate J's requirement, at the only
    // scale available: two runs in one process. It does not test two machines,
    // which is what Gate J actually asks; it tests that nothing in the path
    // iterates a `HashMap`, which `self-hosting.md` §15 names as the unknown
    // source.
    assert_eq!(hello_world(), hello_world());
}

#[test]
fn a_main_that_returns_an_error_has_nothing_to_lower_to() {
    // §9.3's finding 5, asserted rather than remembered. `script-mode.md` §2.3
    // requires a failing script body to print `error: ` to stderr and exit 1.
    // The runtime has `science_print` and `science_write`, which write to
    // stdout; and `science_panic_bytes`, which writes to stderr and then aborts
    // — `SIGABRT` on POSIX, `3` on Windows, and not 1.
    let contract = science_codegen::runtime::EXIT_CONTRACT;
    assert_eq!(contract.required_status, 1);
    assert_eq!(contract.required_stream, "stderr");
    assert_eq!(contract.required_prefix, "error: ");
    assert!(
        !contract.is_satisfiable(),
        "if this fails, `science-rt` has grown an exit path and stage 1's second half is unblocked"
    );
}
