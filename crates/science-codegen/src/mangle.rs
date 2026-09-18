//! Symbol names: Decision 16, and the `SC0404` trap-door that checks them.
//!
//! **The decision.** Mangled names are `_S` followed by length-prefixed path
//! components, followed by a length-prefixed encoding of the type and const
//! arguments in the normal form of `type-checking-and-mir.md` §8 item 4. **No
//! hashes. Names are not shortened.**
//!
//! **The reason for no hashes.** `package-manager.md` Decision 7 removes
//! timestamps, hostnames and absolute paths from output because they are
//! reproducibility hazards. A hash of a path is the same hazard wearing a
//! disguise: it is stable only if the path is, and the path is the thing that is
//! not. A length-prefixed encoding is longer, uglier, and deterministic from the
//! source alone.
//!
//! **The reason for keying on the normal form** rather than the written syntax:
//! `Matrix of (T, a * b)` and `Matrix of (T, b * a)` are one instantiation, and
//! keying on syntax emits two symbols for one function. `type-checking-and-mir.md`
//! §15 calls that *"a correctness trap with a late failure"* — it is not a type
//! error, not a link error on most platforms, and shows up as a performance
//! mystery or a pointer-equality failure long after the cause.
//!
//! **The cost.** Symbol names in the tens to low hundreds of characters, an `nm`
//! dump that is unpleasant to read, and no demangler in F0. `sciencec demangle`
//! is in §14's not-built list and should be roughly fifty lines whenever
//! somebody wants it.
//!
//! # Why length prefixes rather than separators
//!
//! A separator has to be a character that cannot appear in a component, and
//! Science identifiers plus the type grammar leave no such character that is
//! also legal in every object format's symbol table. A length prefix needs no
//! reserved character, is unambiguous under concatenation, and is what Itanium
//! C++ and Rust's own `v0` scheme both landed on. The cost is that a human
//! reading a symbol has to count.
//!
//! # What this module deliberately does not do
//!
//! It does not walk anything. Decision 42 puts the monomorphisation walk above
//! the backend line and so above this crate's backend half, but the walk
//! consumes MIR, which does not exist. What exists here is the *encoding* and
//! the *collision check*, which are the two halves that a walk would call.

use std::collections::BTreeMap;

use science_diagnostics::Diagnostic;

use crate::diagnostics::symbol_collision;
use crate::layout::{CgTy, FloatTy, IntTy, PtrKind};

/// One argument of a monomorphisation key.
///
/// A key is a path plus a list of these. `type-checking-and-mir.md` §8 item 4
/// fixes the normal form for the const case — `k + Σ cᵢ·aᵢ` — and this type
/// carries the *already normalised* value, not the expression. Normalising here
/// would be a second normaliser, and two normalisers that disagree is precisely
/// the failure §15 warns about.
/// A key is ordered by its mangled name and by nothing else, so [`MonoArg`]
/// needs no ordering of its own: [`SymbolTable`] is keyed by the symbol, which
/// is a `String`, which is what Decision 4's sorted emission order sorts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonoArg {
    /// A type argument.
    Ty(CgTy),
    /// A const generic argument, already in normal form and evaluated.
    Const(i128),
}

/// A monomorphisation key: what identifies one emitted item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonoKey {
    /// The path, outermost first: `["mymod", "Matrix", "transpose"]`.
    pub path: Vec<String>,
    /// The type and const arguments, in declaration order.
    pub args: Vec<MonoArg>,
}

impl MonoKey {
    /// A key with no generic arguments.
    pub fn plain(path: &[&str]) -> MonoKey {
        MonoKey { path: path.iter().map(|s| s.to_string()).collect(), args: Vec::new() }
    }

    /// A key with arguments.
    pub fn generic(path: &[&str], args: Vec<MonoArg>) -> MonoKey {
        MonoKey { path: path.iter().map(|s| s.to_string()).collect(), args }
    }

    /// How the key reads in a diagnostic. Not a symbol name.
    pub fn describe(&self) -> String {
        let path = self.path.join("::");
        if self.args.is_empty() {
            return path;
        }
        let args: Vec<String> = self
            .args
            .iter()
            .map(|arg| match arg {
                MonoArg::Ty(ty) => describe_ty(ty),
                MonoArg::Const(value) => value.to_string(),
            })
            .collect();
        format!("{path}[{}]", args.join(", "))
    }
}

/// Mangle a monomorphisation key into a symbol name (Decision 16).
///
/// The grammar:
///
/// ```text
/// symbol    ::= "_S" component+ ( "E" argument+ )?
/// component ::= length name
/// argument  ::= type | "K" length digits
/// ```
///
/// `E` separates the path from the arguments and is unambiguous because a
/// length prefix always begins with a digit and `E` is not one.
pub fn mangle(key: &MonoKey) -> String {
    let encoded: Vec<String> = key
        .args
        .iter()
        .map(|arg| match arg {
            MonoArg::Ty(ty) => encode_ty(ty),
            MonoArg::Const(value) => encode_const(*value),
        })
        .collect();
    assemble(&key.path, &encoded)
}

/// The grammar of Decision 16, assembled from pieces somebody else encoded.
///
/// **The decision.** `_S`, the length prefixes and the `E` separator live in
/// exactly one function, and everything that produces a Science symbol calls
/// it. The *type* encoding is a parameter.
///
/// **The reason.** There are two type encodings in this crate and there have to
/// be: [`encode_ty`] encodes a [`CgTy`], which is the layout model and is
/// deliberately lossy — [`CgTy::Ptr`] carries no pointee and [`CgTy::Interface`]
/// carries no interface — while [`crate::mono`] encodes the checker's `Ty`,
/// where `Box of Int` and `Box of String` have to be two symbols. `mono`'s §4
/// is why the lossy one cannot be the mangler's. What must *not* be duplicated
/// is the frame around them: two spellings of the length prefix is two symbol
/// schemes, and a linker would tell you about it a stage too late.
///
/// **The cost.** One indirection, and a caller that can pass a nonsense
/// argument string. The argument encoding is checked by the encoders' own
/// tests; this function checks nothing, because there is nothing here to check
/// that is not already a property of its inputs.
pub fn assemble(path: &[String], encoded_args: &[String]) -> String {
    let mut out = String::from("_S");
    for component in path {
        out.push_str(&component.len().to_string());
        out.push_str(component);
    }
    if !encoded_args.is_empty() {
        out.push('E');
        for arg in encoded_args {
            out.push_str(arg);
        }
    }
    out
}

/// A const argument, length-prefixed.
///
/// Negative const arguments exist — `const SHIFT be -1` is legal — and a minus
/// sign inside a length-prefixed field is fine because the length counts bytes.
pub fn encode_const(value: i128) -> String {
    let digits = value.to_string();
    format!("K{}{}", digits.len(), digits)
}

/// Encode a [`CgTy`] for a symbol name.
///
/// One-letter tags for the scalars and a length-prefixed name for anything
/// nominal. The encoding is *structural* for the anonymous shapes and *nominal*
/// for the named ones, which is the right way round: two records with the same
/// field types are different types and must mangle differently, and two
/// `Array of F64` are the same type wherever they were written.
///
/// # This is not injective over Science types, and that is why it is not the
/// # monomorphiser's encoder
///
/// A [`CgTy`] is the *layout* model, and [`layout::CgTy`]'s own documentation
/// says what it keeps: *"the set of distinctions that change a layout or an ABI
/// classification, and nothing else"*. Two of those erasures are visible from
/// here:
///
/// - [`CgTy::Ptr`] carries a [`PtrKind`] and no pointee, so `Box of Int` and
///   `Box of String` both encode `Pb`.
/// - [`CgTy::Interface`] carries no interface, so `any Summarize` and
///   `any Report` both encode `D`.
///
/// Both are right for layout — a pointer is a word whatever it points at — and
/// both are wrong for a symbol name. `f of (Box of Int)` and
/// `f of (Box of String)` are two instantiations that must be two symbols, and
/// through this function they are one, which is `SC0404` raised against a
/// program that has nothing wrong with it.
///
/// **So the monomorphisation walk does not use this function.**
/// [`crate::mono`] encodes the checker's `Ty` instead and shares only
/// [`assemble`]; `mono`'s §4 is the whole argument and `tests/mono.rs` is the
/// test that `Box of Int` and `Box of String` come out different there.
///
/// What this function is still for is the items whose key really is a `CgTy`:
/// the type-info descriptors of Decision 20 and the drop glue of Decision 12,
/// neither of which is emitted yet. When they are, they inherit this defect,
/// and the fix is the same one `mono` took.
///
/// [`layout::CgTy`]: crate::layout::CgTy
fn encode_ty(ty: &CgTy) -> String {
    match ty {
        CgTy::Unit => "u".to_string(),
        CgTy::Bool => "b".to_string(),
        CgTy::Char => "c".to_string(),
        CgTy::Int(int) => match int {
            IntTy::I8 => "a",
            IntTy::I16 => "s",
            IntTy::I32 => "i",
            IntTy::I64 => "l",
            IntTy::U8 => "h",
            IntTy::U16 => "t",
            IntTy::U32 => "j",
            IntTy::U64 => "m",
            IntTy::Usize => "z",
            IntTy::Isize => "y",
        }
        .to_string(),
        // Itanium spells `_Float16` `Dh` and `__bf16` `DF16b`; both are used
        // here verbatim rather than invented, so that a symbol this compiler
        // emits demangles to the type the author wrote in any tool that knows
        // the Itanium grammar.
        CgTy::Float(FloatTy::F16) => "Dh".to_string(),
        CgTy::Float(FloatTy::Bf16) => "DF16b".to_string(),
        CgTy::Float(FloatTy::F32) => "f".to_string(),
        CgTy::Float(FloatTy::F64) => "d".to_string(),
        CgTy::Ptr(kind) => match kind {
            PtrKind::Box => "Pb",
            PtrKind::Borrow => "Pr",
            PtrKind::MutBorrow => "Pm",
            PtrKind::Fn => "Pf",
            PtrKind::Raw => "Pp",
        }
        .to_string(),
        CgTy::Interface => "D".to_string(),
        CgTy::Nullable(inner) => format!("N{}", encode_ty(inner)),
        CgTy::Array { elem, len } => format!("A{len}_{}", encode_ty(elem)),
        // Nominal: the name, length-prefixed. The field types are *not*
        // encoded, because a record is identified by its path and two records
        // with the same fields are different types.
        CgTy::Struct { name, .. } => format!("S{}{name}", name.len()),
        CgTy::Choice { name, .. } => format!("C{}{name}", name.len()),
    }
}

fn describe_ty(ty: &CgTy) -> String {
    match ty {
        CgTy::Unit => "()".to_string(),
        CgTy::Bool => "Bool".to_string(),
        CgTy::Char => "Char".to_string(),
        CgTy::Int(int) => format!("{int:?}"),
        CgTy::Float(float) => format!("{float:?}"),
        CgTy::Ptr(kind) => format!("{kind:?} pointer"),
        CgTy::Interface => "any".to_string(),
        CgTy::Nullable(inner) => format!("{}?", describe_ty(inner)),
        CgTy::Array { elem, len } => format!("[{}; {len}]", describe_ty(elem)),
        CgTy::Struct { name, .. } | CgTy::Choice { name, .. } => name.clone(),
    }
}

/// The module's symbol table, and the `SC0404` check.
///
/// **Decision 4's ordering lives here.** *"Every monomorphised item is emitted
/// in sorted order of its mangled symbol name."* `self-hosting.md` §15 names
/// hash-map iteration order inside the compiler as the unknown source of
/// non-determinism behind Gate J; a `BTreeMap` keyed by the mangled name closes
/// it **by construction rather than by testing**, and [`SymbolTable::emission_order`]
/// is the three lines that spend it.
#[derive(Debug, Clone, Default)]
pub struct SymbolTable {
    entries: BTreeMap<String, MonoKey>,
}

impl SymbolTable {
    /// An empty table.
    pub fn new() -> SymbolTable {
        SymbolTable::default()
    }

    /// Record an item, returning `SC0404` if a *different* key already mangled
    /// to the same symbol.
    ///
    /// Recording the same key twice is not an error: a mono walk reaching one
    /// instantiation by two paths is ordinary and the second record is a
    /// no-op. That distinction is the whole value of storing the key rather
    /// than a count.
    pub fn insert(&mut self, key: MonoKey) -> Result<&str, Diagnostic> {
        let symbol = mangle(&key);
        match self.entries.get(&symbol) {
            Some(existing) if *existing == key => {
                Ok(self.entries.get_key_value(&symbol).unwrap().0)
            }
            Some(existing) => Err(symbol_collision(
                &symbol,
                &existing.describe(),
                &key.describe(),
            )),
            None => {
                self.entries.insert(symbol.clone(), key);
                Ok(self.entries.get_key_value(&symbol).unwrap().0)
            }
        }
    }

    /// Every symbol, in the order Decision 4 emits them: sorted by mangled
    /// name.
    pub fn emission_order(&self) -> impl Iterator<Item = (&String, &MonoKey)> {
        self.entries.iter()
    }

    /// How many items are in the module.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the module is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Field, Variant};

    #[test]
    fn a_plain_path_mangles_to_length_prefixed_components() {
        assert_eq!(mangle(&MonoKey::plain(&["main"])), "_S4main");
        assert_eq!(mangle(&MonoKey::plain(&["stats", "mean"])), "_S5stats4mean");
    }

    #[test]
    fn there_is_no_hash_anywhere_in_a_symbol() {
        // The whole of "no hashes", as a property rather than an inspection:
        // the symbol is a function of the key's text and nothing else, so two
        // processes, two directories and two machines produce the same bytes.
        let key = MonoKey::generic(
            &["m", "f"],
            vec![MonoArg::Ty(CgTy::Int(IntTy::I64)), MonoArg::Const(768)],
        );
        let once = mangle(&key);
        let twice = mangle(&key.clone());
        assert_eq!(once, twice);
        assert_eq!(once, "_S1m1fElK3768");
    }

    #[test]
    fn a_cgty_pointer_is_not_injective_and_that_is_why_mono_does_not_use_it() {
        // The defect [`encode_ty`] documents, as a fact a test holds rather
        // than a sentence a reader has to notice. `CgTy::Ptr` carries a kind
        // and no pointee, so `f of (Box of Int)` and `f of (Box of String)`
        // are one symbol through this encoder — `SC0404` raised against a
        // program with nothing wrong with it.
        //
        // If this ever *fails*, `CgTy` has grown a pointee and `crate::mono`'s
        // §4 should be re-read: the second encoder may no longer be needed.
        let boxed = MonoKey::generic(&["f"], vec![MonoArg::Ty(CgTy::Ptr(PtrKind::Box))]);
        assert_eq!(mangle(&boxed), "_S1fEPb");
        let summarize = MonoKey::generic(&["g"], vec![MonoArg::Ty(CgTy::Interface)]);
        assert_eq!(mangle(&summarize), "_S1gED");
        // `crate::mono` encodes the checker's `Ty` instead, and
        // `tests/mono.rs::two_pointer_instantiations_are_two_symbols` is the
        // other half of this pair.
    }

    #[test]
    fn the_grammar_is_assembled_in_one_place() {
        // `assemble` is what the two type encoders share, and sharing it is
        // what keeps there from being two symbol schemes.
        assert_eq!(assemble(&["f".to_string()], &[]), "_S1f");
        assert_eq!(assemble(&["f".to_string()], &["Z".to_string()]), "_S1fEZ");
        assert_eq!(encode_const(-1), "K2-1");
        assert_eq!(encode_const(768), "K3768");
    }

    #[test]
    fn a_negative_const_argument_survives_the_length_prefix() {
        let key = MonoKey::generic(&["f"], vec![MonoArg::Const(-1)]);
        assert_eq!(mangle(&key), "_S1fEK2-1");
    }

    #[test]
    fn two_records_with_the_same_fields_mangle_differently() {
        // Nominal, not structural. A structural encoding would emit one symbol
        // for two types and the linker would pick one.
        let a = CgTy::strukt("Metres", vec![Field::new("v", CgTy::Float(FloatTy::F64))]);
        let b = CgTy::strukt("Feet", vec![Field::new("v", CgTy::Float(FloatTy::F64))]);
        assert_ne!(
            mangle(&MonoKey::generic(&["f"], vec![MonoArg::Ty(a)])),
            mangle(&MonoKey::generic(&["f"], vec![MonoArg::Ty(b)]))
        );
    }

    #[test]
    fn a_record_and_a_choice_of_the_same_name_do_not_collide() {
        let record = CgTy::strukt("X", vec![]);
        let choice = CgTy::choice("X", vec![Variant::unit("a")]);
        assert_ne!(
            mangle(&MonoKey::generic(&["f"], vec![MonoArg::Ty(record)])),
            mangle(&MonoKey::generic(&["f"], vec![MonoArg::Ty(choice)]))
        );
    }

    #[test]
    fn every_scalar_has_a_distinct_tag() {
        let mut tags: Vec<String> = Vec::new();
        for int in [
            IntTy::I8, IntTy::I16, IntTy::I32, IntTy::I64,
            IntTy::U8, IntTy::U16, IntTy::U32, IntTy::U64,
            IntTy::Usize, IntTy::Isize,
        ] {
            tags.push(encode_ty(&CgTy::Int(int)));
        }
        for ty in [CgTy::Unit, CgTy::Bool, CgTy::Char, CgTy::Interface] {
            tags.push(encode_ty(&ty));
        }
        tags.push(encode_ty(&CgTy::Float(FloatTy::F32)));
        tags.push(encode_ty(&CgTy::Float(FloatTy::F64)));
        for kind in [PtrKind::Box, PtrKind::Borrow, PtrKind::MutBorrow, PtrKind::Fn, PtrKind::Raw] {
            tags.push(encode_ty(&CgTy::Ptr(kind)));
        }
        let count = tags.len();
        tags.sort();
        tags.dedup();
        assert_eq!(tags.len(), count, "two scalar types share a mangling tag");
        // `usize` and `i64` in particular: finding 3 again, on the symbol side.
        assert_ne!(encode_ty(&CgTy::Int(IntTy::Usize)), encode_ty(&CgTy::Int(IntTy::U64)));
    }

    #[test]
    fn nesting_is_unambiguous() {
        // `[(Box of T)?; 3]` and `([Box of T; 3])?` must not collide.
        let a = CgTy::array(CgTy::nullable(CgTy::Ptr(PtrKind::Box)), 3);
        let b = CgTy::nullable(CgTy::array(CgTy::Ptr(PtrKind::Box), 3));
        assert_ne!(encode_ty(&a), encode_ty(&b));
    }

    #[test]
    fn an_array_length_cannot_run_into_the_element_tag() {
        // `A12_l` is an array of 12; `A1` followed by `2_l` would be an array
        // of 1 of something else. The `_` is what makes the length terminable.
        assert_eq!(encode_ty(&CgTy::array(CgTy::Int(IntTy::I64), 12)), "A12_l");
        assert_ne!(
            encode_ty(&CgTy::array(CgTy::Int(IntTy::I64), 1)),
            encode_ty(&CgTy::array(CgTy::Int(IntTy::I64), 12))
        );
    }

    #[test]
    fn the_same_key_twice_is_not_a_collision() {
        let mut table = SymbolTable::new();
        let key = MonoKey::generic(&["f"], vec![MonoArg::Ty(CgTy::Int(IntTy::I64))]);
        assert!(table.insert(key.clone()).is_ok());
        assert!(table.insert(key).is_ok());
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn two_keys_one_symbol_is_sc0404() {
        // A genuine collision needs two distinct keys with one mangled name,
        // and the encoding is designed so that no such pair exists — the
        // length prefixes make it unambiguous. So the check cannot be reached
        // through the public API today, which is the correct state of affairs
        // and also means the check would rot untested.
        //
        // It is exercised through the private map instead. That is not
        // cheating: `SC0404` exists for the day somebody changes the encoding
        // and gets it wrong, and this test is what tells them the trap-door
        // still works when they do.
        let mut table = SymbolTable::new();
        table.entries.insert("_S1h".to_string(), MonoKey::plain(&["other"]));
        let error = table.insert(MonoKey::plain(&["h"])).unwrap_err();
        assert_eq!(error.code, crate::diagnostics::code::SC0404);
        assert!(error.notes.iter().any(|n| n.contains("other")));
        assert!(error.notes.iter().any(|n| n.contains("bug in the compiler")));
    }

    #[test]
    fn emission_order_is_sorted_by_symbol_and_not_by_insertion() {
        let mut table = SymbolTable::new();
        for name in ["zeta", "alpha", "mu"] {
            table.insert(MonoKey::plain(&[name])).unwrap();
        }
        let order: Vec<&String> = table.emission_order().map(|(s, _)| s).collect();
        let mut sorted = order.clone();
        sorted.sort();
        assert_eq!(order, sorted, "Decision 4's ordering is what closes Gate J's hazard");
    }

    #[test]
    fn describe_reads_like_source_and_mangle_does_not() {
        let key = MonoKey::generic(
            &["linalg", "Matrix", "transpose"],
            vec![MonoArg::Ty(CgTy::Float(FloatTy::F64)), MonoArg::Const(4)],
        );
        assert_eq!(key.describe(), "linalg::Matrix::transpose[F64, 4]");
        assert_eq!(mangle(&key), "_S6linalg6Matrix9transposeEdK14");
    }
}
