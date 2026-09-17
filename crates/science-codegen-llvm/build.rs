//! Find LLVM, or say exactly which directory is missing.
//!
//! **The decision.** This script does nothing at all unless the `llvm` feature
//! is on. With it on, it locates an LLVM 18.1 installation, emits the two link
//! directives that bind `LLVM-C`, and **fails the build with the paths it
//! looked at** if there is none.
//!
//! **The reason.** `sciencec/src/driver.rs` and `science-codegen`'s manifest
//! both say a workspace nobody without LLVM can build is a workspace nobody can
//! contribute to. Gating on a feature rather than on detection is the difference
//! between a build that behaves the same on every machine and one that quietly
//! produces a different compiler depending on what happens to be installed — and
//! the second is worse, because the machine where it matters is CI.
//!
//! **The cost.** `cargo build --features llvm` is a second command, and the
//! default `cargo clippy --workspace` does not lint the LLVM half. Both are
//! stated in `src/lib.rs` §1.
//!
//! # What is linked, and why it is not what Decision 1 expected
//!
//! `llvm-sys` links ~100 static archives discovered through `llvm-config`.
//! Neither exists in the Windows binary release of LLVM 18.1.8: there is no
//! `llvm-config.exe` and `lib/` holds eleven files. What there is, is
//! `lib/LLVM-C.lib` — a 297 KB import library for `bin/LLVM-C.dll`, exporting
//! 1226 symbols. That is the shape Decision 3 already requires of the
//! *self-hosted* compiler, arriving early; `src/sys.rs` §0 is the argument.

use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=SCIENCE_LLVM_PREFIX");
    println!("cargo:rerun-if-env-changed=LLVM_SYS_181_PREFIX");

    if std::env::var_os("CARGO_FEATURE_LLVM").is_none() {
        return;
    }

    let candidates = candidates();
    let Some(prefix) = candidates.iter().find(|prefix| has_llvm_c(prefix)) else {
        panic!(
            "\n\n`science-codegen-llvm` was built with `--features llvm` and no LLVM 18.1 \
             installation was found.\n\nLooked for `{}` under:\n  {}\n\nInstall LLVM 18.1.8 and \
             set `SCIENCE_LLVM_PREFIX` (or `LLVM_SYS_181_PREFIX`) to the install root:\n  \
             Windows: winget install LLVM.LLVM --version 18.1.8\n  macOS:   brew install llvm@18\n \
             \x20Debian:  apt install llvm-18-dev\n\nBuilding without `--features llvm` produces \
             a compiler that reports SC0400, which is the supported configuration for anyone who \
             does not need a backend.\n\n",
            import_library(),
            candidates
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join("\n  ")
        );
    };

    println!("cargo:rustc-link-search=native={}", prefix.join("lib").display());
    println!("cargo:rustc-link-lib=dylib={}", link_name());
    println!("cargo:rustc-env=SCIENCE_LLVM_PREFIX_RESOLVED={}", prefix.display());

    // Windows resolves a DLL through the executable's directory and `PATH`, and
    // the LLVM installer sets neither. A binary linked here will fail to *load*
    // — before `main`, with no diagnostic — unless this directory is on `PATH`.
    // `src/lib.rs` §1 is the account; this is the reminder at the moment
    // somebody is watching the output.
    if cfg!(windows) {
        println!(
            "cargo:warning=science-codegen-llvm links LLVM-C.dll dynamically; put {} on PATH or \
             the compiler will not start",
            prefix.join("bin").display()
        );
    }
}

/// The link name, which is the file name without `lib`/`.lib`.
///
/// `LLVM-C` on Windows, where the release ships `LLVM-C.lib` beside
/// `LLVM-C.dll`. `LLVM-18` elsewhere, where the shared library is
/// `libLLVM-18.so` / `libLLVM.dylib` and there is no separate C-only object —
/// the C API is exported from the one shared library, which is what Decision 3
/// already names.
fn link_name() -> &'static str {
    if cfg!(windows) { "LLVM-C" } else { "LLVM-18" }
}

fn import_library() -> &'static str {
    if cfg!(windows) { "lib/LLVM-C.lib" } else { "lib/libLLVM-18.so or lib/libLLVM.dylib" }
}

fn has_llvm_c(prefix: &Path) -> bool {
    let lib = prefix.join("lib");
    if cfg!(windows) {
        lib.join("LLVM-C.lib").is_file()
    } else {
        lib.join("libLLVM-18.so").is_file()
            || lib.join("libLLVM.dylib").is_file()
            || lib.join("libLLVM-18.dylib").is_file()
    }
}

fn candidates() -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    for variable in ["SCIENCE_LLVM_PREFIX", "LLVM_SYS_181_PREFIX"] {
        if let Some(value) = std::env::var_os(variable) {
            candidates.push(PathBuf::from(value));
        }
    }
    let defaults: &[&str] = if cfg!(windows) {
        &[r"C:\Program Files\LLVM"]
    } else if cfg!(target_os = "macos") {
        &["/opt/homebrew/opt/llvm@18", "/usr/local/opt/llvm@18"]
    } else {
        &["/usr/lib/llvm-18", "/usr"]
    };
    candidates.extend(defaults.iter().map(PathBuf::from));
    candidates.dedup();
    candidates
}
