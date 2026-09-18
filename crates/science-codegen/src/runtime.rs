//! The runtime boundary: §2.6, and the place §9's findings become facts a test
//! checks.
//!
//! **The decision.** The 55 `science_`-prefixed entry points are described here
//! as data — a signature per symbol — and never as prose. Everything a code
//! generator needs to emit a call is read out of [`RUNTIME`]: the return
//! convention, the descriptor's parameter position, and whether a length is a
//! `usize` or an `Int`.
//!
//! **The reason.** §9.2's finding 1 was that `science-rt`'s §2 kept a
//! hand-maintained list of the entry points returning an aggregate, and that
//! `science_array_with_capacity` had fallen off it. A code generator emitted
//! from that list emits a structural return for a three-word struct, the runtime
//! writes through a return slot pointer the caller never passed, and the program
//! corrupts whatever was in that register. §9.4's verdict is that the list *"is
//! now checked by a test rather than maintained by hand"*; this module is the
//! codegen side of that, and it does not keep a list at all. It keeps
//! signatures and derives the list.
//!
//! **The cost.** Fifty-five signatures transcribed by hand, which is a
//! transcription that can be wrong in exactly the way the thing it replaces was
//! wrong. Two mitigations: `tests/runtime_abi.rs` asserts the count is 55, that
//! every symbol is `science_`-prefixed and unique, and that the derived `sret`
//! set matches the eight the runtime's §2 now names; and the runtime crate is a
//! dev-dependency, so a test can compare the *layouts* against the real Rust
//! types rather than against a second transcription.
//!
//! # Finding 1 recurs, on the symbol hello world calls first
//!
//! Deriving the `sret` set rather than reading it produces **nine** entry
//! points. `science-rt` §2, after the repair §9.2 prompted, names **eight**.
//! The ninth is `science_string_from_bytes`:
//!
//! ```text
//! pub unsafe extern "C" fn science_string_from_bytes(ptr: *const u8, len: usize)
//!     -> ScienceString
//! ```
//!
//! Three words, MEMORY on all three targets, returned by value, and absent from
//! §2's list for exactly the reason `science_array_with_capacity` was absent:
//! the page mentions it in **§8** — *"literal construction
//! (`science_string_from_bytes`)"* — and not in §2, and §2 is the section that
//! specifies the return convention. §9.2's own words apply unchanged: *"The page
//! mentions the function elsewhere, so cross-referencing two paragraphs on the
//! same page would catch it. Reading §2 as the specification of the return
//! convention, which is what §2 is, would not."*
//!
//! **This one is worse than the original, in two ways.**
//!
//! The first is when it fires. `science_array_with_capacity` is a capacity
//! hint that a program reaches at stage 4. `science_string_from_bytes` is the
//! call a **string literal** lowers to under Decision 15, so it is the first
//! runtime call `print("hello, world")` makes — the whole of §10's stage 1 is
//! two calls and this is one of them. A code generator emitted from §2 as the
//! authority corrupts a register on the first Science program ever compiled.
//!
//! The second is that the note already knew. §10's stage 1 says the `sret`
//! convention is needed *"immediately, because `science_string_from_bytes`
//! returns `ScienceString` by value"*. So the fact is written down in
//! `codegen-and-linking.md` §10 and missing from the list in `science-rt` §2
//! that a code generator is invited to be emitted from, which is the precise
//! configuration §9.4 warns about: *"the page is good enough that reading it
//! carefully is productive, and not yet good enough that reading it carelessly
//! is safe."*
//!
//! **What this crate does about it:** nothing to `science-rt`, which is out of
//! its scope to edit. [`RUNTIME`] carries the signature, the set is derived, and
//! `tests/runtime_abi.rs` fails if anyone makes it eight again.
//!
//! # Decision 14: these 55 are the only runtime calls F0 emits
//!
//! > *Everything else is inline. No entry point is added to `science-rt` to make
//! > codegen simpler; the runtime page's §9 already states the principle —
//! > "putting them here would cost a function call for something codegen emits
//! > as a handful of instructions."*
//!
//! So a presence test, a null-coalescing default, `and`/`or`, `not`, `match`,
//! field projection, indexing, closures and vtables are all inline, and this
//! module has no entry for any of them. The one that looks inline and is not is
//! the string literal: see [`crate::descriptor::StringLiteral`].

use crate::abi::{ReturnClass, classify_return};
use crate::layout::{CAbi, CgTy, Field, IntTy, Layout, PtrKind, Triple, Variant, layout_of};

/// The eight `#[repr(C)]` aggregates `science-rt` defines.
///
/// Modelled here as [`CgTy`] so that the layout engine and the ABI classifier
/// can be run over them. `tests/layout.rs` checks each against `size_of` and
/// `align_of` of the real Rust type, which is the only differential test
/// available without a C compiler and is worth more than both classifiers'
/// unit tests put together: it compares this crate's rules against a fixed ABI
/// nobody here controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RtAggregate {
    /// `{ ptr, len, cap }` — three words.
    String,
    /// `{ ptr, len, offset }` — three words.
    Chars,
    /// `{ ptr, len, cap }` — three words.
    Array,
    /// `{ states, keys, values, len, tombstones, cap }` — six words.
    Map,
    /// `{ size, align, drop_fn }` — three words.
    TypeInfo,
    /// `{ key, value, hash_fn, eq_fn }` — eight words.
    MapInfo,
    /// `u8` — one byte, the five payload-free variants of `science-rt`'s §8.1.
    ///
    /// **Decision: `IoError` gets a row of its own, even though no `RUNTIME`
    /// signature mentions it.** Every other row here is the type of an entry
    /// point's argument or return; this one is the type of a *local*. §8 gives
    /// `read_file` the return `(String, IoError?)` and `write_file` the return
    /// `IoError?`, so a program that calls either binds a name of type
    /// `IoError`, and a backend lowering that binding needs a [`CgTy`] for the
    /// bare type and not only for the nullable wrapper.
    ///
    /// **The reason it is here and not written out in the backend** is §9.2's
    /// finding, the one this module exists for: two phases that each spell a
    /// runtime type out separately agree until one of them is edited. The
    /// variant list fixes the discriminant values — `NOT_FOUND` is 0 and
    /// `OTHER` is 4, and `science-rt`'s `tests/layout.rs` pins exactly those
    /// numbers — so a second copy of that list in a second crate is a second
    /// place for the numbering to drift.
    ///
    /// **The cost is a row that `RUNTIME`'s signatures never name**, which
    /// makes this the first entry in [`RtAggregate::ALL`] that is not reachable
    /// from a symbol. That is paid for by [`RtAggregate::NullableIoError`]
    /// being defined in terms of it below, so the two cannot disagree.
    IoError,
    /// `{ present: u8, error: u8 }` — two bytes, and §2's named exception:
    /// *"at two bytes it comes back in a register on both conventions"*.
    NullableIoError,
    /// `{ value: ScienceString, error: ScienceNullableIoError }` — the pair of
    /// §5.4.
    StringAndIoError,
}

impl RtAggregate {
    /// All nine, in a fixed order.
    pub const ALL: [RtAggregate; 9] = [
        RtAggregate::String,
        RtAggregate::Chars,
        RtAggregate::Array,
        RtAggregate::Map,
        RtAggregate::TypeInfo,
        RtAggregate::MapInfo,
        RtAggregate::IoError,
        RtAggregate::NullableIoError,
        RtAggregate::StringAndIoError,
    ];

    /// The C name, as it appears in `science-rt`.
    pub fn name(self) -> &'static str {
        match self {
            RtAggregate::String => "ScienceString",
            RtAggregate::Chars => "ScienceChars",
            RtAggregate::Array => "ScienceArray",
            RtAggregate::Map => "ScienceMap",
            RtAggregate::TypeInfo => "ScienceTypeInfo",
            RtAggregate::MapInfo => "ScienceMapInfo",
            RtAggregate::IoError => "ScienceIoError",
            RtAggregate::NullableIoError => "ScienceNullableIoError",
            RtAggregate::StringAndIoError => "ScienceStringAndIoError",
        }
    }

    /// The type, for layout and ABI classification.
    ///
    /// Note the `usize` in every length and capacity, and note that
    /// `science_array_len` *returns* `i64`. That is §9.3's finding 3 — §4 of
    /// the runtime page says `usize` never surfaces in a Science signature and
    /// it surfaces in eight places — written out rather than smoothed over. The
    /// rule as written is not "usize does not surface"; it is "usize and i64
    /// are the same width on every target F0 supports", which is a different
    /// and much weaker statement.
    pub fn cg_ty(self) -> CgTy {
        let usize_ty = CgTy::Int(IntTy::Usize);
        match self {
            RtAggregate::String => CgTy::strukt(
                "ScienceString",
                vec![
                    Field::new("ptr", CgTy::Ptr(PtrKind::Raw)),
                    Field::new("len", usize_ty.clone()),
                    Field::new("cap", usize_ty),
                ],
            ),
            RtAggregate::Chars => CgTy::strukt(
                "ScienceChars",
                vec![
                    Field::new("ptr", CgTy::Ptr(PtrKind::Raw)),
                    Field::new("len", usize_ty.clone()),
                    Field::new("offset", usize_ty),
                ],
            ),
            RtAggregate::Array => CgTy::strukt(
                "ScienceArray",
                vec![
                    Field::new("ptr", CgTy::Ptr(PtrKind::Raw)),
                    Field::new("len", usize_ty.clone()),
                    Field::new("cap", usize_ty),
                ],
            ),
            RtAggregate::Map => CgTy::strukt(
                "ScienceMap",
                vec![
                    Field::new("states", CgTy::Ptr(PtrKind::Raw)),
                    Field::new("keys", CgTy::Ptr(PtrKind::Raw)),
                    Field::new("values", CgTy::Ptr(PtrKind::Raw)),
                    Field::new("len", usize_ty.clone()),
                    Field::new("tombstones", usize_ty.clone()),
                    Field::new("cap", usize_ty),
                ],
            ),
            RtAggregate::TypeInfo => CgTy::strukt(
                "ScienceTypeInfo",
                vec![
                    Field::new("size", usize_ty.clone()),
                    Field::new("align", usize_ty),
                    // `Option<ScienceDropFn>` in Rust: a nullable function
                    // pointer, which is the niche rule of §5.2 at work and is
                    // why it is one word rather than two.
                    Field::new("drop_fn", CgTy::nullable(CgTy::Ptr(PtrKind::Fn))),
                ],
            ),
            RtAggregate::MapInfo => CgTy::strukt(
                "ScienceMapInfo",
                vec![
                    Field::new("key", RtAggregate::TypeInfo.cg_ty()),
                    Field::new("value", RtAggregate::TypeInfo.cg_ty()),
                    Field::new("hash_fn", CgTy::Ptr(PtrKind::Fn)),
                    Field::new("eq_fn", CgTy::Ptr(PtrKind::Fn)),
                ],
            ),
            // Modelled as the `choice` it is, not as the struct it looks like,
            // so that the layout engine's Decision 18 produces the two bytes
            // rather than a hand-written pair reproducing them. The runtime's
            // §8.1 explains why this takes the discriminant byte and not the
            // niche `IoError`'s 251 unused code points appear to offer: a niche
            // exists only where the *type* guarantees the bit pattern is
            // unreachable, and an integer newtype with five named constants
            // guarantees nothing of the sort, because the next version has a
            // sixth.
            RtAggregate::IoError => CgTy::choice(
                "ScienceIoError",
                vec![
                    Variant::unit("not_found"),
                    Variant::unit("permission_denied"),
                    Variant::unit("already_exists"),
                    Variant::unit("invalid_data"),
                    Variant::unit("other"),
                ],
            ),
            RtAggregate::NullableIoError => CgTy::nullable(RtAggregate::IoError.cg_ty()),
            // §5.4: a pair is *a plain struct, and both fields are live at
            // once*. Not an enum, no tag, nothing for a niche to disambiguate.
            RtAggregate::StringAndIoError => CgTy::strukt(
                "ScienceStringAndIoError",
                vec![
                    Field::new("value", RtAggregate::String.cg_ty()),
                    Field::new("error", RtAggregate::NullableIoError.cg_ty()),
                ],
            ),
        }
    }

    /// The aggregate's layout on `target`.
    pub fn layout(self, target: Triple) -> Layout {
        layout_of(target, &self.cg_ty())
    }
}

/// What a runtime entry point returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtRet {
    /// Nothing.
    Void,
    /// `-> !`: diverges. `science_abort`, `science_panic`,
    /// `science_panic_bytes`. Decision 6 makes the call site `call` followed by
    /// `unreachable`, never `invoke`.
    Never,
    /// `bool`.
    Bool,
    /// `i32`.
    I32,
    /// `i64` — Science's `Int`.
    Int,
    /// `u64`.
    U64,
    /// A raw pointer. For `science_array_get` and friends this pointer **is**
    /// the `(borrowed T)?`, in the niche representation, and codegen uses it
    /// directly with no conversion at all (§5.3).
    Ptr,
    /// One of the eight aggregates, by value.
    Aggregate(RtAggregate),
}

/// What a runtime entry point takes.
///
/// The variants that matter are [`RtParam::Descriptor`] — because §9.3's
/// finding 4 is that its position is unstated and inconsistent — and the
/// [`RtParam::Usize`]/[`RtParam::Int`] pair, which is finding 3.
///
/// **This enum was smaller than [`RtRet`] and the gap is what made a whole
/// class of entry point unrepresentable.** `RtRet` has had `Bool` and `U64`
/// since it was written; this had neither, and no `F64`, `F32` or `Char`
/// either — so until `format.rs` arrived there was **no way to write down a
/// runtime function that takes a number**, and the fact that there was no way
/// was invisible because there was no such function. Every entry point in the
/// original 47 takes a pointer, a length or an exit status, and a table whose
/// vocabulary is exactly its contents cannot be read as a specification of what
/// the boundary *could* carry. The asymmetry is worth naming because it is the
/// shape of a gap nothing tests: a missing variant in a closed enum is a
/// refusal that never fires, in a table that looks complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtParam {
    /// `*const ScienceTypeInfo` or `*const ScienceMapInfo`: the element
    /// descriptor of §6.
    Descriptor,
    /// A pointer to a runtime aggregate, or to a value slot.
    Pointer,
    /// `usize`: the target's pointer width. **Not** `i64`.
    Usize,
    /// `i64`: Science's `Int`.
    Int,
    /// `i32`: a process exit status, and nothing else.
    ///
    /// **One entry point takes this and the variant exists for it**:
    /// `science_exit(code: i32)`. C's `exit` takes an `int`, so a 64-bit
    /// status would be a different signature from the one the platform has —
    /// and unlike [`RtParam::Usize`] against [`RtParam::Int`], where finding 3
    /// is that the two are the same width on every F0 target and the
    /// distinction is bookkeeping, this one is a real width difference that a
    /// call site gets wrong in a register.
    ///
    /// It is deliberately not spelled `Int`: Science has no 32-bit `Int`, and
    /// nothing in the language reaches this parameter. The exit status is the
    /// emitted `main`'s own value, not a Science one.
    I32,
    /// `u64`. `science_string_push_u64` takes one, and §2.3's rendering rule is
    /// why it is a separate entry point from the signed one rather than a cast
    /// at the call site: *"`U8`…`U64` are zero-extended by codegen before the
    /// call"*, and a `u64` above `i64::MAX` sign-extended instead would render
    /// as a negative number.
    U64,
    /// `f64`.
    F64,
    /// `f32`.
    ///
    /// **Not a `f64` narrowed, and `format.rs` says why.** The rendering is the
    /// shortest string that round-trips, *"a property of the width, so the
    /// width has to reach the formatter"* — `0.1f32 as f64` is
    /// `0.10000000149011612`, which round-trips as an `F64` and is the wrong
    /// answer about an `F32`.
    F32,
    /// `bool`, as C's one-byte `_Bool`. §3.1's memory form of a `Bool`.
    Bool,
    /// `u32` holding a Unicode scalar value: `Char`, which is what
    /// `science_chars_next` writes and what `science_string_push_char` reads.
    ///
    /// **Spelled `Char` rather than `U32`**, although the C parameter is a
    /// `u32` and the two lower to the same `i32` with the same width, the same
    /// class and the same register. §3.1 makes `Char` *"a Unicode scalar value
    /// in a `u32`"* and [`crate::layout::CgTy::Char`] is that type, so the
    /// informative name costs nothing and says which of the two facts is
    /// load-bearing: not that the parameter is thirty-two bits, but that its
    /// value set is the scalar values — which is why `format.rs` renders an
    /// out-of-range one as `U+FFFD` instead of trusting it.
    Char,
}

/// One runtime entry point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeFn {
    /// The exported symbol.
    pub symbol: &'static str,
    /// Parameters in order.
    pub params: &'static [RtParam],
    /// The return.
    pub ret: RtRet,
}

impl RuntimeFn {
    /// Where the element descriptor sits in the parameter list, if it takes
    /// one.
    ///
    /// **§9.3's finding 4, discharged.** The runtime's §6 says codegen *"passes
    /// a pointer to it on every call"* and never says where, and the crate is
    /// not consistent: `science_array_free(array, info)` and
    /// `science_map_free(map, info)` put the receiver first;
    /// `science_box_new(info, value)` and `science_box_free(info, ptr)` put the
    /// descriptor first. From the page alone you would pick one convention and
    /// get `Box` backwards — and since both parameters are pointers, that is
    /// not a type error anywhere. It is `science_box_free` treating a
    /// `ScienceTypeInfo` as a value pointer.
    pub fn descriptor_index(&self) -> Option<usize> {
        self.params.iter().position(|p| *p == RtParam::Descriptor)
    }

    /// How the return value is passed on `abi`.
    pub fn return_class(&self, abi: CAbi) -> ReturnClass {
        match self.ret {
            RtRet::Void | RtRet::Never => ReturnClass::Void,
            RtRet::Aggregate(aggregate) => {
                let target = match abi {
                    CAbi::SystemVAmd64 => Triple::X86_64LinuxGnu,
                    CAbi::Aapcs64 => Triple::Aarch64AppleDarwin,
                    CAbi::Win64 => Triple::X86_64WindowsMsvc,
                };
                classify_return(abi, &aggregate.layout(target))
            }
            _ => ReturnClass::Direct { registers: vec![crate::abi::RegClass::Integer] },
        }
    }

    /// Whether a call to this entry point needs an `sret` parameter on `abi`.
    ///
    /// Derived, never listed. That is the whole repair for finding 1.
    pub fn needs_sret(&self, abi: CAbi) -> bool {
        self.return_class(abi).is_sret()
    }
}

/// Shorthands, so the table below reads as a table.
const P: RtParam = RtParam::Pointer;
const D: RtParam = RtParam::Descriptor;
const Z: RtParam = RtParam::Usize;
const N: RtParam = RtParam::Int;

/// The 55 entry points. §2.6: *"They are the whole list."*
///
/// **It was 47, `format.rs` added seven, and `science_string_with_capacity`
/// added the fifty-fifth.** The count is asserted in two
/// places and both had to be edited, which is the point of asserting it: a
/// table that is *"the whole list"* grows only when somebody says so.
///
/// Ordered by module and then as `science-rt` declares them, which is neither
/// alphabetical nor arbitrary: it is the order a reader comparing this table
/// against the crate walks in, and a table sorted differently from its source
/// is a table nobody re-checks.
pub const RUNTIME: &[RuntimeFn] = &[
    // --- array.rs ---
    RuntimeFn { symbol: "science_array_new", params: &[D], ret: RtRet::Aggregate(RtAggregate::Array) },
    // The one §2's list had lost. Three words, MEMORY on every one of the three
    // targets, and indistinguishable from `science_array_new` beside it.
    RuntimeFn { symbol: "science_array_with_capacity", params: &[D, Z], ret: RtRet::Aggregate(RtAggregate::Array) },
    RuntimeFn { symbol: "science_array_reserve", params: &[P, D, Z], ret: RtRet::Void },
    RuntimeFn { symbol: "science_array_free", params: &[P, D], ret: RtRet::Void },
    RuntimeFn { symbol: "science_array_len", params: &[P], ret: RtRet::Int },
    RuntimeFn { symbol: "science_array_is_empty", params: &[P], ret: RtRet::Bool },
    RuntimeFn { symbol: "science_array_as_ptr", params: &[P], ret: RtRet::Ptr },
    RuntimeFn { symbol: "science_array_push", params: &[P, D, P], ret: RtRet::Void },
    RuntimeFn { symbol: "science_array_pop", params: &[P, D, P], ret: RtRet::Bool },
    RuntimeFn { symbol: "science_array_get", params: &[P, D, N], ret: RtRet::Ptr },
    RuntimeFn { symbol: "science_array_get_mut", params: &[P, D, N], ret: RtRet::Ptr },
    // --- boxed.rs ---
    // Descriptor **first**. This is the pair finding 4 is about.
    RuntimeFn { symbol: "science_box_new", params: &[D, P], ret: RtRet::Ptr },
    RuntimeFn { symbol: "science_box_free", params: &[D, P], ret: RtRet::Void },
    // --- exit.rs ---
    // §9.3's finding 5, discharged. These are the two symbols the note asks for
    // by name — *"a stderr writer that does not abort, and
    // `science_exit(code: I32)`"* — and they are what makes `script-mode.md`
    // §2.3's fourth row reachable. Neither returns an aggregate, so neither
    // joins the nine, which the derived set says without anybody deciding.
    RuntimeFn { symbol: "science_write_error_bytes", params: &[P, Z], ret: RtRet::Void },
    RuntimeFn { symbol: "science_exit", params: &[RtParam::I32], ret: RtRet::Never },
    // --- io.rs ---
    RuntimeFn { symbol: "science_write", params: &[P], ret: RtRet::Void },
    RuntimeFn { symbol: "science_print", params: &[P], ret: RtRet::Void },
    RuntimeFn { symbol: "science_read_file", params: &[P], ret: RtRet::Aggregate(RtAggregate::StringAndIoError) },
    RuntimeFn { symbol: "science_write_file", params: &[P, P], ret: RtRet::Aggregate(RtAggregate::NullableIoError) },
    // --- map.rs ---
    RuntimeFn { symbol: "science_map_new", params: &[D], ret: RtRet::Aggregate(RtAggregate::Map) },
    RuntimeFn { symbol: "science_map_free", params: &[P, D], ret: RtRet::Void },
    RuntimeFn { symbol: "science_map_len", params: &[P], ret: RtRet::Int },
    RuntimeFn { symbol: "science_map_insert", params: &[P, D, P, P, P], ret: RtRet::Bool },
    RuntimeFn { symbol: "science_map_get", params: &[P, D, P], ret: RtRet::Ptr },
    RuntimeFn { symbol: "science_map_contains", params: &[P, D, P], ret: RtRet::Bool },
    RuntimeFn { symbol: "science_map_remove", params: &[P, D, P, P], ret: RtRet::Bool },
    // --- mem.rs ---
    RuntimeFn { symbol: "science_alloc", params: &[Z, Z], ret: RtRet::Ptr },
    RuntimeFn { symbol: "science_realloc", params: &[P, Z, Z, Z], ret: RtRet::Ptr },
    RuntimeFn { symbol: "science_dealloc", params: &[P, Z, Z], ret: RtRet::Void },
    // --- panic.rs ---
    RuntimeFn { symbol: "science_abort", params: &[], ret: RtRet::Never },
    RuntimeFn { symbol: "science_panic", params: &[P], ret: RtRet::Never },
    RuntimeFn { symbol: "science_panic_bytes", params: &[P, Z], ret: RtRet::Never },
    // --- string.rs ---
    RuntimeFn { symbol: "science_string_new", params: &[], ret: RtRet::Aggregate(RtAggregate::String) },
    // §1.7's capacity, and the tenth `sret`. Three words returned by value, so
    // the derived set grew by one without anybody adding a name to a list —
    // which is the property this module exists for. `Z` and not `N`: a capacity
    // is not a Science `Int`, which is §9.3's finding 3 and is the same answer
    // `science_array_with_capacity` gives one line apart.
    RuntimeFn { symbol: "science_string_with_capacity", params: &[Z], ret: RtRet::Aggregate(RtAggregate::String) },
    RuntimeFn { symbol: "science_string_from_bytes", params: &[P, Z], ret: RtRet::Aggregate(RtAggregate::String) },
    RuntimeFn { symbol: "science_string_free", params: &[P], ret: RtRet::Void },
    RuntimeFn { symbol: "science_string_clone", params: &[P], ret: RtRet::Aggregate(RtAggregate::String) },
    RuntimeFn { symbol: "science_string_len", params: &[P], ret: RtRet::Int },
    RuntimeFn { symbol: "science_string_is_empty", params: &[P], ret: RtRet::Bool },
    RuntimeFn { symbol: "science_string_as_ptr", params: &[P], ret: RtRet::Ptr },
    RuntimeFn { symbol: "science_string_push_str", params: &[P, P], ret: RtRet::Void },
    // --- format.rs ---
    //
    // **The seven that make a number printable**, and until they existed no
    // program this compiler produced could print one: `codegen-and-linking.md`
    // §10 writes stage 2 and stage 3 as `print(f"{x}")`, and
    // `science-codegen-llvm`'s §0 recorded that there was *"no integer-to-string
    // entry point in the runtime"* to lower it to. They are declared here
    // because §2.6 makes this table *"the whole list"* and
    // `science-codegen-llvm`'s `tests/symbols.rs` checks the two directions
    // against `science-rt`'s own `#[no_mangle]` definitions — so an entry point
    // that exists and is not declared is a hard failure rather than dead code.
    //
    // **One per width and per signedness, rather than one taking a descriptor.**
    // That is `format.rs`'s decision and the reason is in its own notes: the
    // rendering of a float is the shortest string that round-trips *at its
    // width*, and the rendering of an integer depends on whether the bits are
    // signed, so both facts have to reach the formatter and neither survives a
    // cast at the call site.
    RuntimeFn { symbol: "science_string_push_bytes", params: &[P, P, Z], ret: RtRet::Void },
    RuntimeFn { symbol: "science_string_push_i64", params: &[P, N], ret: RtRet::Void },
    RuntimeFn { symbol: "science_string_push_u64", params: &[P, RtParam::U64], ret: RtRet::Void },
    RuntimeFn { symbol: "science_string_push_f64", params: &[P, RtParam::F64], ret: RtRet::Void },
    RuntimeFn { symbol: "science_string_push_f32", params: &[P, RtParam::F32], ret: RtRet::Void },
    RuntimeFn { symbol: "science_string_push_bool", params: &[P, RtParam::Bool], ret: RtRet::Void },
    RuntimeFn { symbol: "science_string_push_char", params: &[P, RtParam::Char], ret: RtRet::Void },
    RuntimeFn { symbol: "science_string_truncate", params: &[P, N], ret: RtRet::Aggregate(RtAggregate::String) },
    RuntimeFn { symbol: "science_string_starts_with", params: &[P, P], ret: RtRet::Bool },
    RuntimeFn { symbol: "science_string_chars", params: &[P], ret: RtRet::Aggregate(RtAggregate::Chars) },
    RuntimeFn { symbol: "science_chars_next", params: &[P, P], ret: RtRet::Bool },
    RuntimeFn { symbol: "science_string_eq", params: &[P, P], ret: RtRet::Bool },
    RuntimeFn { symbol: "science_string_cmp", params: &[P, P], ret: RtRet::I32 },
    RuntimeFn { symbol: "science_string_hash", params: &[P], ret: RtRet::U64 },
];

/// Look an entry point up by symbol.
pub fn runtime_fn(symbol: &str) -> Option<&'static RuntimeFn> {
    RUNTIME.iter().find(|f| f.symbol == symbol)
}

/// The three function-pointer signatures **codegen emits**, which the runtime's
/// contract page does not state.
///
/// **§9.3's finding 2, and the note calls it the most consequential gap**:
/// §6 names `drop_fn`, `hash_fn` and `eq_fn` and gives `ScienceTypeInfo` as
/// `{ size, align, drop_fn }`, but never gives their C signatures. They are
/// documented in `abi.rs`'s item docs, not on the page. This matters more than
/// the others because these three functions are codegen's **own output** and a
/// wrong signature is a wrong calling convention — the compiler would be
/// miscompiling something it wrote itself.
///
/// The signatures, from `crates/science-rt/src/abi.rs`:
///
/// | Slot | C signature |
/// |---|---|
/// | `drop_fn` | `void (*)(uint8_t *value)` |
/// | `hash_fn` | `uint64_t (*)(const uint8_t *key)` |
/// | `eq_fn` | `bool (*)(const uint8_t *a, const uint8_t *b)` |
///
/// All three take erased `u8` pointers, not typed ones: the runtime is not
/// generic, it stores raw bytes, and the descriptor is how it is told what they
/// mean.
pub const EMITTED_FN_SIGNATURES: &[(&str, &str)] = &[
    ("drop_fn", "void (*)(uint8_t *value)"),
    ("hash_fn", "uint64_t (*)(const uint8_t *key)"),
    ("eq_fn", "bool (*)(const uint8_t *a, const uint8_t *b)"),
];

/// How an owned `T?` comes back from the runtime, in both cases.
///
/// §5.3's convention is a `bool` return plus a `*mut u8` out-parameter: `true`
/// means the payload was written and is the caller's to own; `false` means the
/// out slot was **not touched** and the answer is `null`.
///
/// The page then adds: *"the returned `bool` is the same byte as the
/// discriminant of §5.1, so when `T` has no niche codegen may store it straight
/// into the tag"*. That sentence is true of one case and reads like a licence
/// for both, and this is the finding §9 did not make.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnedNullableReturn {
    /// `T` has no niche, so the `T?` is tagged.
    ///
    /// The `bool` **is** the discriminant — `SCIENCE_NULLABLE_PRESENT == 1`,
    /// `SCIENCE_NULLABLE_NULL == 0` — so codegen stores it into the tag byte
    /// and the payload slot is the out-parameter. One store, no branch, and the
    /// payload is meaningful only under the tag, so leaving it untouched on the
    /// false edge is correct.
    StoreBoolIntoTag,
    /// `T` has a niche, so the `T?` **is** the payload and there is no tag.
    ///
    /// There is nowhere to store the byte. Codegen must branch on the `bool`
    /// and materialise a null pointer on the false edge, because the out slot
    /// is documented as untouched and reading it unconditionally reads
    /// uninitialised memory. `science_array_pop` on an
    /// `Array of (Box of T)` is this case and it is not exotic.
    BranchAndMaterialiseNull,
}

/// Which of the two shapes applies to a payload type.
pub fn owned_nullable_return(payload: &CgTy) -> OwnedNullableReturn {
    match payload.niche() {
        Some(_) => OwnedNullableReturn::BranchAndMaterialiseNull,
        None => OwnedNullableReturn::StoreBoolIntoTag,
    }
}

/// What `script-mode.md` §2.3 requires of a program's exit and what
/// `science-rt` provides for it.
///
/// **§9.3's finding 5, discharged in its mechanical half and not in its whole.**
/// The finding was that a failing `main` *"cannot be emitted"*: there was no
/// symbol that wrote to stderr without aborting, and none that exited with a
/// chosen status. `science-rt` now has both — [`ExitContract::eprint_symbol`]
/// and [`ExitContract::exit_symbol`] name them — so
/// [`ExitContract::is_satisfiable`] is true and the emitted `main` implements
/// §2.3's table rather than aborting on its fourth row.
///
/// **What is still short is the rendering, and
/// [`ExitContract::display_is_renderable`] is the record of it.** §2.3 asks for
/// `error: ` followed by *the `Display` of the error*. `Display` is declared in
/// the prelude as an interface **with no methods**, because — in
/// `science-resolve`'s `builtins.rs`, which made the call — writing
/// `Display.display(Formatter)` would invent `Formatter`, a Level 1 type no
/// note specifies, as a side effect of a bound check. So there is no method to
/// call, no vtable slot to call it through, and nothing for codegen to emit but
/// a fixed message.
///
/// That is the honest state and it is deliberately not repaired here. Inventing
/// a `Formatter` to satisfy a table in a design note would be `assign.rs` §3's
/// named failure — a signature invented in passing is how a language acquires a
/// design nobody argued for — and it would be invented by the *backend*, which
/// is the component with the least standing to decide it. Printing something
/// true and less than promised costs a user the error's identity on a path they
/// can still see, diagnose and exit from; inventing the type costs the language
/// a decision.
pub const EXIT_CONTRACT: ExitContract = ExitContract {
    required_status: 1,
    required_prefix: "error: ",
    required_stream: "stderr",
    eprint_symbol: Some("science_write_error_bytes"),
    exit_symbol: Some("science_exit"),
    display_is_renderable: false,
};

/// See [`EXIT_CONTRACT`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitContract {
    /// The exit status a failing script body must produce.
    pub required_status: i32,
    /// What must be printed before the error's `Display`.
    pub required_prefix: &'static str,
    /// Which stream it goes to.
    pub required_stream: &'static str,
    /// The runtime symbol that writes to stderr without aborting. `None`: there
    /// is none.
    pub eprint_symbol: Option<&'static str>,
    /// The runtime symbol that exits with a chosen status. `None`: there is
    /// none.
    pub exit_symbol: Option<&'static str>,
    /// Whether codegen can render the error itself, rather than a placeholder.
    ///
    /// `false`, and it is the half of §2.3 that the two symbols above do not
    /// buy. See [`EXIT_CONTRACT`] for why, and what the alternative would have
    /// cost. It is a field rather than a comment so that the day `Display`
    /// grows a method, the test asserting this is `false` fails and points at
    /// the message that should stop being a placeholder.
    pub display_is_renderable: bool,
}

impl ExitContract {
    /// Whether the runtime has the two symbols a failing exit needs.
    ///
    /// **This asks about the mechanism and not about the message.** A `true`
    /// here means the emitted `main` can write to stderr and end the process
    /// with status 1; [`ExitContract::display_is_renderable`] is the separate
    /// question of whether what it writes is the error rather than a stand-in
    /// for it. Folding the two into one predicate would have made the contract
    /// unsatisfiable for as long as `Display` has no method, which would hide
    /// the exit status behind the rendering and leave a program that returns an
    /// error aborting with 3 — the state this replaced.
    pub fn is_satisfiable(&self) -> bool {
        self.eprint_symbol.is_some() && self.exit_symbol.is_some()
    }

    /// [`ExitContract::display_is_renderable`], read through a call.
    ///
    /// The field is `const`, so `assert!(!EXIT_CONTRACT.display_is_renderable)`
    /// is an assertion the compiler folds away and clippy's
    /// `assertions_on_constants` says so. Reading it through a method keeps the
    /// four places that assert it — here, `tests/runtime_abi.rs`,
    /// `tests/stage_one.rs` and `science-codegen-llvm`'s `tests/exit_code.rs` —
    /// assertions rather than comments, which matters because every one of them
    /// is a tripwire meant to fire on a change somebody else makes.
    pub fn renders_the_error(&self) -> bool {
        self.display_is_renderable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_fifty_five_and_they_are_all_science_prefixed_and_unique() {
        assert_eq!(RUNTIME.len(), 55, "§2.6: \"they are the whole list\"");
        let mut symbols: Vec<&str> = RUNTIME.iter().map(|f| f.symbol).collect();
        for symbol in &symbols {
            assert!(symbol.starts_with("science_"), "{symbol} breaks §8's one-prefix rule");
        }
        let count = symbols.len();
        symbols.sort_unstable();
        symbols.dedup();
        assert_eq!(symbols.len(), count);
    }

    #[test]
    fn the_sret_set_is_derived_and_is_ten_not_the_eight_the_page_listed() {
        // §9.2's finding 1, and its recurrence. The test derives the set from
        // the signatures rather than reading a list, which is the whole repair,
        // and the derived set is **nine**. `science-rt`'s §2 names eight.
        //
        // The ninth is `science_string_from_bytes`, and it is missing for
        // exactly the reason `science_array_with_capacity` was: the page
        // mentions it in §8 ("literal construction") and not in §2, and §2 is
        // the section that specifies the return convention.
        //
        // See the module documentation for why this one is worse.
        //
        // **The tenth is `science_string_with_capacity`**, and it is the first
        // one this test has ever gained without a list being wrong: it was
        // written into `RUNTIME` as a signature, classified by the same code
        // path as the other nine, and the number here changed because the
        // derivation changed its answer. That is the difference this module
        // exists to make.
        let expected = [
            "science_string_new",
            "science_string_with_capacity",
            "science_string_clone",
            "science_string_truncate",
            "science_string_from_bytes",
            "science_array_new",
            "science_array_with_capacity",
            "science_map_new",
            "science_string_chars",
            "science_read_file",
        ];
        for abi in [CAbi::SystemVAmd64, CAbi::Aapcs64, CAbi::Win64] {
            let mut derived: Vec<&str> =
                RUNTIME.iter().filter(|f| f.needs_sret(abi)).map(|f| f.symbol).collect();
            derived.sort_unstable();
            let mut want = expected.to_vec();
            want.sort_unstable();
            assert_eq!(derived, want, "{abi:?}");
        }
    }

    #[test]
    fn io_error_is_one_byte_and_its_nullable_is_that_byte_plus_a_tag() {
        // The bare row exists for a *local* and not for a signature, so nothing
        // in `RUNTIME` would have caught it being wrong. What it has to be is
        // fixed by `science-rt`'s `#[repr(transparent)] struct ScienceIoError(pub
        // u8)` and by that crate's `tests/layout.rs`, which pins one byte at
        // alignment one; and `IoError?` has to be exactly that byte with a
        // discriminant in front of it, which is §8.1's refusal of the niche.
        assert_eq!(RtAggregate::IoError.layout(Triple::X86_64LinuxGnu).size, 1);
        assert_eq!(RtAggregate::IoError.layout(Triple::X86_64LinuxGnu).align, 1);
        assert_eq!(RtAggregate::NullableIoError.layout(Triple::X86_64LinuxGnu).size, 2);
        // The one thing the two rows sharing a definition is meant to buy: the
        // nullable is built *from* the bare type, so a variant added to one is
        // added to both.
        assert_eq!(
            RtAggregate::NullableIoError.cg_ty(),
            CgTy::nullable(RtAggregate::IoError.cg_ty())
        );
    }

    #[test]
    fn nullable_io_error_comes_back_in_a_register_on_every_convention() {
        // §2's named exception, and the one aggregate return that is not sret.
        let f = runtime_fn("science_write_file").unwrap();
        for abi in [CAbi::SystemVAmd64, CAbi::Aapcs64, CAbi::Win64] {
            assert!(!f.needs_sret(abi), "{abi:?}");
        }
        assert_eq!(RtAggregate::NullableIoError.layout(Triple::X86_64LinuxGnu).size, 2);
    }

    #[test]
    fn the_descriptor_is_second_for_containers_and_first_for_box() {
        // §9.3's finding 4, as a test. Getting this backwards is
        // `science_box_free` treating a `ScienceTypeInfo` as a value pointer,
        // which is not a type error anywhere.
        assert_eq!(runtime_fn("science_array_free").unwrap().descriptor_index(), Some(1));
        assert_eq!(runtime_fn("science_map_free").unwrap().descriptor_index(), Some(1));
        assert_eq!(runtime_fn("science_array_push").unwrap().descriptor_index(), Some(1));
        assert_eq!(runtime_fn("science_box_new").unwrap().descriptor_index(), Some(0));
        assert_eq!(runtime_fn("science_box_free").unwrap().descriptor_index(), Some(0));
        // And an entry point that takes none says so.
        assert_eq!(runtime_fn("science_string_len").unwrap().descriptor_index(), None);
    }

    #[test]
    fn a_length_is_a_usize_and_an_index_is_an_int() {
        // §9.3's finding 3 on the parameter side. `science_array_get` takes an
        // `i64` index because an index is Science's `Int`;
        // `science_array_with_capacity` takes a `usize` capacity because a
        // capacity is not.
        assert_eq!(runtime_fn("science_array_get").unwrap().params, &[P, D, N]);
        assert_eq!(runtime_fn("science_array_with_capacity").unwrap().params, &[D, Z]);
        assert_eq!(runtime_fn("science_string_from_bytes").unwrap().params, &[P, Z]);
        assert_eq!(runtime_fn("science_panic_bytes").unwrap().params, &[P, Z]);
        // And the return is the other way round: a length *returns* `i64`.
        assert_eq!(runtime_fn("science_array_len").unwrap().ret, RtRet::Int);
    }

    #[test]
    fn the_diverging_entry_points_return_nothing_and_never_unwind() {
        for symbol in ["science_abort", "science_panic", "science_panic_bytes"] {
            let f = runtime_fn(symbol).unwrap();
            assert_eq!(f.ret, RtRet::Never);
            assert_eq!(f.return_class(CAbi::SystemVAmd64), ReturnClass::Void);
        }
    }

    #[test]
    fn the_owned_nullable_convention_has_two_cases_and_the_page_licenses_one() {
        assert_eq!(
            owned_nullable_return(&CgTy::Int(IntTy::I64)),
            OwnedNullableReturn::StoreBoolIntoTag
        );
        assert_eq!(
            owned_nullable_return(&CgTy::Ptr(PtrKind::Box)),
            OwnedNullableReturn::BranchAndMaterialiseNull
        );
        assert_eq!(
            owned_nullable_return(&CgTy::Interface),
            OwnedNullableReturn::BranchAndMaterialiseNull
        );
    }

    #[test]
    fn the_exit_contract_is_satisfiable_and_names_two_symbols_the_table_has() {
        assert!(EXIT_CONTRACT.is_satisfiable());
        assert_eq!(EXIT_CONTRACT.required_status, 1);
        // Naming a symbol the table does not have is a link error at the end of
        // a long build, so the contract's two names are looked up rather than
        // trusted.
        for symbol in [EXIT_CONTRACT.eprint_symbol, EXIT_CONTRACT.exit_symbol] {
            let symbol = symbol.expect("finding 5's two symbols");
            assert!(runtime_fn(symbol).is_some(), "`{symbol}` is not in the table");
        }
        // The writer returns, which is the whole difference from the panic path
        // beside it; the exit does not.
        assert_eq!(runtime_fn("science_write_error_bytes").unwrap().ret, RtRet::Void);
        assert_eq!(runtime_fn("science_exit").unwrap().ret, RtRet::Never);
        assert_eq!(runtime_fn("science_panic_bytes").unwrap().ret, RtRet::Never);
        // And the half that is still owed. When this fails, `Display` has grown
        // a method and the placeholder in `science-codegen-llvm`'s `lower` is
        // the thing to delete.
        assert!(!EXIT_CONTRACT.renders_the_error());
    }

    #[test]
    fn the_three_emitted_signatures_are_written_down() {
        assert_eq!(EMITTED_FN_SIGNATURES.len(), 3);
        let names: Vec<&str> = EMITTED_FN_SIGNATURES.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, ["drop_fn", "hash_fn", "eq_fn"]);
        // All three take erased byte pointers. A typed pointer here would be a
        // different calling convention on no target and a lie on every one.
        for (_, signature) in EMITTED_FN_SIGNATURES {
            assert!(signature.contains("uint8_t"), "{signature}");
        }
    }
}
