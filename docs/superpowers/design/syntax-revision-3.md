# Science — Design: syntax revision 3

Date: 2026-09-16
Status: decided and shipped
Supersedes, in part: `docs/superpowers/specs/2026-09-16-science-f0-core-design.md`
§4.1 (the keyword rule), §4.3 (the Function row and the reserved-word note under
it), §13 (both lists).
Reverses: `def-and-lambda.md` §3.4, which decided the opposite this morning.
Related: `self-hosting.md` Decision 6 and gate E, which this is the first test
of; `syntax-revision-2.md` §10, which set the bar; `reserved-words.md` §2.3, for
the collision; `llm-ergonomics.md` §3.2, whose migration table gains a row.
Diagnostic block: `SC0156`, shipped in `crates/science-parser/src/parser.rs`.

---

## 0. The change

One keyword, and nothing else.

> **Decision. The function declaration is `def`. `function` is not a keyword and
> is not reserved — it is an ordinary identifier, and a program may bind it.**

```science
def summarize(self) -> String:
    self.body.truncate(200)
```

Everything else in the language is untouched. No node changed shape, no rule
about blocks or borrows or errors moved, and the diff is a token rename plus one
diagnostic. That is why this note is short, and it is the only thing about the
change that is small.

**This is a one-for-one trade on §13's in-use list** — one word out, one word in,
net zero. Revision 2 traded nine for two and could point at the arithmetic as its
justification. This one has no arithmetic to point at; what it changed is not the
size of the list but the rule that governed what could go on it, and that is §2.

---

## 1. How the decision was made, stated plainly

The project owner asked for `def`. They were told it had been decided against,
with the note and the reasons. They said to make the change anyway. That is
their call, it was made, and it is made.

This section exists because the alternative — quietly restating the note as
though `def` had won the argument — is the thing that makes a design record
useless a month later. `def-and-lambda.md` has been rewritten to record the
argument it made, the fact that the argument lost, and what losing it did to the
rule the argument rested on. Nothing in it has been deleted, and §3.2 of that
note, the six-to-one audience count, is still the best evidence anyone has
produced on the question. It was not refuted. It was overruled.

The distinction matters for the next time. An argument that is refuted stops
being available. An argument that is overruled is still available, and the next
person who wants a keyword shortened should expect to meet it again and should
expect it to lose again the same way, unless they have something better than the
last request had.

---

## 2. What it did to the keyword rule

The core spec's §4.1 used to say **no keyword is an abbreviation**, and
`def-and-lambda.md` §1.2 had just restated it as the one clause of §4.1 that
revision 2 left standing. `def` is a word with four letters removed. It breaks
the clause on its face.

§1.3 of that note had also already answered the obvious dodge, before there was
anything to dodge. `const` was one exception; one exception is an exception and
two is a pattern; *whole words, plus a universal symbol, plus `const`, plus
`def`* is not a rule, it is a list, and a list cannot answer a question it does
not already contain.

There are now two. The rule as a prohibition is over, and pretending otherwise
would be the second-worst outcome available. §4.1 of the spec has been rewritten
and this is what it now says:

1. **A whole English word is the default**, and the symbol category revision 2
   added is unchanged: `->` and `>=` stay, because neither is a shortened word.
2. **A shortening is not forbidden. It must beat the audience test**: count the
   spelling across the languages this audience actually writes, which is the
   method `def-and-lambda.md` §3.2 worked out. `elif`, `impl`, `str`, `len`,
   `mut`, `pub` and `fn` all fail it, and each takes one sentence to refuse.
3. **A shortening adopted without beating the test is named in a register**, with
   who decided it and against what. Two entries: `const`, inherited and never
   argued, and `def`, argued and overruled.

**Why this and not the other two options.** *Declare the rule dead and decide by
taste* is what §1.3 refused, and its reason still holds: taste does not scale
past the first disagreement. *Keep the prohibition with an exception list* is the
thing §1.3 proved is not a rule. What is left is the only version that is both
weaker than the old rule and actually true of the language as it stands.

**The cost, stated, and it is the real one.** The old clause could be checked by
looking at the word: *is this shortened?* The audience test cannot — it is
empirical, so it can be argued with, and an argument can be overruled, which is
exactly what happened here. So §4.1 has stopped being a veto and become an
obligation to record. A reviewer meeting the next `def`-shaped request has a
method and a precedent and no authority. `def-and-lambda.md` §1.3 predicted that
the day the rule gained a second exception it would stop deciding anything, and
that prediction is not wrong; the restatement in §4.1 is an attempt to salvage a
predicate from it, not a claim that nothing was lost.

**What is bought back.** The register is written down rather than silent, which
is precisely what §1.4 of that note asked for as its fallback — *then §4.1 should
say so in writing, naming `const` as the one deliberate exception, so that the
next `def`-shaped request meets a stated exception list rather than a silent
one.* The ask is discharged, by a route its author did not want.

---

## 3. The cost, counted

From the commit that made the change:

| | Count |
|---|---|
| Declarations rewritten in `examples/` and fixtures | **384**, across 40 files |
| Declarations embedded in Rust test sources | **264** |
| Snapshots re-blessed | **116** |
| New diagnostic | 1 (`SC0156`) |
| Tests passing after | 736 |

The 264 is the number worth looking at. Those are Science programs written as
string literals inside `.rs` files, where no Science tool can reach them: not
the formatter, not a migration tool, not the parser's own fixtures mechanism.
They were rewritten by hand-equivalent means and they will be rewritten again by
the same means at every future revision. That is a structural cost of testing a
language from inside its own compiler's test suite, it was invisible until a
revision made it visible, and it is the largest single item here.

**One collision, found by breaking a file.**
`examples/21_compiler_shapes.science` used `def` as a **field name** — `def:
DefId`, `binding.def` — because it models the real structures of a compiler.
`def` is ordinary compiler vocabulary and Science intends to compile itself. The
field is `definition` now, the file says why, and `reserved-words.md` §2.3
records the collision and the general finding under it: §13's lists have never
been audited against the vocabulary of a compiler, only against the vocabulary of
a scientist.

**`function` was freed rather than held reserved.** Holding it would have been
the other way to keep `SC0156` working. It was not taken: the parser recognises
the stale declaration positionally — the ordinary word `function` followed by a
name — which costs one token of lookahead, and a reservation would have cost an
identifier that `ffi-c-boundary.md` §4.3 already binds, because GSL's
`gsl_function` has a field called `function`. This is the same trade
`reserved-words.md` §0.1 makes for the dot rule and it goes the same way.

---

## 4. What made it cheap, and whether that is vindication or luck

`self-hosting.md` Decision 6 refused to gate self-hosting on syntax *stability*,
on the ground that "the syntax is settled" is unfalsifiable, and gated it on
**mechanisability** instead. Gate E states the condition:

> - **E1.** `sciencec fmt` exists and is idempotent.
> - **E2.** A migration tool replays the revision-1-to-2 migration from git
>   history and reproduces the committed corpus byte for byte.

`sciencec fmt` shipped hours before this revision was asked for. **E1 was met
when the question arrived, by accident of ordering**, and it is the whole reason
a third corpus migration in one day was an afternoon rather than an argument
about whether to do it at all.

**The gate is vindicated as a criterion.** Decision 6's claim was that the useful
question is not *when is the syntax done* but *how expensive is a migration*, and
that converting the scheduling question into a piece of engineering would pay.
That claim was tested within hours of being written and it held: the cost in §3
is a large number of small mechanical edits and zero design work, which is what
"mechanical" was supposed to mean.

**The ordering is luck, and it should be recorded as luck.** Had the formatter
landed after the request instead of before it, revision 3 would have been done by
hand for the third time in a day and gate E would have been written afterwards,
where it would read as a rationalisation of work already done rather than as a
condition that was met. Nothing about the decision depended on the gate. The
owner did not ask whether E1 was met, and the change would have been made if it
had not been. **Gate E made this revision cheap. It did not make it possible, and
it could not have made it impossible.** A gate that does not gate is a
cost-estimate, and it is worth having, but it should not be mistaken for a
control.

**E2 is still not met**, and this revision did not test it. There is no migration
tool in `crates/`; the revision-1-to-2 replay has never been built, and this
revision was not replayable either. So gate E is half-met, the half that was met
is the half that happened to be useful today, and `self-hosting.md`'s Decision 6
should not be marked discharged.

---

## 5. Revision 2 §10 set a bar. Did this clear it?

The sentence was:

> the language has no users yet and this is the cheapest day it will ever be —
> but it is the last day that argument is free, and a third revision should be
> held to a much higher bar.

**On the merits, no.** The bar was never named as a number or a test, and what
arrived was a request from the project owner against a note that had just
argued the other way. If the bar meant "the case must be stronger than revision
2's cases were", this case is weaker than any of the eight: revision 2's changes
each followed from a stated rule, and this one breaks the rule that was left.

**On the cost, the bar moved.** §10's argument for the bar was that the corpus
moves every time and the argument that migration is free expires. What actually
happened is that migration got cheaper between the two revisions, by §4, so the
premise the bar rested on weakened rather than the case strengthening.

Both of those are worth writing down because they point the same way for
revision 4: **the thing that will be scarce is not migration effort, it is the
ability to say no.** §2 is the honest account of what is left to say it with.

---

## 6. Diagnostics

**`SC0156`, shipped.** `function` followed by a name, anywhere a declaration may
start. The fix is machine-applicable — a word for a word, nothing around it
moves — and the parser consumes the word and continues as though `def` had been
written, so a stale file produces one error per declaration rather than a
cascade. That is the same recovery discipline as `SC0138`–`SC0144` and it shares
their code path.

```
error[SC0156]: the declaration is written `def`
  --> model.science:4:1
   |
 4 | function summarize(self) -> String:
   | ^^^^^^^^ `function` is not a keyword in Science
   |
   = note: `def` declares every function, method and interface member
help: write the declaration as
   |
 4 | def summarize(self) -> String:
   | ~~~
```

**Two codes go back to the free pool.** `def-and-lambda.md` allocated `SC0119`
for `def` starting an item — void, since `def` is now correct — and `SC0136` as
the contingency for exactly this reversal. `SC0136` was never used: the
implementation took `SC0156`, the next free code in the syntax block, which is
also where `SC0155` went for the `try` migration. The compiler is the record, so
`SC0156` is the live code and both of the pre-allocated ones are returned.
`docs/superpowers/design/README.md` carries the change.

**`llm-ergonomics.md` §3.2's `SC0127` row** — `fn` starting an item — now
suggests `def` rather than `function`. The two spellings are three characters
apart, which is a small mercy for the migration and a small worry for the
diagnostic: `fn` and `def` are the two most likely things a model writes, they
now produce different codes with nearly identical fixes, and if that turns out to
confuse rather than help, merging them is a one-line change.

---

## 7. What this asks of other notes

1. **`def-and-lambda.md`** — rewritten in this pass. It records the decision, the
   argument it made against it, and the restated rule. Its `lambda` half is
   untouched and still stands.
2. **`reserved-words.md` §2.3** — added in this pass: the `def` collision, the
   `function` row, and a seventh ask, that §13 be audited against the compiler's
   own vocabulary rather than only the scientist's.
3. **`self-hosting.md`** — gate E should be annotated: E1 met and exercised, E2
   not built. Decision 6 is confirmed as a criterion and not discharged as a
   gate.
4. **`llm-ergonomics.md` §3.2** — the `SC0127` row's fix column, done in this
   pass. Whether `SC0127` and `SC0156` should merge is that note's question.
5. **The core spec** — §4.1, §4.3 and §13, done in this pass.
6. **Whoever owns the closure-type ask** — `function(T) -> U` was spelled that
   way because, in `collections-and-chains.md` §1.2's words, it is *"formed exactly
   as a declaration is"*. It is `def(T) -> U` now, mechanically, because the
   declaration moved. The justification survives the rename and the reading does
   not: `def(F64) -> F64` in a parameter's type annotation is a keyword doing a
   job it has no English claim to, in a position where `function` read as a noun.
   Nobody owns that ask — README's standing-asks table records three customers
   and no owner — so it is flagged here rather than decided.

   > **Taken and decided.** `collections-and-chains.md` §1.2 owns it now and the
   > spelling is **`(A) -> B`, with no keyword**. The flag above is what carried:
   > the reading did not survive the rename, so the ask was re-opened rather than
   > inherited. The answer is that the job `function` was doing — *no new
   > keyword* — never needed a keyword to do it, and the one symbol involved is
   > the `->` §4 of revision 2 already admitted. The decision therefore does not
   > test §4.1's weakened rule at all, which is the most that could be asked of a
   > note deciding a syntax question the day after that rule was overruled. The
   > ambiguity with the tuple type is one token of lookahead past the closing
   > paren; §1.2 works it against `parse_type` and states the grammar change.
