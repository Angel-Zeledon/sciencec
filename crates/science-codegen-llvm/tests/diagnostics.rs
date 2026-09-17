//! `SC0400`'s text, pinned against the thing it goes stale about.
//!
//! **The decision.** The assertions are about the *variables and the command*
//! the message names, not about its prose.
//!
//! **The reason.** The message this replaces was wrong in a specific and
//! expensive way: it told the reader to set `LLVM_SYS_181_PREFIX` and to have
//! `llvm-config`, both of which belong to `llvm-sys`, which this crate does not
//! use and — on the Windows binary release of LLVM 18.1.8 — could not use.
//! Someone who followed it would install LLVM, set the variable, and still have
//! no backend, because the missing thing was `--features llvm` and no version of
//! that message said so. What makes a message rot is a name in it drifting away
//! from a name in the code, so the names are what is checked: every variable the
//! message mentions is one [`science_codegen_llvm::llvm_prefixes`] actually
//! reads, and the feature it names is the one the manifest has.
//!
//! **The cost.** The prose is unpinned, so a rewrite that kept the names and
//! lost the meaning would pass. That is the right trade — a test that pinned the
//! sentences would be edited every time the sentences were, which is the failure
//! mode of a golden file for a diagnostic.
//!
//! This file needs no LLVM and no feature: it is `SC0400` that a machine without
//! either one sees.

use science_codegen::diagnostics::code;

#[test]
fn sc0400_names_the_feature_that_is_actually_missing() {
    let diagnostic = science_codegen_llvm::no_backend();
    assert_eq!(diagnostic.code, code::SC0400);
    assert!(diagnostic.message.contains("llvm"), "{}", diagnostic.message);
    let notes = diagnostic.notes.join("\n");
    assert!(
        notes.contains("--features llvm"),
        "the step that actually turns the backend on is not in the message:\n{notes}"
    );
    assert!(notes.contains("18.1"), "the version Decision 2 pins is not named:\n{notes}");
}

/// Every environment variable the message names is one the crate reads.
///
/// `SCIENCE_LLVM_PREFIX` is this crate's own and `LLVM_SYS_181_PREFIX` is read
/// as well, because a contributor who followed the older note will have set it.
/// Both are in [`science_codegen_llvm::llvm_prefixes`]'s search order and both
/// are in the message; a third name in either place and not the other is the
/// drift this asserts against.
#[test]
fn the_variables_the_message_names_are_the_variables_the_crate_reads() {
    let text = science_codegen_llvm::install_instructions();
    for variable in ["SCIENCE_LLVM_PREFIX", "LLVM_SYS_181_PREFIX"] {
        assert!(text.contains(variable), "`{variable}` is read and not mentioned:\n{text}");
    }
    // The build script reads the same two, and reads them in the same order.
    let build_rs = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("build.rs"),
    )
    .expect("build.rs is readable");
    assert!(
        build_rs.contains("[\"SCIENCE_LLVM_PREFIX\", \"LLVM_SYS_181_PREFIX\"]"),
        "`build.rs` and `lib.rs` disagree about which variables name an LLVM prefix"
    );
}

/// The instruction no longer asks for `llvm-sys`'s prerequisites.
///
/// `llvm-config` and the `-dev` packages are what `llvm-sys` needs. This crate
/// links one shared library through one `extern` block; asking for either is
/// asking the reader to install something that will not help, and on this
/// machine `llvm-config.exe` does not exist in the release at all.
#[test]
fn the_instruction_does_not_ask_for_llvm_syss_prerequisites() {
    let text = science_codegen_llvm::install_instructions();
    assert!(
        text.contains("does not use `llvm-sys`") || text.contains("no `llvm-sys`"),
        "the message should say what it does not need, because the previous one asked for \
         it:\n{text}"
    );
    if cfg!(target_os = "windows") {
        assert!(text.contains("winget") || text.contains("choco"), "{text}");
        // The Windows-specific trap: the DLL has to be findable at run time, and
        // the installer does not put it on `PATH`. `lib.rs` §1's second cost.
        assert!(text.contains("PATH"), "the DLL-loading requirement is not mentioned:\n{text}");
    }
}

/// `BACKENDS` and [`science_codegen_llvm::available`] agree with the build.
#[test]
fn the_backend_list_matches_the_feature_this_was_built_with() {
    if cfg!(feature = "llvm") {
        assert_eq!(science_codegen_llvm::BACKENDS, ["llvm"]);
        assert!(science_codegen_llvm::available());
    } else {
        assert!(science_codegen_llvm::BACKENDS.is_empty());
        assert!(!science_codegen_llvm::available());
    }
}
