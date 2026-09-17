//! Decision 42's one non-negotiable agreement, checked against LLVM itself.
//!
//! **The decision.** For every aggregate in `science-rt`'s ABI, the size and
//! alignment `science_codegen::layout` computes are compared against the size
//! and alignment **LLVM's own `DataLayout`** gives the type
//! [`science_codegen_llvm::emit::LlvmBackend::llvm_type`] builds from it.
//!
//! **The reason.** Decision 42 says a second backend *"must agree about struct
//! offsets"*, and `emit`'s §1 explains why this crate materialises padding
//! itself rather than letting LLVM insert it: *"the day they disagree, the
//! disagreement is silent"* — `science-codegen`'s layout would be in the
//! descriptor handed to `science-rt` and LLVM's would be in the emitted
//! addressing, and nothing would report it. So the agreement is asserted rather
//! than assumed, and it is asserted against the only other implementation on
//! this machine that can be asked at run time.
//!
//! This makes **three** independent implementations of §3.2 in the workspace:
//! `science_codegen::layout`, `rustc`'s `#[repr(C)]` (which
//! `science-rt/tests/layout.rs` pins), and LLVM's `DataLayout` (here).
//!
//! **The cost.** LLVM's `DataLayout` is derived from the target machine, so this
//! checks the host triple and no other. §5.6 makes cross-compilation `SC0406`,
//! so the host is the only triple that can be built for anyway; the two other
//! triples' layouts are `science-codegen`'s tests' to hold.

#![cfg(feature = "llvm")]

use science_codegen::layout::{Triple, layout_of};
use science_codegen::runtime::RtAggregate;
use science_codegen::target::{OptLevel, TargetConfig};
use science_codegen_llvm::emit::LlvmBackend;
use science_codegen_llvm::machine;

fn host() -> Triple {
    Triple::host().expect("`Triple::host` knows this machine; §5.6 makes anything else SC0406")
}

#[test]
fn llvm_and_science_codegen_agree_on_every_runtime_aggregate() {
    let triple = host();
    let config = TargetConfig::new(triple, OptLevel::O0);
    let mut backend = LlvmBackend::new();
    science_codegen::backend::Backend::begin_module(&mut backend, "layout", &config)
        .expect("a module");
    let target = backend.machine().expect("a target machine");
    let data = target.data_layout();

    for aggregate in RtAggregate::ALL {
        let layout = layout_of(triple, &aggregate.cg_ty());
        let ty = backend.llvm_type(&layout);
        // SAFETY: `ty` was built from `backend`'s context, which is alive for
        // the whole of this function and outlives `data`.
        let (size, align) = unsafe { (data.size_of(ty), data.align_of(ty)) };
        assert_eq!(
            size,
            layout.size,
            "`{}`: `science-codegen` says {} bytes and LLVM says {size}. Decision 42 makes this \
             the disagreement a second backend may not have — the descriptor handed to \
             `science-rt` carries the first number and the emitted code uses the second.",
            aggregate.name(),
            layout.size
        );
        assert_eq!(
            align,
            layout.align,
            "`{}`: alignment {} against LLVM's {align}",
            aggregate.name(),
            layout.align
        );
    }
}

/// The scalars, which is where a disagreement would be a data-layout-string bug
/// rather than a struct-packing one.
///
/// `i1` is deliberately absent: §3.1 makes `Bool` *"`i1` in registers and `i8`
/// in memory"*, and `LLVMABISizeOfType(i1)` is 1 byte — the memory form — which
/// is what `Scalar::Bool` lays out as and what this compares.
#[test]
fn llvm_and_science_codegen_agree_on_every_scalar() {
    use science_codegen::layout::{CgTy, FloatTy, IntTy, PtrKind};
    let triple = host();
    let config = TargetConfig::new(triple, OptLevel::O0);
    let mut backend = LlvmBackend::new();
    science_codegen::backend::Backend::begin_module(&mut backend, "scalars", &config)
        .expect("a module");
    let data = backend.machine().expect("a machine").data_layout();

    let scalars = [
        CgTy::Bool,
        CgTy::Char,
        CgTy::Int(IntTy::I8),
        CgTy::Int(IntTy::I16),
        CgTy::Int(IntTy::I32),
        CgTy::Int(IntTy::I64),
        CgTy::Int(IntTy::U8),
        CgTy::Int(IntTy::U16),
        CgTy::Int(IntTy::U32),
        CgTy::Int(IntTy::U64),
        CgTy::Int(IntTy::Usize),
        CgTy::Int(IntTy::Isize),
        CgTy::Float(FloatTy::F32),
        CgTy::Float(FloatTy::F64),
        CgTy::Ptr(PtrKind::Raw),
        CgTy::Ptr(PtrKind::Box),
        CgTy::Interface,
    ];
    for ty in scalars {
        let layout = layout_of(triple, &ty);
        let llvm = backend.llvm_type(&layout);
        // SAFETY: as above.
        let (size, align) = unsafe { (data.size_of(llvm), data.align_of(llvm)) };
        assert_eq!(size, layout.size, "{ty:?}: {} against LLVM's {size}", layout.size);
        assert_eq!(align, layout.align, "{ty:?}: {} against LLVM's {align}", layout.align);
    }
}

/// The module's `datalayout` string is LLVM's own, not one this crate wrote.
///
/// `emit`'s §1 says the layout is `science-codegen`'s and *LLVM is told, not
/// asked*; the string is the telling, and it comes from
/// `LLVMCopyStringRepOfTargetData` so that the two cannot drift by a typo.
#[test]
fn the_data_layout_string_is_the_target_machines_own() {
    let triple = host();
    let config = TargetConfig::new(triple, OptLevel::O0);
    let machine = machine::for_config(&config).expect("a target machine");
    let text = machine.data_layout().as_string();
    assert!(!text.is_empty(), "LLVM gave an empty `datalayout` string");
    assert!(
        text.contains("i64:64"),
        "the `datalayout` string does not look like an x86-64 one: {text}"
    );
    // And the triple LLVM would have chosen on its own agrees with the one
    // `Triple::host` named, which is the check that `SC0406`'s "the host" means
    // the same thing on both sides of Decision 42's line.
    let default = machine::default_triple();
    assert!(
        default.starts_with(triple.as_str().split('-').next().expect("an architecture")),
        "`Triple::host` says `{}` and LLVM says `{default}`",
        triple.as_str()
    );
}
