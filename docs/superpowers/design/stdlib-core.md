# Science — Design: the always-available core (Level 1 of the standard library)

Date: 2026-09-16
Status: draft for F0
Amends: `docs/superpowers/specs/2026-09-16-science-f0-core-design.md` §8, whose
library surface is declared closed. This note is the spec change that restructures
it, and §2 says exactly how far.
Builds on, and does not re-specify: `collections-and-chains.md` (iteration and the
collection types), `data-io.md` (`Path`, `File`, the `data.*` modules),
`strings-formatting-and-docs.md` (interpolation, `Display`, `print`),
`scientific-libraries.md` §5 (`math`), `indexing-and-array-literals.md`
(`a[i]`, `Index`, `Slice of T`).
Written in the syntax of `syntax-revision-2.md` and `syntax-revision-3.md`.

---

## 0. What this note is for, given that four of its six areas are already owned

The request was for "Level 1 of the standard library: collections, io, fs, text,
errors, math — what is available with no import at all." Four of those six already
have an owner:

| Requested area | Already owned by | What that note decided |
|---|---|---|
| collections | `collections-and-chains.md` §1, §5 | `Iterate`, thirty-eight chain methods, `Array`/`Map`/`Set`, insertion order, what is refused |
| io | `strings-formatting-and-docs.md` §4 | `print`, `write`, `print_error`, `write_error`, `flush`, the stream and buffering policy |
| fs | `data-io.md` §7 | `Path`, `File`, the filesystem free functions, `TempDir`/`TempFile`, `glob` |
| text | `strings-formatting-and-docs.md` §1–§3, §6 | interpolation, the format mini-language, `Display`/`Inspect`, Unicode identifiers |
| errors | `syntax-revision-2.md` §3 | `-> (T, Error?)`, `Error` as a one-method interface, explicit conversion, `SC0140` |
| math | `scientific-libraries.md` §5 | the whole catalogue, in eleven subsections |

Re-cataloguing any of that would be worse than useless: it would create a second
copy that drifts. So this note does the thing none of those six answers, which is
also the thing the request actually turns on:

> **Which of it is there when you have written no `use` line at all, and which of
> it you must ask for — and why the line falls where it does.**

Every sibling note above answers "what exists". None answers "what is in scope in
an empty file". That question has a cost attached that no individual note could
see, because it is a *namespace* budget and a *stability* commitment, and both are
global.

The note is therefore organised as: the criterion (§1), the amendment to §8 that
gives the criterion somewhere to land (§2), then the six areas (§3–§8), each one a
*mapping* from what the owning note decided onto the two levels, plus decisions on
the parts genuinely nobody owns. §9 is the summary table, §10 the name audit,
§11 what this asks of the other notes, §12 the risks.

---

## 1. The criterion, made mechanical

### 1.1 The author's version

> If an algorithm may have to change for security or performance reasons without
> the language changing, it does not belong in the core.

This is right, and as stated it is a judgement call. Sharpened, it says something
precise, and the sharpening is the useful part:

> **Level 1 specifies observable behaviour. Where the algorithm is itself
> observable, the module is not Level 1.**

Everything else follows. `String.find` may be naive search today and two-way
tomorrow; nobody can tell, because the observable is "the leftmost byte offset at
which the needle occurs", and that is pinned forever. `base64.decode` cannot make
the same promise, because whether it rejects non-canonical trailing bits *is* the
observable, and the answer to that question has changed in living memory and will
change again.

### 1.2 The five questions

A candidate is Level 1 only if the answers are **yes, no, no, no, no**:

1. **Pinned?** Is there exactly one correct output for every input, fixed by
   something outside this language, that nobody is going to revise?
2. **Adversary?** Can a hostile party choose the input so as to make the
   implementation misbehave — quadratic blowup, non-canonical acceptance,
   traversal, a race?
3. **Performance product?** Does anybody ship a faster implementation as a thing
   you would want to choose? If the choice is real, it must be expressible, and
   Level 1 has no site at which to express a choice.
4. **Variants?** Does the real-world specification have options — padding, line
   length, endianness, locale, tolerance, method?
5. **Does a fix have to outrun the compiler?** If a correctness or security fix
   must reach users faster than `sciencec` releases, it cannot live in
   `science-rt`, because `science-rt` ships with `sciencec` (§8 of the core spec).

Question 5 is the one that makes the criterion mechanical rather than tasteful. It
is not about the algorithm's nature at all; it is about the shipping vehicle. And
it is checkable: ask whether you would be comfortable telling a user "the fix is in
the next compiler release."

### 1.3 A second gate, which is not the criterion

Passing §1.2 makes a module **eligible** for Level 1. It does not entitle it.
The second gate is the one `collections-and-chains.md` §0 already states for its
own vocabulary:

> Every method costs a name the user must learn, a page in the reference, a shape
> in the diagnostics.

At Level 1 the cost is higher still, because a Level 1 free function is a name in
every program's scope forever and a Level 1 type is an ABI commitment (§8: `science-rt`
carries "the representation of every library type"). So: **eligible by §1.2, and
written by enough programs to earn a permanent global name.** The two gates give
different answers often enough that keeping them separate is worth the paragraph —
hex is the clean example (§6.6).

### 1.4 The criterion, validated against a decision already taken

Before naming failures, it is worth checking the test against a split somebody
already made independently. `data-io.md` §2 put bytes, lines, `Path` and `File` in
the core and put CSV, JSON, npy, safetensors, Arrow and Parquet in `data.*`. Run
§1.2 over that split without looking at the reasons:

| | Pinned | Adversary | Perf product | Variants | Fix outruns |
|---|---|---|---|---|---|
| read a file to bytes | yes | no | no | no | no |
| CSV parsing | **no** | **yes** (quoting, embedded newlines, quadratic re-quoting) | **yes** | **yes** (delimiter, quote, escape, BOM, line ending) | **yes** |
| JSON parsing | no | **yes** (deep nesting, number blowup) | **yes** | **yes** (duplicate keys, big integers, NaN) | **yes** |

The test reproduces `data-io.md`'s split exactly, from different premises. That is
the strongest evidence available that it is the right test, and it means the rest
of this note is applying a rule the project has already been following by
instinct.

### 1.5 What fails it, named

The request asked for this to be specific and unafraid. These are the modules and
operations that people expect in a core library and that fail §1.2, with the
question they fail on.

| Candidate | Fails | Why, concretely |
|---|---|---|
| **base64** | Q1, Q2, Q3, Q4, Q5 | Standard/URL-safe/MIME alphabets; padding optional; non-canonical trailing bits are a signature-stripping vector; SIMD decoders are 5–10× and people ship them. The clearest failure in the whole list. |
| **Unicode case mapping** (`to_uppercase`) | Q1, Q4, Q5 | The tables are revised with every Unicode release. Turkish dotless `ı` makes it locale-dependent. `ß` uppercases to two characters. |
| **Grapheme segmentation** | Q1, Q5 | UAX #29 changes per Unicode release; an emoji sequence that was three graphemes last year is one this year. Also ~100 KiB of tables in every binary. |
| **Unicode normalisation** (NFC/NFD for user data) | Q1, Q4, Q5 | Per-version tables. *Identifier* normalisation (`strings-formatting-and-docs.md` §6.2) is exempt: it is the compiler's own, pinned by the compiler's version. |
| **Collation** (locale-aware string ordering) | Q1, Q2, Q4, Q5 | CLDR is revised continuously and is locale-parameterised. This is why `String: Ord` is byte order and says so (§6.4). |
| **Regular expressions** | Q1, Q2, Q3, Q4, Q5 | ReDoS is the textbook adversary; backtracking versus automaton is a visible performance choice; PCRE/RE2/POSIX are four dialects. Not in §8 today, and users will ask. |
| **Cryptographic hashing and randomness** | all five | The entire point of a crypto primitive is that its status changes without the language changing. |
| **`Map`'s hash function** | Q2 | HashDoS against attacker-chosen keys. See §3.4 — the *type* is Level 1 and the hash is not part of its specified behaviour. |
| **`glob`** | Q2, Q4, Q5 | `**/**/**/*` is a pattern-DoS; `**` semantics and hidden-file handling differ between every implementation. |
| **Directory mutation, `rename`, temporary files** | Q2, Q4 | Path traversal, TOCTOU, symlink races, predictable temp names, `TMPDIR`; and Windows and POSIX disagree on rename-over-existing and delete-while-open. |
| **`math` special functions** (`erf`, `bessel_*`, `gamma`) | Q1, Q5 | Accuracy is implementation-defined and *improves*; a better `bessel_j` is a library release, and `scientific-libraries.md` §2 explicitly wants these written and re-writable. |
| **Quadrature, ODE solvers, root finding** | Q1, Q3, Q4 | The method is literally an argument (`solve_ode(problem, method, tolerance)`). A module whose variants are parameters has failed Q4 by construction. |
| **`is_prime`, `factorise`** | Q1, Q2 | Miller–Rabin gives a probabilistic answer that depends on the witness set; `factorise` on a chosen 80-bit semiprime is a denial of service you can write in one line. |
| **CSV / JSON / Parquet** | Q1–Q5 | §1.4. Already in `data.*`; this note only confirms it. |

Two of these will be unpopular and both are correct. **Case mapping is not Level 1**
even though `"abc".to_uppercase()` is something every language has — because its
answer depends on a locale Science has no way to name and on a table that changes
annually. **Temporary files are not Level 1** even though `data-io.md` §7 put them
there and their ownership story is the best advertisement in that note — because
temp-file creation is a security surface and a security surface whose fix ships
with the compiler is a security surface with a six-month patch latency.

### 1.6 What it does *not* say

The criterion does not say the algorithm must be simple, or fast, or final. It
says the *observable* must be. Three Level 1 operations whose algorithms are
expected to change and which are Level 1 anyway:

- **Sorting.** `sorted()` is specified **stable**; the algorithm is unspecified and
  may be replaced. Stability is the observable; the sort network is not.
- **Float formatting and parsing.** The shortest round-tripping decimal
  representation of an `F64` is *unique*, and the correctly-rounded parse of a
  decimal string is *unique*. Ryū may replace Grisu with nobody noticing. Pinned.
- **Substring search.** §1.1.

That distinction — free algorithm, fixed observable — is the whole content of the
criterion, and every Level 1 entry below is justified by naming its observable.

---

## 2. §8 is closed, and this note is the amendment

### 2.1 What §8 says, and what has already happened to it

§8:

> The F0 library is small and its method sets are **closed** — anything not listed
> does not exist, and adding to the list is a spec change. It covers `Option`,
> `Result`, `Box`, `String`, `Array`, `Map`, the traits of §5.4, and the free
> functions `print`, `println`, `panic`, `read_file`, `write_file`.

Five notes have since amended it, each correctly and each invisibly to the others:

| Note | What it did to §8 |
|---|---|
| `syntax-revision-2.md` §3 | Deleted `Option` and `Result` as types (`T?` and `-> (T, Error?)`), added `Error` as an interface, renamed `println` to `print`, added `write`. Still five free functions. |
| `strings-formatting-and-docs.md` §4.3 | Five free functions became **eight** (`print_error`, `write_error`, `flush`). Added the types `Formatter` and `FormatSpec`; added the interfaces `Inspect` and `DisplayNumber`. |
| `collections-and-chains.md` §5, §10 | Added `Set` (A10), added `Hash` to §5.4 (A8), made `Range` implement `Iterate` (A14), and added thirty-eight provided methods on `Iterate`. |
| `data-io.md` §2 | Added `Path`, `File`, `TempDir`, `TempFile`, seven filesystem free functions, `read_bytes`/`write_bytes`/`read_lines`/`write_lines`, and the errors `IoError`, `DataError`, `RowError`. |
| `indexing-and-array-literals.md` §1.1 | Replaced §5.4's `Index` with `Index of Idx` and `IndexMutably of Idx`; added `Slice of T`. |

`examples/README.md` records what the corpus had to invent in the gap, including
the method sets of `String`, `Array` and `Map` — "§8 names the types, the traits
and the free functions, and says anything not listed does not exist — but it
stopped listing."

So §8's word "closed" currently means: closed, except for five open amendments and
one set of method lists that were never written down. That is not a closed surface;
it is an unmaintained one. And the erosion is not anyone's fault — each note
amended §8 for a good reason and had no way to see the other four.

### 2.2 Decision: §8 becomes two levels, and only one of them is closed

**Decision.** §8's single list is replaced by two:

- **Level 1 — the core.** Available with no `use`. Enumerated exhaustively, method
  by method. **Closed**: an addition is a spec change, and a *removal or a
  signature change after 1.0 is an ABI break*, because §8 already says `science-rt`
  carries "the representation of every library type".
- **Level 2 — the toolchain libraries.** `collections`, `text`, `io`, `fs`, `math`,
  `data.*`, and in later phases `linalg`, `stats`, `optimize`, `signal`, `chem`,
  `bio`, `physics`. Shipped with `sciencec`, reached by `use`. **Not closed**: an
  addition is a library release, and that is the point — it is where everything
  that fails §1.2 goes to live, so that it *can* change.

The rule that restores the meaning of the word:

> **A Level 1 addition is a spec change. A Level 2 addition is not.**

**Rejected alternative 1: keep one flat closed list and grow it.** This is the
status quo and it has produced five silent amendments in one day. A list that grows
by one note per note is not closed; calling it closed makes the word carry no
information and makes the next amendment slightly cheaper than the last.

**Rejected alternative 2: no Level 1 at all — everything is imported, including
`print`.** Honest, and it is roughly Rust's position minus the prelude. Rejected
because `script-mode.md` §1 makes a bare file a runnable program, and a runnable
program whose first line must be `use io (print)` is not the language that note
designed. Also because the F1 notebook tier (§7.1 of that note) makes a cell a
fragment of the top level, and a notebook cell that must import `print` is a
notebook nobody uses.

**Rejected alternative 3: a user-editable prelude.** Two programs with the same
text would then mean different things. Against a language whose premise (§1) is
catching mistakes before the run, this is disqualifying on its own.

**The cost, stated.** Two levels means two stability contracts, and users must
know which one they are standing on. The mitigation is that the boundary is
*visible in the source*: if there is a `use` line, you are on Level 2 and the
library may change under you between toolchain releases. That is the same signal
`data-io.md` §2 already relies on, made explicit and given a rule.

### 2.3 The request to the core spec

Rewrite §8's second paragraph as the two lists of §9 below, and record that the
five amendments in §2.1 are absorbed rather than pending. Nothing in this note
needs a compiler feature that is not already asked for elsewhere, with the single
exception of associated constants in a `has:` block (§8.3, §11).

---

## 3. Collections

### 3.1 What was asked for, against what is already decided

The request names lists, dictionaries, sets, deque, queues.
`collections-and-chains.md` §5 decided the collection types and §5.3 refused four
candidates with reasons. The reconciliation:

| Requested | Status | Where |
|---|---|---|
| lists | **Provided**, as `Array of T` | §8 of the core spec; `collections-and-chains.md` §5.1 |
| dictionaries | **Provided**, as `Map of (K, V)`, **insertion-ordered** | `collections-and-chains.md` §5.2 (A9) |
| sets | **Provided**, as `Set of T` | `collections-and-chains.md` §5.1 (A10) |
| deque | **Refused** — "until something outside the chain API asks for it" | `collections-and-chains.md` §5.3 |
| queues | Not considered | — |

Also refused by §5.3, and this note agrees with all four: a sorted map (it is
`.iterate().sorted(by: each.key)`), a small-vector (an ABI cost for no glue
benefit), a slice type over the chain (superseded anyway —
`indexing-and-array-literals.md` §2 gives `Slice of T` for the *indexing* reason,
which is a different argument), and a priority queue.

### 3.2 Decision: `Deque of T` exists, at Level 2, and `Queue` and `Stack` do not

`collections-and-chains.md` §5.3 refused the deque "until something outside the
chain API asks for it." **This note is that ask**, and it is worth being precise
that this is not a reversal: §5.3 refused adding a deque to the *closed core set*,
and Level 2 is a place §5.3 did not have. Its argument survives intact — a chain
still moves forward, and `windows(n)`'s ring buffer is still an implementation
detail of one adapter.

The ask that §5.3 was waiting for is this: **`Array` has `push` and `pop` and no
`pop_front`, so a queue written over `Array` is O(n) per dequeue and nothing in the
type or the name says so.** A user writing breadth-first search — over a graph, a
directory tree, a reaction network, a phylogeny — writes an accidental O(n²) and
the language gives no signal at all. That is a performance trap the closed set
*creates*, and it is outside the chain API by construction, because a work queue
is not a pipeline: it is fed from its own output.

**Decision.** `Deque of T` is provided in `collections` at Level 2, with
`push_back`, `pop_back`, `push_front`, `pop_front`, `first`, `last`, `length`,
`is_empty`, and the three `iterate*` methods `collections-and-chains.md` §5.4
requires of every collection.

**Decision.** There is no `Queue` and no `Stack`. A queue is a `Deque` used from
both ends; a stack is an `Array`. Naming one data structure three times is what §8
exists to prevent, and `collections-and-chains.md` §3 has already spent its budget
on names that carry information.

**Why Level 2 and not Level 1.** It passes §1.2 cleanly — FIFO order is pinned,
there is no adversary, nobody ships a competing deque, there are no variants, and
no fix outruns the compiler. It fails §1.3: the fraction of Science programs that
will ever name a deque is small, and a permanent global type name is expensive.
`use collections (Deque)` is one line in the programs that want it.

**Cost.** A user who does not know the type exists still writes the O(n²) loop.
The mitigation is a lint, not a type: **a `remove(0)`-shaped pattern on an `Array`
inside a loop should suggest `Deque`.** That is a diagnostic in somebody else's
note; it is recorded here as a customer.

### 3.3 Set operations are a reserved-word hazard, and only one name is affected

`collections-and-chains.md` §5.1 gives `Set` the methods `union`, `intersection`,
`difference`, `insert`, `remove`, `has`, `length`, `iterate()`, `Set.from`, and
§3.2 of that note defends the mathematical names: *"Renaming them to `shared_with`
and `without` would be prose for its own sake."* Agreed, and neither note priced
the collision, so this one does.

**`union` is reserved.** §13 lists it under "Reserved, not yet used".
`a.union(b)` does not parse today: `reserved-words.md` §0.1 shows that
`expect_ident` after a `.` accepts `TokenKind::Ident` and nothing else.
`intersection` and `difference` are not reserved and are not at risk. So the
hazard is exactly one name wide.

**Decision.** Level 1 keeps `union`, and depends on **either** of two things
`reserved-words.md` already asks for, whichever lands first:

1. **The dot rule** (§0.1, ask 1): after `.`, any word is a member name. This
   fixes `a.union(b)` outright and costs one function in the parser.
2. **Freeing `union`** (§5.2, ask 2): the FFI note's `extern` sublanguage already
   recognises it contextually, so the global reservation buys nothing.

**If both are refused:** the name is `merged`, which `scientific-libraries.md` §3
already chose for the same collision, so the two notes stay consistent. It is a
worse name — a merge is not an algebraic union — and that is the price of the
reservation, stated where it is paid.

### 3.4 `Set.has` becomes `Set.contains`, and this is a real inconsistency being fixed

`collections-and-chains.md` §5.1 gives `Set` a method `has`. The corpus
(`examples/README.md`) gives `Map` a method `contains`. Those are the same
operation under two names on two sibling types, and `has` additionally collides
with the `Doc has:` keyword, so `set.has(x)` does not parse without the dot rule
either.

**Decision.** Level 1 spells membership `contains` on both `Map` and `Set`.

**Why `contains` and not `has` on both:** `contains` is not reserved and needs no
parser change; `has` is a keyword and would make a fundamental collection method
hostage to a parser rule. Between two names that read equally well, the one that
compiles wins. This adjusts `collections-and-chains.md` §5.1 and is named in §11.

### 3.5 The hash function is not part of `Map`'s specified behaviour

`Map` fails Q2 of §1.2: its keys are frequently attacker-chosen (a header name, a
filename, a column label from a file the user did not write), and a non-randomised
hash is a denial of service.

**Decision.** `Map` and `Set` are Level 1 **types**. The hash function is runtime
state, is not specified, is seeded per process, and may be changed in a patch
release of `science-rt` without a spec change.

This is free, and it is free because of a decision somebody else already made.
`collections-and-chains.md` §5.2 (A9) made `Map` and `Set` iterate in **insertion
order**, for reproducibility. The consequence that note did not draw: **iteration
order is then not a function of the hash at all**, so changing the hash changes no
program's output. A language with hash-ordered iteration cannot swap its hasher
without breaking published results; Science can, and §5.2 bought that without
knowing it. It is worth recording in that note, because it is an argument for A9
that is independent of the one A9 gives.

**Cost.** Per-process seeding means the hash differs between runs, which would be a
`§5.1` reproducibility violation if anything observed it. Nothing does — that is
exactly what the previous paragraph establishes — and the invariant must be
stated in the reference so it is not accidentally broken by a later "iterate in
hash order for speed" optimisation.

### 3.6 Five signatures

```science
Array of T has:
    def push(mutable self, value: T)

    ## Parentheses are required: `borrowed T?` would read as `borrowed (T?)`.
    def get(borrowed self, index: Int) -> (borrowed T)?

Map of (K, V) has:
    ## Returns the displaced value, or null. Insertion order is preserved
    ## (`collections-and-chains.md` §5.2); re-inserting an existing key does
    ## not move it.
    def insert(mutable self, key: K, value: V) -> V? where K: Eq + Hash

Set of T has:
    def union(borrowed self, other: borrowed Set of T) -> Set of T where T: Eq + Hash

## Level 2: use collections (Deque)
Deque of T has:
    def pop_front(mutable self) -> T?
```

### 3.7 In use

```science
use collections (Deque)

def reachable(graph: borrowed Graph, start: NodeId) -> Set of NodeId:
    let mutable seen be Set.new()
    let mutable pending be Deque.new()
    pending.push_back(start)
    loop:
        let node be pending.pop_front()
        if not node?:
            break
        if seen.contains(node):
            continue
        seen.insert(node)
        for next in graph.neighbours(node):
            pending.push_back(next)
    seen
```

`node` is `NodeId?` at the binding and `NodeId` after the `if not node?: break`,
by the flow-sensitive narrowing of `syntax-revision-2.md` §3.1. This is the pattern
every work-queue loop takes, and it is the argument for `loop`/`break` being
enough (§2.2 of that note) made concrete.

---

## 4. io

### 4.1 What is already settled

`strings-formatting-and-docs.md` §4 settles the output surface completely: `print`
is unary over `borrowed any Display`, `write` is the no-newline form,
`print_error` and `write_error` go to stderr, `flush` is a free function, stdout is
line-buffered on a terminal and 64 KiB block-buffered otherwise, stderr is
unbuffered, and cross-stream ordering is not guaranteed. None of that is reopened.

What §4 does not give is the layer underneath. `print` takes a `Display`;
`data-io.md` §7's `File` takes `Array of U8`; and there is no interface connecting
them, which means there is no way to write a function that is generic over "a place
bytes go". That hole is this section's subject, along with the fact that **Science
currently has no way to read standard input at all** — `read_file` is the only
input in §8, and a program in a pipe cannot use it.

### 4.2 Decision: two one-method interfaces, `Read` and `Write`

**Decision.**

```science
interface Read:
    ## Fills as much of `into` as is available. Returns the count. A count of
    ## zero with no error means end of input, and is the only end-of-input
    ## signal.
    def read(mutable self, into: mutable borrowed Array of U8) -> (U64, Error?)

interface Write:
    ## Writes all of `bytes`, or fails. There is no partial write.
    def write(mutable self, bytes: borrowed Array of U8) -> Error?
```

They are named in the shape `Iterate`, `Display`, `Inspect` and `Summarize`
already use — a verb naming what an implementor does.

**`write` writes all of it, and this contradicts `data-io.md` §7.** That note gives
`File.write(...) -> (U64, IoError?)`. The count in a write result is
only useful if the caller loops on it, and the correct loop is the same four lines
in every caller, written wrong in some of them. `data-io.md`'s own worked example
never looks at the count. The partial-write loop belongs in `science-rt` once, not
in every program.

**Rejected alternative:** Go's `io.Writer` signature, which returns the count. Go's
own experience is the argument against it — `io.Writer` implementations that return
a short write without an error are a known bug class, and `io.WriteString`,
`bufio` and `io.Copy` all exist partly to paper over it.

**`read` keeps its count**, because a short read is meaningful: it is how end of
input is signalled, and how a non-blocking source says "that is all I have now".
The zero-with-no-error convention is stated in the signature's doc comment because
it is the one thing implementors get wrong.

### 4.3 Decision: `File` is unbuffered, and buffering is a named wrapper

**Decision.** `File` performs one syscall per `read` or `write`. `BufferedReader`
and `BufferedWriter` are explicit wrappers, at Level 2 in `io`.

**Rejected alternative: buffer `File` by default**, as Python does. Rejected
because `data-io.md` §7 already made `close()` consume the file and return an
error, on the explicit grounds that "a flush that failed at close is a real
data-loss bug that must not be swallowed". Implicit buffering makes `close()` the
*only* place a write error can surface, which converts that note's carefully
narrow data-loss window into the normal case. Explicit buffering keeps `File.write`
honest about when bytes left the process.

**Cost, stated.** A naive loop writing one line at a time to an unbuffered `File`
is syscall-bound, and somebody will write it. Two mitigations, both existing:
`write_lines` and `write_bytes` (Level 1, from `data-io.md` §2) buffer internally
because they own the whole operation, and `File.lines()` returns a `Lines` source
that is buffered by construction. The unbuffered path is the one you get only by
asking for `File` explicitly.

### 4.4 Decision: one new free function, `read_line`

**Decision.**

```science
def read_line() -> (String?, Error?)
```

`null` means end of input; a blank line is `""`, so the two are distinguishable,
which they are not in any design that returns a bare `String`.

This is the **ninth** free function and it is added with the same reluctance
`strings-formatting-and-docs.md` §4.2 expressed over `flush`. The justification is
that without it Science cannot write a filter — `cat data | summarise` — and
"read a line and do something with it" is the second program anyone writes after
hello-world. `script-mode.md` makes a bare file runnable specifically so that this
kind of program is short, and it is not short if it begins with two `use` lines and
a buffered-reader construction.

**Rejected alternative: a `Stdin` type with a `lines()` source, and no free
function.** That is the right *general* mechanism and it is what Level 2 `io`
provides. It is not the right thing to make mandatory for a ten-line script, for
the same reason `print` is not a method on a stream.

**Cost.** `read_line` allocates a `String` per line, and a program reading ten
million lines should use `File.lines()` or `Stdin.lines()` instead. Stated in the
reference; not a language problem.

### 4.5 What is Level 1 and what is Level 2

| Level 1 (no import) | Level 2 (`use io`) |
|---|---|
| `print`, `write`, `print_error`, `write_error`, `flush`, `panic` | `BufferedReader of R`, `BufferedWriter of W` |
| `read_line` | `Stdin`, `Stdout`, `Stderr` as values implementing `Read`/`Write` |
| `Read`, `Write` | `copy(from: mutable borrowed any Read, to: mutable borrowed any Write) -> (U64, Error?)` |
| `File` (from `data-io.md` §7), implementing both | `Lines`, `Bytes` over an arbitrary `Read` |
| `Lines`, from `File.lines()` and `String.lines()` | |
| `Formatter`, `FormatSpec` (from `strings-formatting-and-docs.md` §3.1) | |

`Lines` is Level 1 on purpose. The whole laziness argument in
`collections-and-chains.md` §2.1 is about not materialising a forty-gigabyte file,
and a language where avoiding that requires an import has put the trap on the
default path.

### 4.6 Five signatures

```science
## Level 1. Settled by `strings-formatting-and-docs.md` §4.1; repeated for the
## sake of a complete Level 1 enumeration, not re-decided.
def print(value: borrowed any Display)

## Level 1. New in this note. null at end of input.
def read_line() -> (String?, Error?)

interface Read:
    def read(mutable self, into: mutable borrowed Array of U8) -> (U64, Error?)

interface Write:
    def write(mutable self, bytes: borrowed Array of U8) -> Error?

## Level 2: use io (BufferedReader)
BufferedReader of R has:
    def new(source: R, capacity: Int) -> BufferedReader of R where R: Read
```

### 4.7 In use

```science
def count_lines() -> (Int, IoError?):
    let mutable total be 0
    loop:
        let line, err be read_line()
        if err?:
            return (0, err)
        if not line?:
            break
        total be total + 1
    print(f"{total} lines")
    return (total, null)
```

---

## 5. fs

### 5.1 What `data-io.md` §7 decided, and what Level 1 takes of it

`data-io.md` §7 owns the filesystem entirely: `Path` as its own type, UTF-8 with
`NotUnicode` for the rest, thirteen `Path` methods, seven free functions,
`TempDir`/`TempFile`, `File`, and the rule that there is deliberately no recursive
delete. None of that is reopened. §2 of that note puts all of it in the core.

**This note splits it, and that is a disagreement with `data-io.md` §2 rather than
a clarification of it.** The reason is §1.2, applied to each half:

| Operation | Pinned | Adversary | Variants | Verdict |
|---|---|---|---|---|
| Read/write a whole file by path | yes | no | no | **Level 1** |
| `Path` manipulation (`join`, `parent`, `extension`) | yes | no | no (separators are the OS's, and `Path` exists to hide them) | **Level 1** |
| Create/remove directories, `rename`, `list_directory` | pinned by the OS | **yes** — traversal, TOCTOU, symlink races | **yes** — Windows and POSIX disagree on rename-over-existing and on deleting an open file | **Level 2** |
| `glob` | no | **yes** — pattern DoS | **yes** — `**`, hidden files, case | **Level 2** |
| `TempDir`, `TempFile` | no | **yes** — predictable names, symlink attacks, `TMPDIR` | **yes** | **Level 2** |

**Decision.**

| Level 1 (no import) | Level 2 (`use fs`) |
|---|---|
| `Path`, with the thirteen methods of `data-io.md` §7 | `current_directory`, `create_directory`, `remove_file`, `remove_directory`, `rename`, `list_directory` |
| `read_file`, `write_file`, `read_bytes`, `write_bytes`, `read_lines`, `write_lines` | `glob`, `GlobError` |
| `File`, `IoError` | `TempDir`, `TempFile`, `temporary_directory`, `temporary_file` |

**The second reason, which is about namespace and not security.** Seven filesystem
free functions on top of `strings-formatting-and-docs.md` §4.3's eight is fifteen
names in every program's scope, and `rename`, `glob` and `list_directory` are
words users want for their own functions. `use fs` costs one line in the minority
of programs that mutate a directory tree and buys the name back for everyone else.

**The third reason, which is the best one.** `use fs (remove_file)` at the top of a
file is a *reviewable signal*: it says, in a place a reader looks first, that this
program deletes things. That is worth more than the line it costs, and it is
exactly the kind of visibility `data-io.md` §7 was reaching for when it refused
recursive delete outright.

**Cost, stated.** Every program in `data-io.md` that touches the filesystem gains
one `use fs` line, including its §10 worked example. Nothing else in that note
changes: no signature, no type, no name.

### 5.2 `read_file` takes a `Path`, and `syntax-revision-2.md` §7.1 is affected

`data-io.md` §7 makes `Path` a distinct type precisely so that "a function taking
`borrowed Path` cannot be handed a column name or a URL by accident". §8 of the
core spec and `syntax-revision-2.md` §7.1 both write `read_file("a.txt")` with a
string literal.

Those cannot both hold. Science has no implicit conversion (§5.1: *"No implicit
numeric conversion; `as` is always written"*), and inventing one for this case
would be the first implicit conversion in the language, introduced for a
convenience.

**Decision.** `read_file` and its five siblings take `borrowed Path`. The call is
`read_file(Path.from("a.txt"))`.

**Rejected alternative: two spellings, `read_file(String)` and `fs.read(Path)`.**
Rejected on `syntax-revision-2.md` §1's own grounds — the revision's best argument
was the removal of exactly this kind of duplication, and re-introducing it in the
first library call anybody writes would be inconsistent on the day the ink dried.

**Mitigation, which is the same shape `strings-formatting-and-docs.md` §4.1 used
for `print`:** a dedicated diagnostic with an automatically applicable fix.

```
error[SC0257]: a `String` was passed where a `Path` is expected
  --> load.science:3:23
   |
 3 |     let text, err be read_file("runs/a.csv")
   |                                ^^^^^^^^^^^^ this is a `String`
   |
   = note: `Path` is a separate type so that a column name or a URL cannot be
           passed to the filesystem by accident (`data-io.md` §7)
help: wrap it
   |
 3 |     let text, err be read_file(Path.from("runs/a.csv"))
```

The user meets it once, the fix is mechanical, and after that they write
`Path.from` without thinking. `syntax-revision-2.md` §7.1's example should be
updated; it is named in §11.

### 5.3 Five signatures

```science
## Level 1. Note the concrete error type, not `Error?` — see §7.3.
def read_file(path: borrowed Path) -> (String, IoError?)

## Level 1. No value, so the whole return is the error.
def write_lines(path: borrowed Path, lines: borrowed Array of String) -> IoError?

Path has:
    def join(self, part: borrowed String) -> Path

File has:
    def open(path: borrowed Path) -> (File, IoError?)

## Level 2: use fs (glob). Results are in byte order, always (`data-io.md` §7).
def glob(pattern: borrowed String) -> (Array of Path, GlobError?)
```

### 5.4 In use

```science
use fs (glob)

def total_bytes(pattern: borrowed String) -> (U64, IoError?):
    let paths, err be glob(pattern)
    if err?:
        return (0, IoError.Other(0, err.message()))
    let mutable total be 0
    for path in paths:
        let size, size_err be path.size()
        if size_err?:
            return (0, size_err)
        total be total + size
    return (total, null)
```

The `GlobError` to `IoError` step on line 6 is the explicit conversion
`syntax-revision-2.md` §3.4 requires, written where the caller writes the return.
§7.4 says what it costs and what the alternative would have been.

---

## 6. Text

### 6.1 What is already owned, and what is not

`strings-formatting-and-docs.md` owns interpolation (§1), the format mini-language
(§2), `Display`/`Inspect`/`DisplayNumber` (§3), `print` (§4), doc comments (§5) and
the Unicode-identifier policy (§6). All of it is Level 1 and none of it is
reopened.

What nobody owns: **`String`'s method surface**, and **what `text[i]` means**.
§8 declares `String`'s method set closed and stopped enumerating it, so
`examples/README.md` wrote the corpus's reading down instead — `new`, `length`,
`is_empty`, `push_str`, `truncate`, `starts_with`, `chars` — and recorded two
warts. `indexing-and-array-literals.md` covers `Array` and `Tensor` and does not
mention `String` anywhere.

### 6.2 Decision: `String` implements `Clone`, and the wart is retired

`examples/README.md` records:

> **`String` does not implement `Clone`.** Nothing says `String: Clone`, so every
> file needing an owned copy of a `borrowed String` builds one with `String.new()`
> and `push_str`. The same pair stands in for concatenation, which the library
> also omits.

**Decision.** `String implements Clone`.

It is not merely a wart. It is a contradiction with a sibling note that would be
discovered by the type checker: `collections-and-chains.md` §1.4 specifies

```science
def owned(self) -> Owned of Self where Self.Item is borrowed T, T: Clone
```

and §4.3 of that note gives `docs.iterate().map(each.title).owned()` as the way to
get `Array of String` out of a chain over borrowed titles. That does not compile
unless `String: Clone`. The corpus's reading was a reasonable inference from a
silent spec, and it is wrong.

**Cost.** `Clone` on `String` allocates, and `Clone` being visible in the type
(`collections-and-chains.md` §4.3) is what keeps the allocation visible. That
property is preserved: `.owned()` and `.clone()` are both written.

### 6.3 Decision: `String` implements `Add`, and `+` concatenates

**Decision.** `a + b` on two `String`s is concatenation, through §5.4's `Add`.
`push_str` remains, and is what a loop should use.

**Rejected alternative: no operator, interpolation only.** This is the status quo
and it is defensible — `f"{a}{b}"` is better than `a + b` most of the time, and
`strings-formatting-and-docs.md` §1 makes it pleasant. It is rejected because the
absence produces a *type error on the most-guessed expression in the language*,
and because `String.new()` + `push_str` as the stand-in for concatenation is worse
in every way than either alternative.

This does not contradict `collections-and-chains.md` §1.5, which refused `sum()`
over strings on the grounds that "concatenation is not summation". That is a
statement about the chain terminal and it stands; `String.from(chain)` remains the
way to join a chain. `Add` is about the binary operator.

**Cost, stated.** `s be s + t` in a loop is O(n²) and Java taught a generation to
write it. Science makes it slightly harder to hide than most languages do, because
there is no compound assignment (§4.6's precedence table omits assignment
entirely), so the rebinding is visible on the line. A lint should still name
`push_str` when `s be s + t` appears in a loop whose subject is `s`.

### 6.4 Decision: `String` does not implement `Index`, and this is the biggest call in the note

`text[0]` has three defensible meanings and Science must pick none of them.

| Reading | Cost | Wrong how |
|---|---|---|
| **Byte** | O(1) | Splits a character. `"é"[0]` is half a letter. This is Go's answer and it is the source of a decade of mojibake. |
| **Unicode scalar** | O(n) over UTF-8 | Splits a grapheme. `"é"` as `e` + U+0301 is two scalars. This is Python 3's answer, and it is O(1) there only because Python's strings are not UTF-8 — buying it here means a second string representation, which `data-io.md` §7 spent a whole paragraph refusing. |
| **Grapheme** | O(n), plus UAX #29 tables | Not wrong, and **not Level 1** — it fails Q1 and Q5 of §1.2 outright, because the segmentation rules change with every Unicode release. |

So one reading is fast and wrong, one is slow and subtly wrong, and one is correct
and ineligible. §4.6 of the core spec has the rule that settles it: *a language
cannot resolve an ambiguity by guessing.* Here there is no defensible guess.

**Decision.** `String` implements neither `Index` nor `IndexMutably`. `text[i]` is
a type error, `SC0258`, whose message names the three views by their unit:

```
error[SC0258]: a `String` cannot be indexed
  --> clean.science:7:13
   |
 7 |     let c be line[0]
   |              ^^^^^^^ `String` does not implement `Index`
   |
   = note: a `String` is UTF-8, so there is no single right meaning for `[0]`:
           byte 0, character 0 and grapheme 0 are three different things
help: say which one you mean
   |
   |     line.bytes()[0]                 # U8 — a byte
   |     line.characters().first()       # Char? — a Unicode scalar
   |     line.slice(0..n)                # (borrowed String, TextError?) — by byte offset
```

**Rejected alternative: Swift's `String.Index`.** Correct, and it makes `s[i]`
unwritable anyway, which is the same outcome as refusing the operator — at the
cost of an opaque index type, its arithmetic, and its invalidation rules. Refusing
the operator gets the same safety for none of that.

**Decision.** The three views:

```science
String has:
    ## O(1). The underlying UTF-8, as bytes.
    def bytes(borrowed self) -> borrowed Array of U8

    ## A lazy source over Unicode scalar values.
    def characters(borrowed self) -> Characters

    ## A byte range. Fails if either end is not a character boundary, which is
    ## the honest signature: it is the only one that cannot silently produce
    ## broken UTF-8.
    def slice(borrowed self, bytes: Range of Int) -> (borrowed String, TextError?)
```

Sub-ranges *by character* are the chain: `.characters().skip(a).take(b - a)`,
which is exactly what `collections-and-chains.md` §5.3 said when it refused a slice
type — "Refused, and this is a saving, not a loss."

`text.graphemes()` is Level 2, in `text`, for the reason in §1.5.

### 6.5 Decision: `length()` is bytes, and `truncate` is made total

The same trap, one level down. `length()` must mean something.

**Decision.** `String.length()` is the **byte** length. It is the only O(1) answer,
it is the one every serialisation and every buffer allocation needs, and it is the
number that pairs with `bytes()` and `slice()`. The character count is
`.characters().count()`, which is O(n) and says so by being a chain terminal
(`collections-and-chains.md` §3.1 already distinguishes `length()` from `count()`
on exactly this axis: "`length()` on a collection is O(1), counting a stream
consumes it").

**Decision.** `truncate(bytes: Int)` shortens to **at most** `bytes` bytes,
stopping at the last character boundary at or below that offset.

This is the decision that makes the byte-length choice safe. `truncate` is the one
`String` mutator that can produce invalid UTF-8, and making it stop at a boundary
makes it *total* — it cannot fail, cannot panic, and cannot corrupt. The cost is
that `truncate(200)` may leave 197 bytes, which is correct for every use
`truncate` actually has. `syntax-revision-2.md` §7.1's `self.body.truncate(200)` is
a bounded preview and means exactly what its author intended under this rule.

**Cost, stated plainly.** `"héllo".length()` is 6, not 5, and this will surprise
every Python user forever. It is a documentation problem with no compiler
mitigation available, because both answers are legitimate and neither is a
mistake. The alternative — no `length()` at all — was considered and rejected
because §4.6 of the core spec writes `text.length()` and because a type with no
length method is hostile.

### 6.6 Encodings: hex and base64, which are the criterion's clearest test

Run §1.2 over both.

| | Pinned | Adversary | Perf product | Variants | Fix outruns |
|---|---|---|---|---|---|
| **hex** | **yes** — bijective, one answer, only a case choice on output | no — non-hex input and odd length have exactly one sane answer, which is to fail | marginal | no | no |
| **base64** | **no** | **yes** | **yes** | **yes** | **yes** |

base64's four failures, concretely:

- **Variants.** Standard (RFC 4648 §4), URL-safe (§5), MIME with 76-column line
  breaks, and padding optional in several profiles. Four incompatible answers to
  "what is base64".
- **Adversary.** Non-canonical trailing bits: `"QQ=="` and `"QR=="` both decode to
  `A` under a lenient decoder. Accepting both makes a base64 value *malleable*,
  which breaks signature comparison and token equality. Whether a decoder rejects
  them is the observable, and real libraries have flipped that answer in security
  releases.
- **Performance product.** SIMD base64 is 5–10× scalar, and people ship it.
- **Fix outruns.** The two above mean a base64 decoder's behaviour genuinely does
  change in a patch release. If it lives in `science-rt`, that patch is a compiler
  release.

**Decision. `base64` is Level 2, in `text`, and it is the worked example of §1.**

**Decision. `hex` is also Level 2, in `text` — and for a different reason, which
is the point.** Hex *passes* §1.2. It fails §1.3: hex encoding appears in
approximately no scientific pipeline outside of hashing and binary debugging, and
it has not earned a permanent global name.

Stating both reasons matters because the two verdicts carry different **contracts**,
and users can rely on the difference:

> `text.hex` may not change its behaviour; a change is a spec change.
> `text.base64` **may** change its validation policy in a toolchain release, and
> the release notes will say so.

That is what the criterion buys. Without §1 the two would look identical on the
shelf and one of them would quietly move under a program that depended on it.

### 6.7 What else is Level 2 for the same reasons

- **Case mapping.** `to_uppercase`, `to_lowercase`, case-insensitive comparison.
  Q1 and Q4 (§1.5). `text` provides both the Unicode forms and the ASCII-only
  forms, and names which is which — the ASCII ones are pinned forever, the
  Unicode ones are not.
- **Normalisation.** NFC/NFD/NFKC/NFKD for user data.
- **Collation.** Locale-aware ordering. See §6.8.
- **Graphemes**, **word and sentence segmentation**, **line breaking**.
- **Latin-1 transcoding**, which `data-io.md` §12 already scopes to `data`.

### 6.8 `String: Ord` is byte order, and says so

**Decision.** `String` implements `Ord` by byte order, which for UTF-8 is the same
as code-point order. It is **not** collation.

This matters for the promise in §5.1 of the core spec — *"Results that differ
between runs are opt-in, because this is a language whose users publish."* Byte
order is reproducible everywhere. Collation is locale-dependent, CLDR-versioned,
and would make `sorted()` on a list of sample names produce different output on a
laptop and a cluster. Locale-aware ordering belongs in `text` beside the other
things that change, and a program that wants it says so in a `use` line.

### 6.9 The Level 1 `String` surface

Nineteen methods, against the corpus's seven. Each of the twelve additions is there
because without it a Level 1 promise is unkeepable: §8 gives `read_file`, and a
language that can read a text file and not split it into fields cannot do anything
with it.

```science
String has:
    def new() -> String
    def from_bytes(bytes: borrowed Array of U8) -> (String, TextError?)
    def length(borrowed self) -> Int                              # bytes — §6.5
    def is_empty(borrowed self) -> Bool
    def push_str(mutable self, tail: borrowed String)
    def truncate(mutable self, bytes: Int)                        # total — §6.5
    def starts_with(borrowed self, prefix: borrowed String) -> Bool
    def ends_with(borrowed self, suffix: borrowed String) -> Bool
    def contains(borrowed self, needle: borrowed String) -> Bool
    def find(borrowed self, needle: borrowed String) -> Int?      # byte offset
    def slice(borrowed self, bytes: Range of Int) -> (borrowed String, TextError?)
    def trim(borrowed self) -> borrowed String                    # ASCII whitespace — below
    def split(borrowed self, separator: borrowed String) -> Split
    def replace(borrowed self, from: borrowed String, to: borrowed String) -> String
    def bytes(borrowed self) -> borrowed Array of U8
    def characters(borrowed self) -> Characters
    def lines(borrowed self) -> Lines
    def parse_int(borrowed self) -> (I64, TextError?)
    def parse_float(borrowed self) -> (F64, TextError?)
```

Plus `String implements Clone, Eq, Ord, Add, Display, Inspect, Hash`.

**`trim` removes ASCII whitespace only** — space, tab, CR, LF, FF, VT — and that is
pinned forever. Unicode's `White_Space` property is revised per release, so
`text.trim_unicode()` is the other one. This is a small decision and it is the
criterion applied at its finest grain; it is stated because the alternative is
that somebody picks Unicode whitespace by default and `trim` silently changes
behaviour in a toolchain update.

**`parse_int` and `parse_float` are Level 1** because correctly-rounded decimal
parsing has a unique answer (Q1 yes) and because `read_lines` is useless without
them. The algorithm — Eisel–Lemire, Clinger, whatever comes next — is free.

### 6.10 Five signatures

```science
String has:
    def split(borrowed self, separator: borrowed String) -> Split

    def slice(borrowed self, bytes: Range of Int) -> (borrowed String, TextError?)

    def characters(borrowed self) -> Characters

    def parse_float(borrowed self) -> (F64, TextError?)

## Level 2: use text (graphemes). Not Level 1 because UAX #29 is revised with
## every Unicode release, and its tables are ~100 KiB in every binary (§1.5).
def graphemes(text: borrowed String) -> Graphemes
```

### 6.11 In use

```science
def readings(text: borrowed String) -> (Array of F64, TextError?):
    let mutable values be Array.new()
    for line in text.lines():
        let field be line.trim()
        if field.is_empty() or field.starts_with("#"):
            continue
        let value, err be field.parse_float()
        if err?:
            return (Array.new(), err)
        values.push(value)
    return (values, null)
```

---

## 7. Errors

### 7.1 What is decided, and is not reopened

`syntax-revision-2.md` §3 settles the model: a fallible function returns
`-> (T, Error?)`; `Error` is an interface with one required method; `Error?` in
return position is shorthand for `(any Error)?`; `?` is the presence test;
narrowing is flow-sensitive; conversion is explicit because `From` widening was
removed; and `SC0140` makes an unchecked error a compile error. None of that
changes here.

What §3 does not give is the *inventory*: which concrete error types exist, how a
library declares one, and what happens to an error when it crosses a module
boundary now that `From` widening is gone. `collections-and-chains.md` §6.3
(AMENDMENT 5) asked for `From` conversion inside `try` and that request is moot —
`try` no longer exists — but the underlying problem it named is not, and it is
this section's subject.

### 7.2 Decision: three error types at Level 1, and no more

**Decision.** Level 1 provides `Error` (the interface), `IoError`, and `TextError`.

| Type | Kind | Produced by |
|---|---|---|
| `Error` | interface | — |
| `IoError` | `choice` | `read_file`, `write_file`, `read_bytes`, `write_bytes`, `read_lines`, `write_lines`, `File.*`, `Path.size`, `Path.absolute` |
| `TextError` | `choice` | `String.from_bytes`, `String.slice`, `String.parse_int`, `String.parse_float` |

**One text error type, not two.** `ParseError` and an encoding error would always
be converted into each other at every boundary, and four variants is not a type
worth splitting. Stated because it is a judgement rather than a derivation.

`DataError`, `RowError` and `GlobError` move to Level 2 with the modules that
produce them — `data.*` and `fs` respectively. `data-io.md` §2 put the first two in
the core; they name format-specific conditions, so they belong with the formats,
and nothing at Level 1 can produce one.

### 7.3 Decision: Level 1 functions return their concrete error type, never `Error?`

**Decision.** No Level 1 function has `Error?` in its signature. `read_file`
returns `(String, IoError?)`.

Two reasons, both from `syntax-revision-2.md` §3.4:

1. **`any Error` allocates.** §3.4: *"the interface form allocates where the
   concrete form does not. For an error path that is acceptable; for a function
   called in a tight loop that returns `Error?` on every iteration it is not."*
   `parse_float` inside a loop over a million rows is precisely that function.
2. **The concrete form keeps `match` exhaustive**, which §3.4 names as the reason
   to prefer it whenever the caller may want to distinguish cases. A caller who
   wants to retry on `IoError.Interrupted` and fail on everything else can only do
   that against a concrete type.

**Cost.** A caller whose own signature says `Error?` writes the widening. That is
one line, it is `syntax-revision-2.md` §3.2's stated virtue — *the caller writes
the conversion, because the caller writes the return* — and §7.4 is where the bill
arrives.

### 7.4 Decision: a Level 1 error `choice` carries `Other`, and that is the whole non-exhaustiveness mechanism

There is a real problem hiding here. §11 of the core spec requires exhaustive
`match`, and §3 requires a non-exhaustive `match` to be rejected with the missing
patterns listed. So **adding a variant to a Level 1 error `choice` breaks every
program that matched on it exhaustively**, and operating systems produce conditions
nobody enumerated.

**Decision.**

```science
choice IoError:
    NotFound(Path)
    PermissionDenied(Path)
    AlreadyExists(Path)
    NotADirectory(Path)
    IsADirectory(Path)
    DirectoryNotEmpty(Path)
    NotUnicode(String)
    Interrupted
    UnexpectedEnd
    ## Anything the list above does not name, with the platform's own code.
    Other(I32, String)
```

The `Other` variant *is* the non-exhaustiveness mechanism. Because it exists, a
`match` over `IoError` already needs an `Other` or `_` arm, so every existing
program keeps compiling when a condition that used to arrive as `Other(13, …)`
becomes a named variant.

**Rejected alternative: a `#[non_exhaustive]`-style attribute** requiring a `_` arm
by decree. Rejected because it is a new language feature, §12 of the core spec
reserves macros and attributes without implementing them, and `Other` achieves the
same thing with no language change and better information — the caller can see the
raw code rather than being told only that there was one.

**Cost.** `Other(I32, String)` carries a platform-specific number that means
different things on different systems, and a program that switches on it is not
portable. Stated in the reference; the alternative is that the condition is
unrepresentable, which is worse.

`TextError` takes the same shape without needing an `Other`, because the set of
ways a UTF-8 or numeric parse can fail is genuinely finite:

```science
choice TextError:
    NotUtf8(Int)                        # byte offset of the first invalid sequence
    NotACharacterBoundary(Int)          # byte offset
    NotANumber(String)
    OutOfRange(String)
```

### 7.5 Decision: `Error` gains a defaulted `cause`, and `any Error` implements `Display`

With `From` widening gone, the naive way to cross a module boundary is to flatten
the error into a string:

```science
return (Config.empty(), ConfigError.Unreadable(err.message()))   # loses the cause
```

which destroys the type and doubles the allocation at every level. Go shipped
without a cause chain and added `Unwrap` later, and the omission is the most-cited
thing about its original error design.

**Decision.** `Error` gains one **defaulted** method. It remains a one-required-method
interface, so `syntax-revision-2.md` §3.4 is unchanged.

```science
interface Error:
    def message(self) -> String

    ## The error this one was built from, if any. Defaulted, so implementing
    ## `Error` is still a one-method job.
    def cause(self) -> (any Error)?:
        null
```

**Decision.** `any Error` implements `Display`, and the rendering walks the cause
chain, joining with `": "`.

```science
print(err)      # could not start: could not read runs/a.toml: permission denied
```

This is the whole reporting story and it costs zero new names: `print` already
takes `borrowed any Display` (`strings-formatting-and-docs.md` §4.1), and the
orphan rule (§5.4) permits the implementation because both `Display` and `any Error`
belong to the core library. There is no `error_chain` free function and there is no
tenth free function.

**Cost.** A wrapping error type must hold the wrapped value rather than its
message, which means the wrapper's variant payload is a concrete error type or a
boxed `any Error` — a pointer either way. That is the correct cost and it buys back
the ability to `match` on the root cause.

### 7.6 How a library defines and exports its own error

The rule, stated once:

> **A library exports a concrete error type. An application uses `Error?`.**

A library's caller may want to distinguish cases, and only a concrete type lets it.
An application's top level wants one type it can print, and `Error?` is that. This
is the Go and Rust consensus and it is what `syntax-revision-2.md` §3.4 implies
without saying.

The conversion convention: a wrapping type provides `from_*` associated functions,
named for what they wrap, so that the explicit conversion at the return has an
obvious spelling and the diagnostic in §7.7 has something to suggest.

### 7.7 `SC0259`: the diagnostic that replaces `From`

Removing `From` widening removed a source of magic and created a class of error
whose message would otherwise be a bare type mismatch far from the cause.

```
error[SC0259]: this is an `IoError`, but the function returns a `ConfigError`
  --> config.science:11:34
   |
 9 |     let text, err be read_file(path)
   |               --- `err` is `IoError?`
11 |         return (Config.empty(), err)
   |                                 ^^^ expected `ConfigError?`
   |
   = note: errors are not converted implicitly (`syntax-revision-2.md` §3.4);
           the caller writes the conversion, because the caller writes the return
help: `ConfigError` has a constructor for this
   |
11 |         return (Config.empty(), ConfigError.from_io(err))
```

The `help` is only offered when a `from_*` associated function on the target type
takes the source type; otherwise the note stands alone. This is what makes the
explicit-conversion decision affordable rather than merely principled: the
compiler writes the conversion the first time, and the user learns the shape.

### 7.8 Five signatures

```science
interface Error:
    def message(self) -> String
    def cause(self) -> (any Error)?:
        null

## Both belong to the core library, so the orphan rule (§5.4) permits this.
## Renders the cause chain, joined with ": ".
(any Error) implements Display:
    def display(borrowed self, into: mutable borrowed Formatter)

choice IoError:
    NotFound(Path)
    Other(I32, String)

ConfigError has:
    def from_io(cause: IoError) -> ConfigError
```

### 7.9 In use

```science
choice StartupError:
    Unreadable(IoError)

StartupError implements Error:
    def message(self) -> String:
        "could not read the configuration"

    def cause(self) -> (any Error)?:
        match self:
            StartupError.Unreadable(e): e

def start(path: borrowed Path) -> (Config, StartupError?):
    let text, err be read_file(path)
    if err?:
        return (Config.empty(), StartupError.Unreadable(err))
    return (parse_config(text), null)
```

`print(startup_err)` on the result renders
`could not read the configuration: permission denied` with no further code.

---

## 8. math

### 8.1 The split, and the reason it is not arbitrary

`scientific-libraries.md` §5 catalogues `math` in eleven subsections and several
hundred names. The only question here is which of it needs no import.

**Decision.**

| Level 1 (no import) | Level 2 (`use math`) |
|---|---|
| §5.1 elementary: `abs sign floor ceil round trunc fract min max clamp rem_euclid sqrt cbrt hypot exp ln log2 log10 pow` | §5.2 special functions in full — gamma, error, Bessel, orthogonal polynomials, elliptic, zeta, hypergeometric |
| §5.1 trigonometry: `sin cos tan asin acos atan atan2 sinh cosh tanh to_degrees to_radians` | §5.3 calculus — numerical differentiation and every quadrature rule |
| §5.1 predicates: `is_nan is_infinite is_finite is_close` | §5.4 ordinary differential equations in full |
| Constants: `PI E TAU INFINITY NAN EPSILON MIN MAX` | §5.5 interpolation, §5.6 `Polynomial`, §5.7 root finding |
| | §5.8 number theory and combinatorics |
| | §5.9 reductions not already `Iterate` terminals; §5.10 `Complex` |

Justified by §1.2, one row at a time:

- **Elementary and trigonometric functions pass everything.** Their values are
  pinned by definition; `scientific-libraries.md` §2 says they are written "over
  LLVM intrinsics. No FFI"; there is no variant, no adversary, and nobody ships a
  scalar `sin` as a product. They pass §1.3 too, because — see §8.2 — they cost no
  global names at all.
- **Special functions fail Q1 and Q5.** `scientific-libraries.md` §2 wants them
  written in Science and *re-writable*: "Cephes-derived algorithms are published
  formulas. Writing them in Science means they are generic over `F32`/`F64` and
  differentiable in F2." Accuracy is implementation-defined and improves. A better
  `bessel_j` must be a library release, not a compiler release.
- **Quadrature and ODEs fail Q4 by construction.** The method is an argument:
  `solve_ode(problem, method, tolerance)`. A module whose variants are its
  parameters has variants.
- **Number theory fails Q1 and Q2.** `is_prime` via Miller–Rabin gives a
  probabilistic answer that depends on the witness set — the algorithm *is* the
  observable. `factorise` on a chosen 80-bit semiprime is a denial of service you
  can write in one line.
- **`Complex` and `Polynomial` pass §1.2 and fail §1.3.** Twenty-five methods each
  on a type most programs never mention. `use math (Complex)` is one line, and F1's
  `Tensor of Complex` will want the module imported anyway.

### 8.2 Decision: the Level 1 math surface is methods, not free functions — and this settles a three-way drift

Three notes currently disagree about how to call `sqrt`:

- `scientific-libraries.md` §5.11: `def sqrt(x: F64) -> F64` — a free
  function in `math`.
- `strings-formatting-and-docs.md` §6.1: `σ * Δt.square_root() * θ.cos()` — methods.
- `data-io.md` §11.4 flags it as open: *"`square_root()` and the reductions belong
  to the numerics note."*

**Decision.** The Level 1 subset is **methods on `F32`, `F64` and the integer
types**. `math` at Level 2 keeps the free-function forms, generic over `T: Float`.

**The decisive reason is the namespace.** A Level 1 free function is a global name
forever. The elementary set contains `abs`, `min`, `max`, `round`, `sign`, `clamp`,
`pow` and `trunc` — eight words users want for their own functions, and `min` and
`max` are among the most common local variable names in numerical code. As methods
they cost nothing: member position is unambiguous, and after the dot rule
(`reserved-words.md` §0.1) it is unambiguous even for reserved words. Zero new
global names against roughly thirty-five is the entire argument.

Two supporting reasons: a method needs no import *by construction*, which is what
"always available" should mean; and `each.weight.sqrt()` composes with the chain
vocabulary (`collections-and-chains.md` §4.3's copy-out rule makes `each.weight` an
`F32` value, and a method call on it is the natural next link).

**Rejected alternative: free functions at Level 1.** Rejected on the namespace
count above. **Rejected alternative: both spellings.** Rejected on
`syntax-revision-2.md` §1's grounds; the revision's best argument was removing
duplicate spellings.

**Decision on the names: `sqrt`, not `square_root`.** §4.3 of the core spec says no
*keyword* is an abbreviation where a word exists; that rule is about keywords, and
`syntax-revision-2.md` §0 gives the library rule that supersedes it here — *use the
symbol where one exists and everybody already reads it.* `sqrt`, `sin`, `cos`,
`ln`, `exp`, `abs` are not abbreviated English; they are the universal
mathematical names, printed in every textbook and on every calculator, in exactly
the category `->` was restored to in §4 of that revision. `strings-formatting-and-docs.md`
§6.1's `Δt.square_root()` should become `Δt.sqrt()`; named in §11.

Note the one exception this rule does *not* license: `collections-and-chains.md`
§3.1 keeps `minimum`/`maximum` over `min`/`max` for chain terminals. That is a
different case and both can stand — `min`/`max` on a scalar are binary operations
with two operands and no universal long form, while `minimum()` on a stream is a
reduction whose name is doing prose work. If a reviewer judges the pair too subtle,
the fallback is `minimum`/`maximum` on the scalars too, at the cost of four extra
characters in the most-written arithmetic in the language.

### 8.3 The accuracy contract, and the reproducibility problem it exposes

A Level 1 function must state its observable, and for `sin` the observable is a
number.

**Decision.** Level 1 math functions are specified to within **1 ULP** of the
correctly-rounded result. They are **not** guaranteed bit-identical across targets.

**This is in tension with §5.1 of the core spec** — *"Reductions have a defined
order unless marked `unordered`. Results that differ between runs are opt-in,
because this is a language whose users publish."* A run on a laptop and a run on a
cluster may differ in the last bit of `sin(x)` for large `x`, because argument
reduction quality differs between libms and between targets, and that is a
run-to-run difference in the sense that matters to a paper.

The honest position: **this note states the contract and does not fix the problem**,
because the fix is a codegen decision (pin a correctly-rounded software
implementation, or a `--reproducible-math` mode that refuses to lower to target
intrinsics) and codegen is not this note's territory. It is recorded in §12 as a
risk and in §11 as an ask, and it is flagged now rather than discovered later,
because it is the one place where Level 1 makes a promise it cannot fully keep.

### 8.4 Five signatures

```science
F64 has:
    ## Level 1. Lowers to an LLVM intrinsic. Specified to within 1 ULP (§8.3).
    def sqrt(self) -> F64

    ## Level 1. The two-argument arctangent, quadrant-correct.
    def atan2(self, x: F64) -> F64

    ## Level 1. Relative comparison with the library's default tolerances,
    ## which exists because `a is b` on floats is almost always the wrong test.
    def is_close(self, other: F64) -> Bool

    ## Level 1. An associated constant, which §4.4 does not yet permit in a
    ## `has:` block — the one language ask in this note (§11).
    const PI: F64

## Level 2: use math (integrate). `scientific-libraries.md` §5.11, in revision-2
## syntax. Not Level 1: the method and tolerance are arguments, so it has
## variants by construction (§1.2 Q4).
def integrate(
    f: (F64) -> F64,
    lower: F64,
    upper: F64,
) -> (F64, QuadratureError?)
```

### 8.5 In use

```science
def rms(values: borrowed Array of F64) -> F64:
    if values.is_empty():
        return 0.0
    let mutable total be 0.0
    for v in values:
        total be total + v * v
    (total / values.length() as F64).sqrt()
```

Seven lines, no `use` line, and the only library name in it is `sqrt` — which is a
method, so it is not a name at all in the namespace sense. That is what §8.2 buys.

---

## 9. The summary table

Level 1 is the closed core. Level 2 ships with the toolchain and is imported.
Level 3 is phase-gated and listed for completeness.

| Module | Level | What it exposes | Implemented in | Depends on |
|---|---|---|---|---|
| *(core, unnamed)* | 1 | `T?`, `Box`, `Slice of T`, the §5.4 interfaces plus `Hash`, `Index of Idx`, `IndexMutably of Idx`, `Inspect`, `DisplayNumber`, `Formatter`, `FormatSpec` | Science + `science-rt` | — |
| collections *(core part)* | 1 | `Array`, `Map`, `Set`, `Range`, `Iterate` and its 38 provided methods, `Entry`/`Numbered`/`Pair`/`Parts`/`Outcome` | Science | core |
| `collections` | 2 | `Deque of T` | Science | core |
| text *(core part)* | 1 | `String` (19 methods, §6.9), `Characters`, `Lines`, `Split`, `TextError` | Science | core |
| `text` | 2 | `graphemes`, case mapping, normalisation, collation, word/line breaking, `base64`, `hex`, Latin-1 | Science + vendored Unicode tables | core |
| io *(core part)* | 1 | `print`, `write`, `print_error`, `write_error`, `flush`, `panic`, `read_line`, `Read`, `Write`, `File` | Science + `science-rt` | core, fs *(core part)* |
| `io` | 2 | `BufferedReader`, `BufferedWriter`, `Stdin`/`Stdout`/`Stderr`, `copy`, `Bytes` | Science | core |
| fs *(core part)* | 1 | `Path` (13 methods), `read_file`, `write_file`, `read_bytes`, `write_bytes`, `read_lines`, `write_lines`, `IoError` | Science + libc | core, text |
| `fs` | 2 | directory operations, `rename`, `list_directory`, `glob`/`GlobError`, `TempDir`/`TempFile` | Science + libc | core |
| errors *(core part)* | 1 | `Error` (interface, one required method + defaulted `cause`), `IoError`, `TextError`, `Display` for `any Error` | Science | core |
| math *(core part)* | 1 | ~35 methods and 8 constants on `F32`/`F64`/`Int` (§8.1) | Science over LLVM intrinsics | core |
| `math` | 2 | `scientific-libraries.md` §5.2–§5.10 | Science (Cephes-derived formulas) | math *(core part)* |
| `data`, `data.csv`, `data.json`, `data.npy`, `data.safetensors` | 2 | `data-io.md` §3–§8 | Science (+ vendored `miniz` for npz) | fs, text |
| `data.arrow`, `data.parquet` | 2 | `data-io.md` §3 | linked Arrow C++ | fs, text |
| `linalg` | 3 (F1) | `scientific-libraries.md` §6 | linked BLAS + LAPACK | `math` |
| `signal` | 3 (F1) | `scientific-libraries.md` §9 | linked PocketFFT / FFTW | `math` |
| `stats`, `optimize`, `chem`, `bio`, `physics` | 3 (F1+) | `scientific-libraries.md` §7, §8, §10, §11, §12 | Science, over linked `linalg` | `linalg`, `math` |

**Free functions at Level 1, the complete list — nine:**
`print`, `write`, `print_error`, `write_error`, `flush`, `panic`, `read_line`,
`read_file`, `write_file`.

Plus the four path-based whole-file functions that `data-io.md` §2 adds and this
note keeps at Level 1 — `read_bytes`, `write_bytes`, `read_lines`, `write_lines` —
which brings the true count to **thirteen**. That is a third again on top of
`strings-formatting-and-docs.md` §4.3's eight, and the trend of one or two per note
is the single most likely way this surface stops being small. §12 records it as a
risk.

---

## 10. Names, checked

Every name introduced or relocated by this note, against §13 and
`reserved-words.md`.

| Name | Reserved? | Position | Verdict / alternative |
|---|---|---|---|
| `Read`, `Write` (interfaces) | no | type | Clear. `write` the free function and `Write.write` the method live in different namespaces, and the pattern matches `Display`/`display`. |
| `read_line` | no | free function | Clear. |
| `Deque`, `push_front`, `pop_front`, `push_back`, `pop_back` | no | type, member | Clear. |
| `union` on `Set` | **yes** (§13, "reserved, not yet used") | member | **Hazard.** Needs the dot rule (`reserved-words.md` §0.1) **or** freeing the word (§5.2). If both refused: **`merged`**, matching `scientific-libraries.md` §3. `intersection` and `difference` are not reserved and need nothing. |
| `contains` on `Map` and `Set` | no | member | Chosen over `has`, which **is** a keyword (`Doc has:`) and would need the dot rule to parse at all (§3.4). |
| `has_any` / `has_all` (chain terminals) | no | member | `has_any` is one identifier, not the keyword `has`. No conflict; `collections-and-chains.md` §3.1's names stand. |
| `slice`, `bytes`, `characters`, `lines`, `split`, `trim`, `replace`, `find`, `truncate`, `parse_int`, `parse_float`, `from_bytes` | no | member | Clear. `find` also names a chain terminal; different receiver type, no ambiguity. |
| `TextError`, `IoError`, `GlobError`, `Characters`, `Lines`, `Split`, `Graphemes` | no | type | Clear. |
| `cause` on `Error` | no | member | Clear. |
| `Other` (variant) | no | variant | Clear. |
| `sqrt`, `sin`, `cos`, `atan2`, `abs`, `min`, `max`, `round`, `clamp`, `sign`, `trunc`, `pow`, `is_close`, `to_degrees`, `to_radians` | no | **member only** | Clear as written. Note these are *methods* precisely so that the eight tempting ones — `abs`, `min`, `max`, `round`, `sign`, `clamp`, `pow`, `trunc` — never occupy the global namespace (§8.2). |
| `PI`, `E`, `TAU`, `EPSILON`, `INFINITY`, `NAN`, `MIN`, `MAX` | no | associated constant | Clear. Needs associated constants in a `has:` block (§11). |
| module names `collections`, `text`, `io`, `fs`, `math` | no | module | Clear. `import` is reserved and unused here; Science spells it `use`. |
| `open`, `create`, `append`, `close`, `flush` on `File` | no | member | `data-io.md` §7's names; unchanged. |

**One hazard, one name wide.** `union` is the only genuine collision this note
introduces, it is inherited from `collections-and-chains.md` §5.1 rather than
created here, and `reserved-words.md` already asks for both of the two things that
fix it.

---

## 11. What this note asks of the others

Stated so the seams are visible, in the manner `data-io.md` §11 uses.

**Of the core spec:**

1. **Replace §8's single closed list with the two levels of §2.2 and §9.** The
   five outstanding amendments in §2.1 are absorbed rather than left pending.
2. **Associated constants in a `has:` block.** `F64.PI` needs it. §4.4 gives
   `const NAME be v` at module level and no form inside `has:`. This is the only
   language feature this note asks for, and it is small.
3. **A decision on cross-target float reproducibility** (§8.3), which is a codegen
   question. Level 1 math promises 1 ULP and cannot promise bit-identity; §5.1
   promises that differing results are opt-in. Somebody must reconcile those.

**Of `collections-and-chains.md`:**

4. **§5.1: `Set.has` becomes `Set.contains`**, matching `Map` and avoiding the
   `has` keyword (§3.4).
5. **§5.3's deque refusal is resolved, not overturned** (§3.2). That note refused a
   deque in the closed *core* set; Level 2 is a place it did not have.
6. **§5.2 (A9) has a second argument it did not make**: insertion order means
   iteration order is independent of the hash, which is what lets `science-rt`
   change its hasher for security without breaking a published result (§3.5).
7. **§6.3 (A5) is moot** — `try` no longer exists — but the problem it named
   survives, and §7.5–§7.7 here are the answer.

**Of `data-io.md`:**

8. **§2: the seven filesystem free functions, `TempDir`, `TempFile`, `DataError`
   and `RowError` move to Level 2** (§5.1, §7.2). Nothing else in that note
   changes; its §10 worked example gains one `use fs` line.
9. **§7: `File.write` returns `Error?`, not `(U64, IoError?)`** (§4.2).
   The count is only useful to a caller who loops, and no caller should loop.

**Of `strings-formatting-and-docs.md`:**

10. **§6.1: `Δt.square_root()` becomes `Δt.sqrt()`** (§8.2).
11. **§4.3's free-function list gains `read_line`**, making nine (§4.4).

**Of `scientific-libraries.md`:**

12. **§5.11: `def sqrt(x: F64) -> F64` is the Level 2 form.** The Level 1
    spelling is the method (§8.2). The mapping is mechanical and nothing in the
    catalogue's substance changes.

**Of `syntax-revision-2.md`:**

13. **§7.1's example: `read_file("a.txt")` becomes `read_file(Path.from("a.txt"))`**
    (§5.2), and its `load` function returns `(Doc, IoError?)` rather than
    `(Doc, Error?)` under §7.3.

**Of `reserved-words.md`:**

14. **The dot rule (ask 1) is now load-bearing for a core collection method**,
    `Set.union` (§3.3, §10). It was already independently correct; this adds a
    customer inside the closed core rather than in a library.

**Of `examples/README.md` and the corpus:** `String: Clone` (§6.2), `String: Add`
(§6.3), and the nineteen-method surface (§6.9) replace the corpus's recorded
reading. The two warts that note names — no `clone`, and `String.new()` +
`push_str` standing in for concatenation — are both retired.

**Diagnostics claimed:** `SC0257`, `SC0258`, `SC0259`, all in the types range and
all inside the block this note was allocated. No code outside `SC0257`–`SC0259` is
claimed; `SC0140`, `SC0275`, `SC0331` and the rest are cited, not reused.

| Code | Phase | Condition |
|---|---|---|
| `SC0257` | Types | A `String` passed where a `Path` is expected. Applicable fix: `Path.from(…)`. §5.2. |
| `SC0258` | Types | A `String` indexed with `[]`. Names the three views by their unit. §6.4. |
| `SC0259` | Types | An error of one concrete type returned where a different one is required. Names a `from_*` constructor on the target type when one exists. §7.7. |

---

## 12. Risks

**The Level 1 line is the ABI line, and it is asymmetric.** §8 says `science-rt`
carries "the representation of every library type", and `science-rt` is statically
linked into every binary. So a type that is Level 1 at 1.0 is frozen at 1.0 — its
layout, its method signatures, all of it. Moving something *out* of Level 1 later
breaks every program; moving something *in* later breaks nothing. The line should
therefore be drawn conservatively, and this note has tried to, but the failure mode
is one-directional and the pressure will always be to add.

**Thirteen free functions, and the trend is upward.** Five in §8, eight after
`strings-formatting-and-docs.md` §4.3, nine after §4.4 here, thirteen once
`data-io.md` §2's four whole-file functions are counted. Each addition was
individually justified and the total is now nearly triple the original. There is no
mechanism preventing the next note from adding two more, and the only real defence
is that §2.2's rule makes each one a spec change. Whether that defence holds is the
open question.

**`science-rt` binary size.** Level 1 puts float formatting, float parsing, a hash
function, UTF-8 validation and the elementary math set into every binary, including
hello-world. Dead-code elimination recovers most of it, but `print` pulls in float
formatting unconditionally through `Display`, and that is the largest single piece.
The Unicode tables are the reason grapheme segmentation is Level 2 (§1.5) and that
decision is worth roughly 100 KiB per binary; the rest has not been measured, and
should be before 1.0.

**The reproducibility gap in §8.3 is a promise Level 1 cannot fully keep.** §5.1 of
the core spec says results that differ between runs are opt-in. A Level 1 `sin` that
lowers to a target intrinsic can differ in the last bit between a laptop and a
cluster, and that is not opt-in. This is flagged rather than solved and it is the
most likely thing in this note to be found wrong later.

**The criterion is a filter, not a mandate, and the second gate is subjective.**
§1.3's "written by enough programs to earn a permanent global name" has no
threshold. Hex is at Level 2 because of it (§6.6) and a reviewer could reasonably
put it at Level 1. The mitigation is that §1.3 failures are cheap to reverse —
moving something *into* Level 1 later breaks nothing — while §1.2 failures are not.
So when the two gates disagree, the answer should always be Level 2.

**Moving `glob` and `TempDir` behind `use fs` will read as gratuitous** to anyone
who has not read §1, especially since `data-io.md` §7 makes `TempDir`'s ownership
story one of its best arguments. The mitigation is that the `use` line is one line
and the reason is written down; the risk is that it gets reverted by somebody who
finds the line annoying and does not find the reason.

**`String.length()` in bytes is a permanent surprise** with no compiler mitigation
available (§6.5). Both answers are legitimate, so no diagnostic can fire. This is
pure documentation debt and it will be paid by every Python user, once each.

**Five notes amended §8 in one day without seeing each other**, which is the same
failure mode the README's diagnostic-allocation section records for `SC0010`,
`SC0150`–`SC0159`, `SC0251` and `SC0255`. §2.2's two-level structure is an attempt
to make the next amendment visible rather than invisible, but the structure only
works if the Level 1 enumeration in §9 is maintained as a single table in the spec
rather than reconstructed from the notes. If it is allowed to live in the notes, it
will drift again, and it will drift the same way.
