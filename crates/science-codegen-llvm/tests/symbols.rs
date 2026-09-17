//! Check 1 of `sys.rs` §1: every name in the `extern` block is a symbol this
//! LLVM exports, and nothing that should be absent is present.
//!
//! **The decision.** This file parses `src/sys.rs` **as text** and runs
//! `llvm-nm` over the import library. It does not link against LLVM and it is
//! not behind the `llvm` feature.
//!
//! **The reason.** `sys.rs` opens by saying that every declaration in it *"is a
//! claim about a signature I cannot see"*, because this machine has two of
//! LLVM's forty C headers. The claims split into three kinds and they are not
//! checkable by the same means:
//!
//! 1. **The name exists.** Checkable with `llvm-nm` and nothing else. This file.
//! 2. **The arity and the types are right.** Checkable only by calling the
//!    function and reading what came back — `tests/abi_claims.rs` and
//!    `tests/roundtrip.rs`, both of which need the feature and the DLL.
//! 3. **A `c_uint` parameter is really `unsigned`.** Not checkable here at all,
//!    and `sys.rs` §1 says so.
//!
//! Reading the file as text is what lets kind 1 run in **CI's configuration** —
//! no feature, no LLVM — for the half that needs no LLVM, which is every
//! assertion below the `nm` fence. That half is not nothing: it is where a
//! declaration that should never exist would be caught, and §7.3 obligation 1
//! ("never set a fast-math flag") is enforced by a declaration not existing.
//!
//! **The cost.** A text parse is not a parse: the extractor below understands
//! `pub fn NAME(` inside one `unsafe extern "C" {` block and would be fooled by
//! a second block, a macro, or a declaration written across two lines before the
//! parenthesis. It asserts the shape it expects (one block, every name
//! `LLVM`-prefixed, a plausible count) so that a file it cannot read makes it
//! fail rather than pass with nothing in hand.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The names declared in `src/sys.rs`'s one `extern` block.
fn declared() -> Vec<String> {
    let source = std::fs::read_to_string(sys_rs()).expect("src/sys.rs is readable");
    let block = source
        .split_once("unsafe extern \"C\" {")
        .expect("src/sys.rs has exactly one `unsafe extern \"C\"` block")
        .1;
    assert!(
        !block.contains("extern \"C\" {"),
        "a second `extern` block: this extractor reads one, and `sys.rs`'s own rule is that \
         there is one"
    );
    let mut names = Vec::new();
    for line in block.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("pub fn ") else { continue };
        let Some((name, _)) = rest.split_once('(') else { continue };
        names.push(name.to_string());
    }
    names
}

fn sys_rs() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("sys.rs")
}

#[test]
fn the_block_parses_and_every_name_is_an_llvm_c_entry_point() {
    let names = declared();
    assert!(
        names.len() > 60,
        "only {} declarations were extracted; `sys.rs` §2 describes far more, so the extractor \
         is reading the file wrong rather than the file being short",
        names.len()
    );
    for name in &names {
        assert!(name.starts_with("LLVM"), "`{name}` is not an LLVM-C entry point");
    }
    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len(), "a name is declared twice in the `extern` block");
}

/// §7.3 obligation 1, and Decision 2's version pin, enforced by absence.
///
/// **This is the only obligation in the note that a test can make structural.**
/// *"Never set a fast-math flag"* is, in `target.rs`'s words, *"a line that is
/// correct by not being written, which means nothing tests it"*. Here something
/// does: the three entry points that could set one are named, and the assertion
/// is that no future edit quietly adds a declaration for them. A backend cannot
/// call what it has not declared, in Rust or in Science.
///
/// The other four names are the version trap rather than the policy one: each is
/// either an LLVM 19 widening or an LLVM 14-era spelling that 18 removed or
/// deprecated, and declaring one would either fail to link or silently read the
/// wrong number of bits. `sys.rs` documents each at the declaration that
/// replaced it.
#[test]
fn the_declarations_that_must_not_exist_do_not() {
    let forbidden: &[(&str, &str)] = &[
        ("LLVMSetFastMathFlags", "§7.3 obligation 1: this compiler never sets one"),
        ("LLVMGetFastMathFlags", "§7.3 obligation 1: there is none to read"),
        ("LLVMCanValueUseFastMathFlags", "§7.3 obligation 1"),
        ("LLVMConstStringInContext2", "LLVM 19's widening; 18.1.8 does not export it"),
        ("LLVMBuildLoad", "removed in the opaque-pointer era; `LLVMBuildLoad2` is the one"),
        ("LLVMArrayType", "takes an `unsigned` count; `LLVMArrayType2` takes `uint64_t`"),
        ("LLVMPointerType", "takes a pointee LLVM 18 discards; `LLVMPointerTypeInContext`"),
    ];
    let names = declared();
    for (name, why) in forbidden {
        assert!(
            !names.iter().any(|declared| declared == name),
            "`{name}` is declared, and it must not be — {why}"
        );
    }
}

// --- the `nm` fence -------------------------------------------------------
//
// Everything below needs an LLVM installation. CI has none, so a missing one is
// a skip with a printed reason rather than a failure: a test that fails on every
// machine that is not this one is a test somebody deletes.

/// Where `llvm-nm` and `LLVM-C.lib` live, if they do.
fn import_library_and_nm() -> Option<(PathBuf, PathBuf)> {
    for prefix in science_codegen_llvm::llvm_prefixes() {
        let library = prefix.join("lib").join(if cfg!(windows) {
            "LLVM-C.lib"
        } else if cfg!(target_os = "macos") {
            "libLLVM.dylib"
        } else {
            "libLLVM-18.so"
        });
        let nm = prefix.join("bin").join(if cfg!(windows) { "llvm-nm.exe" } else { "llvm-nm" });
        if library.is_file() && nm.is_file() {
            return Some((library, nm));
        }
    }
    None
}

/// Every symbol the import library exports, lower-cased comparison off — these
/// are C names and the case is the name.
fn exported(nm: &Path, library: &Path) -> Vec<String> {
    let output = Command::new(nm).arg(library).output().expect("`llvm-nm` runs");
    let text = String::from_utf8_lossy(&output.stdout);
    let mut names = Vec::new();
    for line in text.lines() {
        // `00000000 T LLVMContextCreate`, and the `__imp_` alias beside it.
        let Some(name) = line.split_whitespace().nth(2) else { continue };
        let name = name.strip_prefix("__imp_").unwrap_or(name);
        if name.starts_with("LLVM") {
            names.push(name.to_string());
        }
    }
    names.sort();
    names.dedup();
    names
}

#[test]
fn every_declared_symbol_is_exported_by_this_llvm() {
    let Some((library, nm)) = import_library_and_nm() else {
        eprintln!(
            "skipped: no LLVM 18.1 with both `lib/LLVM-C.lib` and `bin/llvm-nm` was found. \
             This is CI's configuration and the parse-only tests above still ran."
        );
        return;
    };
    let exports = exported(&nm, &library);
    assert!(
        exports.len() > 1000,
        "`llvm-nm` reported only {} `LLVM`-prefixed exports, which is too few for the C API — \
         the output format is not what this test parses",
        exports.len()
    );
    let missing: Vec<String> =
        declared().into_iter().filter(|name| !exports.contains(name)).collect();
    assert!(
        missing.is_empty(),
        "`sys.rs` declares {} symbol(s) this LLVM does not export: {missing:?}\n\
         A declaration for a symbol that does not exist is a link error at best; the ones that \
         do exist under a different name are the trap.",
        missing.len()
    );
}

/// The empirical half of `sys.rs`'s note on `LLVMConstStringInContext2`.
///
/// The declaration list above asserts that the name is not *declared*. This
/// asserts the fact that made that the right call: it is not *exported*. The
/// day the pin moves to an LLVM that has it, this fails and the note beside the
/// declaration is the thing to re-read.
#[test]
fn the_llvm_19_string_constructor_is_genuinely_absent() {
    let Some((library, nm)) = import_library_and_nm() else {
        eprintln!("skipped: no LLVM installation found");
        return;
    };
    let exports = exported(&nm, &library);
    assert!(
        !exports.iter().any(|name| name == "LLVMConstStringInContext2"),
        "this LLVM exports `LLVMConstStringInContext2`, so it is not the 18.1.8 Decision 2 pins"
    );
    assert!(
        exports.iter().any(|name| name == "LLVMConstStringInContext"),
        "the 18-era `LLVMConstStringInContext` is missing, which no LLVM 18 should be"
    );
}
