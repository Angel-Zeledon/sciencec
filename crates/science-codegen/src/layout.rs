//! Data layout: §3 of `codegen-and-linking.md`, and the type model it runs on.
//!
//! **The decision.** Layout is computed here, above Decision 42's line, from
//! [`CgTy`] alone, and a backend receives a finished [`Layout`] rather than a
//! type to lay out.
//!
//! **The reason.** Decision 42 names the consequence of the alternative:
//! *"If layout and ABI are computed inside the LLVM backend, adding a second
//! backend means reimplementing them and hoping the two agree, which is §4's
//! silent-corruption failure mode with two more places for it to happen."*
//! Two backends must agree on struct offsets, discriminant positions and niche
//! encodings because an object file from one has to link against an object file
//! from the other. They need not agree on anything else.
//!
//! **The cost.** [`CgTy`] is a third type representation, after the HIR's and
//! whatever `science-types` settles on, and a lowering from MIR's types into it
//! is a pass that has to be written and kept honest. The alternative was to lay
//! out `science_types::Ty` directly, which welds the backend to the type
//! checker's representation and makes both harder to change; the note's own
//! §2.7 already insists that codegen key on the *normal form* rather than on
//! the written syntax, which is the same instinct.
//!
//! # The rules, and where each comes from
//!
//! - **Decision 17.** C layout: fields in declaration order, each at the next
//!   offset that is a multiple of its alignment, the whole rounded up to the
//!   maximum field alignment. No field reordering, ever.
//! - **Decision 18.** A `choice` is a discriminant followed by a union of the
//!   variant payloads; the discriminant is the smallest unsigned integer that
//!   holds the variant count, sits at offset 0, and numbers variants from zero
//!   in declaration order.
//! - **Decision 19.** The niche rule applies when exactly one variant carries a
//!   payload, every other variant carries none, and the payload has a niche.
//!   The enum is then the payload alone.
//! - **§3.1.** `Int` is `i64`; `Bool` is `i1` in registers and `i8` in memory;
//!   `Char` is a Unicode scalar value in a `u32`; a pointer is `ptr`.
//! - A zero-sized type has size 0 and alignment 1 and contributes nothing.
//!
//! # The one place this deliberately disagrees with a note
//!
//! `science-rt`'s §6 says of [`crate::descriptor::TypeInfo`]'s `size` field:
//! *"This is the stride as well as the size; Science, like Rust and unlike C++,
//! has no tail padding that an array may reuse."* So [`CgTy::Array`]'s size is
//! `elem.size * len` with `elem.size` already rounded up to `elem.align` — the
//! size *is* the stride, and there is no separate stride field anywhere in this
//! module. A C++ backend would need one. A note that assumed the C++ rule would
//! produce an `Array of T` whose elements overlap.

use std::collections::BTreeMap;

/// A target triple. F0 supports three, per §5.2, and cross-compilation is
/// `SC0406`.
///
/// The variants are spelled as triples rather than as `(arch, os)` pairs
/// because the triple is what goes in the `[build]` table of
/// `package-manager.md` Decision 3 and what a reviewer compares, and a pair
/// that renders to a triple is a second place for the rendering to drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Triple {
    /// `x86_64-unknown-linux-gnu`.
    X86_64LinuxGnu,
    /// `aarch64-apple-darwin`.
    Aarch64AppleDarwin,
    /// `x86_64-pc-windows-msvc`.
    X86_64WindowsMsvc,
}

impl Triple {
    /// Every triple F0 supports, in a fixed order.
    ///
    /// Fixed because tests iterate it and a test whose failure message depends
    /// on iteration order is a test nobody trusts.
    pub const ALL: [Triple; 3] =
        [Triple::X86_64LinuxGnu, Triple::Aarch64AppleDarwin, Triple::X86_64WindowsMsvc];

    /// The triple as it is written in a build record.
    pub fn as_str(self) -> &'static str {
        match self {
            Triple::X86_64LinuxGnu => "x86_64-unknown-linux-gnu",
            Triple::Aarch64AppleDarwin => "aarch64-apple-darwin",
            Triple::X86_64WindowsMsvc => "x86_64-pc-windows-msvc",
        }
    }

    /// Pointer width in bytes. Eight on every F0 target; the function exists so
    /// that [`IntTy::Usize`] has something to ask, and so that the first
    /// 32-bit target is a change in one place rather than a search.
    pub fn pointer_width(self) -> u64 {
        8
    }

    /// Which C calling convention classifies aggregates on this target (§4.1).
    pub fn c_abi(self) -> CAbi {
        match self {
            Triple::X86_64LinuxGnu => CAbi::SystemVAmd64,
            Triple::Aarch64AppleDarwin => CAbi::Aapcs64,
            Triple::X86_64WindowsMsvc => CAbi::Win64,
        }
    }

    /// The baseline CPU, per Decision 38. Never `native`.
    pub fn baseline_cpu(self) -> &'static str {
        match self {
            Triple::X86_64LinuxGnu | Triple::X86_64WindowsMsvc => "x86-64",
            Triple::Aarch64AppleDarwin => "apple-m1",
        }
    }

    /// The triple this process is running on, when it is one F0 supports.
    ///
    /// `None` is not an error here; it is the input to `SC0406`, which is what
    /// says cross-compilation is out of scope rather than failing obscurely in
    /// the linker.
    pub fn host() -> Option<Triple> {
        match (std::env::consts::ARCH, std::env::consts::OS) {
            ("x86_64", "linux") => Some(Triple::X86_64LinuxGnu),
            ("aarch64", "macos") => Some(Triple::Aarch64AppleDarwin),
            ("x86_64", "windows") => Some(Triple::X86_64WindowsMsvc),
            _ => None,
        }
    }
}

/// The three aggregate-passing conventions of §4.1.
///
/// Separate from [`Triple`] because the convention is the thing [`crate::abi`]
/// switches on, and two triples can share one — which is not true today and
/// will be the moment anyone adds `aarch64-unknown-linux-gnu`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CAbi {
    /// System V AMD64: eightbyte classification, 16-byte cut-off.
    SystemVAmd64,
    /// AAPCS64: the HFA rule, then a 16-byte cut-off.
    Aapcs64,
    /// Windows x64: exactly 1, 2, 4 or 8 bytes in a register, everything else
    /// by hidden reference.
    Win64,
}

/// An integer type.
///
/// [`IntTy::Usize`] and [`IntTy::Isize`] are separate variants from
/// [`IntTy::U64`] and [`IntTy::I64`] **on purpose**, and the purpose is §9.3's
/// finding 3. `science-rt` uses `usize` for every length and capacity and `i64`
/// for every `Int` that surfaces in a Science signature, and on all three F0
/// targets those are the same width — so a code generator that conflated them
/// would be correct by accident and wrong the first time anyone builds for a
/// 32-bit target. Keeping them distinct costs two enum variants and one match
/// arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(missing_docs)]
pub enum IntTy {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    /// The target's pointer width, unsigned. Not `U64`.
    Usize,
    /// The target's pointer width, signed. Not `I64`.
    Isize,
}

impl IntTy {
    /// Width in bytes on `target`.
    pub fn width(self, target: Triple) -> u64 {
        match self {
            IntTy::I8 | IntTy::U8 => 1,
            IntTy::I16 | IntTy::U16 => 2,
            IntTy::I32 | IntTy::U32 => 4,
            IntTy::I64 | IntTy::U64 => 8,
            IntTy::Usize | IntTy::Isize => target.pointer_width(),
        }
    }

    /// Whether the type is signed. Relevant to the backend's choice of `sext`
    /// or `zext` and to nothing in this module.
    pub fn is_signed(self) -> bool {
        matches!(self, IntTy::I8 | IntTy::I16 | IntTy::I32 | IntTy::I64 | IntTy::Isize)
    }

    /// The smallest unsigned integer that holds `count` distinct values, per
    /// Decision 18: `u8` up to 256 variants, `u16` beyond.
    pub fn discriminant_for(count: usize) -> IntTy {
        if count <= 256 {
            IntTy::U8
        } else if count <= 65536 {
            IntTy::U16
        } else {
            IntTy::U32
        }
    }
}

/// A floating-point type. F0 has two; `F16` and `BF16` across the `extern`
/// boundary are `SC0431` and are not in the model at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(missing_docs)]
pub enum FloatTy {
    F32,
    F64,
}

impl FloatTy {
    /// Width in bytes, which is also the alignment on every F0 target.
    pub fn width(self) -> u64 {
        match self {
            FloatTy::F32 => 4,
            FloatTy::F64 => 8,
        }
    }
}

/// What a pointer points at, as far as layout is concerned.
///
/// All five are one machine word and all five lower to LLVM 18's opaque `ptr`.
/// The distinction is not about representation; it is about **which ones have a
/// niche** (Decision 19's table) and which parameter attributes they carry
/// (Decision 24). [`PtrKind::Raw`] has neither, which is the entire reason it
/// is a separate variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PtrKind {
    /// `Box of T`. Never null, owning.
    Box,
    /// `borrowed T`. Never null, `readonly nocapture`, and **not** `noalias`.
    Borrow,
    /// `mutable borrowed T`. Never null, `noalias nocapture`.
    MutBorrow,
    /// A function pointer. Never null.
    Fn,
    /// A raw pointer from the `extern` boundary — `ffi.Span of T` lowers to
    /// one. **May be null**, so it has no niche, and a `(raw ptr)?` is tagged.
    Raw,
}

impl PtrKind {
    /// Whether a value of this kind has a bit pattern its type can never hold.
    ///
    /// The four never-null kinds do; [`PtrKind::Raw`] does not. Decision 19's
    /// table names exactly these four plus `any I`, and adds that the table is
    /// closed: *"a niche exists only where the type guarantees the bit pattern
    /// is unreachable"*.
    pub fn has_niche(self) -> bool {
        !matches!(self, PtrKind::Raw)
    }
}

/// One field of a record, in declaration order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    /// The field's name, for the layout record of Decision 31 and for
    /// diagnostics. Not part of the layout.
    pub name: String,
    /// The field's type.
    pub ty: CgTy,
}

impl Field {
    /// A field.
    pub fn new(name: impl Into<String>, ty: CgTy) -> Field {
        Field { name: name.into(), ty }
    }
}

/// One variant of a `choice`, in declaration order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    /// The variant's name.
    pub name: String,
    /// Its payload, or `None` for a payload-free variant. A payload of
    /// [`CgTy::Unit`] is *not* the same thing: it is a zero-sized payload, and
    /// by §5.1's zero-sized-payload rule it costs the discriminant alone, which
    /// is the same size but a different shape in the layout record.
    pub payload: Option<CgTy>,
}

impl Variant {
    /// A variant carrying nothing.
    pub fn unit(name: impl Into<String>) -> Variant {
        Variant { name: name.into(), payload: None }
    }

    /// A variant carrying a payload.
    pub fn with(name: impl Into<String>, payload: CgTy) -> Variant {
        Variant { name: name.into(), payload: Some(payload) }
    }
}

/// The type model layout and ABI classification are computed from.
///
/// This is deliberately *not* the HIR's `Ty` and not the type checker's. It is
/// the set of distinctions that change a layout or an ABI classification, and
/// nothing else: there are no generics here, because Decision 42 puts the
/// monomorphisation walk above this and monomorphised code has no type
/// parameters left; there are no lifetimes, because `region-inference.md` has
/// erased them by the time anything reaches a backend; and there are no traits,
/// only [`CgTy::Interface`], because a fat pointer is two words whatever
/// interface it points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CgTy {
    /// `()`, an empty record, or any other zero-sized type. Size 0, align 1.
    Unit,
    /// `Bool`. One byte in memory, `i1` in a register (§3.1).
    Bool,
    /// `Char`: a Unicode scalar value in a `u32`, which is what
    /// `science_chars_next` writes.
    Char,
    /// An integer.
    Int(IntTy),
    /// A float.
    Float(FloatTy),
    /// A pointer of some kind.
    Ptr(PtrKind),
    /// `any I`: a fat pointer, `{ data, vtable }`. The niche is in the **data**
    /// pointer and only the data pointer.
    Interface,
    /// A record type, laid out by Decision 17.
    Struct {
        /// The type's name, for the layout record and for mangling.
        name: String,
        /// Fields in declaration order. Order is load-bearing.
        fields: Vec<Field>,
    },
    /// A `choice`, laid out by Decision 18 or Decision 19.
    Choice {
        /// The type's name.
        name: String,
        /// Variants in declaration order. Order fixes the discriminant values.
        variants: Vec<Variant>,
    },
    /// `T?`.
    ///
    /// Sugar, and the desugaring is the point: [`layout_of`] expands it into a
    /// two-variant [`CgTy::Choice`] whose variant 0 is `null` and variant 1 is
    /// `present`, so that Decisions 18 and 19 apply to it unchanged rather than
    /// through a second code path. Those two numbers are
    /// `science_rt::SCIENCE_NULLABLE_NULL` and
    /// `SCIENCE_NULLABLE_PRESENT`, and `tests/layout.rs` checks that they still
    /// are.
    Nullable(Box<CgTy>),
    /// A fixed-length array. `size == elem.size * len`; see the module note on
    /// stride.
    Array {
        /// Element type.
        elem: Box<CgTy>,
        /// Element count.
        len: u64,
    },
}

impl CgTy {
    /// `T?`.
    pub fn nullable(inner: CgTy) -> CgTy {
        CgTy::Nullable(Box::new(inner))
    }

    /// A record.
    pub fn strukt(name: impl Into<String>, fields: Vec<Field>) -> CgTy {
        CgTy::Struct { name: name.into(), fields }
    }

    /// A `choice`.
    pub fn choice(name: impl Into<String>, variants: Vec<Variant>) -> CgTy {
        CgTy::Choice { name: name.into(), variants }
    }

    /// `[T; n]`.
    pub fn array(elem: CgTy, len: u64) -> CgTy {
        CgTy::Array { elem: Box::new(elem), len }
    }

    /// Whether this type has a bit pattern it can never hold, which is what
    /// Decision 19's niche rule needs.
    ///
    /// The list is closed and short: the four never-null pointer kinds and
    /// `any I`. `Bool` is deliberately **not** here — it is `i8` in memory with
    /// two valid values and 254 spare, which *is* a niche, and §3.4 declines it
    /// because exploiting it means a per-type valid-value set, which is the
    /// beginning of rustc's `Scalar::valid_range` machinery. The cost is one
    /// byte on `Bool?`, which the note prices as "a rare type".
    pub fn niche(&self) -> Option<Niche> {
        match self {
            CgTy::Ptr(kind) if kind.has_niche() => {
                Some(Niche { offset: 0, available: 1 })
            }
            // §3.4: "the vtable slot of a null trait object is undefined and
            // codegen must never load it — including on the path that tests for
            // null, which must test the data pointer and only the data
            // pointer." The niche is at offset 0 and is one word wide, and this
            // record is what stops a backend comparing the whole pair.
            CgTy::Interface => Some(Niche { offset: 0, available: 1 }),
            _ => None,
        }
    }
}

/// Where a niche is and how many spare values it offers.
///
/// `available` is 1 for every niche in F0 — a null pointer is one value — which
/// is why §3.4's generalisation (*"the `n` payload-free variants take the first
/// `n` values of the niche"*) has exactly one instance today. The field exists
/// so that the generalisation is expressible rather than assumed away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Niche {
    /// Byte offset of the niche within the payload.
    pub offset: u64,
    /// How many payload-free variants it can encode.
    pub available: u64,
}

/// A scalar, as the backend sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scalar {
    /// `i1` in a register, `i8` in memory.
    Bool,
    /// `u32` holding a Unicode scalar value.
    Char,
    /// An integer of this width.
    Int(IntTy),
    /// A float.
    Float(FloatTy),
    /// An opaque `ptr`, with its niche if it has one.
    Pointer(PtrKind),
}

/// Where one field sits, and what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldPlace {
    /// The field's name, carried for Decision 31's layout record: *"the layout
    /// computation returns a layout record carrying field offsets"*, which is
    /// what debug info needs and what a `--emit=layout` dump prints.
    pub name: String,
    /// Byte offset from the start of the aggregate.
    pub offset: u64,
    /// The field's own layout.
    pub layout: Layout,
}

/// Where one variant's payload sits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantPlace {
    /// The variant's name.
    pub name: String,
    /// Its discriminant value, from declaration order starting at zero.
    pub discriminant: u64,
    /// Its payload's layout, or `None` for a payload-free variant.
    pub payload: Option<Layout>,
}

/// How a type is represented in memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Repr {
    /// Size 0, align 1. Passed as nothing, stored as nothing.
    Zero,
    /// One scalar.
    Scalar(Scalar),
    /// Fields at fixed offsets in declaration order (Decision 17).
    Aggregate {
        /// Offsets, in declaration order.
        fields: Vec<FieldPlace>,
    },
    /// A discriminant followed by a union of payloads (Decision 18).
    Tagged {
        /// The discriminant's integer type. At offset 0, always.
        tag: IntTy,
        /// Where the payload union begins: the next offset that is a multiple
        /// of the union's alignment.
        payload_offset: u64,
        /// Variants in declaration order.
        variants: Vec<VariantPlace>,
    },
    /// The payload alone, with payload-free variants in its niche
    /// (Decision 19).
    Niched {
        /// The one variant that carries a payload.
        payload_variant: usize,
        /// Its layout, which is also the whole type's layout.
        payload: Box<Layout>,
        /// Where the niche is within the payload.
        niche: Niche,
        /// Payload-free variants in declaration order, each paired with the
        /// niche value that encodes it. The first is 0, which is the null
        /// pointer, which is also `SCIENCE_NULLABLE_NULL`.
        niche_variants: Vec<(usize, u64)>,
    },
}

/// A computed layout.
///
/// Decision 31 asks for *"a layout record carrying field offsets"*; this is it,
/// and it carries variant placement too, because debug info for a `choice`
/// needs the discriminant's position as much as a struct's needs its fields'.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// Size in bytes, rounded up to `align`. Also the stride.
    pub size: u64,
    /// Alignment in bytes. Always a power of two, always at least 1.
    pub align: u64,
    /// The representation.
    pub repr: Repr,
}

impl Layout {
    /// A scalar layout.
    fn scalar(size: u64, scalar: Scalar) -> Layout {
        Layout { size, align: size, repr: Repr::Scalar(scalar) }
    }

    /// Whether the type occupies no space, which §3.2 says is *"passed as
    /// nothing"*.
    pub fn is_zero_sized(&self) -> bool {
        self.size == 0
    }

    /// The offset of a field by index, for an aggregate.
    ///
    /// `None` when the layout is not an aggregate or the index is out of range,
    /// rather than a panic, because the caller that asks the wrong question is
    /// a bug in a backend and a backend should report it as `SC0402`-adjacent
    /// rather than abort the compiler mid-emission.
    pub fn field_offset(&self, index: usize) -> Option<u64> {
        match &self.repr {
            Repr::Aggregate { fields } => fields.get(index).map(|f| f.offset),
            _ => None,
        }
    }
}

/// Round `value` up to the next multiple of `align`.
fn align_to(value: u64, align: u64) -> u64 {
    debug_assert!(align.is_power_of_two(), "alignment {align} is not a power of two");
    (value + align - 1) & !(align - 1)
}

/// Compute the layout of `ty` on `target`.
///
/// Deterministic and total: every [`CgTy`] has a layout, there is no failure
/// case, and the answer depends on nothing but the arguments. That is what lets
/// `tests/layout.rs` compare it against `size_of` for the runtime's types and
/// what lets a second backend be held to the same numbers.
pub fn layout_of(target: Triple, ty: &CgTy) -> Layout {
    match ty {
        CgTy::Unit => Layout { size: 0, align: 1, repr: Repr::Zero },
        CgTy::Bool => Layout::scalar(1, Scalar::Bool),
        CgTy::Char => Layout::scalar(4, Scalar::Char),
        CgTy::Int(int) => Layout::scalar(int.width(target), Scalar::Int(*int)),
        CgTy::Float(float) => Layout::scalar(float.width(), Scalar::Float(*float)),
        CgTy::Ptr(kind) => Layout::scalar(target.pointer_width(), Scalar::Pointer(*kind)),

        // `any I` is two words and is an ordinary aggregate for every purpose
        // except the niche, which §3.4 puts in the data pointer alone.
        CgTy::Interface => layout_of(
            target,
            &CgTy::strukt(
                "any",
                vec![
                    Field::new("data", CgTy::Ptr(PtrKind::Raw)),
                    Field::new("vtable", CgTy::Ptr(PtrKind::Raw)),
                ],
            ),
        ),

        CgTy::Struct { fields, .. } => layout_struct(target, fields),

        CgTy::Array { elem, len } => {
            let elem_layout = layout_of(target, elem);
            // `size` is the stride: see the module note. A zero-sized element
            // gives a zero-sized array however long it is, which is what lets
            // `science_array_with_capacity` treat `size == 0` as unbounded
            // capacity without allocating.
            Layout {
                size: elem_layout.size * len,
                align: elem_layout.align,
                repr: Repr::Aggregate {
                    fields: (0..*len)
                        .map(|i| FieldPlace {
                            name: i.to_string(),
                            offset: elem_layout.size * i,
                            layout: elem_layout.clone(),
                        })
                        .collect(),
                },
            }
        }

        // The desugaring, and the reason `T?` has no code path of its own.
        // Variant 0 is `null` and variant 1 is `present`, which are
        // `SCIENCE_NULLABLE_NULL` and `SCIENCE_NULLABLE_PRESENT`, in that
        // order, for the reason `abi.rs` gives at length: the absent case is
        // then the all-zero byte pattern in *both* representations.
        CgTy::Nullable(inner) => layout_choice(
            target,
            &[Variant::unit("null"), Variant::with("present", (**inner).clone())],
        ),

        CgTy::Choice { variants, .. } => layout_choice(target, variants),
    }
}

/// Decision 17: C layout, declaration order, no reordering.
fn layout_struct(target: Triple, fields: &[Field]) -> Layout {
    let mut offset = 0u64;
    let mut align = 1u64;
    let mut places = Vec::with_capacity(fields.len());

    for field in fields {
        let layout = layout_of(target, &field.ty);
        offset = align_to(offset, layout.align);
        align = align.max(layout.align);
        places.push(FieldPlace { name: field.name.clone(), offset, layout: layout.clone() });
        offset += layout.size;
    }

    Layout { size: align_to(offset, align), align, repr: Repr::Aggregate { fields: places } }
}

/// Decisions 18 and 19: the niche rule if it applies, the tagged rule if not.
fn layout_choice(target: Triple, variants: &[Variant]) -> Layout {
    if let Some(layout) = layout_niched(target, variants) {
        return layout;
    }
    layout_tagged(target, variants)
}

/// Decision 19, or `None` when its three conditions are not all met.
///
/// The conditions, restated so that the code below is checkable against them:
/// exactly one variant carries a payload; every other variant carries none; and
/// the payload's type has a niche with room for the payload-free variants.
fn layout_niched(target: Triple, variants: &[Variant]) -> Option<Layout> {
    let mut payload_variant = None;
    for (index, variant) in variants.iter().enumerate() {
        if variant.payload.is_some() {
            if payload_variant.is_some() {
                // More than one payload: Decision 18's general rule.
                return None;
            }
            payload_variant = Some(index);
        }
    }
    let payload_variant = payload_variant?;
    let payload_ty = variants[payload_variant].payload.as_ref()?;
    let niche = payload_ty.niche()?;

    // `n` payload-free variants need `n` niche values. In F0 a niche offers
    // exactly one, so this rejects any three-variant shape and is not dead
    // code waiting for a language change: `choice { A, B, Ptr(p) }` is legal
    // Science today and must fall through to the tagged rule.
    let free: Vec<usize> = (0..variants.len()).filter(|i| *i != payload_variant).collect();
    if free.len() as u64 > niche.available {
        return None;
    }

    let payload = layout_of(target, payload_ty);
    Some(Layout {
        size: payload.size,
        align: payload.align,
        repr: Repr::Niched {
            payload_variant,
            payload: Box::new(payload),
            niche,
            // The first payload-free variant takes niche value 0, which for a
            // pointer is the null pointer. With one payload-free variant, which
            // is every case in F0, `null` is the null pointer and nothing else
            // has to be said.
            niche_variants: free.into_iter().enumerate().map(|(n, v)| (v, n as u64)).collect(),
        },
    })
}

/// Decision 18: discriminant at offset 0, union of payloads after it.
fn layout_tagged(target: Triple, variants: &[Variant]) -> Layout {
    let tag = IntTy::discriminant_for(variants.len());
    let tag_size = tag.width(target);

    let mut payload_align = 1u64;
    let mut payload_size = 0u64;
    let mut payloads = Vec::with_capacity(variants.len());
    for variant in variants {
        let layout = variant.payload.as_ref().map(|ty| layout_of(target, ty));
        if let Some(layout) = &layout {
            payload_align = payload_align.max(layout.align);
            payload_size = payload_size.max(layout.size);
        }
        payloads.push(layout);
    }

    let payload_offset = align_to(tag_size, payload_align);
    let align = tag_size.max(payload_align);
    let size = align_to(payload_offset + payload_size, align);

    Layout {
        size,
        align,
        repr: Repr::Tagged {
            tag,
            payload_offset,
            variants: variants
                .iter()
                .zip(payloads)
                .enumerate()
                .map(|(index, (variant, payload))| VariantPlace {
                    name: variant.name.clone(),
                    discriminant: index as u64,
                    payload,
                })
                .collect(),
        },
    }
}

/// Every scalar leaf of a layout, with its absolute byte offset.
///
/// The ABI classifiers of [`crate::abi`] all need this and none of them needs
/// anything else about the shape: System V asks which eightbyte each leaf falls
/// in, AAPCS64 asks whether every leaf is the same float type, and Windows x64
/// asks whether there is exactly one leaf and whether it is a float. Computing
/// it once here keeps three classifiers from walking three slightly different
/// trees, which is how the three-members-versus-two bugs of §4.1 happen.
///
/// A [`Repr::Tagged`] enum flattens to its discriminant plus the leaves of
/// **every** variant, because the ABI question is about the bytes the value
/// occupies and a union occupies all of them. That is deliberately conservative:
/// a union of a float and an integer classifies as INTEGER on System V, which is
/// what the psABI says for a union whose eightbytes merge.
pub fn scalar_leaves(layout: &Layout) -> Vec<(u64, Scalar)> {
    let mut out = Vec::new();
    collect_leaves(layout, 0, &mut out);
    out.sort_by_key(|(offset, _)| *offset);
    out
}

fn collect_leaves(layout: &Layout, base: u64, out: &mut Vec<(u64, Scalar)>) {
    match &layout.repr {
        Repr::Zero => {}
        Repr::Scalar(scalar) => out.push((base, *scalar)),
        Repr::Aggregate { fields } => {
            for field in fields {
                collect_leaves(&field.layout, base + field.offset, out);
            }
        }
        Repr::Tagged { tag, payload_offset, variants } => {
            out.push((base, Scalar::Int(*tag)));
            for variant in variants {
                if let Some(payload) = &variant.payload {
                    collect_leaves(payload, base + payload_offset, out);
                }
            }
        }
        Repr::Niched { payload, .. } => collect_leaves(payload, base, out),
    }
}

/// A record of every layout computed in one compilation, keyed by type name.
///
/// Decision 4 emits items *"in sorted order of its mangled symbol name"* to
/// close Gate J's hash-map-iteration-order hazard by construction. A layout
/// cache keyed by a `HashMap` would reopen it the moment anything iterated the
/// cache, so this one is a `BTreeMap` and its iteration order is the answer.
#[derive(Debug, Clone, Default)]
pub struct LayoutCache {
    entries: BTreeMap<String, Layout>,
}

impl LayoutCache {
    /// An empty cache.
    pub fn new() -> LayoutCache {
        LayoutCache::default()
    }

    /// Record a named type's layout, returning the previous entry if the name
    /// was already present with a *different* layout.
    ///
    /// Two identical layouts for one name are fine — that is the same type laid
    /// out twice. Two different ones are a bug in whatever produced the names,
    /// and the caller turns it into `SC0404`.
    pub fn insert(&mut self, name: impl Into<String>, layout: Layout) -> Option<Layout> {
        let name = name.into();
        match self.entries.get(&name) {
            Some(existing) if *existing == layout => None,
            Some(existing) => {
                let previous = existing.clone();
                self.entries.insert(name, layout);
                Some(previous)
            }
            None => {
                self.entries.insert(name, layout);
                None
            }
        }
    }

    /// Look a layout up.
    pub fn get(&self, name: &str) -> Option<&Layout> {
        self.entries.get(name)
    }

    /// Every entry, in sorted order of name.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Layout)> {
        self.entries.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Triple = Triple::X86_64LinuxGnu;

    fn size_align(ty: &CgTy) -> (u64, u64) {
        let layout = layout_of(T, ty);
        (layout.size, layout.align)
    }

    #[test]
    fn scalars_are_their_width() {
        assert_eq!(size_align(&CgTy::Unit), (0, 1));
        assert_eq!(size_align(&CgTy::Bool), (1, 1));
        assert_eq!(size_align(&CgTy::Char), (4, 4));
        assert_eq!(size_align(&CgTy::Int(IntTy::I64)), (8, 8));
        assert_eq!(size_align(&CgTy::Float(FloatTy::F32)), (4, 4));
        assert_eq!(size_align(&CgTy::Ptr(PtrKind::Box)), (8, 8));
        assert_eq!(size_align(&CgTy::Interface), (16, 8));
    }

    #[test]
    fn usize_is_not_i64_even_where_it_is_the_same_width() {
        // Finding 3, as a test rather than a sentence. Same width, different
        // type; the day a 32-bit target exists, only the first line changes.
        assert_eq!(IntTy::Usize.width(T), IntTy::I64.width(T));
        assert_ne!(IntTy::Usize, IntTy::U64);
        assert!(!IntTy::Usize.is_signed());
        assert!(IntTy::Isize.is_signed());
    }

    #[test]
    fn a_struct_is_c_layout_and_is_never_reordered() {
        // §3.2's own example: `{ u8, u64, u8 }` is 24 bytes under the C rule
        // and 16 under Rust's. Science pays the 8 bytes.
        let ty = CgTy::strukt(
            "Mixed",
            vec![
                Field::new("a", CgTy::Int(IntTy::U8)),
                Field::new("b", CgTy::Int(IntTy::U64)),
                Field::new("c", CgTy::Int(IntTy::U8)),
            ],
        );
        let layout = layout_of(T, &ty);
        assert_eq!((layout.size, layout.align), (24, 8));
        assert_eq!(layout.field_offset(0), Some(0));
        assert_eq!(layout.field_offset(1), Some(8));
        assert_eq!(layout.field_offset(2), Some(16));
    }

    #[test]
    fn a_zero_sized_field_contributes_nothing() {
        let ty = CgTy::strukt(
            "Padded",
            vec![
                Field::new("nothing", CgTy::Unit),
                Field::new("n", CgTy::Int(IntTy::I32)),
                Field::new("also_nothing", CgTy::Unit),
            ],
        );
        let layout = layout_of(T, &ty);
        assert_eq!((layout.size, layout.align), (4, 4));
        assert_eq!(layout.field_offset(0), Some(0));
        assert_eq!(layout.field_offset(1), Some(0));
        assert_eq!(layout.field_offset(2), Some(4));
    }

    #[test]
    fn a_pointer_nullable_is_the_pointer_and_costs_not_one_bit_more() {
        for kind in [PtrKind::Box, PtrKind::Borrow, PtrKind::MutBorrow, PtrKind::Fn] {
            let ty = CgTy::nullable(CgTy::Ptr(kind));
            let layout = layout_of(T, &ty);
            assert_eq!((layout.size, layout.align), (8, 8), "{kind:?}");
            match layout.repr {
                Repr::Niched { payload_variant, ref niche_variants, .. } => {
                    // `present` is variant 1 and `null` is variant 0, which are
                    // SCIENCE_NULLABLE_PRESENT and SCIENCE_NULLABLE_NULL.
                    assert_eq!(payload_variant, 1);
                    assert_eq!(niche_variants, &[(0usize, 0u64)]);
                }
                other => panic!("expected a niched layout, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_raw_pointer_has_no_niche_because_it_may_be_null() {
        let layout = layout_of(T, &CgTy::nullable(CgTy::Ptr(PtrKind::Raw)));
        assert!(matches!(layout.repr, Repr::Tagged { .. }));
        assert_eq!((layout.size, layout.align), (16, 8));
    }

    #[test]
    fn an_interface_nullable_is_two_words_with_the_null_in_the_data_pointer() {
        let layout = layout_of(T, &CgTy::nullable(CgTy::Interface));
        assert_eq!((layout.size, layout.align), (16, 8));
        match layout.repr {
            Repr::Niched { niche, .. } => assert_eq!(niche.offset, 0),
            other => panic!("expected a niched layout, got {other:?}"),
        }
    }

    #[test]
    fn where_niches_stop() {
        // §3.4's list, checked. `Int?` is two words; `Bool?` is two bytes.
        assert_eq!(size_align(&CgTy::nullable(CgTy::Int(IntTy::I64))), (16, 8));
        assert_eq!(size_align(&CgTy::nullable(CgTy::Bool)), (2, 1));
    }

    #[test]
    fn a_choice_with_two_payloads_is_tagged_even_when_both_have_niches() {
        let ty = CgTy::choice(
            "Either",
            vec![
                Variant::with("left", CgTy::Ptr(PtrKind::Box)),
                Variant::with("right", CgTy::Ptr(PtrKind::Box)),
            ],
        );
        let layout = layout_of(T, &ty);
        assert_eq!((layout.size, layout.align), (16, 8));
        match layout.repr {
            Repr::Tagged { tag, payload_offset, ref variants } => {
                assert_eq!(tag, IntTy::U8);
                assert_eq!(payload_offset, 8);
                assert_eq!(variants[0].discriminant, 0);
                assert_eq!(variants[1].discriminant, 1);
            }
            other => panic!("expected a tagged layout, got {other:?}"),
        }
    }

    #[test]
    fn three_variants_with_one_payload_do_not_fit_one_niche() {
        // One niche value, two payload-free variants: the niche rule must
        // decline rather than encode two variants in one bit pattern.
        let ty = CgTy::choice(
            "Tri",
            vec![
                Variant::unit("a"),
                Variant::unit("b"),
                Variant::with("p", CgTy::Ptr(PtrKind::Box)),
            ],
        );
        assert!(matches!(layout_of(T, &ty).repr, Repr::Tagged { .. }));
    }

    #[test]
    fn an_enum_of_payload_free_variants_is_the_discriminant_alone() {
        let ty = CgTy::choice(
            "IoError",
            vec![
                Variant::unit("not_found"),
                Variant::unit("permission_denied"),
                Variant::unit("already_exists"),
                Variant::unit("invalid_data"),
                Variant::unit("other"),
            ],
        );
        assert_eq!(size_align(&ty), (1, 1));
    }

    #[test]
    fn a_zero_sized_payload_costs_the_discriminant_alone() {
        let ty = CgTy::choice(
            "Flag",
            vec![Variant::unit("off"), Variant::with("on", CgTy::Unit)],
        );
        assert_eq!(size_align(&ty), (1, 1));
    }

    #[test]
    fn a_discriminant_widens_past_256_variants() {
        assert_eq!(IntTy::discriminant_for(2), IntTy::U8);
        assert_eq!(IntTy::discriminant_for(256), IntTy::U8);
        assert_eq!(IntTy::discriminant_for(257), IntTy::U16);
        assert_eq!(IntTy::discriminant_for(70_000), IntTy::U32);
    }

    #[test]
    fn an_array_size_is_the_stride_times_the_length() {
        let ty = CgTy::array(CgTy::Float(FloatTy::F64), 4);
        assert_eq!(size_align(&ty), (32, 8));
        // A padded element keeps its padding in the array: stride == size.
        let padded = CgTy::strukt(
            "P",
            vec![
                Field::new("a", CgTy::Int(IntTy::U8)),
                Field::new("b", CgTy::Int(IntTy::U32)),
            ],
        );
        assert_eq!(size_align(&padded), (8, 4));
        assert_eq!(size_align(&CgTy::array(padded, 3)), (24, 4));
    }

    #[test]
    fn an_array_of_a_zero_sized_type_is_zero_sized() {
        assert_eq!(size_align(&CgTy::array(CgTy::Unit, 1_000)), (0, 1));
    }

    #[test]
    fn scalar_leaves_are_in_offset_order() {
        let ty = CgTy::strukt(
            "Pair",
            vec![
                Field::new("a", CgTy::Float(FloatTy::F64)),
                Field::new("b", CgTy::Int(IntTy::I64)),
            ],
        );
        let leaves = scalar_leaves(&layout_of(T, &ty));
        assert_eq!(leaves.len(), 2);
        assert_eq!(leaves[0].0, 0);
        assert!(matches!(leaves[0].1, Scalar::Float(FloatTy::F64)));
        assert_eq!(leaves[1].0, 8);
        assert!(matches!(leaves[1].1, Scalar::Int(IntTy::I64)));
    }

    #[test]
    fn the_layout_cache_reports_a_genuine_disagreement_only() {
        let mut cache = LayoutCache::new();
        let a = layout_of(T, &CgTy::Int(IntTy::I64));
        let b = layout_of(T, &CgTy::Int(IntTy::I32));
        assert!(cache.insert("X", a.clone()).is_none());
        assert!(cache.insert("X", a).is_none(), "the same layout twice is not a collision");
        assert!(cache.insert("X", b).is_some());
    }
}
