//! **The line.** Decision 42's `MIR -> backend` interface.
//!
//! **The decision.** [`Backend`] is the whole of what a code generator asks of a
//! target. Everything it receives is already decided: a [`Layout`] rather than a
//! type to lay out, an [`AbiSignature`] rather than a signature to classify, a
//! mangled symbol rather than a path, a [`TargetConfig`] that already carries
//! `AllowFPOpFusion = Strict`.
//!
//! **The reason, in Decision 42's own words.**
//!
//! > *A second backend need not agree with LLVM about instruction selection. It
//! > **must** agree about struct offsets, enum discriminant positions, niche
//! > encodings, `sret` classification, symbol names and descriptor contents —
//! > because an object file from one has to link against an object file from
//! > the other, and because `science-rt` is a fixed C ABI that both have to
//! > satisfy.*
//!
//! And the price of not doing this is observable: *"Rust is paying to retrofit
//! exactly this right now. `rustc_codegen_cranelift` exists to make debug builds
//! fast, and it has been expensive precisely because MIR-to-LLVM was not an
//! interface from the start: `rustc_codegen_ssa` had to be extracted from a
//! backend that had already grown into the compiler, years after the fact, and
//! the seam is still visible."*
//!
//! **The cost.** One layer of indirection in a crate nobody has written yet, and
//! *"the discipline not to reach through it — which is the part that actually
//! fails, because the fastest way to fix a bug at eleven at night is always to
//! let the backend peek at something above the line"*. §8.4's answer is that the
//! discipline can be enforced by `cargo` instead: this crate does not name LLVM,
//! `science-codegen-llvm` will, and the dependency points downward only. That
//! costs one line in a manifest and is the cheapest structural enforcement
//! available.
//!
//! # How large the instruction set is, and why it is not larger
//!
//! Exactly as large as §10's stages 0 to 2 need: a string literal, a runtime
//! call, a foreign call, scalar arithmetic, and a return. §10's discipline is
//! that *"every stage produces a program that runs and prints something"*, and
//! the same discipline applied to an interface says: do not design the
//! instruction set for stage 6 before stage 1 has run. Stage 3's `switch`,
//! bounds check and `phi`-free short-circuit are named in [`Terminator`] because
//! the CFG shape is the part an interface is expensive to get wrong; the
//! aggregate, generic and interface-dispatch operations of stages 4 to 6 are
//! not here at all.
//!
//! **What stages 2 and 3 found missing, measured by emitting them.**
//! `science-codegen-llvm`'s `emit` §2 is the list, and it is five: the address
//! of a local, the hidden `sret` slot, a nullable's niche alone, a unary
//! operator, and **a constant with a type**. The last is the one that was a
//! wrong answer rather than a refusal: [`Operand::ConstInt`] carries an `i128`
//! and no width, so `0i8 - 128i8` — two constants, nothing to infer a width
//! from — was computed at `i64` and stored into a one-byte slot, which opaque
//! pointers make legal IR that `LLVMVerifyModule` accepts. An interface that
//! carried the layout on the instruction, as `Alloca` already does, could not
//! have expressed it.
//!
//! # `LLVMVerifyModule` is not optional
//!
//! Decision 34: *"`LLVMVerifyModule` runs at every level including `-O0`, and a
//! verifier failure is an internal compiler error that prints the offending
//! function's IR."* [`Backend::verify`] is on the trait rather than left to the
//! implementation for that reason. Decision 1's first cost item is that calling
//! LLVM-C directly gives up Rust's type checking over value kinds — everything
//! is an `LLVMValueRef`, and `LLVMBuildAdd` on an `i64` and a `double` is a
//! verifier failure at best and a miscompile at worst. The verifier is the
//! mitigation and *"it is not optional"*.

use crate::abi::{AbiSignature, ReturnClass};
use crate::descriptor::{MapInfo, StringLiteral, TypeInfo, Vtable};
use crate::layout::Layout;
use crate::target::{Intrinsic, TargetConfig};

/// A backend's handle for a function it has declared or defined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FuncId(pub u32);

/// A backend's handle for a basic block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockId(pub u32);

/// A local variable, which Decision 8 makes an `alloca` in the entry block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LocalId(pub u32);

/// A value produced by an instruction within a function body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValueId(pub u32);

/// An operand: something an instruction reads.
#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    /// The result of an earlier instruction in this body.
    Value(ValueId),
    /// An integer constant.
    ConstInt(i128),
    /// A float constant.
    ///
    /// Carried as `f64` and emitted as an exact bit pattern by the backend.
    /// `reproducibility.md`'s policy makes this the one constant that must not
    /// be re-parsed from text on the way down: a decimal round-trip through a
    /// printer and a parser is a value-changing transform with no flag attached
    /// to it.
    ConstFloat(f64),
    /// The address of a global: a string literal's bytes, a descriptor, a
    /// function.
    GlobalAddr(String),
    /// A null pointer. The `null` half of a niched `T?`, and the `drop_fn` of a
    /// type that needs none.
    Null,
    /// One of the function's own parameters, by its index in
    /// [`AbiSignature::params`].
    ///
    /// **The decision.** A body names an argument by the *source* index, not by
    /// the position LLVM assigns it. The two differ whenever the return is
    /// [`ReturnClass::Indirect`] — the hidden `sret` pointer shifts everything
    /// by one — or whenever a parameter is [`crate::abi::ArgClass::Ignore`] and
    /// takes no position at all.
    ///
    /// **The reason.** `science-codegen-llvm`'s `emit` §2 listed this as one of
    /// the five things the interface could not say, and its `BodyState::params`
    /// was populated and `#[allow(dead_code)]` for exactly one reason: there
    /// was no operand to read it with. A backend without this cannot emit a
    /// function that takes an argument, which is every function in the language
    /// except a script's `main`. Making it an `Operand` rather than an `Inst`
    /// is what keeps Decision 8 intact: the entry block stores the parameter
    /// into the local's `alloca` with the ordinary `Inst::Store`, and every
    /// read afterwards is the ordinary `Inst::Load`.
    ///
    /// **The cost.** A backend must map source index to ABI position itself,
    /// and the mapping is the one described above rather than the identity.
    /// The alternative — having the caller pass the ABI position — puts
    /// knowledge of the `sret` shift above the line in every front end, which
    /// is the duplication Decision 42 exists to prevent.
    ///
    /// An [`crate::abi::ArgClass::IndirectByPointer`] parameter reads back as
    /// the **pointer**, not as the aggregate: Decision 22 passes it *"by
    /// pointer to a caller-owned slot"* and the pointer is what the frame has.
    Param(u32),
}

/// The arithmetic on integers a backend must emit.
///
/// Deliberately small. §2.6's inline table is longer than this; the rest is
/// stage 3 and beyond.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum IntOp {
    Add,
    Sub,
    Mul,
    /// Signed division.
    ///
    /// **"Division by zero is a panic the caller has already guarded, not a
    /// trap the backend inserts" — and no caller guards it.** That sentence
    /// was this variant's whole specification and it describes a phase that
    /// does not exist: nothing in THIR, MIR or the checker emits a test against
    /// zero before a `/`, and `science-codegen-llvm` found this by trying to
    /// lower one. LLVM's `sdiv` at a zero divisor is **undefined** — not a
    /// trap, not a panic — and `INT_MIN / -1` is undefined too, so a backend
    /// that took the sentence at its word would emit a program the optimiser
    /// may reason backwards from.
    ///
    /// So the LLVM backend **refuses** integer `/` and `%` by name, and §2.6's
    /// operation table owes an answer to a question it did not ask: whose
    /// guard, and at which level. The two candidates are a MIR statement pair
    /// (which keeps Decision 5's one-block-per-block, and costs MIR a notion of
    /// a panicking edge) and a backend expansion (which is two extra LLVM
    /// blocks per division and breaks that decision).
    SDiv,
    UDiv,
    SRem,
    URem,
    And,
    Or,
    Xor,
    Shl,
    AShr,
    LShr,
}

/// The arithmetic on floats a backend must emit.
///
/// **There is no `Fma` here and there never will be.** §7.3 obligation 2 is
/// that LLVM's `TargetOptions` defaults FP contraction to `Standard`, so an
/// `fmul` feeding an `fadd` fuses unless the backend says otherwise — and
/// [`TargetConfig`] says otherwise. A fused multiply-add is reachable only
/// through [`Intrinsic::Fma`], which is what `math.fma(a, b, c)` lowers to: *"a
/// user who wants the fused operation asks for it and gets it everywhere, which
/// is both faster and more deterministic than letting the backend decide per
/// call site"*.
///
/// **And there is no flags field.** §7.3 obligation 1 is to never set a
/// fast-math flag, which is *"an obligation to never opt in — a line that is
/// correct by not being written, which means nothing tests it"*. An enum with
/// no place to put a flag is stronger than a test, because it fails at compile
/// time in the backend that tried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum FloatOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

/// An integer or float comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// What is being called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Callee {
    /// A `science_`-prefixed runtime entry point (§2.6).
    Runtime(&'static str),
    /// A Science function, by mangled symbol.
    Science(String),
    /// An `extern "C"` declaration. Decision 40: *"an `extern "C"` call is a
    /// direct `call` to the declared symbol"* — no thunk, no wrapper, no
    /// trampoline.
    Foreign(String),
    /// A whitelisted intrinsic (Decision 37).
    Intrinsic(Intrinsic),
    /// Decision 13's dynamic dispatch: a function pointer loaded out of a
    /// vtable, called through the signature the *interface* declared.
    ///
    /// **The signature travels with the call and not with the callee, and
    /// that is what makes this safe to have.** Every other variant names a
    /// symbol a backend can look the type up from; this one names a value, so
    /// the `Inst::Call`'s own `ret` — and the parameter layouts its arguments
    /// were materialised at — are the only description of what is being
    /// called. `science-codegen-llvm`'s `Lowerer::dispatch_signature` builds
    /// that description from the interface's declaration and checks every
    /// implementation's against it before either reaches a vtable, which is
    /// §9.2's *"two signatures for one function"* hazard answered by
    /// comparing them rather than by trusting that they agree.
    Indirect {
        /// The function pointer, loaded out of the vtable by an earlier
        /// instruction in this block.
        function: ValueId,
        /// What is being called, as the interface declared it. Boxed because
        /// an [`AbiSignature`] is much larger than every other variant here
        /// and a `Callee` is copied at every call site.
        ///
        /// [`AbiSignature::symbol`] is not a symbol for this variant — there
        /// is no symbol — and carries a description for diagnostics instead:
        /// `any Error::message`, which is what a reader needs when the call
        /// is refused or when its argument count is wrong.
        signature: Box<AbiSignature>,
    },
}

/// One instruction.
#[derive(Debug, Clone, PartialEq)]
pub enum Inst {
    /// Reserve a slot for a local (Decision 8: an `alloca` in the entry block).
    Alloca {
        /// The local.
        local: LocalId,
        /// Its layout.
        layout: Layout,
    },
    /// Read a local.
    Load {
        /// Where the result goes.
        dest: ValueId,
        /// What to read.
        local: LocalId,
    },
    /// Write a local.
    Store {
        /// Where to write.
        local: LocalId,
        /// What to write.
        value: Operand,
    },
    /// Integer arithmetic.
    IntBinary {
        /// Result.
        dest: ValueId,
        /// Operation.
        op: IntOp,
        /// Left.
        lhs: Operand,
        /// Right.
        rhs: Operand,
    },
    /// Float arithmetic. Never fused; see [`FloatOp`].
    FloatBinary {
        /// Result.
        dest: ValueId,
        /// Operation.
        op: FloatOp,
        /// Left.
        lhs: Operand,
        /// Right.
        rhs: Operand,
    },
    /// A comparison producing an `i1`.
    Cmp {
        /// Result.
        dest: ValueId,
        /// Operation.
        op: CmpOp,
        /// Whether the operands are signed integers. Irrelevant for floats.
        signed: bool,
        /// Left.
        lhs: Operand,
        /// Right.
        rhs: Operand,
    },
    /// A call.
    ///
    /// The `ret` field is the classification, already made: a backend reads it
    /// and does not classify. That is the single most load-bearing field in the
    /// interface, because §9.2's finding is what happens when a call site gets
    /// it wrong for one symbol.
    Call {
        /// Where the result goes, when there is one.
        dest: Option<ValueId>,
        /// What is called.
        callee: Callee,
        /// Arguments, already lowered. An indirect aggregate argument is
        /// already a pointer here.
        args: Vec<Operand>,
        /// How the result comes back. When this is [`ReturnClass::Indirect`],
        /// `sret_slot` holds the caller-owned return slot.
        ret: ReturnClass,
        /// The return slot for an `sret` call.
        sret_slot: Option<LocalId>,
    },
}

/// How a basic block ends (§2.2).
///
/// Decision 5: *"Every MIR basic block becomes exactly one LLVM basic block, and
/// codegen merges nothing."* A MIR dump and an IR dump are then diffable against
/// each other, which is what Gate I and Gate J need.
#[derive(Debug, Clone, PartialEq)]
pub enum Terminator {
    /// `ret`.
    Return(Option<Operand>),
    /// `br`.
    Goto(BlockId),
    /// Conditional `br`.
    Branch {
        /// The `i1` condition.
        cond: Operand,
        /// Taken when true.
        then_block: BlockId,
        /// Taken when false.
        else_block: BlockId,
    },
    /// `switch`, for `match` over a discriminant. Arms in declaration order.
    Switch {
        /// The discriminant.
        value: Operand,
        /// `(discriminant, block)` pairs.
        arms: Vec<(u64, BlockId)>,
        /// Where an unmatched value goes. For an exhaustive `match` over a
        /// `choice` this is an `unreachable` block, which is what lets LLVM
        /// drop the range check.
        default: BlockId,
    },
    /// `unreachable`.
    ///
    /// Also what follows a call to `science_panic_bytes`: Decision 6 makes a
    /// panic edge `call` then `unreachable`, never `invoke`, because
    /// `science-rt`'s `panic.rs` says *"no landing pad is emitted anywhere in a
    /// Science binary"*.
    Unreachable,
}

/// One basic block.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// The block's identifier.
    pub id: BlockId,
    /// A label, for IR readability. Not semantic.
    pub label: String,
    /// The instructions, in order.
    pub insts: Vec<Inst>,
    /// How it ends.
    pub terminator: Terminator,
}

/// A function body.
#[derive(Debug, Clone, PartialEq)]
pub struct Body {
    /// Blocks in order. The first is the entry block and holds every `alloca`.
    pub blocks: Vec<Block>,
}

/// What a build asks the backend to produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitKind {
    /// `--emit=llvm-ir`: text, before the linker.
    Ir,
    /// `--emit=obj`: an object file.
    Object,
    /// An executable, by way of the linker driver (§5.1).
    Executable,
}

/// What went wrong below the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendError {
    /// The module failed [`Backend::verify`]. Decision 34 makes this an
    /// internal compiler error that prints the offending function's IR, so the
    /// message carries the IR and not a summary of it.
    VerifierFailed {
        /// The function whose IR failed.
        function: String,
        /// The verifier's message and the function's IR.
        detail: String,
    },
    /// The backend cannot produce this artefact.
    Unsupported {
        /// What was asked for.
        what: String,
    },
    /// Anything else the backend wants to report.
    Other(String),
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendError::VerifierFailed { function, detail } => {
                write!(f, "the module failed verification in `{function}`: {detail}")
            }
            BackendError::Unsupported { what } => write!(f, "unsupported: {what}"),
            BackendError::Other(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for BackendError {}

/// A code generation target.
///
/// Implementors: `science-codegen-llvm` (does not exist yet — see the crate
/// documentation §0) and [`crate::stub::TextBackend`].
///
/// Every method takes something already decided. If a method here ever needs to
/// compute a layout, classify a return or mangle a name, the line has moved and
/// Decision 42 has been lost.
pub trait Backend {
    /// Begin a module.
    ///
    /// Decision 4: **one LLVM module per crate**, no codegen units, no parallel
    /// codegen. The `target` carries `AllowFPOpFusion = Strict` and the
    /// implementation must apply it to whatever target machine it creates —
    /// that is §7.3's obligation 2 and it is the whole reason this parameter is
    /// a [`TargetConfig`] and not a triple.
    fn begin_module(&mut self, name: &str, target: &TargetConfig) -> Result<(), BackendError>;

    /// Emit a string literal's bytes as a `private unnamed_addr constant`
    /// (Decision 15).
    fn define_string_bytes(&mut self, literal: &StringLiteral) -> Result<(), BackendError>;

    /// Emit one `ScienceTypeInfo` (Decision 20).
    ///
    /// Called once per monomorphisation key. A backend that emitted one per
    /// call site would break the runtime's *"same descriptor contents"*
    /// precondition, which nothing detects.
    fn define_type_info(&mut self, symbol: &str, info: &TypeInfo) -> Result<(), BackendError>;

    /// Emit one `ScienceMapInfo`.
    fn define_map_info(&mut self, symbol: &str, info: &MapInfo) -> Result<(), BackendError>;

    /// Emit one of Decision 13's vtables (`Vtable`).
    ///
    /// **Called after every function the table names has been declared**, and
    /// the implementation may rely on that: a backend that has to look a
    /// method up by symbol has nothing to look up until the declaration pass
    /// has run, and a table holding a null where a method belongs is a call
    /// through `any I` that jumps to zero.
    fn define_vtable(&mut self, vtable: &Vtable) -> Result<(), BackendError>;

    /// Declare a function: a `declare` for a foreign or runtime symbol, or a
    /// forward declaration of a Science one.
    fn declare_function(&mut self, sig: &AbiSignature) -> Result<FuncId, BackendError>;

    /// Define a function's body.
    fn define_function(
        &mut self,
        func: FuncId,
        sig: &AbiSignature,
        body: &Body,
    ) -> Result<(), BackendError>;

    /// Run the verifier. **Not optional at any level** — Decision 34.
    fn verify(&mut self) -> Result<(), BackendError>;

    /// Produce the artefact.
    ///
    /// Implementations must call [`Backend::verify`] before emitting, and must
    /// do so even when the caller already has. Verifying twice costs
    /// milliseconds; emitting an unverified module costs a miscompile.
    fn emit(&mut self, kind: EmitKind) -> Result<Vec<u8>, BackendError>;
}
