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
    /// `via pkg-config` asked a `pkg-config` that was there, and it said no.
    /// `SC0460`. See [`library_arguments`] for why this is not the fallback.
    PkgConfigFailed {
        /// The `library` clause's own name, which is the fallback that was
        /// deliberately not taken.
        library: String,
        /// The module named by the clause.
        module: String,
        /// The `pkg-config` command, as it was run.
        command: String,
        /// Everything `pkg-config` said, verbatim.
        output: String,
    },
    /// `kind static` named a library with no archive anywhere on the search
    /// path. `SC0460`.
    NoStaticArchive {
        /// The `library` clause's name.
        library: String,
        /// The file that was looked for.
        file: String,
        /// Every path that was tried, in order.
        searched: Vec<String>,
    },
    /// `when available` reached the link step, and §5.2's `dlopen` table is not
    /// written. `SC0400` — a construct this backend does not lower.
    OptionalLibraryNotLowered {
        /// The `library` clause's name.
        library: String,
    },
    /// Two `extern` blocks name one library with different clauses. `SC0460`.
    ClausesDisagree {
        /// The shared name.
        library: String,
        /// One block's clause.
        one: String,
        /// The other's.
        other: String,
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

/// A library a program's `extern` blocks asked to be linked against, and the
/// three clauses of §5.1 and §5.2 that say *how*.
///
/// # What this used to be, and the comment that was not true
///
/// This was `pub type Library = String;` — a name and nothing else — under a
/// doc comment claiming that *"`via pkg-config`, `kind static` and
/// `when available` … are refused above, by name, rather than carried here
/// half-implemented"*. **No such refusal existed anywhere.** All three parsed,
/// all three reached `hir::ExternLibrary`, and all three were dropped on the
/// way down: four `extern` blocks declaring `library "lzma"` and differing only
/// in their clause passed byte-identical `-llzma` to the linker. `via
/// pkg-config "totally-fake-pkg"` compiled, linked and ran on a machine with no
/// `pkg-config` installed at all, and `when available` — whose entire meaning
/// is *"do not make this a load-time dependency"* — emitted a hard `-lcudnn`
/// and failed the link with `ld: library 'cudnn' not found`, which is the
/// opposite of what the clause asks for.
///
/// So the comment was not describing a decision, it was describing a decision
/// nobody had taken. [`library_arguments`] is where each clause is now either
/// honoured or refused by name, and there is no third thing it can do.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Library {
    /// `library "openblas"`: the link target.
    pub name: String,
    /// `via pkg-config "openblas"`: the module whose `--libs` replaces the
    /// plain name, when `pkg-config` is there to ask.
    pub pkg_config: Option<String>,
    /// `kind static`: link the archive rather than the shared object.
    pub static_link: bool,
    /// `when available`: §5.2's optional library, which must not become a
    /// load-time dependency.
    pub when_available: bool,
}

impl Library {
    /// A plain `library "name"` with no clause on it.
    pub fn named(name: impl Into<String>) -> Library {
        Library { name: name.into(), ..Library::default() }
    }

    /// The clause as it reads in source, for a diagnostic that has to name it.
    ///
    /// A message that says only *"`lzma` could not be linked"* for a block
    /// whose `library "lzma"` is perfectly good and whose `kind static` is the
    /// unhonourable half sends the reader to look at the wrong word.
    pub fn clause(&self) -> String {
        let mut text = String::new();
        if let Some(module) = &self.pkg_config {
            text.push_str(&format!("via pkg-config \"{module}\""));
        }
        for (flag, spelling) in
            [(self.static_link, "kind static"), (self.when_available, "when available")]
        {
            if flag {
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(spelling);
            }
        }
        text
    }
}

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

/// Everything the `library` clauses add to the link line, in Decision 28's
/// source order — or the first clause that could not be honoured.
///
/// **One function, three clauses, and each of them ends in exactly one of two
/// places.** The thing this replaces silently ignored all three; the rule here
/// is that a clause either changes the arguments or produces an error naming
/// itself, and there is no path through which a clause does nothing.
///
/// # `when available` is refused, and that is the honest half of it
///
/// §5.2's full feature is *"a lazily-initialized table of function pointers,
/// populated on first use by `dlopen`/`dlsym` … inside `science-rt`"*, a
/// generated `is_available() -> Bool` binding, and every call in the block
/// turned into an indirect call through the table. None of that exists:
/// `dlopen` appears in this tree exactly once, in a design note, and
/// `is_available()` resolves to nothing (`SC0200`). Dropping the `-l` alone
/// would not make the clause work — the block's symbols are still emitted as
/// direct `declare`s, so the link would fail with *undefined symbol* instead of
/// *library not found*, which is a different wrong answer rather than a right
/// one. What is in reach in one change is to stop the compiler pretending, so
/// the clause is refused by name, with the message saying which piece is
/// missing. A block that is never called is never in this list — Decision 28's
/// filter already drops it — so a program that merely *declares* an optional
/// library still builds.
///
/// # `via pkg-config` falls back only when `pkg-config` is absent
///
/// `SC0412` states the intent: *"a block discovering its flags with `via
/// pkg-config` still declares the plain name as the fallback, because
/// pkg-config is not present on every platform."* **Absent** is the word doing
/// the work. Three outcomes and they are not the same:
///
/// 1. **`pkg-config` is not installed.** Fall back to the plain name. That is
///    the sentence above and the reason `--libs` is not a hard requirement.
/// 2. **`pkg-config` ran and does not know the module.** *Refused.* This was
///    the fallback too, and it is how `via pkg-config "opneblas"` reaches
///    production: the typo produces a working build on the developer's machine,
///    where the plain name resolves, and the wrong flags on the HPC node the
///    clause exists for. A `pkg-config` that is present and says *"No package
///    'openblas' found"* has answered the question, and the answer is not
///    "use the fallback"; it is that the `.pc` file this build depends on is
///    not installed.
/// 3. **`pkg-config` ran and knows it.** Its `--libs` output replaces the plain
///    `-l` entirely. §5.1's whole argument is that the right flags are
///    `-lopenblas` here, `-lblas -llapack` there and six MKL libraries
///    somewhere else; appending the plain name to that would put back the one
///    guess the clause exists to remove.
///
/// # `kind static` names the archive, because no flag says this portably
///
/// `-Wl,-Bstatic` is GNU `ld` and `lld`; Apple's `ld64` has no such flag, and
/// `-l:libfoo.a` is a GNU extension. The one spelling that means *this archive
/// and not the shared object* on every linker is the archive's own path, so
/// `kind static` resolves `libNAME.a` (`NAME.lib` on MSVC) against
/// `$SCIENCE_LIBRARY_PATH` and then the target's default directories, and
/// passes the path it found. Not finding it is a refusal and not a quiet
/// downgrade to `-lNAME`, because a quiet downgrade is the bug this whole
/// function exists to remove: `kind static` on a machine with only
/// `libfoo.dylib` would link dynamically and say nothing.
///
/// **The MSVC caveat is real and is not hidden.** A static archive and an
/// import library are both `.lib` there, so resolving the path pins *which
/// file* is linked without proving it is an archive. That is less than the
/// clause promises, and it is still strictly more than passing `-lNAME`.
pub fn library_arguments(triple: Triple, libraries: &[Library]) -> Result<Vec<String>, LinkError> {
    let mut arguments = Vec::new();
    for library in libraries {
        if let Some(other) =
            libraries.iter().find(|other| other.name == library.name && *other != library)
        {
            return Err(LinkError::ClausesDisagree {
                library: library.name.clone(),
                one: library.clause(),
                other: other.clause(),
            });
        }
        arguments.extend(one_library(triple, library)?);
    }
    Ok(arguments)
}

/// One library's arguments. See [`library_arguments`] for the whole argument.
fn one_library(triple: Triple, library: &Library) -> Result<Vec<String>, LinkError> {
    if library.when_available {
        return Err(LinkError::OptionalLibraryNotLowered { library: library.name.clone() });
    }
    // The UCRT names, which resolve to no flag on MSVC whatever else is on the
    // clause: there is no `m.lib` to find statically either.
    let plain = library_flags(triple, &library.name);
    if plain.is_empty() {
        return Ok(plain);
    }
    if let Some(module) = &library.pkg_config {
        if let Some(flags) = pkg_config_libs(&library.name, module, library.static_link)? {
            return Ok(flags);
        }
    }
    if library.static_link {
        return static_archive(triple, library).map(|path| vec![path]);
    }
    Ok(plain)
}

/// The `pkg-config` to ask, which is a variable because cross-compiling asks a
/// different one.
///
/// A cross build wants `aarch64-linux-gnu-pkg-config`, and a build that has to
/// point at a private `.pc` tree wants its own wrapper. `$SCIENCE_PKG_CONFIG`
/// is the same escape hatch `$SCIENCE_LINKER` already is, and it is what lets
/// the tests in this crate exercise all three of [`library_arguments`]'s
/// `pkg-config` outcomes on a machine where `pkg-config` itself is not
/// installed.
fn pkg_config_program() -> std::ffi::OsString {
    std::env::var_os("SCIENCE_PKG_CONFIG").unwrap_or_else(|| "pkg-config".into())
}

/// `pkg-config --libs`, or `None` when there is no `pkg-config` to run.
///
/// `None` is *absent* and nothing else. A `pkg-config` that started and exited
/// non-zero has answered the question — see [`library_arguments`].
fn pkg_config_libs(
    library: &str,
    module: &str,
    static_link: bool,
) -> Result<Option<Vec<String>>, LinkError> {
    let program = pkg_config_program();
    let mut command = Command::new(&program);
    if static_link {
        // `--static` is what turns `Libs:` into `Libs: + Libs.private:`, which
        // is the transitive closure a static link needs and a dynamic one does
        // not.
        command.arg("--static");
    }
    command.arg("--libs").arg(module);
    let rendered = render(&command);
    match command.output() {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(LinkError::PkgConfigFailed {
            library: library.to_string(),
            module: module.to_string(),
            command: rendered,
            output: error.to_string(),
        }),
        Ok(result) if result.status.success() => Ok(Some(
            String::from_utf8_lossy(&result.stdout)
                .split_whitespace()
                .map(str::to_string)
                .collect(),
        )),
        Ok(result) => {
            let mut output = String::from_utf8_lossy(&result.stdout).into_owned();
            output.push_str(&String::from_utf8_lossy(&result.stderr));
            Err(LinkError::PkgConfigFailed {
                library: library.to_string(),
                module: module.to_string(),
                command: rendered,
                output,
            })
        }
    }
}

/// The static archive `kind static` asked for, found by name in the search
/// path, or the refusal that names everywhere it was not.
fn static_archive(triple: Triple, library: &Library) -> Result<String, LinkError> {
    let name = &library.name;
    let file = match triple {
        Triple::X86_64WindowsMsvc => format!("{name}.lib"),
        _ => format!("lib{name}.a"),
    };
    let mut searched = Vec::new();
    for directory in library_search_paths(triple) {
        let candidate = directory.join(&file);
        searched.push(candidate.display().to_string());
        if candidate.is_file() {
            return Ok(candidate.display().to_string());
        }
    }
    Err(LinkError::NoStaticArchive { library: name.clone(), file, searched })
}

/// Where a library is looked for: §5.1's list, minus the part that does not
/// exist yet.
///
/// §5.1 gives three sources in priority order — `--library-path`, then
/// `$SCIENCE_LIBRARY_PATH`, then the system defaults. **There is no
/// `--library-path` flag in this compiler**, so this is the second and the
/// third; the first is named here rather than silently omitted, because an
/// omission that is not written down is how the clauses above came to be
/// ignored in the first place.
///
/// The defaults are the ones the platform's own linker searches. On MSVC that
/// is `$LIB`, which the environment of a developer prompt sets and `clang`
/// reconstructs through `vswhere`; there is no fixed directory to name.
fn library_search_paths(triple: Triple) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let separator = if cfg!(windows) { ';' } else { ':' };
    if let Some(value) = std::env::var_os("SCIENCE_LIBRARY_PATH") {
        for entry in value.to_string_lossy().split(separator) {
            if !entry.is_empty() {
                paths.push(PathBuf::from(entry));
            }
        }
    }
    match triple {
        Triple::X86_64WindowsMsvc => {
            if let Some(value) = std::env::var_os("LIB") {
                for entry in value.to_string_lossy().split(';') {
                    if !entry.is_empty() {
                        paths.push(PathBuf::from(entry));
                    }
                }
            }
        }
        Triple::Aarch64AppleDarwin => {
            paths.extend(["/opt/homebrew/lib", "/usr/local/lib", "/usr/lib"].map(PathBuf::from));
        }
        Triple::X86_64LinuxGnu => {
            paths.extend(
                [
                    "/usr/local/lib",
                    "/usr/lib/x86_64-linux-gnu",
                    "/usr/lib64",
                    "/usr/lib",
                    "/lib/x86_64-linux-gnu",
                    "/lib",
                ]
                .map(PathBuf::from),
            );
        }
    }
    paths
}

/// Link one object file, the runtime and the declared libraries into an
/// executable, and give back the command line that did it.
///
/// **Link order is the thing that goes wrong first** (§5.4): the program's
/// objects, then `science-rt`, then the declared libraries in dependency
/// order, then the system libraries. `libraries` is Decision 28's *"source
/// order of their `library` clauses"*, and it is the third of the four.
///
/// **The command line is returned on success as well as on failure, and that
/// is what made the clauses testable.** `SC0402` has always printed it when the
/// linker failed, so the only way to see what a *successful* build passed was
/// to wrap the driver in a shell script — which is how the defect this function
/// now fixes was found and is not something a test can do portably. `Built`
/// carries it for the same reason it already carries the resolved driver: §12
/// item 12 wants the build recorded, and *which libraries were linked and how*
/// is the part of that record the `library` clauses decide.
pub fn link(
    driver: &Driver,
    object: &Path,
    runtime: &Path,
    output: &Path,
    triple: Triple,
    libraries: &[Library],
) -> Result<String, LinkError> {
    let mut command = Command::new(&driver.program);
    command.arg(object);
    command.arg(runtime);
    command.arg("-o").arg(output);
    for argument in library_arguments(triple, libraries)? {
        command.arg(argument);
    }
    for library in system_libraries(triple) {
        command.arg(format!("-l{library}"));
    }
    let rendered = render(&command);
    match command.output() {
        Err(error) => {
            Err(LinkError::NotRunnable { command: rendered, reason: error.to_string() })
        }
        Ok(result) if result.status.success() => Ok(rendered),
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
///
/// **Tried both bare and with Mach-O's leading underscore, as two whole
/// words and not as one looser boundary.** The doc comment above this
/// function's caller has always claimed the underscore is admitted; the
/// check here used to require the character *before* the match to be
/// neither alphanumeric *nor* `_`, which is exactly the character Apple's
/// `ld` puts there — `"_cosinus", referenced from:` — so the admission the
/// comment promised never happened and this is the first host to run the
/// test that would have shown it. Loosening the boundary itself to accept a
/// leading `_` would also accept `cosinus` as a match inside
/// `my_cosinus_thing`, which is the false positive the boundary check
/// exists to refuse; matching `_symbol` as its own whole word instead keeps
/// the refusal and admits the platform's mangling.
fn mentions_symbol(output: &str, symbol: &str) -> bool {
    is_whole_word(output, symbol) || is_whole_word(output, &format!("_{symbol}"))
}

/// Whether `output` contains `needle` with a non-identifier character (or
/// the string's edge) on both sides.
fn is_whole_word(output: &str, needle: &str) -> bool {
    let mut from = 0;
    while let Some(found) = output[from..].find(needle) {
        let start = from + found;
        let end = start + needle.len();
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
            LinkError::PkgConfigFailed { library, module, command, output } => {
                science_codegen::diagnostics::library_clause_unhonoured(
                    library,
                    &format!("via pkg-config \"{module}\""),
                    &format!("`pkg-config` ran and could not answer for `{module}`"),
                    &[
                        format!("the command was: {command}"),
                        "what follows is `pkg-config`'s own output, verbatim:".to_string(),
                        output.trim_end().to_string(),
                        "a `pkg-config` that is *absent* falls back to the plain `library` \
                         name, because it is not on every platform. One that is present and \
                         does not know the module has answered, and the answer is that the \
                         `.pc` file this build depends on is not installed — falling back \
                         there is how a typo in the module name ships."
                            .to_string(),
                        "install the module's development package, point `$PKG_CONFIG_PATH` at \
                         its `.pc` file, set `$SCIENCE_PKG_CONFIG` to the `pkg-config` to use, \
                         or drop the `via pkg-config` clause and link the plain name"
                            .to_string(),
                    ],
                )
            }
            LinkError::NoStaticArchive { library, file, searched } => {
                science_codegen::diagnostics::library_clause_unhonoured(
                    library,
                    "kind static",
                    &format!("no `{file}` was found, so there is no archive to link"),
                    &[
                        format!("searched, in order:\n  {}", searched.join("\n  ")),
                        "`kind static` names the archive's path, because no linker flag means \
                         *this archive and not the shared object* on all three platforms: \
                         `-Wl,-Bstatic` is GNU `ld`'s and Apple's `ld64` has no equivalent"
                            .to_string(),
                        "add the directory holding the archive to `$SCIENCE_LIBRARY_PATH`, or \
                         drop `kind static` and link the shared object. Linking the shared \
                         object silently is what this refusal replaces."
                            .to_string(),
                    ],
                )
            }
            LinkError::ClausesDisagree { library, one, other } => {
                science_codegen::diagnostics::library_clause_unhonoured(
                    library,
                    one,
                    &format!(
                        "another `extern` block declares `library \"{library}\"` too, with a \
                         different clause on it"
                    ),
                    &[
                        format!(
                            "one block says `{}` and the other says `{}`",
                            if one.is_empty() { "(no clause)" } else { one },
                            if other.is_empty() { "(no clause)" } else { other },
                        ),
                        "one library is linked once, so one of the two clauses would have to \
                         lose. Which one is not something this compiler should decide quietly: \
                         give both blocks the same clause, or give them different `library` \
                         names."
                            .to_string(),
                    ],
                )
            }
            LinkError::OptionalLibraryNotLowered { library } => {
                science_codegen::diagnostics::construct_not_lowered(
                    &format!("a call into `library \"{library}\" when available`"),
                    "§5.2's optional library is a lazily-initialised table of function \
                     pointers filled in by `dlopen`/`dlsym` inside `science-rt`, a generated \
                     `is_available() -> Bool`, and an indirect call per foreign call. None of \
                     the three is written yet: `is_available()` resolves to nothing (`SC0200`) \
                     and nothing in `science-rt` loads a library at run time.",
                )
                .with_note(format!(
                    "this is refused rather than linked as `-l{library}`, which is what it used \
                     to do — a hard load-time dependency on the library the clause exists to \
                     make optional, failing the link on exactly the machine `when available` \
                     was written for"
                ))
                .with_note(
                    "declaring the block is still fine; only calling into it is refused. Drop \
                     `when available` to depend on the library at load time."
                        .to_string(),
                )
            }
        }
    }
}
