//! §5.1's `via pkg-config` and `kind static`, and §5.2's `when available`,
//! each asserted against **the link command line** — which is the observable
//! that established they did nothing.
//!
//! # What was wrong, and why no existing test saw it
//!
//! All three clauses lexed, parsed, reached `hir::ExternLibrary` and were then
//! dropped. `crate::link::Library` was `String`: the name survived the journey
//! and the clauses did not. Measured on this machine before the repair, with a
//! shell script in `$SCIENCE_LINKER` recording what it was handed — four
//! programs differing **only** in the clause on one `extern` block:
//!
//! ```text
//! library "lzma"                                  → …/libscience_rt.a -o p -llzma
//! library "lzma" via pkg-config "totally-fake-pkg"→ …/libscience_rt.a -o p -llzma
//! library "lzma" kind static                      → …/libscience_rt.a -o p -llzma
//! library "cudnn" when available                  → …/libscience_rt.a -o p -lcudnn
//! ```
//!
//! The first three are byte-identical. The second one names a `pkg-config`
//! module that does not exist, on a machine where `pkg-config` itself is not
//! installed, and it **built, linked and ran with no diagnostic of any kind**.
//! The fourth is worse than useless: `when available` means *do not make this a
//! load-time dependency*, and it produced a hard `-lcudnn` and `ld: library
//! 'cudnn' not found` — the exact failure the clause exists to prevent.
//!
//! **Nothing in the suite could have caught this.** `SC0402` prints the command
//! line when the linker fails, so a failing build showed its flags and a
//! succeeding one showed nothing; the only way to see a successful link line
//! was the shell script, which is not something a portable test can arrange.
//! So the repair includes `Built::link_command`, and every test below asserts
//! the arguments the `library` clauses contributed, exactly, in order.
//!
//! # The fixture is built by the test, on purpose
//!
//! `kind static` needs a static archive that really exists, and `via
//! pkg-config` needs a `pkg-config` that really answers. Neither OpenBLAS nor
//! `pkg-config` is installed on the machine this was written on, and a test
//! that skips itself when a vendor library is missing is a test that is
//! *always* skipped in CI. So [`archive`] compiles four lines of C with the
//! linker driver this backend already found and packs them with `ar`, and
//! [`stub_pkg_config`] writes a three-line shell script. The library under test
//! is then guaranteed present, its contents are known, and the program that
//! calls into it prints a byte that could only have come from the archive.
//!
//! Both fixtures are `#[cfg(unix)]`: `ar` and `#!/bin/sh` have no portable
//! Windows equivalent, and faking one with `lib.exe` and a `.cmd` would be
//! testing the fake. The four clause behaviours that need no fixture — the
//! `when available` refusal, the disagreement between two blocks, the
//! fall-back when `pkg-config` is absent, and the refusal when `kind static`
//! finds no archive — are asserted on every platform.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::layout::Triple;
use science_codegen::target::OptLevel;
use science_codegen_llvm::link::{self, Library, LinkError};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// `$SCIENCE_LIBRARY_PATH` and `$SCIENCE_PKG_CONFIG` are process-wide, and
/// `cargo test` runs this file's tests on threads of one process.
static ENVIRONMENT: Mutex<()> = Mutex::new(());

/// One test's exclusive hold on the environment, which it puts back.
struct Environment {
    _guard: MutexGuard<'static, ()>,
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl Environment {
    fn take() -> Environment {
        let guard = ENVIRONMENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let names = ["SCIENCE_LIBRARY_PATH", "SCIENCE_PKG_CONFIG"];
        let saved = names.iter().map(|name| (*name, std::env::var_os(name))).collect();
        for name in names {
            std::env::remove_var(name);
        }
        Environment { _guard: guard, saved }
    }

    fn set(&self, name: &str, value: impl AsRef<std::ffi::OsStr>) {
        std::env::set_var(name, value);
    }
}

impl Drop for Environment {
    fn drop(&mut self) {
        for (name, value) in &self.saved {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}

/// The target every assertion here is about: the one being built for.
fn host() -> Triple {
    Triple::host().expect("this crate's tests only run on a supported host")
}

/// The arguments the `library` clauses contributed, cut out of the whole
/// command line.
///
/// Everything before `-o <output>` is the object and the runtime, and
/// everything after the clause arguments is [`link::system_libraries`], which
/// is a property of the target rather than of the program. What is left is
/// exactly what this file is about, and asserting it as a list rather than with
/// `contains` is the point: `contains("-llzma")` would also have passed on
/// every one of the four broken builds quoted in the module note.
fn clause_arguments(command: &str) -> Vec<String> {
    let mut arguments: Vec<String> =
        command.split_whitespace().map(str::to_string).collect();
    for _ in 0..link::system_libraries(host()).len() {
        arguments.pop();
    }
    let output = arguments.iter().position(|argument| argument == "-o").expect("`-o` is passed");
    arguments.split_off(output + 2)
}

/// Build a program, assert it links, and give back its link line and what it
/// printed.
fn built(name: &str, source: &str) -> (String, harness::Ran) {
    let dir = scratch("clauses", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O0);
    let command = built.link_command.clone();
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    (command, ran)
}

/// Build a program and give back the diagnostics that stopped it.
fn refused(name: &str, source: &str) -> Vec<science_diagnostics::Diagnostic> {
    let dir = scratch("clauses", name);
    require_runtime();
    let result = lower(source).try_build(&executable(&dir, name), OptLevel::O0);
    let diagnostics = match result {
        Ok(built) => panic!("the build was expected to be refused; it linked with {}", built.link_command),
        Err(diagnostics) => diagnostics,
    };
    let _ = std::fs::remove_dir_all(&dir);
    diagnostics
}

/// An `extern` block declaring `putchar`, which is how a program here shows a
/// byte it computed. The same declaration `stage_two_and_three.rs` uses.
const PUTCHAR: &str = "unsafe extern \"C\" library \"c\":\n    def putchar(c: I32) -> I32\n\n";

// --- the four assertions that need no fixture ------------------------------

/// `when available` no longer emits a hard `-l`, because it no longer links.
///
/// **This is the clause that was actively wrong rather than merely inert.**
/// `library "cudnn" when available` emitted `-lcudnn` and died with `ld:
/// library 'cudnn' not found` on precisely the CPU-only machine §5.2 is
/// written for. The `dlopen` table, `is_available()` and the indirect call are
/// not written — see `link::library_arguments` — so what is asserted here is
/// the honest half: the compiler says which piece is missing, at `SC0400`,
/// and nothing named `cudnn` reaches any linker.
#[test]
fn when_available_is_refused_by_name_and_never_becomes_a_hard_dash_l() {
    let diagnostics = refused(
        "when-available",
        "unsafe extern \"C\" library \"cudnn\" when available:\n    \
         def cudnnGetVersion() -> U64\n\nlet v be unsafe: cudnnGetVersion()\n",
    );
    let rendered = format!("{diagnostics:?}");
    assert!(
        diagnostics.iter().any(|d| d.code == science_codegen::diagnostics::code::SC0400),
        "a construct this backend does not lower is `SC0400`: {rendered}"
    );
    // The old behaviour, in one assertion: `-lcudnn` went to the linker, the
    // linker said `ld: library 'cudnn' not found`, and the build ended in
    // `SC0402` printing that command line. No linker runs now, so there is no
    // `SC0402` to find. (`-lcudnn` itself appears in a *note*, which is the
    // message explaining what it used to do, so the code is what to look at.)
    assert!(
        !diagnostics.iter().any(|d| d.code == science_codegen::diagnostics::code::SC0402),
        "the linker must not be reached at all: {rendered}"
    );
    assert!(
        !rendered.contains("library 'cudnn' not found"),
        "that was the symptom, and it is the linker's sentence and not this compiler's: \
         {rendered}"
    );
    let notes: String =
        diagnostics.iter().flat_map(|d| d.notes.iter()).cloned().collect::<Vec<_>>().join("\n");
    assert!(notes.contains("dlopen"), "the message has to name what is missing: {notes}");
    assert!(
        notes.contains("is_available"),
        "and the binding that goes with it: {notes}"
    );
}

/// Declaring an optional block is still fine; only calling into one is
/// refused.
///
/// Decision 28's filter drops a `library` clause no call reached, and that
/// filter is what keeps the refusal above from being a refusal of the
/// *declaration*. `examples/20_extern.science` is a file of nothing but
/// declarations and it must keep checking clean.
#[test]
fn an_optional_block_that_is_never_called_does_not_stop_the_build() {
    let (command, ran) = built(
        "when-available-unused",
        &format!(
            "{PUTCHAR}unsafe extern \"C\" library \"cudnn\" when available:\n    \
             def cudnnGetVersion() -> U64\n\nlet r be unsafe: putchar(65)\n"
        ),
    );
    assert_eq!(clause_arguments(&command), vec!["-lc".to_string()]);
    assert_eq!(ran.stdout, "A", "stderr: {}", ran.stderr);
    assert_eq!(ran.status, Some(0));
}

/// Two blocks, one library, two different clauses: refused rather than
/// resolved by whichever came first.
///
/// One library is linked once, so one of the two clauses has to lose, and a
/// compiler that picks quietly is the same failure mode as a compiler that
/// ignores both. `library_order` deduplicates by the whole clause and not by
/// the name for exactly this reason — a name-keyed dedup would have made the
/// second block's `kind static` disappear with no message at all.
#[test]
fn two_blocks_naming_one_library_with_different_clauses_are_refused() {
    let arguments = link::library_arguments(
        host(),
        &[
            Library::named("blas"),
            Library { name: "blas".to_string(), static_link: true, ..Library::default() },
        ],
    );
    match arguments {
        Err(LinkError::ClausesDisagree { library, .. }) => assert_eq!(library, "blas"),
        other => panic!("expected a refusal naming `blas`, got {other:?}"),
    }
    // The same name with the same clause twice is one library and not a
    // disagreement: two `extern` blocks may split one library's declarations.
    assert_eq!(
        link::library_arguments(host(), &[Library::named("blas"), Library::named("blas")])
            .expect("agreeing clauses are not a conflict"),
        vec!["-lblas".to_string(), "-lblas".to_string()]
    );
}

/// `via pkg-config` falls back to the plain name when there is no
/// `pkg-config`, and that is the *only* thing that makes it fall back.
///
/// `SC0412` is the parser's statement of intent: *"a block discovering its
/// flags with `via pkg-config` still declares the plain name as the fallback,
/// because pkg-config is not present on every platform."* Not present. A
/// `$SCIENCE_PKG_CONFIG` naming a program that does not exist is that case
/// exactly, and it must not be an error — there is no `pkg-config` on the
/// machine this was written on and a hard failure would make the clause
/// unusable on it.
#[test]
fn an_absent_pkg_config_falls_back_to_the_plain_library_name() {
    let environment = Environment::take();
    environment.set("SCIENCE_PKG_CONFIG", "science-no-such-pkg-config-anywhere");
    let arguments = link::library_arguments(
        host(),
        &[Library {
            name: "blas".to_string(),
            pkg_config: Some("openblas".to_string()),
            ..Library::default()
        }],
    )
    .expect("an absent `pkg-config` is the fallback case and not a failure");
    assert_eq!(arguments, vec!["-lblas".to_string()]);
}

/// `kind static` with no archive anywhere is refused, and does not quietly
/// become a dynamic link.
///
/// This is the whole bug in one assertion. Before the repair `kind static`
/// produced `-lNAME`, which on a machine holding only `libNAME.dylib` links
/// dynamically — the opposite of what was asked — and says nothing. The
/// refusal names every directory it looked in, because "not found" without the
/// search path is a message that cannot be acted on.
#[test]
fn kind_static_with_no_archive_says_so_instead_of_linking_the_shared_object() {
    let environment = Environment::take();
    environment.set("SCIENCE_LIBRARY_PATH", "/science-no-such-directory");
    let arguments = link::library_arguments(
        host(),
        &[Library {
            name: "science-no-such-library".to_string(),
            static_link: true,
            ..Library::default()
        }],
    );
    match arguments {
        Err(LinkError::NoStaticArchive { library, file, searched }) => {
            assert_eq!(library, "science-no-such-library");
            assert!(file.contains("science-no-such-library"), "{file}");
            assert!(
                searched.iter().any(|path| path.contains("science-no-such-directory")),
                "`$SCIENCE_LIBRARY_PATH` is searched first and is named in the message: \
                 {searched:?}"
            );
            assert!(searched.len() > 1, "the platform defaults are searched too: {searched:?}");
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}

// --- the fixture, and the two clauses that need one ------------------------

/// Four lines of C, compiled and packed into `libscienceclause.a`.
///
/// Built rather than found: see the module note. The answer is `65` so that a
/// program that links the archive prints `A` and one that did not prints
/// nothing at all.
#[cfg(unix)]
fn archive(dir: &Path) -> PathBuf {
    let driver = link::find_driver().expect("a linker driver, which is also the C compiler");
    let source = dir.join("answer.c");
    std::fs::write(&source, "int science_clause_answer(void) { return 65; }\n")
        .expect("the fixture's C source");
    let object = dir.join("answer.o");
    let compiled = std::process::Command::new(&driver.program)
        .arg("-c")
        .arg(&source)
        .arg("-o")
        .arg(&object)
        .output()
        .expect("the C compiler runs");
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let archive = dir.join("libscienceclause.a");
    let packed = std::process::Command::new("ar")
        .arg("rcs")
        .arg(&archive)
        .arg(&object)
        .output()
        .expect("`ar` runs");
    assert!(packed.status.success(), "{}", String::from_utf8_lossy(&packed.stderr));
    assert!(archive.is_file());
    archive
}

/// A `pkg-config` that prints `answer` for `--libs` and fails for anything it
/// was not told about.
#[cfg(unix)]
fn stub_pkg_config(dir: &Path, knows: &str, answer: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("stub-pkg-config");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\nfor a in \"$@\"; do\n  if [ \"$a\" = \"{knows}\" ]; then\n    \
             echo '{answer}'\n    exit 0\n  fi\ndone\necho \"Package '$*' not found\" >&2\n\
             exit 1\n"
        ),
    )
    .expect("the stub is written");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .expect("the stub is executable");
    path
}

/// `kind static` links the archive by path, and the program runs.
///
/// The byte is the proof that this is not a test of a string: `A` came out of
/// `libscienceclause.a`, which is the only place `science_clause_answer` is
/// defined, and there is no shared object of that name anywhere for `-l` to
/// have found instead.
#[cfg(unix)]
#[test]
fn kind_static_puts_the_archives_path_on_the_link_line_and_the_program_runs() {
    let environment = Environment::take();
    let dir = scratch("clauses", "static-fixture");
    let archive = archive(&dir);
    environment.set("SCIENCE_LIBRARY_PATH", &dir);

    let (command, ran) = built(
        "static",
        &format!(
            "{PUTCHAR}unsafe extern \"C\" library \"scienceclause\" kind static:\n    \
             def science_clause_answer() -> I32\n\n\
             let a be unsafe: science_clause_answer()\nlet r be unsafe: putchar(a)\n"
        ),
    );
    assert_eq!(
        clause_arguments(&command),
        vec!["-lc".to_string(), archive.display().to_string()],
        "`kind static` is the archive's own path — the one spelling that means *this archive \
         and not the shared object* on every linker — and the plain `library \"c\"` beside it \
         is still `-lc`, in Decision 28's source order"
    );
    assert!(
        !command.contains("-lscienceclause"),
        "the `-l` form is what `kind static` replaces: {command}"
    );
    assert_eq!(ran.stdout, "A", "stderr: {}", ran.stderr);
    assert_eq!(ran.status, Some(0));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A `pkg-config` that answers replaces the plain name entirely, and the
/// program runs on the flags it gave.
///
/// **The block declares `library "science-not-a-real-library"`, and that is
/// the assertion.** §5.1's argument for supporting the clause at all is that
/// the right flags are `-lopenblas` on one machine, `-lblas -llapack` on
/// another and six MKL libraries on a third; if the plain name were appended
/// to `pkg-config`'s answer the one guess the clause exists to remove would be
/// back on the line. Here the plain name resolves to nothing at all, so a
/// build that appended it would fail to link — and the program printing `A`
/// says the flags came from `pkg-config` and only from `pkg-config`.
#[cfg(unix)]
#[test]
fn a_pkg_config_that_answers_replaces_the_plain_name_rather_than_joining_it() {
    let environment = Environment::take();
    let dir = scratch("clauses", "pkg-fixture");
    let archive = archive(&dir);
    let stub = stub_pkg_config(&dir, "scienceclause", &archive.display().to_string());
    environment.set("SCIENCE_PKG_CONFIG", &stub);

    let (command, ran) = built(
        "pkg-config",
        &format!(
            "{PUTCHAR}unsafe extern \"C\" library \"science-not-a-real-library\" \
             via pkg-config \"scienceclause\":\n    \
             def science_clause_answer() -> I32\n\n\
             let a be unsafe: science_clause_answer()\nlet r be unsafe: putchar(a)\n"
        ),
    );
    assert_eq!(
        clause_arguments(&command),
        vec!["-lc".to_string(), archive.display().to_string()],
        "`pkg-config --libs` is the whole of what the block contributes"
    );
    assert!(
        !command.contains("science-not-a-real-library"),
        "the plain name is the fallback for an absent `pkg-config`, not an addition to a \
         present one's answer: {command}"
    );
    assert_eq!(ran.stdout, "A", "stderr: {}", ran.stderr);
    assert_eq!(ran.status, Some(0));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A `pkg-config` that is present and does not know the module is `SC0460`,
/// and the linker is never run.
///
/// **This is the case the task called out and it is a judgement, so here is
/// the argument.** Falling back was the old behaviour — in fact the old
/// behaviour fell back without asking anyone — and it is how `via pkg-config
/// "opneblas"` reaches production: on the developer's machine the plain
/// `library "openblas"` resolves, the build is green, and the typo is found on
/// the cluster where the plain name is wrong and the `.pc` file was the whole
/// point. An absent `pkg-config` genuinely cannot answer. A present one that
/// says *"No package 'x' found"* has answered, and its answer is that the
/// dependency this build declares is not installed.
#[cfg(unix)]
#[test]
fn a_present_pkg_config_that_does_not_know_the_module_is_an_error_not_a_fallback() {
    let environment = Environment::take();
    // Not `"pkg-missing"`: `scratch` deletes the directory it is asked for, so
    // a fixture sharing a name with the build below is removed out from under
    // the test — and a `$SCIENCE_PKG_CONFIG` pointing at a file that no longer
    // exists is the *absent* case, which falls back and passes nothing.
    let dir = scratch("clauses", "pkg-missing-fixture");
    let stub = stub_pkg_config(&dir, "something-else", "-lnothing");
    environment.set("SCIENCE_PKG_CONFIG", &stub);

    let diagnostics = refused(
        "pkg-missing",
        &format!(
            "{PUTCHAR}unsafe extern \"C\" library \"blas\" via pkg-config \"opneblas\":\n    \
             def cblas_ddot() -> F64\n\nlet d be unsafe: cblas_ddot()\nlet r be unsafe: \
             putchar(65)\n"
        ),
    );
    assert!(
        diagnostics.iter().any(|d| d.code == science_codegen::diagnostics::code::SC0460),
        "{diagnostics:?}"
    );
    let rendered = format!("{diagnostics:?}");
    assert!(rendered.contains("opneblas"), "the module the clause named: {rendered}");
    assert!(
        rendered.contains("not found"),
        "`pkg-config`'s own words, verbatim, because the compiler did not decide this: \
         {rendered}"
    );
    assert!(
        !rendered.contains("-lblas"),
        "the plain name is not silently substituted; that is the behaviour this replaces: \
         {rendered}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `kind static` and `via pkg-config` together ask `pkg-config --static`.
///
/// `Libs:` is what a shared link needs and `Libs: + Libs.private:` is what a
/// static one needs; a static link built from the dynamic `--libs` is missing
/// the transitive closure and fails on symbols nobody declared. The stub
/// answers only when it is passed `--static`, so the flag being absent is a
/// refusal rather than a quietly different answer.
#[cfg(unix)]
#[test]
fn a_static_pkg_config_lookup_asks_for_the_private_libraries_too() {
    let environment = Environment::take();
    let dir = scratch("clauses", "pkg-static");
    let stub = stub_pkg_config(&dir, "--static", "-lstatic-answer");
    environment.set("SCIENCE_PKG_CONFIG", &stub);

    let arguments = link::library_arguments(
        host(),
        &[Library {
            name: "blas".to_string(),
            pkg_config: Some("openblas".to_string()),
            static_link: true,
            ..Library::default()
        }],
    )
    .expect("the stub answers when it is asked with `--static`");
    assert_eq!(arguments, vec!["-lstatic-answer".to_string()]);

    // And without `kind static` the same stub refuses, which is what says the
    // flag is passed because of the clause and not always.
    assert!(
        link::library_arguments(
            host(),
            &[Library {
                name: "blas".to_string(),
                pkg_config: Some("openblas".to_string()),
                ..Library::default()
            }],
        )
        .is_err(),
        "`--static` must not be passed for a dynamic link"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
