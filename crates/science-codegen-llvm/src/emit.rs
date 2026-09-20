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
//! always to let the backend peek at something above the line"*. There are five
//! places below where this crate reaches for something the line does not carry,
//! and rather than peek they are named: [`ExtInst::LocalAddr`],
//! [`ExtInst::ReturnSlot`], [`ExtInst::LoadNiche`], [`ExtInst::Neg`] and
//! [`ExtInst::Const`]. See §2.
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
//! # 2. The things the interface cannot say
//!
//! `science_codegen::backend::Operand` has `Value`, `ConstInt`, `ConstFloat`,
//! `GlobalAddr`, `Null` and now `Param`, and `Inst` has `Alloca`, `Load`,
//! `Store`, two binaries, a `Cmp` and a `Call`. Stages 1 to 3 needed five
//! things that vocabulary could not express, and a `choice`, a second function
//! and a record need four more, and §5.3's bool-plus-out-parameter convention
//! needs two more still; each one is a variant of [`ExtInst`] rather than a
//! peek. **One of the eleven has since moved above the line**: `Operand::Param`
//! is `science_codegen::backend`'s now, because a body naming its own argument
//! is not an LLVM fact and the operand's note says so.
//!
//! The first three are stage 1's:
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
//! Stage 3 adds two more:
//!
//! 4. **Unary `-`** ([`ExtInst::Neg`]). `Inst` has no unary form at all. An
//!    integer negation would not have needed one — `IntBinary { Sub, 0, x }` is
//!    what `LLVMBuildNeg` emits — and a float negation does, because
//!    `fsub 0.0, x` is not `fneg x` at `x == +0.0`.
//! 5. **A constant with a type** ([`ExtInst::Const`]). This one closes a hole
//!    that was producing a **wrong answer**, and the entry below is the version
//!    that was wrong.
//!
//! A `choice`, a second function and a record add four more:
//!
//! 6. **A local bound to an indirect parameter** ([`ExtInst::ParamSlot`]).
//!    [`ExtInst::ReturnSlot`] on the argument side: Decision 22 passes an
//!    aggregate argument by pointer to a caller-owned slot, so the callee's
//!    local for it *is* that slot and an `alloca` is a private copy the
//!    caller never sees written.
//! 7. **The discriminant of a tagged `choice`** ([`ExtInst::LoadTag`],
//!    [`ExtInst::StoreTag`]). `Inst::Load` loads a local's whole type, and for
//!    a tagged layout [`LlvmBackend::llvm_type`] makes that an array of
//!    `align`-sized integers — a value no `switch` can branch on.
//!    [`ExtInst::LoadNiche`] is the same shape for Decision 19's other
//!    representation, and the two refuse each other's rather than guessing.
//! 8. **A field's address** ([`ExtInst::FieldAddr`]), which is §2.3's `.field`
//!    row and the last of the three `Inst` gaps the closing sentence named.
//! 9. **A load and a store through a computed address** ([`ExtInst::LoadAt`],
//!    [`ExtInst::StoreAt`]). `Inst::Load` and `Inst::Store` name a `LocalId`,
//!    so neither can reach a field once its address has been computed.
//!
//! `Map.insert` and `Map.remove` add two more, and both exist for the same
//! reason: `science-rt`'s §5.3 hands a `V?` back as one `bool` plus an
//! out-parameter [`crate::lower::Lowerer::lower_owned_nullable_call`] invents,
//! and turning that pair into a `V?` needs one instruction per representation
//! Decision 19 has.
//!
//! 10. **A discriminant written from a value, not a constant**
//!     ([`ExtInst::StoreTagFromValue`]). [`ExtInst::StoreTag`] writes §3.3's
//!     tag from a number known when the module is built — every `choice`
//!     literal and every `null` is one — and the tag `Map.insert` writes is
//!     not: it is the runtime's own `bool`, and §5.3 says outright that the
//!     two are *"the same byte as the discriminant of §5.1"*, which is what
//!     makes a store of it, and not a branch that picks one of two constants,
//!     the correct instruction.
//! 11. **A `select`** ([`ExtInst::Select`]). Decision 19's other
//!     representation has no tag to write. The runtime's out-parameter *is*
//!     the payload there, written when the `bool` comes back `true` and left
//!     untouched — not zeroed, *untouched* — when it comes back `false`, so
//!     the value already sitting in the destination's own memory is either
//!     the answer or garbage and nothing above this line can tell which
//!     without reading the `bool`. The honest fix is a branch, and Decision 5
//!     has nowhere to put one: it gives exactly one LLVM block to one MIR
//!     block, a call is a statement inside that block rather than its
//!     terminator, and `crate::lower`'s own module note already made this
//!     argument once, about a place projection rather than a call — *"a place
//!     projection is not a statement, so there is nowhere here to put three
//!     basic blocks"*. A `select` needs no block at all: it reads the
//!     (possibly garbage) payload unconditionally, the `bool` unconditionally,
//!     and picks between that payload and a null pointer as a value, which is
//!     safe precisely because the garbage is never the operand a later
//!     instruction reads — only `select`'s own choice is.
//!
//! Every one is a local extension and every one is meant to be deleted: when
//! `Inst` grows an `AddrOf`, a field projection and a load and store through a
//! pointer, [`ExtInst`] collapses to `Inst` and this section goes with it. Both
//! entry points run the same emitter, so the trait's `define_function` and the
//! extension's `define_function_ext` cannot drift.
//!
//! **The gap that was named and not repaired, and what it cost.**
//! `Operand::ConstInt` carries an `i128` and no type, so a constant's width has
//! to be inferred, and [`LlvmBackend::width_hint`] infers it from the *other*
//! operand. This section used to say that the failures were *"verifier failures
//! rather than miscompiles — Decision 34 catches them"*, and **that is false
//! for the store the arithmetic feeds.** `0i8 - 128i8` is two constants, so
//! there is no other operand: both took the default `i64`, the subtraction was
//! `i64`, and `store i64 %v, ptr %slot` into a one-byte `alloca` is *valid IR*,
//! because opaque pointers removed the only thing that related a store's width
//! to its destination's. It verified, it linked, it wrote eight bytes into one,
//! and the only symptom was a comparison that answered `false`.
//!
//! [`ExtInst::Const`] is half the repair — [`crate::lower`] materialises every
//! constant at the type the checker gave the expression, so nothing it emits
//! depends on the hint — and `Inst::Store`'s width check is the other half,
//! because the hint is still reachable from a hand-built body and a backend
//! cannot make this class loud any later than at the store.
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
use science_codegen::descriptor::{MapInfo, StringLiteral, TypeInfo, Vtable};
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
    /// runtime boundary does the same — every one of the entry points takes
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
    /// Materialise a constant at a layout's own type.
    ///
    /// **The fifth hole, and it is the one that was a wrong answer rather than
    /// a refusal.** §2's list already named `Operand::ConstInt`'s missing type
    /// as a gap and said [`LlvmBackend::width_hint`] covers it by reading the
    /// *other* operand — which works whenever the other operand is a value and
    /// does nothing when it is not. `0i8 - 128i8` is two constants, so the
    /// hint is empty, both take the default `i64`, the subtraction is `i64`,
    /// and the result is stored into a one-byte slot.
    ///
    /// **`store i64 %v, ptr %slot` is valid IR.** Opaque pointers removed the
    /// only thing that related a store's width to its destination's, so the
    /// verifier passes it, the program links, and it writes eight bytes into
    /// one byte of stack — a wrong value and a smashed neighbour, with nothing
    /// in the IR that looks wrong. §2's note claimed this class was *"verifier
    /// failures rather than miscompiles"*; that is true of `add i32 %x, i64 1`
    /// and false of the store it feeds.
    ///
    /// So [`crate::lower`] no longer relies on the hint: every constant it puts
    /// into an arithmetic instruction goes through this variant first, at the
    /// layout the type checker gave the expression, and the hint is left for
    /// the hand-built bodies in `tests/emitter.rs` that exercise it. The other
    /// half of the repair is in `Inst::Store`, which now refuses a width it was
    /// not expecting instead of writing it.
    Const {
        /// Where the typed constant goes.
        dest: ValueId,
        /// The type to build it at.
        layout: Layout,
        /// The constant. `Operand::Value` is refused: this exists to give a
        /// constant a type, and a value already has one.
        value: Operand,
    },
    /// Negation: §4.6's unary `-`.
    ///
    /// **The fourth hole, and it is an omission rather than a disagreement.**
    /// `science_codegen::backend::Inst` has `IntBinary`, `FloatBinary` and
    /// `Cmp` and no unary form at all, so §4.6's prefix `-` has no spelling
    /// above the line.
    ///
    /// **Integer negation would not have needed a variant** — `IntBinary` with
    /// `Sub`, a zero left operand and no `nsw` is exactly what LLVM's own
    /// `LLVMBuildNeg` emits — **and float negation does, and the difference is
    /// a signed zero.** `fsub double 0.0, x` is not `fneg x`: at `x == +0.0`
    /// the subtraction gives `+0.0` and the negation gives `-0.0`, and
    /// `reproducibility.md`'s policy forbids a value-changing float transform
    /// whoever makes it. One variant covering both is better than a correct
    /// lowering for one type and a wrong one for the other.
    ///
    /// §4.6's other prefix operator, `not`, is **not** here and needs nothing:
    /// [`crate::lower`] emits it as `Cmp { Eq, x, 0 }`, which is right at both
    /// of §3.1's widths for a `Bool` where a bitwise complement is right at
    /// neither.
    ///
    /// It collapses the way the others do: when `Inst` grows a unary form, this
    /// is `Above(..)` and goes.
    Neg {
        /// Where the result goes.
        dest: ValueId,
        /// What is negated. Integer or float, decided by the operand's type —
        /// asked of LLVM rather than carried, for
        /// [`LlvmBackend::value_is_float`]'s reason: the value already knows.
        operand: Operand,
    },
    /// §5.1's `as`: one scalar as another. [`ConvOp`] says which conversion.
    ///
    /// **The eighth hole, and it is the largest omission rather than a
    /// disagreement.** `science_codegen::backend::Inst` has `IntBinary`,
    /// `FloatBinary`, `Cmp` and `Call`, and **no conversion of any kind**, so
    /// `e as T` — which §5.1 says is *"always written, including where it loses
    /// precision"*, and is therefore the only way a Science program moves
    /// between two numeric types at all — has no spelling above the line.
    ///
    /// **The operation is decided in [`crate::lower`] and read here.** That is
    /// the same division `Inst::IntBinary` already has, and here it is not a
    /// convenience: `sext` against `zext` is decided by the **source's**
    /// signedness, and by the time a value reaches this module it is an
    /// `i32` with no sign attached — LLVM integer types are signless. A backend
    /// that guessed from the destination would render `-1i32 as U64` as
    /// `18446744073709551615`, which is legal IR, verifies, and is the wrong
    /// number. So the one bit that cannot be recovered is carried.
    ///
    /// **`from` is carried too, and only one arm needs it.**
    /// [`ConvOp::FloatToIntSat`] and [`ConvOp::FloatToUintSat`] are LLVM
    /// intrinsics whose names are overloaded on *both* widths —
    /// `llvm.fptosi.sat.i32.f64` — so the source's width has to be spelled, and
    /// asking LLVM for the value's type would be reading back something the
    /// caller already knows. It is also checked against the operand, which is
    /// the same width check `Inst::Store` does and for finding 12's reason.
    ///
    /// Like every other variant here it collapses: when `Inst` grows a
    /// conversion form, this is `Above(..)` and goes.
    Convert {
        /// Where the result goes.
        dest: ValueId,
        /// Which conversion.
        op: ConvOp,
        /// What is converted. Must be an [`Operand::Value`]: a constant has no
        /// width of its own, so [`crate::lower`] materialises it through
        /// [`ExtInst::Const`] at `from` first, which is finding 12's repair
        /// applied one instruction earlier.
        value: Operand,
        /// The source's layout, for the intrinsic's name and the width check.
        from: Layout,
        /// The destination's layout.
        to: Layout,
    },
    /// Bind a local to an [`ArgClass::IndirectByPointer`] parameter's pointer
    /// instead of giving it an `alloca`.
    ///
    /// **The sixth hole, and it is [`ExtInst::ReturnSlot`] on the argument
    /// side.** Decision 22 passes every aggregate argument *"by pointer to a
    /// caller-owned slot"*, so the callee's local for that parameter **is** the
    /// caller's slot, reached through the pointer. Decision 8's *"every MIR
    /// local becomes an `alloca`"* would make a private copy of it, and the
    /// copy is not merely wasteful: the callee's writes would land in the copy
    /// and the pointer's `nocapture`-but-not-`readonly` contract says they land
    /// in the caller's slot.
    ///
    /// [`Operand::Param`] cannot do this job. That operand reads the parameter
    /// as a **value** — for an indirect parameter, the pointer — and storing a
    /// pointer into an `alloca` of the aggregate's type is a width mismatch the
    /// store check rejects. What is needed is a binding, and a binding is not an
    /// operand.
    ///
    /// It collapses the way the others do: when `Inst` can bind a local to a
    /// parameter, this variant goes with [`ExtInst::ReturnSlot`].
    ParamSlot {
        /// The local the pointer stands for.
        local: LocalId,
        /// The parameter's index in [`AbiSignature::params`] — the source
        /// index, which [`Operand::Param`]'s note distinguishes from the ABI
        /// position.
        index: u32,
        /// The pointee's layout, so the slot can be typed for a later `Load`.
        layout: Layout,
    },
    /// Load §3.3's discriminant of a [`Repr::Tagged`] local, and nothing else.
    ///
    /// **[`ExtInst::LoadNiche`] for the other of Decision 18's two
    /// representations.** `Inst::Load` loads a local's whole type, and for a
    /// tagged `choice` [`LlvmBackend::llvm_type`] makes that an array of
    /// `align`-sized integers — a value no `switch` can branch on. Decision 18
    /// puts the discriminant *"at offset 0, always"* and fixes its integer type,
    /// so the load is a typed load at the local's own address and needs no
    /// `getelementptr`.
    ///
    /// **This refuses a `Repr::Niched` rather than reading the niche.** The two
    /// are not interchangeable: a niched enum's "discriminant" is a *comparison*
    /// against the niche values, not a field, and answering a discriminant read
    /// with the raw pointer would make every `switch` over a niched nullable
    /// branch on an address.
    LoadTag {
        /// Where the discriminant goes.
        dest: ValueId,
        /// The tagged local.
        local: LocalId,
    },
    /// Store §3.3's discriminant into a [`Repr::Tagged`] local.
    ///
    /// The write half of [`ExtInst::LoadTag`], and the whole of constructing a
    /// payload-free variant. The payload union is **left undefined**, which
    /// Decision 18 permits — *"a variant with no payload contributes nothing to
    /// the union"* — and which is why this is a narrow store rather than a store
    /// of the whole type: a store of the whole type would need a value for bytes
    /// the variant does not have.
    StoreTag {
        /// The tagged local.
        local: LocalId,
        /// The discriminant value, from declaration order starting at zero.
        discriminant: u64,
    },
    /// Store §3.3's discriminant into a [`Repr::Tagged`] local, from a
    /// **runtime value** rather than a number known when the module is built.
    ///
    /// **The tenth hole, and it exists for §5.3's convention and nothing
    /// else.** `ExtInst::StoreTag` covers every discriminant this crate had
    /// occasion to write before `Map.insert` and `Map.remove`: a `choice`
    /// literal names its variant in the source, and `store_null` reads the
    /// null variant's number off the layout — both are constants by the time
    /// they reach an instruction. `science_map_insert`'s `bool` is not: it is
    /// the answer to "was there a displaced value", computed by
    /// `science-rt` at run time, and `science-rt`'s §5.3 makes the two
    /// representations coincide on purpose — *"the returned `bool` is the same
    /// byte as the discriminant of §5.1"* — so `SCIENCE_NULLABLE_NULL == 0`
    /// and `false` agree, `SCIENCE_NULLABLE_PRESENT == 1` and `true` agree,
    /// and the whole repair is one store of a value that already has the
    /// right bit pattern.
    ///
    /// **The width is checked and not assumed.** A `T?` is always a two-variant
    /// `choice`, so [`science_codegen::layout::IntTy::discriminant_for`]
    /// always answers `U8` for its tag, and `RtRet::Bool`'s memory form is
    /// `i8` too (§3.1) — the two happen to agree for every payload type this
    /// crate lays out, and [`LlvmBackend`]'s emitter checks that they do
    /// rather than trusting the coincidence, for `Inst::Store`'s reason:
    /// opaque pointers make a width mismatch here legal IR that writes past
    /// the tag and into the payload union.
    StoreTagFromValue {
        /// The tagged local — a `V?` under construction, never a `choice` a
        /// program wrote, because nothing else builds a tag from a value.
        local: LocalId,
        /// The runtime's `bool`, in its §3.1 memory form.
        value: Operand,
    },
    /// The address of a byte offset within an aggregate: §2.3's `.field`.
    ///
    /// **The seventh hole, and it is the one §2's closing sentence named.**
    /// That sentence says [`ExtInst`] collapses when `Inst` grows *"an `AddrOf`,
    /// … a `Param`, and … a field projection"*; `Operand::Param` now exists
    /// above the line and this is the third.
    ///
    /// **The offset is a byte offset and the index is not available.**
    /// [`crate::sys::LLVMBuildInBoundsGEP2`]'s note is the account: `llvm_type`
    /// materialises padding, so a Science field index is not an LLVM member
    /// index, and `science-codegen`'s `FieldPlace::offset` is the number every
    /// other part of this crate already treats as the authority.
    FieldAddr {
        /// Where the pointer goes.
        dest: ValueId,
        /// The base address. A pointer, so that projections chain:
        /// [`ExtInst::LocalAddr`] gives the first and this gives every one
        /// after it.
        base: Operand,
        /// The byte offset, from `science-codegen`'s layout.
        offset: u64,
    },
    /// Load a value of a known layout from a computed address.
    ///
    /// `Inst::Load` names a `LocalId` and so can only read a whole local.
    /// [`ExtInst::FieldAddr`] produces a `ValueId`, and this is what reads
    /// through one. The alignment is set from the layout rather than left to
    /// LLVM's idea of the type's, for `Inst::Alloca`'s reason: the descriptor
    /// handed to `science-rt` carries `science-codegen`'s number.
    LoadAt {
        /// Where the value goes.
        dest: ValueId,
        /// The address, which must be a pointer.
        address: Operand,
        /// What is there.
        layout: Layout,
    },
    /// Store a value of a known layout through a computed address.
    ///
    /// The write half of [`ExtInst::LoadAt`], and it carries the same width
    /// check `Inst::Store` does and for the same reason: opaque pointers removed
    /// the only thing relating a store's width to its destination's, so a store
    /// eight bytes wide into a one-byte field verifies, links, and smashes the
    /// next field.
    StoreAt {
        /// The address, which must be a pointer.
        address: Operand,
        /// What is there.
        layout: Layout,
        /// What to write.
        value: Operand,
    },
    /// `select`: choose one of two values of the same layout, with no branch
    /// and no basic block of its own.
    ///
    /// **The eleventh hole, and the other half of §5.3's repair.** Decision
    /// 19's niched `T?` has no tag for [`ExtInst::StoreTagFromValue`] to write
    /// — the whole layout *is* the payload — so `Map.insert`'s out-parameter
    /// is the destination's own address, written by `science_map_insert` when
    /// its `bool` is `true` and left alone when it is `false`. Left alone
    /// means exactly that: whatever bit pattern the destination's `alloca`
    /// happened to hold, which is what every other local in this crate holds
    /// before its first store. Reading that as the answer on the `false` edge
    /// is not a subtle bug, it is a published one, waiting for `Map.remove`
    /// to be the first caller: the value would be whatever a previous local
    /// left on the stack, and it would be a different wrong value on every
    /// run.
    ///
    /// **A branch, and not this instruction, is the reading that fits the
    /// module's existing prose** — `crate::lower`'s own note about
    /// `xs[i]` calls a branch and the two blocks it needs *"the honest fix"*
    /// for a comparable gap, and declines only because a place projection has
    /// nowhere to put them. A call does have somewhere: it is a statement, not
    /// a projection, and [`crate::lower::Lowerer::lower_call`] already returns
    /// a [`Terminator`] downstream of it. The reason this crate still does not
    /// branch here is Decision 5, not a shortage of blocks: *"every MIR basic
    /// block becomes exactly one LLVM basic block, and codegen merges
    /// nothing"* — read the other way, one MIR block does not become *two*
    /// LLVM blocks either, and every block this crate emits for a user's body
    /// already has a source block behind it except [`Lowerer::lower_c_main`]'s
    /// three, which are a whole function with no MIR body to hold a second
    /// terminator. Inventing a block *inside* `Map.remove`'s MIR block to hold
    /// the null-materialising store would be the same move `lower_c_main`
    /// makes, made somewhere Decision 5 claims exclusively for MIR — so the
    /// two are not offered the same excuse.
    ///
    /// **What a `select` buys instead**: the payload is read unconditionally —
    /// `ExtInst::LoadNiche`, on a slot that may hold garbage — and so is the
    /// `bool`, and LLVM's `select` on an `i1` and two operands of the same
    /// type is defined for every input, including a garbage operand on the
    /// branch not taken: the garbage is never observed, because `select`
    /// produces the *chosen* operand's value and nothing reads the other one
    /// through it. That is the same reasoning [`ExtInst::StoreTag`]'s own note
    /// already relies on for the payload union of a payload-free variant —
    /// *"left undefined, which Decision 18 permits"* — read at the value level
    /// instead of the memory level.
    Select {
        /// Where the chosen value goes.
        dest: ValueId,
        /// The condition: `i1`, or §3.1's `i8` memory form of a `Bool`, on the
        /// same terms [`Terminator::Branch`]'s own `cond` accepts — this
        /// crate's `bool_cond` narrows it the same way.
        cond: Operand,
        /// The value when `cond` is true.
        if_true: Operand,
        /// The value when `cond` is false. `Operand::Null` for §5.3's use:
        /// Decision 19's absent case, materialised directly rather than read
        /// from anywhere.
        if_false: Operand,
        /// The layout both operands share, so each can be materialised at its
        /// own type rather than guessed from the other the way
        /// [`LlvmBackend::width_hint`] guesses for arithmetic — this
        /// instruction has exactly two operands and carrying the type is
        /// cheaper than the inference [`ExtInst::Const`]'s own note found
        /// unsound.
        layout: Layout,
    },
}

/// Which conversion an [`ExtInst::Convert`] is.
///
/// **Nine operations, and every one of them is a decision somebody had to
/// take.** The list is the cross product of {integer, float} with itself, split
/// by the *source's* signedness where that changes the answer and by direction
/// where that changes the instruction. `crate::lower::Lowerer::conversion` is
/// what maps a pair of Science types onto one of these, and its own note is
/// where §5.1 is read out; this enum is only the instruction.
///
/// **A same-width, same-kind conversion is not here**, because it is not an
/// instruction: `I64 as U64` is the same bits under a different name, and LLVM
/// integers are signless, so the lowering emits the value unchanged rather than
/// a no-op `bitcast`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvOp {
    /// Integer to a narrower integer: `trunc`. Two's-complement truncation,
    /// which is Rust's `as` and what §5.1's *"including where it loses
    /// precision"* buys.
    Trunc,
    /// Integer to a wider integer, source signed: `sext`.
    SignExtend,
    /// Integer to a wider integer, source unsigned: `zext`.
    ZeroExtend,
    /// Signed integer to float: `sitofp`. Rounds to nearest-even.
    IntToFloat,
    /// Unsigned integer to float: `uitofp`.
    UintToFloat,
    /// Float to signed integer, **saturating, NaN to zero**:
    /// `llvm.fptosi.sat`.
    ///
    /// §5.1 decides this one in as many words — *"`as` from float to integer
    /// saturates, and NaN becomes zero"* — and a bare `fptosi` does **not** do
    /// it: LLVM says an out-of-range result is `poison`, which at `-O2` is a
    /// licence to delete the code that would have checked. The saturating
    /// intrinsic is the whole reason this is a `call` and not an instruction.
    FloatToIntSat,
    /// Float to unsigned integer, saturating, NaN to zero:
    /// `llvm.fptoui.sat`. A negative float saturates to zero.
    FloatToUintSat,
    /// Float to a wider float: `fpext`. Exact.
    FloatExtend,
    /// Float to a narrower float: `fptrunc`. Rounds, and overflows to an
    /// infinity rather than to poison.
    FloatTrunc,
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
            Scalar::Float(FloatTy::F16) => unsafe {
                sys::LLVMHalfTypeInContext(self.context.raw())
            },
            Scalar::Float(FloatTy::Bf16) => unsafe {
                sys::LLVMBFloatTypeInContext(self.context.raw())
            },
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
            // The *source* index; `emit_body` already applied the `sret` shift
            // and skipped every `ArgClass::Ignore` when it filled the table, so
            // a lookup here is exact and a body cannot name a position that
            // does not exist. `Operand::Param`'s own note is why the two
            // numberings differ.
            Operand::Param(index) => *state.params.get(&(*index as usize)).ok_or_else(|| {
                BackendError::Other(format!(
                    "the body reads parameter {index}, which its signature does not have — or \
                     which is `ArgClass::Ignore` and therefore occupies no position"
                ))
            })?,
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
                let (slot, ty, layout) = state.local_entry(*local)?;
                let v = self.operand(state, value, Some(ty))?;
                // §3.1's two forms of `Bool`, met from the register side. A
                // comparison produces an `i1` and a `Bool` local's slot is an
                // `i8`, so every `flag be i > 5` is this widening. The widening
                // is `zext` and not `sext` because `sext i1 -> i8` of `true` is
                // `0xff`.
                let v = self.widen_bool(v, ty);
                // **The width is checked, because nothing else checks it.**
                // Opaque pointers removed the relationship between a store's
                // value type and its destination's, so `store i64 %v, ptr %slot`
                // into a one-byte `alloca` verifies, links, and writes eight
                // bytes into one. That is not a hypothetical: it is what
                // `let b be 0i8 - 128i8` emitted before `ExtInst::Const`
                // existed, and the only symptom was a comparison that answered
                // `false`. A backend cannot make this loud any later than here.
                let value_ty = unsafe { sys::LLVMTypeOf(v) };
                if value_ty != ty && Some(value_ty) != self.niche_ty(&layout) {
                    return Err(BackendError::Other(format!(
                        "local _{} is {} and the value stored into it is {}; opaque pointers                          make the mismatch legal IR, so it is caught here or not at all",
                        local.0,
                        self.describe_type(ty),
                        self.describe_type(value_ty)
                    )));
                }
                unsafe { sys::LLVMBuildStore(b, v, slot) };
            }
            ExtInst::Const { dest, layout, value } => {
                if matches!(value, Operand::Value(_)) {
                    return Err(BackendError::Other(
                        "`ExtInst::Const` gives a constant a type, and a value already has one"
                            .to_string(),
                    ));
                }
                let ty = self.llvm_type(layout);
                let v = self.operand(state, value, Some(ty))?;
                if unsafe { sys::LLVMTypeOf(v) } != ty {
                    return Err(BackendError::Unsupported {
                        what: format!(
                            "a constant of a type this backend cannot build at {}",
                            self.describe_type(ty)
                        ),
                    });
                }
                state.values.insert(dest.0, v);
            }
            ExtInst::Neg { dest, operand } => {
                let value = self.operand(state, operand, None)?;
                let name = cstr(&format!("v{}", dest.0));
                let result = if self.value_is_float(value) {
                    unsafe { sys::LLVMBuildFNeg(b, value, name.as_ptr()) }
                } else {
                    // No `nsw`: see `crate::lower::Lowerer::lower_binary`'s note
                    // on overflow. `neg` of `Int.min` wraps to `Int.min`, which
                    // is the release semantics the core spec asks for and is
                    // not undefined.
                    unsafe { sys::LLVMBuildNeg(b, value, name.as_ptr()) }
                };
                state.values.insert(dest.0, result);
            }
            ExtInst::Convert { dest, op, value, from, to } => {
                let result = self.convert(state, *dest, *op, value, from, to)?;
                state.values.insert(dest.0, result);
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
            ExtInst::ParamSlot { local, index, layout } => {
                let slot = *state.params.get(&(*index as usize)).ok_or_else(|| {
                    BackendError::Other(format!(
                        "local _{} is bound to parameter {index}, which the signature does not \
                         have",
                        local.0
                    ))
                })?;
                if !self.value_is_pointer(slot) {
                    return Err(BackendError::Other(format!(
                        "local _{} is bound to parameter {index}, which is not a pointer; \
                         Decision 22 passes an aggregate argument by pointer and this one arrived \
                         by value",
                        local.0
                    )));
                }
                let ty = self.llvm_type(layout);
                state.locals.insert(local.0, (slot, ty, layout.clone()));
            }
            ExtInst::LoadTag { dest, local } => {
                let (slot, _, layout) = state.local_entry(*local)?;
                let ty = self.tag_ty(&layout).ok_or_else(|| {
                    BackendError::Other(format!(
                        "local _{} is read for a discriminant and its representation is not \
                         Decision 18's tagged one",
                        local.0
                    ))
                })?;
                let name = cstr(&format!("v{}", dest.0));
                let value = unsafe { sys::LLVMBuildLoad2(b, ty, slot, name.as_ptr()) };
                unsafe { sys::LLVMSetAlignment(value, layout.align as c_uint) };
                state.values.insert(dest.0, value);
            }
            ExtInst::StoreTag { local, discriminant } => {
                let (slot, _, layout) = state.local_entry(*local)?;
                let ty = self.tag_ty(&layout).ok_or_else(|| {
                    BackendError::Other(format!(
                        "local _{} is written with a discriminant and its representation is not \
                         Decision 18's tagged one",
                        local.0
                    ))
                })?;
                let value = unsafe { sys::LLVMConstInt(ty, *discriminant, 0) };
                let store = unsafe { sys::LLVMBuildStore(b, value, slot) };
                unsafe { sys::LLVMSetAlignment(store, layout.align as c_uint) };
            }
            ExtInst::StoreTagFromValue { local, value } => {
                let (slot, _, layout) = state.local_entry(*local)?;
                let ty = self.tag_ty(&layout).ok_or_else(|| {
                    BackendError::Other(format!(
                        "local _{} is written with a discriminant and its representation is not \
                         Decision 18's tagged one",
                        local.0
                    ))
                })?;
                let v = self.operand(state, value, Some(ty))?;
                let v = self.widen_bool(v, ty);
                // §5.3's convention is a `bool`, and a `T?` is always a
                // two-variant `choice`, so this width check is not expected to
                // fire — see the variant's own doc for why the two widths
                // happen to agree. It is here anyway, for `Inst::Store`'s
                // reason: opaque pointers make a mismatched store legal IR
                // that overwrites the payload union rather than only the tag.
                let value_ty = unsafe { sys::LLVMTypeOf(v) };
                if value_ty != ty {
                    return Err(BackendError::Other(format!(
                        "local _{}'s discriminant is {} and the runtime `bool` written into it \
                         is {}; §5.3's convention only holds where the two agree",
                        local.0,
                        self.describe_type(ty),
                        self.describe_type(value_ty)
                    )));
                }
                let store = unsafe { sys::LLVMBuildStore(b, v, slot) };
                unsafe { sys::LLVMSetAlignment(store, layout.align as c_uint) };
            }
            ExtInst::FieldAddr { dest, base, offset } => {
                let pointer = self.operand(state, base, Some(self.ptr_ty()))?;
                if !self.value_is_pointer(pointer) {
                    return Err(BackendError::Other(
                        "a field address off a base that is not a pointer".to_string(),
                    ));
                }
                let i8_ty = unsafe { sys::LLVMInt8TypeInContext(self.context.raw()) };
                let mut indices =
                    [unsafe { sys::LLVMConstInt(self.int_ty(64), *offset, 0) }];
                let name = cstr(&format!("v{}", dest.0));
                let value = unsafe {
                    sys::LLVMBuildInBoundsGEP2(
                        b,
                        i8_ty,
                        pointer,
                        indices.as_mut_ptr(),
                        1,
                        name.as_ptr(),
                    )
                };
                state.values.insert(dest.0, value);
            }
            ExtInst::LoadAt { dest, address, layout } => {
                let pointer = self.operand(state, address, Some(self.ptr_ty()))?;
                if !self.value_is_pointer(pointer) {
                    return Err(BackendError::Other(
                        "a load through an address that is not a pointer".to_string(),
                    ));
                }
                let ty = self.llvm_type(layout);
                let name = cstr(&format!("v{}", dest.0));
                let value = unsafe { sys::LLVMBuildLoad2(b, ty, pointer, name.as_ptr()) };
                unsafe { sys::LLVMSetAlignment(value, layout.align as c_uint) };
                state.values.insert(dest.0, value);
            }
            ExtInst::StoreAt { address, layout, value } => {
                let pointer = self.operand(state, address, Some(self.ptr_ty()))?;
                if !self.value_is_pointer(pointer) {
                    return Err(BackendError::Other(
                        "a store through an address that is not a pointer".to_string(),
                    ));
                }
                let ty = self.llvm_type(layout);
                let v = self.operand(state, value, Some(ty))?;
                let v = self.widen_bool(v, ty);
                // The same check `Inst::Store` carries, for the same reason and
                // with more to lose: a field's slot is *inside* another object,
                // so an over-wide store here does not merely write the wrong
                // value, it writes over the next field.
                let value_ty = unsafe { sys::LLVMTypeOf(v) };
                if value_ty != ty && Some(value_ty) != self.niche_ty(layout) {
                    return Err(BackendError::Other(format!(
                        "a field is {} and the value stored into it is {}; opaque pointers make \
                         the mismatch legal IR, so it is caught here or not at all",
                        self.describe_type(ty),
                        self.describe_type(value_ty)
                    )));
                }
                let store = unsafe { sys::LLVMBuildStore(b, v, pointer) };
                unsafe { sys::LLVMSetAlignment(store, layout.align as c_uint) };
            }
            ExtInst::Select { dest, cond, if_true, if_false, layout } => {
                let c = self.bool_cond(state, cond, "a `select`")?;
                let ty = self.llvm_type(layout);
                let t = self.operand(state, if_true, Some(ty))?;
                let f = self.operand(state, if_false, Some(ty))?;
                // The same width check every other instruction in this file
                // that takes an `Operand` against a `Layout` carries: an
                // opaque `select %c, i64 %t, ptr %f` (mismatched operand
                // types) is a verifier failure, so this is a refusal rather
                // than a miscompile, but `describe_type` is worth the two
                // extra branches for the message it can give at a call site
                // three functions removed from the mistake.
                let t_ty = unsafe { sys::LLVMTypeOf(t) };
                let f_ty = unsafe { sys::LLVMTypeOf(f) };
                if t_ty != ty || f_ty != ty {
                    return Err(BackendError::Other(format!(
                        "a `select` at {}, whose true operand is {} and false operand is {}",
                        self.describe_type(ty),
                        self.describe_type(t_ty),
                        self.describe_type(f_ty)
                    )));
                }
                let name = cstr(&format!("v{}", dest.0));
                let value = unsafe { sys::LLVMBuildSelect(b, c, t, f, name.as_ptr()) };
                state.values.insert(dest.0, value);
            }
            ExtInst::Above(Inst::Call { dest, callee, args, ret, sret_slot }) => {
                self.emit_call(state, dest, callee, args, ret, sret_slot)?;
            }
        }
        Ok(())
    }

    /// The LLVM integer type of a [`Repr::Tagged`] layout's discriminant.
    ///
    /// `None` for every other representation, which is what makes
    /// [`ExtInst::LoadTag`] and [`ExtInst::StoreTag`] refuse a niched enum
    /// rather than read its payload as a number.
    fn tag_ty(&self, layout: &Layout) -> Option<sys::LLVMTypeRef> {
        match &layout.repr {
            Repr::Tagged { tag, .. } => Some(self.scalar_ty(Scalar::Int(*tag))),
            _ => None,
        }
    }

    /// An `Operand` as an `i1`, narrowing §3.1's `i8` memory form of a `Bool`
    /// rather than refusing it.
    ///
    /// **Factored out of `Terminator::Branch` rather than written twice.**
    /// [`ExtInst::Select`] needs exactly this conversion for exactly this
    /// reason — a condition that came from `Inst::Load` of a `Bool` local, or
    /// from an `Inst::Call` returning `RtRet::Bool`, is an `i8`, and both
    /// `LLVMBuildCondBr` and `LLVMBuildSelect` require an `i1` — so a second
    /// copy of the five lines below is a second place for the two to drift
    /// apart, which is the failure mode every other shared check in this file
    /// (`niche_ty`, `tag_ty`, `widen_bool`) already exists to avoid.
    ///
    /// `what` names the instruction in the one message this can produce, the
    /// same way `Terminator::Branch`'s own inline version used to.
    fn bool_cond(
        &self,
        state: &BodyState,
        cond: &Operand,
        what: &str,
    ) -> Result<sys::LLVMValueRef, BackendError> {
        let c = self.operand(state, cond, Some(self.int_ty(1)))?;
        // `trunc i8 -> i1` keeps the low bit, which is the right answer for
        // the only two values a `Bool` slot may hold. It is also why
        // `crate::lower` emits `not x` as `x == 0` rather than as a bitwise
        // complement: a `Bool` whose slot held `0xff` would truncate to
        // `true`, and a complement of the memory form is exactly how `0xff`
        // would get in there.
        let c = if self.value_is_int_of_width(c, 8) {
            let name = cstr("cond");
            unsafe { sys::LLVMBuildTrunc(self.builder.raw(), c, self.int_ty(1), name.as_ptr()) }
        } else {
            c
        };
        if !self.value_is_int_of_width(c, 1) {
            return Err(BackendError::Unsupported {
                what: format!(
                    "{what} on a condition that is neither `i1` nor §3.1's `i8` memory form of \
                     a `Bool`"
                ),
            });
        }
        Ok(c)
    }

    /// Whether a value is a pointer.
    ///
    /// Asked of LLVM rather than inferred, for [`LlvmBackend::value_is_float`]'s
    /// reason. It guards the three instructions that take an address as an
    /// `Operand`: with opaque pointers a `getelementptr` on an integer is a
    /// verifier failure and a `load` from one is too, but the message names the
    /// instruction rather than the lowering that built it, so the check is here.
    fn value_is_pointer(&self, value: sys::LLVMValueRef) -> bool {
        self.type_kind_of(unsafe { sys::LLVMTypeOf(value) }) == sys::type_kind::POINTER
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

    /// §3.1's `i1`-to-`i8` widening, applied only where it is exactly that.
    ///
    /// **Narrow on purpose.** The only pair this touches is `i1` into `i8`;
    /// every other mismatch between a value and the slot it is stored into is
    /// left alone so that the verifier reports it. A general "coerce the value
    /// to the slot's type" helper would turn a lowering bug — an `i64` stored
    /// into an `i32` local — into a silent truncation, which is the class of
    /// failure §10's staging exists to avoid.
    fn widen_bool(
        &self,
        value: sys::LLVMValueRef,
        slot_ty: sys::LLVMTypeRef,
    ) -> sys::LLVMValueRef {
        if self.value_is_int_of_width(value, 1) && slot_ty == self.int_ty(8) {
            let name = cstr("b");
            unsafe { sys::LLVMBuildZExt(self.builder.raw(), value, slot_ty, name.as_ptr()) }
        } else {
            value
        }
    }

    /// [`ExtInst::Convert`], as one LLVM value.
    ///
    /// **The width is checked before anything is built**, which is the third
    /// place this crate does that and it is here for finding 12's reason: a
    /// `trunc` from an `i64` to an `i32` and a `trunc` from an `i16` to an
    /// `i32` are the same `ExtInst` and only one of them is legal, and LLVM's
    /// answer to the second is an assertion in a debug build and undefined in a
    /// release one. The releases are what ship.
    fn convert(
        &self,
        state: &BodyState,
        dest: ValueId,
        op: ConvOp,
        value: &Operand,
        from: &Layout,
        to: &Layout,
    ) -> Result<sys::LLVMValueRef, BackendError> {
        if !matches!(value, Operand::Value(_)) {
            return Err(BackendError::Other(
                "`ExtInst::Convert` converts a value: a constant has no width of its own, so it \
                 goes through `ExtInst::Const` at the source layout first"
                    .to_string(),
            ));
        }
        let source_ty = self.llvm_type(from);
        let dest_ty = self.llvm_type(to);
        let v = self.operand(state, value, Some(source_ty))?;
        let actual = unsafe { sys::LLVMTypeOf(v) };
        if actual != source_ty {
            return Err(BackendError::Other(format!(
                "a conversion from {} whose operand is {}: the two widths disagree and opaque \
                 pointers make the mismatch legal IR in every case but this one",
                self.describe_type(source_ty),
                self.describe_type(actual)
            )));
        }
        let b = self.builder.raw();
        let name = cstr(&format!("v{}", dest.0));
        let built = match op {
            ConvOp::Trunc => unsafe { sys::LLVMBuildTrunc(b, v, dest_ty, name.as_ptr()) },
            ConvOp::SignExtend => unsafe { sys::LLVMBuildSExt(b, v, dest_ty, name.as_ptr()) },
            ConvOp::ZeroExtend => unsafe { sys::LLVMBuildZExt(b, v, dest_ty, name.as_ptr()) },
            ConvOp::IntToFloat => unsafe { sys::LLVMBuildSIToFP(b, v, dest_ty, name.as_ptr()) },
            ConvOp::UintToFloat => unsafe { sys::LLVMBuildUIToFP(b, v, dest_ty, name.as_ptr()) },
            ConvOp::FloatExtend => unsafe { sys::LLVMBuildFPExt(b, v, dest_ty, name.as_ptr()) },
            ConvOp::FloatTrunc => unsafe { sys::LLVMBuildFPTrunc(b, v, dest_ty, name.as_ptr()) },
            ConvOp::FloatToIntSat | ConvOp::FloatToUintSat => {
                let signed = matches!(op, ConvOp::FloatToIntSat);
                self.saturating_float_to_int(v, from, to, signed, &name)?
            }
        };
        Ok(built)
    }

    /// §5.1's *"`as` from float to integer saturates, and NaN becomes zero"*,
    /// as a call to LLVM's own saturating intrinsic.
    ///
    /// # The decision
    ///
    /// `llvm.fptosi.sat.iN.fM` and `llvm.fptoui.sat.iN.fM`, declared into the
    /// module on first use and called like any other function.
    ///
    /// # The reason
    ///
    /// **A bare `fptosi` is not this conversion and cannot be made into it
    /// afterwards.** LLVM says the result of `fptosi` is `poison` when the
    /// value does not fit, and `poison` is not "some integer" — it is a licence
    /// for the optimiser to assume the case never happens, so a `select` that
    /// clamped the answer afterwards is a `select` `-O2` may delete along with
    /// the branch that fed it. The clamp has to happen to the *float*, before
    /// the conversion, which is four comparisons, three `select`s and two
    /// exactly-representable bounds per width — bounds that are not the obvious
    /// numbers, because `2^63` is a representable `double` and is not a
    /// representable `i64`, so the right constant for `I64` is
    /// `9223372036854774784.0`. The intrinsic is that sequence, written by
    /// people who own the definition of it, and it gives NaN-to-zero in the
    /// same instruction.
    ///
    /// # The cost
    ///
    /// **Two, and the first is the one to watch.** The intrinsic is declared by
    /// *name*: `LLVMAddFunction` with the overloaded spelling, which LLVM turns
    /// back into an intrinsic id when it parses the name. A misspelling is not
    /// a compile error here — it is an ordinary external function, which
    /// verifies, and then fails at **link** time with an undefined symbol,
    /// which is `SC0402` with `llvm.fptosi.sat.i64.f64` in it. The widths are
    /// therefore built from the layouts rather than written out, and
    /// `tests/casts.rs` runs one of each so that the name is exercised rather
    /// than reasoned about.
    ///
    /// The second is that this is the only `call` in the language that is not a
    /// runtime entry point or a user function, so `tests/symbols.rs`'s reverse
    /// list — the LLVM names that must stay undeclared — does not cover it:
    /// nothing in [`crate::sys`] is added for it and there is nothing there for
    /// that test to see.
    fn saturating_float_to_int(
        &self,
        value: sys::LLVMValueRef,
        from: &Layout,
        to: &Layout,
        signed: bool,
        name: &std::ffi::CStr,
    ) -> Result<sys::LLVMValueRef, BackendError> {
        let Repr::Scalar(Scalar::Float(float)) = from.repr else {
            return Err(BackendError::Other(
                "a float-to-integer conversion whose source is not a float".to_string(),
            ));
        };
        let Repr::Scalar(Scalar::Int(int)) = to.repr else {
            return Err(BackendError::Other(
                "a float-to-integer conversion whose destination is not an integer".to_string(),
            ));
        };
        // **The intrinsic's suffix is a name, not a width, and `bfloat` is
        // where the difference bites.** `F16` and `BF16` are both sixteen bits,
        // so a `format!("…f{float_bits}")` built from the width spells
        // `llvm.fptosi.sat.i32.f16` for both — which is correct for `half` and
        // silently **wrong** for `bfloat`, whose overload suffix is `.bf16`.
        // LLVM would accept the declaration and select the wrong intrinsic,
        // which is a verifier-clean wrong answer. The suffix is therefore
        // carried as the string it is.
        let float_suffix = match float {
            FloatTy::F16 => "f16",
            FloatTy::Bf16 => "bf16",
            FloatTy::F32 => "f32",
            FloatTy::F64 => "f64",
        };
        let int_bits = (int.width(self.triple()) * 8) as u32;
        let which = if signed { "fptosi" } else { "fptoui" };
        let symbol = format!("llvm.{which}.sat.i{int_bits}.{float_suffix}");
        let int_ty = self.int_ty(int_bits);
        let float_ty = self.scalar_ty(Scalar::Float(float));
        let mut params = [float_ty];
        let fn_type = unsafe { sys::LLVMFunctionType(int_ty, params.as_mut_ptr(), 1, 0) };
        let module = self.module_ref()?;
        let c_name = cstr(&symbol);
        let existing = unsafe { sys::LLVMGetNamedFunction(module.raw(), c_name.as_ptr()) };
        let function = if existing.is_null() {
            unsafe { sys::LLVMAddFunction(module.raw(), c_name.as_ptr(), fn_type) }
        } else {
            existing
        };
        let mut args = [value];
        Ok(unsafe {
            sys::LLVMBuildCall2(
                self.builder.raw(),
                fn_type,
                function,
                args.as_mut_ptr(),
                1,
                name.as_ptr(),
            )
        })
    }

    /// The type of a niched layout's niche scalar at offset 0, if it has one.
    ///
    /// **The one store whose value is narrower than its slot on purpose.**
    /// Decision 19 puts the absent case of a `T?` in the payload's first
    /// pointer, so `_0 = null` for an `Error?` is `store ptr null` into a
    /// `{ ptr, ptr }` slot: eight bytes of sixteen, with the vtable word left
    /// undefined, which §3.4 requires rather than merely permits — *"the vtable
    /// slot of a null trait object is undefined and codegen must never load
    /// it"*. [`ExtInst::LoadNiche`] is the same asymmetry read back, and this
    /// is what keeps the store's width check from rejecting its own half.
    fn niche_ty(&self, layout: &Layout) -> Option<sys::LLVMTypeRef> {
        let Repr::Niched { niche, payload, .. } = &layout.repr else { return None };
        if niche.offset != 0 {
            return None;
        }
        science_codegen::layout::scalar_leaves(payload)
            .into_iter()
            .find(|(offset, _)| *offset == 0)
            .map(|(_, scalar)| self.scalar_ty(scalar))
    }

    /// A type's kind and, for an integer, whether it is the one expected —
    /// enough for a message that says which two widths disagreed.
    ///
    /// `LLVMPrintTypeToString` would say it exactly and is not declared;
    /// `sys.rs`'s rule is *"declare only what you call"*, and a declaration
    /// added for an error path is one no test exercises. The integer widths
    /// this compiler can build are few enough to name.
    fn describe_type(&self, ty: sys::LLVMTypeRef) -> String {
        if self.type_kind_of(ty) == sys::type_kind::INTEGER {
            for bits in [1u32, 8, 16, 32, 64, 128] {
                if ty == self.int_ty(bits) {
                    return format!("i{bits}");
                }
            }
            return "an integer of some other width".to_string();
        }
        let kind = self.type_kind_of(ty);
        if kind == sys::type_kind::HALF {
            "half".to_string()
        } else if kind == sys::type_kind::BFLOAT {
            "bfloat".to_string()
        } else if kind == sys::type_kind::FLOAT {
            "float".to_string()
        } else if kind == sys::type_kind::DOUBLE {
            "double".to_string()
        } else if kind == sys::type_kind::POINTER {
            "ptr".to_string()
        } else {
            // A struct or an array, and `sys::type_kind` names neither —
            // `tests/abi_claims.rs` checks each named constant against what
            // LLVM does with it, so a constant added for a message would be a
            // claim nothing verifies.
            //
            // **This used to be `half`'s and `bfloat`'s home too**, before
            // either had a named constant: a `let h: F16 be 1.5` whose stored
            // value came out `double` (see `type_is_float`'s note) reported
            // the *local* — a `half` alloca, laid out as a scalar by
            // `layout.rs` — as *"an aggregate"*, which sent the diagnosis
            // looking at layout and codegen's aggregate-lowering path when the
            // fault was a missing type-kind comparison two functions away.
            "an aggregate".to_string()
        }
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

    /// Whether a type is `half`, `bfloat`, `float` or `double`.
    ///
    /// **All four, not the two this once named.** `layout.rs`'s `FloatTy` doc
    /// comment calls `F16`/`BF16` *"a headline type"* with the same run of the
    /// mill scalar status as `F32`, and `Operand::ConstFloat`'s handling below
    /// is where that claim was false: `expected.filter(|ty|
    /// self.type_is_float(*ty))` silently discarded a `half`/`bfloat`
    /// expectation because this predicate did not recognise either kind, so a
    /// literal headed for an `F16` slot built itself as a `double` instead —
    /// no diagnostic, just a store the width check downstream had to catch.
    /// `describe_type` had the matching half of the bug: without `HALF`/
    /// `BFLOAT` in its own kind match it fell through to *"an aggregate"*,
    /// which is why the caught mismatch was reported as one.
    fn type_is_float(&self, ty: sys::LLVMTypeRef) -> bool {
        let kind = self.type_kind_of(ty);
        kind == sys::type_kind::HALF
            || kind == sys::type_kind::BFLOAT
            || kind == sys::type_kind::FLOAT
            || kind == sys::type_kind::DOUBLE
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
        // **The indirect case is resolved first and separately, because it has
        // no symbol to look anything up by.** Every other callee names a
        // declaration this module already holds — that declaration is where
        // the function type and the classified signature come from — and
        // Decision 13's dispatch has neither: what it has is a pointer loaded
        // out of a vtable and the interface's own declaration of what lives in
        // that slot, which `Callee::Indirect` carries for exactly this reason.
        if let Callee::Indirect { function, signature } = callee {
            let pointer = *state.values.get(&function.0).ok_or_else(|| {
                BackendError::Other(format!(
                    "the dispatch of `{}` through a value no instruction in this block produced",
                    signature.symbol
                ))
            })?;
            let (fn_type, _) = self.fn_type(signature);
            return self.emit_call_through(
                state, dest, pointer, fn_type, signature, args, ret, sret_slot,
            );
        }
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
            Callee::Indirect { .. } => unreachable!("returned above"),
        };
        let declared = self
            .declared
            .get(&symbol)
            .ok_or_else(|| BackendError::Other(format!("`{symbol}` is called and not declared")))?;
        let function = declared.value;
        let fn_type = declared.fn_type;
        let sig = &declared.sig;
        self.emit_call_through(state, dest, function, fn_type, sig, args, ret, sret_slot)
    }

    /// The half of [`LlvmBackend::emit_call`] that is the same whether the
    /// callee was found by symbol or loaded out of a vtable: materialise the
    /// arguments at the parameter layouts, build the `call`, and put `sret`
    /// back on the call site.
    ///
    /// **Shared rather than copied, and the `sret` attribute is why.**
    /// `sys::LLVMAddCallSiteAttribute`'s own note says what a missing one
    /// costs on System V, and a second copy of this function would be a second
    /// place for it to go missing — on the dispatch path, where the receiver
    /// is erased and the symptom would be a corrupted register in whichever
    /// implementation happened to run.
    #[allow(clippy::too_many_arguments)]
    fn emit_call_through(
        &self,
        state: &mut BodyState,
        dest: &Option<ValueId>,
        function: sys::LLVMValueRef,
        fn_type: sys::LLVMTypeRef,
        sig: &AbiSignature,
        args: &[Operand],
        ret: &ReturnClass,
        sret_slot: &Option<LocalId>,
    ) -> Result<(), BackendError> {
        let symbol = sig.symbol.clone();
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
                let c = self.bool_cond(state, cond, "a branch")?;
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
    /// The function's parameters, by **source** index.
    ///
    /// **This was populated and unreadable, and the note that said so is worth
    /// keeping.** `science_codegen::backend::Operand` had no `Param` form, so a
    /// body had no way to name its own arguments; the table was filled in
    /// anyway *"so that the fix above the line is 'add an operand' and not 'add
    /// an operand and then find where the values come from'"*. That is exactly
    /// what happened: `Operand::Param` is above the line now and this table is
    /// what it reads.
    ///
    /// The key is the source index and not the ABI position, and the shift is
    /// applied where the table is filled: an `sret` return puts the hidden
    /// pointer at position 0 and moves everything down one, and an
    /// `ArgClass::Ignore` parameter takes no position at all. A body that named
    /// positions would have to know both.
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

    /// Decision 13's vtable: `[N x ptr]`, one method address per slot.
    ///
    /// **Every slot is looked up by symbol and a missing one is an error here
    /// rather than a null.** `define_type_info` takes the same line about its
    /// `drop_fn` and for a sharper reason: a null in a descriptor's drop slot
    /// means *"nothing to drop"* and is a legitimate value, while a null in a
    /// vtable slot is a call through `any I` that jumps to address zero. There
    /// is no reading of a missing method that produces a working program, so
    /// the module is refused while there is still something to say about it.
    ///
    /// The lookup is `LLVMGetNamedFunction` and not a declaration: every method
    /// a vtable names is a function this module defines, and the emission
    /// driver declares all of them before the first vtable is defined. A
    /// backend that declared one here would paper over a reachability bug —
    /// the method would be declared, never defined, and the failure would move
    /// from this line to the linker.
    fn define_vtable(&mut self, vtable: &Vtable) -> Result<(), BackendError> {
        if vtable.is_empty() {
            return Err(BackendError::Other(format!(
                "the vtable `{}` has no slots: an interface with no methods needs no table, and \
                 a zero-length one is a global nothing can index",
                vtable.symbol
            )));
        }
        let module = self.module_ref()?;
        let mut slots: Vec<sys::LLVMValueRef> = Vec::with_capacity(vtable.methods.len());
        for method in &vtable.methods {
            let name = cstr(method);
            let value = unsafe { sys::LLVMGetNamedFunction(module.raw(), name.as_ptr()) };
            if value.is_null() {
                return Err(BackendError::Other(format!(
                    "the vtable `{}` names `{method}`, which the module does not define",
                    vtable.symbol
                )));
            }
            slots.push(value);
        }
        let ptr_ty = self.ptr_ty();
        let constant = unsafe {
            sys::LLVMConstArray2(ptr_ty, slots.as_mut_ptr(), slots.len() as u64)
        };
        let array_ty = unsafe { sys::LLVMArrayType2(ptr_ty, slots.len() as u64) };
        let name = cstr(&vtable.symbol);
        let global = unsafe { sys::LLVMAddGlobal(module.raw(), array_ty, name.as_ptr()) };
        unsafe {
            sys::LLVMSetInitializer(global, constant);
            sys::LLVMSetGlobalConstant(global, 1);
            sys::LLVMSetLinkage(global, sys::linkage::PRIVATE);
            // **Not `unnamed_addr`, unlike every other constant here.** A
            // descriptor and a string literal may be merged with a
            // bit-identical twin because nothing compares their addresses;
            // two vtables that happen to hold the same method addresses are
            // two different types' tables, and the day anything asks *"is this
            // the same implementation"* — a downcast, an equality on `any I` —
            // the answer has to be about identity. Leaving the address named
            // costs a merge LLVM would otherwise make and keeps that question
            // answerable.
        }
        self.verified = false;
        Ok(())
    }

    /// Decision 20's `ScienceMapInfo`: two nested descriptors and two function
    /// addresses.
    ///
    /// **The refusal this replaced was right about the hazard and wrong about
    /// the remedy.** It read: *"emitting a descriptor whose `hash_fn` and
    /// `eq_fn` no function in the module defines would produce a module that
    /// verifies and crashes"* — true, and the answer is to check, exactly as
    /// [`LlvmBackend::define_type_info`] checks its `drop_fn` and
    /// [`LlvmBackend::define_vtable`] checks every slot. A null in a
    /// descriptor's function slot is worse than a link error, because the
    /// runtime calls it: a `hash_fn` that is null is a jump to address zero on
    /// the first `insert`.
    ///
    /// `LLVMGetNamedFunction` finds a **declaration** as well as a definition,
    /// which is what makes this work at all: `science_string_hash` and
    /// `science_int_hash` live in `science-rt` and this module only declares
    /// them. That is the difference from a vtable slot, whose method this
    /// module is expected to define, and it is why the error below says
    /// *declares* rather than *defines*.
    fn define_map_info(&mut self, symbol: &str, info: &MapInfo) -> Result<(), BackendError> {
        let module = self.module_ref()?;
        let usize_ty = self.int_ty((self.triple().pointer_width() * 8) as u32);
        let ptr_ty = self.ptr_ty();
        let mut nested = |descriptor: &science_codegen::descriptor::TypeInfo| {
            let mut members = [
                unsafe { sys::LLVMConstInt(usize_ty, descriptor.size, 0) },
                unsafe { sys::LLVMConstInt(usize_ty, descriptor.align, 0) },
                unsafe { sys::LLVMConstPointerNull(ptr_ty) },
            ];
            unsafe { sys::LLVMConstStructInContext(self.context.raw(), members.as_mut_ptr(), 3, 0) }
        };
        let key = nested(&info.key);
        let value = nested(&info.value);

        let mut function = |name: &str| -> Result<sys::LLVMValueRef, BackendError> {
            let c_name = cstr(name);
            let found = unsafe { sys::LLVMGetNamedFunction(module.raw(), c_name.as_ptr()) };
            if found.is_null() {
                return Err(BackendError::Other(format!(
                    "the map descriptor `{symbol}` names `{name}`, which the module does not                      declare: the runtime calls it on the first operation, so a null here is a                      jump to address zero rather than a link error"
                )));
            }
            Ok(found)
        };
        let hash = function(&info.hash_fn)?;
        let eq = function(&info.eq_fn)?;

        let mut members = [key, value, hash, eq];
        let constant = unsafe {
            sys::LLVMConstStructInContext(self.context.raw(), members.as_mut_ptr(), 4, 0)
        };
        let global_ty = {
            let mut info_ty = [usize_ty, usize_ty, ptr_ty];
            let nested_ty = self.anon_struct(&mut info_ty);
            let mut tys = [nested_ty, nested_ty, ptr_ty, ptr_ty];
            self.anon_struct(&mut tys)
        };
        let name = cstr(symbol);
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
