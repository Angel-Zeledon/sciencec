//! The runtime boundary: §2.6, and the place §9's findings become facts a test
//! checks.
//!
//! **The decision.** The 45 `science_`-prefixed entry points are described here
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
//! **The cost.** Forty-five signatures transcribed by hand, which is a
//! transcription that can be wrong in exactly the way the thing it replaces was
//! wrong. Two mitigations: `tests/runtime_abi.rs` asserts the count is 45, that
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
//! # Decision 14: these 45 are the only runtime calls F0 emits
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
    /// `{ present: u8, error: u8 }` — two bytes, and §2's named exception:
    /// *"at two bytes it comes back in a register on both conventions"*.
    NullableIoError,
    /// `{ value: ScienceString, error: ScienceNullableIoError }` — the pair of
    /// §5.4.
    StringAndIoError,
}

impl RtAggregate {
    /// All eight, in a fixed order.
    pub const ALL: [RtAggregate; 8] = [
        RtAggregate::String,
        RtAggregate::Chars,
        RtAggregate::Array,
        RtAggregate::Map,
        RtAggregate::TypeInfo,
        RtAggregate::MapInfo,
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
            RtAggregate::NullableIoError => CgTy::nullable(CgTy::choice(
                "ScienceIoError",
                vec![
                    Variant::unit("not_found"),
                    Variant::unit("permission_denied"),
                    Variant::unit("already_exists"),
                    Variant::unit("invalid_data"),
                    Variant::unit("other"),
                ],
            )),
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

/// The 45 entry points. §2.6: *"They are the whole list."*
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
    RuntimeFn { symbol: "science_string_from_bytes", params: &[P, Z], ret: RtRet::Aggregate(RtAggregate::String) },
    RuntimeFn { symbol: "science_string_free", params: &[P], ret: RtRet::Void },
    RuntimeFn { symbol: "science_string_clone", params: &[P], ret: RtRet::Aggregate(RtAggregate::String) },
    RuntimeFn { symbol: "science_string_len", params: &[P], ret: RtRet::Int },
    RuntimeFn { symbol: "science_string_is_empty", params: &[P], ret: RtRet::Bool },
    RuntimeFn { symbol: "science_string_as_ptr", params: &[P], ret: RtRet::Ptr },
    RuntimeFn { symbol: "science_string_push_str", params: &[P, P], ret: RtRet::Void },
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
/// **§9.3's finding 5, and it is not fixed.** There is no `science_main`, no
/// runtime initialisation, no teardown, no `argv` access and no `science_exit`.
/// Codegen emitting `main` itself is survivable — that is what a code generator
/// does. The exit contract is not:
///
/// > *A hello world can be emitted from this page. A `main` that returns an
/// > error cannot.* There is no symbol that writes to stderr without aborting,
/// > and no symbol that exits with a chosen code.
///
/// `science_print` and `science_write` go to stdout. `science_panic_bytes`
/// writes to stderr and then calls `std::process::abort()` — the platform's
/// abort status, `SIGABRT` on POSIX and `3` on Windows, and not `1`.
///
/// This constant is empty of symbols on purpose. It is the record of a gap, and
/// the day somebody adds `science_eprint` and `science_exit` to the runtime,
/// this is the item that tells them what the two of them are for.
pub const EXIT_CONTRACT: ExitContract = ExitContract {
    required_status: 1,
    required_prefix: "error: ",
    required_stream: "stderr",
    eprint_symbol: None,
    exit_symbol: None,
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
}

impl ExitContract {
    /// Whether the runtime can currently implement the contract.
    pub fn is_satisfiable(&self) -> bool {
        self.eprint_symbol.is_some() && self.exit_symbol.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_forty_five_and_they_are_all_science_prefixed_and_unique() {
        assert_eq!(RUNTIME.len(), 45, "§2.6: \"they are the whole list\"");
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
    fn the_sret_set_is_derived_and_is_nine_not_the_eight_the_page_lists() {
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
        let expected = [
            "science_string_new",
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
    fn the_exit_contract_is_not_satisfiable_and_that_is_the_finding() {
        assert!(!EXIT_CONTRACT.is_satisfiable());
        assert_eq!(EXIT_CONTRACT.required_status, 1);
        assert!(EXIT_CONTRACT.eprint_symbol.is_none());
        assert!(EXIT_CONTRACT.exit_symbol.is_none());
        // And nothing in the table can stand in for them: the only stderr
        // writer aborts.
        assert!(runtime_fn("science_eprint").is_none());
        assert!(runtime_fn("science_exit").is_none());
        assert_eq!(runtime_fn("science_panic_bytes").unwrap().ret, RtRet::Never);
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
