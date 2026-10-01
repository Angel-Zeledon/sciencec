//! The standard library modules written in Science, bundled into the compiler.
//!
//! `stdlib-core.md` §9 marks most of the library *"Science"*, and until `io`
//! nothing was: the prelude is declared as data in [`crate::builtins`] and
//! implemented in `science-rt`. A module here is ordinary Science source —
//! lexed, parsed, resolved, checked and monomorphised with the program that
//! `use`s it — which the compiler carries so that no install step can lose it.
//!
//! # How a program reaches one
//!
//! **Decision. A bundled module is a fallback for [`crate::modules::
//! collect_crate`]'s `open`: when no file answers a candidate path, the
//! caller asks [`source`] for it.** It enters the crate as a module at the
//! root, exactly as a sibling file of that name would, so `use io (File)` and
//! a user's `use text.parser` are the same mechanism.
//!
//! **Reason.** `collect_crate`'s own documentation reserves `open` for this —
//! *"a caller that wants to try three directories tries three directories"* —
//! and `stdlib-shape-and-packages.md` §6.3 orders the search: the module path,
//! then `SCIENCE_PATH`, then the toolchain. This is the toolchain, and it is
//! last.
//!
//! **Cost.** A user file named `io.science` beside the entry shadows the
//! bundled one, silently. That is §6.3's order and Python's, and it has
//! Python's footgun; the alternative — a bundled name no program may use for a
//! module of its own — is a namespace decision §6.3 did not take.
//!
//! Every caller of `collect_crate` must fall back the same way, or two of them
//! build different programs from one source: `sciencec`'s driver, its UI test
//! harness, and `science-codegen-llvm`'s execution-test harness all do.

/// The bundled modules, by the candidate path `collect_crate` asks for.
const MODULES: &[(&str, &str)] = &[
    ("io.science", include_str!("../stdlib/io.science")),
    ("collections.science", include_str!("../stdlib/collections.science")),
    ("os.science", include_str!("../stdlib/os.science")),
    ("time.science", include_str!("../stdlib/time.science")),
];

/// The source of the bundled module at `candidate`, if there is one.
pub fn source(candidate: &str) -> Option<&'static str> {
    MODULES.iter().find(|(path, _)| *path == candidate).map(|(_, text)| *text)
}

/// The name a bundled module's file is registered under, for diagnostics.
///
/// Not a path anything can open: the angle brackets say so, the way a
/// `<built-in>` location does in a C compiler.
pub fn display_path(candidate: &str) -> String {
    format!("<stdlib>/{candidate}")
}

/// Every bundled module, as `(candidate path, source)`.
pub fn modules() -> impl Iterator<Item = (&'static str, &'static str)> {
    MODULES.iter().copied()
}

#[cfg(test)]
mod tests {
    use super::modules;
    use science_diagnostics::FileId;

    /// A bundled module is a file every `use` of it compiles, so a mistake in
    /// one is a mistake in every program that imports it — found here, where
    /// it names the module, rather than as a diagnostic in a stranger's build.
    #[test]
    fn every_bundled_module_lexes_parses_and_resolves_clean() {
        for (path, text) in modules() {
            let file = FileId(0);
            let (tokens, lexed) = science_lexer::lex(file, text);
            assert!(!lexed.has_errors(), "`{path}` must lex");
            let (ast, parsed) = science_parser::parse_module(&tokens, file);
            assert!(!parsed.has_errors(), "`{path}` must parse");
            let sources = [crate::SourceModule { file, path: path.to_string(), ast, entry: false }];
            let (_, resolution) = crate::resolve_crate(&sources);
            assert!(!resolution.has_errors(), "`{path}` must resolve");
        }
    }
}
