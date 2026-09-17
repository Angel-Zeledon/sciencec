//! Decision 42's line, enforced where `cargo` cannot reach.
//!
//! **What `cargo` already checks.** `science-codegen` does not depend on
//! `llvm-sys`, so *"the target-independent half does not name LLVM"* is a fact
//! the manifest makes true. Decision 42 asks for exactly that and §15 asks for
//! it as a test: *"if Decision 42 is worth having, it is worth a test that
//! asserts the target-independent half does not name LLVM at all"*.
//!
//! **What `cargo` cannot check, and this file does.** `mono.rs` added
//! `science-mir`, `science-types` and `science-resolve` to a crate that had
//! only `science-diagnostics`, and the manifest's comment claims that the
//! reach is confined to that one module: layout, ABI classification, the
//! descriptors, the runtime table, mangling and the target configuration are
//! still defined over [`science_codegen::layout::CgTy`] and know nothing about
//! the checker's types. That is a claim about six files, it is exactly the kind
//! of claim that erodes under deadline pressure — §15's own words — and it is a
//! `grep`.
//!
//! **The cost of doing it this way.** A source scan is a blunt instrument: it
//! would be fooled by a re-export and it reads comments as if they were code.
//! Both are acceptable here because the thing being protected is a *habit*
//! rather than a soundness property, and the alternative — a second crate, one
//! more level of Decision 42's split — is a manifest change the note has not
//! asked for.

use std::path::{Path, PathBuf};

/// The modules that must stay defined over `CgTy` alone.
///
/// `mono` is deliberately absent: it is the one item in Decision 42's table
/// that is defined over the *input*, and `Cargo.toml` says why at length.
/// `lib.rs` is absent because it documents the others and quotes their names.
const BELOW_THE_SEAM: &[&str] = &[
    "abi.rs",
    "backend.rs",
    "descriptor.rs",
    "layout.rs",
    "mangle.rs",
    "runtime.rs",
    "stub.rs",
    "target.rs",
];

/// The front-half crates `mono.rs` may name and the others may not.
const FRONT_HALF: &[&str] = &["science_mir", "science_types", "science_resolve"];

fn source_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn read(name: &str) -> String {
    std::fs::read_to_string(source_dir().join(name))
        .unwrap_or_else(|error| panic!("{name}: {error}"))
}

/// A line that is code rather than a doc comment or an ordinary comment.
fn code_lines(source: &str) -> impl Iterator<Item = (usize, &str)> {
    source.lines().enumerate().filter_map(|(at, line)| {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            return None;
        }
        Some((at + 1, line))
    })
}

#[test]
fn the_layout_and_abi_half_does_not_name_the_front_end() {
    for module in BELOW_THE_SEAM {
        let source = read(module);
        for (line, text) in code_lines(&source) {
            for crate_name in FRONT_HALF {
                assert!(
                    !text.contains(crate_name),
                    "{module}:{line} names `{crate_name}`; \
                     layout and ABI are defined over `CgTy` and `Cargo.toml` says why"
                );
            }
        }
    }
}

#[test]
fn nothing_in_this_crate_names_llvm_sys() {
    // Decision 42 and §15's ask. `cargo` checks the dependency; this checks the
    // text, because a `#[cfg(feature = "llvm")]` would satisfy `cargo` and
    // break the decision.
    for entry in std::fs::read_dir(source_dir()).expect("src/") {
        let path = entry.expect("an entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("readable");
        let name = path.file_name().expect("a name").to_string_lossy().into_owned();
        for (line, text) in code_lines(&source) {
            assert!(!text.contains("llvm_sys"), "{name}:{line}");
            assert!(!text.contains("inkwell"), "{name}:{line}");
        }
    }
}

#[test]
fn the_mono_module_is_the_only_one_that_reaches_above_the_line() {
    // The positive half: if this fails because `mono.rs` stopped naming the
    // front end, the dependency in `Cargo.toml` is no longer paying for
    // anything and should come out.
    let source = read("mono.rs");
    for crate_name in FRONT_HALF {
        assert!(
            code_lines(&source).any(|(_, text)| text.contains(crate_name)),
            "mono.rs does not name `{crate_name}`"
        );
    }
}
