//! `print` of a value the program is still holding, **built, linked, run**,
//! with stdout asserted.
//!
//! # The program this file exists for
//!
//! ```text
//! let s be "hola"
//! print(s)
//! print(s)
//! ```
//!
//! It built, linked, ran, exited 0 and printed `hola` and then an empty line.
//! Nothing anywhere reported anything: the type checker has no signature for
//! `print` to check the argument against, region inference had no borrow to be
//! about, and `LLVMVerifyModule` cannot see the difference between a
//! `ScienceString` that has been freed and one that has not.
//!
//! **What was wrong was one word in an operand.** `science-mir`'s `lower` §5
//! called the argument of a signature-less callee a `move`, this crate's
//! [`Lowerer::lower_print`] frees what it is handed the last reference to, so
//! the first `print` released the buffer and the second read it back. Crate §3
//! finding 23 is the account.
//!
//! # Why it is a *run* and not an IR assertion
//!
//! The same reason `tests/interpolation.rs` gives for finding 18: a free of a
//! buffer somebody else still owns is `call void @science_string_free(ptr %x)`,
//! and so is a correct one. The IR is identical in shape and the count is
//! identical too — one `science_string_free` either way — because the bug was
//! not *how many* releases there were but *which* value got one. Only running
//! it separates them, and only running it with the output captured: what the
//! freed read produced here was an empty line, which a human at a terminal
//! reads straight past.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

/// Build one program at `-O2` and give back what it printed.
///
/// `-O2` for `tests/interpolation.rs`'s reason: a `print` is `alloca`s and
/// addresses, and the optimiser is where a load of something that was freed
/// stops being indistinguishable from a load of something that was not.
fn prints(name: &str, source: &str) -> String {
    let dir = scratch("printing", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// The IR of a program that built.
fn ir(name: &str, source: &str) -> String {
    let dir = scratch("printing", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O0);
    let text = built.ir.clone();
    let _ = std::fs::remove_dir_all(&dir);
    text
}

/// **The line.** `hola`, twice.
#[test]
fn a_printed_binding_is_still_the_programs_afterwards() {
    let source = "let s be \"hola\"\nprint(s)\nprint(s)\n";
    assert_eq!(prints("twice", source), "hola\nhola\n");
}

/// **Three prints and a use after both**, so that the failure is a length
/// rather than a coincidence.
///
/// The freed version printed the string once and then two empty lines, because
/// `science_string_free` leaves the header zeroed and
/// `science-rt`'s `science_print` renders a zero-length string as nothing. A
/// fourth use — the interpolation on the last line — is there because a hole
/// that reads a freed buffer aborts inside the runtime rather than printing
/// nothing, which is the louder of the two failures and the one the earlier
/// spelling would have reached second.
#[test]
fn a_binding_survives_every_print_of_it() {
    let source = "let s be \"mundo\"\nprint(s)\nprint(s)\nprint(s)\nprint(f\"{s}!\")\n";
    assert_eq!(prints("thrice", source), "mundo\nmundo\nmundo\nmundo!\n");
}

/// **A `String` is released exactly once, and by its scope.**
///
/// The binding is `String` and a `String` owns memory, so exactly one
/// `science_string_free` is owed. It used to come from
/// [`Lowerer::lower_print`]'s call site, because MIR said the call was the
/// value's last owner; it comes from the `TerminatorKind::Drop` that
/// `emit_scope_exit` emits now, because MIR says the frame still owns it. The
/// count is the same in both, which is precisely why the count is not the
/// assertion this file rests on — it is here to catch the other direction, a
/// free that was removed and not replaced.
#[test]
fn a_printed_binding_is_freed_once_by_its_scope() {
    let text = ir("freed_once", "let s be \"hola\"\nprint(s)\nprint(s)\n");
    assert_eq!(
        text.matches("@science_string_free(").count() - 1,
        1,
        "a printed binding should be released once, by its scope:\n{text}"
    );
}

/// **A literal still is the call site's, and the call site still frees it.**
///
/// Decision 15 makes `print("…")` three calls — build, print, free — because
/// the `String` the runtime builds has no MIR local and therefore no scope to
/// be dropped by. That arm did not change and this is what pins it: two frees
/// for two literals, one per evaluation, which is §2.6's *"a literal in a loop
/// allocates on every iteration"* seen from the releasing end.
#[test]
fn a_printed_literal_is_freed_by_the_call_site() {
    let text = ir("literal", "print(\"una\")\nprint(\"otra\")\n");
    assert_eq!(
        text.matches("@science_string_free(").count() - 1,
        2,
        "each literal `print` builds a temporary and frees it:\n{text}"
    );
    assert_eq!(prints("literal_runs", "print(\"una\")\nprint(\"otra\")\n"), "una\notra\n");
}
