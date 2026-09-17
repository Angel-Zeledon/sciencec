//! `LlvmBackend` — Decision 42's line, from below.
//!
//! **The decision.** This module implements
//! [`science_codegen::backend::Backend`] and decides nothing. It computes no
//! layout, classifies no return, mangles no name and chooses no attribute; every
//! one of those arrives already decided in a [`Layout`], an [`AbiSignature`] or
//! a [`TargetConfig`]. The `science-codegen-llvm` crate does not contain the
//! word `classify` outside a comment, and that is checkable.
//!
//! **The reason** is Decision 42's own, and it is worth restating in the place
//! it applies to rather than only where it was made:
//!
//! > *A second backend need not agree with LLVM about instruction selection. It
//! > **must** agree about struct offsets, enum discriminant positions, niche
//! > encodings, `sret` classification, symbol names and descriptor contents.*
//!
//! **The cost, and this crate paid it immediately.** §8.4 says the discipline
//! *"actually fails, because the fastest way to fix a bug at eleven at night is
//! always to let the backend peek at something above the line"*. There are three
//! places below where this crate reaches for something the line does not carry,
//! and rather than peek they are named: [`ExtInst::LocalAddr`],
//! [`ExtInst::ReturnSlot`] and [`ExtInst::LoadNiche`]. See §2.
//!
//! # 1. Types: the layout is `science-codegen`'s and LLVM is told, not asked
//!
//! [`llvm_type`] turns a [`Layout`] into an `LLVMTypeRef` by **materialising the
//! padding**. A `{ i8, i64 }` becomes `{ i8, [7 x i8], i64 }`, not `{ i8, i64 }`
//! with LLVM inserting seven bytes of its own. The two agree today on every type
//! F0 can build, and the point of not relying on that is that the day they
//! disagree, the disagreement is silent and is §4.1's failure mode:
//! `science-codegen`'s `layout` would say one offset, LLVM's `DataLayout` would
//! say another, and the descriptor handed to `science-rt` would carry the first
//! while the emitted `getelementptr` used the second.
//!
//! `tests/layout_agreement.rs` asserts the agreement anyway, for all eight of
//! `science-rt`'s aggregates, by asking LLVM's own `LLVMABISizeOfType` — which
//! makes it the third independent implementation of §3.2 in the workspace, after
//! `science-codegen::layout` and `rustc`'s `#[repr(C)]`.
//!
//! **What is not modelled:** the structs are anonymous. Decision 11 asks for a
//! *named* LLVM struct per Science `type`, and [`Layout`] carries field names
//! but not the aggregate's own name, so there is nothing here to name it with.
//! `LLVMStructCreateNamed` is declared and unused for that reason; the fix is a
//! field on `Layout`, which is above the line.
//!
//! # 2. The three things the interface cannot say
//!
//! `science_codegen::backend::Operand` has `Value`, `ConstInt`, `ConstFloat`,
//! `GlobalAddr` and `Null`, and `Inst` has `Alloca`, `Load`, `Store`, two
//! binaries, a `Cmp` and a `Call`. Stage 1 — one script body, one string
//! literal, one `print` — needs three things that vocabulary cannot express, and
//! each one is a variant of [`ExtInst`] rather than a peek:
//!
//! 1. **The address of a local** ([`ExtInst::LocalAddr`]). `science_print` takes
//!    `*const ScienceString`, and the string it prints lives in the `alloca`
//!    that the `sret` call before it filled in. `science-codegen`'s own
//!    `tests/stage_one.rs` runs into this and passes
//!    `Operand::Value(ValueId(0))` — a value no instruction in that body
//!    produces. Against a text backend that renders `%0` and nobody notices;
//!    against LLVM it is an undefined reference, which is how it was found.
//! 2. **The hidden return slot** ([`ExtInst::ReturnSlot`]). MIR's `_0` *is* the
//!    caller's slot when the return is classified `Indirect`, and Decision 8's
//!    *"every local becomes an `alloca`"* makes a private copy of it instead.
//!    This one was silent: the emitted `main` read an untouched slot and took
//!    the error branch.
//! 3. **The niche of a nullable, alone** ([`ExtInst::LoadNiche`]). `Inst::Load`
//!    loads a local's whole type, and §3.4 forbids loading the vtable word of a
//!    possibly-null interface object *"including on the path that tests for
//!    null"*.
//!
//! Every one is a local extension and every one is meant to be deleted: when
//! `Inst` grows an `AddrOf`, `Operand` grows a `Param`, and either grows a field
//! projection, [`ExtInst`] collapses to `Inst` and this section goes with it.
//! Both entry points run the same emitter, so the trait's `define_function` and
//! the extension's `define_function_ext` cannot drift.
//!
//! **A fourth gap is named and not repaired.** `Operand::ConstInt` carries an
//! `i128` and no type, so a constant's width has to be inferred; see
//! [`LlvmBackend::width_hint`] for what that costs and where it stops.
//!
//! # 3. Verification is not optional and runs twice
//!
//! Decision 34. [`LlvmBackend::verify`] is called by the pipeline and again
//! inside [`LlvmBackend::emit`], *"because verifying twice costs milliseconds;
//! emitting an unverified module costs a miscompile"*. A failure carries the
//! whole module's IR, not a summary, because Decision 34 asks for the offending
//! function's IR and this crate does not have a per-function printer declared.

use std::collections::BTreeMap;
use std::ffi::c_uint;

use science_codegen::abi::{AbiParam, AbiSignature, ArgClass, ReturnClass};
use science_codegen::backend::{
    Backend, BackendError, BlockId, Body, Callee, CmpOp, EmitKind, FloatOp, FuncId, Inst,
    IntOp, LocalId, Operand, Terminator, ValueId,
};
use science_codegen::descriptor::{MapInfo, StringLiteral, TypeInfo};
use science_codegen::layout::{FloatTy, Layout, Repr, Scalar, Triple};
use science_codegen::target::TargetConfig;

use crate::machine;
use crate::owned::{Attrs, Builder, Context, Module, TargetMachine, cstr};
use crate::sys;

/// An instruction, plus the one form the interface above the line cannot spell.
///
/// See the module documentation §2. Every variant but [`ExtInst::LocalAddr`] is
/// `science_codegen`'s, carried through untouched.
#[derive(Debug, Clone, PartialEq)]
pub enum ExtInst {
    /// An instruction from above the line.
    Above(Inst),
    /// The address of a local's `alloca`.
    ///
    /// **This is the hole, and it is one instruction wide.** Decision 22 passes
    /// every aggregate argument *"by pointer to a caller-owned slot"*, and the
    /// runtime boundary does the same — every one of the 45 entry points takes
    /// aggregates as `*const`/`*mut`. So the commonest operand in the language's
    /// commonest call shape is "the address of that local", and it has no
    /// spelling.
    LocalAddr {
        /// Where the pointer goes.
        dest: ValueId,
        /// Whose address.
        local: LocalId,
    },
    /// Bind a local to the function's hidden `sret` pointer instead of giving
    /// it an `alloca`.
    ///
    /// **The second hole, and it is the one that produces a wrong answer rather
    /// than a verifier failure.** Decision 8 says *"every MIR local becomes an
    /// `alloca` in the function's entry block"*, and MIR's local `_0` is the
    /// return place. For a function classified `Indirect` those two sentences
    /// contradict each other: `_0` **is** the caller's slot, reached through the
    /// hidden parameter, and an `alloca` for it is a second, private copy that
    /// the `ret` never transfers anywhere. `science_codegen::backend::Operand`
    /// has no `Param` form — [`BodyState::sret`] is populated and unreadable for
    /// exactly this reason — so there is no way to say "`_0` lives there"
    /// without one new instruction.
    ///
    /// The failure it fixes is silent: `_S4main` stored `null` into its own
    /// stack slot, returned, and `main` read an untouched slot and took the
    /// error branch on whatever the stack happened to hold. Nothing verifies
    /// differently and nothing links differently; the program simply aborts
    /// instead of printing.
    ///
    /// Like [`ExtInst::LocalAddr`] this is meant to be deleted, and it collapses
    /// the same way: when `Inst` can name a parameter, `_0` is bound by an
    /// ordinary instruction and this variant goes.
    ReturnSlot {
        /// The local the hidden pointer stands for — `_0` in every case F0 can
        /// build, and not assumed to be, because the emitter can check.
        local: LocalId,
        /// `_0`'s layout, which is the function's `ret_layout`. Carried so the
        /// slot can be typed for a later `Load` without asking the signature
        /// twice.
        layout: Layout,
    },
    /// Load a `Repr::Niched` local's **niche scalar**, and nothing else.
    ///
    /// **Not `Inst::Load`, and the difference is §3.4's rule.** `Inst::Load`
    /// loads a local's whole type. For `(any Error)?` that is `{ ptr, ptr }`,
    /// and §3.4 says *"the vtable slot of a null trait object is undefined and
    /// codegen must never load it — including on the path that tests for
    /// null"*. Loading the pair is an `icmp` between a struct and a pointer,
    /// which the verifier rejects — so this one is loud rather than silent, and
    /// it is the reason `hello, world` did not compile rather than the reason it
    /// would have misbehaved.
    ///
    /// Restricted to a niche at offset 0, because [`crate::sys`] declares no
    /// `LLVMBuildGEP2` — §2 of that module says why — so there is no way to
    /// address a field that is not the first. Every niche F0 can build is at 0
    /// (§3.4 puts it in the payload's first pointer), and a niche that is not is
    /// refused by the emitter rather than loaded from the wrong place.
    LoadNiche {
        /// Where the scalar goes.
        dest: ValueId,
        /// The nullable local.
        local: LocalId,
    },
}

/// A basic block in the extended instruction set.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtBlock {
    /// The block's identifier.
    pub id: BlockId,
    /// A label, for IR readability.
    pub label: String,
    /// The instructions, in order.
    pub insts: Vec<ExtInst>,
    /// How it ends.
    pub terminator: Terminator,
}

/// A function body in the extended instruction set.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtBody {
    /// Blocks in order. The first is the entry block and holds every `alloca`.
    pub blocks: Vec<ExtBlock>,
}

impl ExtBody {
    /// Lift a body from above the line.
    pub fn lift(body: &Body) -> ExtBody {
        ExtBody {
            blocks: body
                .blocks
                .iter()
                .map(|block| ExtBlock {
                    id: block.id,
                    label: block.label.clone(),
                    insts: block.insts.iter().cloned().map(ExtInst::Above).collect(),
                    terminator: block.terminator.clone(),
                })
                .collect(),
        }
    }
}

/// What a declared function is, from the emitter's side.
struct Declared {
    value: sys::LLVMValueRef,
    fn_type: sys::LLVMTypeRef,
    sig: AbiSignature,
}

/// The LLVM backend.
///
/// **Field order is load-bearing.** Rust drops fields in declaration order, and
/// the module, the builder and every type and value derived from the context
/// must die before the context does. `builder` and `module` are therefore above
/// `context`, and moving `context` up is a use-after-free with no compiler
/// error. [`crate::owned`]'s "ordering rule" section says the same from the
/// other side.
pub struct LlvmBackend {
    builder: Builder,
    module: Option<Module>,
    machine: Option<TargetMachine>,
    context: Context,
    attrs: Option<Attrs>,
    config: Option<TargetConfig>,
    declared: BTreeMap<String, Declared>,
    order: Vec<String>,
    verified: bool,
}

impl LlvmBackend {
    /// A backend with a fresh context and the X86 target registered.
    pub fn new() -> LlvmBackend {
        let context = machine::context();
        let builder = Builder::new(&context);
        LlvmBackend {
            builder,
            module: None,
            machine: None,
            context,
            attrs: None,
            config: None,
            declared: BTreeMap::new(),
            order: Vec::new(),
            verified: false,
        }
    }

    /// The module as LLVM IR text, for a test or for `--emit=llvm-ir`.
    pub fn ir(&self) -> String {
        self.module.as_ref().map(|m| m.print()).unwrap_or_default()
    }

    /// The target machine, once [`Backend::begin_module`] has run.
    pub fn machine(&self) -> Option<&TargetMachine> {
        self.machine.as_ref()
    }

    /// The context, for a test that wants to build a type of its own.
    pub fn context(&self) -> &Context {
        &self.context
    }

    fn module_ref(&self) -> Result<&Module, BackendError> {
        self.module.as_ref().ok_or_else(|| {
            BackendError::Other("no module: `begin_module` was not called".to_string())
        })
    }

    fn attrs_ref(&self) -> Result<&Attrs, BackendError> {
        self.attrs.as_ref().ok_or_else(|| {
            BackendError::Other("no attribute table: `begin_module` was not called".to_string())
        })
    }

    fn triple(&self) -> Triple {
        self.config.as_ref().map(|c| c.triple()).unwrap_or(Triple::X86_64LinuxGnu)
    }

    // --- types ------------------------------------------------------------

    fn void_ty(&self) -> sys::LLVMTypeRef {
        unsafe { sys::LLVMVoidTypeInContext(self.context.raw()) }
    }

    fn ptr_ty(&self) -> sys::LLVMTypeRef {
        unsafe { sys::LLVMPointerTypeInContext(self.context.raw(), 0) }
    }

    fn int_ty(&self, bits: u32) -> sys::LLVMTypeRef {
        unsafe { sys::LLVMIntTypeInContext(self.context.raw(), bits as c_uint) }
    }

    fn byte_array(&self, len: u64) -> sys::LLVMTypeRef {
        unsafe { sys::LLVMArrayType2(sys::LLVMInt8TypeInContext(self.context.raw()), len) }
    }

    fn anon_struct(&self, members: &mut [sys::LLVMTypeRef]) -> sys::LLVMTypeRef {
        unsafe {
            sys::LLVMStructTypeInContext(
                self.context.raw(),
                members.as_mut_ptr(),
                members.len() as c_uint,
                0,
            )
        }
    }

    /// A [`Layout`] as an LLVM type, with padding materialised.
    ///
    /// Public so that `tests/layout_agreement.rs` can ask LLVM's own
    /// `DataLayout` what it makes of the type this builds. That test is the only
    /// place two independent implementations of §3.2 can be compared without a C
    /// compiler, and it needs this function to have one of them.
    ///
    /// See the module documentation §1 for why the padding is explicit. The
    /// `Tagged` case is the one that is *not* structural: a discriminant plus a
    /// union has no LLVM spelling, so it becomes an array of `align`-sized
    /// integers with the right size and alignment, and the tag is read back with
    /// a typed load at offset 0 when something needs it. Nothing in stage 1
    /// does; `Error?` is `Nullable(Interface)`, and an interface object has a
    /// null niche in its data pointer (Decision 19), so it takes the `Niched`
    /// path and never the tagged one.
    pub fn llvm_type(&self, layout: &Layout) -> sys::LLVMTypeRef {
        match &layout.repr {
            Repr::Zero => self.anon_struct(&mut []),
            Repr::Scalar(scalar) => self.scalar_ty(*scalar),
            Repr::Aggregate { fields } => {
                let mut members = Vec::with_capacity(fields.len() * 2 + 1);
                let mut offset = 0u64;
                for field in fields {
                    if field.offset > offset {
                        members.push(self.byte_array(field.offset - offset));
                        offset = field.offset;
                    }
                    members.push(self.llvm_type(&field.layout));
                    offset += field.layout.size;
                }
                if layout.size > offset {
                    members.push(self.byte_array(layout.size - offset));
                }
                self.anon_struct(&mut members)
            }
            Repr::Tagged { .. } => {
                let unit = self.int_ty((layout.align * 8) as u32);
                unsafe { sys::LLVMArrayType2(unit, layout.size / layout.align) }
            }
            Repr::Niched { payload, .. } => self.llvm_type(payload),
        }
    }

    /// A scalar's LLVM type, in its **memory** form.
    ///
    /// §3.1: *"`Bool` is `i1` in registers and `i8` in memory."* Everything this
    /// function is asked about is a thing being stored, loaded, passed or
    /// returned, so `i8` is the answer, and the `i1` form appears only where a
    /// comparison produces one.
    fn scalar_ty(&self, scalar: Scalar) -> sys::LLVMTypeRef {
        match scalar {
            Scalar::Bool => unsafe { sys::LLVMInt8TypeInContext(self.context.raw()) },
            Scalar::Char => unsafe { sys::LLVMInt32TypeInContext(self.context.raw()) },
            Scalar::Int(int) => self.int_ty((int.width(self.triple()) * 8) as u32),
            Scalar::Float(FloatTy::F32) => unsafe {
                sys::LLVMFloatTypeInContext(self.context.raw())
            },
            Scalar::Float(FloatTy::F64) => unsafe {
                sys::LLVMDoubleTypeInContext(self.context.raw())
            },
            Scalar::Pointer(_) => self.ptr_ty(),
        }
    }

    fn is_pointer(layout: &Layout) -> bool {
        match &layout.repr {
            Repr::Scalar(Scalar::Pointer(_)) => true,
            Repr::Niched { payload, .. } => Self::is_pointer(payload),
            _ => false,
        }
    }

    /// The LLVM function type for a classified signature, and whether it carries
    /// a hidden `sret` pointer.
    ///
    /// The `sret` slot is **not** in [`AbiSignature::params`] — that type's own
    /// note says so, and says why: *"a backend that prepended it to this list
    /// would double it the first time anyone iterated both"*. It is prepended
    /// here, once, and every attribute index below is offset by it.
    fn fn_type(&self, sig: &AbiSignature) -> (sys::LLVMTypeRef, bool) {
        let sret = sig.ret.is_sret();
        let mut params: Vec<sys::LLVMTypeRef> = Vec::with_capacity(sig.params.len() + 1);
        if sret {
            params.push(self.ptr_ty());
        }
        for param in &sig.params {
            match param.class {
                ArgClass::Ignore => {}
                ArgClass::Direct => params.push(self.llvm_type(&param.layout)),
                ArgClass::IndirectByPointer => params.push(self.ptr_ty()),
            }
        }
        let ret = match &sig.ret {
            ReturnClass::Void | ReturnClass::Indirect => self.void_ty(),
            ReturnClass::Direct { .. } => self.llvm_type(&sig.ret_layout),
        };
        let ty = unsafe {
            sys::LLVMFunctionType(ret, params.as_mut_ptr(), params.len() as c_uint, 0)
        };
        (ty, sret)
    }

    /// Apply a parameter's attributes at `index`, skipping the pointer-only ones
    /// on a parameter that is not a pointer.
    ///
    /// LLVM rejects `align`, `noalias`, `nocapture` and `readonly` on a
    /// non-pointer parameter, and `science_codegen::abi::ParamAttrs` is a plain
    /// record that does not know what it is attached to. A backend that applied
    /// them blindly would fail the verifier on the first `borrowed Int`
    /// parameter that arrived classified `Direct`.
    fn apply_param_attrs(
        &self,
        function: sys::LLVMValueRef,
        index: sys::LLVMAttributeIndex,
        param: &AbiParam,
    ) -> Result<(), BackendError> {
        let attrs = self.attrs_ref()?;
        let pointer =
            matches!(param.class, ArgClass::IndirectByPointer) || Self::is_pointer(&param.layout);
        if !pointer {
            return Ok(());
        }
        unsafe {
            if param.attrs.noalias {
                sys::LLVMAddAttributeAtIndex(function, index, attrs.noalias());
            }
            if param.attrs.nocapture {
                sys::LLVMAddAttributeAtIndex(function, index, attrs.nocapture());
            }
            if param.attrs.readonly {
                sys::LLVMAddAttributeAtIndex(function, index, attrs.readonly());
            }
            if let Some(align) = param.attrs.align {
                sys::LLVMAddAttributeAtIndex(function, index, attrs.align(align));
            }
        }
        Ok(())
    }

    // --- operands ---------------------------------------------------------

    fn operand(
        &self,
        state: &BodyState,
        op: &Operand,
        expected: Option<sys::LLVMTypeRef>,
    ) -> Result<sys::LLVMValueRef, BackendError> {
        Ok(match op {
            Operand::Value(ValueId(id)) => *state.values.get(id).ok_or_else(|| {
                BackendError::Other(format!(
                    "the body reads %{id}, which no instruction in it produces"
                ))
            })?,
            // **The expectation is filtered by kind, not trusted.**
            // `LLVMConstInt` of a `double` and `LLVMConstReal` of an `i64` are
            // assertion failures in a debug LLVM and undefined in a release one,
            // and the releases are what ship. An expectation of the wrong kind
            // means the interface above the line said "integer constant" where
            // the other operand is a float — a disagreement this level cannot
            // resolve — so the constant takes its default type and the verifier
            // reports the mismatch, which is the loud outcome rather than the
            // undefined one.
            Operand::ConstInt(value) => {
                let ty = expected
                    .filter(|ty| self.type_kind_of(*ty) == sys::type_kind::INTEGER)
                    .unwrap_or_else(|| self.int_ty(64));
                unsafe { sys::LLVMConstInt(ty, *value as u64, 1) }
            }
            Operand::ConstFloat(value) => {
                let ty = expected.filter(|ty| self.type_is_float(*ty)).unwrap_or_else(|| unsafe {
                    sys::LLVMDoubleTypeInContext(self.context.raw())
                });
                unsafe { sys::LLVMConstReal(ty, *value) }
            }
            Operand::GlobalAddr(symbol) => {
                let name = cstr(symbol);
                let module = self.module_ref()?;
                let global = unsafe { sys::LLVMGetNamedGlobal(module.raw(), name.as_ptr()) };
                if !global.is_null() {
                    global
                } else {
                    let function =
                        unsafe { sys::LLVMGetNamedFunction(module.raw(), name.as_ptr()) };
                    if function.is_null() {
                        return Err(BackendError::Other(format!(
                            "`@{symbol}` is named as a global address and the module has no such \
                             global or function"
                        )));
                    }
                    function
                }
            }
            Operand::Null => unsafe { sys::LLVMConstPointerNull(self.ptr_ty()) },
        })
    }

    // --- bodies -----------------------------------------------------------

    fn emit_body(
        &mut self,
        func: FuncId,
        sig: &AbiSignature,
        body: &ExtBody,
    ) -> Result<(), BackendError> {
        let symbol = self
            .order
            .get(func.0 as usize)
            .cloned()
            .ok_or_else(|| BackendError::Other(format!("no function {}", func.0)))?;
        let (function, sret) = {
            let declared = self.declared.get(&symbol).ok_or_else(|| {
                BackendError::Other(format!("`{symbol}` was not declared"))
            })?;
            if declared.sig != *sig {
                return Err(BackendError::Other(format!(
                    "`{symbol}` is being defined with a signature it was not declared with"
                )));
            }
            (declared.value, declared.sig.ret.is_sret())
        };

        let mut state = BodyState::default();
        // Every block up front, so that a `br` to a later block resolves.
        for block in &body.blocks {
            let label = cstr(&block.label);
            let bb = unsafe {
                sys::LLVMAppendBasicBlockInContext(self.context.raw(), function, label.as_ptr())
            };
            state.blocks.insert(block.id.0, bb);
        }
        // Parameters, by position, so that a body can read them. The `sret`
        // slot shifts every index by one.
        let base = usize::from(sret);
        let mut position = base;
        for (index, param) in sig.params.iter().enumerate() {
            if matches!(param.class, ArgClass::Ignore) {
                continue;
            }
            let value = unsafe { sys::LLVMGetParam(function, position as c_uint) };
            state.params.insert(index, value);
            position += 1;
        }
        if sret {
            state.sret = Some(unsafe { sys::LLVMGetParam(function, 0) });
        }

        for block in &body.blocks {
            let bb = state.blocks[&block.id.0];
            unsafe { sys::LLVMPositionBuilderAtEnd(self.builder.raw(), bb) };
            for inst in &block.insts {
                self.emit_inst(&mut state, inst)?;
            }
            self.emit_terminator(&state, sig, &block.terminator)?;
        }
        self.verified = false;
        Ok(())
    }

    fn emit_inst(&self, state: &mut BodyState, inst: &ExtInst) -> Result<(), BackendError> {
        let b = self.builder.raw();
        match inst {
            ExtInst::LocalAddr { dest, local } => {
                let slot = state.local(*local)?;
                state.values.insert(dest.0, slot);
            }
            ExtInst::ReturnSlot { local, layout } => {
                let slot = state.sret.ok_or_else(|| {
                    BackendError::Other(format!(
                        "local _{} is bound to the hidden return slot of a function that does \
                         not have one",
                        local.0
                    ))
                })?;
                let ty = self.llvm_type(layout);
                state.locals.insert(local.0, (slot, ty, layout.clone()));
            }
            ExtInst::LoadNiche { dest, local } => {
                let (slot, _, layout) = state.local_entry(*local)?;
                let Repr::Niched { niche, payload, .. } = &layout.repr else {
                    return Err(BackendError::Other(format!(
                        "local _{} is read for a niche and its representation has none",
                        local.0
                    )));
                };
                if niche.offset != 0 {
                    return Err(BackendError::Unsupported {
                        what: format!(
                            "a niche at offset {}: reading it needs a `getelementptr` and \
                             `crate::sys` declares none",
                            niche.offset
                        ),
                    });
                }
                let scalar = science_codegen::layout::scalar_leaves(payload)
                    .into_iter()
                    .find(|(offset, _)| *offset == 0)
                    .map(|(_, scalar)| scalar)
                    .ok_or_else(|| {
                        BackendError::Other(format!(
                            "local _{} has a niche at offset 0 and no scalar there",
                            local.0
                        ))
                    })?;
                let ty = self.scalar_ty(scalar);
                let name = cstr(&format!("v{}", dest.0));
                let value = unsafe { sys::LLVMBuildLoad2(b, ty, slot, name.as_ptr()) };
                state.values.insert(dest.0, value);
            }
            ExtInst::Above(Inst::Alloca { local, layout }) => {
                let ty = self.llvm_type(layout);
                let name = cstr(&format!("l{}", local.0));
                let slot = unsafe { sys::LLVMBuildAlloca(b, ty, name.as_ptr()) };
                // `science-codegen`'s layout is the authority, so the alloca is
                // told the alignment rather than left to LLVM's idea of the
                // type's. They agree; the descriptor handed to `science-rt`
                // carries the first, so the first is what the stack slot uses.
                unsafe { sys::LLVMSetAlignment(slot, layout.align as c_uint) };
                state.locals.insert(local.0, (slot, ty, layout.clone()));
            }
            ExtInst::Above(Inst::Load { dest, local }) => {
                let (slot, ty, _) = state.local_entry(*local)?;
                let name = cstr(&format!("v{}", dest.0));
                let value = unsafe { sys::LLVMBuildLoad2(b, ty, slot, name.as_ptr()) };
                state.values.insert(dest.0, value);
            }
            ExtInst::Above(Inst::Store { local, value }) => {
                let (slot, ty, _) = state.local_entry(*local)?;
                let v = self.operand(state, value, Some(ty))?;
                unsafe { sys::LLVMBuildStore(b, v, slot) };
            }
            ExtInst::Above(Inst::IntBinary { dest, op, lhs, rhs }) => {
                let hint = self.width_hint(state, lhs, rhs).or_else(|| Some(self.int_ty(64)));
                let l = self.operand(state, lhs, hint)?;
                let r = self.operand(state, rhs, hint)?;
                let build = match op {
                    IntOp::Add => sys::LLVMBuildAdd,
                    IntOp::Sub => sys::LLVMBuildSub,
                    IntOp::Mul => sys::LLVMBuildMul,
                    IntOp::SDiv => sys::LLVMBuildSDiv,
                    IntOp::UDiv => sys::LLVMBuildUDiv,
                    IntOp::SRem => sys::LLVMBuildSRem,
                    IntOp::URem => sys::LLVMBuildURem,
                    IntOp::And => sys::LLVMBuildAnd,
                    IntOp::Or => sys::LLVMBuildOr,
                    IntOp::Xor => sys::LLVMBuildXor,
                    IntOp::Shl => sys::LLVMBuildShl,
                    IntOp::AShr => sys::LLVMBuildAShr,
                    IntOp::LShr => sys::LLVMBuildLShr,
                };
                let name = cstr(&format!("v{}", dest.0));
                state.values.insert(dest.0, unsafe { build(b, l, r, name.as_ptr()) });
            }
            ExtInst::Above(Inst::FloatBinary { dest, op, lhs, rhs }) => {
                let double = unsafe { sys::LLVMDoubleTypeInContext(self.context.raw()) };
                let hint = self.width_hint(state, lhs, rhs).or(Some(double));
                let l = self.operand(state, lhs, hint)?;
                let r = self.operand(state, rhs, hint)?;
                // No flags are set on the result, here or anywhere: §7.3
                // obligation 1, and `LLVMSetFastMathFlags` is not declared.
                let build = match op {
                    FloatOp::Add => sys::LLVMBuildFAdd,
                    FloatOp::Sub => sys::LLVMBuildFSub,
                    FloatOp::Mul => sys::LLVMBuildFMul,
                    FloatOp::Div => sys::LLVMBuildFDiv,
                    FloatOp::Rem => sys::LLVMBuildFRem,
                };
                let name = cstr(&format!("v{}", dest.0));
                state.values.insert(dest.0, unsafe { build(b, l, r, name.as_ptr()) });
            }
            ExtInst::Above(Inst::Cmp { dest, op, signed, lhs, rhs }) => {
                let hint = self.width_hint(state, lhs, rhs);
                let l = self.operand(state, lhs, hint)?;
                let r = self.operand(state, rhs, hint)?;
                let name = cstr(&format!("v{}", dest.0));
                // **Either side, not the left one.** `Cmp { lhs: ConstInt(0),
                // rhs: Value(a double) }` is an `Operand` pair the interface
                // above the line can build, and asking only `lhs` answers
                // "integer" for it.
                let is_float = self.value_is_float(l) || self.value_is_float(r);
                let value = if is_float {
                    let predicate = match op {
                        CmpOp::Eq => sys::real_predicate::OEQ,
                        CmpOp::Ne => sys::real_predicate::ONE,
                        CmpOp::Lt => sys::real_predicate::OLT,
                        CmpOp::Le => sys::real_predicate::OLE,
                        CmpOp::Gt => sys::real_predicate::OGT,
                        CmpOp::Ge => sys::real_predicate::OGE,
                    };
                    unsafe { sys::LLVMBuildFCmp(b, predicate, l, r, name.as_ptr()) }
                } else {
                    let predicate = match (op, signed) {
                        (CmpOp::Eq, _) => sys::int_predicate::EQ,
                        (CmpOp::Ne, _) => sys::int_predicate::NE,
                        (CmpOp::Lt, true) => sys::int_predicate::SLT,
                        (CmpOp::Lt, false) => sys::int_predicate::ULT,
                        (CmpOp::Le, true) => sys::int_predicate::SLE,
                        (CmpOp::Le, false) => sys::int_predicate::ULE,
                        (CmpOp::Gt, true) => sys::int_predicate::SGT,
                        (CmpOp::Gt, false) => sys::int_predicate::UGT,
                        (CmpOp::Ge, true) => sys::int_predicate::SGE,
                        (CmpOp::Ge, false) => sys::int_predicate::UGE,
                    };
                    unsafe { sys::LLVMBuildICmp(b, predicate, l, r, name.as_ptr()) }
                };
                state.values.insert(dest.0, value);
            }
            ExtInst::Above(Inst::Call { dest, callee, args, ret, sret_slot }) => {
                self.emit_call(state, dest, callee, args, ret, sret_slot)?;
            }
        }
        Ok(())
    }

    /// The type a constant operand should take, read off whichever operand is
    /// a value.
    ///
    /// **The defect this closes.** [`LlvmBackend::operand`] types an
    /// `Operand::ConstInt` as `i64` and an `Operand::ConstFloat` as `double`
    /// when it is given no expectation, and the three binary instructions gave
    /// it none: `IntBinary` passed `None` for both sides and `FloatBinary`
    /// passed `double` for both. So `x + 1` on an `I32` built
    /// `add i32 %x, i64 1`, and `x < 1.0` on an `F32` built
    /// `fcmp olt float %x, double 1.0`. Both are verifier failures rather than
    /// miscompiles — Decision 34 catches them — and both make every
    /// non-`Int`/non-`F64` arithmetic expression in the language uncompilable
    /// the day stage 3 reaches one. Nothing in stage 1 reaches one, which is why
    /// the module compiled with the defect in it.
    ///
    /// `science_codegen::backend::Operand::ConstInt` carries an `i128` and no
    /// type, so the width has to come from somewhere; the other operand is the
    /// only place, and it is exact whenever it is a value. Two constants
    /// compared against each other still fall back to the default, which is the
    /// one case where the interface above the line genuinely does not say.
    fn width_hint(
        &self,
        state: &BodyState,
        lhs: &Operand,
        rhs: &Operand,
    ) -> Option<sys::LLVMTypeRef> {
        for operand in [lhs, rhs] {
            if let Operand::Value(ValueId(id)) = operand {
                if let Some(value) = state.values.get(id) {
                    return Some(unsafe { sys::LLVMTypeOf(*value) });
                }
            }
        }
        None
    }

    /// Whether a value is an integer of exactly `bits` bits.
    ///
    /// `LLVMGetIntTypeWidth` would answer directly and is not declared;
    /// comparing against a freshly built `iN` is the same answer through the
    /// declarations that already exist, because LLVM interns types in a context
    /// and two `i1`s from one context are one pointer.
    fn value_is_int_of_width(&self, value: sys::LLVMValueRef, bits: u32) -> bool {
        let ty = unsafe { sys::LLVMTypeOf(value) };
        self.type_kind_of(ty) == sys::type_kind::INTEGER && ty == self.int_ty(bits)
    }

    /// An LLVM type's kind.
    fn type_kind_of(&self, ty: sys::LLVMTypeRef) -> c_uint {
        unsafe { sys::LLVMGetTypeKind(ty) }
    }

    /// Whether a type is `float` or `double`.
    fn type_is_float(&self, ty: sys::LLVMTypeRef) -> bool {
        let kind = self.type_kind_of(ty);
        kind == sys::type_kind::FLOAT || kind == sys::type_kind::DOUBLE
    }

    /// Whether a value's type is a float, so that [`CmpOp`] picks `fcmp` over
    /// `icmp`.
    ///
    /// `science_codegen::backend::Inst::Cmp` carries `signed: bool` and says
    /// *"irrelevant for floats"*, which leaves the backend to work out which it
    /// has. Asking LLVM is exact; inferring it from whatever produced the value
    /// would be a second model of a fact the value already carries.
    fn value_is_float(&self, value: sys::LLVMValueRef) -> bool {
        self.type_is_float(unsafe { sys::LLVMTypeOf(value) })
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_call(
        &self,
        state: &mut BodyState,
        dest: &Option<ValueId>,
        callee: &Callee,
        args: &[Operand],
        ret: &ReturnClass,
        sret_slot: &Option<LocalId>,
    ) -> Result<(), BackendError> {
        let symbol = match callee {
            Callee::Runtime(symbol) => (*symbol).to_string(),
            Callee::Science(symbol) | Callee::Foreign(symbol) => symbol.clone(),
            Callee::Intrinsic(intrinsic) => {
                return Err(BackendError::Unsupported {
                    what: format!(
                        "a call to `{}`: Decision 37's intrinsic whitelist is reachable from \
                         above the line but nothing in stage 1 emits one, so the declaration is \
                         not built yet",
                        intrinsic.llvm_name()
                    ),
                });
            }
        };
        let declared = self
            .declared
            .get(&symbol)
            .ok_or_else(|| BackendError::Other(format!("`{symbol}` is called and not declared")))?;
        let function = declared.value;
        let fn_type = declared.fn_type;
        let sig = &declared.sig;

        let mut values: Vec<sys::LLVMValueRef> = Vec::with_capacity(args.len() + 1);
        if ret.is_sret() {
            let slot = sret_slot.ok_or_else(|| {
                BackendError::Other(format!(
                    "`{symbol}` returns indirectly and the call carries no return slot; §9.2's \
                     failure mode is exactly this and it corrupts a register"
                ))
            })?;
            values.push(state.local(slot)?);
        }
        let expected: Vec<Option<sys::LLVMTypeRef>> = sig
            .params
            .iter()
            .filter(|p| !matches!(p.class, ArgClass::Ignore))
            .map(|p| match p.class {
                ArgClass::IndirectByPointer => Some(self.ptr_ty()),
                _ => Some(self.llvm_type(&p.layout)),
            })
            .collect();
        if args.len() != expected.len() {
            return Err(BackendError::Other(format!(
                "`{symbol}` takes {} arguments and the call passes {}",
                expected.len(),
                args.len()
            )));
        }
        for (arg, ty) in args.iter().zip(expected) {
            values.push(self.operand(state, arg, ty)?);
        }

        let name = match dest {
            // An `sret` call and a `void` call produce no value, and LLVM
            // refuses to name one.
            Some(ValueId(id)) if !matches!(ret, ReturnClass::Void) && !ret.is_sret() => {
                cstr(&format!("v{id}"))
            }
            _ => cstr(""),
        };
        let call = unsafe {
            sys::LLVMBuildCall2(
                self.builder.raw(),
                fn_type,
                function,
                values.as_mut_ptr(),
                values.len() as c_uint,
                name.as_ptr(),
            )
        };
        if ret.is_sret() {
            // The attribute has to be on the call as well as on the
            // declaration; `sys::LLVMAddCallSiteAttribute`'s note says what a
            // missing one costs on System V.
            let attrs = self.attrs_ref()?;
            let slot_ty = self.llvm_type(&sig.ret_layout);
            unsafe {
                sys::LLVMAddCallSiteAttribute(
                    call,
                    sys::LLVM_ATTRIBUTE_FIRST_PARAM_INDEX,
                    attrs.sret(slot_ty),
                );
                sys::LLVMAddCallSiteAttribute(
                    call,
                    sys::LLVM_ATTRIBUTE_FIRST_PARAM_INDEX,
                    attrs.align(sig.ret_layout.align),
                );
            }
        }
        if let Some(ValueId(id)) = dest {
            if !matches!(ret, ReturnClass::Void) && !ret.is_sret() {
                state.values.insert(*id, call);
            }
        }
        Ok(())
    }

    fn emit_terminator(
        &self,
        state: &BodyState,
        sig: &AbiSignature,
        terminator: &Terminator,
    ) -> Result<(), BackendError> {
        let b = self.builder.raw();
        match terminator {
            Terminator::Return(value) => unsafe {
                match value {
                    // An `sret` function returns nothing: the value went
                    // through the hidden pointer before the terminator ran.
                    Some(_) if sig.ret.is_sret() => {
                        sys::LLVMBuildRetVoid(b);
                    }
                    Some(operand) => {
                        let ty = self.llvm_type(&sig.ret_layout);
                        let v = self.operand(state, operand, Some(ty))?;
                        sys::LLVMBuildRet(b, v);
                    }
                    None => {
                        sys::LLVMBuildRetVoid(b);
                    }
                }
            },
            Terminator::Goto(target) => unsafe {
                sys::LLVMBuildBr(b, state.block(*target)?);
            },
            Terminator::Branch { cond, then_block, else_block } => {
                let c = self.operand(state, cond, Some(self.int_ty(1)))?;
                // **`i1`, and it is checked rather than assumed.** §3.1 makes
                // `Bool` *"`i1` in registers and `i8` in memory"*, and
                // [`LlvmBackend::scalar_ty`] answers with the memory form — so a
                // condition that came from `Inst::Load` of a `Bool` local is an
                // `i8`, and `br i8` is a verifier failure. The repair is a
                // `trunc`, and `LLVMBuildTrunc` is deliberately **not** declared:
                // `sys.rs`'s rule is *"declare only what you call"*, nothing in
                // stage 1 can produce a loaded `Bool` condition, and a
                // declaration nobody calls is a claim nobody checks. So this
                // refuses, by name, and the day a `Bool` local reaches a branch
                // the refusal says which line to write.
                if !self.value_is_int_of_width(c, 1) {
                    return Err(BackendError::Unsupported {
                        what: "a branch on a condition that is not `i1` — §3.1's memory form of                                `Bool` is `i8` and narrowing it needs a `trunc`, which                                `crate::sys` does not declare"
                            .to_string(),
                    });
                }
                unsafe {
                    sys::LLVMBuildCondBr(
                        b,
                        c,
                        state.block(*then_block)?,
                        state.block(*else_block)?,
                    );
                }
            }
            Terminator::Switch { value, arms, default } => unsafe {
                let discr = self.operand(state, value, None)?;
                let tag_ty = sys::LLVMTypeOf(discr);
                // The case constants below are built against this type. It has
                // to be an integer: `LLVMConstInt` of a `ptr` or a struct is an
                // assertion in a debug LLVM and silence in a release one, and
                // the releases are what ship.
                if sys::LLVMGetTypeKind(tag_ty) != sys::type_kind::INTEGER {
                    return Err(BackendError::Unsupported {
                        what: "a `switch` on a value that is not an integer; Decision 18 makes                                every discriminant a `u8`, `u16` or `u32`, so this is a                                discriminant that was read from the wrong place"
                            .to_string(),
                    });
                }
                let switch = sys::LLVMBuildSwitch(
                    b,
                    discr,
                    state.block(*default)?,
                    arms.len() as c_uint,
                );
                for (discriminant, target) in arms {
                    // The case value must have the switched value's exact
                    // type. Decision 18 makes a discriminant `u8` up to 256
                    // variants and `u16` beyond, so a hard-coded `i64` here
                    // would be a verifier failure on every `match` in the
                    // language.
                    let on = sys::LLVMConstInt(tag_ty, *discriminant, 0);
                    sys::LLVMAddCase(switch, on, state.block(*target)?);
                }
            },
            Terminator::Unreachable => unsafe {
                sys::LLVMBuildUnreachable(b);
            },
        }
        Ok(())
    }

    /// Define a function from the extended instruction set.
    ///
    /// The same emitter [`Backend::define_function`] runs; see the module
    /// documentation §2.
    pub fn define_function_ext(
        &mut self,
        func: FuncId,
        sig: &AbiSignature,
        body: &ExtBody,
    ) -> Result<(), BackendError> {
        self.emit_body(func, sig, body)
    }

    /// Write an object file. The pipeline's entry point; [`Backend::emit`] uses
    /// it through a temporary file.
    pub fn emit_object_to(&mut self, path: &std::path::Path) -> Result<(), BackendError> {
        self.emit_file_to(path, sys::file_type::OBJECT)
    }

    /// Write assembly. `tests/float_policy.rs`'s only way to answer Decision
    /// 36's question, because the question is about instructions.
    pub fn emit_assembly_to(&mut self, path: &std::path::Path) -> Result<(), BackendError> {
        self.emit_file_to(path, sys::file_type::ASSEMBLY)
    }

    fn emit_file_to(
        &mut self,
        path: &std::path::Path,
        file_type: c_uint,
    ) -> Result<(), BackendError> {
        self.verify()?;
        let opt = self.config.as_ref().map(|c| c.opt()).unwrap_or_default();
        let module = self
            .module
            .as_ref()
            .ok_or_else(|| BackendError::Other("no module".to_string()))?;
        let machine = self
            .machine
            .as_ref()
            .ok_or_else(|| BackendError::Other("no target machine".to_string()))?;
        machine::optimise(module, machine, opt).map_err(BackendError::Other)?;
        // Decision 35's grep, at the only moment the whole module exists and
        // before anything irreversible happens to it. §7.3 obligation 1 is *"an
        // obligation to never opt in — a line that is correct by not being
        // written, which means nothing tests it"*; this is the compiler testing
        // it on its own output rather than a test testing it on a corpus.
        let ir = module.print();
        if let Some((line, flag)) = science_codegen::target::first_fast_math_flag(&ir) {
            return Err(BackendError::Other(format!(
                "a fast-math flag `{flag}` reached the emitted IR at line {line}; \
                 `reproducibility.md` Decision 3 forbids every value-changing float transform \
                 and nothing in this compiler is allowed to set one"
            )));
        }
        machine::emit_to_file(module, machine, path, file_type).map_err(BackendError::Other)
    }
}

impl Default for LlvmBackend {
    fn default() -> LlvmBackend {
        LlvmBackend::new()
    }
}

/// Per-body bookkeeping: what a `ValueId`, a `LocalId` and a `BlockId` mean.
#[derive(Default)]
struct BodyState {
    values: BTreeMap<u32, sys::LLVMValueRef>,
    locals: BTreeMap<u32, (sys::LLVMValueRef, sys::LLVMTypeRef, Layout)>,
    blocks: BTreeMap<u32, sys::LLVMBasicBlockRef>,
    /// The function's parameters, by source index.
    ///
    /// **Populated and unreadable, and that is the interface's second gap.**
    /// `science_codegen::backend::Operand` has no `Param` form, so a body has no
    /// way to name its own arguments — the same shape of hole as
    /// [`ExtInst::LocalAddr`], one level up. It does not block stage 1, whose
    /// only function is `main()`, and it blocks every stage after it. Filled in
    /// here so that the fix above the line is "add an operand" and not "add an
    /// operand and then find where the values come from".
    #[allow(dead_code)]
    params: BTreeMap<usize, sys::LLVMValueRef>,
    /// The hidden return slot of an `sret` function.
    ///
    /// Reachable only through [`ExtInst::ReturnSlot`], which is the third thing
    /// the interface above the line cannot spell: `Operand` has no `Param` form,
    /// so a body cannot name its own hidden pointer and MIR's `_0` cannot be
    /// bound to it without a local extension.
    sret: Option<sys::LLVMValueRef>,
}

impl BodyState {
    fn local(&self, local: LocalId) -> Result<sys::LLVMValueRef, BackendError> {
        self.local_entry(local).map(|(slot, _, _)| slot)
    }

    fn local_entry(
        &self,
        local: LocalId,
    ) -> Result<(sys::LLVMValueRef, sys::LLVMTypeRef, Layout), BackendError> {
        self.locals.get(&local.0).cloned().ok_or_else(|| {
            BackendError::Other(format!(
                "local _{} is used before its `Alloca`; Decision 8 puts every one of them in the \
                 entry block",
                local.0
            ))
        })
    }

    fn block(&self, id: BlockId) -> Result<sys::LLVMBasicBlockRef, BackendError> {
        self.blocks.get(&id.0).copied().ok_or_else(|| {
            BackendError::Other(format!("the body branches to bb{}, which it does not have", id.0))
        })
    }
}

impl Backend for LlvmBackend {
    fn begin_module(&mut self, name: &str, target: &TargetConfig) -> Result<(), BackendError> {
        let attrs = Attrs::new(&self.context).map_err(|missing| {
            BackendError::Other(format!(
                "this LLVM does not know the attribute kinds `{}`; LLVM 18 has all of them and a \
                 later one renamed at least `nocapture`, so this is a version mismatch rather \
                 than a typo",
                missing.join("`, `")
            ))
        })?;
        let machine = machine::for_config(target).map_err(BackendError::Other)?;
        let module = Module::new(&self.context, name);
        machine::describe(&module, &machine, target.triple());
        self.module = Some(module);
        self.machine = Some(machine);
        self.attrs = Some(attrs);
        self.config = Some(target.clone());
        self.declared.clear();
        self.order.clear();
        self.verified = false;
        Ok(())
    }

    fn define_string_bytes(&mut self, literal: &StringLiteral) -> Result<(), BackendError> {
        let module = self.module_ref()?;
        let name = cstr(&literal.bytes_symbol);
        let bytes = literal.bytes.clone();
        let constant = unsafe {
            sys::LLVMConstStringInContext(
                self.context.raw(),
                bytes.as_ptr() as *const std::ffi::c_char,
                bytes.len() as c_uint,
                // Not NUL-terminated: the length travels beside the pointer.
                1,
            )
        };
        let array_ty = self.byte_array(bytes.len() as u64);
        let global = unsafe { sys::LLVMAddGlobal(module.raw(), array_ty, name.as_ptr()) };
        unsafe {
            sys::LLVMSetInitializer(global, constant);
            sys::LLVMSetGlobalConstant(global, 1);
            sys::LLVMSetLinkage(global, sys::linkage::PRIVATE);
            sys::LLVMSetUnnamedAddress(global, sys::unnamed_addr::GLOBAL);
            sys::LLVMSetAlignment(global, 1);
        }
        self.verified = false;
        Ok(())
    }

    fn define_type_info(&mut self, symbol: &str, info: &TypeInfo) -> Result<(), BackendError> {
        let module = self.module_ref()?;
        let usize_ty = self.int_ty((self.triple().pointer_width() * 8) as u32);
        let drop_fn = match &info.drop_fn {
            None => unsafe { sys::LLVMConstPointerNull(self.ptr_ty()) },
            Some(glue) => {
                let name = cstr(glue);
                let value = unsafe { sys::LLVMGetNamedFunction(module.raw(), name.as_ptr()) };
                if value.is_null() {
                    return Err(BackendError::Other(format!(
                        "the descriptor `{symbol}` names drop glue `{glue}` that the module does \
                         not define"
                    )));
                }
                value
            }
        };
        let mut members = [
            unsafe { sys::LLVMConstInt(usize_ty, info.size, 0) },
            unsafe { sys::LLVMConstInt(usize_ty, info.align, 0) },
            drop_fn,
        ];
        let constant = unsafe {
            sys::LLVMConstStructInContext(self.context.raw(), members.as_mut_ptr(), 3, 0)
        };
        let name = cstr(symbol);
        let global_ty = {
            let mut tys = [usize_ty, usize_ty, self.ptr_ty()];
            self.anon_struct(&mut tys)
        };
        let global = unsafe { sys::LLVMAddGlobal(module.raw(), global_ty, name.as_ptr()) };
        unsafe {
            sys::LLVMSetInitializer(global, constant);
            sys::LLVMSetGlobalConstant(global, 1);
            sys::LLVMSetLinkage(global, sys::linkage::PRIVATE);
            sys::LLVMSetUnnamedAddress(global, sys::unnamed_addr::GLOBAL);
        }
        self.verified = false;
        Ok(())
    }

    fn define_map_info(&mut self, _symbol: &str, _info: &MapInfo) -> Result<(), BackendError> {
        Err(BackendError::Unsupported {
            what: "a `ScienceMapInfo`: stage 4's containers are not lowered, and emitting a \
                   descriptor whose `hash_fn` and `eq_fn` no function in the module defines \
                   would produce a module that verifies and crashes"
                .to_string(),
        })
    }

    fn declare_function(&mut self, sig: &AbiSignature) -> Result<FuncId, BackendError> {
        if let Some(index) = self.order.iter().position(|s| *s == sig.symbol) {
            // Declaring the same symbol twice is not an error — a forward
            // declaration followed by a definition is the ordinary case — but
            // it must not create a second LLVM function, which is what
            // `LLVMAddFunction` would do, silently renaming it to `foo.1`.
            let declared = &self.declared[&sig.symbol];
            if declared.sig != *sig {
                return Err(BackendError::Other(format!(
                    "`{}` is declared twice with two different signatures",
                    sig.symbol
                )));
            }
            return Ok(FuncId(index as u32));
        }
        let (fn_type, sret) = self.fn_type(sig);
        let module = self.module_ref()?;
        let name = cstr(&sig.symbol);
        let function = unsafe { sys::LLVMAddFunction(module.raw(), name.as_ptr(), fn_type) };

        let attrs = self.attrs_ref()?;
        if sig.nounwind {
            // Decision 6, on every function without exception: no `invoke`, no
            // `landingpad`, no personality routine.
            unsafe {
                sys::LLVMAddAttributeAtIndex(
                    function,
                    sys::LLVM_ATTRIBUTE_FUNCTION_INDEX,
                    attrs.nounwind(),
                )
            };
        }
        if sret {
            let slot_ty = self.llvm_type(&sig.ret_layout);
            unsafe {
                sys::LLVMAddAttributeAtIndex(
                    function,
                    sys::LLVM_ATTRIBUTE_FIRST_PARAM_INDEX,
                    attrs.sret(slot_ty),
                );
                sys::LLVMAddAttributeAtIndex(
                    function,
                    sys::LLVM_ATTRIBUTE_FIRST_PARAM_INDEX,
                    attrs.noalias(),
                );
                sys::LLVMAddAttributeAtIndex(
                    function,
                    sys::LLVM_ATTRIBUTE_FIRST_PARAM_INDEX,
                    attrs.align(sig.ret_layout.align),
                );
            }
        }
        let mut index = sys::LLVM_ATTRIBUTE_FIRST_PARAM_INDEX + u32::from(sret);
        for param in &sig.params {
            if matches!(param.class, ArgClass::Ignore) {
                continue;
            }
            self.apply_param_attrs(function, index, param)?;
            index += 1;
        }
        let _ = sys::LLVM_ATTRIBUTE_RETURN_INDEX;

        let id = FuncId(self.order.len() as u32);
        self.order.push(sig.symbol.clone());
        self.declared
            .insert(sig.symbol.clone(), Declared { value: function, fn_type, sig: sig.clone() });
        self.verified = false;
        Ok(id)
    }

    fn define_function(
        &mut self,
        func: FuncId,
        sig: &AbiSignature,
        body: &Body,
    ) -> Result<(), BackendError> {
        self.emit_body(func, sig, &ExtBody::lift(body))
    }

    fn verify(&mut self) -> Result<(), BackendError> {
        let module = self.module_ref()?;
        match module.verify() {
            Ok(()) => {
                self.verified = true;
                Ok(())
            }
            Err(detail) => Err(BackendError::VerifierFailed {
                function: "<module>".to_string(),
                detail: format!("{detail}\n--- the module ---\n{}", module.print()),
            }),
        }
    }

    fn emit(&mut self, kind: EmitKind) -> Result<Vec<u8>, BackendError> {
        match kind {
            EmitKind::Ir => {
                self.verify()?;
                Ok(self.ir().into_bytes())
            }
            EmitKind::Object => {
                let path = std::env::temp_dir()
                    .join(format!("science-{}-{}.o", std::process::id(), self.order.len()));
                self.emit_object_to(&path)?;
                let bytes = std::fs::read(&path)
                    .map_err(|e| BackendError::Other(format!("could not read the object: {e}")))?;
                let _ = std::fs::remove_file(&path);
                Ok(bytes)
            }
            EmitKind::Executable => Err(BackendError::Unsupported {
                what: "an executable from `Backend::emit`: §5 makes the linker the driver's, not \
                       the backend's, so `crate::link` is what produces one and `crate::build` is \
                       what calls it"
                    .to_string(),
            }),
        }
    }
}
