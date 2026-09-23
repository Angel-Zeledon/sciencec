# A field read off a method call's result does not lower

**Status: FIXED**, and so is §5's `ExprKind::Index` half, which was left open
when the rest of this was written. `crates/science-mir/src/lower.rs`,
`Builder::as_place`, the `ExprKind::Field` and `ExprKind::Index` arms.
Regression test: `a_field_of_a_method_calls_result_is_read_off_a_temporary` in
`crates/science-mir/tests/places.rs`, beside the free-function sibling it was
named after, and three more for the index — see §5.

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

`cargo test -p science-mir` was 130/0, `cargo test -p science-codegen-llvm
--features llvm --test arrays` 15/15, and `cargo test --workspace --features
llvm` **2232 passed, 0 failed** — the 2231 baseline plus the one test added
below. Those are that commit's numbers; §5 re-measures after the index half.

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

## 5. The same gap in the `Index` arm — **closed**

`as_place`'s `ExprKind::Index` arm called `self.as_place(*base, block)?` too, so
an index whose *base* had no place of its own was the same bug. The program that
reproduced it exited 0 from `sciencec check` and was refused by `sciencec build`
with the same `SC0400`:

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

It now builds, prints `1` and exits 0. The arm takes §2's fallback verbatim.

`cargo test -p science-mir` is **133/0** (130 plus the three below),
`cargo test -p science-codegen-llvm --features llvm --test arrays` is **17/17**
(15 plus the two below), and `cargo test --workspace --features llvm` is **2241
passed, 0 failed** — 2236 measured on this tree with the change reverted, plus
the five tests added.

### 5.1 §2's second property does **not** carry over unchanged

§2 claims the `Field` arm *"cannot regress a program that compiles today"*,
because every route out of the `?` it replaced ended in a refusal. That was
checked again for `Index` rather than assumed, and it is **not** true. There are
seven callers of `as_place`; six of them turn a `None` on an `ExprKind::Index`
into `expr_into`'s `Rvalue::Error` and an `SC0400`, directly or through one
hop — `expr_into` itself, `operand`'s fallback, `value_hole` (which delegates to
`operand`), `borrow_source`, `lower_match`'s scrutinee, and `lower_for`'s
subject, which reaches `borrow_source`. The seventh does not.

`StmtKind::Assign`'s target takes `None` to mean *"the resolver and the checker
both already reported this"* and **drops the target on the floor**, evaluating
only the value into a discarded temporary. But `sciencec check` accepts
`h.items()[0] be 5`, so nothing reported it. Measured, before the change:

```
$ sciencec check assign.science  → 0
$ sciencec build assign.science  → 0
$ ./assign                       → "done", exit 0
```

A program that builds, runs, and silently does nothing. After the change the
target is a real place — the element of the temporary the base was materialised
into — so the base is evaluated, `bounds_check` runs over it, and the write
lands in storage that dies at the end of the statement. Still no lasting effect,
because a method's returned array *is* a temporary and there is nothing else the
sentence could mean, and `./assign` still prints `done` and exits 0. The one
observable difference is that an out-of-range index in that position now panics
where it used to be discarded:

```
$ ./assign_oob   # h.items()[i] be 5, i = 99
panic: index out of bounds       (was: "done", exit 0)
```

**That is the right way round** — an index that is evaluated is bounds-checked,
everywhere — but it is a behaviour change on a program that compiles today, and
§2's *"the only programs this arm can change are the ones the backend refuses"*
is a sentence about the `Field` arm and not about this one. Whether the front
end should accept an index into a temporary as an assignment target at all is a
separate question, and is left open.

### 5.2 What the bounds check and the element borrow actually do here

This was split out on the ground that indexing is where those two live. They
were measured, not assumed. The MIR of `def f(h: Holder) -> Int: h.items()[1]`:

```
    borrow 1: shared of _2,      reserved bb1[3]     # the length borrow
    borrow 2: shared of _2[_4],  reserved bb3[1]     # the element borrow
  bb0:  _2 = items(move _3) -> bb1                   # the materialised base
  bb1:  _6 = runtime science_array_len(move _5) -> bb2
  bb2:  if copy _9 then bb3 else bb4
  bb3:  _11 = borrowed _2[_4] (borrow 2)  …  drop _2 -> bb5
  bb4:  _10 = runtime science_panic_bytes("index out of bounds") -> diverges
```

Both name `_2`, the temporary the call's result went into, and both sit inside
its storage-live range. `bounds_check`'s `is_array` is asked of that place and
answers yes, so the guard is emitted for this path exactly as for `xs[i]`; it
fires at runtime at both ends, which `arrays.rs` runs.

**One consequence worth stating, because it looks like a new refusal and is
not.** Reading an element goes through `read_ergonomic`, which builds a real
`&Int` at the element's address — so `let one be h.items()[1]` binds a reference
into an array that dies at the close of the statement that built it, and
`science-regions` now reports `SC0333`, *"this expression is borrowed for longer
than this expression exists"*. Before the change the same line was `SC0400`.
Both are refusals; the new one is rule 5 telling the truth about the lowering
rather than the backend reporting a hole. `print(f"{h.items()[1]}")`,
`h.items()[0] + h.items()[2]` and `h.names()[0].length()` all read the element
within the statement and all run.

### 5.3 The tests

`crates/science-mir/tests/places.rs`, beside the `Field` pair:

- `an_index_of_a_method_calls_result_is_read_off_a_temporary`
- `an_index_of_a_method_calls_result_is_bounds_checked`
- `the_element_borrow_of_an_indexed_method_call_names_the_temporary`

All three were run against the unfixed arm and all three fail on it, with the
hole itself as the message:

```
assertion `left == right` failed: expected one element borrow: fn f:
  bb0:
    StorageLive(_2)
    _2 = {error}
```

`crates/science-codegen-llvm/tests/arrays.rs`, which builds, links and runs:

- `an_index_of_a_method_calls_result_builds_and_runs` — three ascending indices,
  a `String` element, and an arithmetic use, all off a method call's result
- `an_index_out_of_bounds_panics_on_a_method_calls_result` — both ends

§3's invariant is untouched and stays untouched: the storage is still given in
the two arms that need it and the `ExprKind::Call` arm is still not widened, so
`iteration.rs`'s two loop-borrow tests are green without being looked at.

Two things the fix does now handle, checked by running them: a record literal's
field (`V2(x: 3.0, y: 4.0).x` prints `3.0`) and a chain
(`a.scaled(2.0).scaled(3.0).y` prints `12.0`).
