//! Check 2 of `sys.rs` §1, for the claims that are **numbers**: every C enum
//! constant transcribed into `src/sys.rs` is checked against what LLVM does
//! with it.
//!
//! **The decision.** Each constant is passed to the function it belongs to and
//! the resulting IR is read back. Nothing here compares a number to a number.
//!
//! **The reason, and it is the one `sys.rs` states and could not act on.** The
//! headers are not on this machine, so `int_predicate::SLT = 40` is a claim
//! nobody can read. It is also the *worst* kind of claim in the file: a wrong
//! symbol name fails to link, a wrong arity corrupts a register loudly, but a
//! wrong enum constant **compiles, links, verifies and computes the wrong
//! answer**. `sys.rs` says exactly this about the integer predicates —
//! *"writing 0 here for `Eq` would compile, link, verify, and compare floats"* —
//! and about `LLVMTypeKind`, `LLVMLinkage` and `LLVMRealPredicate` in the same
//! words. Every one of those is checked below by building the instruction and
//! asserting the mnemonic LLVM printed.
//!
//! **The cost.** The check is against LLVM's *printer*, so it verifies that the
//! constant selects the operation whose name the IR uses, not that the operation
//! does what its name says. That is the right boundary: what the operation does
//! is LLVM's contract, and what the constant selects is this file's.
//!
//! It needs the feature and a loadable `LLVM-C.dll`; there is no text-only half.

#![cfg(feature = "llvm")]

use science_codegen_llvm::owned::{Builder, Module, cstr};
use science_codegen_llvm::{machine, sys};

/// A context, a module, a builder and a function with one block, for building
/// one instruction and printing it.
struct Scratch {
    builder: Builder,
    module: Module,
    context: science_codegen_llvm::owned::Context,
}

impl Scratch {
    fn new() -> Scratch {
        let context = machine::context();
        let module = Module::new(&context, "scratch");
        let builder = Builder::new(&context);
        Scratch { builder, module, context }
    }

    fn ctx(&self) -> sys::LLVMContextRef {
        self.context.raw()
    }

    /// Open a `void f()` and position the builder in its entry block.
    fn begin(&self, name: &str) -> sys::LLVMValueRef {
        unsafe {
            let void = sys::LLVMVoidTypeInContext(self.ctx());
            let ty = sys::LLVMFunctionType(void, std::ptr::null_mut(), 0, 0);
            let cname = cstr(name);
            let function = sys::LLVMAddFunction(self.module.raw(), cname.as_ptr(), ty);
            let entry = cstr("entry");
            let block =
                sys::LLVMAppendBasicBlockInContext(self.ctx(), function, entry.as_ptr());
            sys::LLVMPositionBuilderAtEnd(self.builder.raw(), block);
            function
        }
    }

    /// Two `i64` values LLVM cannot constant-fold: an `alloca` and a `load`.
    fn two_loaded_ints(&self) -> (sys::LLVMValueRef, sys::LLVMValueRef) {
        unsafe { self.two_loaded(sys::LLVMInt64TypeInContext(self.ctx())) }
    }

    /// Two `double` values LLVM cannot constant-fold.
    fn two_loaded_doubles(&self) -> (sys::LLVMValueRef, sys::LLVMValueRef) {
        unsafe { self.two_loaded(sys::LLVMDoubleTypeInContext(self.ctx())) }
    }

    unsafe fn two_loaded(&self, ty: sys::LLVMTypeRef) -> (sys::LLVMValueRef, sys::LLVMValueRef) {
        unsafe {
            let b = self.builder.raw();
            let slot = cstr("slot");
            let a_name = cstr("a");
            let b_name = cstr("b");
            let one = sys::LLVMBuildAlloca(b, ty, slot.as_ptr());
            let two = sys::LLVMBuildAlloca(b, ty, slot.as_ptr());
            (
                sys::LLVMBuildLoad2(b, ty, one, a_name.as_ptr()),
                sys::LLVMBuildLoad2(b, ty, two, b_name.as_ptr()),
            )
        }
    }

    fn finish(&self) -> String {
        unsafe { sys::LLVMBuildRetVoid(self.builder.raw()) };
        self.module.print()
    }
}

/// `LLVMIntPredicate`: all ten, by the mnemonic each one makes LLVM print.
#[test]
fn every_integer_predicate_selects_the_comparison_it_is_named_after() {
    let cases: &[(std::ffi::c_uint, &str)] = &[
        (sys::int_predicate::EQ, "eq"),
        (sys::int_predicate::NE, "ne"),
        (sys::int_predicate::UGT, "ugt"),
        (sys::int_predicate::UGE, "uge"),
        (sys::int_predicate::ULT, "ult"),
        (sys::int_predicate::ULE, "ule"),
        (sys::int_predicate::SGT, "sgt"),
        (sys::int_predicate::SGE, "sge"),
        (sys::int_predicate::SLT, "slt"),
        (sys::int_predicate::SLE, "sle"),
    ];
    let scratch = Scratch::new();
    scratch.begin("predicates");
    // **Both operands are loaded, not constant.** `LLVMBuildICmp` folds a
    // comparison of two constants to `true`/`false` and emits no instruction at
    // all, so a test written with `LLVMConstInt` operands asserts against an
    // empty function and passes for the wrong reason — or, here, fails and says
    // so. The fold is `IRBuilder`'s and it is on by default.
    let (lhs, rhs) = scratch.two_loaded_ints();
    for (index, (predicate, _)) in cases.iter().enumerate() {
        unsafe {
            let name = cstr(&format!("c{index}"));
            sys::LLVMBuildICmp(scratch.builder.raw(), *predicate, lhs, rhs, name.as_ptr());
        }
    }
    let ir = scratch.finish();
    for (index, (_, mnemonic)) in cases.iter().enumerate() {
        let wanted = format!("%c{index} = icmp {mnemonic} i64 %a, %b");
        assert!(
            ir.contains(&wanted),
            "`int_predicate` constant {index} did not produce `{wanted}`.\n\
             A wrong constant here compiles, links and verifies, and compares the wrong \
             way round.\n{ir}"
        );
    }
}

/// `LLVMRealPredicate`: the six ordered members `sys.rs` transcribes.
///
/// The `O` and not `U` choice is `reproducibility.md` Decision 3's, and it is
/// the one enum where picking the neighbouring constant is *silently* wrong
/// rather than obviously so — `ueq` differs from `oeq` only on NaN.
#[test]
fn every_real_predicate_is_the_ordered_form() {
    let cases: &[(std::ffi::c_uint, &str)] = &[
        (sys::real_predicate::OEQ, "oeq"),
        (sys::real_predicate::OGT, "ogt"),
        (sys::real_predicate::OGE, "oge"),
        (sys::real_predicate::OLT, "olt"),
        (sys::real_predicate::OLE, "ole"),
        (sys::real_predicate::ONE, "one"),
    ];
    let scratch = Scratch::new();
    scratch.begin("real_predicates");
    // Loaded, not constant: see the integer test.
    let (lhs, rhs) = scratch.two_loaded_doubles();
    for (index, (predicate, _)) in cases.iter().enumerate() {
        unsafe {
            let name = cstr(&format!("f{index}"));
            sys::LLVMBuildFCmp(scratch.builder.raw(), *predicate, lhs, rhs, name.as_ptr());
        }
    }
    let ir = scratch.finish();
    for (index, (_, mnemonic)) in cases.iter().enumerate() {
        let wanted = format!("%f{index} = fcmp {mnemonic} double %a, %b");
        assert!(ir.contains(&wanted), "expected `{wanted}` in:\n{ir}");
    }
    assert!(
        !ir.contains("fcmp u"),
        "an unordered predicate reached the IR; `reproducibility.md` Decision 3 forbids the \
         value change that would be:\n{ir}"
    );
}

/// `LLVMTypeKind`: `Float` is 2 and `Double` is 3, asked of LLVM rather than of
/// a header.
///
/// This is the constant [`science_codegen_llvm::emit`]'s float detection turns
/// on. If it were wrong, every `Cmp` in the language would build an `icmp` over
/// two doubles — a verifier failure, so loud, but only once something compares
/// floats, which no test before this one did.
#[test]
fn the_float_type_kinds_are_what_the_emitter_compares_against() {
    let scratch = Scratch::new();
    unsafe {
        let float = sys::LLVMFloatTypeInContext(scratch.ctx());
        let double = sys::LLVMDoubleTypeInContext(scratch.ctx());
        let i64_ty = sys::LLVMInt64TypeInContext(scratch.ctx());
        let ptr = sys::LLVMPointerTypeInContext(scratch.ctx(), 0);
        assert_eq!(sys::LLVMGetTypeKind(float), sys::type_kind::FLOAT);
        assert_eq!(sys::LLVMGetTypeKind(double), sys::type_kind::DOUBLE);
        // The half that matters as much: nothing else answers yes.
        for other in [i64_ty, ptr] {
            let kind = sys::LLVMGetTypeKind(other);
            assert_ne!(kind, sys::type_kind::FLOAT);
            assert_ne!(kind, sys::type_kind::DOUBLE);
        }
    }
}

/// `LLVMTypeKind`: `Half` is 1 and `BFloat` is 18, asked of LLVM rather than
/// of a header.
///
/// These are the two constants `type_is_float` and `describe_type` gained
/// alongside `FLOAT`/`DOUBLE`, once `let h: F16 be 1.5` showed that a `half`
/// or `bfloat` expectation fell through both: `Operand::ConstFloat` built a
/// `double` for an `F16` slot because `type_is_float` did not recognise
/// `half`, and the store-width check that caught the mismatch described the
/// `half` alloca as *"an aggregate"* because `describe_type` did not either.
/// `BFloat` is 18 rather than 4 — LLVM's enum runs `Half, Float, Double,
/// X86_FP80, FP128, PPC_FP128, Label, Integer, Function, Struct, Array,
/// Pointer, Vector, Metadata, X86_MMX, Token, ScalableVector` before it gets
/// there — so a guess at "the next float after `Double`" would have been
/// wrong by fourteen.
#[test]
fn the_half_precision_type_kinds_are_what_the_emitter_compares_against() {
    let scratch = Scratch::new();
    unsafe {
        let half = sys::LLVMHalfTypeInContext(scratch.ctx());
        let bfloat = sys::LLVMBFloatTypeInContext(scratch.ctx());
        let i16_ty = sys::LLVMIntTypeInContext(scratch.ctx(), 16);
        assert_eq!(sys::LLVMGetTypeKind(half), sys::type_kind::HALF);
        assert_eq!(sys::LLVMGetTypeKind(bfloat), sys::type_kind::BFLOAT);
        // `half` and `bfloat` are both sixteen bits and neither is `i16`, the
        // narrow-integer confusion `layout.rs`'s `FloatTy` doc comment warns
        // a merged representation would invite.
        assert_ne!(sys::type_kind::HALF, sys::type_kind::BFLOAT);
        assert_ne!(sys::LLVMGetTypeKind(i16_ty), sys::type_kind::HALF);
        assert_ne!(sys::LLVMGetTypeKind(i16_ty), sys::type_kind::BFLOAT);
    }
}

/// `LLVMTypeKind::LLVMPointerTypeKind` is 12, asked of LLVM rather than of a
/// header.
///
/// This is the constant the four address-taking instructions turn on —
/// `FieldAddr`, `LoadAt`, `StoreAt` and `ParamSlot`. It is a **positional** C
/// enum thirteen members down, which is the class `sys.rs` names as the
/// dangerous one: an entry inserted above it shifts it, and a wrong answer here
/// is a `getelementptr` on an `i64` that the check was supposed to stop.
///
/// The negative half is the one that would be silent. `INTEGER` is 8 and
/// `POINTER` is 12, so a constant off by four would make every field address
/// pass the guard and fail the verifier with a message about the GEP rather
/// than about the base — which is loud, but names the wrong instruction.
#[test]
fn the_pointer_type_kind_is_what_the_address_instructions_compare_against() {
    let scratch = Scratch::new();
    unsafe {
        let ptr = sys::LLVMPointerTypeInContext(scratch.ctx(), 0);
        assert_eq!(sys::LLVMGetTypeKind(ptr), sys::type_kind::POINTER);
        // Nothing this compiler can build answers yes by accident — and `i64`
        // is the one that matters, because a pointer-width integer is what a
        // mislowered address would be.
        for other in [
            sys::LLVMInt64TypeInContext(scratch.ctx()),
            sys::LLVMInt8TypeInContext(scratch.ctx()),
            sys::LLVMDoubleTypeInContext(scratch.ctx()),
            sys::LLVMArrayType2(sys::LLVMInt8TypeInContext(scratch.ctx()), 8),
        ] {
            assert_ne!(sys::LLVMGetTypeKind(other), sys::type_kind::POINTER);
        }
        // And the three named constants are three different numbers, which is
        // the property a positional enum loses first.
        assert_ne!(sys::type_kind::POINTER, sys::type_kind::INTEGER);
        assert_ne!(sys::type_kind::POINTER, sys::type_kind::DOUBLE);
    }
}

/// `LLVMBuildInBoundsGEP2` with an `i8` element type and one index is a byte
/// offset, and the offset LLVM's own `DataLayout` agrees it is.
///
/// **This is the claim the whole of field projection rests on.** `sys.rs`'s note
/// on the declaration says the byte form is used *because* the LLVM struct
/// type's member index is not the Science field index. That trade is only sound
/// if `gep i8, ptr %p, N` really lands `N` bytes in — and with opaque pointers
/// nothing about the emitted IR would look wrong if it did not.
///
/// Asserted by reading the IR, because a `getelementptr` on an `alloca` with a
/// constant index is folded by LLVM into a form whose text names the offset.
#[test]
fn an_i8_gep_is_a_byte_offset() {
    let scratch = Scratch::new();
    scratch.begin("gep");
    let b = scratch.builder.raw();
    unsafe {
        let i8_ty = sys::LLVMInt8TypeInContext(scratch.ctx());
        let i64_ty = sys::LLVMInt64TypeInContext(scratch.ctx());
        // `{ i8, [7 x i8], i64 }` — the shape `llvm_type` materialises for a
        // record whose second field is at offset 8. The padding is explicit,
        // which is exactly why the LLVM member index (2) is not the Science
        // field index (1) and why the offset is what is passed.
        let mut members = [i8_ty, sys::LLVMArrayType2(i8_ty, 7), i64_ty];
        let strukt = sys::LLVMStructTypeInContext(scratch.ctx(), members.as_mut_ptr(), 3, 0);
        let slot_name = cstr("s");
        let slot = sys::LLVMBuildAlloca(b, strukt, slot_name.as_ptr());
        let mut indices = [sys::LLVMConstInt(i64_ty, 8, 0)];
        let gep_name = cstr("field");
        let address =
            sys::LLVMBuildInBoundsGEP2(b, i8_ty, slot, indices.as_mut_ptr(), 1, gep_name.as_ptr());
        sys::LLVMBuildStore(b, sys::LLVMConstInt(i64_ty, 7, 0), address);
    }
    let ir = scratch.finish();
    assert!(
        ir.contains("getelementptr inbounds i8, ptr %s, i64 8"),
        "an `i8` GEP of 8 is not eight bytes in — the whole of field projection rests on it \
         being one:\n{ir}"
    );
    assert!(
        ir.contains("store i64 7, ptr %field"),
        "the store did not go through the computed address:\n{ir}"
    );
}

/// `LLVMLinkage` and `LLVMUnnamedAddr`: Decision 15's
/// `private unnamed_addr constant`, and Decision 12's `internal`.
///
/// Positional C enums, both. `sys.rs` names the hazard — *"an entry inserted
/// above the one you want shifts it"* — and `private` shifting to `dllimport`
/// would export a string literal's bytes from the executable.
#[test]
fn the_linkage_and_unnamed_addr_constants_print_the_keywords_they_name() {
    let scratch = Scratch::new();
    unsafe {
        let i8_ty = sys::LLVMInt8TypeInContext(scratch.ctx());
        let array = sys::LLVMArrayType2(i8_ty, 3);
        for (symbol, linkage, keyword) in [
            ("private_one", sys::linkage::PRIVATE, "private"),
            ("internal_one", sys::linkage::INTERNAL, "internal"),
        ] {
            let name = cstr(symbol);
            let global = sys::LLVMAddGlobal(scratch.module.raw(), array, name.as_ptr());
            let bytes = cstr("abc");
            let init = sys::LLVMConstStringInContext(scratch.ctx(), bytes.as_ptr(), 3, 1);
            sys::LLVMSetInitializer(global, init);
            sys::LLVMSetGlobalConstant(global, 1);
            sys::LLVMSetLinkage(global, linkage);
            sys::LLVMSetUnnamedAddress(global, sys::unnamed_addr::GLOBAL);
            let ir = scratch.module.print();
            assert!(
                ir.contains(&format!("@{symbol} = {keyword} unnamed_addr constant")),
                "`linkage::{}` did not print `{keyword}`, or `unnamed_addr::GLOBAL` did not \
                 print `unnamed_addr`:\n{ir}",
                keyword.to_uppercase()
            );
        }
    }
}

/// `LLVMAttributeIndex`'s two reserved values, and `sret` being a **type**
/// attribute.
///
/// `LLVM_ATTRIBUTE_FUNCTION_INDEX` is `-1` crossing as `unsigned`, which is the
/// one place in `sys.rs` where a signed constant changes meaning on the way
/// across. If it were wrong, `nounwind` would land on a parameter that may not
/// exist, and Decision 6 would be silently unapplied.
#[test]
fn the_attribute_indices_put_nounwind_on_the_function_and_sret_on_parameter_one() {
    let scratch = Scratch::new();
    let attrs = science_codegen_llvm::owned::Attrs::new(&scratch.context)
        .expect("LLVM 18 knows every attribute kind this backend emits");
    unsafe {
        let void = sys::LLVMVoidTypeInContext(scratch.ctx());
        let ptr = sys::LLVMPointerTypeInContext(scratch.ctx(), 0);
        let i64_ty = sys::LLVMInt64TypeInContext(scratch.ctx());
        let mut members = [ptr, i64_ty, i64_ty];
        let string = sys::LLVMStructTypeInContext(scratch.ctx(), members.as_mut_ptr(), 3, 0);
        let mut params = [ptr];
        let ty = sys::LLVMFunctionType(void, params.as_mut_ptr(), 1, 0);
        let name = cstr("returns_a_string");
        let function = sys::LLVMAddFunction(scratch.module.raw(), name.as_ptr(), ty);
        sys::LLVMAddAttributeAtIndex(
            function,
            sys::LLVM_ATTRIBUTE_FUNCTION_INDEX,
            attrs.nounwind(),
        );
        sys::LLVMAddAttributeAtIndex(
            function,
            sys::LLVM_ATTRIBUTE_FIRST_PARAM_INDEX,
            attrs.sret(string),
        );
        sys::LLVMAddAttributeAtIndex(
            function,
            sys::LLVM_ATTRIBUTE_FIRST_PARAM_INDEX,
            attrs.align(8),
        );
    }
    let ir = scratch.module.print();
    assert!(
        ir.contains("declare void @returns_a_string(ptr sret({ ptr, i64, i64 }) align 8)"),
        "`sret` did not land on parameter 1 as a type attribute:\n{ir}"
    );
    assert!(
        ir.contains("attributes #0 = { nounwind }"),
        "`nounwind` did not land on the function's own attribute slot:\n{ir}"
    );
}

/// `LLVMGetEnumAttributeKindForName` returns 0 for a name LLVM does not know,
/// and [`science_codegen_llvm::owned::Attrs`] exists to turn that into a
/// failure rather than an attribute with no meaning.
#[test]
fn an_unknown_attribute_name_is_zero_which_is_why_attrs_checks() {
    let scratch = Scratch::new();
    let bogus = cstr("definitely_not_an_llvm_attribute");
    let kind = unsafe {
        sys::LLVMGetEnumAttributeKindForName(bogus.as_ptr(), "definitely_not_an_llvm_attribute".len())
    };
    assert_eq!(kind, 0, "LLVM no longer returns 0 for an unknown attribute name");
    assert!(
        science_codegen_llvm::owned::Attrs::new(&scratch.context).is_ok(),
        "one of the six attribute names this backend emits is unknown to this LLVM"
    );
}

/// `LLVMVerifierFailureAction::RETURN_STATUS` hands the message back instead of
/// aborting, and `LLVMBool` is non-zero on failure.
///
/// Both halves matter and both are inversions a reader gets wrong: `sys.rs` says
/// *"reading one of them as 'true means it worked' is a compiler that emits
/// nothing and reports success"*. The test builds a module that is definitely
/// broken and asserts the verifier says so **and the process is still running**.
#[test]
fn the_verifier_returns_a_status_rather_than_aborting() {
    let scratch = Scratch::new();
    let function = scratch.begin("falls_off_the_end");
    let _ = function;
    // No terminator: a block that falls off the end is the smallest module LLVM
    // rejects, and it does not need a wrong type anywhere.
    let broken = scratch.module.verify();
    assert!(broken.is_err(), "a block with no terminator verified");
    let message = broken.unwrap_err();
    assert!(
        !message.is_empty(),
        "`RETURN_STATUS` produced no message, so the `char **OutMessage` parameter is not \
         being written"
    );
    // And a well-formed module verifies, so the failure above is the module's
    // and not the call's.
    let ok = Scratch::new();
    ok.begin("returns");
    ok.finish();
    assert_eq!(ok.module.verify(), Ok(()));
}

/// `LLVMCodeGenFileType`: `ASSEMBLY` is 0 and `OBJECT` is 1, checked by what
/// comes out of the file rather than by the number.
#[test]
fn the_file_type_constants_select_assembly_and_an_object() {
    let config = science_codegen::target::TargetConfig::new(
        science_codegen::layout::Triple::host().expect("a supported host"),
        science_codegen::target::OptLevel::O0,
    );
    let scratch = Scratch::new();
    scratch.begin("empty");
    scratch.finish();
    let target = machine::for_config(&config).expect("a target machine");
    machine::describe(&scratch.module, &target, config.triple());

    let dir = std::env::temp_dir().join(format!("science-filetype-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temporary directory");

    let assembly = dir.join("out.s");
    machine::emit_to_file(&scratch.module, &target, &assembly, sys::file_type::ASSEMBLY)
        .expect("assembly");
    let text = std::fs::read_to_string(&assembly).expect("the assembly is text");
    assert!(text.contains("empty"), "the assembly does not name the function:\n{text}");

    let object = dir.join("out.o");
    machine::emit_to_file(&scratch.module, &target, &object, sys::file_type::OBJECT)
        .expect("an object");
    let bytes = std::fs::read(&object).expect("the object is readable");
    assert!(bytes.len() > 16, "the object is too small to be one");
    assert!(
        bytes != text.as_bytes(),
        "`OBJECT` and `ASSEMBLY` produced the same file, so one of the two constants is wrong"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
