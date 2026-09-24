//! A closure, called directly, with nothing captured — the boundary
//! `hello.rs`'s `a_program_past_the_boundary_is_refused_by_name` and
//! `stage_two_and_three.rs`'s `nothing_past_the_boundary_produces_an_executable`
//! used to sit at, and no longer does. Their own doc comments narrate why; this
//! file is the execution test §10's own discipline asks for whenever a
//! construct crosses that line — *"every stage below produces a program that
//! runs and prints something, and no stage is finished until an execution test
//! asserts its output and exit code."*
//!
//! **What moved, and then moved again.** `science-mir`'s `lower.rs` §8.5 gave a
//! capture-free closure's body its own `Body`, keyed on the closure's `param`;
//! §8.6 now gives a *capturing* one a body too, with the captures as trailing
//! parameters. `science_codegen::mono`'s `Mono::walk_rvalue` enqueues either
//! exactly as it enqueues any other address-taken function. Here, `cg_ty_in`'s
//! `TyKind::Closure` arm gives every closure value §10's own layout — *"a
//! struct of `{ fn ptr, captures }`"*, realised as the pair `{ code, env }`,
//! which is uniform across an arrow type the way the captures themselves could
//! never be; `Lowerer::lower_closure` builds it, filling the environment from
//! the borrows `science-mir` took; and `Lowerer::lower_indirect_closure_call`
//! loads both words and calls one with the other, exactly as
//! `Lowerer::lower_dispatch` loads a receiver and a vtable slot.
//!
//! **What the tests below are really pinning is the discipline, not the
//! plumbing.** `science-mir`'s `lower.rs` §8 decides that *"every capture is a
//! borrow of the captured place … there is no by-value capture and no copy
//! capture, not even for a `Copy` type"*, and three of the properties asserted
//! here are that sentence with an executable behind it: a captured `String` is
//! still readable by its owner afterwards
//! (`a_captured_string_is_borrowed_and_its_owner_still_has_it`), a closure that
//! captures nothing refers to no storage and so may leave the frame that built
//! it (`a_capture_free_closure_outlives_the_function_that_made_it`), and a
//! closure that would *move* a capture out is refused rather than built
//! (`a_closure_that_moves_a_capture_out_is_refused_by_name`) — §8.2's named
//! hole, closed in the only direction a borrow discipline leaves open.
//!
//! **One property is not here because this harness cannot see it.** A closure
//! that *outlives* what it captures is refused by `science-regions`, with
//! `SC0333` naming the captured binding; `harness::lower` stops at MIR and
//! never runs that check, so the fixture would build. It is exercised through
//! the driver instead.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

/// The corpus form this whole effort was scoped around, built and run.
#[test]
fn a_capture_free_closure_is_called_and_prints_its_answer() {
    let lowered = lower(
        "def apply(f: (Int) -> Int) -> Int:
    f(1)

def main():
    print(apply(x giving x + 1))
",
    );
    let dir = scratch("closures", "apply");
    require_runtime();
    let built = lowered.build_at(&executable(&dir, "apply"), OptLevel::O2);

    let ran = run(&built);
    assert_eq!(ran.stdout, "2\n", "stderr was: {}", ran.stderr);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Two closures in one module, both naming their parameter `x` — which two
/// unnamed `giving`s would too, since the implicit form always binds `each`.
/// Before `path_of`'s `DefKind::Param` arm, both would mangle to the same
/// symbol and one body would silently replace the other in the object file;
/// this is that collision, built and run rather than asserted about a string.
#[test]
fn two_closures_with_the_same_parameter_name_are_two_symbols() {
    let lowered = lower(
        "def apply(f: (Int) -> Int) -> Int:
    f(1)

def main():
    print(apply(x giving x + 1))
    print(apply(x giving x * 10))
",
    );
    let dir = scratch("closures", "two_closures_named_x");
    require_runtime();
    let built = lowered.build_at(&executable(&dir, "two_closures_named_x"), OptLevel::O2);

    let ran = run(&built);
    assert_eq!(ran.stdout, "2\n10\n", "stderr was: {}", ran.stderr);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A closure called a hundred thousand times, to catch what a single call
/// cannot: a function pointer materialised wrong, or a call built with the
/// wrong calling convention, both pass once and fail (or corrupt) on repeat.
#[test]
fn a_capture_free_closure_runs_a_hundred_thousand_times() {
    let lowered = lower(
        "def apply(f: (Int) -> Int) -> Int:
    f(1)

def main():
    let mutable total be 0
    for i in 0..100000:
        total be total + apply(x giving x + 1)
    print(total)
",
    );
    let dir = scratch("closures", "loop");
    require_runtime();
    let built = lowered.build_at(&executable(&dir, "loop"), OptLevel::O2);

    let ran = run(&built);
    assert_eq!(ran.stdout, "200000\n", "stderr was: {}", ran.stderr);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The shape §8.6 was scoped around: one capture, of an `Int`, read.
///
/// **`2 + 10` and not `2 + 1`**, so that a wrong answer is a wrong *number*
/// rather than a number that could have come from the parameter alone. An
/// environment word never stored, or stored at the wrong offset, reads as some
/// other stack slot; an environment never passed reads as whatever the register
/// held. Neither can produce `12` by accident.
#[test]
fn a_closure_that_captures_an_int_is_called_and_prints_its_answer() {
    let lowered = lower(
        "def apply(f: (Int) -> Int, v: Int) -> Int:
    return f(v)

def main():
    let n be 10
    print(f\"{apply(x giving x + n, 2)}\")
",
    );
    let dir = scratch("closures", "captures_int");
    require_runtime();
    let built = lowered.build_at(&executable(&dir, "captures_int"), OptLevel::O2);

    let ran = run(&built);
    assert_eq!(ran.stdout, "12\n", "stderr was: {}", ran.stderr);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The capture that would be interesting if the discipline were by value, and
/// is interesting for the opposite reason because it is not.
///
/// `science-mir`'s `lower.rs` §8 takes a **shared borrow** of `greeting`, so
/// the closure owns nothing, frees nothing, and leaves the binding exactly
/// where it was. The second `print` is the whole assertion: under a by-value or
/// a move capture it would read a `String` whose buffer the closure had taken
/// or released, and the failure would be a wrong line or a crash rather than a
/// diagnostic. Reading it twice *after* the closure has been built and called
/// is what makes the borrow observable from a program.
#[test]
fn a_captured_string_is_borrowed_and_its_owner_still_has_it() {
    let lowered = lower(
        "def apply(f: (Int) -> String, v: Int) -> String:
    return f(v)

def main():
    let greeting be \"hello\"
    print(apply(x giving f\"{greeting} {x}\", 7))
    print(greeting)
    print(f\"{greeting.length()}\")
",
    );
    let dir = scratch("closures", "captures_string");
    require_runtime();
    let built = lowered.build_at(&executable(&dir, "captures_string"), OptLevel::O2);

    let ran = run(&built);
    assert_eq!(ran.stdout, "hello 7\nhello\n5\n", "stderr was: {}", ran.stderr);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Three captures in one closure, and the environment's *order* is the thing
/// being read.
///
/// One capture cannot tell a right offset from a wrong one — every wrong offset
/// is the only offset. Three distinct powers of ten can: `1`, `20` and `300`
/// summed onto `4000` give `4321`, and any permutation or any misaligned load
/// gives a different number. The order the environment is written in is
/// `science-mir`'s `capture::captures_of` first-mention order, which is also
/// the order that crate gave the body's trailing parameters — this is the test
/// that the two halves agree.
#[test]
fn three_captures_are_read_back_in_the_order_they_were_written() {
    let lowered = lower(
        "def apply(f: (Int) -> Int, v: Int) -> Int:
    return f(v)

def main():
    let a be 1
    let b be 20
    let c be 300
    print(f\"{apply(x giving x + a + b + c, 4000)}\")
",
    );
    let dir = scratch("closures", "three_captures");
    require_runtime();
    let built = lowered.build_at(&executable(&dir, "three_captures"), OptLevel::O2);

    let ran = run(&built);
    assert_eq!(ran.stdout, "4321\n", "stderr was: {}", ran.stderr);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A capturing closure built and called inside a loop, and a capture reached
/// through a *second* closure.
///
/// The loop is `a_capture_free_closure_runs_a_hundred_thousand_times`'s reason
/// applied to an environment: a slot filled once and read many times passes a
/// single call and fails on repeat if the `alloca` is re-used or the store
/// lands after the load. The nested case is `capture::Walker`'s
/// `ExprKind::Closure` arm, run — the inner closure names `outer`, which
/// neither closure introduced, so both capture it and the environment is built
/// twice, once at each level.
#[test]
fn a_capture_survives_a_loop_and_a_second_closure_around_it() {
    let lowered = lower(
        "def apply(f: (Int) -> Int, v: Int) -> Int:
    return f(v)

def main():
    let one be 1
    let mutable total be 0
    for i in 0..5:
        total be total + apply(x giving x * one, i)
    print(f\"{total}\")

    let outer be 9
    print(f\"{apply(x giving apply(y giving y + outer, x), 1)}\")
",
    );
    let dir = scratch("closures", "loop_and_nested");
    require_runtime();
    let built = lowered.build_at(&executable(&dir, "loop_and_nested"), OptLevel::O2);

    let ran = run(&built);
    assert_eq!(ran.stdout, "10\n10\n", "stderr was: {}", ran.stderr);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The property the `{ code, env }` pair was chosen to keep, and the one a
/// single pointer into the creating frame would have cost.
///
/// A capture-free closure's value is `{ @fn, null }`: it refers to no storage,
/// so returning it out of `make` is returning two words and nothing else.
/// `science-regions` refuses the capturing version of this program — the
/// capture is a borrow and `SC0333` says so — and that refusal is exactly what
/// makes it safe for this one to be allowed rather than swept into the same
/// rule.
#[test]
fn a_capture_free_closure_outlives_the_function_that_made_it() {
    let lowered = lower(
        "def make() -> (Int) -> Int:
    return x giving x + 1

def main():
    let f be make()
    print(f\"{f(41)}\")
",
    );
    let dir = scratch("closures", "returned");
    require_runtime();
    let built = lowered.build_at(&executable(&dir, "returned"), OptLevel::O2);

    let ran = run(&built);
    assert_eq!(ran.stdout, "42\n", "stderr was: {}", ran.stderr);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    let _ = std::fs::remove_dir_all(&dir);
}

/// §8.2's named hole, closed as a refusal.
///
/// A closure body that *moves* a capture out is modelled by §8's discipline as
/// an exclusive borrow, because a borrow discipline has no spelling for a move
/// — and while no closure body was lowered at all, that mismatch was an
/// unreported mistake. Lowering the body would turn it into a running one: the
/// `String` would be copied into `take`, freed there, and freed again by the
/// frame that still owns `name`. So `science-mir`'s §8.6 withholds the body and
/// this is the refusal that reaches the author, produced by an actual build.
///
/// **This replaces `a_closure_that_captures_something_is_still_refused_by_name`,
/// which asserted that every capturing closure was refused.** That is no longer
/// true and the test could not be kept: the four above are the same build
/// succeeding. What is kept is its shape — a refusal named at the point the
/// value would be built, with `SC0400` and the construct in the message — now
/// narrowed to the one case that is genuinely not buildable.
#[test]
fn a_closure_that_moves_a_capture_out_is_refused_by_name() {
    let lowered = lower(
        "def take(s: String) -> Int:
    return s.length()

def apply(f: (Int) -> Int, v: Int) -> Int:
    return f(v)

def main():
    let name be \"world\"
    print(f\"{apply(x giving x + take(name), 0)}\")
",
    );
    let dir = scratch("closures", "moves_a_capture");
    let diagnostics = lowered
        .try_build(&dir.join("out"), OptLevel::O2)
        .map(|_| ())
        .expect_err("a closure that moves a capture out is not lowered");
    let first = diagnostics.first().expect("a diagnostic");
    assert_eq!(first.code, science_codegen::diagnostics::code::SC0400);
    assert!(
        first.message.contains("closure") && first.message.contains("moves"),
        "the refusal must name the construct and what is wrong with it, and it said: {}",
        first.message
    );
    let _ = std::fs::remove_dir_all(&dir);
}
