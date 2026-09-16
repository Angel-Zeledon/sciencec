//! Where a source file sits in the module tree.
//!
//! §4.3 gives two sentences and F0 has no `mod` declaration (§12), so the tree
//! is a function of file paths and nothing else:
//!
//! - a file is a module named after it, and
//! - a directory with a `mod.link` is a module containing its siblings.
//!
//! Which means `mod.link` is the one file that does *not* introduce a module
//! of its own: it **is** its directory's module, and its items land there. The
//! rest is arithmetic on path components.
//!
//! Two things the spec leaves open, decided here and recorded so a later
//! change is a deliberate one:
//!
//! - **A directory without a `mod.link` still becomes a module.** The
//!   alternative is that `a/b.link` is unreachable whenever `a/mod.link` is
//!   missing, which turns a missing file into a silent loss of code. An
//!   implicit module is empty until something is put in it, and costs nothing.
//! - **The crate root is an unnamed module** holding the top-level files. No
//!   file is the crate root, so `main.link` is the module `main`, and nothing
//!   about a name is special-cased.

/// The chain of module names leading to a source file, outermost first.
///
/// `text/parser.link` is `["text", "parser"]`; `text/mod.link` is `["text"]`,
/// because a `mod.link` is its directory. A path at the root gives a chain of
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

    let stem = file.strip_suffix(".link").unwrap_or(file);
    let mut chain: Vec<String> = parts.into_iter().map(str::to_string).collect();

    // `mod.link` names no module of its own: it is the directory's.
    if stem != "mod" {
        chain.push(stem.to_string());
    }
    chain
}

#[cfg(test)]
mod tests {
    use super::module_chain;

    fn chain(path: &str) -> Vec<String> {
        module_chain(path)
    }

    #[test]
    fn a_file_at_the_root_is_a_module_named_after_it() {
        assert_eq!(chain("main.link"), ["main"]);
    }

    #[test]
    fn a_file_in_a_directory_is_a_module_under_it() {
        assert_eq!(chain("text/parser.link"), ["text", "parser"]);
    }

    #[test]
    fn a_mod_file_is_its_own_directory_rather_than_a_module_under_it() {
        assert_eq!(chain("text/mod.link"), ["text"]);
        assert_eq!(chain("text/parser/mod.link"), ["text", "parser"]);
    }

    #[test]
    fn a_mod_file_at_the_root_is_the_crate_root() {
        assert_eq!(chain("mod.link"), Vec::<String>::new());
    }

    #[test]
    fn a_directory_without_a_mod_file_still_yields_a_module() {
        // `a/mod.link` may be absent; `a` exists anyway, empty.
        assert_eq!(chain("a/b.link"), ["a", "b"]);
    }

    #[test]
    fn native_separators_and_a_leading_dot_are_accepted() {
        assert_eq!(chain(r".\text\parser.link"), ["text", "parser"]);
        assert_eq!(chain("./text/parser.link"), ["text", "parser"]);
    }

    #[test]
    fn a_path_without_the_extension_still_names_a_module() {
        assert_eq!(chain("text/parser"), ["text", "parser"]);
    }

    #[test]
    fn an_empty_path_is_the_crate_root() {
        assert_eq!(chain(""), Vec::<String>::new());
    }
}
