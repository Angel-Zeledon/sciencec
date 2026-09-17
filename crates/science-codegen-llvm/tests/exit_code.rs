//! `script-mode.md` §2.3's exit table, as two programs that are built, run, and
//! asked what status they exited with and what they said.
//!
//! **The decision.** One test compiles a Science program through the whole
//! pipeline and asserts it exits 0 with nothing on stderr. The other builds a
//! module whose `_S4main` returns a **non-null** `Error?` — written by this file
//! rather than lowered from source — and asserts that it exits 1 and says
//! something on stderr beginning with §2.3's `error: `.
//!
//! **Why the second one is not a Science program, and it is not a shortcut.**
//! §2.3's fourth row needs a non-null `(any Error)?`. Producing one from source
//! means a concrete error type, a `BoxThenWiden` coercion, a heap box and a
//! vtable with `Error.message` in it, and this backend has none of the four.
//! So the row would be untestable until stage 4, which is exactly the state
//! that let it sit unreachable and aborting.
//!
//! What this file does instead is invent the **value** and keep everything else
//! real: the `main` under test is the one [`Lowerer::lower_c_main`] emits, the
//! module is assembled by [`science_codegen_llvm::emit_and_link`] — the same
//! function `build` calls — and the program is linked against the same
//! `science_rt` and run. The invented half is one `store` of a global's address
//! into the return slot, which is what a real `Error?` would leave there.
//! `tests/emitter.rs` is the precedent and gives the same reason in its own
//! words: *"none of these shapes is reachable from a Science program this
//! compiler can compile."*
//!
//! **The cost.** A test that builds its own `_S4main` can pass while the
//! lowering that would produce the same shape is wrong — so this file asserts
//! nothing about how an error gets into that slot, only about what `main` does
//! once it is there. The day a Science program can return an error, the second
//! test should become the first's shape and this paragraph should go.
//!
//! §10's discipline is what both halves are held to: *"no stage is finished
//! until an execution test asserts its output and exit code."* Each assertion
//! below is a pair.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::abi::AbiSignature;
use science_codegen::backend::{BlockId, Inst, LocalId, Operand, Terminator};
use science_codegen::layout::{CgTy, Triple, layout_of};
use science_codegen::mangle::{MonoKey, mangle};
use science_codegen::runtime::EXIT_CONTRACT;
use science_codegen::target::{OptLevel, TargetConfig};
use science_codegen_llvm::emit::{ExtBlock, ExtBody, ExtInst};
use science_codegen_llvm::emit_and_link;
use science_codegen_llvm::lower::{ERROR_MESSAGE, Lowerer};

/// Rows 1 to 3: a script body that hands back a null error exits 0 and says
/// nothing.
///
/// **`return null` and not `print("hello, world")`**, which `tests/hello.rs`
/// already runs. This program has no output at all, so "says nothing on stderr"
/// is the assertion rather than a side effect of a program that happens to write
/// to the other stream — and the exit status comes from `main`'s null edge
/// alone, with no `science_print` between it and the start of the process.
///
/// **It is the third row and not the first, and the difference is a front-end
/// gap this test cannot close.** §2.3's first row is *falling off the end*,
/// which needs a file whose script body is empty; such a file produces no MIR
/// body at all today, so there is nothing for the backend to build. The second
/// row is a bare `return`, which §2.4 calls *"sugar for `return null`"* and
/// which does not typecheck: `sciencec check` on a file containing only
/// `return` reports `SC0525`, *expected `any Error?`, found `()`*. All three
/// rows are one value and one edge by the time they reach here, so the edge is
/// covered; the two spellings are not, and neither is this crate's to fix.
#[test]
fn a_script_body_that_returns_null_exits_zero_and_says_nothing() {
    let lowered = lower("return null\n");
    let dir = scratch("exit", "null");
    require_runtime();
    let built = lowered.build_at(&executable(&dir, "ok"), OptLevel::O2);

    let ran = run(&built);
    assert_eq!(
        ran.status,
        Some(0),
        "`script-mode.md` §2.3: a script body whose error is null exits 0. stderr: {}",
        ran.stderr
    );
    assert_eq!(ran.stderr, "", "a script that did not fail must say nothing on stderr");
    assert_eq!(ran.stdout, "", "and nothing on stdout either");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Row 4: a script body that hands back a non-null error exits 1 and says so on
/// stderr.
///
/// **The one this file exists for.** Before `science-rt` grew
/// `science_write_error_bytes` and `science_exit`, this program called
/// `science_panic_bytes` and aborted — status 3 on Windows, `SIGABRT` on POSIX
/// — because there was no symbol that wrote to stderr and returned and none
/// that chose an exit status. The assertions below are the two halves of §2.3's
/// fourth row and neither is worth having without the other: a status with no
/// message is a program that fails silently, and a message with no status is one
/// a shell believes succeeded.
///
/// **`-O0`, for `tests/hello.rs`'s reason applied to the other branch.** The
/// value `main` reads here is the address of a global, so it is a compile-time
/// constant: at `-O2` the inliner folds `_S4main` into `main`, proves the
/// pointer non-null and deletes the branch — correctly, and leaving a program
/// that exercises the failing block without ever having taken the edge to it.
/// `-O0` keeps the `icmp` and the `br` this test is about.
#[test]
fn a_script_body_that_returns_an_error_exits_one_and_says_something() {
    // A crate to borrow a `DefTable` and a `Types` from. The `Lowerer` needs
    // both to exist; this test's `_S4main` is built from neither, because there
    // is no Science source that produces it.
    let lowered = lower("return null\n");
    let triple = Triple::host().expect("a supported host");
    let mut lowerer = Lowerer::new(triple, &lowered.krate.defs, &lowered.types);

    // `Error?` is `(any Error)?`: Decision 13's two-word fat pointer with the
    // null niche in the data word, returned through `sret` on every supported
    // convention because it is neither 1, 2, 4 nor 8 bytes.
    let ret_layout = layout_of(triple, &CgTy::nullable(CgTy::Interface));
    let science_main = AbiSignature::science(
        triple,
        mangle(&MonoKey::plain(&["main"])),
        ret_layout.clone(),
        vec![],
    );
    assert!(
        science_main.ret.is_sret(),
        "the whole shape of the emitted `main` rests on `Error?` coming back indirectly"
    );

    // The invented half, and all of it: a non-null data word. Any global's
    // address will do — what a real `Error?` puts here is a box, and `main`
    // never dereferences it. §3.4 is why the vtable word beside it is left
    // undefined and why that is safe: nothing may load it, on either edge.
    let marker = lowerer.intern_literal("a stand-in for a boxed error");
    let body = ExtBody {
        blocks: vec![ExtBlock {
            id: BlockId(0),
            label: "entry".to_string(),
            insts: vec![
                ExtInst::ReturnSlot { local: LocalId(0), layout: ret_layout },
                ExtInst::Above(Inst::Store {
                    local: LocalId(0),
                    value: Operand::GlobalAddr(marker.bytes_symbol.clone()),
                }),
            ],
            terminator: Terminator::Return(None),
        }],
    };

    let c_main = lowerer.lower_c_main(&science_main).expect("the emitted `main`");
    let module = lowerer.finish(vec![(science_main, body), c_main]);

    let dir = scratch("exit", "error");
    let output = executable(&dir, "failing");
    require_runtime();
    let config = TargetConfig::new(triple, OptLevel::O0);
    let built = match emit_and_link(&module, "exit.science", &config, &output) {
        Ok(built) => built,
        Err(diagnostics) => panic!(
            "the build failed:\n{}",
            diagnostics
                .iter()
                .map(|d| format!("{}: {}", d.code, d.message))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    };

    // The emitter's own text, before the program is run: the failing block calls
    // the writer and then the exit, and it calls neither of the two symbols that
    // would mean the old behaviour is back.
    assert!(
        built.ir.contains("@science_write_error_bytes(") && built.ir.contains("@science_exit("),
        "the failing edge does not go through finding 5's two symbols:\n{}",
        built.ir
    );
    assert!(
        !built.ir.contains("@science_panic_bytes("),
        "a failing script body is not a panic, and §2.3's fourth row is not §8's abort:\n{}",
        built.ir
    );
    assert!(
        built.ir.contains("icmp eq ptr"),
        "§3.4: the test is against the data pointer alone:\n{}",
        built.ir
    );

    let ran = run(&built);
    assert_eq!(
        ran.status,
        Some(EXIT_CONTRACT.required_status),
        "`script-mode.md` §2.3: a non-null error is status 1, and not the abort status. \
         stderr: {}",
        ran.stderr
    );
    assert!(
        ran.stderr.starts_with(EXIT_CONTRACT.required_prefix),
        "§2.3 requires the `error: ` prefix; stderr was {:?}",
        ran.stderr
    );
    assert_eq!(ran.stderr, ERROR_MESSAGE, "the message is the constant, verbatim");
    assert_eq!(ran.stdout, "", "§2.3 puts the error on stderr, and nothing goes to stdout");
    let _ = std::fs::remove_dir_all(&dir);
}

/// What the message says, and the half of §2.3 it does not say.
///
/// **A unit test in an integration file, on purpose.** [`ERROR_MESSAGE`] is the
/// one place this compiler falls short of a design note *in output a user
/// reads*, so the shortfall is asserted rather than described: the prefix is
/// §2.3's, and the rest is not the error's `Display`, because the prelude's
/// `Display` has no method to call. `EXIT_CONTRACT.display_is_renderable` is the
/// same fact one crate up.
///
/// The day `Display` gets a method, this test is the one that says what to
/// delete.
#[test]
fn the_message_carries_the_required_prefix_and_admits_what_it_cannot_render() {
    assert!(ERROR_MESSAGE.starts_with(EXIT_CONTRACT.required_prefix));
    assert!(ERROR_MESSAGE.ends_with('\n'), "the writer adds nothing, so the constant must");
    assert!(
        ERROR_MESSAGE.contains("Display"),
        "the message must name what it could not reach, or it is a shrug: {ERROR_MESSAGE:?}"
    );
    assert!(
        !EXIT_CONTRACT.renders_the_error(),
        "if this fails, `Display` has a method and this message is no longer the honest answer"
    );
}
