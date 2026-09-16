//! Science's name resolution: AST in, HIR out.
//!
//! This is the phase §7.1 of the design spec draws between the parser and the
//! type checker. It builds the module tree, walks every scope, and turns every
//! path into a [`DefId`](hir::DefId), so that no phase after it ever looks a
//! name up by string. It also settles the three questions §4.4 hands it —
//! binding or variant, call or construction, positional or named — because
//! they cannot be answered by syntax alone.
//!
//! - [`hir`] is the tree it produces.
//! - [`resolve`] is the pass. [`resolve_module`] and [`resolve_crate`] are the
//!   entry points.
//! - [`scope`] is the rib stack, [`modules`] the file-path-to-module rule, and
//!   [`builtins`] the prelude of §5.1 and §8.
//! - [`dump`] renders an HIR tree the way `science-parser` renders an AST.
//!
//! ```no_run
//! use science_diagnostics::FileId;
//! use science_parser::ast;
//!
//! # let ast: ast::Module = unimplemented!();
//! let (krate, diagnostics) = science_resolve::resolve_module(FileId(0), "main.science", &ast);
//! print!("{}", science_resolve::dump::dump_crate(&krate));
//! assert!(!diagnostics.has_errors());
//! ```
//!
//! Resolution never stops at the first error. An unresolved name becomes an
//! error node, the walk continues, and one compilation reports every problem
//! it can see.

pub mod builtins;
pub mod dump;
pub mod hir;
pub mod modules;
pub mod resolve;
pub mod scope;

pub use resolve::{resolve_crate, resolve_module, SourceModule};

/// The diagnostics this crate emits.
///
/// §9 of the design spec gives `SC0200`-`SC0299` to "resolution and types",
/// which is two phases sharing one range. The split, agreed with the phase
/// that owns the other half:
///
/// | Range | Owner |
/// |---|---|
/// | `SC0200`-`SC0249` | `science-resolve`: names, scopes, modules, imports, the orphan rule |
/// | `SC0250`-`SC0299` | `science-types`: inference, trait bounds, exhaustiveness |
///
/// With one exception, which is in the spec rather than in this split: §4.4
/// names `SC0210` for a non-exhaustive `match`. That is an exhaustiveness
/// check and so belongs to `science-types`, on the wrong side of the line. The
/// code is therefore **left unassigned here**, reserved for `science-types` to
/// use as §4.4 spells it. If the split is ever renegotiated, that is the one
/// number to look at first.
pub mod codes {
    use science_diagnostics::Code;

    /// A name that resolves to nothing in any scope.
    pub const UNRESOLVED_NAME: Code = Code(200);
    /// The same name declared twice where only one may be.
    pub const DUPLICATE_DEFINITION: Code = Code(201);
    /// A `use` naming a module or an item that does not exist.
    pub const UNRESOLVED_IMPORT: Code = Code(202);
    /// A bare name that two choice types in scope both offer as a variant
    /// (§4.5).
    pub const AMBIGUOUS_NAME: Code = Code(203);
    /// A path qualifier that is neither a module nor a choice type.
    pub const NOT_A_MODULE: Code = Code(204);
    /// A record literal or record pattern naming a field the record lacks.
    pub const UNKNOWN_FIELD: Code = Code(205);
    /// Named arguments on something that is not a record, or positional
    /// arguments on something that is (§4.4).
    pub const CONSTRUCTION_MISMATCH: Code = Code(206);
    /// An implementation whose interface and type both belong to other
    /// modules (§5.4).
    pub const ORPHAN_IMPL: Code = Code(207);
    /// `self` or `Self` where there is no implementation or interface to give
    /// it a meaning.
    pub const SELF_OUTSIDE_IMPL: Code = Code(208);
    /// One of the words §4.1 reserves for F1-F4, used as a name.
    pub const RESERVED_WORD: Code = Code(209);

    // Code(210) is deliberately skipped: §4.4 assigns it to a non-exhaustive
    // `match`, which `science-types` reports. See the module documentation.

    /// A name used in a position its kind cannot fill: a function where a type
    /// belongs, a record where an interface belongs.
    pub const WRONG_NAMESPACE: Code = Code(211);

    /// `each` written where no call argument encloses it, so there is no
    /// subject for it to name (§4.6).
    ///
    /// The parser reports a *nested* `each`, which it can see; it cannot see
    /// whether there is an enclosing argument at all once an expression has
    /// been lifted out of one, so the bare node arrives here and this is where
    /// it is answered.
    pub const EACH_WITHOUT_SUBJECT: Code = Code(212);

    /// Every code this crate can emit, for the test that keeps them inside
    /// `SC0200`-`SC0249` and distinct.
    pub const ALL: &[Code] = &[
        UNRESOLVED_NAME,
        DUPLICATE_DEFINITION,
        UNRESOLVED_IMPORT,
        AMBIGUOUS_NAME,
        NOT_A_MODULE,
        UNKNOWN_FIELD,
        CONSTRUCTION_MISMATCH,
        ORPHAN_IMPL,
        SELF_OUTSIDE_IMPL,
        RESERVED_WORD,
        WRONG_NAMESPACE,
        EACH_WITHOUT_SUBJECT,
    ];

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn every_code_is_this_crates_to_emit() {
            for code in ALL {
                assert!(
                    (200..=249).contains(&code.0),
                    "{code} is outside SC0200-SC0249, which is this crate's range"
                );
            }
        }

        #[test]
        fn no_code_is_used_twice() {
            let mut seen = ALL.to_vec();
            seen.sort();
            seen.dedup();
            assert_eq!(seen.len(), ALL.len(), "two diagnostics share a code");
        }

        #[test]
        fn the_code_section_4_4_reserves_for_exhaustiveness_is_left_alone() {
            assert!(
                !ALL.iter().any(|code| code.0 == 210),
                "SC0210 belongs to the type checker's exhaustiveness check (§4.4)"
            );
        }
    }
}
