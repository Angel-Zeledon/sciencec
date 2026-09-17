//! §10's stage 1 and its gate: `print("hello, world")` becomes a program that
//! prints `hello, world` and exits 0.
//!
//! **The decision.** This test runs the whole pipeline — lex, parse, resolve,
//! check, MIR, lower, LLVM, object, link — and then **runs the executable** and
//! asserts its stdout and its exit code.
//!
//! **The reason** is §10's own discipline, which is the reason the staging
//! exists at all: *"every stage below produces a program that runs and prints
//! something, and no stage is finished until an execution test asserts its
//! output and exit code."* Every check short of running the program passes on a
//! backend that emits a correct-looking module and a wrong one. Two of the three
//! defects this crate had were exactly that shape: `_S4main` wrote its `null`
//! into a private `alloca` instead of the caller's `sret` slot, which verifies,
//! links, and takes the error branch on whatever the stack held; and
//! `Reloc::Static` on Windows x64 emitted an `ADDR32` relocation the linker
//! rejects. Neither is visible in the IR.
//!
//! **The cost, and it is what makes this the only test in the crate with
//! prerequisites.** It needs LLVM, a `clang` to drive the link, and
//! `target/<profile>/science_rt.lib` — which `cargo build -p science-rt`
//! produces and building `sciencec` does not, because Cargo builds a
//! dependency's `rlib` and not its `staticlib`. A `cargo test --workspace
//! --features llvm` builds every member, `science-rt` among them, so the archive
//! is there; a `cargo test -p science-codegen-llvm --features llvm` on a cold
//! tree may not have it, and the failure says so rather than reporting a linker
//! error.

#![cfg(feature = "llvm")]

use science_codegen::driver::BuildRequest;
use science_codegen_llvm::{BuildInput, build};
use science_diagnostics::{Diagnostics, FileId};
use science_mir::mir::Body;
use science_resolve::hir;
use science_types::items::Declarations;
use science_types::{Aliases, AtomOrder, Types, check_crate, thir};

struct Lowered {
    krate: hir::Crate,
    types: Types,
    bodies: Vec<Body>,
}

/// One program, all the way to MIR. The same harness
/// `science-codegen/tests/mono.rs` uses, cut to what a build needs.
fn lower(source: &str) -> Lowered {
    let file = FileId(0);
    let (tokens, lexed) = science_lexer::lex(file, source);
    assert!(!lexed.has_errors(), "the fixture must lex");
    let (ast, parsed) = science_parser::parse_module(&tokens, file);
    assert!(!parsed.has_errors(), "the fixture must parse");
    let (krate, resolution) = science_resolve::resolve_module(file, "hello.science", &ast);
    assert!(!resolution.has_errors(), "the fixture must resolve");

    let order = AtomOrder::of(&krate.defs);
    let mut types = Types::new();
    let mut diagnostics = Diagnostics::new();
    let mut aliases = Aliases::of(&krate, &mut types, &order, &mut diagnostics);
    let decls = Declarations::of(&krate, &mut types, &order, &mut diagnostics);
    let thir: Vec<thir::Body> =
        check_crate(&krate, &decls, &mut types, &mut aliases, &order, &mut diagnostics);
    assert!(
        !diagnostics.has_errors(),
        "the fixture must check: {:?}",
        diagnostics.iter().map(|d| d.code.0).collect::<Vec<_>>()
    );
    let bodies = {
        let mut context = science_mir::Context {
            defs: &krate.defs,
            decls: &decls,
            types: &mut types,
            aliases: &mut aliases,
        };
        science_mir::lower_crate(&mut context, &thir)
    };
    Lowered { krate, types, bodies }
}

/// A directory of this test's own, named after the process so two `cargo test`
/// runs cannot collide.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("science-hello-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

fn runtime_is_built() -> bool {
    science_codegen_llvm::link::find_runtime().is_ok()
}

#[test]
fn hello_world_runs_and_exits_zero() {
    let lowered = lower("print(\"hello, world\")\n");
    let dir = scratch("run");
    let output = dir.join(if cfg!(windows) { "hello.exe" } else { "hello" });
    let request = BuildRequest::new(vec!["hello.science".to_string()]);
    let input = BuildInput {
        request: &request,
        defs: &lowered.krate.defs,
        types: &lowered.types,
        bodies: &lowered.bodies,
        output: output.clone(),
    };

    if !runtime_is_built() {
        panic!(
            "`science_rt.lib` was not found. Cargo builds a dependency's rlib and not its \
             staticlib, so run `cargo build -p science-rt` — or `cargo test --workspace \
             --features llvm`, which builds every member."
        );
    }

    let built = match build(&input) {
        Ok(built) => built,
        Err(diagnostics) => panic!(
            "the build failed:\n{}",
            diagnostics
                .iter()
                .map(|d| format!("{}: {}\n  {}", d.code, d.message, d.notes.join("\n  ")))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    };
    assert!(built.executable.is_file(), "no executable at {}", built.executable.display());

    let run = std::process::Command::new(&built.executable).output().expect("the program runs");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n"),
        "hello, world\n",
        "stderr was: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        run.status.code(),
        Some(0),
        "`script-mode.md` §2.3: a script body that returns no error exits 0. stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The IR, pinned at the four shapes that were wrong and are the ones a future
/// change would get wrong again.
///
/// **Not a golden file.** A whole-module snapshot of LLVM IR breaks on every
/// LLVM release for reasons that are not this crate's, and the version pin is
/// `codegen-and-linking.md` Decision 2's job rather than a test's. What is
/// pinned is four sentences, each of which was false at some point this week.
#[test]
fn the_module_says_what_stage_one_says_it_should() {
    let lowered = lower("print(\"hello, world\")\n");
    let dir = scratch("ir");
    let output = dir.join(if cfg!(windows) { "hello.exe" } else { "hello" });
    // **`-O0`, and the reason is a fact about `Built::ir` worth knowing.** That
    // field holds the module *after* Decision 33's pipeline has run: `build`
    // prints it once, after `emit_object_to`, because `--emit=llvm-ir` wants the
    // optimised form. At `-O2` the inliner folds `_S4main` into `main` and the
    // null test disappears entirely — correctly, since the value is a constant
    // `null` — so a test that pinned the emitter's output at the default level
    // would be pinning the optimiser's instead. `-O0` is the emitter's own text.
    let mut request = BuildRequest::new(vec!["hello.science".to_string()]);
    request.opt = science_codegen::target::OptLevel::O0;
    let input = BuildInput {
        request: &request,
        defs: &lowered.krate.defs,
        types: &lowered.types,
        bodies: &lowered.bodies,
        output,
    };
    if !runtime_is_built() {
        panic!("run `cargo build -p science-rt` first; see the other test in this file");
    }
    let ir = build(&input).map_err(|_| "the build failed").expect("a build").ir;

    // Decision 15: the bytes are a `private unnamed_addr constant`, not
    // NUL-terminated, and the length travels beside the pointer.
    assert!(
        ir.contains("private unnamed_addr constant [12 x i8] c\"hello, world\""),
        "the literal is not Decision 15's global:\n{ir}"
    );
    // §9.2's finding, on the symbol hello world calls first: the construction is
    // an `sret` call and not a structural return.
    assert!(
        ir.contains("@science_string_from_bytes(ptr") && ir.contains("sret({ ptr, i64, i64 })"),
        "`science_string_from_bytes` is not declared with an `sret` slot:\n{ir}"
    );
    // The drop is not optional: the temporary `String` is owned by the call site
    // and nothing else frees it.
    assert!(ir.contains("@science_string_free("), "the temporary is leaked:\n{ir}");
    // §3.4: the data pointer and **only** the data pointer. A `load { ptr, ptr }`
    // here is the bug `ExtInst::LoadNiche` exists to have stopped.
    assert!(
        ir.contains("icmp eq ptr"),
        "the null test is not against a bare pointer:\n{ir}"
    );
    assert!(
        !ir.contains("load { ptr, ptr }"),
        "the vtable word of a possibly-null `(any Error)?` was loaded, which §3.4 forbids \
         even on the path that tests for null:\n{ir}"
    );
    // Decision 6, on every function without exception.
    assert!(ir.contains("nounwind"), "Decision 6's attribute is missing:\n{ir}");
    assert!(
        !ir.contains("invoke ") && !ir.contains("landingpad"),
        "Decision 6: there is no unwinding anywhere in this language:\n{ir}"
    );
    // Decision 35's own check, restated as a test rather than trusted to the
    // compiler's self-check.
    assert_eq!(science_codegen::target::first_fast_math_flag(&ir), None, "{ir}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// §10's boundary is sharp, and the refusal names the construct.
///
/// A program one step past stage 1 is `SC0400` and not an internal error. This
/// is the half of `lower`'s contract that a user meets first.
#[test]
fn a_program_past_stage_one_is_refused_by_name() {
    let lowered = lower("def helper() -> Int:\n    return 1\n\nprint(\"hi\")\n");
    let dir = scratch("refused");
    let request = BuildRequest::new(vec!["hello.science".to_string()]);
    let input = BuildInput {
        request: &request,
        defs: &lowered.krate.defs,
        types: &lowered.types,
        bodies: &lowered.bodies,
        output: dir.join("out"),
    };
    let diagnostics = build(&input).map(|_| ()).expect_err("a second function is not stage 1");
    let first = diagnostics.iter().next().expect("a diagnostic");
    assert_eq!(first.code, science_codegen::diagnostics::code::SC0400);
    assert!(
        first.message.contains("helper"),
        "the refusal must name the construct, and it said: {}",
        first.message
    );
    let _ = std::fs::remove_dir_all(&dir);
}
