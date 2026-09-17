//! `f"…"` as MIR: §1.7's builder, and the two things §1.6 makes it easy to get
//! wrong.
//!
//! # What this file is for, and what it deliberately leaves to another crate
//!
//! The *sequence* — which entry point, in which order, with which literal — is
//! settled here, because it is this crate's decision and a test that ran the
//! program would only tell you that the whole of it was right. What it cannot
//! settle is whether the sequence **executes**: an argument in the wrong
//! register, a pointer read as a `ScienceString`, a `String` freed twice. Those
//! are `science-codegen-llvm`'s `tests/interpolation.rs`, which builds, links
//! and runs the same programs and asks what they printed, and that file's note
//! records the one that verified, linked, and died at run time.
//!
//! # The two mistakes this file exists to refuse
//!
//! 1. **A hole that moves.** §1.6 is a decision — *"an interpolation borrows
//!    its operands; `f"{doc}"` does not move `doc`"* — and it has a reason:
//!    *"a debugging `print` that moves the value you were about to use is a
//!    diagnostic in the `SC0300` range caused by a line the user added to
//!    understand a different problem"*. A `Move` of a `String` hole would put
//!    that diagnostic on a correct program, which is a wrong IR and worse than
//!    no IR. [`a_string_hole_is_borrowed_and_not_moved`] is the whole of it.
//! 2. **An accumulator borrow that is not the right kind, or is not one per
//!    call.** `lower`'s §6 makes an exclusive borrow in argument position
//!    two-phase; the accumulator's is in argument position and is used exactly
//!    once, at the call that consumes it. One borrow reused across the whole
//!    sequence would be an exclusive loan live over the evaluation of every
//!    hole, and a hole is an arbitrary expression.

mod support;

use science_mir::mir::{
    Body, BorrowKind, Callee, Constant, Local, Operand, Place, Rvalue, StatementKind,
    TerminatorKind, Unresolved,
};
use science_resolve::hir::{DefKind, Literal};
use support::{lower, Lowered};

/// Every call a body makes, as `(what it calls, what it was given)`.
///
/// A runtime call is named by its symbol, an unresolved one by its variant, so
/// that the assertion below reads as the sequence §1.7 describes rather than as
/// a match on an enum.
fn calls(body: &Body) -> Vec<String> {
    body.blocks()
        .filter_map(|(_, block)| match &block.terminator.kind {
            TerminatorKind::Call { callee, args, .. } => Some(match callee {
                Callee::Runtime(symbol) => match args.get(1) {
                    Some(Operand::Const(Constant::Literal(Literal::Str(text)))) => {
                        format!("{symbol}({text:?})")
                    }
                    _ => (*symbol).to_string(),
                },
                Callee::Def(_) => "call".to_string(),
                Callee::Indirect(_) => "indirect".to_string(),
                Callee::Unresolved(which) => format!("unresolved {which:?}"),
            }),
            _ => None,
        })
        .collect()
}

/// Every `science_string_*` symbol a body names, in order.
fn pushes(body: &Body) -> Vec<&'static str> {
    body.blocks()
        .filter_map(|(_, block)| match &block.terminator.kind {
            TerminatorKind::Call { callee: Callee::Runtime(symbol), .. } => Some(*symbol),
            _ => None,
        })
        .collect()
}

/// The local a binding was lowered to.
fn local_of(lowered: &Lowered, body: &Body, name: &str) -> Local {
    let def = lowered.def(name, DefKind::Local);
    body.local_of(def).unwrap_or_else(|| panic!("no local for `{name}`"))
}

/// Whether any operand anywhere moves out of this place.
fn moves(body: &Body, place: &Place) -> bool {
    let moved = |operand: &Operand| operand.moved_place() == Some(place);
    body.blocks().any(|(_, block)| {
        let in_terminator = match &block.terminator.kind {
            TerminatorKind::Call { args, .. } => args.iter().any(moved),
            TerminatorKind::If { cond, .. } => moved(cond),
            TerminatorKind::Switch { discr, .. } => moved(discr),
            _ => false,
        };
        in_terminator
            || block.statements.iter().any(|statement| match &statement.kind {
                StatementKind::Assign { rvalue: Rvalue::Use(operand), .. } => moved(operand),
                _ => false,
            })
    })
}

/// **The sequence, in full.** §1.7's *"builder over the fragments"*, with the
/// fragments in the order they were written.
///
/// One `science_string_with_capacity` for the accumulator, one
/// `science_string_push_bytes` per text run carrying the run itself, and one
/// `science_string_push_i64` for the `Int` hole — which is the shape
/// `science-codegen-llvm`'s `tests/formatting_boundary.rs` hand-built and ran
/// a pass before anything produced it.
#[test]
fn the_builder_is_one_call_per_fragment_in_source_order() {
    let lowered = lower("let n be 42\nprint(f\"n es {n}!\")\n");
    assert_eq!(
        calls(lowered.body("main")),
        vec![
            "science_string_with_capacity".to_string(),
            "science_string_push_bytes(\"n es \")".to_string(),
            "science_string_push_i64".to_string(),
            "science_string_push_bytes(\"!\")".to_string(),
            // `print` itself, which is a `Callee::Def` with no signature.
            "call".to_string(),
        ]
    );
}

/// **No hole is left in the IR.** The arm this replaced assigned
/// [`Rvalue::Error`], which is what every phase below read as *"the front end
/// gave up"* — including `science-regions`, whose `calls_a_hole` suppresses
/// every finding in a body that has one.
///
/// So this is not a cosmetic assertion: while the `Rvalue::Error` stood, a
/// program with an `f"…"` in it was **not region-checked at all**, and the
/// first thing this lowering buys is that it is.
#[test]
fn an_interpolation_leaves_no_rvalue_error_behind() {
    let lowered = lower("let n be 42\nlet s be \"t\"\nprint(f\"{n}{s}\")\n");
    let errors = lowered
        .statements("main")
        .into_iter()
        .filter(|statement| statement == "assign error")
        .count();
    assert_eq!(errors, 0, "an f-string still lowers to a hole");
    assert!(lowered.unresolved("main").is_empty(), "and names no unresolved callee");
}

/// **§1.6, as an assertion about operands.** The `String` hole is a shared
/// borrow of the binding's own local and nothing in the body moves it.
///
/// **The fixture has no second use of `s` on purpose.** A `print(s)` after the
/// interpolation would move `s` — `print` takes its argument by value, because
/// `builtins.rs` leaves it undeclared and there is no signature to auto-borrow
/// against — and this test would then be asserting something about `print`
/// rather than about the interpolation. That the second use *compiles and
/// runs* is the other half of §1.6 and it is asserted where it can be run, in
/// `science-codegen-llvm`'s `tests/interpolation.rs`.
#[test]
fn a_string_hole_is_borrowed_and_not_moved() {
    let lowered = lower("let s be \"uno\"\nprint(f\"{s}\")\nlet n be 1\n");
    let body = lowered.body("main");
    let s = Place::local(local_of(&lowered, body, "s"));

    let shared: Vec<&science_mir::BorrowData> =
        body.borrows().iter().filter(|data| data.place == s).collect();
    assert_eq!(shared.len(), 1, "the hole should be borrowed exactly once");
    assert_eq!(shared[0].kind, BorrowKind::Shared, "§1.6 asks for a read, not a write");

    assert!(
        !moves(body, &s),
        "the interpolation moved its hole: §1.6 says `f\"{{doc}}\"` does not move `doc`"
    );
    // And the binding is still this frame's to release, which is the other half
    // of the same fact.
    assert!(
        body.blocks().any(|(_, block)| matches!(
            &block.terminator.kind,
            TerminatorKind::Drop { place, .. } if *place == s
        )),
        "a `String` that was only borrowed is still dropped here"
    );
}

/// **A hole the runtime renders from a register is read, not borrowed.**
///
/// All six scalar entry points take their argument by value, so a loan would be
/// a reference at a parameter declared `i64` — which is
/// `tests/formatting_boundary.rs`'s subject one argument over. §1.6 is met
/// without one: an `Int` is trivially copyable, so `lower`'s §5 gives
/// [`Operand::Copy`] and the original is left behind by construction.
#[test]
fn a_scalar_hole_is_copied_rather_than_borrowed() {
    let lowered = lower("let n be 42\nprint(f\"{n}\")\n");
    let body = lowered.body("main");
    let n = Place::local(local_of(&lowered, body, "n"));
    assert!(
        body.borrows().iter().all(|data| data.place != n),
        "a scalar hole should need no loan"
    );
    let copied = body.blocks().any(|(_, block)| match &block.terminator.kind {
        TerminatorKind::Call { args, .. } => args.contains(&Operand::Copy(n.clone())),
        _ => false,
    });
    assert!(copied, "the `Int` hole should reach its entry point as a copy");
}

/// **One accumulator borrow per call, exclusive, two-phase, and activated.**
///
/// `lower`'s §6 is the rule and this is the measurement: four fragments, four
/// loans, all of the same place, every one with an activation point. A single
/// loan reused would be three fewer, and a loan with no activation would be an
/// exclusive borrow that rule 4 treats as reserved forever.
#[test]
fn the_accumulator_is_borrowed_once_per_call_and_every_borrow_activates() {
    let lowered = lower("let a be 1\nlet b be 2\nprint(f\"x{a}y{b}\")\n");
    let body = lowered.body("main");
    let accumulator: Vec<&science_mir::BorrowData> = body
        .borrows()
        .iter()
        .filter(|data| data.kind == BorrowKind::TwoPhase)
        .collect();
    assert_eq!(accumulator.len(), 4, "one per fragment: `x`, `{{a}}`, `y`, `{{b}}`");
    let place = accumulator[0].place.clone();
    for data in &accumulator {
        assert_eq!(data.place, place, "every push borrows the same accumulator");
        assert!(data.activation.is_some(), "a two-phase borrow with no activation");
        assert!(
            data.reserved <= data.activation.expect("checked"),
            "a borrow activated before it was reserved"
        );
    }
    // And the accumulator is the destination of the `science_string_with_capacity` that
    // opened the sequence, rather than some other local that happens to be a
    // `String`.
    let built = body.blocks().any(|(_, block)| {
        matches!(
            &block.terminator.kind,
            TerminatorKind::Call { callee: Callee::Runtime("science_string_with_capacity"), destination, .. }
                if *destination == place
        )
    });
    assert!(built, "the borrowed place is not the one `science_string_with_capacity` wrote");
}

/// **Each of the eight entry points is chosen by the hole's type.**
///
/// The seven pushes plus the `science_string_with_capacity` that opens every
/// sequence.
/// `science-codegen-llvm`'s `tests/interpolation.rs` runs the same program and
/// asserts what it printed; this asserts that the *choice* was made here, which
/// is the half a rendering test cannot distinguish from a runtime that guesses.
#[test]
fn the_entry_point_is_chosen_by_the_holes_type() {
    let source = "let i be 42\nlet u be 1u64\nlet d be 2.5\nlet f be 0.5f32\n\
                  let b be true\nlet c be 'x'\nlet s be \"t\"\n\
                  print(f\"{i}{u}{d}{f}{b}{c}{s}\")\n";
    assert_eq!(
        pushes(lower(source).body("main")),
        vec![
            "science_string_with_capacity",
            "science_string_push_i64",
            "science_string_push_u64",
            "science_string_push_f64",
            "science_string_push_f32",
            "science_string_push_bool",
            "science_string_push_char",
            "science_string_push_str",
        ]
    );
}

/// **A borrow is looked through rather than rendered.** A `borrowed String`
/// hole is a `String`, and a `borrowed Int` hole is an `Int`.
///
/// The dereference is §4's and it is load-bearing rather than tidy: without it
/// the operand handed to `science_string_push_str` is the address of this
/// frame's *parameter slot*, which is a pointer to a pointer, which the runtime
/// reads as a `{ ptr, len, cap }`. `tests/interpolation.rs`'s note is what that
/// does when you run it.
#[test]
fn a_borrowed_hole_renders_its_referent() {
    let source = "def f(s: borrowed String, n: borrowed Int) -> Int:\n\
                  \x20   print(f\"{s}{n}\")\n\x20   return 0\n";
    let lowered = lower(source);
    assert_eq!(
        pushes(lowered.body("f")),
        vec!["science_string_with_capacity", "science_string_push_str", "science_string_push_i64"]
    );
    // The `String` hole's loan names what the parameter points at, not the
    // parameter. `lib.rs` §5's sentence about a borrow naming the referent is
    // the rule; this is it holding for one more construct.
    let body = lowered.body("f");
    let shared: Vec<&science_mir::BorrowData> =
        body.borrows().iter().filter(|data| data.kind == BorrowKind::Shared).collect();
    assert_eq!(shared.len(), 1);
    assert!(
        !shared[0].place.is_local(),
        "the hole's loan names the parameter rather than the `String` it points at"
    );
}

/// **The three shapes a text run can have, and the empty literal.**
///
/// `f""` is one call and nothing else: the accumulator is still built, because
/// the expression has type `String` and something has to own one.
#[test]
fn the_edges_of_the_fragment_list() {
    let text_only = lower("print(f\"solo\")\n");
    assert_eq!(
        calls(text_only.body("main")),
        vec![
            "science_string_with_capacity".to_string(),
            "science_string_push_bytes(\"solo\")".to_string(),
            "call".to_string()
        ]
    );

    let leading = lower("let a be 1\nprint(f\"{a} fin\")\n");
    assert_eq!(
        pushes(leading.body("main")),
        vec!["science_string_with_capacity", "science_string_push_i64", "science_string_push_bytes"]
    );

    let trailing = lower("let a be 1\nprint(f\"ini {a}\")\n");
    assert_eq!(
        pushes(trailing.body("main")),
        vec!["science_string_with_capacity", "science_string_push_bytes", "science_string_push_i64"]
    );

    let adjacent = lower("let a be 1\nlet b be 2\nprint(f\"{a}{b}\")\n");
    assert_eq!(
        pushes(adjacent.body("main")),
        vec!["science_string_with_capacity", "science_string_push_i64", "science_string_push_i64"],
        "two adjacent holes have no text run between them"
    );

    let empty = lower("print(f\"\")\n");
    assert_eq!(pushes(empty.body("main")), vec!["science_string_with_capacity"]);
}

/// **Nothing here drops the accumulator, and the way it is released is
/// somebody else's.**
///
/// Bound, it is an ordinary local and `Builder::emit_scope_exit` drops it.
/// Passed to `print`, it is a temporary — and `print`'s signature is one this
/// crate cannot see, so `lower`'s §5 reads its argument rather than consuming
/// it and the *same* `emit_scope_exit` drops the temporary at the end of the
/// statement. A drop emitted from the f-string lowering itself would be a
/// second one either way.
///
/// **This assertion used to be the other way round and the change is the bug
/// it was covering.** It read *"passed to `print`, the accumulator is moved
/// and nothing drops it"*, because §5 called a signature-less callee's
/// argument a move and `science-codegen-llvm`'s `lower_print` then freed what
/// it printed. With one release per value that is consistent; it stops being
/// consistent the moment the value has a *name*, because `print(s)` freed a
/// binding the frame still owned. One release, and the scope is the one that
/// does it, is the rule that holds for both spellings.
#[test]
fn the_accumulator_is_dropped_by_the_scope_and_not_by_this_lowering() {
    let bound = lower("let s be f\"hola\"\nlet n be 1\n");
    let body = bound.body("main");
    let s = Place::local(local_of(&bound, body, "s"));
    let drops: Vec<&Place> = body
        .blocks()
        .filter_map(|(_, block)| match &block.terminator.kind {
            TerminatorKind::Drop { place, flag, .. } => {
                assert_eq!(*flag, None, "an f-string should need no drop flag");
                Some(place)
            }
            _ => None,
        })
        .collect();
    assert_eq!(drops, vec![&s], "a bound f-string is dropped exactly once, as itself");

    // Passed to `print`, the accumulator is read and the scope still drops it,
    // exactly once, as itself.
    let printed = lower("print(f\"hola\")\n");
    let body = printed.body("main");
    let drops: Vec<&Place> = body
        .blocks()
        .filter_map(|(_, block)| match &block.terminator.kind {
            TerminatorKind::Drop { place, flag, .. } => {
                assert_eq!(*flag, None, "an f-string should need no drop flag");
                Some(place)
            }
            _ => None,
        })
        .collect();
    assert_eq!(drops.len(), 1, "the accumulator is released once, by its scope");
    assert!(
        !moves(body, drops[0]),
        "`print` reads its argument; a move here is the free at the call site coming back"
    );
}

/// **A hole whose type has no entry point is a named hole, not a guess and not
/// an `Rvalue::Error`.**
///
/// A user record is §3.1's `Formatter`, which no note specifies and no prelude
/// declares, so it gets [`Unresolved::Display`] rather than a push. The
/// accumulator borrow is still taken, because it is right whatever the renderer
/// turns out to be — which is what lets `science-codegen-llvm` name the type in
/// its refusal instead of saying *"the front end gave up"*.
///
/// **This used to be `7i32`**, and it is not any more, which is the change §7
/// item 10 records: an `I32` hole is a cast and a `push_i64` now. The refusal
/// left is the one no cast can close, because what is missing is a *method* and
/// not a width.
#[test]
fn a_hole_with_no_entry_point_keeps_its_borrows_and_names_the_hole() {
    let lowered = lower(
        "type Punto:\n\x20   x: Int\n\nPunto implements Display\n\n\
         let p be Punto(x: 1)\nprint(f\"p={p}\")\n",
    );
    let body = lowered.body("main");
    assert_eq!(lowered.unresolved("main"), vec![Unresolved::Display]);
    assert_eq!(
        lowered.statements("main").iter().filter(|s| *s == "assign error").count(),
        0,
        "a missing renderer is a callee hole and not a value hole"
    );
    // Two accumulator borrows — the `n=` run and the refused hole — so the
    // loans region inference sees are the ones a working renderer would have
    // produced.
    assert_eq!(
        body.borrows().iter().filter(|data| data.kind == BorrowKind::TwoPhase).count(),
        2
    );
}

/// **A hole with no place gets a temporary, and the temporary is what is read.**
///
/// `f"{a + b}"` has nothing to borrow, so `Builder::operand` puts the sum in a
/// temporary that dies with the statement. Nothing about the sequence changes.
#[test]
fn a_computed_hole_goes_through_a_temporary() {
    let lowered = lower("let a be 1\nlet b be 2\nprint(f\"{a + b}\")\n");
    assert_eq!(
        pushes(lowered.body("main")),
        vec!["science_string_with_capacity", "science_string_push_i64"]
    );
    assert!(
        lowered.statements("main").iter().any(|statement| statement == "assign binary"),
        "the sum should be computed into a place of its own"
    );
}

/// **§1.7's capacity, as the number it is.**
///
/// `science-rt`'s `tests/capacity.rs` measures what this number buys — three
/// allocations become one for exactly this f-string — and names 57 as the
/// capacity it was measured at. This is the other end of that constant: the
/// compiler's own arithmetic, asserted here so that the two halves cannot drift
/// apart silently. `"n es "` is five bytes and `" y x es "` is eight; an `Int`
/// hole estimates twenty and an `F64` hole twenty-four.
///
/// The per-width estimates are asserted beside it, because the whole point of
/// giving `I8` four bytes rather than `I64`'s twenty is that an estimate is not
/// a bound and over-reserving is not recoverable — `String` has no
/// `shrink_to_fit`.
#[test]
fn the_capacity_is_the_fragments_plus_a_per_type_estimate() {
    let capacity = |source: &str| -> u64 {
        let lowered = lower(source);
        let body = lowered.body("main");
        let found = body
            .blocks()
            .find_map(|(_, block)| match &block.terminator.kind {
                TerminatorKind::Call {
                    callee: Callee::Runtime("science_string_with_capacity"),
                    args,
                    ..
                } => match args.first() {
                    Some(Operand::Const(Constant::Count(count))) => Some(*count),
                    other => panic!("the capacity is not a computed count: {other:?}"),
                },
                _ => None,
            })
            .expect("every f-string opens with `science_string_with_capacity`");
        found
    };

    // The acceptance case: 5 + 8 text, 20 for the `Int`, 24 for the `F64`.
    assert_eq!(
        capacity("let n be 42\nlet x be 0.5\nlet s be f\"n es {n} y x es {x}\"\n"),
        57,
        "`science-rt`'s tests/capacity.rs measured one allocation at this number"
    );
    // No holes: the literal's own bytes, and nothing else.
    assert_eq!(capacity("let s be f\"hola\"\n"), 4);
    // No fragments at all: nothing is reserved, which is `science_string_new`
    // exactly and allocates nothing.
    assert_eq!(capacity("let s be f\"\"\n"), 0);
    // Each width gets its own longest spelling rather than `I64`'s.
    assert_eq!(capacity("let n be 7i8\nlet s be f\"{n}\"\n"), 4, "`-128`");
    assert_eq!(capacity("let n be 7u8\nlet s be f\"{n}\"\n"), 3, "`255`");
    assert_eq!(capacity("let n be 7i32\nlet s be f\"{n}\"\n"), 11, "`-2147483648`");
    assert_eq!(capacity("let b be true\nlet s be f\"{b}\"\n"), 5, "`false`");
    assert_eq!(capacity("let c be 'x'\nlet s be f\"{c}\"\n"), 4, "one UTF-8 scalar");
    // A hole with no renderer contributes nothing: there will be no bytes.
    assert_eq!(
        capacity(
            "type Punto:\n\x20   x: Int\n\nPunto implements Display\n\n\
             let p be Punto(x: 1)\nlet s be f\"{p}\"\n"
        ),
        0
    );
}

/// **The six narrow integer widths reach their entry point through a cast**,
/// which is the sentence `science_string_push_i64` has always carried and which
/// §7 item 10 recorded as false.
///
/// The cast is asserted as a *statement* and the entry point as the call, so a
/// regression that widened by choosing `push_i64` and forgetting the cast — the
/// exact failure the refusal used to prevent — fails here rather than in
/// `LLVMVerifyModule`.
///
/// **The signed and unsigned lists are separate and that is the assertion that
/// matters.** `-1i32` extended as signed is `-1` and extended as unsigned is
/// `4294967295`; both are legal `i64`s, so nothing below this catches the
/// swap. `science-codegen-llvm`'s `tests/casts.rs` runs the values.
#[test]
fn a_narrow_integer_hole_is_cast_before_it_is_pushed() {
    for (suffix, entry) in [
        ("i8", "science_string_push_i64"),
        ("i16", "science_string_push_i64"),
        ("i32", "science_string_push_i64"),
        ("u8", "science_string_push_u64"),
        ("u16", "science_string_push_u64"),
        ("u32", "science_string_push_u64"),
    ] {
        let source = format!("let n be 7{suffix}\nlet s be f\"{{n}}\"\n");
        let lowered: Lowered = lower(&source);
        let body = lowered.body("main");
        assert_eq!(
            pushes(body),
            vec!["science_string_with_capacity", entry],
            "`{suffix}` chose the wrong entry point"
        );
        assert_eq!(
            lowered.statements("main").iter().filter(|s| *s == "assign cast").count(),
            1,
            "`{suffix}` reached `{entry}` with no cast in front of it, which is a register \
             the callee reads bytes of that nothing wrote"
        );
        assert!(
            lowered.unresolved("main").is_empty(),
            "`{suffix}` is still a hole with no renderer"
        );
    }

    // And the widths that are already the parameter's are not cast.
    for already in ["let n be 7\nlet s be f\"{n}\"\n", "let n be 7u64\nlet s be f\"{n}\"\n"] {
        let lowered = lower(already);
        assert_eq!(
            lowered.statements("main").iter().filter(|s| *s == "assign cast").count(),
            0,
            "an `I64`/`U64` hole needs no cast"
        );
    }
}
