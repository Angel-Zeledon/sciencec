//! The module search path: where a `use` that is not a sibling file goes.
//!
//! `science_resolve::modules::collect_crate` asks for one candidate path at a
//! time — `util/text.science`, `util/text/mod.science` — and leaves the
//! answer to its caller. This is the answer, in the order
//! `stdlib-shape-and-packages.md` §6.3 fixes and `package-manager.md` §7.1
//! makes F0:
//!
//! 1. **a mounted package**, when the candidate's first component is the name
//!    of a package in the build (Decision 13: *"the `modules` directory is
//!    mounted at the package name"*);
//! 2. **the crate root**, the directory the entry sits in — what every
//!    `use` meant before there was a manifest;
//! 3. **`--module-path`**, each directory in the order given;
//! 4. **`SCIENCE_PATH`**, in the platform's path-list syntax;
//! 5. **the toolchain** — `science_resolve::stdlib`'s bundled modules, which
//!    the caller asks after this returns `None`, because that crate owns them.
//!
//! # A mounted name is final
//!
//! **Decision. When a candidate's first component names a package, that
//! package answers it or nothing does.** `use util.text` with a dependency
//! `util` that has no `text` is `SC0202`, never a fall-through to a
//! `util/text.science` beside the entry.
//!
//! **Reason.** The manifest said where `util` is. A lookup that quietly finds
//! a different `util` when the declared one lacks a module is the shadowing
//! bug §6.3's order exists to make predictable, made unpredictable.
//!
//! **Cost.** A root package with a file `util.science` *and* a dependency
//! named `util` cannot reach the file. The manifest wins because it is the
//! explicit statement; the file is the one to rename.
//!
//! # What a mount does not know
//!
//! `collect_crate`'s callback is handed a path and not the module that asked
//! for it, so every package in the graph is mounted for every file in the
//! build. A root package can therefore `use` a package that only one of its
//! dependencies declares. Recorded as a cost rather than fixed: closing it
//! means teaching `collect_crate` who asked, and §7.1 does not need it.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct ModuleSearch {
    /// The crate root (step 2).
    pub root: PathBuf,
    /// `(package name, modules directory)`, step 1.
    pub mounts: Vec<(String, PathBuf)>,
    /// `--module-path` then `SCIENCE_PATH`, steps 3 and 4.
    pub dirs: Vec<PathBuf>,
}

impl ModuleSearch {
    /// The file that answers `candidate`, if one does before the toolchain.
    pub fn locate(&self, candidate: &str) -> Option<PathBuf> {
        let (head, rest) = match candidate.split_once('/') {
            Some((head, rest)) => (head, rest),
            // `util.science` is the package's root module: `src/mod.science`.
            None => (candidate.strip_suffix(".science").unwrap_or(candidate), "mod.science"),
        };
        if let Some((_, modules)) = self.mounts.iter().find(|(name, _)| name == head) {
            let path = modules.join(rest);
            return path.is_file().then_some(path);
        }
        std::iter::once(&self.root)
            .chain(&self.dirs)
            .map(|base| base.join(candidate))
            .find(|path| path.is_file())
    }

    /// Whether `candidate` lies under a mounted package. A caller uses it to
    /// skip the toolchain fallback: a mount is final (see the module notes).
    pub fn is_mounted(&self, candidate: &str) -> bool {
        let head = candidate.split('/').next().unwrap_or("");
        let head = head.strip_suffix(".science").unwrap_or(head);
        self.mounts.iter().any(|(name, _)| name == head)
    }
}

/// Steps 3 and 4: the `--module-path` directories, then `SCIENCE_PATH`'s.
///
/// An empty entry in `SCIENCE_PATH` is skipped rather than read as the
/// working directory — the working directory is exactly the input
/// `modules.rs` refuses to let decide what a `use` means.
pub fn search_dirs(flags: &[PathBuf], science_path: Option<OsString>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = flags.to_vec();
    if let Some(list) = science_path {
        dirs.extend(std::env::split_paths(&list).filter(|p| !p.as_os_str().is_empty()));
    }
    dirs
}

/// `to` written relative to `from`, with `/`, for a lockfile that must read
/// the same on every machine. Both are expected absolute.
pub fn relative(from: &Path, to: &Path) -> String {
    let from: Vec<_> = from.components().collect();
    let to: Vec<_> = to.components().collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = std::iter::repeat("..".to_string()).take(from.len() - common).collect();
    parts.extend(to[common..].iter().map(|c| c.as_os_str().to_string_lossy().into_owned()));
    if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_climb_and_descend() {
        assert_eq!(relative(Path::new("/a/b/app"), Path::new("/a/b/util")), "../util");
        assert_eq!(relative(Path::new("/a/app"), Path::new("/a/app/vendor/x")), "vendor/x");
        assert_eq!(relative(Path::new("/a"), Path::new("/a")), ".");
    }

    #[test]
    fn the_flag_comes_before_the_environment() {
        let joined = std::env::join_paths(["/env/one", "", "/env/two"]).unwrap();
        let dirs = search_dirs(&[PathBuf::from("/flag")], Some(joined));
        assert_eq!(dirs, [PathBuf::from("/flag"), PathBuf::from("/env/one"), PathBuf::from("/env/two")]);
    }
}
