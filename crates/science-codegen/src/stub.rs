//! The stub backend: LLVM's stand-in, and the reason it is worth having.
//!
//! **The decision.** [`TextBackend`] implements [`Backend`] by writing a
//! deterministic text transcript in LLVM IR's shape. It emits no machine code,
//! drives no linker, and produces nothing that runs.
//!
//! **The reason.** Decision 42's above-the-line half makes a long sequence of
//! decisions — this return is `sret`, that field is at offset 16, this
//! descriptor is emitted once, that multiply is followed by an add and not
//! fused — and without a backend none of them is *observable*. A test can call
//! [`crate::abi::classify_return`] and check the answer, which is worth doing
//! and is done; but it cannot check that the decision **survives the trip across
//! the line**, and the trip is where an interface is wrong. The transcript makes
//! that testable: `tests/float_policy.rs` greps it for fast-math flags, which is
//! Decision 35's test at the only level this machine can reach.
//!
//! **The cost, stated plainly.** The transcript is not LLVM IR. It is IR-shaped
//! text with the same tokens, and a test that passes against it proves that
//! *this crate* emitted the right thing, not that LLVM accepted it. A reader who
//! takes a green `tests/float_policy.rs` as evidence that `a * b + c` does not
//! become an `fma` on a real machine has been misled, and the test file says so
//! in its first paragraph. The LLVM-level assertion — create a target machine,
//! set `AllowFPOpFusion`, emit assembly, look for `vfmadd` — is written down in
//! that file as the test to add and cannot be run here.
//!
//! **Why it is not a second backend.** It is not an alternative code generator
//! and must never grow into one: Decision 42 is explicit that *"F0 builds no
//! second backend"*, and the insurance it buys is the line, not a second
//! implementation. This is a test double.

use std::fmt::Write as _;

use crate::abi::{AbiParam, AbiSignature, ArgClass, ReturnClass};
use crate::backend::{
    Backend, BackendError, Block, Body, Callee, CmpOp, EmitKind, FloatOp, FuncId, Inst, IntOp,
    Operand, Terminator,
};
use crate::descriptor::{MapInfo, StringLiteral, TypeInfo};
use crate::layout::{Layout, Repr, Scalar};
use crate::target::TargetConfig;

/// A backend that writes a transcript.
#[derive(Debug, Default)]
pub struct TextBackend {
    out: String,
    functions: Vec<String>,
    started: bool,
    verified: bool,
}

impl TextBackend {
    /// A new, empty backend.
    pub fn new() -> TextBackend {
        TextBackend::default()
    }

    /// The transcript so far.
    pub fn transcript(&self) -> &str {
        &self.out
    }

    fn line(&mut self, text: impl AsRef<str>) {
        self.out.push_str(text.as_ref());
        self.out.push('\n');
    }
}

/// How a layout is spelled in the transcript.
///
/// Structural, and that is deliberate: printing a nominal name would hide
/// whether the layout that crossed the line was the one the layout engine
/// computed. A transcript that says `{ptr, i64, i64}` is a transcript a reader
/// can check against `ScienceString`.
fn render_layout(layout: &Layout) -> String {
    match &layout.repr {
        Repr::Zero => "{}".to_string(),
        Repr::Scalar(scalar) => render_scalar(*scalar),
        Repr::Aggregate { fields } => {
            let inner: Vec<String> = fields.iter().map(|f| render_layout(&f.layout)).collect();
            format!("{{{}}}", inner.join(", "))
        }
        Repr::Tagged { tag, .. } => format!("{{tag:{}, ..{}b}}", render_scalar(Scalar::Int(*tag)), layout.size),
        Repr::Niched { payload, .. } => render_layout(payload),
    }
}

fn render_scalar(scalar: Scalar) -> String {
    match scalar {
        // §3.1: `i1` in registers, `i8` in memory. The transcript renders the
        // memory form, because everything it prints is a stored value.
        Scalar::Bool => "i8".to_string(),
        Scalar::Char => "i32".to_string(),
        Scalar::Int(int) => format!("i{}", int.width(crate::layout::Triple::X86_64LinuxGnu) * 8),
        Scalar::Float(crate::layout::FloatTy::F32) => "float".to_string(),
        Scalar::Float(crate::layout::FloatTy::F64) => "double".to_string(),
        Scalar::Pointer(_) => "ptr".to_string(),
    }
}

fn render_operand(operand: &Operand) -> String {
    match operand {
        Operand::Value(value) => format!("%{}", value.0),
        Operand::ConstInt(value) => value.to_string(),
        // Printed with enough digits to round-trip, because a transcript is
        // compared and a truncated float is a comparison that passes for the
        // wrong reason.
        Operand::ConstFloat(value) => format!("{value:?}"),
        Operand::GlobalAddr(symbol) => format!("@{symbol}"),
        Operand::Null => "null".to_string(),
    }
}

fn render_callee(callee: &Callee) -> String {
    match callee {
        Callee::Runtime(symbol) => format!("@{symbol}"),
        Callee::Science(symbol) | Callee::Foreign(symbol) => format!("@{symbol}"),
        Callee::Intrinsic(intrinsic) => format!("@{}", intrinsic.llvm_name()),
    }
}

fn render_int_op(op: IntOp) -> &'static str {
    match op {
        IntOp::Add => "add",
        IntOp::Sub => "sub",
        IntOp::Mul => "mul",
        IntOp::SDiv => "sdiv",
        IntOp::UDiv => "udiv",
        IntOp::SRem => "srem",
        IntOp::URem => "urem",
        IntOp::And => "and",
        IntOp::Or => "or",
        IntOp::Xor => "xor",
        IntOp::Shl => "shl",
        IntOp::AShr => "ashr",
        IntOp::LShr => "lshr",
    }
}

/// Note what is **not** here: there is no case producing `fma`, `fmuladd` or a
/// fast-math flag, and there is no `flags` parameter for one to arrive through.
/// [`FloatOp`] has no variant that could reach such a case, so this function is
/// total without one.
fn render_float_op(op: FloatOp) -> &'static str {
    match op {
        FloatOp::Add => "fadd",
        FloatOp::Sub => "fsub",
        FloatOp::Mul => "fmul",
        FloatOp::Div => "fdiv",
        FloatOp::Rem => "frem",
    }
}

fn render_cmp(op: CmpOp, signed: bool) -> String {
    let suffix = match (op, signed) {
        (CmpOp::Eq, _) => "eq",
        (CmpOp::Ne, _) => "ne",
        (CmpOp::Lt, true) => "slt",
        (CmpOp::Lt, false) => "ult",
        (CmpOp::Le, true) => "sle",
        (CmpOp::Le, false) => "ule",
        (CmpOp::Gt, true) => "sgt",
        (CmpOp::Gt, false) => "ugt",
        (CmpOp::Ge, true) => "sge",
        (CmpOp::Ge, false) => "uge",
    };
    format!("icmp {suffix}")
}

fn render_param(param: &AbiParam) -> String {
    let mut text = match param.class {
        ArgClass::Ignore => "; zero-sized, passed as nothing".to_string(),
        ArgClass::Direct => render_layout(&param.layout),
        ArgClass::IndirectByPointer => format!("ptr byval({})", render_layout(&param.layout)),
    };
    // Attribute order is fixed so that a transcript is comparable. LLVM does
    // not care; a diff does.
    if param.attrs.noalias {
        text.push_str(" noalias");
    }
    if param.attrs.nocapture {
        text.push_str(" nocapture");
    }
    if param.attrs.readonly {
        text.push_str(" readonly");
    }
    if let Some(align) = param.attrs.align {
        let _ = write!(text, " align {align}");
    }
    format!("{text} %{}", param.name)
}

fn render_signature(sig: &AbiSignature) -> String {
    let mut params: Vec<String> = Vec::new();
    // Decision 21's hidden pointer, materialised here and only here. It is not
    // in `sig.params` — see `AbiSignature::params` — so a backend that forgets
    // this line emits a call the runtime disagrees with, which is §9.2's
    // failure exactly.
    if sig.ret.is_sret() {
        params.push(format!("ptr sret({}) align {} %ret", render_layout(&sig.ret_layout), sig.ret_layout.align));
    }
    params.extend(sig.params.iter().map(render_param));

    let ret = match &sig.ret {
        ReturnClass::Void | ReturnClass::Indirect => "void".to_string(),
        ReturnClass::Direct { .. } => render_layout(&sig.ret_layout),
    };
    let keyword = if sig.foreign { "declare" } else { "define" };
    let attrs = if sig.nounwind { " nounwind" } else { "" };
    format!("{keyword} {ret} @{}({}){attrs}", sig.symbol, params.join(", "))
}

impl Backend for TextBackend {
    fn begin_module(&mut self, name: &str, target: &TargetConfig) -> Result<(), BackendError> {
        self.started = true;
        self.line(format!("; module {name}"));
        self.line(format!("target triple = \"{}\"", target.triple().as_str()));
        self.line(format!("target cpu = \"{}\"", target.cpu().as_str()));
        self.line(format!("opt level = {}", target.opt().as_str()));
        // The line that is correct by being written. §7.3 obligation 2.
        self.line(format!(
            "TargetOptions.AllowFPOpFusion = {}",
            target.fp_contract().llvm_name()
        ));
        Ok(())
    }

    fn define_string_bytes(&mut self, literal: &StringLiteral) -> Result<(), BackendError> {
        let escaped: String = literal
            .bytes
            .iter()
            .map(|b| {
                if b.is_ascii_graphic() || *b == b' ' {
                    (*b as char).to_string()
                } else {
                    format!("\\{b:02X}")
                }
            })
            .collect();
        self.line(format!(
            "@{} = private unnamed_addr constant [{} x i8] c\"{escaped}\"",
            literal.bytes_symbol,
            literal.len()
        ));
        Ok(())
    }

    fn define_type_info(&mut self, symbol: &str, info: &TypeInfo) -> Result<(), BackendError> {
        let drop = match &info.drop_fn {
            Some(glue) => format!("ptr @{glue}"),
            None => "ptr null".to_string(),
        };
        self.line(format!(
            "@{symbol} = private unnamed_addr constant %ScienceTypeInfo {{ i64 {}, i64 {}, {drop} }}",
            info.size, info.align
        ));
        Ok(())
    }

    fn define_map_info(&mut self, symbol: &str, info: &MapInfo) -> Result<(), BackendError> {
        self.line(format!(
            "@{symbol} = private unnamed_addr constant %ScienceMapInfo {{ \
             key {{ i64 {}, i64 {} }}, value {{ i64 {}, i64 {} }}, ptr @{}, ptr @{} }}",
            info.key.size, info.key.align, info.value.size, info.value.align, info.hash_fn, info.eq_fn
        ));
        Ok(())
    }

    fn declare_function(&mut self, sig: &AbiSignature) -> Result<FuncId, BackendError> {
        let id = FuncId(self.functions.len() as u32);
        self.functions.push(sig.symbol.clone());
        if sig.foreign {
            let text = render_signature(sig);
            self.line(text);
        }
        Ok(id)
    }

    fn define_function(
        &mut self,
        _func: FuncId,
        sig: &AbiSignature,
        body: &Body,
    ) -> Result<(), BackendError> {
        let header = render_signature(sig);
        self.line(format!("{header} {{"));
        for block in &body.blocks {
            self.render_block(block);
        }
        self.line("}");
        Ok(())
    }

    fn verify(&mut self) -> Result<(), BackendError> {
        // Decision 34's shape, with the checks a transcript can actually make.
        // A real verifier checks type agreement; this one checks the two
        // invariants the interface is responsible for, which is more than
        // nothing and is honest about being less than LLVM.
        if !self.started {
            return Err(BackendError::Other("no module was begun".to_string()));
        }
        if let Some((line, flag)) = crate::target::first_fast_math_flag(&self.out) {
            return Err(BackendError::VerifierFailed {
                function: format!("line {line}"),
                detail: format!(
                    "a fast-math flag `{flag}` reached the backend; \
                     `reproducibility.md` Decision 3 forbids every value-changing \
                     floating-point transform"
                ),
            });
        }
        if !self.out.contains("AllowFPOpFusion = Strict") {
            return Err(BackendError::VerifierFailed {
                function: "<module>".to_string(),
                detail: "the target machine was created without AllowFPOpFusion = Strict"
                    .to_string(),
            });
        }
        self.verified = true;
        Ok(())
    }

    fn emit(&mut self, kind: EmitKind) -> Result<Vec<u8>, BackendError> {
        // Decision 34: verify before emitting, even when the caller already
        // has. Verifying twice costs milliseconds.
        self.verify()?;
        match kind {
            EmitKind::Ir => Ok(self.out.clone().into_bytes()),
            // Honest refusal rather than an empty file. A stub that returned
            // `Ok(vec![])` for an object file would let a caller believe it had
            // compiled something.
            EmitKind::Object => Err(BackendError::Unsupported {
                what: "an object file: the stub backend emits text, and no LLVM is installed"
                    .to_string(),
            }),
            EmitKind::Executable => Err(BackendError::Unsupported {
                what: "an executable: the stub backend emits text, and no LLVM is installed"
                    .to_string(),
            }),
        }
    }
}

impl TextBackend {
    fn render_block(&mut self, block: &Block) {
        self.line(format!("{}:", block.label));
        for inst in &block.insts {
            let text = match inst {
                Inst::Alloca { local, layout } => {
                    format!("  %l{} = alloca {}, align {}", local.0, render_layout(layout), layout.align)
                }
                Inst::Load { dest, local } => format!("  %{} = load %l{}", dest.0, local.0),
                Inst::Store { local, value } => {
                    format!("  store {}, %l{}", render_operand(value), local.0)
                }
                Inst::IntBinary { dest, op, lhs, rhs } => format!(
                    "  %{} = {} {}, {}",
                    dest.0,
                    render_int_op(*op),
                    render_operand(lhs),
                    render_operand(rhs)
                ),
                Inst::FloatBinary { dest, op, lhs, rhs } => format!(
                    "  %{} = {} {}, {}",
                    dest.0,
                    render_float_op(*op),
                    render_operand(lhs),
                    render_operand(rhs)
                ),
                Inst::Cmp { dest, op, signed, lhs, rhs } => format!(
                    "  %{} = {} {}, {}",
                    dest.0,
                    render_cmp(*op, *signed),
                    render_operand(lhs),
                    render_operand(rhs)
                ),
                Inst::Call { dest, callee, args, ret, sret_slot } => {
                    let mut rendered: Vec<String> = Vec::new();
                    if ret.is_sret() {
                        match sret_slot {
                            Some(slot) => rendered.push(format!("ptr sret %l{}", slot.0)),
                            // Not a panic: a call classified `Indirect` with no
                            // slot is precisely §9.2's corruption, and the
                            // right response is to make it visible in the
                            // transcript where `verify` and a reader can both
                            // see it.
                            None => rendered.push("ptr sret <MISSING>".to_string()),
                        }
                    }
                    rendered.extend(args.iter().map(render_operand));
                    let prefix = match dest {
                        Some(dest) => format!("  %{} = call ", dest.0),
                        None => "  call ".to_string(),
                    };
                    format!("{prefix}{}({})", render_callee(callee), rendered.join(", "))
                }
            };
            self.line(text);
        }
        let terminator = match &block.terminator {
            Terminator::Return(None) => "  ret void".to_string(),
            Terminator::Return(Some(value)) => format!("  ret {}", render_operand(value)),
            Terminator::Goto(block) => format!("  br label %b{}", block.0),
            Terminator::Branch { cond, then_block, else_block } => format!(
                "  br {}, label %b{}, label %b{}",
                render_operand(cond),
                then_block.0,
                else_block.0
            ),
            Terminator::Switch { value, arms, default } => {
                let arms: Vec<String> =
                    arms.iter().map(|(d, b)| format!("{d} -> %b{}", b.0)).collect();
                format!(
                    "  switch {}, default %b{} [{}]",
                    render_operand(value),
                    default.0,
                    arms.join(", ")
                )
            }
            Terminator::Unreachable => "  unreachable".to_string(),
        };
        self.line(terminator);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::ParamAttrs;
    use crate::layout::{CgTy, FloatTy, IntTy, Triple, layout_of};
    use crate::target::{FpContract, OptLevel};

    fn config() -> TargetConfig {
        TargetConfig::new(Triple::X86_64LinuxGnu, OptLevel::O3)
    }

    #[test]
    fn the_module_header_records_the_contraction_setting() {
        let mut backend = TextBackend::new();
        backend.begin_module("m", &config()).unwrap();
        assert!(backend.transcript().contains("AllowFPOpFusion = Strict"));
        assert!(!backend.transcript().contains(FpContract::Standard.llvm_name()));
    }

    #[test]
    fn verify_refuses_a_module_that_never_set_contraction() {
        // The failure the interface exists to prevent, forced: a backend that
        // wrote a header without the line.
        let mut backend = TextBackend::new();
        backend.started = true;
        backend.line("; module m");
        let error = backend.verify().unwrap_err();
        assert!(matches!(error, BackendError::VerifierFailed { .. }));
    }

    #[test]
    fn verify_refuses_a_fast_math_flag() {
        let mut backend = TextBackend::new();
        backend.begin_module("m", &config()).unwrap();
        backend.line("  %x = fadd reassoc double %a, %b");
        let error = backend.verify().unwrap_err();
        match error {
            BackendError::VerifierFailed { detail, .. } => {
                assert!(detail.contains("reassoc"));
                assert!(detail.contains("Decision 3"));
            }
            other => panic!("expected a verifier failure, got {other:?}"),
        }
    }

    #[test]
    fn an_sret_call_renders_the_hidden_pointer_and_a_direct_one_does_not() {
        let string = crate::runtime::RtAggregate::String.layout(Triple::X86_64LinuxGnu);
        let sig = AbiSignature {
            symbol: "science_string_from_bytes".to_string(),
            ret: crate::abi::classify_return(crate::layout::CAbi::SystemVAmd64, &string),
            ret_layout: string,
            params: vec![
                AbiParam {
                    name: "ptr".to_string(),
                    class: ArgClass::Direct,
                    layout: layout_of(Triple::X86_64LinuxGnu, &CgTy::Ptr(crate::layout::PtrKind::Raw)),
                    attrs: ParamAttrs::default(),
                },
                AbiParam {
                    name: "len".to_string(),
                    class: ArgClass::Direct,
                    layout: layout_of(Triple::X86_64LinuxGnu, &CgTy::Int(IntTy::Usize)),
                    attrs: ParamAttrs::default(),
                },
            ],
            foreign: true,
            nounwind: true,
        };
        let mut backend = TextBackend::new();
        backend.begin_module("m", &config()).unwrap();
        backend.declare_function(&sig).unwrap();
        let text = backend.transcript();
        assert!(text.contains("sret"), "{text}");
        assert!(text.contains("declare void @science_string_from_bytes"), "{text}");
    }

    #[test]
    fn a_multiply_and_an_add_stay_two_instructions() {
        // Decision 36's assertion, at the level this machine can reach. The
        // transcript is not LLVM, so this proves the *lowering* never asks for
        // a fused operation; `tests/float_policy.rs` says what it does not
        // prove.
        let mut backend = TextBackend::new();
        backend.begin_module("m", &config()).unwrap();
        backend.render_block(&Block {
            id: crate::backend::BlockId(0),
            label: "b0".to_string(),
            insts: vec![
                Inst::FloatBinary {
                    dest: crate::backend::ValueId(0),
                    op: FloatOp::Mul,
                    lhs: Operand::ConstFloat(2.0),
                    rhs: Operand::ConstFloat(3.0),
                },
                Inst::FloatBinary {
                    dest: crate::backend::ValueId(1),
                    op: FloatOp::Add,
                    lhs: Operand::Value(crate::backend::ValueId(0)),
                    rhs: Operand::ConstFloat(1.0),
                },
            ],
            terminator: Terminator::Return(Some(Operand::Value(crate::backend::ValueId(1)))),
        });
        let text = backend.transcript();
        assert!(text.contains("fmul"));
        assert!(text.contains("fadd"));
        assert!(!text.contains("fma"), "{text}");
        assert!(!text.contains("fmuladd"), "{text}");
        backend.verify().unwrap();
    }

    #[test]
    fn emitting_an_object_file_is_refused_rather_than_faked() {
        let mut backend = TextBackend::new();
        backend.begin_module("m", &config()).unwrap();
        assert!(backend.emit(crate::backend::EmitKind::Ir).is_ok());
        match backend.emit(crate::backend::EmitKind::Object) {
            Err(BackendError::Unsupported { what }) => assert!(what.contains("no LLVM")),
            other => panic!("a stub that returns bytes here is a stub that lies: {other:?}"),
        }
        assert!(backend.emit(crate::backend::EmitKind::Executable).is_err());
    }

    #[test]
    fn a_descriptor_with_no_destructor_renders_a_null_and_not_a_symbol() {
        let mut backend = TextBackend::new();
        backend.begin_module("m", &config()).unwrap();
        backend
            .define_type_info("_S5ArrayEd.typeinfo", &TypeInfo { size: 8, align: 8, drop_fn: None })
            .unwrap();
        assert!(backend.transcript().contains("ptr null"));
        assert!(backend.transcript().contains("unnamed_addr constant"));
    }

    #[test]
    fn a_string_literal_renders_its_bytes_and_a_length() {
        let mut backend = TextBackend::new();
        backend.begin_module("m", &config()).unwrap();
        backend.define_string_bytes(&StringLiteral::new(0, "hi\n")).unwrap();
        let text = backend.transcript();
        assert!(text.contains("[3 x i8]"), "{text}");
        assert!(text.contains("\\0A"), "a newline must be escaped, not emitted: {text}");
    }

    #[test]
    fn the_transcript_is_deterministic() {
        let render = || {
            let mut backend = TextBackend::new();
            backend.begin_module("m", &config()).unwrap();
            backend.define_string_bytes(&StringLiteral::new(0, "x")).unwrap();
            backend
                .define_type_info("a.typeinfo", &TypeInfo { size: 1, align: 1, drop_fn: None })
                .unwrap();
            backend.transcript().to_string()
        };
        assert_eq!(render(), render());
    }

    #[test]
    fn a_float_constant_round_trips_through_the_transcript() {
        // A truncated float in a transcript is a comparison that passes for the
        // wrong reason.
        let rendered = render_operand(&Operand::ConstFloat(0.1 + 0.2));
        assert_eq!(rendered.parse::<f64>().unwrap(), 0.1 + 0.2);
        let _ = FloatTy::F64;
    }
}
