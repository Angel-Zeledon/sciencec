# Science — Design: `with` as a policy block

Date: 2026-09-18
Status: draft for review
Owns: the **policy clause** of a `with` head — `seed`, `device`, `precision`,
`unchecked` — the lexical scoping rule over it, the rule that a policy is
consumed explicitly rather than read ambiently, and the provenance obligation.
Extends, and does not re-decide: `broadcasting.md` §6.2, which owns the `with`
construct, took the reserved word, and defined the **shape clause**
(`with table.shape as (rows, 768)`). §6.3's refutability rule and the `else`
branch are that note's and are unchanged here. **This note adds a second clause
kind to a head that is already a list, and nothing else.**
Depends on: `reproducibility.md` §4 (`--deterministic`, the allow-list) and §5.2
(the run record), `effects.md` §3.1 (the `ambient` bit, and why §2 below is not
a fourth bit), `stdlib-standard.md` §5 (`random`, `Key`, the counter-based
generator), `scientific-libraries.md` §12 (units), `contracts.md` §7 (the
profiles, whose `unchecked` this scopes), `codegen-and-linking.md` (float
control), `property-testing.md` §6 (the seed field this shares),
`durable-computation.md` §4.
Syntax: `syntax-revision-2.md` and `syntax-revision-3.md`.
Diagnostics claimed: **`SC0178`–`SC0179`** (syntax), **`SC0241`–`SC0245`**
(resolution) and **`SC0660`–`SC0669`** (types).

---

## 0. The verdict up front

Four things are configured once at the top of a scientific program and used
everywhere inside it: the random seed, the device, the float precision, and
whether expensive checks run. Today each is passed by hand through every call, or
set in a global, and the second is how every one of them eventually goes wrong.

Python's answer is the context manager — `with torch.no_grad():`,
`with np.errstate(...)`, `torch.set_default_device(...)` — and it is genuinely
the right shape. Its failure is that it works by mutating hidden global state, so
a function called inside the block cannot be told from one called outside it, a
library can stash the value and use it later, and nothing about the block is
visible in a type or in an artefact.

> **Decision 1. A `with` head may carry policy clauses beside
> `broadcasting.md` §6.2's shape clauses. A policy is a value bound in the
> block's scope, not a global that is mutated; the compiler requires that
> anything needing it takes it from there; and the block is recorded in the
> provenance artefact.**

```science
with seed 42, device gpu:0, precision f32:
    let model be fit(data)
```

## 1. Why this extends `with` rather than inventing a construct

`broadcasting.md` §6.2 already spent the reserved word, and its head is already a
comma-separated list of clauses — *"several operands bind in one head, and a
repeated binder is a runtime equality check — which is the whole reason the
construct takes a list"*. So the grammar has a slot.

More than convenience, the two clause kinds are the same idea. §6.2's clause
takes something known only at runtime (a shape) and makes it a rigid, checked
fact for the extent of a block. A policy clause takes something chosen by the
author and does the same. Both are *"this is fixed here, and everything inside
may rely on it"*, and a language with two constructs for that has one construct
too many.

```science
with table.shape as [sample: rows, feature: 768], seed 42, precision f32:
    ...
```

**What this note must not break**, and does not: §6.3's rule that `else` is
required exactly when a clause is refutable. **A policy clause is never
refutable** — its value is written in the source — so it never requires an
`else` and never forbids one that a shape clause in the same head required.
Refutability is computed per clause and or-ed, which is what §6.3 already does
for a list.

## 2. Decision 2 — a policy is bound, not ambient, and this is the whole design

> **Decision 2. Each policy clause introduces a binding in the block's scope.
> A function that needs a policy declares a parameter for it. `with` does not
> change what any function reads; it changes what is in scope to pass.**

This is the sentence that separates the construct from Python's, and it is worth
being precise about because the obvious reading of §0's example is the wrong one.
`with seed 42:` does **not** make `random()` inside the block return a seeded
stream. `effects.md` §3.1 makes reading machine-supplied state the `ambient` bit,
and a construct that silently redirects what a callee reads would be ambient
state with better syntax — the exact thing `reproducibility.md` §4 was written to
detect and refuse.

What happens instead: `seed 42` binds a `Key` (`stdlib-standard.md` §5) named
`seed` in the block. A function that needs randomness takes a `key: borrowed Key`
parameter, as it must today, and the call site writes it. The gain is that the
value is written *once* per block instead of once per call, that it cannot be
misspelled or shadowed by a stale one, and that the block is a syntactic region a
tool can point at.

### 2.1 The one concession: implicit forwarding by name

Threading `key` through eleven call sites is exactly the tedium the construct
exists to remove, so:

> **Decision 3. Inside a `with` block, a parameter whose name and type match a
> policy binding may be **omitted at the call site** and is filled from the
> block. Omission is only legal inside a block that binds it; outside, the
> argument is mandatory as it is today.**

```science
with seed 42:
    let a be bootstrap(data, 1000)        # `key` filled from the block
    let b be bootstrap(data, 1000, key: other_key)   # explicit wins
```

This is a call-site elaboration, decided at type-check time, and it is visible in
the signature: `bootstrap` still declares `key: borrowed Key` and still receives
it. The effect bits are unchanged, the function is unchanged, and
`sciencec fmt --explicit` can write every omission back out, which is the
mitigation for the readability objection.

**The risk, named:** this is the one place in the note where a reader cannot see
where a value came from by looking at the line. It is bounded — the value is
always in a `with` head lexically above, in the same function — and it is the
minimum concession that makes the construct worth using. A design with no
implicit forwarding is just a `let`, and a design where callees read ambiently is
Python's.

## 3. The four policies

### 3.1 `seed`

Binds a `Key`. `with seed 42` is a literal; `with seed key` forwards an existing
one. Nesting derives a child key by `Key.derive`, so an inner block is
independent of the outer and both are reproducible — the property
`stdlib-standard.md` §5 built the counter-based generator for.

### 3.2 `device`

Binds a `Device`. Allocations of device memory inside the block default to it
under the same name-matching rule as §2.1. This is the one policy with an
ownership dimension: §1 of the core spec claims *ownership-tracked device
memory*, and a `with device` block is where the allocation site is decided.
Crossing devices inside one expression is `SC0660`, and the message names both.

`effects.md` §6.1 decided device allocation is deliberately **not** an effect.
Nothing here contradicts that: the device is a value, the policy is a binding,
and no bit is added. The closed set stays closed.

### 3.3 `precision`

Binds the default float width for *literals and inference* inside the block —
`f32`, `f64`, or `mixed`. It does **not** silently narrow existing values: a
`F64` that flows into the block stays `F64`, and `precision` only decides what an
unannotated `1.5` becomes and what an unconstrained type variable defaults to.

A policy that rewrote the width of values crossing into the block would be the
worst feature in the language — it is `reproducibility.md` §3.2's float channel
opened deliberately — and `SC0661` fires when a block's precision would change
the type of a value bound outside it.

### 3.4 `unchecked`

Scopes `contracts.md` §7's third profile to a block, so an inner loop can drop
its checks without the whole binary doing so. This is the narrowest and most
defensible form of that escape: it is lexical, it is greppable, and it appears in
the artefact of §5. `unchecked` inside a `with` head does **not** enable
`unsafe:` — `ffi-c-boundary.md` §3 owns that word and its powers are a closed
list this note does not touch.

### 3.5 The set is closed

> **Decision 4. Four policies. Adding a fifth is a spec change.**

This is `effects.md` §3.1's closed-set sentence applied to a construct with the
same growth risk. A `with` head that accepts arbitrary key-value pairs becomes a
configuration language inside the language, and every note that wants a knob puts
one there.

## 4. Decision 5 — a policy that is declared and never consumed is an error

> **Decision 5. A policy binding that no call in the block consumes is
> `SC0241`, an error and not a warning.**

```science
with seed 42:
    let m be fit(data)        # SC0241: `fit` takes no `key`
```

The author of that block believes their fit is seeded. It is not, and the result
is irreproducible in the specific way that looks reproducible — the run record
would show a seed that did nothing. An unused variable is a warning everywhere
else in this language and this one is an error, because the cost of the two
mistakes is not comparable.

## 5. Decision 6 — the block is in the run record

> **Decision 6. Every `with` policy block that encloses a value-producing call is
> recorded in `reproducibility.md` §5.2's run record, as an ordered list of
> `{clause, value, span}` objects. Values are strings and integers only.**

This is the payoff and it is why the construct earns its place over a plain
`let`. A reviewer holding a figure can ask what seed produced it, on which
device, at which precision, with checks on or off — and get an answer from the
artefact rather than from the source. `reproducibility.md` §5.4's three-cost
reviewer workflow gains its most requested field for free, and Decision 10's
no-floats rule holds because a precision is a name and a seed is an integer.

Interaction with `--deterministic` (`reproducibility.md` §4): a `with seed`
block is exactly the evidence the flag wants, and `--deterministic` **requires**
that every `Key` in the program originate in a `with seed` head or a parameter,
never from `Stream.from_entropy`. That is the allow-list mechanism of §4.3 with a
syntactic locus attached, which is strictly easier to audit than a call graph.

## 6. The grammar

```
with-block  := "with" clause ("," clause)* ":" block else-clause?
clause      := shape-clause | policy-clause
shape-clause  := expression ".shape" "as" shape-pattern      # broadcasting.md §6.2
policy-clause := policy-name expression
policy-name := "seed" | "device" | "precision" | "unchecked"
```

The four policy names are **contextual**, recognised only as the first token of a
clause in a `with` head. `seed`, `device` and `precision` are words a scientist
uses constantly as ordinary identifiers — `reserved-words.md` explicitly lists
`device` among the words §13 was right to leave free — and all three keep
working: `let seed be 42` compiles, and `with seed seed:` is legal and forwards
it. §13's reserved list gains nothing.

`unchecked` takes no expression. The others take one.

## 7. What this note does not do

- **No dynamic scoping.** §2. A policy never crosses a function boundary
  implicitly; the parameter does.
- **No user-defined policies.** §3.5.
- **No `with` as an expression.** `broadcasting.md` §6.2 rejected that for shape
  clauses because the types cannot escape the block, and there is no reason to
  make a head that mixes clause kinds sometimes be an expression.
- **No resource management.** `with open(...) as f:` is Python's other use of
  the word, and Science does not need it: §6 of the core spec drops a file when
  it leaves scope, which is the whole feature, obtained from ownership. This is
  worth stating because it is the first thing a Python user will try.

## 8. Diagnostics allocated

| Code | Phase | Means |
|---|---|---|
| `SC0178` | Syntax | An unknown policy name in a `with` head, with the four offered |
| `SC0179` | Syntax | A policy clause given a value where it takes none, or the reverse |
| `SC0241` | Resolution | A policy is bound and never consumed (§4) |
| `SC0242` | Resolution | The same policy given twice in one head |
| `SC0243` | Resolution | An omitted argument matches two policy bindings |
| `SC0244` | Resolution | An argument omitted outside any block that binds it |
| `SC0245` | Resolution | *Warning.* A `with seed` block nested in another, noting that the key is derived and not reused |
| `SC0660` | Types | Operands on two devices in one expression (§3.2) |
| `SC0661` | Types | A `precision` block would change the type of a value bound outside it (§3.3) |
| `SC0662` | Types | A policy value has the wrong type — `with seed "abc"` |
| `SC0663`–`SC0669` | Types | Free |

## 9. What this note asks of others

| Ask | Of | Size |
|---|---|---|
| A second clause kind in the `with` head, and refutability computed per clause | `broadcasting.md` §6.2–§6.3 | Grammar plus one or-fold; the list already exists |
| Argument omission from a policy binding | `type-checking-and-mir.md` | **The one real ask.** Call-site elaboration at type-check time |
| `with` blocks in the run record | `reproducibility.md` §5.2 | One array of objects |
| `--deterministic` requires keys to originate in a head | `reproducibility.md` §4.3 | Tightens an allow-list that exists |
| `Key.derive` for nesting | `stdlib-standard.md` §5 | One method, standard for counter-based generators |
| `unchecked` as a block-scoped profile | `contracts.md` §7 | The profile exists; this scopes it |

## 10. Risks

- **Decision 3's implicit forwarding is the one genuinely contestable thing
  here.** It trades locality for the feature being usable at all. If it proves
  confusing, the fallback is to require an explicit `key: seed` at each call —
  which loses most of the value but keeps the record of §5, and that is the part
  a reviewer actually needs.
- **Four policies will become five.** Decision 4 says a fifth is a spec change
  and that is the only defence a closed set ever has.
- **`precision` is the dangerous one.** §3.3 restricts it to literals and
  defaults, and someone will still expect it to convert their data. `SC0661` is
  the guard and its wording will decide whether this policy is a good idea.
