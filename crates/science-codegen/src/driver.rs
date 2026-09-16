//! What `sciencec build` would call, and what it reports today.
//!
//! **The decision.** The `build` command's logic lives here rather than in
//! `sciencec`, and `sciencec`'s subcommand is expected to be a dozen lines that
//! collect paths, run the front end and call [`build`].
//!
//! **The reason.** `sciencec/Cargo.toml` argues that the driver is *"a call into
//! an existing crate plus the bookkeeping a shell needs"*, and a `build` command
//! that decided for itself which backend exists, what `SC0400` says and how to
//! name the missing package would be the first subcommand that was not. It is
//! also the half that can be tested: `sciencec` has no library target, so
//! anything written there is reachable only through the binary's own
//! `#[cfg(test)]` module, and a diagnostic's text is worth more tests than that
//! gives it.
//!
//! **The cost.** One more indirection between the command line and the answer,
//! and a `sciencec` that has to depend on `science-codegen` to have a `build`
//! command at all.
//!
//! # What it reports today
//!
//! `SC0400`. There is no backend compiled into this `sciencec` because LLVM is
//! not installed on this machine — see the crate documentation §0. §11 defines
//! `SC0400` as *"a toolchain feature required to build this program is not
//! compiled into this `sciencec`; names the feature and how to obtain a build
//! that has it"*, which is exactly the situation, and reporting it as a
//! first-class diagnostic is the difference between a compiler with a missing
//! feature and a compiler that is broken.

// `BACKENDS` is an empty constant slice today, so `clippy` can see that
// `BACKENDS.is_empty()` is always true and says so. The check is not redundant:
// it is the one line that changes behaviour the day `science-codegen-llvm`
// exists and pushes an entry into it, and replacing it with an unconditional
// `SC0400` would be replacing a compiler that has no backend with a compiler
// that cannot have one.
#![allow(clippy::const_is_empty)]

use science_diagnostics::{Diagnostic, Diagnostics};

use crate::backend::EmitKind;
use crate::diagnostics::{backend_not_compiled_in, cross_compilation_unsupported};
use crate::layout::Triple;
use crate::target::{OptLevel, TargetConfig};

/// The backends compiled into this `sciencec`.
///
/// Empty. The `llvm` backend lives in `science-codegen-llvm`, which needs
/// `llvm-sys`, which needs `llvm-config` or `LLVM_SYS_181_PREFIX`.
///
/// A slice rather than an `Option` because Decision 42 is explicitly about
/// there being more than one eventually, and a build that reported "the backend
/// is missing" would be a build that had forgotten the decision.
pub const BACKENDS: &[&str] = &[];

/// What a `build` invocation asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildRequest {
    /// The files to build.
    pub inputs: Vec<String>,
    /// What to produce.
    pub emit: EmitKind,
    /// The optimisation level. `-O2` by default, per §7.
    pub opt: OptLevel,
    /// The target. `None` means the host.
    pub target: Option<Triple>,
    /// §4.4's unsupported debugging flag.
    pub no_noalias: bool,
}

impl BuildRequest {
    /// A default request: the host, `-O2`, an executable.
    pub fn new(inputs: Vec<String>) -> BuildRequest {
        BuildRequest {
            inputs,
            emit: EmitKind::Executable,
            opt: OptLevel::default(),
            target: None,
            no_noalias: false,
        }
    }
}

/// What a successful build would have produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildOutput {
    /// The artefact's bytes.
    pub bytes: Vec<u8>,
    /// The configuration it was built with, for the `[build]` table of
    /// `package-manager.md` Decision 3.
    pub config: TargetConfig,
}

/// Run a build.
///
/// Returns diagnostics rather than a `Result<_, String>` because a build failure
/// is reported the way every other phase reports one — through
/// `science-diagnostics`, rendered by the driver, counted in the
/// `3 errors, 1 warning` summary. A compiler that reported one kind of failure
/// differently from the others is a compiler whose CI cannot gate on the
/// difference.
pub fn build(request: &BuildRequest) -> Result<BuildOutput, Diagnostics> {
    let mut diagnostics = Diagnostics::new();

    let host = Triple::host();
    match (request.target, host) {
        (Some(requested), Some(host)) if requested != host => {
            diagnostics.push(cross_compilation_unsupported(requested.as_str(), host.as_str()));
        }
        (Some(requested), None) => {
            diagnostics
                .push(cross_compilation_unsupported(requested.as_str(), "an unsupported host"));
        }
        _ => {}
    }

    if BACKENDS.is_empty() {
        diagnostics.push(no_backend());
    }

    Err(diagnostics)
}

/// The `SC0400` this compiler emits, with the platform's own instructions.
///
/// The `how` half of §11's *"names the feature and how to obtain a build that
/// has it"* is per-platform, because a message that says "install LLVM" on a
/// machine where the answer is one `winget` line is a message that costs an
/// hour.
pub fn no_backend() -> Diagnostic {
    backend_not_compiled_in("llvm", install_instructions())
}

/// How to obtain LLVM 18.1 on this host.
///
/// Decision 2 pins the version and Decision 1's cost item 5 names the
/// requirement: *"`llvm-config` becomes a build-time requirement for building
/// `sciencec`."* The instruction names the version because an LLVM 19 install
/// satisfies `llvm-config` and fails `llvm-sys = "181"`, which is a confusing
/// second failure to hit after fixing the first.
pub fn install_instructions() -> &'static str {
    if cfg!(target_os = "windows") {
        "install LLVM 18.1.8 — `winget install LLVM.LLVM --version 18.1.8` or \
         `choco install llvm --version=18.1.8` — then set `LLVM_SYS_181_PREFIX` to the \
         install root, and rebuild `sciencec` with the `science-codegen-llvm` backend"
    } else if cfg!(target_os = "macos") {
        "install LLVM 18.1 — `brew install llvm@18` — then set `LLVM_SYS_181_PREFIX` to \
         `$(brew --prefix llvm@18)`, and rebuild `sciencec` with the `science-codegen-llvm` \
         backend"
    } else {
        "install LLVM 18.1 development files — `apt install llvm-18-dev` or your \
         distribution's equivalent — then ensure `llvm-config-18` is on `PATH` or set \
         `LLVM_SYS_181_PREFIX`, and rebuild `sciencec` with the `science-codegen-llvm` backend"
    }
}

/// Whether this `sciencec` can produce an executable at all.
///
/// A caller that wants to say so before doing any work — a `--version` that
/// lists capabilities, say — asks this rather than inspecting [`BACKENDS`].
pub fn can_build() -> bool {
    !BACKENDS.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{code, is_claimed};

    #[test]
    fn a_build_reports_sc0400_and_not_a_panic() {
        let request = BuildRequest::new(vec!["hello.science".to_string()]);
        let diagnostics = build(&request).unwrap_err();
        let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();
        assert!(codes.contains(&code::SC0400), "{codes:?}");
        for code in codes {
            assert!(is_claimed(code), "{code} is outside this note's range");
        }
    }

    #[test]
    fn sc0400_names_the_feature_and_the_command_that_installs_it() {
        let diagnostic = no_backend();
        assert_eq!(diagnostic.code, code::SC0400);
        assert!(diagnostic.message.contains("llvm"));
        let notes = diagnostic.notes.join("\n");
        assert!(notes.contains("18.1"), "the pinned version has to be in the message: {notes}");
        assert!(notes.contains("LLVM_SYS_181_PREFIX"), "{notes}");
    }

    #[test]
    fn the_instructions_name_a_real_package_manager_for_this_host() {
        let text = install_instructions();
        assert!(text.contains("LLVM_SYS_181_PREFIX"));
        if cfg!(target_os = "windows") {
            assert!(text.contains("winget") || text.contains("choco"));
        }
    }

    #[test]
    fn asking_for_another_target_is_sc0406_as_well_as_sc0400() {
        // Both, and in that order: the target is wrong *and* there is no
        // backend. Reporting only the first would make fixing it produce a
        // second failure, which is the shape of error message people hate.
        let other = Triple::ALL.iter().copied().find(|t| Some(*t) != Triple::host());
        let Some(other) = other else { return };
        let mut request = BuildRequest::new(vec!["a.science".to_string()]);
        request.target = Some(other);
        let diagnostics = build(&request).unwrap_err();
        let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();
        if Triple::host().is_some() {
            assert!(codes.contains(&code::SC0406), "{codes:?}");
        }
        assert!(codes.contains(&code::SC0400), "{codes:?}");
    }

    #[test]
    fn building_for_the_host_explicitly_is_not_a_cross_compilation() {
        let Some(host) = Triple::host() else { return };
        let mut request = BuildRequest::new(vec!["a.science".to_string()]);
        request.target = Some(host);
        let diagnostics = build(&request).unwrap_err();
        let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();
        assert!(!codes.contains(&code::SC0406), "{codes:?}");
    }

    #[test]
    fn this_compiler_says_plainly_that_it_cannot_build() {
        assert!(!can_build());
        assert!(BACKENDS.is_empty());
    }

    #[test]
    fn the_default_request_is_an_executable_at_o2_for_the_host() {
        let request = BuildRequest::new(vec![]);
        assert_eq!(request.emit, EmitKind::Executable);
        assert_eq!(request.opt, OptLevel::O2);
        assert_eq!(request.target, None);
        assert!(!request.no_noalias);
    }
}
