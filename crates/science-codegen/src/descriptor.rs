//! Element descriptors, drop glue, and the string literal that looks inline
//! and is not.
//!
//! # The descriptor, and the one place a codegen bug is silent
//!
//! `science-rt` §6 is the element-descriptor convention: one implementation of
//! `Array` and `Map`, parameterised at run time by `{ size, align, drop_fn }`,
//! so that monomorphisation does not multiply the container. The page states
//! the precondition in a block quote and §3.5 calls it the sharpest sentence in
//! the crate:
//!
//! > *Every call touching a given container must pass the same descriptor
//! > contents that were passed when it was created. Passing a descriptor for a
//! > different type reinterprets the container's bytes as that type. Nothing
//! > detects this.*
//!
//! **The decision (Decision 20).** Codegen emits exactly one
//! `private unnamed_addr constant ScienceTypeInfo` per monomorphised element
//! type, keyed by the monomorphisation key of §2.7, and every call site loads
//! the address of that one global. A descriptor is never constructed at a call
//! site and never constructed twice.
//!
//! **The reason.** That turns an unchecked runtime precondition into a property
//! of the symbol table: two call sites pass the same descriptor because they
//! pass the same symbol. And it makes the precondition *testable* — a test can
//! assert the module contains exactly one descriptor per distinct mono key —
//! which is better than a convention nobody can check.
//!
//! **The cost.** A global per element type, and the discipline never to build
//! one inline. `unnamed_addr` lets LLVM merge two descriptors that happen to be
//! bit-identical, which is harmless because the runtime compares contents and
//! not addresses.

use std::collections::BTreeMap;

use crate::layout::{CgTy, Layout, Repr, Triple, layout_of};
use crate::mangle::{MonoKey, mangle};

/// A `ScienceTypeInfo` to be emitted as a constant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeInfo {
    /// The element's size in bytes, which is also its stride.
    pub size: u64,
    /// The element's alignment. Always a power of two, always at least one.
    pub align: u64,
    /// The drop glue symbol, or `None`.
    ///
    /// **`None` matters for performance and the runtime page says why:** it
    /// lets the runtime skip the per-element destructor loop entirely when
    /// freeing an `Array of F64`, which is the common case in this language. A
    /// codegen that emits a do-nothing glue function instead of `null` is
    /// correct and is quietly slower on the hottest container in the language.
    pub drop_fn: Option<String>,
}

/// Decision 13's vtable: the method table one `T implements I:` block puts
/// behind an `any I`.
///
/// **The decision.** One `private unnamed_addr constant [N x ptr]` per
/// `(interface, concrete type)` pair, holding the address of each of the
/// interface's methods **in the interface's own declaration order**, and every
/// `any I` made from that type carries the address of that one global in its
/// second word.
///
/// **The reason the order is the interface's and not the block's.** The slot
/// index is the only thing a call site through `any I` knows: it has the
/// interface's declaration — which says `message` is slot 0 — and a vtable
/// pointer, and nothing else. An `implements` block may write its methods in
/// any order it likes, and two blocks for one interface routinely do; if the
/// table followed the block, two implementations of one interface would
/// disagree about which slot `message` is, and the call would jump to whatever
/// the other type happened to write first. That is a program that verifies,
/// links, and calls the wrong method — this file's neighbours are full of that
/// shape, and this is its version.
///
/// It is the same convention Decision 18 uses for a `choice`: position in
/// declaration order *is* the index, derived from the source and from nothing
/// else.
///
/// **The cost.** A global per `(interface, type)` pair rather than per type,
/// and the slot order is a contract between two places that never see each
/// other — [`crate::backend::Backend::define_vtable`] writes it and the
/// dispatching call site reads it. Both derive the order from the same walk of
/// the interface's children, which is what keeps them in step; a second
/// ordering rule anywhere is the defect this comment exists to prevent.
///
/// **One slot beyond the methods: `descriptor`, Decision 12's answer to
/// releasing the value behind the table.** A method's slot is resolved from
/// the concrete type because dispatch needs an address only the concrete
/// implementation has; a drop needs no such address — the concrete type's
/// `ScienceTypeInfo` is already a global, built by
/// [`crate::descriptor::type_info`] at the same boxing site that built the
/// table, and the table is the one thing a release of `any I` can reach the
/// concrete type *through*. So `descriptor` names that global rather than
/// inventing a second lookup: `science_box_free(descriptor, data)` is the same
/// call an ordinary `Box[T]`'s glue already makes, with the descriptor read
/// back out of the table instead of known at the call site. It sits after
/// every method slot rather than before them so that a method's own index —
/// its position among the interface's declared methods, which
/// `Lowerer::lower_dispatch` computes independently of this struct — is
/// untouched by its presence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vtable {
    /// The constant's symbol.
    pub symbol: String,
    /// The mangled symbol of each method, in the interface's declaration
    /// order. Never empty: an interface with no methods needs no table, and
    /// emitting a zero-length one would be a global nothing can index.
    pub methods: Vec<String>,
    /// The concrete type's `ScienceTypeInfo` symbol, emitted at slot
    /// `methods.len()` — after every method, so a method's own slot index is
    /// never a function of whether this one exists.
    pub descriptor: String,
}

impl Vtable {
    /// The symbol a vtable for `(interface, concrete)` is emitted under.
    ///
    /// Derived from the two mangled names rather than from a counter, so that
    /// interning the same pair twice from two call sites lands on one global
    /// without either call site having to know the other happened —
    /// [`DescriptorTable::symbol_for`]'s reason, applied to a pair.
    pub fn symbol_for(interface: &MonoKey, concrete: &MonoKey) -> String {
        format!("{}.vtable.{}", mangle(concrete), mangle(interface))
    }

    /// How many method slots the table has. The descriptor slot is not one of
    /// these: it is not indexed by a method's position and no caller asks for
    /// it by number.
    pub fn len(&self) -> usize {
        self.methods.len()
    }

    /// Whether the table has no method slots, which no emitted vtable may be
    /// — the descriptor slot alone is not a table anything can dispatch
    /// through.
    pub fn is_empty(&self) -> bool {
        self.methods.is_empty()
    }
}

/// A `ScienceMapInfo` to be emitted as a constant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapInfo {
    /// The key type's descriptor.
    pub key: TypeInfo,
    /// The value type's descriptor.
    pub value: TypeInfo,
    /// The key type's hash function, supplied by codegen from its `Eq`
    /// implementation. Never null: the runtime has no fallback.
    pub hash_fn: String,
    /// The key type's equality. Never null.
    pub eq_fn: String,
}

/// Whether a type owns anything that has to be released (Decision 12).
///
/// A type whose fields transitively need no destructor gets no glue function
/// and no call. In F0 the owning types are the four library containers and
/// anything reaching one, plus any type with its own `Drop` implementation.
///
/// This function is told about the latter through `has_drop_impl`, because a
/// user `Drop` is a fact about a definition and not about a layout, and
/// [`CgTy`] deliberately does not carry definitions. A caller with no type
/// checker — which is every caller today — passes a closure that always says
/// no, and gets the right answer for every type in the standard library.
pub fn needs_drop(ty: &CgTy, has_drop_impl: &dyn Fn(&str) -> bool) -> bool {
    match ty {
        CgTy::Unit | CgTy::Bool | CgTy::Char | CgTy::Int(_) | CgTy::Float(_) => false,
        // A borrow owns nothing; a box and a raw pointer are handled by the
        // type that contains them, and a bare `Box of T` needs
        // `science_box_free`.
        CgTy::Ptr(kind) => matches!(kind, crate::layout::PtrKind::Box),
        // A trait object's drop goes through its vtable, so it always needs
        // glue: codegen cannot see what is behind it.
        //
        // **This answer is right for an owning object and wrong for a borrowed
        // one, and `CgTy` cannot tell them apart.** `Box of any I` and
        // `borrowed any I` are both [`CgTy::Interface`] — two words, same
        // layout, same ABI class, same niche — and the model erases exactly the
        // distinction `PtrKind` keeps for thin pointers, where the arm above
        // answers yes for `Box` and no for `Borrow`. A `borrowed any Summarize`
        // that reached here would be given drop glue it must not have.
        //
        // **It is written down rather than fixed, and the reason is that it has
        // no caller.** There is no `Ty -> CgTy` lowering in this crate — the
        // crate note's §2 says so and says what is missing — so nothing can
        // construct this arm's input from a program, and a distinction added
        // ahead of its consumer is a distinction no test can hold to account.
        // The defect is also not new and not a coercion's: `borrowed any
        // Summarize` is a record field in `examples/08_dyn_dispatch.science`
        // and has been since that file was written, and `Box of any Summarize`
        // was a type four signatures there could name before anything coerced
        // into one.
        //
        // **What closes it** is one more distinction in [`CgTy::Interface`] —
        // owning or borrowed, exactly as [`crate::layout::PtrKind`] draws it —
        // taken together with the lowering that would first need it. That is
        // `codegen-and-linking.md` Decision 42's model to change, not this
        // function's, and this comment exists so that whoever writes the
        // lowering meets the question before the bug.
        CgTy::Interface => true,
        CgTy::Nullable(inner) => needs_drop(inner, has_drop_impl),
        CgTy::Array { elem, len } => *len > 0 && needs_drop(elem, has_drop_impl),
        CgTy::Struct { name, fields } => {
            has_drop_impl(name) || fields.iter().any(|f| needs_drop(&f.ty, has_drop_impl))
        }
        CgTy::Choice { name, variants } => {
            has_drop_impl(name)
                || variants
                    .iter()
                    .any(|v| v.payload.as_ref().is_some_and(|ty| needs_drop(ty, has_drop_impl)))
        }
    }
}

/// Build the descriptor for an element type.
pub fn type_info(
    target: Triple,
    ty: &CgTy,
    drop_glue: Option<String>,
) -> TypeInfo {
    let layout = layout_of(target, ty);
    TypeInfo { size: layout.size, align: layout.align, drop_fn: drop_glue }
}

/// Every descriptor a module emits, at most one per monomorphisation key.
///
/// A `BTreeMap` for Decision 4's reason: emission order is sorted by symbol, and
/// a `HashMap` here would reopen Gate J's hazard the moment anything iterated.
#[derive(Debug, Clone, Default)]
pub struct DescriptorTable {
    type_infos: BTreeMap<String, TypeInfo>,
    map_infos: BTreeMap<String, MapInfo>,
}

impl DescriptorTable {
    /// An empty table.
    pub fn new() -> DescriptorTable {
        DescriptorTable::default()
    }

    /// The symbol a descriptor for `key` is emitted under.
    ///
    /// Derived from the mangled name so that the *"keyed by the monomorphisation
    /// key of §2.7"* half of Decision 20 is true by construction rather than by
    /// a second keying scheme that could disagree with the first.
    pub fn symbol_for(key: &MonoKey) -> String {
        format!("{}.typeinfo", mangle(key))
    }

    /// The symbol a `ScienceMapInfo` for `key` is emitted under.
    pub fn map_symbol_for(key: &MonoKey) -> String {
        format!("{}.mapinfo", mangle(key))
    }

    /// Record a descriptor, returning the symbol every call site must use.
    ///
    /// Recording the same key twice returns the same symbol and emits nothing
    /// new, which is Decision 20's *"never constructed twice"*.
    pub fn intern(&mut self, key: &MonoKey, info: TypeInfo) -> String {
        let symbol = DescriptorTable::symbol_for(key);
        self.type_infos.entry(symbol.clone()).or_insert(info);
        symbol
    }

    /// Record a map descriptor.
    pub fn intern_map(&mut self, key: &MonoKey, info: MapInfo) -> String {
        let symbol = DescriptorTable::map_symbol_for(key);
        self.map_infos.entry(symbol.clone()).or_insert(info);
        symbol
    }

    /// Every `ScienceTypeInfo`, in emission order.
    pub fn type_infos(&self) -> impl Iterator<Item = (&String, &TypeInfo)> {
        self.type_infos.iter()
    }

    /// Every `ScienceMapInfo`, in emission order.
    pub fn map_infos(&self) -> impl Iterator<Item = (&String, &MapInfo)> {
        self.map_infos.iter()
    }

    /// How many distinct descriptors the module holds.
    ///
    /// The number a test asserts against the count of distinct mono keys, which
    /// is what makes the runtime's unchecked precondition checkable.
    pub fn len(&self) -> usize {
        self.type_infos.len() + self.map_infos.len()
    }

    /// Whether the module emits no descriptors.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// How a string literal is lowered (Decision 15).
///
/// **The decision.** A string literal lowers to
/// `call science_string_from_bytes(@.str.N, len)` against a
/// `private unnamed_addr constant [N x i8]`. **Every evaluation of the literal
/// allocates.**
///
/// **The reason, and it is a finding rather than a preference.** The obvious
/// optimisation is to emit a static `ScienceString` — `{ ptr: @.str.N, len: N,
/// cap: 0 }` — pointing at read-only data, and rely on
/// `science_string_free` calling `science_dealloc(ptr, cap, 1)` with
/// `size == 0`, which is documented as a no-op. Freeing it would be harmless.
/// It looks free.
///
/// It is unsound, and the runtime's contract page does not say so.
/// `ScienceString` carries the invariant `cap == 0 => len == 0`, established by
/// `ScienceString::empty()` and relied on by the internal `reserve`: growing
/// from `cap == 0` routes through `science_realloc` with `old_size == 0`, which
/// is documented as *"nothing was allocated, so there is nothing to copy or
/// release"* — it allocates fresh and **copies nothing**. A static literal with
/// `len > 0, cap == 0` would survive `free`, survive `len`, survive `print`, and
/// then, on the first `push_str`, silently lose its first `len` bytes to
/// uninitialised memory.
///
/// **The cost.** A literal in a loop allocates on every iteration. That is a
/// real performance defect in exactly the place a scientific program writes
/// formatted output, and the fix is a runtime change: either a
/// `science_string_from_static` returning a String the runtime will not free, or
/// a `reserve` that copies when `cap == 0 && len > 0`, plus the invariant
/// written on the contract page either way.
///
/// **This type has one constructor and no alternative.** The unsound
/// representation is the one an optimising author reaches for on purpose, so
/// the way to prevent it is not a comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringLiteral {
    /// The constant's symbol: `.str.N`.
    pub bytes_symbol: String,
    /// The bytes. Not NUL-terminated: `ScienceString` carries a length.
    pub bytes: Vec<u8>,
}

impl StringLiteral {
    /// The `N`th string literal in a module.
    pub fn new(index: usize, text: &str) -> StringLiteral {
        StringLiteral { bytes_symbol: format!(".str.{index}"), bytes: text.as_bytes().to_vec() }
    }

    /// The length that goes in the `science_string_from_bytes` call. Bytes, not
    /// characters.
    pub fn len(&self) -> u64 {
        self.bytes.len() as u64
    }

    /// Whether the literal is empty. An empty literal still goes through
    /// `science_string_from_bytes`, which returns the empty string and
    /// allocates nothing.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// The runtime symbol a literal's construction calls. There is one.
    pub fn constructor(&self) -> &'static str {
        "science_string_from_bytes"
    }
}

/// The drop glue a type needs, or `None`.
///
/// Decision 12: an emitted `internal` function per monomorphised type, whose
/// body drops fields in reverse declaration order and then calls the type's own
/// `Drop` implementation if it has one. For the library types the glue is a
/// single runtime call, and **the descriptor is the second argument for `Array`
/// and `Map` and the first for `Box`** — §9.3's finding 4, which is not
/// derivable from the runtime's contract page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DropGlue {
    /// One runtime call: `science_string_free(%p)`,
    /// `science_array_free(%p, @typeinfo.T)`,
    /// `science_map_free(%p, @mapinfo.KV)`, `science_box_free(@typeinfo.T, %p)`.
    RuntimeCall {
        /// The symbol.
        symbol: &'static str,
        /// Where the descriptor goes, or `None` when the call takes none.
        descriptor_index: Option<usize>,
    },
    /// An emitted function that drops fields in reverse declaration order.
    Emitted {
        /// The glue function's symbol.
        symbol: String,
        /// Field indices to drop, in reverse declaration order.
        fields: Vec<usize>,
    },
}

/// The drop glue for a layout, given the type it came from.
///
/// Returns `None` when the type needs none, which is the case Decision 20's
/// `drop_fn: null` exists for.
pub fn drop_glue(
    ty: &CgTy,
    layout: &Layout,
    key: &MonoKey,
    has_drop_impl: &dyn Fn(&str) -> bool,
) -> Option<DropGlue> {
    if !needs_drop(ty, has_drop_impl) {
        return None;
    }
    if let CgTy::Ptr(crate::layout::PtrKind::Box) = ty {
        return Some(DropGlue::RuntimeCall {
            symbol: "science_box_free",
            descriptor_index: Some(0),
        });
    }
    let fields = match &layout.repr {
        Repr::Aggregate { fields } => (0..fields.len()).rev().collect(),
        _ => Vec::new(),
    };
    Some(DropGlue::Emitted { symbol: format!("{}.drop", mangle(key)), fields })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Field, FloatTy, IntTy, PtrKind, Variant};

    const T: Triple = Triple::X86_64LinuxGnu;

    fn no_drop_impls(_: &str) -> bool {
        false
    }

    #[test]
    fn an_array_of_f64_gets_a_null_drop_fn_and_that_is_a_performance_decision() {
        // The hottest container in the language. A do-nothing glue function
        // here would be correct and quietly slower on every free.
        assert!(!needs_drop(&CgTy::Float(FloatTy::F64), &no_drop_impls));
        let info = type_info(T, &CgTy::Float(FloatTy::F64), None);
        assert_eq!(info, TypeInfo { size: 8, align: 8, drop_fn: None });
    }

    #[test]
    fn a_box_owns_and_a_borrow_does_not() {
        assert!(needs_drop(&CgTy::Ptr(PtrKind::Box), &no_drop_impls));
        assert!(!needs_drop(&CgTy::Ptr(PtrKind::Borrow), &no_drop_impls));
        assert!(!needs_drop(&CgTy::Ptr(PtrKind::MutBorrow), &no_drop_impls));
        assert!(!needs_drop(&CgTy::Ptr(PtrKind::Fn), &no_drop_impls));
    }

    #[test]
    fn ownership_is_transitive_through_aggregates_and_nullables() {
        let owning = CgTy::strukt(
            "Node",
            vec![
                Field::new("tag", CgTy::Int(IntTy::I64)),
                Field::new("next", CgTy::nullable(CgTy::Ptr(PtrKind::Box))),
            ],
        );
        assert!(needs_drop(&owning, &no_drop_impls));

        let plain = CgTy::strukt(
            "Point",
            vec![
                Field::new("x", CgTy::Float(FloatTy::F64)),
                Field::new("y", CgTy::Float(FloatTy::F64)),
            ],
        );
        assert!(!needs_drop(&plain, &no_drop_impls));

        let choice = CgTy::choice(
            "Maybe",
            vec![Variant::unit("none"), Variant::with("some", CgTy::Ptr(PtrKind::Box))],
        );
        assert!(needs_drop(&choice, &no_drop_impls));
    }

    #[test]
    fn a_user_drop_implementation_makes_an_otherwise_plain_type_own() {
        let plain = CgTy::strukt("Handle", vec![Field::new("fd", CgTy::Int(IntTy::I32))]);
        assert!(!needs_drop(&plain, &no_drop_impls));
        assert!(needs_drop(&plain, &|name| name == "Handle"));
    }

    /// **`always` is this model's word, not the language's.** The arm's comment
    /// says why: `Box of any I` and `borrowed any I` are one [`CgTy`], the
    /// answer here is the owning one, and the borrowed one is wrong. Pinning it
    /// keeps the wrong answer from being taken as an oversight and keeps the
    /// day it is fixed to a line in a diff.
    #[test]
    fn a_trait_object_always_needs_glue_because_codegen_cannot_see_behind_it() {
        assert!(needs_drop(&CgTy::Interface, &no_drop_impls));
    }

    #[test]
    fn a_zero_length_array_of_an_owning_type_needs_no_glue() {
        assert!(!needs_drop(&CgTy::array(CgTy::Ptr(PtrKind::Box), 0), &no_drop_impls));
        assert!(needs_drop(&CgTy::array(CgTy::Ptr(PtrKind::Box), 1), &no_drop_impls));
    }

    #[test]
    fn a_descriptor_is_interned_once_per_key() {
        let mut table = DescriptorTable::new();
        let key = MonoKey::generic(
            &["Array"],
            vec![crate::mangle::MonoArg::Ty(CgTy::Float(FloatTy::F64))],
        );
        let first = table.intern(&key, type_info(T, &CgTy::Float(FloatTy::F64), None));
        let second = table.intern(&key, type_info(T, &CgTy::Float(FloatTy::F64), None));
        assert_eq!(first, second, "every call site loads the address of that one global");
        assert_eq!(table.len(), 1, "never constructed twice");
    }

    #[test]
    fn two_element_types_get_two_descriptors() {
        let mut table = DescriptorTable::new();
        table.intern(
            &MonoKey::generic(&["Array"], vec![crate::mangle::MonoArg::Ty(CgTy::Float(FloatTy::F64))]),
            type_info(T, &CgTy::Float(FloatTy::F64), None),
        );
        table.intern(
            &MonoKey::generic(&["Array"], vec![crate::mangle::MonoArg::Ty(CgTy::Int(IntTy::I64))]),
            type_info(T, &CgTy::Int(IntTy::I64), None),
        );
        assert_eq!(table.len(), 2);
        // Emission order is sorted, which is Decision 4.
        let symbols: Vec<&String> = table.type_infos().map(|(s, _)| s).collect();
        let mut sorted = symbols.clone();
        sorted.sort();
        assert_eq!(symbols, sorted);
    }

    #[test]
    fn a_descriptor_symbol_is_derived_from_the_mono_key() {
        let key = MonoKey::generic(
            &["Array"],
            vec![crate::mangle::MonoArg::Ty(CgTy::Float(FloatTy::F64))],
        );
        assert_eq!(DescriptorTable::symbol_for(&key), "_S5ArrayEd.typeinfo");
        assert_ne!(DescriptorTable::symbol_for(&key), DescriptorTable::map_symbol_for(&key));
    }

    #[test]
    fn a_string_literal_always_goes_through_the_runtime() {
        // Decision 15. There is no second constructor, and that is the test:
        // the unsound static form has no way to be spelled.
        let literal = StringLiteral::new(0, "hello, world");
        assert_eq!(literal.constructor(), "science_string_from_bytes");
        assert_eq!(literal.len(), 12);
        assert_eq!(literal.bytes_symbol, ".str.0");
        // Not NUL-terminated: the length travels with the pointer.
        assert!(!literal.bytes.contains(&0));
    }

    #[test]
    fn a_literals_length_is_in_bytes_and_not_characters() {
        let literal = StringLiteral::new(1, "é");
        assert_eq!(literal.len(), 2);
    }

    #[test]
    fn box_drop_glue_puts_the_descriptor_first() {
        let ty = CgTy::Ptr(PtrKind::Box);
        let layout = layout_of(T, &ty);
        let key = MonoKey::plain(&["b"]);
        assert_eq!(
            drop_glue(&ty, &layout, &key, &no_drop_impls),
            Some(DropGlue::RuntimeCall { symbol: "science_box_free", descriptor_index: Some(0) })
        );
    }

    #[test]
    fn an_aggregate_drops_its_fields_in_reverse_declaration_order() {
        let ty = CgTy::strukt(
            "Two",
            vec![
                Field::new("a", CgTy::Ptr(PtrKind::Box)),
                Field::new("b", CgTy::Ptr(PtrKind::Box)),
            ],
        );
        let layout = layout_of(T, &ty);
        match drop_glue(&ty, &layout, &MonoKey::plain(&["Two"]), &no_drop_impls) {
            Some(DropGlue::Emitted { fields, .. }) => assert_eq!(fields, vec![1, 0]),
            other => panic!("expected emitted glue, got {other:?}"),
        }
    }

    #[test]
    fn a_type_that_owns_nothing_gets_no_glue_function_and_no_call() {
        let ty = CgTy::Float(FloatTy::F64);
        let layout = layout_of(T, &ty);
        assert_eq!(drop_glue(&ty, &layout, &MonoKey::plain(&["f"]), &no_drop_impls), None);
    }
}
