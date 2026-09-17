//! MIR to the backend's instruction set, for §10's stages 0 and 1.
//!
//! **Where this belongs, and it is not here.** Decision 42's table puts
//! *"which operations are runtime calls"* above the line and leaves *"emitting
//! the call"* below it, which makes MIR-to-`Inst` an above-the-line job. There
//! is no such module in `science-codegen`: its own §2 says *"there is still no
//! `lower_hir` function"* and *"a `Ty -> CgTy` lowering does not exist"*, and
//! both are exactly what this file needs. So this file is written here, against
//! `science_codegen`'s vocabulary and using none of its own, and **it should
//! move above the line the day that crate grows a place for it.** Nothing in it
//! names LLVM; the test for that is that the module compiles with
//! `crate::sys` removed from scope, and `tests/independence.rs` in
//! `science-codegen` is the model for making that a fact rather than a claim.
//!
//! **What is lowered.** §10's stage 1 and nothing beyond it:
//!
//! > *`print("hello, world")` … the script body of `script-mode.md` §2.1
//! > lowered to `def main() -> Error?`; a string literal as a
//! > `private unnamed_addr constant` plus a `science_string_from_bytes` call
//! > (Decision 15); a `science_println` call; drop glue for one `String`; and
//! > the `sret` convention, immediately.*
//!
//! Everything else is refused as `SC0400`, which §11 defines as *"a toolchain
//! feature required to build this program is not compiled into this
//! `sciencec`"* — and an unlowered MIR construct is literally that. The refusal
//! names the construct, so that a user who writes a loop is told "loops" and not
//! "internal error".
//!
//! **The cost of refusing rather than half-lowering** is that the set of
//! programs this compiler builds is tiny and the boundary is sharp. §10's own
//! discipline is the argument: *"every stage below produces a program that runs
//! and prints something, and no stage is finished until an execution test
//! asserts its output and exit code."* A backend that lowered a loop it had not
//! tested would produce a program that runs and prints the wrong thing.
//!
//! # The entry point, and §9.3's finding 5 met from the other side
//!
//! `science-rt` has no program entry point, no `science_exit`, and no symbol
//! that writes to stderr without aborting. [`EXIT_CONTRACT`] in
//! `science_codegen::runtime` records that, and
//! `science-codegen`'s `tests/stage_one.rs` asserts that it is unsatisfiable.
//!
//! So this module emits `main` itself, as §9.3 says it must, and the emitted
//! `main` is honest about the half it cannot do:
//!
//! - the Science entry is `_S4main`, mangled by Decision 16, returning `Error?`
//!   through `sret` because `Error?` is `(any Error)?`, two words, and Windows
//!   x64 returns anything that is not 1, 2, 4 or 8 bytes by hidden pointer;
//! - C's `main` calls it, tests **the data pointer and only the data pointer**
//!   — §3.4: *"the vtable slot of a null trait object is undefined and codegen
//!   must never load it — including on the path that tests for null"* — and
//!   returns 0 when it is null;
//! - and on the non-null edge it calls `science_panic_bytes` with a message
//!   saying why it cannot do what `script-mode.md` §2.3 requires, then
//!   `unreachable`.
//!
//! **That last bullet is wrong and is deliberately wrong.** §2.3 requires
//! `error: ` plus the error's `Display` on stderr and exit **1**;
//! `science_panic_bytes` writes to stderr and then calls `abort()`, which is
//! `SIGABRT` on POSIX and 3 on Windows. It is the closest reachable behaviour
//! and it is reachable only by the program the compiler cannot yet produce —
//! nothing in stage 1 can return a non-null error, because nothing in stage 1
//! can construct one. When `science_exit` and a non-aborting stderr writer
//! exist, this is four lines.

use science_codegen::abi::{AbiParam, AbiSignature, ArgClass, ParamAttrs, ReturnClass};
use science_codegen::backend::{
    BlockId, Callee, Inst, LocalId, Operand, Terminator, ValueId,
};
use science_codegen::descriptor::StringLiteral;
use science_codegen::diagnostics::backend_not_compiled_in;
use science_codegen::layout::{CgTy, IntTy, Layout, PtrKind, Repr, Triple, layout_of};
use science_codegen::mangle::{MonoKey, mangle};
use science_codegen::runtime::{RUNTIME, RtAggregate, RtParam, RtRet, RuntimeFn, runtime_fn};
use science_diagnostics::Diagnostic;
use science_mir::mir::{self, Body as MirBody, Constant, Rvalue, StatementKind, TerminatorKind};
use science_resolve::hir::{DefTable, Literal};
use science_types::Types;
use science_types::ty::TyKind;

use crate::emit::{ExtBlock, ExtBody, ExtInst};

/// Why a program could not be lowered.
///
/// One variant, carrying the construct's name, because every refusal is the
/// same refusal: this compiler does not implement that yet. §11's `SC0400` is
/// the code and its contract is *"names the feature and how to obtain a build
/// that has it"* — here the second half is "a later stage of the backend", which
/// is not a package anyone can install, and the note says so rather than
/// inventing an install command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unlowered {
    /// The construct, named the way a user would recognise it.
    pub construct: String,
}

impl Unlowered {
    fn new(construct: impl Into<String>) -> Unlowered {
        Unlowered { construct: construct.into() }
    }

    /// The `SC0400` this becomes.
    pub fn to_diagnostic(&self) -> Diagnostic {
        backend_not_compiled_in(
            &self.construct,
            "this `sciencec` implements §10's stages 0 and 1 of `codegen-and-linking.md` — a \
             script body, a string literal and `print` — and refuses everything else rather than \
             lowering a construct no execution test has ever run",
        )
    }
}

/// The signature of a runtime entry point, derived from
/// [`science_codegen::runtime::RUNTIME`].
///
/// **Derived and never written down**, which is `runtime.rs`'s whole argument:
/// §9.2's finding was a hand-maintained list of the entry points that need
/// `sret`, and `science_array_with_capacity` had fallen off it. A declaration
/// built from the signature cannot fall off anything.
///
/// **No pointer attributes are emitted, and that is a finding rather than
/// laziness.** Decision 24 wants `readonly nocapture` on a shared borrow and
/// `noalias nocapture` on an exclusive one, and
/// [`science_codegen::runtime::RtParam`] has **one** pointer variant: it does
/// not distinguish `*const ScienceString` from `*mut ScienceString`. Every one
/// of the 45 signatures makes the distinction in Rust —
/// `science_print(text: *const ScienceString)` against
/// `science_string_free(value: *mut ScienceString)` — and the table that codegen
/// reads erases it. Emitting `readonly` on a guess is §4.4's failure mode with
/// the compiler on the wrong end of it, so nothing is emitted and the
/// optimisation Decision 24 buys is left on the table until `RtParam` splits.
pub fn runtime_signature(target: Triple, entry: &RuntimeFn) -> AbiSignature {
    let ret_ty = match entry.ret {
        RtRet::Void | RtRet::Never => CgTy::Unit,
        RtRet::Bool => CgTy::Bool,
        RtRet::I32 => CgTy::Int(IntTy::I32),
        RtRet::Int => CgTy::Int(IntTy::I64),
        RtRet::U64 => CgTy::Int(IntTy::U64),
        RtRet::Ptr => CgTy::Ptr(PtrKind::Raw),
        RtRet::Aggregate(aggregate) => aggregate.cg_ty(),
    };
    let ret_layout = layout_of(target, &ret_ty);
    let params = entry
        .params
        .iter()
        .enumerate()
        .map(|(index, param)| {
            let ty = match param {
                RtParam::Descriptor | RtParam::Pointer => CgTy::Ptr(PtrKind::Raw),
                RtParam::Usize => CgTy::Int(IntTy::Usize),
                RtParam::Int => CgTy::Int(IntTy::I64),
            };
            AbiParam {
                name: format!("a{index}"),
                class: ArgClass::Direct,
                layout: layout_of(target, &ty),
                attrs: ParamAttrs::default(),
            }
        })
        .collect();
    AbiSignature {
        symbol: entry.symbol.to_string(),
        ret: science_codegen::abi::classify_return_for(target, &ret_layout),
        ret_layout,
        params,
        foreign: true,
        nounwind: true,
    }
}

/// Every runtime entry point, by symbol, for a target.
pub fn runtime_signatures(target: Triple) -> Vec<AbiSignature> {
    RUNTIME.iter().map(|entry| runtime_signature(target, entry)).collect()
}

/// A lowered module: what the backend is asked to emit, in order.
pub struct Lowered {
    /// The string literals, in the order they were interned.
    pub literals: Vec<StringLiteral>,
    /// Declarations for every runtime entry point the module calls.
    pub declarations: Vec<AbiSignature>,
    /// Definitions, in Decision 4's sorted-by-symbol order.
    pub definitions: Vec<(AbiSignature, ExtBody)>,
}

/// The whole of what this backend can lower.
pub struct Lowerer<'a> {
    target: Triple,
    defs: &'a DefTable,
    types: &'a Types,
    literals: Vec<StringLiteral>,
    declarations: Vec<AbiSignature>,
}

impl<'a> Lowerer<'a> {
    /// A lowerer for one crate.
    pub fn new(target: Triple, defs: &'a DefTable, types: &'a Types) -> Lowerer<'a> {
        Lowerer { target, defs, types, literals: Vec::new(), declarations: Vec::new() }
    }

    fn declare(&mut self, symbol: &str) -> Result<AbiSignature, Unlowered> {
        if let Some(existing) = self.declarations.iter().find(|s| s.symbol == symbol) {
            return Ok(existing.clone());
        }
        let entry = runtime_fn(symbol)
            .ok_or_else(|| Unlowered::new(format!("a call to `{symbol}`, which is not one of \
                                                   `science-rt`'s 45 entry points")))?;
        let sig = runtime_signature(self.target, entry);
        self.declarations.push(sig.clone());
        Ok(sig)
    }

    fn intern_literal(&mut self, text: &str) -> StringLiteral {
        if let Some(existing) = self.literals.iter().find(|l| l.bytes == text.as_bytes()) {
            return existing.clone();
        }
        let literal = StringLiteral::new(self.literals.len(), text);
        self.literals.push(literal.clone());
        literal
    }

    /// A `science_types::Ty` as a [`CgTy`].
    ///
    /// **This is the `Ty -> CgTy` lowering `science-codegen`'s §2 says does not
    /// exist**, cut down to the types stage 1 can produce. It belongs above the
    /// line beside `layout`, and it is here because there is nowhere above the
    /// line to put it.
    ///
    /// The `Named` case reads the *name*, which is wrong in general — two
    /// modules can each define a `String` — and is right for the prelude types
    /// it is restricted to, all of which are builtins. A real lowering keys on
    /// the `DefId`. Recorded rather than smoothed because the fix is to move the
    /// function, not to patch it.
    fn cg_ty(&self, ty: science_types::ty::Ty) -> Result<CgTy, Unlowered> {
        match self.types.kind(ty) {
            TyKind::Unit => Ok(CgTy::Unit),
            TyKind::Nullable(inner) => Ok(CgTy::nullable(self.cg_ty(*inner)?)),
            // `any I` — Decision 13's two-word fat pointer, with the null niche
            // in the data pointer (§3.4).
            TyKind::Object { .. } => Ok(CgTy::Interface),
            TyKind::Borrowed { mutable, .. } => Ok(CgTy::Ptr(if *mutable {
                PtrKind::MutBorrow
            } else {
                PtrKind::Borrow
            })),
            TyKind::Named { def, args } if args.is_empty() => {
                let name = self.defs.get(*def).name.as_str();
                match name {
                    "Int" => Ok(CgTy::Int(IntTy::I64)),
                    "I8" => Ok(CgTy::Int(IntTy::I8)),
                    "I16" => Ok(CgTy::Int(IntTy::I16)),
                    "I32" => Ok(CgTy::Int(IntTy::I32)),
                    "I64" => Ok(CgTy::Int(IntTy::I64)),
                    "U8" => Ok(CgTy::Int(IntTy::U8)),
                    "U16" => Ok(CgTy::Int(IntTy::U16)),
                    "U32" => Ok(CgTy::Int(IntTy::U32)),
                    "U64" => Ok(CgTy::Int(IntTy::U64)),
                    "F32" => Ok(CgTy::Float(science_codegen::layout::FloatTy::F32)),
                    "F64" => Ok(CgTy::Float(science_codegen::layout::FloatTy::F64)),
                    "Bool" => Ok(CgTy::Bool),
                    "Char" => Ok(CgTy::Char),
                    "String" => Ok(RtAggregate::String.cg_ty()),
                    other => Err(Unlowered::new(format!("a value of type `{other}`"))),
                }
            }
            other => Err(Unlowered::new(format!("a value of type `{other:?}`"))),
        }
    }

    fn layout_of_ty(&self, ty: science_types::ty::Ty) -> Result<Layout, Unlowered> {
        Ok(layout_of(self.target, &self.cg_ty(ty)?))
    }

    /// Lower one crate's MIR.
    ///
    /// `bodies` is every MIR body in the crate. Stage 1 has exactly one and it
    /// is `main`; anything else is refused, because a second function means a
    /// call to it and a call to it means the parameter operand
    /// `science_codegen::backend::Operand` does not have (see
    /// `crate::emit`'s `BodyState::params`).
    pub fn lower_crate(&mut self, bodies: &[MirBody]) -> Result<Lowered, Unlowered> {
        let entry = bodies
            .iter()
            .find(|body| self.defs.get(body.def()).name == "main")
            .ok_or_else(|| Unlowered::new("a program with no `main`"))?;
        if bodies.len() > 1 {
            let other = bodies
                .iter()
                .find(|body| self.defs.get(body.def()).name != "main")
                .map(|body| self.defs.get(body.def()).name.clone())
                .unwrap_or_default();
            return Err(Unlowered::new(format!("a second function, `{other}`")));
        }

        let science_main = self.lower_body(entry)?;
        let c_main = self.lower_c_main(&science_main.0)?;
        let mut definitions = vec![science_main, c_main];
        // Decision 4: *"every monomorphised item is emitted in sorted order of
        // its mangled symbol name."* Three lines, and it closes Gate J's named
        // hazard by construction rather than by testing.
        definitions.sort_by(|a, b| a.0.symbol.cmp(&b.0.symbol));
        Ok(Lowered {
            literals: std::mem::take(&mut self.literals),
            declarations: std::mem::take(&mut self.declarations),
            definitions,
        })
    }

    fn lower_body(&mut self, body: &MirBody) -> Result<(AbiSignature, ExtBody), Unlowered> {
        if body.params().next().is_some() {
            return Err(Unlowered::new("a function with parameters"));
        }
        let return_local = mir::Local::from_index(0);
        let ret_layout = self.layout_of_ty(body.local_decl(return_local).ty)?;
        let key = MonoKey::plain(&["main"]);
        let sig = AbiSignature::science(self.target, mangle(&key), ret_layout.clone(), vec![]);

        // Decision 8: every local is an `alloca` in the entry block. MIR's
        // `StorageLive`/`StorageDead` are not modelled — an `alloca` lives for
        // the frame — which is correct and costs stack: two locals with
        // disjoint live ranges get two slots where LLVM's stack colouring would
        // have shared one. `-O2`'s `StackColoring` pass recovers it only when
        // it is given `llvm.lifetime` intrinsics, which is what those two
        // statements should become and do not yet.
        let mut entry_insts: Vec<ExtInst> = Vec::new();
        let mut locals: Vec<(LocalId, Layout)> = Vec::new();
        for (local, decl) in body.locals() {
            let layout = self.layout_of_ty(decl.ty)?;
            let id = LocalId(local.index() as u32);
            entry_insts.push(ExtInst::Above(Inst::Alloca { local: id, layout: layout.clone() }));
            locals.push((id, layout));
        }

        let mut values = ValueCounter::default();
        let mut blocks: Vec<ExtBlock> = Vec::new();
        for (id, block) in body.blocks() {
            let mut insts = if id.index() == 0 { std::mem::take(&mut entry_insts) } else { Vec::new() };
            for statement in &block.statements {
                self.lower_statement(&statement.kind, &locals, &mut values, &mut insts)?;
            }
            let terminator =
                self.lower_terminator(&block.terminator.kind, &locals, &mut values, &mut insts)?;
            blocks.push(ExtBlock {
                id: BlockId(id.index() as u32),
                label: format!("bb{}", id.index()),
                insts,
                terminator,
            });
        }
        Ok((sig, ExtBody { blocks }))
    }

    fn lower_statement(
        &mut self,
        kind: &StatementKind,
        locals: &[(LocalId, Layout)],
        _values: &mut ValueCounter,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        match kind {
            // Not modelled; see `lower_body`.
            StatementKind::StorageLive(_) | StatementKind::StorageDead(_) => Ok(()),
            // Decision 26's drop flags and two-phase activations are statements
            // a code generator may ignore only because stage 1 has no drop that
            // is conditional and no two-phase borrow. Refused rather than
            // ignored, because ignoring a `SetDropFlag` is a double free.
            StatementKind::SetDropFlag { .. } => {
                Err(Unlowered::new("a conditionally moved value, which needs a drop flag"))
            }
            StatementKind::Activate(_) => Err(Unlowered::new("a two-phase borrow")),
            StatementKind::Nop => Ok(()),
            StatementKind::Assign { place, rvalue } => {
                if !place.projection.is_empty() {
                    return Err(Unlowered::new("an assignment through a field or an index"));
                }
                let local = LocalId(place.local.index() as u32);
                let layout = locals
                    .iter()
                    .find(|(id, _)| *id == local)
                    .map(|(_, layout)| layout.clone())
                    .ok_or_else(|| Unlowered::new("an assignment to an unknown local"))?;
                match rvalue {
                    Rvalue::Use(mir::Operand::Const(Constant::Literal(Literal::Null))) => {
                        self.store_null(local, &layout, insts)
                    }
                    Rvalue::Use(mir::Operand::Const(Constant::Unit)) => Ok(()),
                    other => Err(Unlowered::new(format!("{}", describe_rvalue(other)))),
                }
            }
        }
    }

    /// Store `null` into a `T?`.
    ///
    /// Two representations and both put the answer at offset 0, which is why
    /// this needs no `getelementptr`. Decision 19's niche form is a null
    /// pointer; Decision 18's tagged form is `SCIENCE_NULLABLE_NULL`, which
    /// `science-rt`'s `abi.rs` fixes at **zero** — *"the absent case is then the
    /// all-zero byte pattern in both representations"*. The niche's own offset
    /// is checked rather than assumed, because §3.4 generalises the rule and a
    /// future niche that is not at 0 would otherwise be written to the wrong
    /// place.
    fn store_null(
        &self,
        local: LocalId,
        layout: &Layout,
        insts: &mut Vec<ExtInst>,
    ) -> Result<(), Unlowered> {
        match &layout.repr {
            Repr::Niched { niche, .. } if niche.offset == 0 => {
                insts.push(ExtInst::Above(Inst::Store { local, value: Operand::Null }));
                Ok(())
            }
            Repr::Tagged { .. } => Err(Unlowered::new(
                "a `null` of a tagged nullable, which needs a store to the tag byte and the \
                 instruction set has no field projection",
            )),
            Repr::Niched { .. } => Err(Unlowered::new("a `null` whose niche is not at offset 0")),
            _ => Err(Unlowered::new("a `null` of a type that is not nullable")),
        }
    }

    fn lower_terminator(
        &mut self,
        kind: &TerminatorKind,
        locals: &[(LocalId, Layout)],
        values: &mut ValueCounter,
        insts: &mut Vec<ExtInst>,
    ) -> Result<Terminator, Unlowered> {
        match kind {
            TerminatorKind::Return => Ok(Terminator::Return(None)),
            TerminatorKind::Goto { target } => Ok(Terminator::Goto(BlockId(target.index() as u32))),
            TerminatorKind::Unreachable => Ok(Terminator::Unreachable),
            TerminatorKind::Call { callee, args, destination, target } => {
                self.lower_call(callee, args, destination, *target, locals, values, insts)
            }
            TerminatorKind::If { .. } => Err(Unlowered::new("an `if`")),
            TerminatorKind::Switch { .. } => Err(Unlowered::new("a `match`")),
            TerminatorKind::Drop { .. } => Err(Unlowered::new("a drop of a named value")),
        }
    }

    /// Lower a call.
    ///
    /// Stage 1 has one: `print` of a string literal. Decision 15 makes the
    /// literal itself a call — *"a string literal lowers to
    /// `call science_string_from_bytes(@.str.N, len)` … every evaluation of the
    /// literal allocates"* — so the one MIR terminator becomes three LLVM calls
    /// and the middle one is the user's.
    ///
    /// The third is the drop, and it is not optional. The temporary `String` is
    /// owned by the call site and nothing else frees it, so omitting
    /// `science_string_free` leaks twelve bytes per evaluation. §2.6's cost note
    /// for Decision 15 is the other half of the same fact: *"a literal in a loop
    /// allocates on every iteration"*, and it also frees on every iteration.
    #[allow(clippy::too_many_arguments)]
    fn lower_call(
        &mut self,
        callee: &mir::Callee,
        args: &[mir::Operand],
        destination: &mir::Place,
        target: Option<mir::BlockId>,
        locals: &[(LocalId, Layout)],
        values: &mut ValueCounter,
        insts: &mut Vec<ExtInst>,
    ) -> Result<Terminator, Unlowered> {
        let def = match callee {
            mir::Callee::Def(def) => *def,
            mir::Callee::Indirect(_) => return Err(Unlowered::new("a call through a closure")),
            mir::Callee::Runtime(symbol) => {
                return Err(Unlowered::new(format!("a whole-array call to `{symbol}`")));
            }
            mir::Callee::Unresolved(_) => {
                return Err(Unlowered::new("a method call the front end could not resolve"));
            }
        };
        let name = self.defs.get(def).name.clone();
        if !self.defs.get(def).is_builtin() || name != "print" {
            return Err(Unlowered::new(format!("a call to `{name}`")));
        }
        let [mir::Operand::Const(Constant::Literal(Literal::Str(text)))] = args else {
            return Err(Unlowered::new("a `print` of anything but a string literal"));
        };
        let _ = destination;

        // A slot for the temporary `String`. It is not a MIR local — MIR's
        // `print("…")` has the literal as a constant operand, with no local to
        // hold the `ScienceString` the runtime builds — so codegen invents one,
        // and it goes in the entry block with the rest (Decision 8). Stage 1 has
        // one basic block, so "the entry block" and "here" are the same place;
        // the day they are not, this has to move.
        let string_layout = layout_of(self.target, &RtAggregate::String.cg_ty());
        let slot = LocalId(locals.len() as u32 + values.temporaries);
        values.temporaries += 1;
        insts.push(ExtInst::Above(Inst::Alloca { local: slot, layout: string_layout.clone() }));

        let literal = self.intern_literal(text);
        let from_bytes = self.declare("science_string_from_bytes")?;
        insts.push(ExtInst::Above(Inst::Call {
            dest: None,
            callee: Callee::Runtime("science_string_from_bytes"),
            args: vec![
                Operand::GlobalAddr(literal.bytes_symbol.clone()),
                Operand::ConstInt(literal.len() as i128),
            ],
            ret: from_bytes.ret.clone(),
            sret_slot: Some(slot),
        }));

        // The address of the slot, which is the operand the interface above the
        // line cannot spell. `crate::emit`'s §2 is the account.
        let address = ValueId(values.next());
        insts.push(ExtInst::LocalAddr { dest: address, local: slot });

        let print = self.declare("science_print")?;
        insts.push(ExtInst::Above(Inst::Call {
            dest: None,
            callee: Callee::Runtime("science_print"),
            args: vec![Operand::Value(address)],
            ret: print.ret.clone(),
            sret_slot: None,
        }));

        let free = self.declare("science_string_free")?;
        insts.push(ExtInst::Above(Inst::Call {
            dest: None,
            callee: Callee::Runtime("science_string_free"),
            args: vec![Operand::Value(address)],
            ret: free.ret.clone(),
            sret_slot: None,
        }));

        match target {
            Some(block) => Ok(Terminator::Goto(BlockId(block.index() as u32))),
            // A call with no successor is a panic (§2.2), and `print` is not
            // one.
            None => Ok(Terminator::Unreachable),
        }
    }

    /// C's `main`, which `science-rt` does not provide. See the module
    /// documentation.
    fn lower_c_main(
        &mut self,
        science_main: &AbiSignature,
    ) -> Result<(AbiSignature, ExtBody), Unlowered> {
        let i32_layout = layout_of(self.target, &CgTy::Int(IntTy::I32));
        let sig = AbiSignature {
            symbol: "main".to_string(),
            ret: science_codegen::abi::classify_return_for(self.target, &i32_layout),
            ret_layout: i32_layout,
            params: vec![],
            foreign: false,
            nounwind: true,
        };
        if !science_main.ret.is_sret() {
            return Err(Unlowered::new(
                "an entry point whose `Error?` does not come back through `sret`; stage 1 only \
                 knows the two-word interface object",
            ));
        }
        let slot = LocalId(0);
        let data = ValueId(0);
        let is_null = ValueId(1);
        let message = self.intern_literal(
            "error: the script body returned an error, and this compiler cannot print it or exit \
             1 — `science-rt` has no stderr writer that does not abort and no `science_exit` \
             (codegen-and-linking.md §9.3, finding 5)",
        );
        let panic = self.declare("science_panic_bytes")?;

        let entry = ExtBlock {
            id: BlockId(0),
            label: "entry".to_string(),
            insts: vec![
                ExtInst::Above(Inst::Alloca {
                    local: slot,
                    layout: science_main.ret_layout.clone(),
                }),
                ExtInst::Above(Inst::Call {
                    dest: None,
                    callee: Callee::Science(science_main.symbol.clone()),
                    args: vec![],
                    ret: science_main.ret.clone(),
                    sret_slot: Some(slot),
                }),
                // §3.4: the data pointer and **only** the data pointer. The
                // vtable word of a null `(any Error)?` is undefined, and it is
                // at offset 8, so this load must not widen.
                ExtInst::Above(Inst::Load { dest: data, local: slot }),
                ExtInst::Above(Inst::Cmp {
                    dest: is_null,
                    op: science_codegen::backend::CmpOp::Eq,
                    signed: false,
                    lhs: Operand::Value(data),
                    rhs: Operand::Null,
                }),
            ],
            terminator: Terminator::Branch {
                cond: Operand::Value(is_null),
                then_block: BlockId(1),
                else_block: BlockId(2),
            },
        };
        let ok = ExtBlock {
            id: BlockId(1),
            label: "ok".to_string(),
            insts: vec![],
            terminator: Terminator::Return(Some(Operand::ConstInt(0))),
        };
        let failed = ExtBlock {
            id: BlockId(2),
            label: "failed".to_string(),
            insts: vec![ExtInst::Above(Inst::Call {
                dest: None,
                callee: Callee::Runtime("science_panic_bytes"),
                args: vec![
                    Operand::GlobalAddr(message.bytes_symbol.clone()),
                    Operand::ConstInt(message.len() as i128),
                ],
                ret: panic.ret.clone(),
                sret_slot: None,
            })],
            // Decision 6: `call` then `unreachable`, never `invoke`.
            terminator: Terminator::Unreachable,
        };
        Ok((sig, ExtBody { blocks: vec![entry, ok, failed] }))
    }
}

/// Fresh `ValueId`s and the count of invented locals.
#[derive(Default)]
struct ValueCounter {
    next: u32,
    temporaries: u32,
}

impl ValueCounter {
    fn next(&mut self) -> u32 {
        let id = self.next;
        self.next += 1;
        id
    }
}

fn describe_rvalue(rvalue: &Rvalue) -> &'static str {
    match rvalue {
        Rvalue::Use(_) => "an assignment",
        Rvalue::Ref { .. } => "a borrow",
        Rvalue::Unary { .. } => "a unary operator",
        Rvalue::Binary { .. } => "arithmetic",
        Rvalue::Cast { .. } => "a cast",
        Rvalue::Record { .. } => "a record literal",
        Rvalue::Variant { .. } => "a choice variant",
        Rvalue::Tuple(_) => "a tuple",
        Rvalue::Range { .. } => "a range",
        Rvalue::IsPresent(_) => "a `?` presence test",
        Rvalue::Discriminant(_) => "a discriminant read",
        Rvalue::Coerce { .. } => "a coercion",
        Rvalue::Narrow { .. } => "a narrowed read",
        Rvalue::Closure { .. } => "a closure",
        Rvalue::Error => "an expression the front end could not check",
    }
}
