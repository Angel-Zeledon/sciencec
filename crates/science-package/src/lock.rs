//! `science.lock`, in the shape §7.1 ships it: the format before the resolver.
//!
//! `package-manager.md` §7.1: *"`science.lock` written, even when it records
//! only this package and the `[build]` table. **The format ships before the
//! resolver does**."* The reason is §7.1's own: a lockfile format added after
//! builds exist means no build before it can be identified.
//!
//! **What is in it.** Every package in the graph, root first and then by
//! name, with its version and — for a dependency — `path+` and its directory
//! relative to the root; and a `[build]` table with the `sciencec` version
//! and the host it ran on.
//!
//! **What is not, and why each is not a guess.** Decision 2's SHA-256 of a
//! canonical archive is F1 (§7.2), and its canonicalisation rules are a
//! compatibility surface §2.2 says must be specified and tested from the
//! first commit — so no hash is written until those rules are, rather than a
//! hash of something else that would have to be invalidated. Decision 3's
//! LLVM version and optimisation level are not known to this crate, and a
//! field written with a placeholder is a field a reader will believe.
//!
//! **Deterministic, and written only when it changes**, so a rebuild leaves
//! the file's timestamp alone and a checked-in lock does not churn.

use std::path::Path;

use crate::graph::Graph;
use crate::search::relative;

/// The lockfile's file name, beside `science.toml`.
pub const LOCK: &str = "science.lock";

/// The lock's text for `graph`, built by `sciencec` version `compiler`.
pub fn render(graph: &Graph, compiler: &str) -> String {
    let mut out = String::new();
    out.push_str("# science.lock — written by `sciencec`; do not edit it by hand.\n");
    out.push_str("# package-manager.md §7.1: the format ships before the resolver, and\n");
    out.push_str("# content hashes (Decision 2) arrive with it, in F1.\n");
    out.push_str("lock-version = 1\n");

    let root = graph.root();
    let mut entries: Vec<(String, String, Option<String>)> = vec![(
        root.manifest.name.clone(),
        root.manifest.version.clone(),
        None,
    )];
    let mut dependencies: Vec<_> = graph
        .dependencies()
        .iter()
        .map(|p| {
            (p.manifest.name.clone(), p.manifest.version.clone(), Some(format!("path+{}", relative(&root.root, &p.root))))
        })
        .collect();
    dependencies.sort();
    entries.extend(dependencies);

    for (name, version, source) in entries {
        out.push_str("\n[[package]]\n");
        out.push_str(&format!("name = {}\n", quoted(&name)));
        out.push_str(&format!("version = {}\n", quoted(&version)));
        if let Some(source) = source {
            out.push_str(&format!("source = {}\n", quoted(&source)));
        }
    }

    out.push_str("\n[build]\n");
    out.push_str(&format!("sciencec = {}\n", quoted(compiler)));
    out.push_str(&format!(
        "host = {}\n",
        quoted(&format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS))
    ));
    out
}

/// Writes `text` to `science.lock` under `root`, unless it already says that.
pub fn write(root: &Path, text: &str) -> std::io::Result<()> {
    let path = root.join(LOCK);
    if std::fs::read_to_string(&path).is_ok_and(|existing| existing == text) {
        return Ok(());
    }
    std::fs::write(path, text)
}

fn quoted(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}
