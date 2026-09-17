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
//! always to let the backend peek at something above the line"*. There is one
//! place below where this crate reaches for something the line does not carry,
//! and rather than peek it is named: [`ExtInst::LocalAddr`]. See §2.
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
//! # 2. The one thing the interface cannot say
//!
//! `science_codegen::backend::Operand` has `Value`, `ConstInt`, `ConstFloat`,
//! `GlobalAddr` and `Null`. **There is no way to name the address of a local**,
//! and §10's stage 1 needs one on its second instruction: `science_print` takes
//! `*const ScienceString`, and the string it prints lives in the `alloca` that
//! the `sret` call before it filled in.
//!
//! `science-codegen`'s own `tests/stage_one.rs` runs into this and passes
//! `Operand::Value(ValueId(0))` — a value no instruction in that body produces.
//! Against a text backend that renders `%0` and nobody notices. Against LLVM it
//! is an undefined reference, which is how this was found.
//!
//! [`ExtInst`] is the minimum repair: the above-the-line [`Inst`] embedded
//! unchanged, plus exactly one new form. It is a local extension and it is meant
//! to be deleted — when `science_codegen::backend::Inst` grows an `AddrOf`,
//! [`ExtInst`] collapses to `Inst` and this section goes with it. Both entry
//! points run the same emitter, so the trait's `define_function` and the
//! extension's `define_function_ext` cannot drift.
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
    Backend, BackendError, Block, BlockId, Body, Callee, CmpOp, EmitKind, FloatOp, FuncId, Inst,
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
    /// See the module documentation §1 for why the padding is explicit. The
    /// `Tagged` case is the one that is *not* structural: a discriminant plus a
    /// union has no LLVM spelling, so it becomes an array of `align`-sized
    /// integers with the right size and alignment, and the tag is read back with
    /// a typed load at offset 0 when something needs it. Nothing in stage 1
    /// does; `Error?` is `Nullable(Interface)`, and an interface object has a
    /// null niche in its data pointer (Decision 19), so it takes the `Niched`
    /// path and never the tagged one.
    fn llvm_type(&self, layout: &Layout) -> sys::LLVMTypeRef {
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
            Operand::ConstInt(value) => {
                let ty = expected.unwrap_or_else(|| self.int_ty(64));
                unsafe { sys::LLVMConstInt(ty, *value as u64, 1) }
            }
            Operand::ConstFloat(value) => {
                let ty = expected.unwrap_or_else(|| unsafe {
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
                let l = self.operand(state, lhs, None)?;
                let r = self.operand(state, rhs, None)?;
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
                let l = self.operand(state, lhs, Some(double))?;
                let r = self.operand(state, rhs, Some(double))?;
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
                let l = self.operand(state, lhs, None)?;
                let r = self.operand(state, rhs, None)?;
                let name = cstr(&format!("v{}", dest.0));
                let is_float = self.value_is_float(l);
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

    /// Whether a value's type is a float, so that [`CmpOp`] picks `fcmp` over
    /// `icmp`.
    ///
    /// `science_codegen::backend::Inst::Cmp` carries `signed: bool` and says
    /// *"irrelevant for floats"*, which leaves the backend to work out which it
    /// has. Asking LLVM is exact; inferring it from whatever produced the value
    /// would be a second model of a fact the value already carries.
    fn value_is_float(&self, value: sys::LLVMValueRef) -> bool {
        let kind = unsafe { sys::LLVMGetTypeKind(sys::LLVMTypeOf(value)) };
        kind == sys::type_kind::FLOAT || kind == sys::type_kind::DOUBLE
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
            Terminator::Branch { cond, then_block, else_block } => unsafe {
                let c = self.operand(state, cond, Some(self.int_ty(1)))?;
                sys::LLVMBuildCondBr(b, c, state.block(*then_block)?, state.block(*else_block)?);
            },
            Terminator::Switch { value, arms, default } => unsafe {
                let discr = self.operand(state, value, None)?;
                let switch = sys::LLVMBuildSwitch(
                    b,
                    discr,
                    state.block(*default)?,
                    arms.len() as c_uint,
                );
                let tag_ty = sys::LLVMTypeOf(discr);
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
    /// The hidden return slot of an `sret` function, for the same reason.
    #[allow(dead_code)]
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
