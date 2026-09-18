//! §10's stage 2 and stage 3: a call into a C library, control flow, and
//! integers — each as a program that is built, **run**, and asked what it
//! printed and what status it exited with.
//!
//! # Why every test here runs the program
//!
//! §10's discipline: *"every stage below produces a program that runs and
//! prints something, and no stage is finished until an execution test asserts
//! its output and exit code."* This crate's §3 is the list of what reading
//! could not have caught, and this file added three entries to it:
//!
//! 1. **A C library's output was discarded.** `putchar(65)` emitted
//!    `call i32 @putchar(i32 65)`, linked, exited 0 and printed **nothing**,
//!    because `science_exit` flushed Rust's buffered stdout and not the C
//!    runtime's. Every assertion short of running the program passed.
//! 2. **`0i8 - 128i8` computed at `i64` and stored eight bytes into a one-byte
//!    slot.** `Operand::ConstInt` carries no type, both operands were
//!    constants, so the width fell back to the default — and `store i64 %v,
//!    ptr %slot` is *valid IR* under opaque pointers, so the verifier passed
//!    it. The symptom was a comparison that answered `false`.
//! 3. **`SC0461` never fired on this machine**, because `link.exe` is
//!    localised and the marker being matched was the English prose.
//!    `crate::link::undefined_symbols` is the account.
//!
//! # The two places this file departs from §10's own programs
//!
//! §10 writes stage 2 as `print(f"{x}")` and stage 3 as a `for i in 0..10:`
//! loop whose total is printed the same way. **Neither is a program this
//! language can parse or lower**, and both departures are recorded at the test
//! that makes them:
//!
//! - **There is no string interpolation.** `f"{x}"` is not in the lexer, the
//!   parser, the AST or the corpus; `print(f"{x}")` is a parse error. There is
//!   also no integer-to-string entry point among `science-rt`'s 47, so even
//!   spelled another way there is no way to print a number. Every program here
//!   that has to show a computed value shows it as a **byte**, through
//!   `putchar` — which is the same C-library call stage 2 is about, so the
//!   observation costs nothing that was not already being tested.
//! - **`for` does not reach this backend.** Its `next` arrives as
//!   `science_mir::mir::Unresolved::IterateNext`, because
//!   `science_types::thir::ExprKind::For` has no field for the callee the
//!   checker's `iterate_item` found. `loop:` with a `break` is stage 3's CFG
//!   in a form the front end delivers, and it exercises the same back edge.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::layout::Triple;
use science_codegen::target::OptLevel;

/// An `extern` block declaring `putchar`, which every program that has to show
/// a number uses.
const PUTCHAR: &str = "unsafe extern \"C\" library \"c\":\n    def putchar(c: I32) -> I32\n\n";

/// Build and run one program, and give back what it printed.
fn output(name: &str, source: &str) -> harness::Ran {
    let dir = scratch("stage23", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    ran
}

/// A program whose answer is one byte, written by the C library.
fn byte(name: &str, source: &str) -> String {
    let ran = output(name, source);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// `Y` when the condition held and `N` when it did not, decided inside the
/// compiled program and written by `putchar`.
fn answers(name: &str, setup: &str, condition: &str) -> String {
    byte(
        name,
        &format!(
            "{PUTCHAR}{setup}if {condition}:\n    let r be unsafe: putchar(89)\nelse:\n    \
             let r be unsafe: putchar(78)\n"
        ),
    )
}

// --- stage 2 --------------------------------------------------------------

/// §10's stage 2 gate: `cos(0.0)` through `library "m"` prints `1` and exits 0.
///
/// > *`unsafe extern "C" library "m": def cos(x: F64) -> F64` … **Gate:** the
/// > program prints `1`, exits 0.*
///
/// **The `f"{x}"` in §10's own program is replaced by a comparison**, for the
/// module note's reason: there is no interpolation in this language and no
/// float formatter in the runtime. What is left is the whole of what the gate
/// is about — the `declare`, the symbol name, the `library` clause becoming a
/// link decision, and the value coming back in the right register — because
/// the program's output depends on `cos(0.0)` having returned exactly `1.0`.
/// A wrong calling convention, a wrong return class or a wrong symbol gives
/// `not 1`.
#[test]
fn stage_two_calls_libm_and_the_answer_decides_what_is_printed() {
    let ran = output(
        "libm",
        "unsafe extern \"C\" library \"m\":\n    def cos(x: F64) -> F64\n\n\
         let x be unsafe: cos(0.0)\nif x is 1.0:\n    print(\"1\")\nelse:\n    print(\"not 1\")\n",
    );
    assert_eq!(ran.stdout, "1\n", "stderr: {}", ran.stderr);
    assert_eq!(ran.status, Some(0));
}

/// A C library writes the program's output, and the program's output survives.
///
/// **This is the test that found finding 11**, and it is worth having
/// separately from the `cos` one because the `cos` program's output goes
/// through `science_print` — Rust's buffered stdout, which
/// `science_exit` already flushed. Here the only writer is the C runtime's
/// `stdout`, which nothing flushed: the program emitted the right call, linked,
/// exited 0 and printed nothing. It is also invisible at a terminal, because a
/// console is line-buffered and a pipe is not, so only a harness that captures
/// output can see it.
#[test]
fn a_byte_written_by_a_c_library_reaches_standard_output() {
    assert_eq!(byte("putchar", &format!("{PUTCHAR}let r be unsafe: putchar(65)\n")), "A");
}

/// The typo half of stage 2's gate: `SC0461`, naming the declaration and the
/// library clause, **and** the linker's own text beside it.
///
/// > *changing `cos` to `cosinus` produces `SC0461` naming the declaration's
/// > span and the library clause, and not `ld`'s output.*
///
/// Both diagnostics are reported, and the "not `ld`'s output" half is read as
/// *not only* `ld`'s output: §5.5 requires the command line and the linker's
/// text verbatim for a failure, and a reader whose library was the wrong one
/// needs the flags that were passed as much as the span.
#[test]
fn a_misspelled_foreign_symbol_is_sc0461_against_its_declaration() {
    let dir = scratch("stage23", "typo");
    require_runtime();
    let diagnostics = lower(
        "unsafe extern \"C\" library \"m\":\n    def cosinus(x: F64) -> F64\n\n\
         let x be unsafe: cosinus(0.0)\nprint(\"unreachable\")\n",
    )
    .try_build(&executable(&dir, "typo"), OptLevel::O2)
    .map(|_| ())
    .expect_err("`cosinus` is in no library");

    let attributed = diagnostics
        .iter()
        .find(|d| d.code == science_codegen::diagnostics::code::SC0461)
        .expect("Decision 29 attributes an undefined symbol to its `extern` declaration");
    assert!(attributed.message.contains("cosinus"), "{}", attributed.message);
    assert!(
        attributed.labels.iter().any(|label| label.primary),
        "the gate asks for the declaration's span"
    );
    let notes = attributed.notes.join("\n");
    assert!(notes.contains("library \"m\""), "the gate asks for the library clause: {notes}");
    // §5.5: the raw text is still handed over, with the command that produced
    // it. `SC0461` says which declaration; `SC0402` says what was run.
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == science_codegen::diagnostics::code::SC0402),
        "the linker's own output is not suppressed by having understood one line of it"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A spelling mistake that is **not** an undefined symbol stays `SC0402`.
///
/// Decision 29's condition is that codegen can attribute the failure. A linker
/// output with no undefined-symbol marker in it must not be mined for
/// substrings that happen to be declared symbols, which is §5.5's rule: *"a
/// diagnostic that pretends to have understood a linker error it did not
/// understand is worse than one that hands over the raw text."*
#[test]
fn the_attribution_needs_a_marker_and_not_just_the_name() {
    let declared = ["cos".to_string(), "putchar".to_string()];
    // The name is there; the reason is not.
    let unrelated = "clang: error: no such file or directory: 'cos.o'";
    assert!(science_codegen_llvm::link::undefined_symbols(unrelated, &declared).is_empty());
    // `link.exe`'s Spanish, which is what this machine emits. The prose is
    // translated and `LNK2019` is not.
    let localised = "p.obj : error LNK2019: símbolo externo cosinus sin resolver al que se hace \
                     referencia en la función _S4main";
    assert!(
        science_codegen_llvm::link::undefined_symbols(localised, &["cosinus".to_string()])
            .len()
            == 1,
        "a localised linker must still be understood, or `SC0461` never fires outside English"
    );
    // A whole word: `cos` is a substring of `cosinus` and must not match it.
    assert!(
        science_codegen_llvm::link::undefined_symbols(localised, &declared).is_empty(),
        "`cos` is a substring of `cosinus`"
    );
}

/// What a `library` clause becomes on each target, including the two names
/// that become nothing.
///
/// §5.2's table gives the Windows link name as `openblas.lib` and `clang`
/// takes `-lopenblas` for it — so the translation table Decision 25 budgets
/// for is not needed, **except** for `c` and `m`, which name no file on MSVC
/// because both are inside the UCRT the driver already links. Passing them
/// through produces `LNK1181: cannot open input file 'm.lib'` for a program
/// whose only sin was to spell stage 2's own `library "m"`.
#[test]
fn the_library_clause_becomes_the_right_flag_on_each_target() {
    for triple in [Triple::X86_64LinuxGnu, Triple::Aarch64AppleDarwin] {
        assert_eq!(science_codegen_llvm::link::library_flags(triple, "m"), vec!["-lm"]);
        assert_eq!(
            science_codegen_llvm::link::library_flags(triple, "openblas"),
            vec!["-lopenblas"]
        );
    }
    let windows = Triple::X86_64WindowsMsvc;
    assert!(science_codegen_llvm::link::library_flags(windows, "m").is_empty());
    assert!(science_codegen_llvm::link::library_flags(windows, "c").is_empty());
    assert_eq!(
        science_codegen_llvm::link::library_flags(windows, "openblas"),
        vec!["-lopenblas"],
        "only the two C-standard names are special; everything else is `-l` and the name"
    );
}

// --- stage 3: control flow ------------------------------------------------

/// A loop with a back edge, a comparison, a `break`, and an accumulator whose
/// value is printed.
///
/// §10's stage 3 program sums `0..10` with a `for` and prints the total. This
/// is the same computation with `loop:` and a `break`, because `for` does not
/// reach this backend (see the module note), and the total is shown as the
/// byte `'0'` — `45 + 3 == 48` — because there is no way to print a number.
///
/// **The answer depends on every part of it**: a back edge that runs once too
/// often, a comparison with the wrong predicate, or an `alloca` that moved
/// into the loop body all give a different byte or no byte at all.
#[test]
fn stage_three_runs_a_loop_and_prints_what_it_accumulated() {
    let stdout = byte(
        "loop",
        &format!(
            "{PUTCHAR}let mutable i be 0i32\nlet mutable total be 0i32\nloop:\n    \
             i be i + 1i32\n    total be total + i\n    if i >= 9i32:\n        break\n\
             let r be unsafe: putchar(total + 3i32)\n"
        ),
    );
    assert_eq!(stdout, "0", "1+2+…+9 is 45, and 45 + 3 is the byte `0`");
}

/// Decision 8 holds across a loop: every `alloca` is in the entry block.
///
/// **An `alloca` in a loop body is not a verifier failure and not visible in
/// any output.** It is a stack frame that grows by one slot per iteration
/// until the program dies, which for a ten-iteration test is invisible and for
/// a real program is not. Stage 1 could not have this bug — one basic block —
/// and the shape is pinned here because the repair (`BodyCtx::entry`) is a
/// thing a future edit would undo by pushing an `alloca` where it is needed.
#[test]
fn every_alloca_is_in_the_entry_block_however_late_the_slot_was_invented() {
    let dir = scratch("stage23", "allocas");
    require_runtime();
    let ir = lower(&format!(
        "{PUTCHAR}let mutable i be 0i32\nloop:\n    i be i + 1i32\n    \
         if i >= 3i32:\n        break\nlet r be unsafe: putchar(65)\n"
    ))
    .build_at(&executable(&dir, "allocas"), OptLevel::O0)
    .ir;
    // Not `"define void @_S4main"`: a script body's `_S4main` returns
    // `Error?`, which is `void` only on the convention that returns it
    // through `sret` — SysV and AAPCS64 return it in two registers instead
    // (`lower_c_main`'s own history), so the return type in this text varies
    // with the host and the symbol is the only part of the signature that
    // does not.
    let science_main = ir
        .split_once("@_S4main(")
        .expect("the entry point")
        .1
        .split_once("\n}")
        .expect("its body")
        .0;
    let entry = science_main.split_once("bb1:").map(|(head, _)| head).unwrap_or(science_main);
    let total = science_main.matches("alloca ").count();
    assert!(total > 0, "the program has locals:\n{science_main}");
    assert_eq!(
        entry.matches("alloca ").count(),
        total,
        "Decision 8: every `alloca` is in the entry block, and one of them is not:\n{science_main}"
    );
}

/// Decision 5 holds: one MIR basic block, one LLVM basic block, and the
/// numbers are MIR's.
///
/// *"Every MIR basic block becomes exactly one LLVM basic block, and codegen
/// merges nothing"* — which is what makes a MIR dump and an IR dump diffable,
/// and which Gate I and Gate J rest on. The labels are `bbN`, named after the
/// MIR block, so the check is that MIR's numbering survives and that codegen
/// invented nothing.
///
/// **The comparison is against MIR's *reachable* blocks, and that is a fact
/// about `Built::ir` rather than a weakening.** That field holds the module
/// **after** Decision 33's pipeline, because `--emit=llvm-ir` wants the
/// optimised form — and even at `-O0` the pipeline drops a basic block with no
/// predecessors. `loop:` with a `break` produces one: MIR's lowering leaves an
/// unreachable arm behind, `science-mir` does no dead-block elimination on
/// purpose (*"a `Goto` chain that a peephole would collapse is left alone,
/// because codegen-and-linking.md Decision 5 wants a MIR dump and an IR dump
/// to be diffable"*), and LLVM removes it anyway. So the one-to-one claim is
/// exact for the emitter and one-to-one-on-reachable-blocks for anything that
/// reads `Built::ir`, and a test that asserted otherwise would be asserting
/// against `SimplifyCFG`.
#[test]
fn the_ir_has_one_block_per_reachable_mir_block_and_they_keep_their_numbers() {
    let dir = scratch("stage23", "blocks");
    require_runtime();
    let source = format!(
        "{PUTCHAR}let mutable i be 0i32\nloop:\n    i be i + 1i32\n    \
         if i >= 3i32:\n        break\nlet r be unsafe: putchar(65)\n"
    );
    let lowered = lower(&source);
    let body = lowered
        .bodies
        .iter()
        .find(|body| lowered.krate.defs.get(body.def()).name == "main")
        .expect("a `main`");
    let reachable = science_mir::mir::reverse_postorder(body);
    assert!(reachable.len() > 4, "this program has a loop and a branch in it");
    let ir = lowered.build_at(&executable(&dir, "blocks"), OptLevel::O0).ir;
    let science_main = ir
        .split_once("@_S4main(")
        .expect("the entry point")
        .1
        .split_once("\n}")
        .expect("its body")
        .0;
    for block in &reachable {
        let label = format!("bb{}:", block.index());
        assert!(
            science_main.contains(&label),
            "MIR's {label} has no LLVM block:\n{science_main}"
        );
    }
    let emitted = science_main
        .lines()
        .filter(|line| {
            line.split_once(':').is_some_and(|(head, _)| {
                head.len() > 2
                    && head.starts_with("bb")
                    && head[2..].bytes().all(|b| b.is_ascii_digit())
            })
        })
        .count();
    assert_eq!(
        emitted,
        reachable.len(),
        "codegen merged or invented a block; Decision 5 allows neither:\n{science_main}"
    );
    // The back edge is the thing a loop is, and it is the one shape a
    // block-per-block lowering could get right in count and wrong in structure.
    assert!(
        science_main.contains("preds = %bb5, %bb0") || science_main.contains("preds = %bb0, %bb5"),
        "the loop header has no back edge:\n{science_main}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `if`/`else` over a comparison, both edges taken by two programs.
#[test]
fn both_edges_of_an_if_are_taken_by_the_program_that_should_take_them() {
    assert_eq!(answers("if-true", "let total be 1 + 2 + 3\n", "total is 6"), "Y");
    assert_eq!(answers("if-false", "let total be 1 + 2 + 3\n", "total is 7"), "N");
}

/// `not`, which is emitted as a comparison against zero rather than as a
/// bitwise complement.
///
/// §3.1 gives `Bool` two widths, and `LLVMBuildNot` is right at neither: on
/// the `i8` memory form it turns `false` into `0xff`, which is a bit pattern
/// no `Bool` may hold, which the `trunc` on the next branch reads as `true`,
/// and which nothing reports. `x == 0` is right at both.
#[test]
fn not_of_a_stored_bool_is_the_value_it_should_be() {
    assert_eq!(answers("not-true", "let a be true\nlet b be not a\n", "b"), "N");
    assert_eq!(answers("not-false", "let a be false\nlet b be not a\n", "b"), "Y");
    assert_eq!(answers("not-not", "let a be true\nlet b be not not a\n", "b"), "Y");
}

// --- stage 3: integers, and what "integers" was hiding --------------------

/// Signedness is the operand's and not the instruction's default.
///
/// `200u8 > 100u8` is true; the same two bytes compared as `i8` are `-56` and
/// `100`, so a signed predicate answers false. Nothing in
/// `science_codegen::backend::Inst::Cmp` decides this — it carries `signed` and
/// the lowering fills it in from the operand's type — so this is the test that
/// the lowering asks the right operand.
#[test]
fn an_unsigned_comparison_is_unsigned() {
    assert_eq!(answers("u8-gt", "let a be 200u8\nlet b be 100u8\n", "a > b"), "Y");
    assert_eq!(answers("i8-gt", "let a be -56i8\nlet b be 100i8\n", "a > b"), "N");
}

/// Overflow wraps, at the operand's own width, and does not trap.
///
/// The core spec says *"integer overflow panics in debug builds and wraps in
/// release, as Rust does"*, and `codegen-and-linking.md` says nothing about
/// overflow anywhere. What is emitted is the release half — `add` with neither
/// `nsw` nor `nuw` — so these two answers are the spec's for a release build
/// exactly. `nsw` would make both **undefined** rather than wrapping, which is
/// the silent version of this test failing.
#[test]
fn integer_overflow_wraps_at_the_operands_width() {
    assert_eq!(answers("i8-wrap", "let a be 127i8\nlet b be a + 1i8\n", "b is -128i8"), "Y");
    assert_eq!(answers("u8-wrap", "let a be 255u8\nlet b be a + 1u8\n", "b is 0u8"), "Y");
    assert_eq!(
        answers("i32-wrap", "let a be 2147483647i32\nlet b be a + 1i32\n", "b is -2147483648i32"),
        "Y"
    );
}

/// A constant with no value operand beside it still takes its own width.
///
/// **This is the miscompile of finding 12, as a program.** `0i8 - 128i8` is two
/// constants: `Operand::ConstInt` carries no type, the emitter's `width_hint`
/// reads the *other* operand and there is no other operand, so both took the
/// default `i64` — and `store i64` into a one-byte slot is legal IR under
/// opaque pointers. The verifier passed it, the program linked, and the
/// comparison answered `false` while writing seven bytes past its slot.
#[test]
fn arithmetic_on_two_constants_is_done_at_the_declared_width() {
    assert_eq!(answers("const-const", "let b be 0i8 - 128i8\n", "b is -128i8"), "Y");
    assert_eq!(answers("neg-lit", "let z be -128i8\n", "z < 0i8"), "Y");
    assert_eq!(answers("small-add", "let s be 1i8 + 2i8\n", "s is 3i8"), "Y");
}

/// Floats are computed at their own width, and `-` is `fneg`.
///
/// `0.5f32 + 0.25f32` at `double` and at `float` give the same answer, so the
/// third case is the one that would catch a width mistake: `16777217f32` is
/// not representable and rounds to `16777216`, which `double` arithmetic would
/// have kept.
#[test]
fn float_arithmetic_is_done_at_the_declared_width() {
    assert_eq!(answers("f32-add", "let a be 0.5f32\nlet b be a + 0.25f32\n", "b is 0.75f32"), "Y");
    assert_eq!(
        answers("f32-round", "let a be 16777216.0f32\nlet b be a + 1.0f32\n", "b is a"),
        "Y",
        "`16777217` is not representable in an `f32`; at `double` it would be"
    );
    assert_eq!(answers("f64-neg", "let a be 3.5\nlet b be -a\n", "b < 0.0"), "Y");
}

/// `Char` compares as a scalar and is not confused with an integer width.
#[test]
fn a_char_compares_as_itself() {
    assert_eq!(answers("char-eq", "let c be 'A'\n", "c is 'A'"), "Y");
    assert_eq!(answers("char-ne", "let c be 'A'\n", "c is 'B'"), "N");
}

/// §7.3 obligation 1 still holds over everything this file emits.
#[test]
fn no_fast_math_flag_reaches_any_of_these_programs() {
    let dir = scratch("stage23", "fastmath");
    require_runtime();
    let ir = lower(
        "unsafe extern \"C\" library \"m\":\n    def cos(x: F64) -> F64\n\n\
         let x be unsafe: cos(0.5)\nlet y be x * x + x\nif y > 0.0:\n    print(\"p\")\nelse:\n\
         \x20   print(\"n\")\n",
    )
    .build_at(&executable(&dir, "fastmath"), OptLevel::O3)
    .ir;
    assert_eq!(science_codegen::target::first_fast_math_flag(&ir), None, "{ir}");
    let _ = std::fs::remove_dir_all(&dir);
}

// --- the boundary ---------------------------------------------------------

/// Where the boundary now is, construct by construct, each named in its own
/// refusal.
///
/// **This is the most valuable assertion in the file**, because it is the one
/// that goes stale the moment somebody lowers one of these: a construct that
/// starts working and is still listed here fails loudly rather than leaving
/// the boundary undocumented. Each row is the smallest program that still
/// cannot be built, and the fragment is what the refusal has to name.
///
/// **It has gone stale once already and that is the design working.** Four
/// rows left this list when a `choice`, a second function and a record were
/// emitted — a `match` over a two-variant payload-free `choice`, a program with
/// a second function, a record literal, and a drop that has to run nothing —
/// and each of them failed here first, loudly, with *"the boundary has moved
/// and this list has not"*. `tests/past_stage_three.rs` is where they went.
#[test]
fn the_boundary_is_where_it_says_it_is() {
    let cases: &[(&str, &str, &str)] = &[
        // §10's own stage 3 program. The hole is above this crate, and **what
        // it is has changed while the row has not moved.** It was
        // `thir::ExprKind::For` having no field for a callee, which this crate
        // reports as a `for` loop; the refusal that fires first is now the
        // range *temporary*, because `science-resolve`'s `builtins.rs` gained
        // `Range of T implements Iterate:` and `science-types` types `0..3` at
        // `Range of I64` instead of `TyKind::Error`. A value of that type is
        // one of §2.6's runtime containers and this backend emits no
        // descriptor for one, so the fragment is the type and no longer the
        // construct. The callee hole is still there behind it and is still
        // this list's reason for the row.
        ("for", "let mutable t be 0\nfor i in 0..3:\n    t be t + i\n", "`Range of I64`"),
        // Refused rather than emitted: nothing above emits the zero check that
        // `IntOp`'s own note says the caller has already made.
        ("div", "let a be 6\nlet b be a / 2\nprint(\"x\")\n", "divide-by-zero"),
        ("shift", "let a be 6\nlet b be a << 2\nprint(\"x\")\n", "shift"),
        // A value that owns something Decision 12's glue would have to
        // release. **Three of the four cases are lowered now**: a `br` when
        // the value owns nothing, `science_string_free` when it is a bare
        // `String`, and `Lowerer::intern_drop_glue`'s emitted function when it
        // is a record — `tests/methods.rs` runs a record that owns a `String`
        // and a record that owns one through a nested record. What is left is
        // a `choice`: releasing its payload means switching on the
        // discriminant and dropping only the active variant, which is one
        // block per arm where the glue builder emits one block.
        (
            "drop",
            "choice C:\n    A\n    B(String)\n\nlet c be A\nprint(\"x\")\n",
            "is not a record",
        ),
        // **Still refused, and no longer for the reason this list was written
        // with.** `cg_ty` has the `TyKind::Tuple` arm now and
        // `let t: (Int, Int) be (1, 2)` builds, runs and prints; what is left
        // is that the *unannotated* literal types as `(<error>, <error>)`,
        // because `science-types`' `ExprKind::Tuple` arm reads each element's
        // type before inference has defaulted it. `tests/past_stage_three.rs`
        // is the account and the programs.
        ("tuple", "let t be (1, 2)\nprint(\"x\")\n", "`TyKind::Error`"),
        // **A cast used to be here.** `Rvalue::Cast` has a lowering and
        // `tests/casts.rs` runs every pair §5.1 defines; the pairs it does not
        // define are refused there.
        // **A method used to be here and it built the day somebody asked what
        // its receiver's type actually was.** The refusal read *"its receiver
        // is a `Self` this crate cannot resolve to a concrete type"*, and MIR's
        // `_1` for `P has: def get(self)` is a `borrowed P` — substituted by
        // `science-types` before THIR exists. Crate §3 finding 24;
        // `tests/methods.rs` is the programs.
        //
        // What is left of it is the one receiver that really is a `Self`: the
        // **default body** of a method an `interface` declares, which needs one
        // copy per implementor and is therefore the same refusal `generic`
        // below gets.
        (
            "interface-default",
            "interface S:\n    def size(self) -> Int\n\n    def twice(self) -> Int:\n        \
             self.size() + self.size()\n\ntype P:\n    x: Int\n\nP implements S:\n    \
             def size(self) -> Int:\n        self.x\n\nlet p be P(x: 1)\n\
             let v be p.twice()\nprint(\"x\")\n",
            "monomorphis",
        ),
        // Nothing monomorphises, so a generic function's parameter reaches here
        // as a `TyKind::Param` with no layout.
        (
            "generic",
            "def identity of T(value: T) -> T:\n    value\n\nlet v be identity(1)\nprint(\"x\")\n",
            "monomorphis",
        ),
    ];
    for (name, source, fragment) in cases {
        let dir = scratch("stage23", name);
        let outcome = lower(source).try_build(&executable(&dir, name), OptLevel::O2).map(|_| ());
        let Err(diagnostics) = outcome else {
            panic!("`{name}` built: the boundary has moved and this list has not");
        };
        let first = diagnostics.first().expect("a refusal carries a diagnostic");
        assert_eq!(
            first.code,
            science_codegen::diagnostics::code::SC0400,
            "`{name}` is refused with the wrong code"
        );
        assert!(
            first.message.contains(fragment),
            "`{name}`'s refusal must name the construct, and it said: {}",
            first.message
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// A refusal is `SC0400` and never a panic, an internal error or a program.
///
/// The list above asserts what each one says; this asserts that the build
/// *failed* at all, which is the half that a lowering added without a refusal
/// would break silently.
#[test]
fn nothing_past_the_boundary_produces_an_executable() {
    for source in [
        "let mutable t be 0\nfor i in 0..3:\n    t be t + i\n",
        "let t be (1, 2)\nprint(\"x\")\n",
        "let a be 6\nlet b be a / 2\nprint(\"x\")\n",
    ] {
        let dir = scratch("stage23", "refused");
        let output = executable(&dir, "refused");
        assert!(lower(source).try_build(&output, OptLevel::O2).is_err());
        assert!(!output.is_file(), "a refused build left an executable behind");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
