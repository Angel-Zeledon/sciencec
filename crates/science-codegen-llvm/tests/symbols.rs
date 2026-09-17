//! The two symbol boundaries this crate sits between, each checked in both
//! directions.
//!
//! **Below it, LLVM**: check 1 of `sys.rs` §1 — every name in the `extern` block
//! is a symbol this LLVM exports, and nothing that should be absent is present.
//! That is the whole of this file down to the last section.
//!
//! **Above it, `science-rt`**: every `#[no_mangle]` the runtime defines is an
//! entry point `science_codegen::runtime::RUNTIME` declares, and every symbol
//! that table declares is one the runtime defines. Same class of defect, same
//! method, different pair of files; the last section is the account and it was
//! added when finding 5's two entry points were.
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

// --- the runtime side ------------------------------------------------------
//
// Everything above is about `sys.rs` and LLVM's own exports. The same class of
// defect exists at the other boundary and had no test: **a symbol the emitter
// declares and the runtime does not define is a link error at the end of a long
// build, and a symbol the runtime defines and the emitter never declares is
// dead weight the linker still carries.** `science_codegen::runtime::RUNTIME`
// is the emitter's side of that boundary and `science-rt`'s `#[no_mangle]`
// functions are the runtime's, and until finding 5 added two entry points
// nothing compared them.
//
// Read as text, for the reason the LLVM half is read as text: it runs in **CI's
// configuration**, with no `llvm` feature and no LLVM installation, which is
// where a new entry point is most likely to be added and least likely to be
// linked. The cost is the same cost — a text parse is not a parse — so the
// extractor asserts the shape it expects rather than passing with nothing in
// hand.

/// Every `#[no_mangle] … extern "C" fn` in `science-rt`'s sources, by name.
fn runtime_definitions() -> Vec<String> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .join("science-rt")
        .join("src");
    let mut names = Vec::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&src)
        .unwrap_or_else(|e| panic!("`{}` is readable: {e}", src.display()))
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| path.extension().is_some_and(|e| e == "rs"))
        .collect();
    files.sort();
    assert!(files.len() > 5, "only {} modules under {}", files.len(), src.display());
    for file in &files {
        let text = std::fs::read_to_string(file).expect("a readable module");
        let mut tagged = false;
        for line in text.lines() {
            let line = line.trim();
            if line == "#[no_mangle]" {
                tagged = true;
                continue;
            }
            if !tagged {
                continue;
            }
            tagged = false;
            // `pub extern "C" fn NAME(` or `pub unsafe extern "C" fn NAME(`.
            let Some(rest) = line.split(" fn ").nth(1) else {
                panic!("`{}` has a `#[no_mangle]` on something this extractor cannot read: {line}",
                       file.display());
            };
            let (name, _) = rest.split_once('(').expect("a parameter list on the same line");
            names.push(name.to_string());
        }
    }
    names
}

#[test]
fn every_runtime_definition_is_an_entry_point_codegen_knows_about() {
    use science_codegen::runtime::RUNTIME;

    let defined = runtime_definitions();
    assert!(
        defined.len() > 40,
        "only {} `#[no_mangle]` functions were extracted, which is far short of the table — the \
         extractor is reading `science-rt` wrong rather than the crate being small",
        defined.len()
    );
    for name in &defined {
        assert!(name.starts_with("science_"), "`{name}` breaks §8's one-prefix rule");
    }

    let declared: Vec<&str> = RUNTIME.iter().map(|f| f.symbol).collect();
    let undeclared: Vec<&String> =
        defined.iter().filter(|name| !declared.contains(&name.as_str())).collect();
    assert!(
        undeclared.is_empty(),
        "`science-rt` exports {} symbol(s) `RUNTIME` does not declare: {undeclared:?}\n\
         §2.6 says the table is the whole list, so an entry point nothing declares is one \
         nothing can call — dead code in every binary this compiler produces.",
        undeclared.len()
    );
    let undefined: Vec<&&str> =
        declared.iter().filter(|symbol| !defined.iter().any(|name| name == *symbol)).collect();
    assert!(
        undefined.is_empty(),
        "`RUNTIME` declares {} symbol(s) `science-rt` does not define: {undefined:?}\n\
         That is an unresolved external at the end of a link, which is the slowest place in the \
         build to find a typo.",
        undefined.len()
    );
    assert_eq!(defined.len(), declared.len());
}

/// The two symbols `script-mode.md` §2.3's fourth row needs, at the boundary
/// they cross.
///
/// The test above would catch either one going missing; this one says what they
/// are for, so that a later reader deleting an "unused" runtime function finds
/// out here rather than in a linker's output.
#[test]
fn finding_fives_two_symbols_are_defined_and_declared() {
    use science_codegen::runtime::{EXIT_CONTRACT, runtime_fn};

    let defined = runtime_definitions();
    for symbol in [EXIT_CONTRACT.eprint_symbol, EXIT_CONTRACT.exit_symbol] {
        let symbol = symbol.expect("§9.3 finding 5 is discharged and both are named");
        assert!(defined.iter().any(|name| name == symbol), "`science-rt` lost `{symbol}`");
        assert!(runtime_fn(symbol).is_some(), "`RUNTIME` lost `{symbol}`");
    }
}
