//! The link step: §5, and the one decision in it that this crate does not take
//! the note's answer to.
//!
//! **The decision.** `sciencec` invokes **`clang`** as the linker driver on
//! Windows, not `link.exe`. The search order is `SCIENCE_LINKER`, then `CC`,
//! then `clang` beside the LLVM installation this backend already depends on,
//! then the platform default (`cc` on Unix, `clang` on Windows).
//!
//! **What the note says, and why this departs from it.** Decision 25:
//!
//! > *`sciencec` does not invoke a linker directly. It invokes the platform's C
//! > compiler driver as the linker driver: `cc` on Linux, `cc` on macOS, and
//! > `link.exe` on Windows … **Windows is the exception and it is not one.**
//! > MSVC has no `cc`-shaped driver; `cl.exe` in link mode is awkward and
//! > `link.exe` is the actual tool. The substantive difference is flag syntax —
//! > `/OUT:` rather than `-o`, `foo.lib` rather than `-lfoo`, `/LIBPATH:`
//! > rather than `-L` — which is a translation table, not a second design.*
//!
//! The flag table is not the problem. **`link.exe` is not findable.** It is not
//! on `PATH` outside a Visual Studio developer prompt, and invoking it needs
//! `LIB` and `LIBPATH` pointing at the MSVC CRT and the Windows SDK — which
//! means either running `vcvarsall.bat` and harvesting its environment, or
//! reimplementing `vswhere`'s registry walk and the SDK version search. That is
//! precisely the work Decision 25 gives as the *reason* to use a C driver:
//!
//! > *A C compiler driver knows where `crt1.o`, `crti.o` and `crtn.o` live,
//! > what the dynamic loader path is, which libc variant is installed … Invoking
//! > `ld` directly means reimplementing all of that.*
//!
//! `link.exe` is Windows' `ld`, not Windows' `cc`. Naming it in the driver row
//! reads the platform's tools one notch too low, and the argument in Decision
//! 25's own prose points the other way from the choice it makes.
//!
//! **`clang` is the `cc`-shaped driver Decision 25 says MSVC does not have.**
//! It ships with the LLVM 18.1.8 installation the backend already requires, it
//! is the same version Decision 2 pins, it finds the MSVC toolchain itself
//! through `vswhere`, and it takes `-o` and `-lfoo` on Windows exactly as on
//! Linux — so the translation table Decision 25 budgets for is not needed
//! either. §5.2's Windows row is then the same shape as the other two.
//!
//! **What it costs, and there are four things.**
//!
//! 1. **An LLVM installation becomes a *runtime* requirement of `sciencec` and
//!    not only a build-time one.** Decision 1's cost item 5 said `llvm-config`
//!    at build time; this adds `clang` at link time. On a machine with MSVC and
//!    no LLVM, `sciencec build` cannot link, and `SCIENCE_LINKER` is the escape
//!    hatch — except that the flags passed are `cc`-shaped, so the escape hatch
//!    only reaches another `cc`-shaped driver. **Nothing here can drive
//!    `link.exe`.**
//! 2. **MSVC is still required underneath.** `clang` targeting
//!    `x86_64-pc-windows-msvc` links against the MSVC CRT and the Windows SDK,
//!    so the install-time requirement Decision 25 names is routed rather than
//!    removed.
//! 3. **`-fuse-ld` is not passed**, so the linker is whatever `clang` picks, and
//!    **on this machine it is not `lld-link`.** `clang -v` shows it invoking
//!    `…/VC/Tools/MSVC/<version>/bin/Hostx64/x64/link.exe`, found through
//!    `vswhere`, with the MSVC and Windows SDK `-libpath:` flags filled in — so
//!    the tool Decision 25 names *is* what links, and what `clang` supplies is
//!    exactly the environment discovery this module says is the reason not to
//!    invoke it directly. `lld-link.exe` sits beside `clang.exe` in the LLVM
//!    installation and is not chosen; `clang` prefers the platform linker for
//!    an MSVC target. That is one more thing the build record does not name, and
//!    §12 item 12 already asks for the pipeline and the CPU to be recorded; the
//!    linker belongs on the same list, and *which* linker — not just which
//!    driver — is the part that is missing, because [`Driver`] records the
//!    program it ran and the program it ran is a driver.
//! 4. **`SCIENCE_LINKER` and `CC` are environment variables**, and
//!    `package-manager.md` Decision 7 removes environment-dependent inputs from
//!    output for reproducibility. The resolved driver is therefore returned
//!    beside the result so a build record can carry it. Nothing writes that
//!    record yet.
//!
//! # The Windows import libraries nobody wrote down
//!
//! Decision 27 links `science-rt` statically into every binary and §5.2's table
//! gives the Windows artefact as `science_rt.lib`. That is true and it is not
//! sufficient: `science-rt` is a Rust `staticlib`, so the archive contains
//! Rust's `std`, and `std` on `x86_64-pc-windows-msvc` has undefined references
//! into five Windows import libraries. `rustc --print native-static-libs` names
//! them, and they are [`WINDOWS_SYSTEM_LIBRARIES`] below.
//!
//! Without them the link fails with thirty `LNK2019`s naming `WSAStartup` and
//! `NtReadFile` — symbols from the socket and named-pipe code in `std` that no
//! Science program will ever reach, dragged in because a Rust `staticlib` is
//! not a granular archive. **That is a fact about `science-rt`'s crate type, not
//! about Science**, and it is the first thing §5's link-order paragraph would
//! have hit. It is also an argument for `science-rt` eventually being
//! `#![no_std]`, which its own module documentation gestures at and does not
//! take.

use std::path::{Path, PathBuf};
use std::process::Command;

use science_codegen::layout::Triple;

/// The system import libraries a Rust `staticlib` needs on
/// `x86_64-pc-windows-msvc`.
///
/// From `rustc --print native-static-libs` over `science-rt`, which is the only
/// authority on this: the list changes with the standard library, not with
/// Science.
pub const WINDOWS_SYSTEM_LIBRARIES: [&str; 5] =
    ["kernel32", "ntdll", "userenv", "ws2_32", "dbghelp"];

/// The system libraries a Rust `staticlib` needs on `x86_64-unknown-linux-gnu`.
///
/// Not exercised: `Triple::host()` is Windows on the machine this was written
/// on, and §5.6 makes anything else `SC0406`. Written down because the Windows
/// list above would otherwise look like a Science requirement rather than a
/// Rust-`staticlib` one.
pub const LINUX_SYSTEM_LIBRARIES: [&str; 4] = ["dl", "pthread", "m", "rt"];

/// What went wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    /// No linker driver was found. `SC0401`.
    NoDriver {
        /// The candidates tried, in order.
        candidates: Vec<String>,
    },
    /// The runtime archive was not found. Decision 27 links it into every
    /// binary and there is no build without it.
    NoRuntime {
        /// Where the search looked.
        searched: Vec<String>,
    },
    /// The driver ran and failed. `SC0402` — *"prints the command line and the
    /// linker's output verbatim, and says that it is doing so"*.
    Failed {
        /// The command, as it was run.
        command: String,
        /// Everything the driver said.
        output: String,
    },
    /// The driver could not be started at all.
    NotRunnable {
        /// The command.
        command: String,
        /// The operating system's reason.
        reason: String,
    },
}

/// A resolved linker driver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Driver {
    /// The program to run.
    pub program: PathBuf,
    /// Where it came from, for a build record and for `SC0401`'s message.
    pub source: &'static str,
}

/// Find a linker driver.
///
/// Decision 25's order, with one addition: the `clang` beside the LLVM
/// installation this backend already links, which is the only candidate
/// guaranteed to be the version Decision 2 pins.
pub fn find_driver() -> Result<Driver, LinkError> {
    let mut candidates: Vec<String> = Vec::new();

    if let Some(path) = std::env::var_os("SCIENCE_LINKER") {
        let path = PathBuf::from(path);
        candidates.push(format!("$SCIENCE_LINKER = {}", path.display()));
        if is_runnable(&path) {
            return Ok(Driver { program: path, source: "SCIENCE_LINKER" });
        }
    }
    if let Some(path) = std::env::var_os("CC") {
        let path = PathBuf::from(path);
        candidates.push(format!("$CC = {}", path.display()));
        if is_runnable(&path) {
            return Ok(Driver { program: path, source: "CC" });
        }
    }
    for prefix in crate::llvm_prefixes() {
        let path = prefix.join("bin").join(if cfg!(windows) { "clang.exe" } else { "clang" });
        candidates.push(path.display().to_string());
        if path.is_file() {
            return Ok(Driver { program: path, source: "the LLVM installation" });
        }
    }
    for name in default_drivers() {
        candidates.push((*name).to_string());
        let path = PathBuf::from(name);
        if is_runnable(&path) {
            return Ok(Driver { program: path, source: "PATH" });
        }
    }
    Err(LinkError::NoDriver { candidates })
}

fn default_drivers() -> &'static [&'static str] {
    if cfg!(windows) { &["clang.exe", "clang"] } else { &["cc", "clang", "gcc"] }
}

/// Whether a program can be started. `--version` rather than `Path::is_file`,
/// because `CC=cc` is a name on `PATH` and not a path.
fn is_runnable(program: &Path) -> bool {
    Command::new(program)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// The static runtime archive Decision 27 links into every binary.
///
/// Searched, in order: `$SCIENCE_RT_LIB`, then beside the running executable,
/// then one directory up — which covers `target/debug/sciencec.exe` and
/// `target/debug/deps/<test>.exe` respectively, and is where
/// `cargo build -p science-rt` puts it.
///
/// **It is not built as a side effect of building `sciencec`.** Cargo builds a
/// dependency's `rlib` and only a dependency's `rlib`; the `staticlib` in
/// `science-rt`'s `crate-type` list is produced when that package is built
/// directly. So `cargo build -p science-rt` is a step, and [`LinkError::NoRuntime`]
/// says so rather than leaving a linker error to explain it.
pub fn find_runtime() -> Result<PathBuf, LinkError> {
    let name = if cfg!(windows) { "science_rt.lib" } else { "libscience_rt.a" };
    let mut searched = Vec::new();

    if let Some(path) = std::env::var_os("SCIENCE_RT_LIB") {
        let path = PathBuf::from(path);
        searched.push(format!("$SCIENCE_RT_LIB = {}", path.display()));
        if path.is_file() {
            return Ok(path);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        for directory in exe.parent().into_iter().chain(exe.parent().and_then(|p| p.parent())) {
            let candidate = directory.join(name);
            searched.push(candidate.display().to_string());
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(LinkError::NoRuntime { searched })
}

/// The system libraries for a target.
pub fn system_libraries(triple: Triple) -> Vec<&'static str> {
    match triple {
        Triple::X86_64WindowsMsvc => WINDOWS_SYSTEM_LIBRARIES.to_vec(),
        Triple::X86_64LinuxGnu => LINUX_SYSTEM_LIBRARIES.to_vec(),
        // macOS resolves all of this through libSystem, which the driver links
        // by default.
        Triple::Aarch64AppleDarwin => Vec::new(),
    }
}

/// A library a program's `extern` blocks asked to be linked against.
///
/// A name and nothing else: `via pkg-config`, `kind static` and
/// `when available` are §5.4's and are refused above, by name, rather than
/// carried here half-implemented.
pub type Library = String;

/// The `-l` flags one library name becomes on a target.
///
/// **Two names are special on Windows and the specialness is real rather than
/// a convenience.** §5.2's table gives the Windows link name as `openblas.lib`
/// and `clang` accepts `-lopenblas` for it, so the translation Decision 25
/// budgets for is not needed — *except* for `c` and `m`, which name the POSIX
/// C standard library and its maths half. On MSVC those are not separate
/// libraries at all: both are inside the UCRT, which the driver already links
/// into every image, and there is no `c.lib` or `m.lib` for `link.exe` to
/// find. Passing them through produces `LNK1181: cannot open input file
/// 'm.lib'` for a program whose only sin was to declare `library "m"` the way
/// §10's stage 2 spells it.
///
/// So on `x86_64-pc-windows-msvc` those two names resolve to **no flag**, and
/// the symbols resolve out of the CRT the driver links anyway. Everything else
/// is passed through unchanged, on all three targets.
///
/// **What this costs:** a Windows user who genuinely has a third-party
/// `m.lib` cannot name it. That is a narrow loss against making the note's own
/// stage-2 program unbuildable on the platform this compiler is developed on.
pub fn library_flags(triple: Triple, name: &str) -> Vec<String> {
    if triple == Triple::X86_64WindowsMsvc && matches!(name, "c" | "m") {
        return Vec::new();
    }
    vec![format!("-l{name}")]
}

/// Link one object file, the runtime and the declared libraries into an
/// executable.
///
/// **Link order is the thing that goes wrong first** (§5.4): the program's
/// objects, then `science-rt`, then the declared libraries in dependency
/// order, then the system libraries. `libraries` is Decision 28's *"source
/// order of their `library` clauses"*, and it is the third of the four.
pub fn link(
    driver: &Driver,
    object: &Path,
    runtime: &Path,
    output: &Path,
    triple: Triple,
    libraries: &[Library],
) -> Result<(), LinkError> {
    let mut command = Command::new(&driver.program);
    command.arg(object);
    command.arg(runtime);
    command.arg("-o").arg(output);
    for library in libraries {
        for flag in library_flags(triple, library) {
            command.arg(flag);
        }
    }
    for library in system_libraries(triple) {
        command.arg(format!("-l{library}"));
    }
    let rendered = render(&command);
    match command.output() {
        Err(error) => {
            Err(LinkError::NotRunnable { command: rendered, reason: error.to_string() })
        }
        Ok(result) if result.status.success() => Ok(()),
        Ok(result) => {
            let mut output = String::from_utf8_lossy(&result.stdout).into_owned();
            output.push_str(&String::from_utf8_lossy(&result.stderr));
            Err(LinkError::Failed { command: rendered, output })
        }
    }
}

/// Which declared foreign symbols a linker's output says are undefined.
///
/// **Decision 29's condition, made a function rather than a guess.** The
/// decision is that `SC0461` is emitted *"for an undefined symbol that
/// codegen's own table can attribute to an `extern` declaration"*, and §5.5's
/// rule for everything else is that a diagnostic which *"pretends to have
/// understood a linker error it did not understand is worse than one that
/// hands over the raw text"*. So this looks for a marker meaning *not found*
/// **and** for the symbol as a whole word, and reports nothing on a failure
/// that merely happens to mention the name.
///
/// # The markers are error codes on Windows, and that is a finding
///
/// The obvious marker is `link.exe`'s prose, *"unresolved external symbol"*.
/// **`link.exe` is localised.** On the machine this was written on it says:
///
/// ```text
/// p.obj : error LNK2019: símbolo externo cosinus sin resolver al que se hace
/// referencia en la función _S4main
/// ```
///
/// — so an English substring match finds nothing, `SC0461` is never emitted,
/// and §10 stage 2's gate silently fails on every non-English Windows install.
/// It is invisible to a test written on an English machine, which is the only
/// kind of test anyone would have written.
///
/// `LNK2019` and `LNK2001` are **not** translated: they are MSVC diagnostic
/// identifiers, stable across versions and locales, and they are what is
/// matched. The three Unix markers stay prose because `ld`, `lld` and the
/// Apple linker have no such identifiers; a translated GNU `ld` would fall
/// through to `SC0402` with its text intact, which is the honest failure and
/// not a wrong answer.
///
/// **Forcing the linker into English was the other option and it was
/// rejected.** `VSLANG=1033` in the child's environment would make the output
/// matchable — and §5.5 requires that output to be printed *verbatim* to the
/// user, so it would also mean showing a Spanish-speaking user an English
/// linker error to make the compiler's own parsing easier. The error codes
/// cost nothing and take nothing away.
///
/// The symbol match is on a whole word, because `cos` is a substring of `cosh`
/// and of every path with `cos` in it. Mach-O's leading underscore is
/// admitted, since that is the platform's spelling of the same symbol.
pub fn undefined_symbols<'a>(output: &str, declared: &'a [String]) -> Vec<&'a String> {
    const CODES: [&str; 2] = ["LNK2019", "LNK2001"];
    const PROSE: [&str; 3] = ["undefined reference", "undefined symbol", "undefined symbols"];
    let lowered = output.to_lowercase();
    let marked = CODES.iter().any(|code| output.contains(code))
        || PROSE.iter().any(|marker| lowered.contains(marker));
    if !marked {
        return Vec::new();
    }
    declared.iter().filter(|symbol| mentions_symbol(output, symbol)).collect()
}

/// Whether `output` names `symbol` as a whole identifier.
fn mentions_symbol(output: &str, symbol: &str) -> bool {
    let mut from = 0;
    while let Some(found) = output[from..].find(symbol) {
        let start = from + found;
        let end = start + symbol.len();
        let before_ok = output[..start]
            .chars()
            .next_back()
            .map(|c| !c.is_alphanumeric() && c != '_')
            .unwrap_or(true);
        let after_ok = output[end..]
            .chars()
            .next()
            .map(|c| !c.is_alphanumeric() && c != '_')
            .unwrap_or(true);
        if before_ok && after_ok {
            return true;
        }
        from = end;
    }
    false
}

/// The command line, for `SC0402`, which prints it verbatim.
fn render(command: &Command) -> String {
    let mut text = command.get_program().to_string_lossy().into_owned();
    for argument in command.get_args() {
        text.push(' ');
        let argument = argument.to_string_lossy();
        if argument.contains(' ') {
            text.push('"');
            text.push_str(&argument);
            text.push('"');
        } else {
            text.push_str(&argument);
        }
    }
    text
}

impl LinkError {
    /// Render as a diagnostic in this note's range.
    ///
    /// `SC0401` for no driver, `SC0402` for everything else — §5.5's
    /// *"`SC0461` is emitted for an undefined symbol that codegen's own table
    /// can attribute to an `extern` declaration. Any other linker failure is
    /// `SC0402`."*
    ///
    /// **This function only ever produces the second.** The table exists now —
    /// `lower::Lowered::foreign` — but it belongs to the module, not to a
    /// `LinkError`, so `crate::emit_and_link` reads it, calls
    /// [`undefined_symbols`], and pushes an `SC0461` per attributed symbol
    /// **before** pushing this. Both are reported: the `SC0461`s say which
    /// declarations failed and the `SC0402` says what was run, because a reader
    /// whose `library` clause named the wrong library needs the flags as much
    /// as the span. What stays unattributed stays §5.5's honest answer: *"a
    /// diagnostic that pretends to have understood a linker error it did not
    /// understand is worse than one that hands over the raw text with the
    /// command that produced it."*
    pub fn to_diagnostic(&self) -> science_diagnostics::Diagnostic {
        use science_codegen::diagnostics::{linker_failed, no_linker_driver};
        match self {
            LinkError::NoDriver { candidates } => no_linker_driver(candidates),
            LinkError::NoRuntime { searched } => science_codegen::diagnostics::linker_failed(
                "(the linker was never run)",
                &format!(
                    "the Science runtime archive was not found. Decision 27 links it into every \
                     binary and there is no build without it.\nsearched:\n  {}\n\nbuild it with \
                     `cargo build -p science-rt`, or point `$SCIENCE_RT_LIB` at it. Cargo builds \
                     a dependency's rlib and not its staticlib, so building `sciencec` does not \
                     produce it.",
                    searched.join("\n  ")
                ),
            ),
            LinkError::Failed { command, output } => linker_failed(command, output),
            LinkError::NotRunnable { command, reason } => linker_failed(command, reason),
        }
    }
}
