//! The prelude: the names §5.1 and §8 say exist before any file is read.
//!
//! Without this, `fn f(a: &String)` reports an unresolved name, and every
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
    /// root, so no `use` can reach it and no user `impl` can claim to own it —
    /// which is what makes `impl Clone for String` an orphan (§5.4).
    pub module: DefId,
    /// Name to definition, for everything but variants.
    pub names: Vec<(String, DefId)>,
    /// Unqualified enum variants: `None` as well as `Option.None` (§4.5).
    pub variants: Vec<(String, DefId)>,
    /// How many payload types each variant carries, which is what decides
    /// whether a bare name in a pattern is a match or a binding (§4.4).
    pub variant_arity: Vec<(DefId, usize)>,
}

/// §5.1, plus the aliases. `Never` is §4.5.
const PRIMITIVES: &[&str] = &[
    "I8", "I16", "I32", "I64", "U8", "U16", "U32", "U64", "F32", "F64", "Bool", "Char", "String",
    "Int", "Float", "Never",
];

/// Library types with a representation in `link-rt` but no declaration in any
/// `.link` file (§8).
const LIBRARY_TYPES: &[&str] = &["Array", "Map", "Box", "Chars", "IoError"];

/// The traits the compiler knows about (§5.4).
const TRAITS: &[&str] = &["Copy", "Clone", "Drop", "Eq", "Ord", "Iterate", "From"];

/// The free functions (§8).
const FUNCTIONS: &[&str] = &["print", "println", "panic", "read_file", "write_file"];

/// `(enum name, [(variant name, payload arity)])`.
const ENUMS: &[(&str, &[(&str, usize)])] = &[
    ("Option", &[("Some", 1), ("None", 0)]),
    ("Result", &[("Ok", 1), ("Err", 1)]),
];

/// Allocates the prelude into `defs`.
pub fn build(defs: &mut DefTable) -> Prelude {
    let module = defs.alloc(DefKind::Module, "core", BUILTIN_SPAN, None);
    let mut prelude = Prelude {
        module,
        names: Vec::new(),
        variants: Vec::new(),
        variant_arity: Vec::new(),
    };

    let declare = |defs: &mut DefTable, kind: DefKind, name: &str, out: &mut Prelude| {
        let id = defs.alloc(kind, name, BUILTIN_SPAN, Some(module));
        out.names.push((name.to_string(), id));
        id
    };

    for name in PRIMITIVES.iter().chain(LIBRARY_TYPES) {
        declare(defs, DefKind::Primitive, name, &mut prelude);
    }
    for name in TRAITS {
        declare(defs, DefKind::Trait, name, &mut prelude);
    }
    for name in FUNCTIONS {
        declare(defs, DefKind::Fn, name, &mut prelude);
    }
    for (name, variants) in ENUMS {
        let enum_id = declare(defs, DefKind::Enum, name, &mut prelude);
        for (variant, arity) in *variants {
            let id = defs.alloc(DefKind::Variant, *variant, BUILTIN_SPAN, Some(enum_id));
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
    fn option_and_result_variants_are_reachable_unqualified() {
        let mut defs = DefTable::new();
        let prelude = build(&mut defs);
        for name in ["Some", "None", "Ok", "Err"] {
            assert!(prelude.variants.iter().any(|(n, _)| n == name), "`{name}` must be reachable");
        }
    }

    #[test]
    fn none_is_a_unit_variant_and_some_is_not() {
        let mut defs = DefTable::new();
        let prelude = build(&mut defs);
        let arity = |name: &str| {
            let (_, id) = prelude.variants.iter().find(|(n, _)| n == name).unwrap();
            prelude.variant_arity.iter().find(|(v, _)| v == id).unwrap().1
        };
        assert_eq!(arity("None"), 0, "a bare `None` in a pattern is a match, not a binding");
        assert_eq!(arity("Some"), 1);
    }

    #[test]
    fn every_builtin_definition_is_marked_as_having_no_source() {
        let mut defs = DefTable::new();
        build(&mut defs);
        assert!(defs.iter().all(|d| d.is_builtin()));
    }
}
