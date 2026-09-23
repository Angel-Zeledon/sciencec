# A field read off a method call's result does not lower

**Status:** open, reproduced, located. One obvious fix tried and **rejected
with evidence** — read §3 before attempting it again.

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

`sciencec check` exits **0**. `sciencec build` refuses:

> error[SC0400]: this `sciencec` cannot build an expression the front end
> replaced with a hole: its MIR is `Rvalue::Error`, and if `sciencec check`
> passed the same program then nothing was wrong with it and the gap is in a
> lowering rather than in the source

That refusal is right about where to look.

## 1. What it is, and what it is not

Binding the result first **works**, and that is the whole shape of the bug:

```science
let r be a.scaled(2.0)
print(r.x)                  # fine
```

A **free function**'s result also works: `make(2.0).x` lowers correctly. It is
specific to a **method** call.

Ruled out by minimal programs, each checked individually: it is not `let` in a
method body, not `F64`, not returning a record, not operators, not `Add`. It is
exactly *field projection whose base is an `ExprKind::MethodCall`*.

## 2. Where it is

`crates/science-mir/src/lower.rs`, `Builder::as_place`.

`as_place` has a materialising arm for `ExprKind::Call { .. }` — it gives the
call's result a temporary, because a call is the one expression that has no
storage of its own. The arm carries a long comment explaining precisely that.

**There is no such arm for `ExprKind::MethodCall`.** So the `Field` arm's
`self.as_place(*base, block)?` returns `None`, `expr_into`'s
`Local | SelfValue | Field | Index` arm falls to its `None` branch, and the
expression becomes `Rvalue::Error`.

## 3. The two fixes that do not work, and why

Both were implemented and run. Do not repeat them without new information.

**(a) Widen the arm** to `ExprKind::Call { .. } | ExprKind::MethodCall { .. }`.

Fixes the program. Breaks two tests in `crates/science-mir/tests/iteration.rs`:
`a_subject_with_no_place_is_borrowed_through_a_temporary` and
`the_next_call_reads_through_the_loops_reference`.

`for c in text.chars():` **depends on a method call having no place**, so that
§7.1's `borrow_source` takes the subject through a temporary with a
storage-dead point, which rule 5 needs. Giving a method call a place routes the
loop subject down the place path instead.

**(b) Materialise only in the `Field` arm**, leaving general `as_place` alone.

Fixes the program, keeps `science-mir` green at **130/0** — and **hangs the
array tests**. `cargo test -p science-codegen-llvm --features llvm --test
arrays` is 15/15 on a pristine tree and, with this change, kills
`the_runtime_counts_what_the_literal_pushed` on the harness's 10-second
budget. A hang, not a wrong answer.

That hang is unexplained and is the thing to understand first. The guess worth
checking is that the base gets evaluated twice — once into the temporary and
once by whatever the array path does with it — so a `push` inside the base runs
in a loop that never settles. **Verify it before believing it.**

## 4. How to know you have fixed it

1. The program at the top builds, runs, prints `2.0`, exits 0.
2. `cargo test -p science-mir` green.
3. `cargo test -p science-codegen-llvm --features llvm --test arrays` green —
   this is the one both attempts failed.
4. Full corpus still 17 of 22, and
   `cargo test --workspace --features llvm -- --test-threads=4` still 2229.

`crates/science-mir/tests/places.rs` already holds
`a_field_of_a_calls_result_is_read_off_a_temporary`, the free-function sibling
of the missing case. The regression test for this one belongs beside it.
