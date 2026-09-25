//! `ffi-c-boundary.md` §1.3's two view types, from a Science `Array` to a real
//! C function and back.
//!
//! > *`ffi.Span of T` is `{ borrowed T, Int }` — a pointer with a length …
//! > **At the ABI only the pointer crosses.** The length exists solely on the
//! > Science side, and it exists for one reason: so that the safe wrapper's
//! > precondition — that `lda * n` does not exceed the buffer — is an
//! > expression the compiler type-checks rather than a comment.*
//!
//! # Why the assertions are numbers and not IR
//!
//! Every mistake available in this area **links**. A span passed by its own
//! address rather than by the pointer inside it is a `ptr` where a `ptr`
//! belongs; a span passed whole is two words where the callee reads one and
//! then reads the length as the next argument; a `len` crossing when it should
//! not shifts every argument after it by one. None of the three is a type
//! error anywhere, all three produce an executable, and each produces a
//! different wrong number. So the tests that matter here run the program and
//! compare the number, and the two that assert IR say why they cannot.
//!
//! # The two fixtures, and why neither is a mock
//!
//! [`archive`] compiles six lines of C and packs them with `ar`, which is
//! `library_clauses.rs`'s fixture and its argument: a test that needed a
//! library the machine might not have would be a test that is skipped exactly
//! where it matters. `science_span_sum` reads through the pointer it is given
//! and adds up `n` doubles; `science_span_scale` writes through one. If the
//! pointer is wrong the sum is not 10.0, and on most wrong pointers the
//! program does not survive to print anything.
//!
//! The **gate** is separate and is `cblas_ddot` against the platform's own
//! BLAS — `codegen-and-linking.md` §10 stage 5 names that program by name. It
//! is `#[cfg(target_os = "macos")]` because macOS ships BLAS in Accelerate and
//! `-lblas` resolves with nothing installed, where on Linux it is a package
//! that may not be there. The portable half above covers the same lowering;
//! what the gate adds is a library nobody here wrote.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;
use science_codegen_llvm::link;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// `$SCIENCE_LIBRARY_PATH` is process-wide and `cargo test` runs this file's
/// tests in one process. `library_clauses.rs` has the same guard for the same
/// reason; the two files' locks are separate, which is safe because only this
/// one and that one set the variable and neither runs while the other holds it
/// under `--test-threads=1`.
static ENVIRONMENT: Mutex<()> = Mutex::new(());

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

/// A static archive holding one reader and one writer of a `double*`.
///
/// **`n` is a parameter of the C function and not something the ABI supplied**,
/// which is the whole of §1.3's *"the length does not cross"* stated as a
/// fixture: the Science declaration passes it as an ordinary `I64`, exactly as
/// `cblas_ddot`'s `n` is an ordinary `int`, and the span contributes one word.
#[cfg(unix)]
fn archive(dir: &Path) -> PathBuf {
    let driver = link::find_driver().expect("a linker driver, which is also the C compiler");
    let source = dir.join("span.c");
    std::fs::write(
        &source,
        "double science_span_sum(const double *xs, long n) {\n\
         \x20   double total = 0.0;\n\
         \x20   for (long i = 0; i < n; i++) { total += xs[i]; }\n\
         \x20   return total;\n\
         }\n\
         void science_span_scale(double *xs, long n, double by) {\n\
         \x20   for (long i = 0; i < n; i++) { xs[i] *= by; }\n\
         }\n",
    )
    .expect("the fixture's C source");
    let object = dir.join("span.o");
    let compiled = std::process::Command::new(&driver.program)
        .arg("-c")
        .arg(&source)
        .arg("-o")
        .arg(&object)
        .output()
        .expect("the C compiler runs");
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let archive = dir.join("libsciencespan.a");
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

/// The `extern` block the fixture archive is declared through.
#[cfg(unix)]
const SPAN_BLOCK: &str = "unsafe extern \"C\" library \"sciencespan\" kind static:\n    \
     def science_span_sum(xs: ffi.Span[F64], n: I64) -> F64\n    \
     def science_span_scale(xs: ffi.MutableSpan[F64], n: I64, by: F64)\n\n";

/// Build and run one program against the fixture archive.
#[cfg(unix)]
fn ran_against_fixture(name: &str, body: &str) -> harness::Ran {
    let environment = Environment::take();
    let dir = scratch("spans", name);
    require_runtime();
    let _archive = archive(&dir);
    environment.set("SCIENCE_LIBRARY_PATH", &dir);
    let source = format!("{SPAN_BLOCK}{body}");
    let built = lower(&source).build_at(&executable(&dir, name), OptLevel::O0);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    ran
}

/// The IR of one program built against the fixture archive, at `-O0`.
#[cfg(unix)]
fn ir_against_fixture(name: &str, body: &str, no_noalias: bool) -> String {
    let environment = Environment::take();
    let dir = scratch("spans", name);
    require_runtime();
    let _archive = archive(&dir);
    environment.set("SCIENCE_LIBRARY_PATH", &dir);
    let source = format!("{SPAN_BLOCK}{body}");
    let built = lower(&source)
        .try_build_with(&executable(&dir, name), OptLevel::O0, no_noalias)
        .unwrap_or_else(|diagnostics| {
            panic!(
                "the build failed:\n{}",
                diagnostics
                    .iter()
                    .map(|d| format!("{}: {}", d.code, d.message))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        });
    let text = built.ir.clone();
    let _ = std::fs::remove_dir_all(&dir);
    text
}

/// The one `declare` line naming `needle`.
fn declaration(ir: &str, needle: &str) -> String {
    ir.lines()
        .find(|line| line.starts_with("declare") && line.contains(needle))
        .unwrap_or_else(|| panic!("no `declare` of `{needle}` in:\n{ir}"))
        .to_string()
}

// --- the conversion, against a C library that reads the pointer -------------

/// `&Array[F64]` reaches a C function declared `ffi.Span[F64]`, and the
/// number the C function computes is right.
///
/// **10.0 is the assertion and 4 is the other half of it.** The C side adds
/// `n` doubles starting at the pointer it was handed. If the pointer were the
/// span's own address — the mistake `ArgClass::SpanPointer` exists to tell
/// apart from `IndirectByPointer`, and the one that links cleanly — the first
/// "double" read would be the bit pattern of a pointer, which is about 6e-310,
/// and the second would be the length as a double, about 2e-323. The sum would
/// not be 10.
#[cfg(unix)]
#[test]
fn a_borrowed_array_coerces_to_a_span_and_c_reads_the_elements() {
    let ran = ran_against_fixture(
        "sum",
        "let xs be [1.0, 2.0, 3.0, 4.0]\n\
         let total be unsafe: science_span_sum(&xs, 4)\n\
         print(f\"{total}\")\n",
    );
    assert_eq!(ran.stdout, "10.0\n", "stderr: {}", ran.stderr);
    assert_eq!(ran.status, Some(0));
}

/// The mutable direction: C writes through the pointer and Science sees it.
///
/// **This is the direction where a wrong pointer is silent rather than
/// wrong-looking.** A read through a bad pointer produces a number nobody
/// believes; a *write* through one corrupts whatever is there and the array
/// reads back unchanged, which is exactly what a `ffi.MutableSpan` passed by
/// its own address would do — it would scale the span's two words and leave
/// the elements alone. So the assertion is on the array after the call.
#[cfg(unix)]
#[test]
fn an_exclusive_span_lets_c_write_through_to_the_array() {
    let ran = ran_against_fixture(
        "scale",
        "let mutable xs be [1.0, 2.0, 3.0]\n\
         unsafe: science_span_scale(&mut xs, 3, 10.0)\n\
         print(f\"{xs[0]} {xs[1]} {xs[2]}\")\n",
    );
    assert_eq!(ran.stdout, "10.0 20.0 30.0\n", "stderr: {}", ran.stderr);
    assert_eq!(ran.status, Some(0));
}

/// `.span()` and `.span_mutably()` name the conversion where it has to be
/// written out.
///
/// `SC0421`'s own note is the specification: *"a call site still passes the
/// array — `&Array[T]` coerces to `ffi.Span[T]` there, and `.span()` names the
/// conversion **where it has to be written out**"*. A `let` is one of those
/// places: `assign.rs`'s rule 8 is gated on `Site::Argument`, so the binding
/// below has no coercion available to it and the method is the only spelling.
#[cfg(unix)]
#[test]
fn span_and_span_mutably_name_the_conversion_a_let_binding_needs() {
    let ran = ran_against_fixture(
        "method",
        "let xs be [1.0, 2.0, 3.0, 4.0]\n\
         let view be xs.span()\n\
         let total be unsafe: science_span_sum(view, 4)\n\
         print(f\"{total}\")\n\
         let mutable ys be [1.0, 2.0, 3.0]\n\
         let writable be ys.span_mutably()\n\
         unsafe: science_span_scale(writable, 3, 10.0)\n\
         print(f\"{ys[0]} {ys[1]} {ys[2]}\")\n",
    );
    assert_eq!(ran.stdout, "10.0\n10.0 20.0 30.0\n", "stderr: {}", ran.stderr);
    assert_eq!(ran.status, Some(0));
}

// --- what crosses, and with what attached ----------------------------------

/// Decision 24 and §2.2 at the C boundary: the exclusive span is `noalias` and
/// the shared one is `readonly` and **not** `noalias`.
///
/// > *"`mutable borrowed` and `ffi.MutableSpan` lower with LLVM's `noalias`.
/// > Rule 4 of §6.1 guarantees exclusivity on the Science side, so the
/// > attribute is justified by the language."*
///
/// **The negative half is the one that matters**, for `borrow_attributes.rs`'
/// reason restated one type over: two shared spans of one array are legal —
/// rule 4 is *shared many, or exclusive one* — so `noalias` on an `ffi.Span`
/// would be a lie the optimiser is entitled to act on, and §4.4 calls that the
/// single place where a region bug produces a wrong answer rather than a
/// missed error. A test asserting only the positive case would pass with the
/// attribute on both.
///
/// **IR and not a run, for `borrow_attributes.rs`' reason**: `noalias` has no
/// observable behaviour. It is a promise to the optimiser and there is nothing
/// to print.
///
/// **`nocapture` is absent here as it is there**, and the reason is stronger
/// rather than merely the same: `param_attrs` withholds it from a Science
/// function because this compiler has no escape analysis, and §2.2 says of a C
/// function that whether it *"does not retain the pointer past the call"* has
/// *"no way to state it and no way to check it"*. There is not even a body.
#[cfg(unix)]
#[test]
fn an_exclusive_span_is_noalias_and_a_shared_span_is_readonly_and_not() {
    let ir = ir_against_fixture(
        "attributes",
        "let mutable xs be [1.0, 2.0]\n\
         let total be unsafe: science_span_sum(&xs, 2)\n\
         unsafe: science_span_scale(&mut xs, 2, 2.0)\n\
         print(f\"{total}\")\n",
        false,
    );

    let shared = declaration(&ir, "science_span_sum");
    assert!(shared.contains("ptr readonly align 8"), "{shared}");
    assert!(
        !shared.contains("noalias"),
        "§2.2 gives `noalias` to `ffi.MutableSpan` and withholds it from `ffi.Span`, because \
         two shared spans of one array are legal: {shared}"
    );

    let exclusive = declaration(&ir, "science_span_scale");
    assert!(exclusive.contains("ptr noalias align 8"), "{exclusive}");
    assert!(
        !exclusive.contains("readonly"),
        "an exclusive span is written through, which is the whole of what it is for: \
         {exclusive}"
    );
}

/// §4.4's `--no-noalias` reaches a span's attribute too.
///
/// The flag exists so that *"is this a region bug or a codegen bug"* is one
/// recompile, and a span whose attribute the flag did not reach would be the
/// one shape the mitigation does not cover — which is the shape §4.4 is most
/// worried about, since a span is a whole buffer where a borrow is one value.
#[cfg(unix)]
#[test]
fn no_noalias_suppresses_an_exclusive_spans_attribute_and_leaves_the_alignment() {
    let body = "let mutable xs be [1.0, 2.0]\n\
                unsafe: science_span_scale(&mut xs, 2, 2.0)\n\
                print(f\"{xs[0]}\")\n";
    let with = declaration(&ir_against_fixture("noalias-on", body, false), "science_span_scale");
    let without =
        declaration(&ir_against_fixture("noalias-off", body, true), "science_span_scale");
    assert!(with.contains("ptr noalias align 8"), "{with}");
    assert!(!without.contains("noalias"), "{without}");
    assert!(
        without.contains("ptr align 8"),
        "the flag suppresses one attribute and not the parameter: {without}"
    );
}

/// The length does not cross the ABI.
///
/// §1.3: *"At the ABI only the pointer crosses. The length exists solely on
/// the Science side."* The declaration is written with **two** parameters where
/// the span is one of them, and the emitted `declare` has to show exactly two
/// words for them — `ptr` for the span and `i64` for the count the author
/// wrote. A span that crossed whole would put three there, and every argument
/// after it would be off by one.
#[cfg(unix)]
#[test]
fn only_the_pointer_of_a_span_crosses_and_the_length_stays_in_science() {
    let ir = ir_against_fixture(
        "abi",
        "let xs be [1.0, 2.0]\n\
         let total be unsafe: science_span_sum(&xs, 2)\n\
         print(f\"{total}\")\n",
        false,
    );
    let shared = declaration(&ir, "science_span_sum");
    let parameters = shared
        .split_once('(')
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(inside, _)| inside.split(',').count())
        .expect("a `declare` has a parameter list");
    assert_eq!(
        parameters, 2,
        "`science_span_sum(xs: ffi.Span[F64], n: I64)` is a pointer and a count, and the \
         span's own length is not a third argument: {shared}"
    );
    assert!(shared.starts_with("declare double @science_span_sum(ptr "), "{shared}");
    assert!(shared.contains(", i64)"), "{shared}");
}

// --- what the relation refuses ---------------------------------------------

/// The coercion does not weaken and does not strengthen.
///
/// §1.7 refuses `Span` into `MutableSpan` by name — *"the unsound direction"* —
/// and `assign.rs`'s rule 6 refuses the other for §6's reason: mutability is
/// not this relation's question. Both are `SC0525` at the argument, and this
/// asserts the pair rather than one of them, because a rule written with `>=`
/// where it wanted `==` passes the half that is checked.
///
/// **This is the one test in this file that passed before rule 8 existed**,
/// and it is here for that reason rather than in spite of it: the other eight
/// measure a capability that was absent, and this one measures a refusal that
/// was free when nothing was admitted and has to be paid for now that
/// something is.
#[test]
fn a_spans_mutability_is_the_arrays_and_neither_direction_bends() {
    for (array, span) in [("let xs be", "ffi.MutableSpan"), ("let mutable xs be", "ffi.Span")] {
        let borrow = if span == "ffi.Span" { "&mut xs" } else { "&xs" };
        let source = format!(
            "unsafe extern \"C\" library \"c\":\n    \
             def science_not_called(xs: {span}[F64]) -> I32\n\n\
             {array} [1.0]\n\
             let n be unsafe: science_not_called({borrow})\n\
             print(f\"{{n}}\")\n"
        );
        let file = science_diagnostics::FileId(0);
        let (tokens, lexed) = science_lexer::lex(file, &source);
        assert!(!lexed.has_errors());
        let (ast, parsed) = science_parser::parse_module(&tokens, file);
        assert!(!parsed.has_errors());
        let (krate, resolution) = science_resolve::resolve_module(file, "fixture.science", &ast);
        assert!(!resolution.has_errors());
        let order = science_types::AtomOrder::of(&krate.defs);
        let mut types = science_types::Types::new();
        let mut diagnostics = science_diagnostics::Diagnostics::new();
        let mut aliases =
            science_types::Aliases::of(&krate, &mut types, &order, &mut diagnostics);
        let decls = science_types::items::Declarations::of(
            &krate,
            &mut types,
            &order,
            &mut diagnostics,
        );
        science_types::check_crate(
            &krate,
            &decls,
            &mut types,
            &mut aliases,
            &order,
            &mut diagnostics,
        );
        let codes: Vec<u16> = diagnostics.iter().map(|d| d.code.0).collect();
        assert!(
            codes.contains(&525),
            "`{borrow}` into `{span}[F64]` must not be admitted; got {codes:?}"
        );
    }
}

// --- §10 stage 5's own gate -------------------------------------------------

/// **The gate.** `cblas_ddot` against the platform's BLAS, under a bare
/// `library` name.
///
/// `codegen-and-linking.md` §10 stage 5 is *"a `cblas_ddot` against a real
/// BLAS returning the right number"*, and this is that program. Nothing in
/// this repository defines `cblas_ddot`; the number comes out of a library the
/// operating system shipped.
///
/// 1·4 + 2·3 + 3·2 + 4·1 = 20.
///
/// `#[cfg(target_os = "macos")]`: Accelerate provides BLAS and `-lblas`
/// resolves against it with nothing installed. On Linux `libblas` is a package
/// and a test that needed it would be skipped on the machines that matter,
/// which is `library_clauses.rs`' own argument for building its fixture. The
/// lowering under test is the same one the portable tests above cover; what
/// this adds is a library nobody here wrote and a signature nobody here chose.
#[cfg(target_os = "macos")]
#[test]
fn cblas_ddot_against_a_real_blas_returns_the_dot_product() {
    let environment = Environment::take();
    let _ = &environment;
    let dir = scratch("spans", "ddot");
    require_runtime();
    let source = "unsafe extern \"C\" library \"blas\":\n    \
         def cblas_ddot(n: I32, x: ffi.Span[F64], incx: I32, y: ffi.Span[F64], incy: I32) \
         -> F64\n\n\
         let xs be [1.0, 2.0, 3.0, 4.0]\n\
         let ys be [4.0, 3.0, 2.0, 1.0]\n\
         let d be unsafe: cblas_ddot(4, &xs, 1, &ys, 1)\n\
         print(f\"{d}\")\n";
    let built = lower(source).build_at(&executable(&dir, "ddot"), OptLevel::O0);
    assert!(built.link_command.contains("-lblas"), "{}", built.link_command);
    let ran = run(&built);
    assert_eq!(ran.stdout, "20.0\n", "stderr: {}", ran.stderr);
    assert_eq!(ran.status, Some(0));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The same gate through `via pkg-config`, which §10 asks for beside the bare
/// name.
///
/// The stub answers `-lblas` for the module `blas`, and the block names a
/// module — `science-not-a-real-blas` — that resolves to nothing at all. So a
/// build that appended the plain name would put `-lscience-not-a-real-blas` on
/// the line and fail to link, and the program printing 20 says the flag came
/// from `pkg-config` and only from `pkg-config`.
#[cfg(all(unix, target_os = "macos"))]
#[test]
fn the_gate_runs_through_a_pkg_config_that_answers_lblas() {
    use std::os::unix::fs::PermissionsExt;
    let environment = Environment::take();
    let dir = scratch("spans", "ddot-pkg");
    require_runtime();
    let stub = dir.join("stub-pkg-config");
    std::fs::write(
        &stub,
        "#!/bin/sh\nfor a in \"$@\"; do\n  if [ \"$a\" = \"blas\" ]; then\n    \
         echo '-lblas'\n    exit 0\n  fi\ndone\necho \"Package '$*' not found\" >&2\nexit 1\n",
    )
    .expect("the stub is written");
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755))
        .expect("the stub is executable");
    environment.set("SCIENCE_PKG_CONFIG", &stub);

    let source = "unsafe extern \"C\" library \"science-not-a-real-blas\" via pkg-config \
         \"blas\":\n    \
         def cblas_ddot(n: I32, x: ffi.Span[F64], incx: I32, y: ffi.Span[F64], incy: I32) \
         -> F64\n\n\
         let xs be [1.0, 2.0, 3.0, 4.0]\n\
         let ys be [4.0, 3.0, 2.0, 1.0]\n\
         let d be unsafe: cblas_ddot(4, &xs, 1, &ys, 1)\n\
         print(f\"{d}\")\n";
    let built = lower(source).build_at(&executable(&dir, "ddot-pkg"), OptLevel::O0);
    assert!(built.link_command.contains("-lblas"), "{}", built.link_command);
    assert!(
        !built.link_command.contains("science-not-a-real-blas"),
        "`pkg-config`'s answer replaces the plain name: {}",
        built.link_command
    );
    let ran = run(&built);
    assert_eq!(ran.stdout, "20.0\n", "stderr: {}", ran.stderr);
    assert_eq!(ran.status, Some(0));
    let _ = std::fs::remove_dir_all(&dir);
}
