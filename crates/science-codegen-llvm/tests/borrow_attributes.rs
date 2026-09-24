//! Decision 24's parameter attributes, read back out of the emitted IR.
//!
//! > *A `mutable borrowed T` parameter is emitted `noalias nocapture` and
//! > aligned. A `borrowed T` parameter is emitted `readonly nocapture` and
//! > aligned, and **not** `noalias`.*
//!
//! **`nocapture` is the one word of that sentence this compiler does not emit**,
//! and [`Lowerer::param_attrs`] carries the account of why: a Science function
//! may return a borrow it was given, which is exactly what `nocapture` promises
//! it does not do. [`a_borrow_that_escapes_the_callee_is_not_claimed_nocapture`]
//! is the program, and it is the reason rather than a restatement of it.
//!
//! **The negative case is the one this file exists for.** `noalias` on a
//! `&mut T` is an optimisation; `noalias` on a `&T` is a **miscompile**, and
//! §4.4 says why in the sentence that makes this the most dangerous emission in
//! the compiler: *"this is the single place in the language where a bug in
//! region inference produces a wrong answer rather than a missed error"*. Two
//! shared borrows of one place are legal — rule 4 is *shared many, or exclusive
//! one* — so the claim would be a lie the optimiser is entitled to act on, and
//! the failure would be a program that computes something else, silently, at
//! `-O2` only. A test that only checked the positive case would pass on the day
//! somebody moved `noalias` up one line in `borrow_attrs`.
//!
//! **Why the IR and not a run.** `noalias` has no observable behaviour: it is a
//! promise to the optimiser, and a correct program and a miscompiled one differ
//! only in what the optimiser was allowed to do. There is nothing to print. The
//! IR text is the artefact, and `Built::ir` is where the build already keeps it
//! — the same string `--emit=llvm-ir` writes out.
//!
//! **`-O0`, throughout.** These are assertions about what *codegen emitted*.
//! At `-O2` LLVM may add attributes of its own (it infers `nocapture` and
//! `readonly` routinely, and `memory(argmem: read)` besides), so an assertion
//! against optimised IR would be an assertion about LLVM's inference and would
//! pass with this crate emitting nothing at all — which is exactly the state
//! finding 14 described.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, scratch};
use science_codegen::target::OptLevel;

/// The IR of a program built at `-O0`, with §4.4's flag either way.
fn ir(source: &str, name: &str, no_noalias: bool) -> String {
    let dir = scratch("borrow-attributes", name);
    require_runtime();
    let built = lower(source)
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

/// The one `define` line naming `needle`, with its attributes.
///
/// **Matched on `define` and not on the symbol alone**, because the same symbol
/// appears on the `call` at every call site and a `call`'s argument list carries
/// attributes too. The declaration is the claim Decision 24 is about; a call's
/// copy of it is downstream of this one.
fn define(ir: &str, needle: &str) -> String {
    let line = ir
        .lines()
        .find(|line| line.starts_with("define") && line.contains(needle))
        .unwrap_or_else(|| panic!("no `define` of `{needle}` in:\n{ir}"));
    line.to_string()
}

/// A program with one of each: a shared borrow of an `I64` and an exclusive
/// borrow of an `Int`.
///
/// **Both in one program**, so that the positive and the negative assertion are
/// about one run of one emitter. Two programs would leave room for the two
/// halves to disagree about the flag, the target or the optimisation level
/// without any test noticing.
const BOTH: &str = "\
def read(value: &I64) -> I64:
    value

def bump(counter: &mut Int):
    counter be counter + 1

def main():
    let mutable hits be 0
    bump(hits)
    print(read(hits))
";

/// Decision 24's first sentence: `&mut T` is `noalias` and aligned.
#[test]
fn an_exclusive_borrow_parameter_is_noalias_and_aligned() {
    let ir = ir(BOTH, "exclusive", false);
    let line = define(&ir, "bump");
    assert!(line.contains("noalias"), "no `noalias` on `&mut Int`:\n{line}");
    assert!(line.contains("align 8"), "no `align 8` on `&mut Int`:\n{line}");
    // Decision 24 does not put `readonly` on an exclusive borrow, and the whole
    // point of `&mut` is that the callee writes through it. `readonly` here
    // would be the same class of lie as `noalias` on a shared one, in the other
    // direction and with a louder failure.
    assert!(!line.contains("readonly"), "`readonly` on `&mut Int`:\n{line}");
}

/// Decision 24's second sentence, and **the assertion that catches a future
/// miscompile**: `&T` is `readonly` and aligned, and is **not** `noalias`.
///
/// Two shared borrows of one place are legal, so `noalias` would be false. The
/// consequence is not a worse program: it is a *different* program, at `-O2`,
/// with no diagnostic anywhere.
#[test]
fn a_shared_borrow_parameter_is_readonly_and_never_noalias() {
    let ir = ir(BOTH, "shared", false);
    let line = define(&ir, "read");
    assert!(line.contains("readonly"), "no `readonly` on `&I64`:\n{line}");
    assert!(line.contains("align 8"), "no `align 8` on `&I64`:\n{line}");
    assert!(
        !line.contains("noalias"),
        "`noalias` on a **shared** borrow — two shared borrows of one place are legal, so this \
         is a lie to the optimiser and a wrong answer at `-O2`:\n{line}"
    );
}

/// §4.4's mitigation: *"`--no-noalias` as an unsupported debugging flag that
/// suppresses the attribute, so that 'is this a region bug or a codegen bug' is
/// one recompile rather than a week."*
///
/// **And suppresses nothing else.** A flag that also dropped `nocapture`, or the
/// alignment, would change more than the one thing the bisection is about, and a
/// bisection that changes two things answers neither question.
#[test]
fn no_noalias_suppresses_the_attribute_on_an_exclusive_borrow_and_nothing_else() {
    let ir = ir(BOTH, "suppressed", true);
    let line = define(&ir, "bump");
    assert!(!line.contains("noalias"), "`--no-noalias` did not suppress it:\n{line}");
    assert!(line.contains("align 8"), "`--no-noalias` dropped the alignment too:\n{line}");

    // The shared borrow is unchanged, because it never had the attribute. This
    // is the assertion that the flag is a *suppression* and not a second code
    // path: the two builds must differ in exactly one token, on exactly one
    // function.
    let shared = define(&ir, "read");
    assert!(shared.contains("readonly"), "`--no-noalias` changed the shared borrow:\n{shared}");
    assert!(!shared.contains("noalias"), "`noalias` on a shared borrow:\n{shared}");
}

/// The alignment is the **referent's**, not the pointer's.
///
/// `&U8` is a pointer, and a pointer is eight-byte aligned on every F0 target —
/// so a `param_attrs` that reported `layout.align` instead of the referent's
/// would say `align 8` here and be believed. One byte is the right answer and
/// it is the only one that distinguishes the two readings.
#[test]
fn the_alignment_emitted_is_the_referents_and_not_the_pointers() {
    let source = "\
def peek(byte: &U8) -> U8:
    byte

def main():
    let mutable small be 7 as U8
    print(peek(small) as Int)
";
    let ir = ir(source, "referent-align", false);
    let line = define(&ir, "peek");
    assert!(line.contains("align 1"), "the alignment is not the referent's:\n{line}");
    assert!(!line.contains("align 8"), "the pointer's alignment was used:\n{line}");
}

/// **The program that says why `nocapture` is withheld**, and the one to read
/// before putting it back.
///
/// `keep` is handed a `&mut Int` and returns a record holding it. The emitted
/// body is `ret { ptr } { %0 }` — the argument pointer, copied into a value that
/// outlives the call — and LLVM's `nocapture` is the promise that exactly this
/// does not happen: *"the callee does not make any copies of the pointer that
/// outlive the callee itself"*. `def passthrough(n: &mut Int) -> &mut Int: n`
/// checks clean too, and is the same violation with nothing in between.
///
/// **The assertion is on the `&mut` that escapes, not on the emitter's mood.**
/// A future escape analysis is welcome to make this function `nocapture` again
/// — once it can tell `keep` from `bump`. What must not happen is `nocapture`
/// returning unconditionally, and this is the test that stops it: the attribute
/// would appear here, on the one function in the suite where it is provably
/// false.
///
/// `noalias` is still emitted, and still correct. Its claim is scoped to *"the
/// execution of the function"*, and `keep` executes no memory access at all;
/// what the caller does with the returned pointer afterwards is outside the
/// claim, and rule 4 governs it.
#[test]
fn a_borrow_that_escapes_the_callee_is_not_claimed_nocapture() {
    let source = "\
type Holder:
    seen: &mut Int

def keep(n: &mut Int) -> Holder:
    Holder(seen: n)

def main():
    let mutable x be 1
    let h be keep(x)
    h.seen be 42
    print(h.seen)
";
    let ir = ir(source, "escaping", false);
    let line = define(&ir, "keep");
    assert!(
        !line.contains("nocapture"),
        "`nocapture` on a pointer this function **returns** — LLVM is entitled to conclude the \
         caller's slot never escapes, and it does:\n{line}"
    );
    assert!(line.contains("noalias"), "no `noalias` on `&mut Int`:\n{line}");
    assert!(line.contains("align 8"), "no `align 8` on `&mut Int`:\n{line}");
}

/// A parameter that is not a borrow gets nothing.
///
/// **The attributes are pointer-only and LLVM rejects them elsewhere** — an
/// `align` on an `i64` argument fails the verifier — so this is not only a
/// tidiness assertion. It is also the one that says `param_attrs` keys off the
/// representation rather than off the position: `by_value` takes an `Int`, which
/// is `Direct` and is not a pointer, and a version that attributed every
/// parameter would not have got past `LLVMVerifyModule` to be asserted about.
#[test]
fn a_parameter_that_is_not_a_borrow_carries_no_pointer_attributes() {
    let source = "\
def by_value(n: Int) -> Int:
    n + 1

def main():
    print(by_value(41))
";
    let ir = ir(source, "by-value", false);
    let line = define(&ir, "by_value");
    for attribute in ["noalias", "nocapture", "readonly", "align"] {
        assert!(!line.contains(attribute), "`{attribute}` on a by-value `Int`:\n{line}");
    }
}
