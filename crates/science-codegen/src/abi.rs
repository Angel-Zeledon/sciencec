//! The C ABI: §4 of `codegen-and-linking.md`. The section to read twice.
//!
//! **The decision.** F0 implements **return** classification for all three
//! targets and **no argument classifier at all** (Decision 21). Every aggregate
//! argument in a Science-to-Science call is passed by pointer (Decision 22), and
//! an aggregate argument across the `extern` boundary is refused as `SC0429`.
//!
//! **The reason.** The return rules are three rules with three special cases and
//! there is no way to refuse them: `science_string_from_bytes` returns
//! `ScienceString` by value, and a Science program that writes a string literal
//! calls it. The argument rules are the same classification *plus* register
//! exhaustion, stack alignment, the `al`-holds-the-vector-count varargs
//! convention, the Windows shadow space and AAPCS64's three separate counters —
//! an order of magnitude more surface, and the surface where the
//! three-members-versus-two bugs live. `ffi-c-boundary.md` §5.4 measured the
//! restriction and found every library in the target set usable under it.
//!
//! **The cost, and it is paid in the hot path.** A Science function returning
//! `(Int, Error?)` — two words, which System V returns in `rax`/`rdx` — goes
//! through `sret` instead, and `(T, Error?)` is the return shape of *every
//! fallible function in the language*. `ArgumentPromotion` and IPSCCP recover
//! most of it for `internal` functions at `-O2`; Decision 22 makes the
//! convention explicitly unstable so it can be changed the day the classifier
//! exists; and until then the language's most common call shape is slower than
//! C's for no reason except that the classifier is not written yet.
//!
//! # Why this module is above Decision 42's line
//!
//! `sret` classification is in Decision 42's table of things two backends must
//! agree on, and the reason is the sharpest one in the note: an object file from
//! one backend has to link against an object file from the other, and
//! `science-rt` is a fixed C ABI that both have to satisfy. A classifier inside
//! the LLVM backend is a classifier the second backend reimplements, and §4.1's
//! failure mode is *"not a compile error, not a link error, a `dgemm` that
//! returns numbers"*.
//!
//! # What a backend does with the answer
//!
//! An LLVM backend needs only [`ReturnClass::Indirect`] versus
//! [`ReturnClass::Direct`]: LLVM assigns registers itself once it is told
//! whether there is an `sret` parameter. [`ReturnClass::Direct`] carries the
//! eightbyte register classes anyway, because a backend that is not LLVM — the
//! fast debug backend Decision 42 is insurance for — has to assign them, and
//! recomputing them below the line is the duplication this module exists to
//! prevent.

use science_diagnostics::{Diagnostic, Label, Span};

use crate::diagnostics::code;
use crate::layout::{CAbi, CgTy, FloatTy, Layout, Scalar, Triple, scalar_leaves};

/// Which register file an eightbyte goes in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegClass {
    /// `rax`/`rdx`, `x0`/`x1` — a general-purpose register.
    Integer,
    /// `xmm0`/`xmm1`, `v0`–`v3` — a vector register.
    Sse,
}

/// How a function's return value is passed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnClass {
    /// Nothing is returned: a `()` return, or an aggregate of size zero.
    ///
    /// §3.2 says a zero-sized type *"is passed as nothing"*, which is a
    /// different answer from `Direct { registers: [] }` and worth a variant,
    /// because a backend that emitted an empty struct return would be emitting
    /// a value LLVM has to place somewhere.
    Void,
    /// Returned in registers.
    Direct {
        /// The eightbyte classes, in order. Empty is impossible; [`Self::Void`]
        /// is how nothing is spelled.
        registers: Vec<RegClass>,
    },
    /// Returned through a hidden pointer the caller passes: LLVM's `sret`.
    Indirect,
}

impl ReturnClass {
    /// Whether the caller must pass a return slot pointer.
    ///
    /// This is the one bit an LLVM backend needs, and it is a method rather
    /// than a match at every call site because §9.2's finding was *exactly*
    /// this bit getting the wrong answer for one entry point.
    pub fn is_sret(&self) -> bool {
        matches!(self, ReturnClass::Indirect)
    }
}

/// Classify a return value.
///
/// The rules, from §4.2's table, in full:
///
/// | Target | Rule |
/// |---|---|
/// | System V x86-64 | ≤ 16 bytes: classify the one or two eightbytes into `rax`/`rdx`/`xmm0`/`xmm1`. > 16 bytes, or any MEMORY eightbyte: hidden pointer in `rdi`, returned in `rax`. |
/// | AArch64 | An HFA of ≤ 4 identical float members: `v0`–`v3`. Otherwise ≤ 16 bytes: `x0`/`x1`. Otherwise: hidden pointer in `x8`. |
/// | Windows x64 | Exactly 1, 2, 4 or 8 bytes: `rax` (or `xmm0` for a single float member). Everything else: hidden pointer in `rcx`, returned in `rax`. |
///
/// The asymmetry in the middle column is the point of §4.1: a
/// `{ double, double }` is sixteen bytes, goes in `xmm0`/`xmm1` on System V,
/// in `v0`/`v1` on AArch64, and **by hidden reference** on Windows, because
/// sixteen is not one of 1, 2, 4 or 8.
pub fn classify_return(abi: CAbi, layout: &Layout) -> ReturnClass {
    if layout.size == 0 {
        return ReturnClass::Void;
    }
    match abi {
        CAbi::SystemVAmd64 => classify_return_sysv(layout),
        CAbi::Aapcs64 => classify_return_aapcs64(layout),
        CAbi::Win64 => classify_return_win64(layout),
    }
}

/// Classify a return value for a target.
pub fn classify_return_for(target: Triple, layout: &Layout) -> ReturnClass {
    classify_return(target.c_abi(), layout)
}

fn classify_return_sysv(layout: &Layout) -> ReturnClass {
    if layout.size > 16 {
        return ReturnClass::Indirect;
    }
    // One class per eightbyte, merged over the leaves that land in it. The
    // psABI's merge rule reduced to the cases F0 can produce: NO_CLASS with
    // anything is that thing, SSE with SSE stays SSE, and anything else
    // becomes INTEGER. MEMORY arises here only from a leaf straddling an
    // eightbyte boundary, which C layout cannot produce for a naturally
    // aligned scalar and which a packed representation could — F0 has none,
    // so the check is a guard rather than a path.
    let eightbytes = layout.size.div_ceil(8) as usize;
    let mut classes: Vec<Option<RegClass>> = vec![None; eightbytes];
    for (offset, scalar) in scalar_leaves(layout) {
        let width = scalar_width(scalar);
        if offset / 8 != (offset + width - 1) / 8 {
            return ReturnClass::Indirect;
        }
        let slot = (offset / 8) as usize;
        let class = match scalar {
            Scalar::Float(_) => RegClass::Sse,
            _ => RegClass::Integer,
        };
        classes[slot] = Some(match classes[slot] {
            None => class,
            Some(RegClass::Sse) if class == RegClass::Sse => RegClass::Sse,
            Some(_) => RegClass::Integer,
        });
    }
    // An eightbyte covered by padding alone is INTEGER: it is still returned.
    ReturnClass::Direct {
        registers: classes.into_iter().map(|c| c.unwrap_or(RegClass::Integer)).collect(),
    }
}

fn classify_return_aapcs64(layout: &Layout) -> ReturnClass {
    if let Some(count) = homogeneous_float_aggregate(layout) {
        let leaves = scalar_leaves(layout);
        let class = match leaves[0].1 {
            Scalar::Float(_) => RegClass::Sse,
            _ => RegClass::Integer,
        };
        return ReturnClass::Direct { registers: vec![class; count] };
    }
    if layout.size > 16 {
        return ReturnClass::Indirect;
    }
    ReturnClass::Direct { registers: vec![RegClass::Integer; layout.size.div_ceil(8) as usize] }
}

fn classify_return_win64(layout: &Layout) -> ReturnClass {
    if !matches!(layout.size, 1 | 2 | 4 | 8) {
        return ReturnClass::Indirect;
    }
    let leaves = scalar_leaves(layout);
    // "or `xmm0` for a single float member" — and *single* is load-bearing.
    // A `{ float, float }` is eight bytes and comes back in `rax`, not in
    // `xmm0`, because it is two members. Reading the rule as "contains a
    // float" puts a struct in the wrong register file and produces plausible
    // numbers.
    let class = match leaves.as_slice() {
        [(_, Scalar::Float(_))] => RegClass::Sse,
        _ => RegClass::Integer,
    };
    ReturnClass::Direct { registers: vec![class] }
}

/// The member count if `layout` is a homogeneous float aggregate of up to four
/// members, per AAPCS64.
///
/// "Homogeneous" means every leaf is the same floating-point type, and nested
/// structs flatten into the count — `{ { float, float }, { float, float } }` is
/// an HFA of four, not of two. A bare `double` is an HFA of one, which is why
/// the caller gets the same answer for a scalar and for a struct wrapping one:
/// AAPCS64 does too.
fn homogeneous_float_aggregate(layout: &Layout) -> Option<usize> {
    let leaves = scalar_leaves(layout);
    if leaves.is_empty() || leaves.len() > 4 {
        return None;
    }
    let first = match leaves[0].1 {
        Scalar::Float(float) => float,
        _ => return None,
    };
    for (_, scalar) in &leaves {
        match scalar {
            Scalar::Float(float) if *float == first => {}
            _ => return None,
        }
    }
    // A struct with padding between its floats is not an HFA: the members must
    // be consecutive. `{ float, i32-sized hole, float }` cannot arise from C
    // layout of two floats, but `{ float, double }` fails the type check above
    // and `{ double, [4 x i8] }` would fail here.
    let expected = leaves.len() as u64 * first.width();
    if layout.size != expected {
        return None;
    }
    Some(leaves.len())
}

fn scalar_width(scalar: Scalar) -> u64 {
    match scalar {
        Scalar::Bool => 1,
        Scalar::Char => 4,
        Scalar::Int(int) => int.width(Triple::X86_64LinuxGnu),
        Scalar::Float(float) => float.width(),
        Scalar::Pointer(_) => 8,
    }
}

/// How one argument is passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgClass {
    /// Passed as nothing: a zero-sized type. §3.2.
    Ignore,
    /// Passed in a register as itself. Every scalar, on every target.
    Direct,
    /// Passed as a pointer to a caller-owned slot.
    ///
    /// Decision 22, and note what it is *not*: this is not the platform C
    /// convention's indirect class, it is Science's private convention, which
    /// applies to every aggregate regardless of size. A `{ f64, f64 }` goes
    /// through memory here where System V would use `xmm0`/`xmm1`.
    IndirectByPointer,
}

/// Classify an argument in a Science-to-Science call (Decision 22).
///
/// One rule, and it is "pass a pointer". Legal because both sides are compiled
/// by the same compiler at the same version; safe because it is the same
/// convention `science-rt` already uses, every one of whose 45 entry points
/// takes aggregates as `*const`/`*mut` and never by value.
pub fn classify_science_argument(layout: &Layout) -> ArgClass {
    if layout.size == 0 {
        return ArgClass::Ignore;
    }
    match layout.repr {
        crate::layout::Repr::Scalar(_) => ArgClass::Direct,
        // A niched enum *is* its payload, so `(Box of T)?` is one pointer and
        // passes directly. Getting this wrong would pass a pointer to a
        // pointer, which is a type error nowhere and a wrong answer
        // everywhere.
        crate::layout::Repr::Niched { ref payload, .. } => classify_science_argument(payload),
        _ => ArgClass::IndirectByPointer,
    }
}

/// Why an `extern` signature was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbiRefusal {
    /// `SC0429`: a by-value aggregate crosses the `extern` boundary.
    ///
    /// §4.2 extends `ffi-c-boundary.md`'s code to **function-pointer types**,
    /// because a Science closure installed as a C callback taking a by-value
    /// aggregate is the same classification problem with the compiler on the
    /// callee side, where a mistake corrupts the *caller's* stack.
    AggregateByValue {
        /// What is being described, for the message: `"parameter"` or
        /// `"return value"`.
        position: String,
    },
    /// `SC0431`: `F16`/`BF16` by value across the boundary. The C ABI for
    /// half-precision is not settled across the three targets.
    HalfPrecision,
}

/// Classify an argument crossing the `extern "C"` boundary.
///
/// Scalars pass; aggregates are refused. The refusal is the decision: F0 has no
/// argument classifier, so a by-value aggregate has no correct answer to give
/// and guessing produces §4.1's `dgemm`.
pub fn classify_extern_argument(ty: &CgTy, layout: &Layout) -> Result<ArgClass, AbiRefusal> {
    check_extern_scalar(ty, layout, "parameter")
}

/// Classify a return value crossing the `extern "C"` boundary.
///
/// Note the asymmetry with [`classify_extern_argument`] and that it is
/// deliberate: an aggregate **return** is implemented, because the runtime
/// boundary forces it and there is no way to refuse
/// `science_string_from_bytes`. An aggregate **argument** is refused, because
/// nothing forces it and `ffi-c-boundary.md` §5.4 measured the cost of refusing
/// at zero for every library in the target set.
pub fn classify_extern_return(
    ty: &CgTy,
    layout: &Layout,
    abi: CAbi,
) -> Result<ReturnClass, AbiRefusal> {
    if let CgTy::Float(FloatTy::F32) = ty {
        // F32 is fine; the refusal is F16/BF16, which are not in the model.
    }
    Ok(classify_return(abi, layout))
}

fn check_extern_scalar(ty: &CgTy, layout: &Layout, position: &str) -> Result<ArgClass, AbiRefusal> {
    match ty {
        CgTy::Unit => Ok(ArgClass::Ignore),
        CgTy::Bool | CgTy::Char | CgTy::Int(_) | CgTy::Float(_) | CgTy::Ptr(_) => {
            debug_assert!(matches!(layout.repr, crate::layout::Repr::Scalar(_)));
            Ok(ArgClass::Direct)
        }
        // A niched nullable pointer is a pointer, and `ffi-c-boundary.md`'s
        // `ffi.Span of T` lowering to a bare pointer relies on it. A tagged
        // nullable is an aggregate and is refused like any other.
        CgTy::Nullable(_) => match &layout.repr {
            crate::layout::Repr::Niched { .. } => Ok(ArgClass::Direct),
            _ => Err(AbiRefusal::AggregateByValue { position: position.to_string() }),
        },
        CgTy::Struct { .. } | CgTy::Choice { .. } | CgTy::Array { .. } | CgTy::Interface => {
            Err(AbiRefusal::AggregateByValue { position: position.to_string() })
        }
    }
}

impl AbiRefusal {
    /// Render the refusal as a diagnostic.
    ///
    /// `SC0429` and `SC0431` are `ffi-c-boundary.md`'s codes, not this note's.
    /// §11 says so explicitly — *"Amended, not claimed"* — and a codegen that
    /// invented a code in its own range for this would be taking a code the
    /// partition gave to somebody else.
    pub fn to_diagnostic(&self, span: Span) -> Diagnostic {
        match self {
            AbiRefusal::AggregateByValue { position } => Diagnostic::error(
                code::SC0429,
                "a by-value aggregate cannot cross the `extern \"C\"` boundary",
            )
            .with_label(Label::primary(span, format!("this {position} is an aggregate")))
            .with_note(
                "F0 implements no argument classifier: System V, AAPCS64 and Windows x64 \
                 classify aggregates by three different rules, and guessing links cleanly \
                 and corrupts the stack at run time",
            )
            .with_note("pass a pointer to it instead; every library in the target set does"),
            AbiRefusal::HalfPrecision => Diagnostic::error(
                code::SC0431,
                "`F16` and `BF16` cannot cross the `extern \"C\"` boundary by value",
            )
            .with_label(Label::primary(span, "half-precision by value"))
            .with_note("the C ABI for half-precision is not settled across the three targets"),
        }
    }
}

/// What a borrow parameter is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorrowKind {
    /// `borrowed T`.
    Shared,
    /// `mutable borrowed T`.
    Exclusive,
}

/// The attributes a parameter carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ParamAttrs {
    /// `noalias`. See [`borrow_attrs`] for the one place this is dangerous.
    pub noalias: bool,
    /// `nocapture`.
    pub nocapture: bool,
    /// `readonly`.
    pub readonly: bool,
    /// `align N`, when the pointee's alignment is known.
    pub align: Option<u64>,
    /// `sret`. Only ever on the hidden return slot parameter.
    pub sret: bool,
}

impl ParamAttrs {
    /// The hidden return slot parameter of an [`ReturnClass::Indirect`] return.
    pub fn sret(align: u64) -> ParamAttrs {
        ParamAttrs { sret: true, noalias: true, nocapture: true, align: Some(align), ..Default::default() }
    }
}

/// Attributes for a borrow parameter (Decision 24).
///
/// > *A `mutable borrowed T` parameter is emitted `noalias nocapture` and
/// > aligned. A `borrowed T` parameter is emitted `readonly nocapture` and
/// > aligned, and **not** `noalias`.*
///
/// **Why `borrowed` does not get `noalias`.** Two shared borrows of the same
/// place are legal — rule 4 is *"shared many, or exclusive one"* — so a
/// `noalias` claim on a shared borrow is a lie the optimiser is entitled to act
/// on.
///
/// **Why this is the most dangerous line in the crate.** §4.4:
/// *"This is the single place in the language where a bug in region inference
/// produces a wrong answer rather than a missed error."* Everywhere else a
/// soundness hole means a program is accepted that should have been rejected,
/// and it runs and does something. Here, `noalias` tells LLVM it may reorder,
/// cache and duplicate loads and stores across the call, and if rule 4 did not
/// actually hold the optimiser computes something else. It is the attribute
/// whose miscompiles took rustc years to shake out, and rustc had lifetime
/// annotations to reason from.
///
/// **`no_noalias` is §4.4's mitigation and it should be taken.** An unsupported
/// debugging flag that suppresses the attribute, so that "is this a region bug
/// or a codegen bug" is one recompile rather than a week. It is three lines and
/// it is the flag every backend eventually adds after not having it.
pub fn borrow_attrs(kind: BorrowKind, align: u64, no_noalias: bool) -> ParamAttrs {
    match kind {
        BorrowKind::Shared => ParamAttrs {
            noalias: false,
            nocapture: true,
            readonly: true,
            align: Some(align),
            sret: false,
        },
        BorrowKind::Exclusive => ParamAttrs {
            noalias: !no_noalias,
            nocapture: true,
            readonly: false,
            align: Some(align),
            sret: false,
        },
    }
}

/// Attributes for a pointer produced by [`ArgClass::IndirectByPointer`].
///
/// Decision 22's by-pointer aggregate argument is a *move* into the callee, not
/// a borrow: the caller owns the slot and must treat it as moved-from
/// afterwards, which is `science-rt` §3's third row. So it is `nocapture` and
/// aligned, and it is **not** `readonly` — the callee may write through it —
/// and it is **not** `noalias`, because nothing in the region rules forbids the
/// caller passing the same slot twice to a two-argument function.
pub fn indirect_argument_attrs(align: u64) -> ParamAttrs {
    ParamAttrs { noalias: false, nocapture: true, readonly: false, align: Some(align), sret: false }
}

/// A function signature after ABI classification: what crosses Decision 42's
/// line.
///
/// Everything here is decided. A backend reads it and emits; it does not
/// classify, does not compute a layout, and does not decide an attribute. That
/// is the whole of Decision 42 expressed as a struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbiSignature {
    /// The mangled symbol, or the C name for an `extern` declaration.
    pub symbol: String,
    /// How the return value comes back.
    pub ret: ReturnClass,
    /// The return type's layout, needed for the `sret` slot's size and
    /// alignment.
    pub ret_layout: Layout,
    /// Parameters, in source order. An [`ReturnClass::Indirect`] return's
    /// hidden pointer is **not** here: it is implied by `ret`, and a backend
    /// that prepended it to this list would double it the first time anyone
    /// iterated both.
    pub params: Vec<AbiParam>,
    /// Whether this is a declaration of a foreign symbol (`declare`) rather
    /// than a definition.
    pub foreign: bool,
    /// `nounwind`, always. Decision 6: codegen never emits `invoke` or
    /// `landingpad`, because `science-rt`'s `panic.rs` says *"no destructor
    /// runs, no frame is popped, no landing pad is emitted anywhere in a
    /// Science binary"*. The field exists so that a backend reads the fact
    /// rather than assuming it; F1's unwinding is the one-way door §2.2 names.
    pub nounwind: bool,
}

/// One classified parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbiParam {
    /// The parameter's name, for IR readability and debug info.
    pub name: String,
    /// How it is passed.
    pub class: ArgClass,
    /// Its layout.
    pub layout: Layout,
    /// Its attributes.
    pub attrs: ParamAttrs,
}

impl AbiSignature {
    /// A Science-to-Science function signature, classified.
    pub fn science(
        target: Triple,
        symbol: impl Into<String>,
        ret_layout: Layout,
        params: Vec<(String, Layout, ParamAttrs)>,
    ) -> AbiSignature {
        let ret = classify_return_for(target, &ret_layout);
        AbiSignature {
            symbol: symbol.into(),
            ret,
            ret_layout,
            params: params
                .into_iter()
                .map(|(name, layout, attrs)| AbiParam {
                    class: classify_science_argument(&layout),
                    name,
                    layout,
                    attrs,
                })
                .collect(),
            foreign: false,
            nounwind: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Field, FloatTy, IntTy, PtrKind, layout_of};

    fn lay(ty: &CgTy) -> Layout {
        layout_of(Triple::X86_64LinuxGnu, ty)
    }

    fn strukt(fields: &[CgTy]) -> CgTy {
        CgTy::strukt(
            "S",
            fields.iter().enumerate().map(|(i, t)| Field::new(i.to_string(), t.clone())).collect(),
        )
    }

    const F64: CgTy = CgTy::Float(FloatTy::F64);
    const F32: CgTy = CgTy::Float(FloatTy::F32);
    const I64: CgTy = CgTy::Int(IntTy::I64);

    #[test]
    fn the_three_conventions_disagree_about_two_doubles_and_that_is_the_point() {
        // §4.1's worked example. `{ double, double }` is sixteen bytes:
        // register-passed on System V and AAPCS64, by hidden reference on
        // Windows, because sixteen is not one of 1, 2, 4 or 8. A classifier
        // that treated Windows "like the others" would return numbers.
        let layout = lay(&strukt(&[F64, F64]));
        assert_eq!(
            classify_return(CAbi::SystemVAmd64, &layout),
            ReturnClass::Direct { registers: vec![RegClass::Sse, RegClass::Sse] }
        );
        assert_eq!(
            classify_return(CAbi::Aapcs64, &layout),
            ReturnClass::Direct { registers: vec![RegClass::Sse, RegClass::Sse] }
        );
        assert_eq!(classify_return(CAbi::Win64, &layout), ReturnClass::Indirect);
    }

    #[test]
    fn three_doubles_go_on_the_stack_on_two_conventions_and_in_registers_on_the_third() {
        // §4.1: "A classifier that gets the merge rule right for two members
        // and wrong for three produces a library that works on `Complex64` and
        // fails on a 3-vector."
        //
        // That sentence sits in the **System V** bullet and is true there: 24
        // bytes is over the 16-byte cut-off. It is false on AArch64, where a
        // 3-vector of doubles is an HFA of three and goes in `v0`–`v2`, and the
        // AAPCS64 bullet says so two paragraphs later. Reading the first bullet
        // as the general rule — which is easy, because it is the one with the
        // memorable example — gives a classifier that passes a 3-vector
        // indirectly on Apple silicon and corrupts the call.
        //
        // This test is written per-convention for that reason. A loop over the
        // three ABIs asserting one answer was the first version and it was
        // wrong.
        let layout = lay(&strukt(&[F64, F64, F64]));
        assert_eq!(classify_return(CAbi::SystemVAmd64, &layout), ReturnClass::Indirect);
        assert_eq!(classify_return(CAbi::Win64, &layout), ReturnClass::Indirect);
        assert_eq!(
            classify_return(CAbi::Aapcs64, &layout),
            ReturnClass::Direct { registers: vec![RegClass::Sse; 3] }
        );
        // Four is still an HFA; five is not, and that is where AAPCS64 agrees
        // with the others again.
        assert_eq!(classify_return(CAbi::Aapcs64, &lay(&CgTy::array(F64, 5))), ReturnClass::Indirect);
    }

    #[test]
    fn sysv_merges_a_mixed_eightbyte_to_integer() {
        // `{ f32, i32 }` is one eightbyte holding an SSE leaf and an INTEGER
        // leaf; the merge rule says INTEGER.
        let layout = lay(&strukt(&[F32, CgTy::Int(IntTy::I32)]));
        assert_eq!(
            classify_return(CAbi::SystemVAmd64, &layout),
            ReturnClass::Direct { registers: vec![RegClass::Integer] }
        );
        // `{ f32, f32 }` is one eightbyte of two SSE leaves: still SSE.
        let layout = lay(&strukt(&[F32, F32]));
        assert_eq!(
            classify_return(CAbi::SystemVAmd64, &layout),
            ReturnClass::Direct { registers: vec![RegClass::Sse] }
        );
    }

    #[test]
    fn aapcs64_hfa_takes_up_to_four_members_and_not_five() {
        // §4.1: "{ float × 4 } goes in v0–v3; { float × 5 } goes indirectly."
        let four = lay(&CgTy::array(F32, 4));
        assert_eq!(
            classify_return(CAbi::Aapcs64, &four),
            ReturnClass::Direct { registers: vec![RegClass::Sse; 4] }
        );
        let five = lay(&CgTy::array(F32, 5));
        assert_eq!(classify_return(CAbi::Aapcs64, &five), ReturnClass::Indirect);
    }

    #[test]
    fn aapcs64_nested_structs_flatten_into_the_hfa_count() {
        // "Nested structs flatten into the count." Two pairs of floats is an
        // HFA of four, not of two.
        let nested = lay(&strukt(&[strukt(&[F32, F32]), strukt(&[F32, F32])]));
        assert_eq!(
            classify_return(CAbi::Aapcs64, &nested),
            ReturnClass::Direct { registers: vec![RegClass::Sse; 4] }
        );
    }

    #[test]
    fn a_mixed_aggregate_is_not_an_hfa() {
        let mixed = lay(&strukt(&[F32, I64]));
        assert_eq!(
            classify_return(CAbi::Aapcs64, &mixed),
            ReturnClass::Direct { registers: vec![RegClass::Integer; 2] }
        );
        // Two different float types are not homogeneous either.
        let heterogeneous = lay(&strukt(&[F32, F64]));
        match classify_return(CAbi::Aapcs64, &heterogeneous) {
            ReturnClass::Direct { registers } => {
                assert_eq!(registers, vec![RegClass::Integer; 2]);
            }
            other => panic!("expected a direct return, got {other:?}"),
        }
    }

    #[test]
    fn win64_returns_one_float_member_in_xmm0_and_two_in_rax() {
        // The "single" in "a single float member" is load-bearing.
        assert_eq!(
            classify_return(CAbi::Win64, &lay(&strukt(&[F64]))),
            ReturnClass::Direct { registers: vec![RegClass::Sse] }
        );
        assert_eq!(
            classify_return(CAbi::Win64, &lay(&strukt(&[F32, F32]))),
            ReturnClass::Direct { registers: vec![RegClass::Integer] }
        );
    }

    #[test]
    fn win64_refuses_every_size_that_is_not_a_power_of_two_up_to_eight() {
        for size in [3u64, 5, 6, 7, 9, 12, 16, 24] {
            let layout = lay(&CgTy::array(CgTy::Int(IntTy::U8), size));
            assert_eq!(classify_return(CAbi::Win64, &layout), ReturnClass::Indirect, "{size}");
        }
    }

    #[test]
    fn a_zero_sized_return_is_nothing_on_every_target() {
        let layout = lay(&CgTy::Unit);
        for abi in [CAbi::SystemVAmd64, CAbi::Aapcs64, CAbi::Win64] {
            assert_eq!(classify_return(abi, &layout), ReturnClass::Void, "{abi:?}");
        }
    }

    #[test]
    fn every_aggregate_argument_is_a_pointer_and_every_scalar_is_not() {
        assert_eq!(classify_science_argument(&lay(&I64)), ArgClass::Direct);
        assert_eq!(classify_science_argument(&lay(&CgTy::Unit)), ArgClass::Ignore);
        assert_eq!(
            classify_science_argument(&lay(&strukt(&[F64, F64]))),
            ArgClass::IndirectByPointer
        );
        // The tax §4.2 names: `(Int, Error?)` is two words and still goes
        // through memory.
        assert_eq!(
            classify_science_argument(&lay(&strukt(&[I64, CgTy::nullable(CgTy::Ptr(PtrKind::Box))]))),
            ArgClass::IndirectByPointer
        );
    }

    #[test]
    fn a_niched_nullable_passes_directly_and_is_not_a_pointer_to_a_pointer() {
        let layout = lay(&CgTy::nullable(CgTy::Ptr(PtrKind::Box)));
        assert_eq!(classify_science_argument(&layout), ArgClass::Direct);
    }

    #[test]
    fn extern_refuses_an_aggregate_argument_and_accepts_a_scalar() {
        let ty = strukt(&[F64, F64]);
        assert!(classify_extern_argument(&ty, &lay(&ty)).is_err());
        assert_eq!(classify_extern_argument(&F64, &lay(&F64)), Ok(ArgClass::Direct));
        // `ffi.Span of T` lowers to a bare pointer, which passes.
        let span = CgTy::Ptr(PtrKind::Raw);
        assert_eq!(classify_extern_argument(&span, &lay(&span)), Ok(ArgClass::Direct));
    }

    #[test]
    fn extern_accepts_an_aggregate_return_although_it_refuses_the_argument() {
        // The asymmetry, asserted, because it looks like an inconsistency and
        // is a decision: the runtime forces the return and nothing forces the
        // argument.
        let ty = strukt(&[F64, F64, F64]);
        let layout = lay(&ty);
        assert!(classify_extern_argument(&ty, &layout).is_err());
        assert_eq!(
            classify_extern_return(&ty, &layout, CAbi::SystemVAmd64),
            Ok(ReturnClass::Indirect)
        );
    }

    #[test]
    fn borrow_attributes_follow_decision_24() {
        let shared = borrow_attrs(BorrowKind::Shared, 8, false);
        assert!(!shared.noalias, "a shared borrow must never be noalias: rule 4 permits two");
        assert!(shared.readonly);
        assert!(shared.nocapture);
        assert_eq!(shared.align, Some(8));

        let exclusive = borrow_attrs(BorrowKind::Exclusive, 8, false);
        assert!(exclusive.noalias);
        assert!(!exclusive.readonly);
        assert!(exclusive.nocapture);
    }

    #[test]
    fn no_noalias_suppresses_the_attribute_and_nothing_else() {
        let with = borrow_attrs(BorrowKind::Exclusive, 8, false);
        let without = borrow_attrs(BorrowKind::Exclusive, 8, true);
        assert!(with.noalias && !without.noalias);
        assert_eq!(ParamAttrs { noalias: false, ..with }, without);
        // The flag has no effect on a shared borrow, which never had it.
        assert_eq!(
            borrow_attrs(BorrowKind::Shared, 8, true),
            borrow_attrs(BorrowKind::Shared, 8, false)
        );
    }

    #[test]
    fn an_indirect_argument_is_not_readonly_because_it_is_a_move() {
        let attrs = indirect_argument_attrs(8);
        assert!(!attrs.readonly);
        assert!(!attrs.noalias);
        assert!(attrs.nocapture);
    }

    #[test]
    fn a_signature_does_not_carry_the_hidden_return_pointer_as_a_parameter() {
        let sig = AbiSignature::science(
            Triple::X86_64WindowsMsvc,
            "_S4main",
            lay(&strukt(&[F64, F64])),
            vec![("x".into(), lay(&I64), ParamAttrs::default())],
        );
        assert!(sig.ret.is_sret());
        assert_eq!(sig.params.len(), 1, "the sret slot is implied by `ret`, not listed");
        assert!(sig.nounwind);
    }
}
