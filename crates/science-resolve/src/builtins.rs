//! The prelude: the names §5.1 and §8 say exist before any file is read.
//!
//! Without this, `def f(a: borrowed String)` reports an unresolved name, and every
//! later phase would have to special-case a handful of strings — exactly the
//! string lookup the HIR exists to abolish. So the primitives, the library
//! types, their variants, the compiler-known traits and the free functions all
//! get real `DefId`s, in a module of their own that no file can name.
//!
//! What is *not* here: methods. §8 lists `String.len`, `Array.push` and the
//! rest, but a method is reached through a receiver whose type this phase does
//! not know (see `ExprKind::MethodCall`), so registering them would buy
//! nothing. The type checker owns them, and it can hang them off these ids.
//!
//! Builtin definitions carry [`BUILTIN_SPAN`](crate::hir::BUILTIN_SPAN): they
//! have no source, and §9 has no room for a node without a span.

use crate::hir::{DefId, DefKind, DefTable, BUILTIN_SPAN};

/// Everything the prelude defines, ready to be merged into a module scope.
pub struct Prelude {
    /// The module the definitions hang from. It is not a child of the crate
    /// root, so no `use` can reach it and no user implementation can claim to own it —
    /// which is what makes `String implements Clone:` an orphan (§5.4).
    pub module: DefId,
    /// Name to definition, for everything but variants.
    pub names: Vec<(String, DefId)>,
    /// Unqualified variants of a choice type: `None` as well as `Option.None`
    /// (§4.5).
    pub variants: Vec<(String, DefId)>,
    /// How many payload types each variant carries, which is what decides
    /// whether a bare name in a pattern is a match or a binding (§4.4).
    pub variant_arity: Vec<(DefId, usize)>,
    /// Nested modules of the prelude and the names in each: today just `ffi`.
    /// The caller gives each one a scope of its own, which is what makes
    /// `ffi.Span` a two-segment path rather than a special case.
    pub modules: Vec<(DefId, Vec<(String, DefId)>)>,
}

/// §5.1, plus the aliases. `Never` is §4.5.
const PRIMITIVES: &[&str] = &[
    "I8", "I16", "I32", "I64", "U8", "U16", "U32", "U64", "F16", "BF16", "F32", "F64", "Bool",
    "Char", "String", "Int", "Float", "Never",
];

/// The C scalar vocabulary of `ffi-c-boundary.md` §1.3, usable inside an
/// `extern` block. They are deliberately *not* aliases of the Science
/// primitives: §1.6's whole argument is that a width the author did not think
/// about is how a numerical program gets silently wrong answers, so `CLong`
/// stays a type whose width the author has to consider.
const C_SCALARS: &[&str] = &[
    "CChar", "CInt", "CUInt", "CLong", "CULong", "CLongLong", "CULongLong", "CFloat", "CDouble",
    "CSizeT", "CPtrDiff", "CVoid",
];

/// Library types with a representation in `science-rt` but no declaration in any
/// `.science` file (§8).
const LIBRARY_TYPES: &[&str] = &["Array", "Map", "Box", "Chars", "IoError"];

/// The interfaces the compiler knows about (§5.4).
/// §5.4 lists seventeen, and the operator ones are load-bearing: "a scientific
/// language in which `+` does not work on your own type is not a scientific
/// language". Leaving them out made `Vector2 implements Add:` unresolvable in
/// three corpus files, which nothing caught until the driver ran resolution
/// over `examples/` for the first time.
const INTERFACES: &[&str] = &[
    "Add", "Sub", "Mul", "Div", "Rem", "Pow", "MatMul", "Neg", "Index", "Eq", "Ord", "Copy",
    "Clone", "Drop", "Iterate", "From", "Display",
    // `Error`, the one-method interface of revision 2 §3.4. It is in the
    // prelude and not in a module because `-> (T, Error?)` is the signature of
    // every fallible function in the language, and a name that common cannot
    // need an import. `Error?` is shorthand for `(any Error)?`; the expansion
    // is in `resolve_nullable_inner`, not here.
    "Error",
];

/// The free functions (§8).
const FUNCTIONS: &[&str] = &["print", "write", "panic", "read_file", "write_file"];

/// `ffi`, the closed vocabulary of `ffi-c-boundary.md` §1.3.
///
/// A module rather than a scatter of prelude names, because `ffi.Span` is how
/// every example in that note writes it and because a name as short as `Span`
/// in the top-level prelude would collide with the first user who wanted one.
///
/// `Complex32` and `Complex64` are here although §1.3's own table omits them:
/// §2.4 of that note already *uses* `ffi.Complex64`, `c-binding-coverage.md`
/// §4.1 measures 930 complex LAPACK and CBLAS routines behind the pair, and a
/// vocabulary that a sibling section uses and the table does not define is a
/// hole rather than a decision.
///
/// The whole module is an ask, not a fact: §10.3 of the note requests it as a
/// change to §8's closed library and says so rather than assuming it. It is
/// built here because an `extern` block whose parameter types do not resolve
/// teaches nothing, and because the set is closed — adding to it is a spec
/// change, which is exactly what a `const` list in one file makes visible.
const FFI_TYPES: &[&str] = &[
    // Pointers and views (§1.3). `Span` and `MutableSpan` carry a length that
    // does not cross the ABI; `Pointer` is nullable and of unknown validity;
    // `OpaqueHandle` is non-null and never dereferenced.
    "Span",
    "MutableSpan",
    "Pointer",
    "DevicePointer",
    "OpaqueHandle",
    "FunctionPointer",
    // Strings (§1.3). Neither converts to `String` without a copy.
    "CString",
    "CStr",
    // The out-parameter idiom (§1.3).
    "Uninitialized",
    // `c-binding-coverage.md` §4.1(1): half of LAPACK.
    "Complex32",
    "Complex64",
];

/// The marker interface of §1.4: fields in declaration order at the offsets
/// the platform C ABI gives them.
const FFI_INTERFACES: &[&str] = &["CLayout"];

/// `(choice type name, [(variant name, payload arity)])`.
///
/// Empty since revision 2 §3. `Option of T` became `T?` and `Result of (T, E)`
/// became the pair `-> (T, E?)`, which took `Some`, `None`, `Ok` and `Err`
/// with them. The table stays because the prelude will have a choice type
/// again and the machinery below is the part worth keeping; an empty table is
/// also the honest record that these four names are *free*, not merely unused.
const CHOICES: &[(&str, &[(&str, usize)])] = &[];

/// Allocates the prelude into `defs`.
pub fn build(defs: &mut DefTable) -> Prelude {
    let module = defs.alloc(DefKind::Module, "core", BUILTIN_SPAN, None);
    let mut prelude = Prelude {
        module,
        names: Vec::new(),
        variants: Vec::new(),
        variant_arity: Vec::new(),
        modules: Vec::new(),
    };

    let declare = |defs: &mut DefTable, kind: DefKind, name: &str, out: &mut Prelude| {
        let id = defs.alloc(kind, name, BUILTIN_SPAN, Some(module));
        out.names.push((name.to_string(), id));
        id
    };

    for name in PRIMITIVES.iter().chain(LIBRARY_TYPES).chain(C_SCALARS) {
        declare(defs, DefKind::Primitive, name, &mut prelude);
    }
    for name in INTERFACES {
        declare(defs, DefKind::Interface, name, &mut prelude);
    }
    for name in FUNCTIONS {
        declare(defs, DefKind::Fn, name, &mut prelude);
    }
    // `ffi` is a module of the prelude, and its own names live in its own
    // scope rather than in the prelude's.
    let ffi = declare(defs, DefKind::Module, "ffi", &mut prelude);
    let mut ffi_names = Vec::new();
    for name in FFI_TYPES {
        let id = defs.alloc(DefKind::Primitive, *name, BUILTIN_SPAN, Some(ffi));
        ffi_names.push((name.to_string(), id));
    }
    for name in FFI_INTERFACES {
        let id = defs.alloc(DefKind::Interface, *name, BUILTIN_SPAN, Some(ffi));
        ffi_names.push((name.to_string(), id));
    }
    prelude.modules.push((ffi, ffi_names));

    for (name, variants) in CHOICES {
        let choice_id = declare(defs, DefKind::Choice, name, &mut prelude);
        for (variant, arity) in *variants {
            let id = defs.alloc(DefKind::Variant, *variant, BUILTIN_SPAN, Some(choice_id));
            prelude.variants.push((variant.to_string(), id));
            prelude.variant_arity.push((id, *arity));
        }
    }

    prelude
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prelude_defines_the_primitives_of_section_5_1() {
        let mut defs = DefTable::new();
        let prelude = build(&mut defs);
        for name in ["String", "Bool", "Int", "I32", "Never"] {
            assert!(
                prelude.names.iter().any(|(n, _)| n == name),
                "`{name}` must be in the prelude"
            );
        }
    }

    #[test]
    fn the_option_and_result_variants_are_gone_and_the_names_are_free() {
        let mut defs = DefTable::new();
        let prelude = build(&mut defs);
        // Revision 2 §3 removed all four with the types that carried them.
        // This asserts they are *free*, not merely absent: a program may now
        // declare its own `Ok`, and a `Some` in a pattern is a binding.
        for name in ["Some", "None", "Ok", "Err"] {
            assert!(
                !prelude.variants.iter().any(|(n, _)| n == name),
                "`{name}` was removed with `Option` and `Result`"
            );
        }
        for name in ["Option", "Result"] {
            assert!(
                !prelude.names.iter().any(|(n, _)| n == name),
                "`{name}` was removed by revision 2 §3"
            );
        }
    }

    #[test]
    fn error_is_an_interface_in_the_prelude() {
        let mut defs = DefTable::new();
        let prelude = build(&mut defs);
        let (_, id) = prelude
            .names
            .iter()
            .find(|(n, _)| n == "Error")
            .expect("`Error` is the interface every fallible signature names");
        assert_eq!(defs.get(*id).kind, DefKind::Interface);
    }


    #[test]
    fn the_ffi_module_holds_the_vocabulary_of_section_1_3() {
        let mut defs = DefTable::new();
        let prelude = build(&mut defs);

        let (_, names) = prelude
            .modules
            .iter()
            .find(|(id, _)| defs.get(*id).name == "ffi")
            .expect("`ffi` must be a module of the prelude");

        for name in ["Span", "MutableSpan", "Pointer", "OpaqueHandle", "CStr", "CString"] {
            assert!(names.iter().any(|(n, _)| n == name), "`ffi.{name}` must exist");
        }
        // The two the parent note uses and its own table never defined.
        for name in ["Complex32", "Complex64"] {
            assert!(names.iter().any(|(n, _)| n == name), "`ffi.{name}` must exist");
        }
        // The marker interface is an interface, not a type: `implements` has
        // to reach it and a type position must not.
        let (_, layout) =
            names.iter().find(|(n, _)| n == "CLayout").expect("`ffi.CLayout` must exist");
        assert_eq!(defs.get(*layout).kind, DefKind::Interface);
    }

    #[test]
    fn the_ffi_names_are_not_also_bare_prelude_names() {
        // `Span` in the top level of the prelude would take a name a user
        // wants, which is the reason §1.3 spells every one of them `ffi.`.
        let mut defs = DefTable::new();
        let prelude = build(&mut defs);
        for name in ["Span", "Pointer", "CStr", "Complex64", "CLayout"] {
            assert!(
                !prelude.names.iter().any(|(n, _)| n == name),
                "`{name}` must be reachable only as `ffi.{name}`"
            );
        }
        assert!(prelude.names.iter().any(|(n, _)| n == "ffi"));
    }
    #[test]
    fn every_builtin_definition_is_marked_as_having_no_source() {
        let mut defs = DefTable::new();
        build(&mut defs);
        assert!(defs.iter().all(|d| d.is_builtin()));
    }
}
