# Concurrency: the mechanism, and cancellation

**Status.** Design. Nothing below is built.

**What this note is not.** It does not choose a concurrency model —
`stdlib-shape-and-packages.md` Decision 3 did that — and it does not restate the
surface, which is §3.6 of that note and §10 of `stdlib-standard.md`: `scope`,
`Thread`, `Mutex`, `RwLock`, `Channel`, a sequentially consistent `Atomic of T`,
and the marker interfaces `Share` and `ShareBorrowed`.

**What it is.** The mechanism underneath that surface, and one thing nobody
owns. Three questions, in order of how much trouble they cause:

1. **How does `scope` make a leaked thread and a data race into compile
   errors?** The surface implies it. No note says how, and the answer is a
   requirement addressed to `region-inference.md`, which has never been asked.
2. **How is a long-running computation cancelled?** Nothing anywhere answers
   this, and `mcp-servers.md` §18 is blocked on it.
3. **What happens when the program links something that has its own thread
   pool?** §3.6 says there is no second pool. Linking polars gives you one
   anyway.

---

## 1. Goroutines, and why the record already refused them

The request that prompted this note was for goroutines. The decision record
already answers it, in `stdlib-shape-and-packages.md`'s table of rejected
alternatives, and the reason given there is the right one:

> green threads tax the foreign call, which is this language's hot path

That is the whole argument and it is decisive for this project specifically. A
cgo call costs on the order of 50–100 ns, and it costs that **because** Go must
switch from the goroutine's small movable stack onto a system stack before C
runs. A language whose reason for existing is that the ML ecosystem's C is
usable cannot adopt the one feature that taxes every C call.

**One thing the record does not say, and it is the harder half.** Go can move a
goroutine's stack because it copies the stack and **rewrites every pointer into
it**, which it can do because its garbage collector knows precisely where every
pointer is. Science has borrows into stack frames and no GC. A `borrowed T`
pointing into a stack that moves is dangling the instant it moves, and nothing
in the language could find it. This is not an engineering difficulty; it is a
contradiction with §6.1 rule 5. It is also exactly why Rust removed green
threads in 2014 and why its `async` is stackless coroutines rather than a
scheduler with movable stacks.

**Decision 1. Growable, movable stacks are foreclosed permanently, not
deferred.** Every other decision in this note is downstream of it.

**The cost is already recorded and should not be softened:** *"Science is a bad
language for a network server."* An OS thread costs roughly 1 µs to start and 8
KB–1 MB of stack against a goroutine's ~0.2 µs and ~2 KB. At a hundred thousand
concurrent connections that difference decides the argument. At the tens of
concurrent requests a scientific tool server handles, it is not measurable. The
honest position is that Science is losing a contest it is not entering.

---

## 2. What `scope` buys that Go cannot have

§3.6 spells thread creation `scope.start(…)` rather than a free `spawn`, and
gives the reason as keeping `spawn` for F3. There is a better reason and it is
the point of this section.

**Decision 2. A thread's lifetime is bounded by a lexical scope, and the scope
does not return until every thread it started has finished.**

```science
def fit_all(runs: borrowed Array of Run) -> Array of Fit:
    let mutable out be (Array of Fit).new()
    scope:
        for run in runs:
            scope.start(giving: fit_one(run))
    # every thread has finished here; not by convention, by construction
    out
```

This is structured concurrency, and against Go it trades two things for one.

**What it costs.** A thread cannot outlive the function that started it. A
detached background worker — Go's most common pattern — has no spelling. F3's
actors are the answer for anything genuinely long-lived, and until F3 there is
no answer at all.

**What it buys, and Go cannot have either half.**

**Leaks stop existing.** The single most common complaint about Go's
concurrency is the leaked goroutine: nothing tracks whether one finished, a
blocked one is invisible, and the symptom is memory growth in production weeks
later. Under Decision 2 a thread that has not finished is a scope that has not
returned. There is nothing to leak.

**Data races become type errors, using machinery that already exists.** `Share`
and `ShareBorrowed` are already Level 1 marker interfaces. Rule 4 already says a
value is borrowed shared any number of times or exclusively once, never both.
A closure passed to `scope.start` captures borrows, and the same rule that
rejects two exclusive borrows in one thread rejects an exclusive borrow sent to
another. **No new analysis is required** — this is rule 4 applied across a
thread boundary instead of across a statement boundary.

That is the argument this project should make about concurrency, rather than
competing on goroutine counts: **a concurrent program that is wrong does not
compile.** For code whose failure mode is a silently corrupted result rather
than a crash, that is the trade a scientist should want.

---

## 3. What this requires from region inference

`region-inference.md` was written without knowing about this section, and
Decision 2 puts a requirement on it that nothing else in the project does.

`scope.start(giving: fit_one(run))` passes a closure that borrows `run`, which
borrows `runs`, which is the enclosing function's parameter. For this to be
sound, the borrow must be live for at most the scope's extent — not the
function's.

**Requirement 1, addressed to `region-inference.md`.** A `scope` block is a
**region boundary**. Every borrow captured by a closure passed to
`scope.start` must have a region contained in the scope's extent, and the scope
block's exit is a point at which those regions end.

That note's machinery handles this without extension: its §2 lattice is a set of
MIR points, and "contained in the scope's extent" is set inclusion against the
points between the scope's entry and exit blocks. What it needs is for MIR to
give the scope an explicit entry and exit block, which is a lowering
requirement, not a region one.

**Requirement 2.** A borrow may not cross a `scope.start` boundary *outward*.
A thread may not return a borrow of anything it created. This falls out of
Decision 2 — the thread's frame is gone when the scope returns — but it needs a
diagnostic of its own (`SC0381`) because the generic "borrow does not live long
enough" message would be about a frame the reader cannot see.

**Requirement 3, and this one is new work.** The captured environment of a
closure passed to `scope.start` must be checked against `Share` and
`ShareBorrowed`, which is a *type* question asked at a *region* boundary. §2
says no new analysis is needed for the race check itself, and that is true of
rule 4; it is not true of deciding which types may cross at all. `SC0382` is
"this type may not be shared between threads", and it must name the field that
made the containing type unshareable, not just the type — the useless version of
this diagnostic is Rust's `Rc<T> cannot be sent between threads safely` pointing
at a struct forty fields deep.

---

## 4. Cancellation, which nobody owns

`mcp-servers.md` §18 records progress and cancellation as having "no good answer
here", and calls it the strongest live argument against that note's own central
decision. Nothing in `stdlib-shape-and-packages.md`, `stdlib-standard.md` or
`effects.md` addresses it. This section is the answer.

### 4.1 Why the obvious answers are wrong

**Killing a thread** is out. There is no safe point to stop at, destructors do
not run, a `Mutex` held at the moment of death stays locked forever, and
`crates/science-rt`'s contract says a value is freed when told and only then.
Every language that shipped thread termination has deprecated it.

**Cancellation tokens in every signature** is what Go does with `context.Context`
as a first parameter, and it works because Go's I/O primitives all take one. Ours
block, by Decision 3 of the policy note. A token every function accepts and
almost none consults is noise with a false promise attached.

### 4.2 The decision

**Decision 3. Cancellation is cooperative, the flag is ambient rather than a
parameter, and the language defines exactly where it is observed.**

Ambient rather than a parameter for a reason that is not convenience:
`mcp-servers.md` decides that a `tool`'s JSON Schema is generated from its
signature, so **a cancellation parameter would appear in the schema**, where it
is meaningless to the model calling the tool. That note called this out and had
no way around it. Making the flag ambient is the way around it.

`effects.md` already has the machinery. It infers an `ambient` bit —
*"reads state supplied by the machine rather than by an argument"* — with a
witness chain. A cancellation check is exactly that, and reusing the existing
lattice means no new keyword and no new inference.

**Decision 4 — REVERSED. The observation point is wherever the author already
reports progress, and nowhere else. The compiler inserts nothing.**

This section first decided that the compiler would check at the top of every
loop body, on the grounds that the loop head is the point a long computation
passes through. `mcp-servers.md` §9.5 rejected that within the hour and it is
right. The reversal is recorded rather than edited away, because the first
answer is the one everybody reaches for and the reasons it fails are not
obvious.

**Why it fails.** A compiler-inserted check is a memory load and a branch in the
inner loop of *every numerical kernel in the language*, to improve a protocol's
user experience. `codegen-and-linking.md` §8 has since written down the
project's first performance target — C and Rust ±20% — and that target did not
exist when the first decision was made. A load in the hot loop is precisely what
misses it.

It is also **unpredictable in a way the author cannot see**: which loops receive
a check becomes a performance question with no syntax attached to it, so a
programmer tuning a kernel is tuning against an invisible decision.

And it is not what the protocol asks for. MCP's own model is cooperative —
*"a server is not obligated to actually stop the work; it is only obligated to
acknowledge the request"* — so a guarantee bought with a branch in every loop
buys more than the protocol wants.

**What replaces it, and it is better than a compromise.** `progress.report`
returns a `Bool`, and it is false when the call has been cancelled. Reporting
progress and observing cancellation become **one call site**, which means the
interruption point is the place the author already decided was a sensible place
to be interrupted. A loop with no progress reporting is a loop the author did
not think of as long-running, and it is uncancellable — which is a consequence
of what they wrote rather than of what the compiler guessed.

**The cost, unchanged and now more honest.** A tool whose body is one BLAS call
cannot be cancelled, and neither can one that never reports. §9.6 of that note
concedes a sharper version: CPython checks for signals between bytecodes, so a
pure-Python loop is *more* interruptible than a Science one. That is a real
place where this language is worse, and the trade is that the Science loop is
the one running at C speed.

**Decision 5. `cancellable` is inferred, not declared, and it propagates.** A
function that observes cancellation, or calls one that does, is `cancellable`.
A `tool` body that is not `cancellable` anywhere is a **warning**, not an error:
a tool that runs for two milliseconds needs no cancellation, and the compiler
cannot tell which is which. The warning is `SC0383` and it names the loop that
would have been the natural observation point.

**What happens on observation.** The function returns. Under the error model
this is `-> (T, Error?)` with a `Cancelled` error, which is the model working
rather than a special case. **`Cancelled` is an ordinary error type**, so a
caller may handle it, log it, or return it, and nothing about the control flow
is new.

**Cost, stated.** A numerical kernel that is one long loop with no calls gets one
predictable branch per iteration, which is real but small; a kernel that is a
single BLAS call gets no observation point at all and **cannot be cancelled** —
`dgemm` will run to completion. That is not a gap this language can close, since
the code is not ours, and the honest thing is to say so in the tool's
documentation rather than to pretend the flag is checked.

### 4.3 Progress

**Decision 6. Progress reporting is an ambient sink rather than a parameter, and
it is the same call that observes cancellation.** A parameter would otherwise
appear in a `tool`'s generated JSON Schema, where it means nothing to the model
calling the tool.

`progress.report(done, total, message)` returns a `Bool` that is false when the
call has been cancelled, so the two concerns share one call site — see the
reversal in Decision 4 for why that is the whole design rather than a
convenience. `mcp-servers.md` §9.3 places the sink under `stdlib-standard.md`
§8.2's existing logger exception, *"ambient state is acceptable exactly when it
cannot change the program's computed result"*, rather than taking a fourth
effect bit that `effects.md` Decision 2 has closed the set against.

**The message is not decoration.** A promoted long-running call becomes a task,
and the task extension has **no progress notifications at all** — only a
free-text status. So the fraction is the half that survives one path and the
message is the half that survives both.

---

## 5. The foreign thread pool, and a policy that cannot hold

`stdlib-shape-and-packages.md` §3.6 states: **"There is no second thread pool,
and none is exposed."** The reason is good — 256 threads on a 64-core shared node
is an administrative incident rather than a performance bug — and the policy
also says the pool's size defaults to the *allocation* (`SLURM_CPUS_PER_TASK`,
`OMP_NUM_THREADS`, a cgroup quota) rather than to `os.cpu_count()`.

**The policy is not enforceable and this note is the first to say so.**

Linking polars gives the process a second pool: `polars_core::POOL`, a
`LazyLock<ThreadPool>` created on first use. It is not configurable through
`rayon`'s global builder — `ThreadPoolBuilder::build_global()` does not affect
it, a fact this project has already verified. OpenBLAS and MKL each have a third
and fourth, sized from `OPENBLAS_NUM_THREADS` and `MKL_NUM_THREADS` or from the
core count when those are unset. A program that calls `.parallel()` over a chain
whose body calls into BLAS has `N × M` threads for `N` workers and `M` BLAS
threads each, on a node that allocated the program eight cores.

**Decision 7. The policy is restated as a budget that Science coordinates but
does not own.** Science's own pool is sized from the allocation; at startup the
runtime sets `OPENBLAS_NUM_THREADS`, `MKL_NUM_THREADS` and `OMP_NUM_THREADS` to
1 **if they are unset**, on the grounds that a library called from inside a
parallel chain should be serial; and the run record of `reproducibility.md`
records every pool it found and every variable it set.

**Cost.** Setting an environment variable at startup is a global side effect
that a library has no business performing, and it is wrong for a program that
calls BLAS from one thread and wants BLAS to use all cores. `effects.md`'s
`ambient` bit makes it visible, `--deterministic` can reject it, and it is opt-out
rather than opt-in because the default that oversubscribes a shared cluster node
is the worse failure.

**This contradicts §3.6's flat "no second pool" and the contradiction is the
point:** the policy describes a program that links nothing, and this language
exists to link things.

---

## 6. The MCP server, which is where all of this lands

`mcp-servers.md` decides that a `tool` is a declaration of a callable exposed to
a model, and its §18 leaves progress and cancellation open. This note closes
them, and the pieces fit as follows.

A scientific tool is long-running by nature — that note's own example is a peak
fit over a 40 GB run taking minutes. So:

- The handler runs on a thread started in a `scope` that the dispatch loop owns
  (§2), so a slow tool does not block the protocol stream.
- Cancellation arrives as `notifications/cancelled`, sets the ambient flag, and
  the handler observes it at a loop head (§4) and returns `Cancelled`.
- Progress reports travel the same way in reverse (§4.3).
- Neither appears in the generated JSON Schema, which is what makes
  `mcp-servers.md`'s central decision survive contact with long-running tools.
- The thread budget (§5) matters more here than anywhere, because a server
  handling three concurrent tool calls into polars is the exact shape that
  oversubscribes.

**One thing this does not solve.** The stdio transport forbids non-protocol
bytes on stdout, and `mcp-servers.md` §7 already has the compiler catching a
stray `print` (`SC0519`). With a handler on another thread, that check is now
**necessary rather than merely useful** — two threads writing to stdout
interleave, and the corruption is non-deterministic.

---

## 7. Diagnostics

Claimed: **`SC0381`–`SC0398`** in the Ownership range. `SC0300`–`SC0302`,
`SC0312`, `SC0330`–`SC0335`, `SC0340`–`SC0341`, `SC0379`, `SC0380` and `SC0399`
are claimed elsewhere and are not reused.

Named above: `SC0381` (a borrow escapes a thread outward), `SC0382` (a type that
may not cross a thread boundary, naming the field responsible), `SC0383` (a
`tool` body with no cancellation observation point — a warning).

---

## 8. Deliberately not built

- **Green threads, fibers, movable stacks.** Decision 1, permanently.
- **`async`/`await`.** The policy note's Decision 3. The words stay reserved.
- **Detached threads.** Decision 2. F3's actors are the answer and there is none
  before F3.
- **Relaxed memory orderings.** `stdlib-standard.md` §10.2 already refused them;
  nothing here reopens it.
- **Thread-local storage.** No caller, and it interacts badly with a pool whose
  threads are reused.
- **Cancellation of a foreign call.** §4.2. `dgemm` runs to completion.

---

## 9. Risks

**Decision 4 was wrong and lasted under an hour.** This section originally
warned that its own observation points were "a guess about where the cost is
acceptable" and named the fallback — observation only where the author asks for
it — as what to do if the branch showed up in a benchmark. It did not take a
benchmark: `mcp-servers.md` §9.5 rejected the guess on the argument, and
`codegen-and-linking.md` §8 then supplied the performance target that settles
it. The risk was correctly identified and the mitigation was correctly named;
what the section got wrong was betting against both of them. **The remaining
risk is the reverse one**: a body that never reports progress cannot be
cancelled at all, and nothing warns the author until a user tries.

**Requirement 3 is new analysis, presented as if it were not.** §2 claims races
need no new machinery because rule 4 already covers them. That is true of the
race; it is not true of deciding which *types* may cross a thread boundary, and
`SC0382`'s quality determines whether concurrency in this language is usable or
merely safe.

**§5 admits the language cannot enforce its own thread policy.** Setting
environment variables at startup is a hack with a good reason, and the first
user who wants BLAS to use all cores from a single-threaded program will find it
surprising. A better mechanism would be to call each library's own
`openblas_set_num_threads` through FFI — which this language can do, and which
requires knowing which library is linked, which `native-dependencies.md` knows
and this note does not.

**Nothing here is testable.** There is no code generator, so no Science program
has ever started a thread. Every number in §1 is from Go's and Linux's
documented behaviour rather than measured here.
