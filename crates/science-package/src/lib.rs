//! Packages: `package-manager.md`'s F0 half, and the part of F1 that is a
//! directory walk.
//!
//! - [`toml`] — the normative TOML subset (Decision 12);
//! - [`manifest`] — `science.toml`'s schema and its `SP` diagnostics (§4.2, §8.2);
//! - [`graph`] — the root package and its `path` dependencies (§4.2, §8.2's
//!   `SP0010` and `SP0013`);
//! - [`search`] — the module search path (`stdlib-shape-and-packages.md` §6.3);
//! - [`lock`] — `science.lock`, format first (§7.1).
//!
//! # Where the commands live
//!
//! **Decision. The package manager is `sciencec`'s subcommands — `sciencec
//! new`, and `build` / `run` / `test` / `check` with no file named — and this
//! crate is the library they call. There is no `scargo` binary.**
//!
//! **Reason.** `docs/DREAM.md` §28 names a separate `scargo`, and says in its
//! own contrast note that where it and `package-manager.md` speak of the same
//! thing, the note wins. The note spells every command `sciencec build`,
//! `sciencec fetch`, `sciencec add`, and §11 argues the choice outright:
//! *"Putting the package manager inside `sciencec` (rather than shipping a
//! second binary) is right for a user who has to `scp` one file onto a
//! cluster."*
//!
//! **Cost.** §11's, accepted there: a resolution bug is a compiler release.
//!
//! This crate does I/O — it reads manifests and writes the lock — which is
//! why it is not part of `science-resolve`, whose `modules` is careful to do
//! none. It does not read, parse or compile a `.science` file; that is the
//! driver's, through the same pipeline a script goes through.

pub mod graph;
pub mod lock;
pub mod manifest;
pub mod search;
pub mod toml;

pub use graph::{find_root, Graph, Package, MANIFEST};
pub use manifest::Manifest;
pub use search::ModuleSearch;

#[cfg(test)]
mod tests {
    use science_diagnostics::Code;

    #[test]
    fn an_sp_code_renders_in_its_own_namespace() {
        assert_eq!(Code::sp(13).to_string(), "SP0013");
        assert_eq!(Code(202).to_string(), "SC0202");
        assert_eq!(Code(9999).to_string(), "SC9999");
    }
}
