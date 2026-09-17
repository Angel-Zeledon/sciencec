//! Where a source file sits in the module tree, and which files are in the
//! crate at all.
//!
//! §4.3 gives two sentences and F0 has no `mod` declaration (§12), so the tree
//! is a function of file paths and nothing else:
//!
//! - a file is a module named after it, and
//! - a directory with a `mod.science` is a module containing its siblings.
//!
//! Which means `mod.science` is the one file that does *not* introduce a module
//! of its own: it **is** its directory's module, and its items land there. The
//! rest is arithmetic on path components.
//!
//! Two things the spec leaves open, decided here and recorded so a later
//! change is a deliberate one:
//!
//! - **A directory without a `mod.science` still becomes a module.** The
//!   alternative is that `a/b.science` is unreachable whenever `a/mod.science` is
//!   missing, which turns a missing file into a silent loss of code. An
//!   implicit module is empty until something is put in it, and costs nothing.
//! - **The crate root is an unnamed module** holding the top-level files. No
//!   file is the crate root, so `main.science` is the module `main`, and nothing
//!   about a name is special-cased.
//!
//! # What a crate is
//!
//! **Decision. A crate is one entry file plus the transitive closure of the
//! modules its `use` declarations name, and the crate root is the directory the
//! entry file sits in.** [`collect_crate`] is that closure; it is the whole of
//! what "`use` loads a file" means.
//!
//! **Reason.** `script-mode.md` §4.2 rule 1 makes the file named on the command
//! line the entry, and §4.3 scopes the only rule that distinguishes a module
//! from a script — `SC0213` — to a module *"reached by `use` in this
//! compilation"*. Both sentences are about one entry and the files it reaches,
//! so that is what a crate is. Rooting it at the entry's directory is forced by
//! the alternative being worse: rooting it at the working directory would make
//! `use text.parser` mean a different file depending on where the command was
//! typed.
//!
//! **What this deliberately does not decide.** `package-manager.md` Decision 15
//! owns the entry file and Decision 12 owns `science.toml`, and that note asks
//! for *"crate root"* to become *"package root"* — the directory holding the
//! manifest. Nothing here contradicts either: the manifest is optional (its
//! §4.6), a file with no manifest above it is *"a single-file package named
//! after the file"*, and this is that case and only that case. When the
//! manifest arrives it decides the root and the entry, and hands both to
//! [`collect_crate`], which does not care where they came from.
//!
//! **The cost, and it is a real one.** `stdlib-shape-and-packages.md` §6.3
//! calls a hard-wired root a one-way door and asks for an ordered search path —
//! `--module-path`, then `SCIENCE_PATH`, then the toolchain. This is a
//! hard-wired root of exactly one entry. The door is not closed: [`open`] is a
//! callback, so a caller that wants to try three directories tries three
//! directories and nothing here changes. What is *not* built is the flag, the
//! environment variable and the toolchain directory, because each of them is a
//! decision about where packages live and that decision is the package
//! manager's.
//!
//! [`open`]: collect_crate

/// The chain of module names leading to a source file, outermost first.
///
/// `text/parser.science` is `["text", "parser"]`; `text/mod.science` is `["text"]`,
/// because a `mod.science` is its directory. A path at the root gives a chain of
/// one, and the empty chain is the crate root itself.
///
/// Separators may be `/` or `\`, so a caller may hand over a native path
/// without converting it first. Empty components are dropped, which also
/// absorbs a leading `./`.
pub fn module_chain(path: &str) -> Vec<String> {
    let mut parts: Vec<&str> = path
        .split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();

    let Some(file) = parts.pop() else {
        return Vec::new();
    };

    let stem = file.strip_suffix(".science").unwrap_or(file);
    let mut chain: Vec<String> = parts.into_iter().map(str::to_string).collect();

    // `mod.science` names no module of its own: it is the directory's.
    if stem != "mod" {
        chain.push(stem.to_string());
    }
    chain
}

/// The files that could hold the module `chain` names, in the order they are
/// tried.
///
/// The inverse of [`module_chain`], and it has to be a *list* because the
/// forward direction is many-to-one: §4.4 gives a module two spellings — a file
/// and a directory with a `mod.science` — and a `use` says which module it
/// wants, never which spelling. The file is tried first because it is the
/// cheaper thing to have written; a tree carrying both `text.science` and
/// `text/mod.science` has two files claiming one module, and which of them wins
/// is a question for whoever wrote the directory, not for this function.
///
/// Paths come out relative to the crate root, with `/`, which is the spelling
/// [`module_chain`] reads back.
pub fn candidate_files(chain: &[String]) -> Vec<String> {
    // The crate root has no name, so nothing can `use` it and nothing asks.
    if chain.is_empty() {
        return Vec::new();
    }
    let joined = chain.join("/");
    vec![format!("{joined}.science"), format!("{joined}/mod.science")]
}

/// Every module chain a file's `use` declarations name, shortest first.
///
/// `use compiler.frontend.lexer (Lexer)` asks for three modules and not one:
/// `compiler`, `compiler.frontend` and `compiler.frontend.lexer`. Each prefix
/// is a module in its own right, each may be a `mod.science` with items of its
/// own, and `text.parser.lex(source)` written in a body needs the *first*
/// segment to be a module the crate root offers. Shortest first so that a
/// directory's `mod.science` is loaded before the files under it.
///
/// The imported *names* — the `(Lexer)` part — are not modules and are not
/// here. Resolving them is [`crate::resolve`]'s job and needs the module.
fn imported_chains(ast: &science_parser::ast::Module) -> Vec<Vec<String>> {
    let mut chains = Vec::new();
    for item in &ast.items {
        let science_parser::ast::ItemKind::Use(decl) = &item.kind else { continue };
        let mut chain = Vec::new();
        for segment in &decl.path.segments {
            chain.push(segment.name.name.clone());
            chains.push(chain.clone());
        }
    }
    chains
}

/// The crate rooted at `entry`: the entry file, then every module reached from
/// it by `use`, transitively.
///
/// **Decision. `use` is the only thing that loads a file.** A path written in a
/// body — `text.parser.lex(source)` — reaches a module the crate already has
/// and never pulls a new one in.
///
/// **Reason.** `script-mode.md` §4.1 makes importing observably inert, and the
/// argument it gives is that a load which is invisible at its use site is an
/// effect. `use` is a declaration at the top of a file, which is where a reader
/// looks to find out what a file depends on; a type path in the middle of an
/// expression is not. The rule also keeps §4.3 checkable: *"reached by `use`"*
/// is a property somebody can read off the file.
///
/// **Cost.** A module used only through a fully qualified path has to be
/// imported anyway, and the `use` line will look redundant to anyone who has
/// written Python. It is not redundant: it is the file's dependency list.
///
/// `open` is handed one candidate path at a time, relative to the crate root
/// and with `/` separators, and answers with the file's id and syntax tree or
/// with `None` for *"no such file"*. Everything about where the crate root is,
/// how a file is read and what a failed read reports belongs to the caller —
/// this crate does no I/O — which is also what leaves room for
/// `stdlib-shape-and-packages.md` §6.3's search path.
///
/// A module is asked for once however many files `use` it, and the entry comes
/// out first, so `krate.modules[0]` is the entry module.
pub fn collect_crate(
    entry: crate::SourceModule,
    mut open: impl FnMut(&str) -> Option<(science_diagnostics::FileId, science_parser::ast::Module)>,
) -> Vec<crate::SourceModule> {
    let mut sources = vec![entry];
    // Keyed by chain rather than by path, so that `text.science` and
    // `text/mod.science` cannot both be loaded, and so that a cycle —
    // `a` uses `b`, `b` uses `a` — terminates at the second visit.
    let mut asked: std::collections::HashSet<Vec<String>> = std::collections::HashSet::new();
    asked.insert(module_chain(&sources[0].path));

    let mut next = 0;
    while next < sources.len() {
        for chain in imported_chains(&sources[next].ast) {
            if !asked.insert(chain.clone()) {
                continue;
            }
            for candidate in candidate_files(&chain) {
                if let Some((file, ast)) = open(&candidate) {
                    sources.push(crate::SourceModule {
                        file,
                        path: candidate,
                        ast,
                        entry: false,
                    });
                    break;
                }
            }
        }
        next += 1;
    }
    sources
}

#[cfg(test)]
mod tests {
    use super::{candidate_files, module_chain};

    fn chain(path: &str) -> Vec<String> {
        module_chain(path)
    }

    #[test]
    fn a_file_at_the_root_is_a_module_named_after_it() {
        assert_eq!(chain("main.science"), ["main"]);
    }

    #[test]
    fn a_file_in_a_directory_is_a_module_under_it() {
        assert_eq!(chain("text/parser.science"), ["text", "parser"]);
    }

    #[test]
    fn a_mod_file_is_its_own_directory_rather_than_a_module_under_it() {
        assert_eq!(chain("text/mod.science"), ["text"]);
        assert_eq!(chain("text/parser/mod.science"), ["text", "parser"]);
    }

    #[test]
    fn a_mod_file_at_the_root_is_the_crate_root() {
        assert_eq!(chain("mod.science"), Vec::<String>::new());
    }

    #[test]
    fn a_directory_without_a_mod_file_still_yields_a_module() {
        // `a/mod.science` may be absent; `a` exists anyway, empty.
        assert_eq!(chain("a/b.science"), ["a", "b"]);
    }

    #[test]
    fn native_separators_and_a_leading_dot_are_accepted() {
        assert_eq!(chain(r".\text\parser.science"), ["text", "parser"]);
        assert_eq!(chain("./text/parser.science"), ["text", "parser"]);
    }

    #[test]
    fn a_path_without_the_extension_still_names_a_module() {
        assert_eq!(chain("text/parser"), ["text", "parser"]);
    }

    #[test]
    fn an_empty_path_is_the_crate_root() {
        assert_eq!(chain(""), Vec::<String>::new());
    }

    fn candidates(chain: &[&str]) -> Vec<String> {
        candidate_files(&chain.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn a_module_is_either_a_file_or_a_directory_with_a_mod_file() {
        assert_eq!(candidates(&["text", "parser"]), ["text/parser.science", "text/parser/mod.science"]);
        assert_eq!(candidates(&["text"]), ["text.science", "text/mod.science"]);
    }

    #[test]
    fn every_candidate_reads_back_as_the_chain_it_came_from() {
        for chain in [vec!["text"], vec!["text", "parser"], vec!["a", "b", "c"]] {
            let expected: Vec<String> = chain.iter().map(|s| s.to_string()).collect();
            for candidate in candidates(&chain) {
                assert_eq!(module_chain(&candidate), expected, "{candidate}");
            }
        }
    }

    #[test]
    fn the_crate_root_is_not_a_file_anything_can_ask_for() {
        assert!(candidate_files(&[]).is_empty());
    }
}
