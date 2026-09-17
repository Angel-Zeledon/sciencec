//! The LLVM-C binding: one `extern "C"` block, and it is the deliverable.
//!
//! # 0. Why this file exists rather than `llvm-sys`
//!
//! `codegen-and-linking.md` Decision 1 says `science-codegen` is written against
//! LLVM's C API *"from the first line, in both implementations"*, and gives the
//! reason: `inkwell` is Rust, a Science program cannot call it, and a
//! self-hosted backend must reach LLVM through an `extern` block — the bucket
//! `c-binding-coverage.md` §3 classifies as fully reachable. Decision 1 then
//! names `llvm-sys` as the Rust side's transliteration of that same surface and
//! rejects writing the declarations by hand, on the grounds that doing it by
//! hand *"buys purity and costs the LLVM upgrade path"*.
//!
//! **That rejection does not survive contact with this machine, and the reason
//! is not purity.** The LLVM 18.1.8 installation here is the Windows binary
//! release, and `llvm-sys` cannot be built against it:
//!
//! | What `llvm-sys` needs | What is here |
//! |---|---|
//! | `bin/llvm-config` | absent |
//! | `lib/LLVMCore.lib`, `LLVMX86CodeGen.lib`, … (~100 static archives) | absent |
//! | `include/llvm-c/*.h` | only `lto.h` and `Remarks.h` |
//! | | **`lib/LLVM-C.lib` + `bin/LLVM-C.dll`, exporting 1226 symbols** |
//!
//! So the choice on this machine is not "hand-written declarations versus
//! `llvm-sys`". It is "hand-written declarations versus no backend". And the
//! shape that *is* present — a single versioned shared library behind the
//! stable C API — is exactly the shape Decision 3 already requires of the
//! self-hosted compiler: *"the self-hosted `science-codegen` declares
//! `unsafe extern "C" library "LLVM-18":` and links `libLLVM` dynamically —
//! `libLLVM-18.so`, `libLLVM.dylib`, `LLVM-C.dll`."*
//!
//! **The decision, therefore:** the Rust backend is written the way Decision 3
//! says the Science one will be — one `extern` block against one dynamic
//! library — and this file is the draft of that block. Decision 1's *"second
//! reason, which is a dividend and not a cost"* is what this buys: the binding
//! file the Rust implementation uses **is** the `extern` block the Science
//! implementation will use, and it is now a file rather than a promise.
//!
//! **The cost, and it is Decision 1's own, so it should be read as accepted
//! rather than as discovered:** an LLVM upgrade is a re-transcription of this
//! file rather than a version bump. Decision 2 pins 18.1.8 and says the pin is
//! load-bearing for numbers rather than merely for compilation, so an upgrade
//! was already going to be a code change; this makes it a slightly larger one.
//! The mitigation is that the file is small — see §2 — and that §1's three
//! checks fire on every build.
//!
//! **Decision 1's asymmetry is also gone, and that is a gain.** Decision 3
//! calls the static-Rust/dynamic-Science split *"deliberate and survivable only
//! because Decision 2 pins the version"*. Here there is no split: both
//! implementations link `LLVM-C.dll` dynamically at the same version through
//! the same entry points. Gate J's byte-identical requirement is easier to
//! meet, not harder. The price is §1.5's broken promise arriving early: a
//! `sciencec` built with this backend is not self-contained either.
//!
//! # 1. Every declaration below is a claim about a signature I cannot see
//!
//! The headers are not on this machine. A wrong declaration is not a compile
//! error and not a link error: on Win64 the caller cleans the stack, so a wrong
//! arity produces a garbage argument and a plausible-looking crash somewhere
//! else. That is §4.1's failure mode pointed at the compiler instead of at
//! `dgemm`.
//!
//! **Four checks, and none of them is "I was careful".** All four are written
//! and all four run; `tests/` is where each one lives.
//!
//! 1. **The symbol exists.** `tests/symbols.rs` parses this file, extracts every
//!    name in the block, runs `llvm-nm` over `LLVM-C.lib`, and fails on any name
//!    the import library does not export. All 100 are exported by this
//!    installation's 1226. That catches a typo and a function that does not
//!    exist in 18.1 — including the one this file hit:
//!    `LLVMConstStringInContext2` is LLVM 19 and is **not** exported here, so
//!    the 18-era `LLVMConstStringInContext` is what is declared. The same file
//!    asserts the *reverse* list: the three fast-math setters and the four
//!    superseded spellings stay undeclared, which is §7.3 obligation 1 made
//!    structural rather than aspirational.
//! 2. **Every transcribed enum constant is checked against what LLVM does with
//!    it.** `tests/abi_claims.rs` passes each one to the function it belongs to
//!    and reads the mnemonic back out of the IR: all ten `LLVMIntPredicate`
//!    members, all six ordered `LLVMRealPredicate` members, `LLVMTypeKind`'s
//!    float cases, `LLVMLinkage`'s `private` and `internal`, `LLVMUnnamedAddr`,
//!    the two attribute indices, and `LLVMCodeGenFileType`. **This is the check
//!    the list most needed**, because a wrong enum constant is the only class of
//!    error here that compiles, links, verifies *and computes the wrong answer*:
//!    a wrong name fails to link and a wrong arity crashes.
//! 3. **The arity and the types are checked by the IR that comes back.** Every
//!    module this crate builds is printed with `LLVMPrintModuleToString` and the
//!    text is asserted against what was intended, and `tests/roundtrip.rs` feeds
//!    a module declaring **every one of** `science-rt`'s runtime entry points back to
//!    `clang -x ir`,
//!    which re-parses it with LLVM 18's own parser and assembles it to an
//!    object. `clang` was built from the headers this machine does not have, so
//!    it is the only reader here that knows what the C API's callers were
//!    supposed to produce. A declaration with the wrong argument count or the
//!    wrong pointer/integer split produces a module that says something other
//!    than what was asked for, and the text is where that shows. This is the
//!    check that catches ABI mistakes, because it compares *behaviour* and not
//!    spelling.
//! 4. **`LLVMVerifyModule` runs on every module**, Decision 34, before anything
//!    is emitted — and the crate calls it a second time inside `emit`, because
//!    *"verifying twice costs milliseconds; emitting an unverified module costs
//!    a miscompile"*.
//!
//! **What none of the four catches**, stated so nobody assumes otherwise: a
//! parameter that is `unsigned` where this file says `u64`, in a function whose
//! result this crate never inspects. Every declaration below is called by
//! `crate::emit` or `crate::owned`, and every call's effect is asserted
//! somewhere, which is the only reason the risk is bounded rather than open.
//!
//! **The ones still taken on trust, named so that a reader knows the list is not
//! empty:** `LLVMSetAlignment`'s `unsigned Bytes` and `LLVMCreateEnumAttribute`'s
//! `uint64_t Val`, both of which are exercised only with small values, so a
//! width error in either would not show; and `LLVMABISizeOfType`'s
//! `unsigned long long` return, which `tests/layout_agreement.rs` compares
//! against `science-codegen`'s own numbers for seventeen scalars and eight
//! aggregates — which is as close to a width check as this machine can get.
//!
//! # 2. What is declared, and what is not
//!
//! §1.6 of the note estimates *"roughly 190"* entry points for an F0 code
//! generator and calls that the full surface. This file declares far fewer,
//! which is that estimate restricted to §10's stages 0 to 4: there is no
//! `DIBuilder` (§6 is line tables and stage 1 needs none), no metadata, and no
//! `LLVMInitializeAArch64*` (cross-compilation is `SC0406`, so the only target
//! is the host, and the host is x86-64 for both x86 triples).
//!
//! **One entry has moved from the second list to the first and the reason is
//! worth keeping.** This paragraph used to name `LLVMBuildGEP2` among the
//! absentees, because *"opaque pointers mean a global array's address is the
//! global, and no aggregate projection is lowered yet"*. The first half is
//! still true and the second stopped being true the day a record had a field:
//! [`LLVMBuildInBoundsGEP2`] is declared below and its note says why the call
//! is a **byte** offset rather than §2.3's field index.
//!
//! **Declare only what you call** is the rule, and it is the rule because the
//! list is a specification: a declaration nobody calls is a claim nobody checks,
//! and §1's checks 2 and 3 only fire on calls.
//!
//! # 3. The one thing LLVM-C cannot do, and Decision 36 depends on it
//!
//! Decision 36 requires `AllowFPOpFusion = Strict` on every target machine, and
//! §15 calls it *"the single highest-value line in this note … which is exactly
//! why it will be the one that is missing"*.
//!
//! **There is no such line, because LLVM-C has no entry point that sets it.**
//! The 1226 symbols `LLVM-C.lib` exports contain `LLVMSetFastMathFlags`,
//! `LLVMGetFastMathFlags` and `LLVMCanValueUseFastMathFlags` — the
//! per-instruction flags — and nothing whatever for `TargetOptions`. LLVM 18's
//! `LLVMCreateTargetMachineOptions` builder sets CPU, features, ABI,
//! optimisation level, relocation model and code model, and has no FP-fusion
//! setter. [`crate::machine`] states what is done instead and
//! `tests/float_policy.rs` is the empirical answer.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ffi::{c_char, c_double, c_int, c_uint, c_ulonglong};

// --- opaque handles -------------------------------------------------------
//
// LLVM-C's handles are `struct LLVMOpaqueFoo *`. Modelled as distinct
// zero-variant types behind a raw pointer rather than as `*mut c_void`, so that
// passing a `LLVMTypeRef` where a `LLVMValueRef` belongs is a Rust type error.
// Decision 1's first cost item is that calling LLVM-C directly gives up exactly
// this checking — *"everything is an `LLVMValueRef`"* — and this recovers the
// part of it that costs nothing: the handle kinds. It does **not** recover the
// part that matters, which is `i64` versus `double` inside one `LLVMValueRef`,
// and `LLVMVerifyModule` is what catches that.

macro_rules! opaque {
    ($($name:ident => $handle:ident),* $(,)?) => {$(
        /// An opaque LLVM object. Never constructed or dereferenced on this side.
        #[repr(C)]
        pub struct $name {
            _private: [u8; 0],
        }
        /// An opaque LLVM handle.
        pub type $handle = *mut $name;
    )*};
}

opaque! {
    LLVMOpaqueContext => LLVMContextRef,
    LLVMOpaqueModule => LLVMModuleRef,
    LLVMOpaqueType => LLVMTypeRef,
    LLVMOpaqueValue => LLVMValueRef,
    LLVMOpaqueBasicBlock => LLVMBasicBlockRef,
    LLVMOpaqueBuilder => LLVMBuilderRef,
    LLVMOpaqueAttribute => LLVMAttributeRef,
    LLVMOpaqueTarget => LLVMTargetRef,
    LLVMOpaqueTargetMachine => LLVMTargetMachineRef,
    LLVMOpaqueTargetData => LLVMTargetDataRef,
    LLVMOpaquePassBuilderOptions => LLVMPassBuilderOptionsRef,
    LLVMOpaqueError => LLVMErrorRef,
}

/// `typedef int LLVMBool;` — **not** a `bool`, and the distinction is
/// load-bearing in both directions.
///
/// Every `LLVMBool`-returning function in this file returns **non-zero on
/// failure**, which is the opposite of what the name suggests:
/// `LLVMVerifyModule` returns 1 when the module is broken,
/// `LLVMGetTargetFromTriple` returns 1 when there is no such target,
/// `LLVMTargetMachineEmitToFile` returns 1 when it did not write the file.
/// Reading one of them as "true means it worked" is a compiler that emits
/// nothing and reports success.
pub type LLVMBool = c_int;

/// `LLVMAttributeIndex` — `unsigned`, with two reserved values.
pub type LLVMAttributeIndex = c_uint;

/// The return value's attribute slot.
pub const LLVM_ATTRIBUTE_RETURN_INDEX: LLVMAttributeIndex = 0;

/// The function's own attribute slot. `-1` as an `unsigned`, which is how the
/// header spells it (`LLVMAttributeFunctionIndex = -1`) and is the one place
/// here where a signed constant crosses as unsigned.
pub const LLVM_ATTRIBUTE_FUNCTION_INDEX: LLVMAttributeIndex = c_uint::MAX;

/// The first parameter's attribute slot. Parameters are 1-based because 0 is
/// the return value.
pub const LLVM_ATTRIBUTE_FIRST_PARAM_INDEX: LLVMAttributeIndex = 1;

/// `LLVMLinkage`, the members this crate uses.
///
/// Transcribed with their positions rather than their names alone, because the
/// C enum is positional and an entry inserted above the one you want shifts it:
/// `LLVMExternalLinkage` is 0, `LLVMInternalLinkage` is 8, `LLVMPrivateLinkage`
/// is 9. Decision 15 wants `private` for a string literal's bytes and Decision
/// 12 wants `internal` for drop glue, which is the whole of what is needed.
pub mod linkage {
    use super::c_uint;
    /// `external` — the default, and what a `declare` and an exported `define`
    /// carry.
    pub const EXTERNAL: c_uint = 0;
    /// `internal` — Decision 12's drop glue.
    pub const INTERNAL: c_uint = 8;
    /// `private` — Decision 15's `.str.N` and Decision 20's descriptors.
    pub const PRIVATE: c_uint = 9;
}

/// `LLVMUnnamedAddr`. Decision 15 and Decision 20 both say
/// `private unnamed_addr constant`, and Decision 20 gives the reason it is safe:
/// *"`unnamed_addr` lets LLVM merge two descriptors that happen to be
/// bit-identical, which is harmless because the runtime compares contents and
/// not addresses."*
pub mod unnamed_addr {
    use super::c_uint;
    /// The address is significant.
    pub const NONE: c_uint = 0;
    /// `local_unnamed_addr`.
    pub const LOCAL: c_uint = 1;
    /// `unnamed_addr`.
    pub const GLOBAL: c_uint = 2;
}

/// `LLVMIntPredicate`.
///
/// The numbering starts at 32 because it mirrors `llvm::CmpInst::Predicate`,
/// where the float predicates occupy 0–15 and the integer ones begin at
/// `ICMP_EQ = 32`. Writing 0 here for `Eq` would compile, link, verify, and
/// compare floats.
pub mod int_predicate {
    use super::c_uint;
    /// `eq`.
    pub const EQ: c_uint = 32;
    /// `ne`.
    pub const NE: c_uint = 33;
    /// `ugt`.
    pub const UGT: c_uint = 34;
    /// `uge`.
    pub const UGE: c_uint = 35;
    /// `ult`. The bounds check of Decision 10 is this one.
    pub const ULT: c_uint = 36;
    /// `ule`.
    pub const ULE: c_uint = 37;
    /// `sgt`.
    pub const SGT: c_uint = 38;
    /// `sge`.
    pub const SGE: c_uint = 39;
    /// `slt`.
    pub const SLT: c_uint = 40;
    /// `sle`.
    pub const SLE: c_uint = 41;
}

/// `LLVMRealPredicate`, the ordered members.
///
/// **`O` and not `U`, and it is a reproducibility question rather than a taste
/// one.** An ordered predicate is false when either operand is NaN; an unordered
/// one is true. `reproducibility.md` Decision 3 forbids every value-changing
/// float transform, and picking the unordered form would change the answer of
/// every comparison involving a NaN — silently, and only on the inputs nobody
/// tests.
pub mod real_predicate {
    use super::c_uint;
    /// `oeq`.
    pub const OEQ: c_uint = 1;
    /// `ogt`.
    pub const OGT: c_uint = 2;
    /// `oge`.
    pub const OGE: c_uint = 3;
    /// `olt`.
    pub const OLT: c_uint = 4;
    /// `ole`.
    pub const OLE: c_uint = 5;
    /// `one`.
    pub const ONE: c_uint = 6;
}

/// `LLVMCodeGenOptLevel`.
///
/// **Not the optimisation pipeline.** Decision 33 selects the pipeline by
/// string through [`LLVMRunPasses`] — *"the pipeline is selected by string,
/// which means the pipeline name is a value that can go in the `[build]`
/// table"* — and this enum is the *code generator's* level, which governs
/// instruction selection and register allocation after the pipeline has run.
/// Both are set, from one [`science_codegen::target::OptLevel`].
pub mod codegen_opt {
    use super::c_uint;
    /// `-O0`.
    pub const NONE: c_uint = 0;
    /// `-O1`.
    pub const LESS: c_uint = 1;
    /// `-O2`.
    pub const DEFAULT: c_uint = 2;
    /// `-O3`.
    pub const AGGRESSIVE: c_uint = 3;
}

/// `LLVMRelocMode`.
pub mod reloc {
    use super::c_uint;
    /// Let the target decide.
    pub const DEFAULT: c_uint = 0;
    /// Non-relocatable.
    pub const STATIC: c_uint = 1;
    /// Position-independent.
    pub const PIC: c_uint = 2;
}

/// `LLVMCodeModel`.
pub mod code_model {
    use super::c_uint;
    /// Let the target decide.
    pub const DEFAULT: c_uint = 0;
    /// The small code model.
    pub const SMALL: c_uint = 3;
}

/// `LLVMCodeGenFileType`.
pub mod file_type {
    use super::c_uint;
    /// `.s`. Used by `tests/float_policy.rs`, which has to look at instructions
    /// rather than at IR to answer Decision 36's question.
    pub const ASSEMBLY: c_uint = 0;
    /// `.o`. What §10's stage 0 produces.
    pub const OBJECT: c_uint = 1;
}

/// `LLVMTypeKind`, the two members this crate distinguishes.
///
/// Positional, like every other C enum here: `Void` is 0, `Half` is 1, and the
/// two below follow. Only the float cases are named, because the only question
/// asked is "is this an `fcmp`".
pub mod type_kind {
    use super::c_uint;
    /// `float`.
    pub const FLOAT: c_uint = 2;
    /// `double`.
    pub const DOUBLE: c_uint = 3;
    /// `iN`. Asked by [`crate::emit`]'s `switch`, which builds a case constant
    /// with `LLVMConstInt` against the switched value's type: `LLVMConstInt` on
    /// a type that is not an integer is an assertion failure in a debug LLVM and
    /// undefined in a release one, and this installation is a release one.
    pub const INTEGER: c_uint = 8;
    /// `ptr`. LLVM 18 has one pointer type and no pointee, so this constant
    /// answers the only question left about one: *is it a pointer at all.*
    /// Asked by [`crate::emit`]'s `FieldAddr`, `LoadAt`, `StoreAt` and
    /// `ParamSlot`, each of which takes an address as an `Operand` and would
    /// otherwise hand `LLVMBuildInBoundsGEP2` an integer.
    pub const POINTER: c_uint = 12;
}

/// `LLVMVerifierFailureAction`.
///
/// Only [`verifier_action::RETURN_STATUS`] is ever passed. `AbortProcessAction`
/// is the header's first member and would turn Decision 34's *"internal
/// compiler error that prints the offending function's IR"* into a bare
/// `abort()` with nothing printed.
pub mod verifier_action {
    use super::c_uint;
    /// `abort()` immediately.
    pub const ABORT_PROCESS: c_uint = 0;
    /// Print to stderr and carry on.
    pub const PRINT_MESSAGE: c_uint = 1;
    /// Hand the message back. The only one used.
    pub const RETURN_STATUS: c_uint = 2;
}

unsafe extern "C" {
    // --- context, module, and printing ------------------------------------

    /// `LLVMContextRef LLVMContextCreate(void)`
    pub fn LLVMContextCreate() -> LLVMContextRef;

    /// `void LLVMContextDispose(LLVMContextRef C)`
    pub fn LLVMContextDispose(c: LLVMContextRef);

    /// `LLVMModuleRef LLVMModuleCreateWithNameInContext(const char *ModuleID, LLVMContextRef C)`
    ///
    /// Note the argument order: the name comes **first**, unlike almost every
    /// other `...InContext` function in the API, where the context leads.
    pub fn LLVMModuleCreateWithNameInContext(
        module_id: *const c_char,
        c: LLVMContextRef,
    ) -> LLVMModuleRef;

    /// `void LLVMDisposeModule(LLVMModuleRef M)`
    pub fn LLVMDisposeModule(m: LLVMModuleRef);

    /// `void LLVMSetTarget(LLVMModuleRef M, const char *Triple)`
    pub fn LLVMSetTarget(m: LLVMModuleRef, triple: *const c_char);

    /// `void LLVMSetDataLayout(LLVMModuleRef M, const char *DataLayoutStr)`
    pub fn LLVMSetDataLayout(m: LLVMModuleRef, data_layout: *const c_char);

    /// `char *LLVMPrintModuleToString(LLVMModuleRef M)`
    ///
    /// **The caller owns the string** and must free it with
    /// [`LLVMDisposeMessage`]. Decision 1's second cost item is exactly this
    /// class of fact — *"LLVM-C has real ownership transfers"* — and
    /// [`crate::owned::Message`] is the newtype that stops it being remembered.
    pub fn LLVMPrintModuleToString(m: LLVMModuleRef) -> *mut c_char;

    /// `void LLVMDisposeMessage(char *Message)`
    pub fn LLVMDisposeMessage(message: *mut c_char);

    // --- types ------------------------------------------------------------

    /// `LLVMTypeRef LLVMVoidTypeInContext(LLVMContextRef C)`
    pub fn LLVMVoidTypeInContext(c: LLVMContextRef) -> LLVMTypeRef;

    /// `LLVMTypeRef LLVMInt1TypeInContext(LLVMContextRef C)`
    ///
    /// §3.1: *"`Bool` is `i1` in registers and `i8` in memory."* Both appear,
    /// and confusing them is a store of one bit where a byte was reserved.
    pub fn LLVMInt1TypeInContext(c: LLVMContextRef) -> LLVMTypeRef;

    /// `LLVMTypeRef LLVMInt8TypeInContext(LLVMContextRef C)`
    pub fn LLVMInt8TypeInContext(c: LLVMContextRef) -> LLVMTypeRef;

    /// `LLVMTypeRef LLVMInt32TypeInContext(LLVMContextRef C)`
    pub fn LLVMInt32TypeInContext(c: LLVMContextRef) -> LLVMTypeRef;

    /// `LLVMTypeRef LLVMInt64TypeInContext(LLVMContextRef C)`
    pub fn LLVMInt64TypeInContext(c: LLVMContextRef) -> LLVMTypeRef;

    /// `LLVMTypeRef LLVMIntTypeInContext(LLVMContextRef C, unsigned NumBits)`
    pub fn LLVMIntTypeInContext(c: LLVMContextRef, num_bits: c_uint) -> LLVMTypeRef;

    /// `LLVMTypeRef LLVMFloatTypeInContext(LLVMContextRef C)`
    pub fn LLVMFloatTypeInContext(c: LLVMContextRef) -> LLVMTypeRef;

    /// `LLVMTypeRef LLVMDoubleTypeInContext(LLVMContextRef C)`
    pub fn LLVMDoubleTypeInContext(c: LLVMContextRef) -> LLVMTypeRef;

    /// `LLVMTypeRef LLVMPointerTypeInContext(LLVMContextRef C, unsigned AddressSpace)`
    ///
    /// The opaque pointer constructor. §3.1: *"a pointer is `ptr` — LLVM 18 has
    /// only opaque pointers, which removes an entire category of bug from §2.3's
    /// GEP lowering and is one of Decision 2's reasons."* The older
    /// `LLVMPointerType(LLVMTypeRef, unsigned)` still exists in 18 and takes a
    /// pointee it immediately discards; this one does not invite the question.
    pub fn LLVMPointerTypeInContext(c: LLVMContextRef, address_space: c_uint) -> LLVMTypeRef;

    /// `LLVMTypeRef LLVMArrayType2(LLVMTypeRef ElementType, uint64_t ElementCount)`
    ///
    /// **`ArrayType2` and not `ArrayType`**, and the `2` is the whole point: the
    /// original takes an `unsigned` count and LLVM 17 added this one to widen it
    /// to `uint64_t`. Declaring the old name with a `u64` parameter would pass a
    /// 64-bit value where 32 bits are read — correct for every literal shorter
    /// than four gigabytes and wrong in a way nothing would ever exercise.
    pub fn LLVMArrayType2(element_type: LLVMTypeRef, element_count: u64) -> LLVMTypeRef;

    /// `LLVMTypeRef LLVMStructTypeInContext(LLVMContextRef C, LLVMTypeRef *ElementTypes, unsigned ElementCount, LLVMBool Packed)`
    pub fn LLVMStructTypeInContext(
        c: LLVMContextRef,
        element_types: *mut LLVMTypeRef,
        element_count: c_uint,
        packed: LLVMBool,
    ) -> LLVMTypeRef;

    /// `LLVMTypeRef LLVMStructCreateNamed(LLVMContextRef C, const char *Name)`
    ///
    /// Decision 11: *"a Science `type` lowers to a **named** LLVM struct"*. The
    /// name is not semantic to LLVM and is entirely for the IR dump, which
    /// Decision 5 makes diffable against a MIR dump.
    pub fn LLVMStructCreateNamed(c: LLVMContextRef, name: *const c_char) -> LLVMTypeRef;

    /// `void LLVMStructSetBody(LLVMTypeRef StructTy, LLVMTypeRef *ElementTypes, unsigned ElementCount, LLVMBool Packed)`
    ///
    /// `Packed` is **always 0**. Decision 17 gives the C layout rule, and this
    /// crate materialises the padding itself as explicit `[N x i8]` members
    /// rather than letting LLVM's own struct layout do it, so that the offsets
    /// in the IR are `science_codegen::layout`'s and not LLVM's. A packed struct
    /// with explicit padding and an unpacked struct with implicit padding agree
    /// on every type F0 can build and disagree the first time they do not, which
    /// is a disagreement nobody would notice.
    pub fn LLVMStructSetBody(
        struct_ty: LLVMTypeRef,
        element_types: *mut LLVMTypeRef,
        element_count: c_uint,
        packed: LLVMBool,
    );

    /// `LLVMTypeRef LLVMFunctionType(LLVMTypeRef ReturnType, LLVMTypeRef *ParamTypes, unsigned ParamCount, LLVMBool IsVarArg)`
    ///
    /// `IsVarArg` is always 0 here. §4.5: *"varargs are refused … `printf` is not
    /// callable from Science."*
    pub fn LLVMFunctionType(
        return_type: LLVMTypeRef,
        param_types: *mut LLVMTypeRef,
        param_count: c_uint,
        is_var_arg: LLVMBool,
    ) -> LLVMTypeRef;

    // --- constants and globals --------------------------------------------

    /// `LLVMValueRef LLVMConstStringInContext(LLVMContextRef C, const char *Str, unsigned Length, LLVMBool DontNullTerminate)`
    ///
    /// `DontNullTerminate` is **1** here, always. Decision 15's `StringLiteral`
    /// carries the bytes and a length, and `science_string_from_bytes(ptr, len)`
    /// reads exactly `len` of them: a NUL would make the constant one byte
    /// longer than the length passed beside it, which is not wrong but is a
    /// discrepancy between two numbers a reader compares.
    ///
    /// **`LLVMConstStringInContext2` is deliberately not declared** — it is LLVM
    /// 19's widening of `Length` to `size_t` and it is not exported by this
    /// installation. `tests/symbols.rs` is what established that, rather than a
    /// guess.
    pub fn LLVMConstStringInContext(
        c: LLVMContextRef,
        str_: *const c_char,
        length: c_uint,
        dont_null_terminate: LLVMBool,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMConstInt(LLVMTypeRef IntTy, unsigned long long N, LLVMBool SignExtend)`
    pub fn LLVMConstInt(int_ty: LLVMTypeRef, n: c_ulonglong, sign_extend: LLVMBool)
    -> LLVMValueRef;

    /// `LLVMValueRef LLVMConstReal(LLVMTypeRef RealTy, double N)`
    ///
    /// The bit pattern crosses as a `double` and is never re-parsed from text.
    /// `science_codegen::backend::Operand::ConstFloat`'s own note is the reason:
    /// *"a decimal round-trip through a printer and a parser is a value-changing
    /// transform with no flag attached to it."*
    pub fn LLVMConstReal(real_ty: LLVMTypeRef, n: c_double) -> LLVMValueRef;

    /// `LLVMValueRef LLVMConstNull(LLVMTypeRef Ty)`
    pub fn LLVMConstNull(ty: LLVMTypeRef) -> LLVMValueRef;

    /// `LLVMValueRef LLVMConstPointerNull(LLVMTypeRef Ty)`
    ///
    /// The `null` half of a niched `T?` (Decision 19) and the `drop_fn` of a
    /// type that needs none (Decision 20).
    pub fn LLVMConstPointerNull(ty: LLVMTypeRef) -> LLVMValueRef;

    /// `LLVMValueRef LLVMConstStructInContext(LLVMContextRef C, LLVMValueRef *ConstantVals, unsigned Count, LLVMBool Packed)`
    pub fn LLVMConstStructInContext(
        c: LLVMContextRef,
        constant_vals: *mut LLVMValueRef,
        count: c_uint,
        packed: LLVMBool,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMAddGlobal(LLVMModuleRef M, LLVMTypeRef Ty, const char *Name)`
    pub fn LLVMAddGlobal(m: LLVMModuleRef, ty: LLVMTypeRef, name: *const c_char) -> LLVMValueRef;

    /// `LLVMValueRef LLVMGetNamedGlobal(LLVMModuleRef M, const char *Name)`
    ///
    /// Null when there is none. Decision 20's *"a descriptor is never
    /// constructed at a call site and never constructed twice"* is enforced
    /// above the line by `science_codegen::descriptor::DescriptorTable`; this is
    /// how a call site finds the one global that table interned.
    pub fn LLVMGetNamedGlobal(m: LLVMModuleRef, name: *const c_char) -> LLVMValueRef;

    /// `void LLVMSetInitializer(LLVMValueRef GlobalVar, LLVMValueRef ConstantVal)`
    pub fn LLVMSetInitializer(global_var: LLVMValueRef, constant_val: LLVMValueRef);

    /// `void LLVMSetGlobalConstant(LLVMValueRef GlobalVar, LLVMBool IsConstant)`
    pub fn LLVMSetGlobalConstant(global_var: LLVMValueRef, is_constant: LLVMBool);

    /// `void LLVMSetLinkage(LLVMValueRef Global, LLVMLinkage Linkage)`
    pub fn LLVMSetLinkage(global: LLVMValueRef, linkage: c_uint);

    /// `void LLVMSetUnnamedAddress(LLVMValueRef Global, LLVMUnnamedAddr UnnamedAddr)`
    pub fn LLVMSetUnnamedAddress(global: LLVMValueRef, unnamed_addr: c_uint);

    /// `void LLVMSetAlignment(LLVMValueRef V, unsigned Bytes)`
    pub fn LLVMSetAlignment(v: LLVMValueRef, bytes: c_uint);

    // --- functions and attributes -----------------------------------------

    /// `LLVMValueRef LLVMAddFunction(LLVMModuleRef M, const char *Name, LLVMTypeRef FunctionTy)`
    pub fn LLVMAddFunction(
        m: LLVMModuleRef,
        name: *const c_char,
        function_ty: LLVMTypeRef,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMGetNamedFunction(LLVMModuleRef M, const char *Name)`
    ///
    /// Null when there is none. Declaring one symbol twice with
    /// [`LLVMAddFunction`] does **not** fail: LLVM renames the second to
    /// `foo.1`, and the call site then calls a function nothing defines. That is
    /// a link error rather than a miscompile, and it is still the wrong error to
    /// hand a user, so every declaration goes through this first.
    pub fn LLVMGetNamedFunction(m: LLVMModuleRef, name: *const c_char) -> LLVMValueRef;

    /// `LLVMValueRef LLVMGetParam(LLVMValueRef Fn, unsigned Index)`
    pub fn LLVMGetParam(fn_: LLVMValueRef, index: c_uint) -> LLVMValueRef;

    /// `LLVMTypeRef LLVMTypeOf(LLVMValueRef Val)`
    ///
    /// Declared for one reason and it is a real one:
    /// `science_codegen::backend::Inst::Cmp` carries `signed: bool` and says
    /// *"irrelevant for floats"*, which leaves the backend to work out whether
    /// it is building an `icmp` or an `fcmp`. Asking the value is exact;
    /// inferring it from the layout of whatever produced it is a second model
    /// of the same fact.
    pub fn LLVMTypeOf(val: LLVMValueRef) -> LLVMTypeRef;

    /// `LLVMTypeKind LLVMGetTypeKind(LLVMTypeRef Ty)`
    ///
    /// The members used are [`type_kind::FLOAT`] and [`type_kind::DOUBLE`].
    /// Their numbering is positional in `LLVMTypeKind`, where `Void` is 0,
    /// `Half` is 1, `Float` is 2 and `Double` is 3.
    pub fn LLVMGetTypeKind(ty: LLVMTypeRef) -> c_uint;

    /// `unsigned LLVMGetEnumAttributeKindForName(const char *Name, size_t SLen)`
    ///
    /// Returns **0** for a name LLVM does not know, which is the trap: a typo in
    /// `"nocapture"` produces kind 0, and kind 0 handed to
    /// [`LLVMCreateEnumAttribute`] is an assertion failure in a debug LLVM and
    /// an attribute with no meaning in a release one. [`crate::owned::Attrs`]
    /// looks every name up once, at module start, and fails loudly on 0 rather
    /// than carrying it into an emitted call.
    pub fn LLVMGetEnumAttributeKindForName(name: *const c_char, s_len: usize) -> c_uint;

    /// `LLVMAttributeRef LLVMCreateEnumAttribute(LLVMContextRef C, unsigned KindID, uint64_t Val)`
    ///
    /// `Val` is 0 for a flag attribute (`nounwind`, `noalias`, `nocapture`,
    /// `readonly`) and the byte count for `align`.
    pub fn LLVMCreateEnumAttribute(c: LLVMContextRef, kind_id: c_uint, val: u64)
    -> LLVMAttributeRef;

    /// `LLVMAttributeRef LLVMCreateTypeAttribute(LLVMContextRef C, unsigned KindID, LLVMTypeRef type_ref)`
    ///
    /// **`sret` is a type attribute in LLVM 18, not a flag**, and this is the
    /// declaration §4.2 depends on. `sret(%ScienceString)` names the type the
    /// hidden pointer points at; a bare `sret` flag is not expressible through
    /// [`LLVMCreateEnumAttribute`] and would be rejected by the verifier if it
    /// were.
    pub fn LLVMCreateTypeAttribute(
        c: LLVMContextRef,
        kind_id: c_uint,
        type_ref: LLVMTypeRef,
    ) -> LLVMAttributeRef;

    /// `void LLVMAddAttributeAtIndex(LLVMValueRef F, LLVMAttributeIndex Idx, LLVMAttributeRef A)`
    pub fn LLVMAddAttributeAtIndex(f: LLVMValueRef, idx: LLVMAttributeIndex, a: LLVMAttributeRef);

    /// `void LLVMAddCallSiteAttribute(LLVMValueRef C, LLVMAttributeIndex Idx, LLVMAttributeRef A)`
    ///
    /// The call site's copy, and it is not optional. LLVM requires `sret` on the
    /// **call** as well as on the declaration; a call without it passes the
    /// return slot as an ordinary first argument, which happens to be the same
    /// register on Windows x64 and is not the same on System V. A backend tested
    /// only on Windows would never find out.
    pub fn LLVMAddCallSiteAttribute(c: LLVMValueRef, idx: LLVMAttributeIndex, a: LLVMAttributeRef);

    // --- basic blocks and the builder -------------------------------------

    /// `LLVMBasicBlockRef LLVMAppendBasicBlockInContext(LLVMContextRef C, LLVMValueRef Fn, const char *Name)`
    pub fn LLVMAppendBasicBlockInContext(
        c: LLVMContextRef,
        fn_: LLVMValueRef,
        name: *const c_char,
    ) -> LLVMBasicBlockRef;

    /// `LLVMBuilderRef LLVMCreateBuilderInContext(LLVMContextRef C)`
    pub fn LLVMCreateBuilderInContext(c: LLVMContextRef) -> LLVMBuilderRef;

    /// `void LLVMDisposeBuilder(LLVMBuilderRef Builder)`
    pub fn LLVMDisposeBuilder(builder: LLVMBuilderRef);

    /// `void LLVMPositionBuilderAtEnd(LLVMBuilderRef Builder, LLVMBasicBlockRef Block)`
    pub fn LLVMPositionBuilderAtEnd(builder: LLVMBuilderRef, block: LLVMBasicBlockRef);

    /// `LLVMValueRef LLVMBuildAlloca(LLVMBuilderRef, LLVMTypeRef Ty, const char *Name)`
    ///
    /// Decision 8: *"every MIR local becomes an `alloca` in the function's entry
    /// block."* The entry-block half is the caller's job and it matters — an
    /// `alloca` in a loop body is a stack allocation per iteration.
    pub fn LLVMBuildAlloca(
        builder: LLVMBuilderRef,
        ty: LLVMTypeRef,
        name: *const c_char,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildLoad2(LLVMBuilderRef, LLVMTypeRef Ty, LLVMValueRef PointerVal, const char *Name)`
    ///
    /// The `2` suffix is the opaque-pointer era: the type being loaded has to be
    /// passed because the pointer no longer carries it. `LLVMBuildLoad` is gone
    /// in 18.
    pub fn LLVMBuildLoad2(
        builder: LLVMBuilderRef,
        ty: LLVMTypeRef,
        pointer_val: LLVMValueRef,
        name: *const c_char,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildStore(LLVMBuilderRef, LLVMValueRef Val, LLVMValueRef Ptr)`
    ///
    /// **Value first, pointer second** — the opposite of `memcpy` and of most
    /// people's memory of it.
    pub fn LLVMBuildStore(
        builder: LLVMBuilderRef,
        val: LLVMValueRef,
        ptr: LLVMValueRef,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildInBoundsGEP2(LLVMBuilderRef, LLVMTypeRef Ty, LLVMValueRef Pointer, LLVMValueRef *Indices, unsigned NumIndices, const char *Name)`
    ///
    /// **Declared now, and §2's *"no `LLVMBuildGEP2`"* is the sentence this
    /// deletes.** That note's reason was *"no aggregate projection is lowered
    /// yet"*, which stopped being true the moment a record had a field.
    ///
    /// **It is called with `Ty = i8` and one index, which is a byte offset, and
    /// that is a decision rather than a shortcut.** §2.3's table says
    /// *"`getelementptr inbounds` with the struct's field index"*, and the
    /// struct's field index is **not** available here: [`crate::emit`]'s
    /// `llvm_type` materialises padding, so a Science record's field 1 may be
    /// LLVM member 2 with an `[7 x i8]` between them. Two numberings for one
    /// field is §4.1's failure mode in miniature. `science-codegen`'s
    /// `FieldPlace::offset` is the authority everywhere else in this crate — it
    /// is what the descriptor carries and what `LLVMSetAlignment` is told — so
    /// it is the authority here too, and the GEP is the byte form that names it
    /// directly. `inbounds` is kept: the offset is inside the object by
    /// construction, and dropping it would cost alias analysis for nothing.
    pub fn LLVMBuildInBoundsGEP2(
        builder: LLVMBuilderRef,
        ty: LLVMTypeRef,
        pointer: LLVMValueRef,
        indices: *mut LLVMValueRef,
        num_indices: c_uint,
        name: *const c_char,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildCall2(LLVMBuilderRef, LLVMTypeRef, LLVMValueRef Fn, LLVMValueRef *Args, unsigned NumArgs, const char *Name)`
    ///
    /// The `LLVMTypeRef` is the **function type**, not the return type, and it is
    /// required for the same reason [`LLVMBuildLoad2`] needs one. A call whose
    /// type disagrees with the callee's is a verifier failure, which is Decision
    /// 34 earning its place rather than a theory about it.
    pub fn LLVMBuildCall2(
        builder: LLVMBuilderRef,
        ty: LLVMTypeRef,
        fn_: LLVMValueRef,
        args: *mut LLVMValueRef,
        num_args: c_uint,
        name: *const c_char,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildRet(LLVMBuilderRef, LLVMValueRef V)`
    pub fn LLVMBuildRet(builder: LLVMBuilderRef, v: LLVMValueRef) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildRetVoid(LLVMBuilderRef)`
    ///
    /// What an `sret` function returns: the value went through the hidden
    /// pointer, so there is nothing left to return.
    pub fn LLVMBuildRetVoid(builder: LLVMBuilderRef) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildBr(LLVMBuilderRef, LLVMBasicBlockRef Dest)`
    pub fn LLVMBuildBr(builder: LLVMBuilderRef, dest: LLVMBasicBlockRef) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildCondBr(LLVMBuilderRef, LLVMValueRef If, LLVMBasicBlockRef Then, LLVMBasicBlockRef Else)`
    pub fn LLVMBuildCondBr(
        builder: LLVMBuilderRef,
        if_: LLVMValueRef,
        then_: LLVMBasicBlockRef,
        else_: LLVMBasicBlockRef,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildSwitch(LLVMBuilderRef, LLVMValueRef V, LLVMBasicBlockRef Else, unsigned NumCases)`
    ///
    /// `NumCases` is a capacity hint, not a promise: cases are added afterwards
    /// with [`LLVMAddCase`] and a wrong hint costs one reallocation.
    pub fn LLVMBuildSwitch(
        builder: LLVMBuilderRef,
        v: LLVMValueRef,
        else_: LLVMBasicBlockRef,
        num_cases: c_uint,
    ) -> LLVMValueRef;

    /// `void LLVMAddCase(LLVMValueRef Switch, LLVMValueRef OnVal, LLVMBasicBlockRef Dest)`
    pub fn LLVMAddCase(switch: LLVMValueRef, on_val: LLVMValueRef, dest: LLVMBasicBlockRef);

    /// `LLVMValueRef LLVMBuildUnreachable(LLVMBuilderRef)`
    ///
    /// Decision 6: what follows `science_panic_bytes`. Never an `invoke`.
    pub fn LLVMBuildUnreachable(builder: LLVMBuilderRef) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildAdd(LLVMBuilderRef, LLVMValueRef LHS, LLVMValueRef RHS, const char *Name)`
    pub fn LLVMBuildAdd(
        b: LLVMBuilderRef,
        lhs: LLVMValueRef,
        rhs: LLVMValueRef,
        name: *const c_char,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildSub(LLVMBuilderRef, LLVMValueRef, LLVMValueRef, const char *)`
    pub fn LLVMBuildSub(
        b: LLVMBuilderRef,
        lhs: LLVMValueRef,
        rhs: LLVMValueRef,
        name: *const c_char,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildMul(LLVMBuilderRef, LLVMValueRef, LLVMValueRef, const char *)`
    pub fn LLVMBuildMul(
        b: LLVMBuilderRef,
        lhs: LLVMValueRef,
        rhs: LLVMValueRef,
        name: *const c_char,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildSDiv(LLVMBuilderRef, LLVMValueRef, LLVMValueRef, const char *)`
    pub fn LLVMBuildSDiv(
        b: LLVMBuilderRef,
        lhs: LLVMValueRef,
        rhs: LLVMValueRef,
        name: *const c_char,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildUDiv(LLVMBuilderRef, LLVMValueRef, LLVMValueRef, const char *)`
    pub fn LLVMBuildUDiv(
        b: LLVMBuilderRef,
        lhs: LLVMValueRef,
        rhs: LLVMValueRef,
        name: *const c_char,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildSRem(LLVMBuilderRef, LLVMValueRef, LLVMValueRef, const char *)`
    pub fn LLVMBuildSRem(
        b: LLVMBuilderRef,
        lhs: LLVMValueRef,
        rhs: LLVMValueRef,
        name: *const c_char,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildURem(LLVMBuilderRef, LLVMValueRef, LLVMValueRef, const char *)`
    pub fn LLVMBuildURem(
        b: LLVMBuilderRef,
        lhs: LLVMValueRef,
        rhs: LLVMValueRef,
        name: *const c_char,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildAnd(LLVMBuilderRef, LLVMValueRef, LLVMValueRef, const char *)`
    ///
    /// Bitwise, and only bitwise. §2.6: *"`and` / `or` — short-circuit: a `br`
    /// and a `phi`, never `and i1`."*
    pub fn LLVMBuildAnd(
        b: LLVMBuilderRef,
        lhs: LLVMValueRef,
        rhs: LLVMValueRef,
        name: *const c_char,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildOr(LLVMBuilderRef, LLVMValueRef, LLVMValueRef, const char *)`
    pub fn LLVMBuildOr(
        b: LLVMBuilderRef,
        lhs: LLVMValueRef,
        rhs: LLVMValueRef,
        name: *const c_char,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildXor(LLVMBuilderRef, LLVMValueRef, LLVMValueRef, const char *)`
    ///
    /// §2.6's `not`: `xor i1 %x, true`.
    pub fn LLVMBuildXor(
        b: LLVMBuilderRef,
        lhs: LLVMValueRef,
        rhs: LLVMValueRef,
        name: *const c_char,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildShl(LLVMBuilderRef, LLVMValueRef, LLVMValueRef, const char *)`
    pub fn LLVMBuildShl(
        b: LLVMBuilderRef,
        lhs: LLVMValueRef,
        rhs: LLVMValueRef,
        name: *const c_char,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildAShr(LLVMBuilderRef, LLVMValueRef, LLVMValueRef, const char *)`
    pub fn LLVMBuildAShr(
        b: LLVMBuilderRef,
        lhs: LLVMValueRef,
        rhs: LLVMValueRef,
        name: *const c_char,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildLShr(LLVMBuilderRef, LLVMValueRef, LLVMValueRef, const char *)`
    pub fn LLVMBuildLShr(
        b: LLVMBuilderRef,
        lhs: LLVMValueRef,
        rhs: LLVMValueRef,
        name: *const c_char,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildFAdd(LLVMBuilderRef, LLVMValueRef, LLVMValueRef, const char *)`
    ///
    /// **No flags parameter, and that is the API rather than a choice.** A
    /// fast-math flag is set afterwards with `LLVMSetFastMathFlags`, which this
    /// file does not declare — §7.3 obligation 1 is *"never set a fast-math
    /// flag"*, and the strongest available form of "never" is not to have the
    /// declaration. That is the same move
    /// `science_codegen::backend::FloatOp` makes one level up, where the enum
    /// has no place to put a flag.
    pub fn LLVMBuildFAdd(
        b: LLVMBuilderRef,
        lhs: LLVMValueRef,
        rhs: LLVMValueRef,
        name: *const c_char,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildFSub(LLVMBuilderRef, LLVMValueRef, LLVMValueRef, const char *)`
    pub fn LLVMBuildFSub(
        b: LLVMBuilderRef,
        lhs: LLVMValueRef,
        rhs: LLVMValueRef,
        name: *const c_char,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildFMul(LLVMBuilderRef, LLVMValueRef, LLVMValueRef, const char *)`
    pub fn LLVMBuildFMul(
        b: LLVMBuilderRef,
        lhs: LLVMValueRef,
        rhs: LLVMValueRef,
        name: *const c_char,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildFDiv(LLVMBuilderRef, LLVMValueRef, LLVMValueRef, const char *)`
    pub fn LLVMBuildFDiv(
        b: LLVMBuilderRef,
        lhs: LLVMValueRef,
        rhs: LLVMValueRef,
        name: *const c_char,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildFRem(LLVMBuilderRef, LLVMValueRef, LLVMValueRef, const char *)`
    pub fn LLVMBuildFRem(
        b: LLVMBuilderRef,
        lhs: LLVMValueRef,
        rhs: LLVMValueRef,
        name: *const c_char,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildICmp(LLVMBuilderRef, LLVMIntPredicate Op, LLVMValueRef LHS, LLVMValueRef RHS, const char *Name)`
    pub fn LLVMBuildICmp(
        b: LLVMBuilderRef,
        op: c_uint,
        lhs: LLVMValueRef,
        rhs: LLVMValueRef,
        name: *const c_char,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildFCmp(LLVMBuilderRef, LLVMRealPredicate Op, LLVMValueRef LHS, LLVMValueRef RHS, const char *Name)`
    pub fn LLVMBuildFCmp(
        b: LLVMBuilderRef,
        op: c_uint,
        lhs: LLVMValueRef,
        rhs: LLVMValueRef,
        name: *const c_char,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildNeg(LLVMBuilderRef, LLVMValueRef V, const char *Name)`
    ///
    /// `not` and unary `-` are §4.6's two prefix operators and both are
    /// stage 3's. `LLVMBuildNeg` emits `sub 0, v` with no `nsw`, which is the
    /// wrapping negation the core spec's release semantics asks for; see
    /// [`crate::lower`]'s overflow note for what is *not* emitted and why.
    pub fn LLVMBuildNeg(b: LLVMBuilderRef, v: LLVMValueRef, name: *const c_char)
    -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildFNeg(LLVMBuilderRef, LLVMValueRef V, const char *Name)`
    ///
    /// A float negation is a sign-bit flip and is exact, so it is one of the
    /// few float operations §7.3 has nothing to say about.
    pub fn LLVMBuildFNeg(b: LLVMBuilderRef, v: LLVMValueRef, name: *const c_char)
    -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildZExt(LLVMBuilderRef, LLVMValueRef Val, LLVMTypeRef DestTy, const char *Name)`
    ///
    /// §3.1: *"`Bool` is `i1` in registers and `i8` in memory."* A comparison
    /// produces the register form and a `Bool` local's slot is the memory
    /// form, so a store of one into the other is this instruction. Zero- and
    /// not sign-extension: `sext i1 -> i8` of `true` is `0xff`.
    pub fn LLVMBuildZExt(
        b: LLVMBuilderRef,
        value: LLVMValueRef,
        dest_ty: LLVMTypeRef,
        name: *const c_char,
    ) -> LLVMValueRef;

    /// `LLVMValueRef LLVMBuildTrunc(LLVMBuilderRef, LLVMValueRef Val, LLVMTypeRef DestTy, const char *Name)`
    ///
    /// The other direction of the same rule: `br` takes an `i1` and a `Bool`
    /// loaded from its slot is an `i8`. `crate::emit`'s branch used to refuse
    /// this case by name, saying *"narrowing it needs a `trunc`, which
    /// `crate::sys` does not declare"* — stage 3 is the day a `Bool` local
    /// reaches a branch, and this is the line that refusal named.
    pub fn LLVMBuildTrunc(
        b: LLVMBuilderRef,
        value: LLVMValueRef,
        dest_ty: LLVMTypeRef,
        name: *const c_char,
    ) -> LLVMValueRef;

    // --- target, target machine, emission ---------------------------------

    /// `void LLVMInitializeX86TargetInfo(void)`
    ///
    /// The four `LLVMInitializeX86*` entry points are real exported functions,
    /// not the header's `static inline LLVMInitializeAllTargets` wrappers — a
    /// distinction that matters here because the inline ones cannot be reached
    /// through an `extern` block at all, in Rust or in Science. A self-hosted
    /// backend has to call the per-target four by name.
    pub fn LLVMInitializeX86TargetInfo();

    /// `void LLVMInitializeX86Target(void)`
    pub fn LLVMInitializeX86Target();

    /// `void LLVMInitializeX86TargetMC(void)`
    pub fn LLVMInitializeX86TargetMC();

    /// `void LLVMInitializeX86AsmPrinter(void)`
    ///
    /// Required for the **object** writer as well as for `.s`: the MC layer's
    /// object streamer is registered by the asm printer's initialiser, so a
    /// backend that skips this gets *"target does not support generation of this
    /// file type"* out of [`LLVMTargetMachineEmitToFile`] and no other clue.
    pub fn LLVMInitializeX86AsmPrinter();

    /// `char *LLVMGetDefaultTargetTriple(void)` — caller frees.
    pub fn LLVMGetDefaultTargetTriple() -> *mut c_char;

    /// `LLVMBool LLVMGetTargetFromTriple(const char *Triple, LLVMTargetRef *T, char **ErrorMessage)`
    ///
    /// Returns **1 on failure**, with the message in `*ErrorMessage` and the
    /// caller owning it.
    pub fn LLVMGetTargetFromTriple(
        triple: *const c_char,
        t: *mut LLVMTargetRef,
        error_message: *mut *mut c_char,
    ) -> LLVMBool;

    /// `LLVMTargetMachineRef LLVMCreateTargetMachine(LLVMTargetRef T, const char *Triple, const char *CPU, const char *Features, LLVMCodeGenOptLevel Level, LLVMRelocMode Reloc, LLVMCodeModel CodeModel)`
    ///
    /// Seven parameters, three of them C enums passed as `int`. This is the
    /// declaration §7.3's obligation 2 would have had to reach through, and
    /// there is no eighth parameter for `AllowFPOpFusion`: see the module
    /// documentation §3, and [`crate::machine`] for what is done instead.
    pub fn LLVMCreateTargetMachine(
        t: LLVMTargetRef,
        triple: *const c_char,
        cpu: *const c_char,
        features: *const c_char,
        level: c_uint,
        reloc: c_uint,
        code_model: c_uint,
    ) -> LLVMTargetMachineRef;

    /// `void LLVMDisposeTargetMachine(LLVMTargetMachineRef T)`
    pub fn LLVMDisposeTargetMachine(t: LLVMTargetMachineRef);

    /// `LLVMTargetDataRef LLVMCreateTargetDataLayout(LLVMTargetMachineRef T)`
    ///
    /// **The data layout is LLVM's, and this crate does not compute one.** §3's
    /// layout rules are `science-codegen`'s and are computed above the line;
    /// what the module's `datalayout` string does is tell LLVM the same facts so
    /// that its `DataLayout` agrees with them. `tests/layout_agreement.rs` is the
    /// check that they do, and it is the only place two independent
    /// implementations of §3.2 can be compared without a C compiler.
    pub fn LLVMCreateTargetDataLayout(t: LLVMTargetMachineRef) -> LLVMTargetDataRef;

    /// `void LLVMDisposeTargetData(LLVMTargetDataRef TD)`
    pub fn LLVMDisposeTargetData(td: LLVMTargetDataRef);

    /// `char *LLVMCopyStringRepOfTargetData(LLVMTargetDataRef TD)` — caller frees.
    pub fn LLVMCopyStringRepOfTargetData(td: LLVMTargetDataRef) -> *mut c_char;

    /// `unsigned long long LLVMABISizeOfType(LLVMTargetDataRef TD, LLVMTypeRef Ty)`
    pub fn LLVMABISizeOfType(td: LLVMTargetDataRef, ty: LLVMTypeRef) -> c_ulonglong;

    /// `unsigned LLVMABIAlignmentOfType(LLVMTargetDataRef TD, LLVMTypeRef Ty)`
    pub fn LLVMABIAlignmentOfType(td: LLVMTargetDataRef, ty: LLVMTypeRef) -> c_uint;

    /// `LLVMBool LLVMTargetMachineEmitToFile(LLVMTargetMachineRef T, LLVMModuleRef M, const char *Filename, LLVMCodeGenFileType codegen, char **ErrorMessage)`
    ///
    /// §10's stage 0 in one call. Returns **1 on failure**. The `Filename`
    /// parameter became `const char *` in LLVM 17 and was `char *` before; it is
    /// the same pointer either way, and it is noted because a reader comparing
    /// this file against an older header will see the difference and wonder.
    pub fn LLVMTargetMachineEmitToFile(
        t: LLVMTargetMachineRef,
        m: LLVMModuleRef,
        filename: *const c_char,
        codegen: c_uint,
        error_message: *mut *mut c_char,
    ) -> LLVMBool;

    // --- verification and the pass pipeline -------------------------------

    /// `LLVMBool LLVMVerifyModule(LLVMModuleRef M, LLVMVerifierFailureAction Action, char **OutMessage)`
    ///
    /// Decision 34, and it is *"not optional"*. Returns **1 when the module is
    /// broken**.
    pub fn LLVMVerifyModule(
        m: LLVMModuleRef,
        action: c_uint,
        out_message: *mut *mut c_char,
    ) -> LLVMBool;

    /// `LLVMPassBuilderOptionsRef LLVMCreatePassBuilderOptions(void)`
    pub fn LLVMCreatePassBuilderOptions() -> LLVMPassBuilderOptionsRef;

    /// `void LLVMDisposePassBuilderOptions(LLVMPassBuilderOptionsRef Options)`
    pub fn LLVMDisposePassBuilderOptions(options: LLVMPassBuilderOptionsRef);

    /// `LLVMErrorRef LLVMRunPasses(LLVMModuleRef M, const char *Passes, LLVMTargetMachineRef TM, LLVMPassBuilderOptionsRef Options)`
    ///
    /// Decision 33's *"selected by string"*, literally: the `Passes` argument is
    /// `"default<O2>"`, which is [`science_codegen::target::OptLevel::pipeline`]
    /// with nothing added. Returns a **null** `LLVMErrorRef` on success, which is
    /// the one function here whose success value is not an integer zero.
    pub fn LLVMRunPasses(
        m: LLVMModuleRef,
        passes: *const c_char,
        tm: LLVMTargetMachineRef,
        options: LLVMPassBuilderOptionsRef,
    ) -> LLVMErrorRef;

    /// `char *LLVMGetErrorMessage(LLVMErrorRef Err)`
    ///
    /// **Consumes the error**, so it is called exactly once and the handle is
    /// dead afterwards. The returned string is the caller's and is freed with
    /// [`LLVMDisposeErrorMessage`], not with [`LLVMDisposeMessage`]; they are the
    /// same allocator today and the header says to use the matching one.
    pub fn LLVMGetErrorMessage(err: LLVMErrorRef) -> *mut c_char;

    /// `void LLVMDisposeErrorMessage(char *ErrMsg)`
    pub fn LLVMDisposeErrorMessage(err_msg: *mut c_char);
}
