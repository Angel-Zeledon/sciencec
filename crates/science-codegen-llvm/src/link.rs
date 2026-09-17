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
//! 3. **`-fuse-ld` is not passed**, so the linker is whatever `clang` picks —
//!    `lld-link` on Windows when it is beside it, `ld.lld`/`ld.bfd` on Linux.
//!    That is one more thing the build record does not name, and §12 item 12
//!    already asks for the pipeline and the CPU to be recorded; the linker
//!    belongs on the same list.
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

/// Link one object file and the runtime into an executable.
///
/// **Link order is the thing that goes wrong first** (§5.4): the program's
/// objects, then `science-rt`, then the declared libraries in dependency order,
/// then the system libraries. There are no declared libraries yet — stage 2's
/// `extern` blocks are not lowered — so the order here is three of the four.
pub fn link(
    driver: &Driver,
    object: &Path,
    runtime: &Path,
    output: &Path,
    triple: Triple,
) -> Result<(), LinkError> {
    let mut command = Command::new(&driver.program);
    command.arg(object);
    command.arg(runtime);
    command.arg("-o").arg(output);
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
    /// `SC0402`."* There is no `extern` declaration table yet, because stage 2
    /// is not built, so every failure here is the second case, which is the
    /// honest one: *"a diagnostic that pretends to have understood a linker
    /// error it did not understand is worse than one that hands over the raw
    /// text with the command that produced it."*
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
