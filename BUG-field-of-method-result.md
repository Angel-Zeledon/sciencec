# A field read off a method call's result does not lower

**Status: FIXED.** `crates/science-mir/src/lower.rs`, `Builder::as_place`, the
`ExprKind::Field` arm. Regression test:
`a_field_of_a_method_calls_result_is_read_off_a_temporary` in
`crates/science-mir/tests/places.rs`, beside the free-function sibling it was
named after.

Kept rather than deleted, because two of the three things learned here are
about *dead ends*, and a deleted document is an invitation to walk them again.
§3 is a real invariant that forbids the obvious fix. §4 is a measurement
artefact that cost a whole session and will recur on this machine.

## The program

```science
type V2:
    x: F64
    y: F64

V2 has:
    def scaled(self, k: F64) -> V2:
        V2(x: self.x * k, y: self.y * k)

def main():
    let a be V2(x: 1.0, y: 2.0)
    print(a.scaled(2.0).x)
```

`sciencec check` exited 0; `sciencec build` refused with `SC0400` — *"its MIR is
`Rvalue::Error`"*. It now builds, runs, prints `2.0` and exits 0.

`cargo test -p science-mir` is 130/0, `cargo test -p science-codegen-llvm
--features llvm --test arrays` is 15/15, and `cargo test --workspace --features
llvm` is **2232 passed, 0 failed** — the 2231 baseline plus the one test added
below.

## 1. What it was

`as_place` has a materialising arm for `ExprKind::Call { .. }` — a call's result
has no storage until something gives it some, so the arm gives it a temporary.
**There was no such arm for `ExprKind::MethodCall`**, so the `Field` arm's
`self.as_place(*base, block)?` returned `None`, `expr_into`'s
`Local | SelfValue | Field | Index` arm fell to its `None` branch, and the
expression became `Rvalue::Error`.

Binding the result first worked (`let r be a.scaled(2.0)` then `r.x`), and a
free function's result worked (`make(2.0).x`). It was exactly *field projection
whose base is an `ExprKind::MethodCall`*.

## 2. The fix

The `Field` arm now materialises a base that has no place of its own, instead of
giving up:

```rust
let (place, block) = match self.as_place(*base, block) {
    Some(found) => found,
    None => { /* temp of the base's type; expr_into; project from there */ }
};
```

Two properties are worth stating, because they are what make it safe.

**It is local.** `as_place` still answers `None` for a method call to every
*other* caller, which §3 shows is load-bearing.

**It cannot regress a program that compiles today.** Every route out of the `?`
it replaces ended in a refusal, not in a behaviour: `expr_into`'s
`Local | SelfValue | Field | Index` arm emits `Rvalue::Error`, and `value_hole`
and `borrow_source` both fall back to lowering the same `Field` node and reach
that same `Rvalue::Error`. `SC0400` is what the author saw. The only programs
this arm can change are the ones the backend refused.

It is also not confined to method calls, and deliberately so: `V2(x: 1.0, y:
2.0).x` and an `if`- or `match`-valued base are the same sentence, and were the
same refusal.

## 3. Why the arm above is *not* widened — a real invariant

Widening `as_place`'s materialising arm to
`ExprKind::Call { .. } | ExprKind::MethodCall { .. }` fixes the program and
breaks two tests in `crates/science-mir/tests/iteration.rs`:

```
a_subject_with_no_place_is_borrowed_through_a_temporary
the_next_call_reads_through_the_loops_reference
  assertion `left == right` failed
    left: Shared
   right: Exclusive
```

**Those tests are right and the widening is wrong.** A `None` from `as_place`
is not only *"there is no place"*; `borrow_source` reads it as *"this value is
the loop's own, so give it a temporary with a storage-dead point"*, and §4.4's
**shared** borrow of a `for` subject is taken only down the place path. Answer
`Some` for a method call and `for c in text.chars():` routes down that path, so
the `Chars` temporary the chain produced is borrowed *shared* — and then
`Iterate.next`, which is `def next(mutable self)`, is called through it. That is
a silent aliasing defect, not a snapshot that needs updating.

This is the reason the storage is given in the `Field` arm and nowhere else.

## 4. The "hang" in the previous session was a measurement artefact

A previous attempt recorded that materialising in the `Field` arm *"hangs the
array tests"* — `the_runtime_counts_what_the_literal_pushed` killed on
`harness::RUN_BUDGET`'s ten seconds — and guessed that the base was being
evaluated twice, so a `push` inside it never settled.

**The guess is wrong, and so is the attribution. Do not chase it.** Three
measurements, in increasing order of how conclusive they are:

1. With the fix in place, `cargo test -p science-codegen-llvm --features llvm
   --test arrays` is **15/15**, repeatedly.

2. The MIR that test's program lowers to is **byte-identical with and without
   the change** (dumped via `science_mir::dump::body` before and after; the only
   diff was cargo's own timing line).

3. The reason it is identical: `let xs be [10, 20, 30]` /
   `print(f"{xs.length()}")` **contains no field projection at all**. There is
   no `ExprKind::Field` in it, so the `Field` arm is never entered while
   lowering it. *No* variation of a change confined to that arm can affect that
   program. The three `science_array_push` calls the guess was about are the
   array literal's own, they are straight-line code in `bb1`–`bb3`, and nothing
   here touches them.

What actually happened is the machine. `RUN_BUDGET` is ten seconds of **wall
clock** around the spawned executable, and on macOS the *first* execution of a
freshly linked, unsigned binary blocks on a system-wide verification daemon
before `main` ever runs:

```
first run of a fresh binary:   real 0m0.529s   user 0m0.002s   sys 0m0.002s
same binary, second run:       real 0m0.004s   user 0m0.002s   sys 0m0.001s
```

Half a second of wall clock for four milliseconds of work — and it **serialises
system-wide**. Fifteen freshly built binaries launched simultaneously (11 cores,
otherwise idle) finish strictly one at a time, 0.335 s apart:

```
run  1: elapsed 0.399s      run  8: elapsed 3.054s
run  3: elapsed 0.733s      run 15: elapsed 3.394s
run  4: elapsed 1.065s      run  7: elapsed 3.732s
run  5: elapsed 1.393s      run 12: elapsed 4.065s
run  2: elapsed 1.728s      run 11: elapsed 4.394s
run  6: elapsed 2.047s      run 13: elapsed 4.723s
run  9: elapsed 2.373s      run 14: elapsed 5.085s
run 10: elapsed 2.714s
```

That is five seconds of the ten-second budget spent by the *last* test in the
queue, with nothing else competing. This checkout sits in one of **63
worktrees** under `.claude/worktrees` on an 11-core machine; every other one
that links a test executable joins the same queue. Ten seconds is comfortably reachable, and when it is
reached it kills whichever single test happens to be last — an arbitrary one, a
timeout rather than a wrong answer, and no relationship to the diff. Wall-clock
for the same `arrays` suite varied 6.1 s to 9.8 s across consecutive runs here.

**If a `RUN_BUDGET` timeout appears in one execution test and the MIR is
unchanged, suspect the queue before the compiler.** The cheap discriminator is
measurement 2 above: dump the MIR with and without the change and diff it.

## 5. Left open: the same gap in the `Index` arm

`as_place`'s `ExprKind::Index` arm still calls `self.as_place(*base, block)?`,
so an index whose *base* is a method call is the same bug, unfixed.
**Reproduced, not inferred** — this program exits 0 from `sciencec check` and is
refused by `sciencec build` with the same `SC0400`:

```science
type Holder:
    n: Int

Holder has:
    def items(self) -> Array[Int]:
        [1, 2, 3]

def main():
    let h be Holder(n: 0)
    print(f"{h.items()[0]}")
```

The same fallback would fix it and §2's argument would carry over unchanged.
It is named rather than done because indexing is where the bounds check and the
element borrow live, it wants its own test in `places.rs` and its own execution
test, and none of that was in this bug's scope.

Two things the fix does now handle, checked by running them: a record literal's
field (`V2(x: 3.0, y: 4.0).x` prints `3.0`) and a chain
(`a.scaled(2.0).scaled(3.0).y` prints `12.0`).
