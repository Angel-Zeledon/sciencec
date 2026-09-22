//! The six phases in front of the backend, and the two things §10 asks of
//! every stage: **run the program, assert its output and its exit code.**
//!
//! **Why this is one file and not three copies.** `hello.rs`, `exit_code.rs`
//! and `stage_two_and_three.rs` all need lex → parse → resolve → check → MIR →
//! `BuildInput`, and the copies had already begun to drift: one asserted the
//! fixture checked clean and printed the codes, one did not. Worse, the
//! `BuildInput` a test fills in is the thing under test's *input*, so a field
//! added for stage 2 — `decls`, `externs` — is a field three files have to
//! learn about, and the one that forgets builds a program with no `extern`
//! blocks in it and passes.
//!
//! **What it deliberately does not do** is assert anything about a program. It
//! builds and it runs; every expectation lives in the test that has one.
//!
//! **`dead_code` is allowed, and the reason is the one this file exists for.**
//! A `mod harness;` is compiled separately into *each* including test binary,
//! so anything one file does not use is dead in that binary — and
//! `formatting_boundary.rs` needs only [`lower`] and [`run`], because the
//! function it builds is one no Science source produces. Without this, adding a
//! test file that uses four of the six helpers turns the other two into
//! warnings and the fix people reach for is to delete them.
#![allow(dead_code)]

use science_codegen::driver::BuildRequest;
use science_codegen::target::OptLevel;
use science_codegen_llvm::{BuildInput, Built, build};
use science_diagnostics::{Diagnostics, FileId};
use science_mir::mir::Body;
use science_resolve::hir;
use science_codegen::mono::{MonoBody, MonoSet};
use science_types::items::Declarations;
use science_types::{Aliases, AtomOrder, Types, check_crate, thir};

/// One program, all the way to MIR.
pub struct Lowered {
    pub krate: hir::Crate,
    pub types: Types,
    pub decls: Declarations,
    pub bodies: Vec<Body>,
    /// Decision 42's walk, run here for the reason `sciencec`'s driver runs it
    /// there: it needs `&mut Types`, and `try_build` takes `&self`.
    ///
    /// **The harness runs the real walk rather than handing over an empty set**,
    /// so every execution test in this crate exercises the same emission path a
    /// `sciencec build` does. A harness that short-circuited it would leave the
    /// one thing the swap changed untested by the only tests that run programs.
    pub mono: MonoSet,
    /// The same instances, each with its body substituted. The driver builds
    /// these in the same borrow of `Types` the walk holds, and so does this.
    pub instances: Vec<MonoBody>,
}

/// Lex, parse, resolve, check and lower one source file.
///
/// Every phase's diagnostics are asserted empty, with the codes printed: a
/// fixture that stops before the backend is a test that is not testing the
/// backend, and the code is what says which phase refused it.
pub fn lower(source: &str) -> Lowered {
    let file = FileId(0);
    let (tokens, lexed) = science_lexer::lex(file, source);
    assert!(!lexed.has_errors(), "the fixture must lex: {:?}", codes(&lexed));
    let (ast, parsed) = science_parser::parse_module(&tokens, file);
    assert!(!parsed.has_errors(), "the fixture must parse: {:?}", codes(&parsed));
    let (krate, resolution) = science_resolve::resolve_module(file, "fixture.science", &ast);
    assert!(!resolution.has_errors(), "the fixture must resolve: {:?}", codes(&resolution));

    let order = AtomOrder::of(&krate.defs);
    let mut types = Types::new();
    let mut diagnostics = Diagnostics::new();
    let mut aliases = Aliases::of(&krate, &mut types, &order, &mut diagnostics);
    let decls = Declarations::of(&krate, &mut types, &order, &mut diagnostics);
    let thir: Vec<thir::Body> =
        check_crate(&krate, &decls, &mut types, &mut aliases, &order, &mut diagnostics);
    assert!(!diagnostics.has_errors(), "the fixture must check: {:?}", codes(&diagnostics));
    let bodies = {
        let mut context = science_mir::Context {
            defs: &krate.defs,
            decls: &decls,
            types: &mut types,
            aliases: &mut aliases,
        };
        science_mir::lower_crate(&mut context, &thir)
    };
    let (mono, instances) = {
        let mut walk = science_codegen::mono::Mono::new(&krate.defs, &decls, &mut types, &bodies);
        let mut set = walk.collect(science_codegen::mono::RootSet::EntryPoint);
        let instances = walk.instantiate(&mut set);
        (set, instances)
    };
    Lowered { krate, types, decls, bodies, mono, instances }
}

fn codes(diagnostics: &Diagnostics) -> Vec<(u16, String)> {
    diagnostics.iter().map(|d| (d.code.0, d.message.clone())).collect()
}

impl Lowered {
    /// Build this program into `output`, or return the diagnostics that
    /// stopped it.
    ///
    /// The `externs` list is collected here rather than left to the caller for
    /// the reason the module note gives: a test that passed an empty one would
    /// be building a program whose `extern` blocks had silently vanished, and
    /// the only symptom is a refusal that names the foreign function as
    /// unknown.
    pub fn try_build(
        &self,
        output: &std::path::Path,
        opt: OptLevel,
    ) -> Result<Built, Vec<science_diagnostics::Diagnostic>> {
        let mut request = BuildRequest::new(vec!["fixture.science".to_string()]);
        request.opt = opt;
        let externs = science_codegen_llvm::extern_blocks(&self.krate);
        let input = BuildInput {
            request: &request,
            defs: &self.krate.defs,
            types: &self.types,
            decls: &self.decls,
            externs: &externs,
            bodies: &self.bodies,
            mono: &self.mono,
            instances: &self.instances,
            output: output.to_path_buf(),
        };
        build(&input).map_err(|diagnostics| diagnostics.into_vec())
    }

    /// Build, and panic with every diagnostic if it does not.
    pub fn build_at(&self, output: &std::path::Path, opt: OptLevel) -> Built {
        match self.try_build(output, opt) {
            Ok(built) => built,
            Err(diagnostics) => panic!(
                "the build failed:\n{}",
                diagnostics
                    .iter()
                    .map(|d| format!("{}: {}\n  {}", d.code, d.message, d.notes.join("\n  ")))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
        }
    }
}

/// What running a built program produced.
pub struct Ran {
    pub stdout: String,
    pub stderr: String,
    pub status: Option<i32>,
}

/// How long [`run`] waits for the program before concluding it will never
/// exit, killing it, and reporting that rather than blocking forever.
///
/// Ten seconds against programs this crate's own tests build and run on
/// purpose to finish near-instantly. Measured, not guessed:
/// `a_boxed_value_owning_a_string_is_built_and_freed_ten_thousand_times` in
/// `methods.rs` — the heaviest execution test in this crate, a real loop run
/// ten thousand times — builds, links and runs in under half a second end to
/// end. Twenty times that leaves room for a slow or loaded machine without
/// leaving room for a program that is actually stuck, which is what a false
/// positive here would have to mean.
const RUN_BUDGET: std::time::Duration = std::time::Duration::from_secs(10);

/// Run a built program and collect all three, but do not wait past
/// [`RUN_BUDGET`] for it.
///
/// `Command::output` blocks on `waitpid` with no way to give up, and that is
/// exactly the shape of the defect `7674487` shipped: `for i in 0..5:` with a
/// `continue` in it hung forever, in the language's most-written construct,
/// and the whole suite passed, because a hang has no exit code to compare and
/// no diagnostic to match — nothing before that fix ever asked the one
/// question that would have caught it. A test that reaches a genuinely
/// nonterminating program now gets a `status: None` and a `stderr` that says
/// why, instead of a `cargo test` run that has to be killed by hand.
///
/// `\r\n` is normalised to `\n` because `science_print` writes `\n` on every
/// platform — that is its own decision, *"Science text is UTF-8 and its
/// terminator is `\n`"* — but a process spawned on Windows can still have its
/// stream translated, and a test that failed on the line ending would be
/// failing about the wrong thing.
pub fn run(built: &Built) -> Ran {
    assert!(built.executable.is_file(), "no executable at {}", built.executable.display());
    use std::io::Read;
    use std::process::Stdio;

    let mut child = std::process::Command::new(&built.executable)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the program spawns");

    // Read stdout and stderr on their own threads so a program that fills one
    // pipe's buffer while nobody is waiting on the other cannot deadlock this
    // harness against its own subject.
    let mut stdout_pipe = child.stdout.take().expect("stdout was piped");
    let mut stderr_pipe = child.stderr.take().expect("stderr was piped");
    let stdout_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut buf);
        buf
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut buf);
        buf
    });

    let deadline = std::time::Instant::now() + RUN_BUDGET;
    let status = loop {
        match child.try_wait().expect("polling the child") {
            Some(status) => break Some(status),
            None if std::time::Instant::now() >= deadline => break None,
            None => std::thread::sleep(std::time::Duration::from_millis(20)),
        }
    };

    let timed_out = status.is_none();
    if timed_out {
        // The pipe readers above are still blocked in `read_to_end` until the
        // child's ends close, which a kill (rather than a plain drop) does.
        let _ = child.kill();
        let _ = child.wait();
    }

    let stdout = stdout_reader.join().expect("the stdout reader thread");
    let mut stderr = stderr_reader.join().expect("the stderr reader thread");
    if timed_out {
        stderr.extend_from_slice(
            format!(
                "\n[harness] killed after not exiting within {:?} — see harness::RUN_BUDGET",
                RUN_BUDGET
            )
            .as_bytes(),
        );
    }

    Ran {
        stdout: String::from_utf8_lossy(&stdout).replace("\r\n", "\n"),
        stderr: String::from_utf8_lossy(&stderr).replace("\r\n", "\n"),
        status: status.and_then(|s| s.code()),
    }
}

/// A directory of this test's own, named after the process so two `cargo test`
/// runs cannot collide.
pub fn scratch(prefix: &str, name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("science-{prefix}-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

/// The executable's path inside a scratch directory.
pub fn executable(dir: &std::path::Path, stem: &str) -> std::path::PathBuf {
    dir.join(if cfg!(windows) { format!("{stem}.exe") } else { stem.to_string() })
}

/// Fail with the instruction rather than with a linker error.
///
/// Cargo builds a dependency's `rlib` and not its `staticlib`, so
/// `target/<profile>/science_rt.lib` is produced by building `science-rt`
/// directly and not by building this crate.
pub fn require_runtime() {
    if science_codegen_llvm::link::find_runtime().is_err() {
        panic!(
            "`science_rt.lib` was not found. Cargo builds a dependency's rlib and not its \
             staticlib, so run `cargo build -p science-rt` — or `cargo test --workspace \
             --features llvm`, which builds every member."
        );
    }
}
