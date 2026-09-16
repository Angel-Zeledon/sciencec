# Science — Design: the standard library, level 2

Date: 2026-09-16
Status: draft for review
Depends on: `../specs/2026-09-16-science-f0-core-design.md` §2, §4, §5.4, §8, §9,
§13; `syntax-revision-2.md` (every signature below is written in it);
`ffi-c-boundary.md` (every C binding below is expressed in its mechanism, and no
other).
**Read first: `stdlib-shape-and-packages.md`.** It owns the levels, the
concurrency model, the naming conventions and the Level 3 package set, and it
says so. Where this note and that note differed, this note has moved; §0.3 lists
the three places it has not, with reasons.
Sibling: `stdlib-core.md` owns Level 1.
Reconciles rather than re-specifies: `scientific-libraries.md` §7.3 (`random`),
`data-io.md` §3 and §11.3 (`json`, `Instant`/`Duration`),
`collections-and-chains.md` §7 (parallelism),
`strings-formatting-and-docs.md` §5.5 (doc tests).
Answers: `script-mode.md` §10's two deferred items (`os.args`, `os.exit`);
`stdlib-shape-and-packages.md` §5.4's explicit prerequisite — *"there is no `net`
module in any note in this repository [...] It should be a note, and it is not
one this note can write."*

Diagnostic block: `SC0265`–`SC0270`, types and traits.

Scope: the modules that ship with the toolchain and must be imported — `time`,
`os`, `random`, `testing`, `logging`, `json`, `regex`, `thread`, `net` — and the
two the criterion pushes out of that level, `http` and `crypto`.

---

## 0. What this note owns, and the rule it applies

### 0.1 The levels, adopted from the note that owns them

`stdlib-shape-and-packages.md` §1.1 fixes the levels and grades them by
**distributability** rather than by usefulness. This note uses that scheme
unchanged:

| Level | What it is | How a user gets it |
|---|---|---|
| **1 — core** | Linked into every binary. `stdlib-core.md` owns it. | Nothing; it is the language |
| **2 — standard** | Ships in the toolchain tarball, `use`-able, not linked unless used. **This note.** | Nothing; it is in the tarball |
| **3 — official packages** | Separate artefacts, absent from a minimal toolchain. `stdlib-shape-and-packages.md` §5 owns the set. | An extra directory on `SCIENCE_PATH` |

An earlier draft of this note invented a fourth level and numbered the core zero.
That is withdrawn; three levels, numbered as above.

**The load-bearing fact, which that note states and this one repeats because two
of its modules land on it:** Level 3 has no distribution mechanism.
§6 of that note reduces "its own release cycle" to *"a Level 3 package may make a
breaking change in a release that does not make a breaking change to the
language"* — a deprecation cycle, not a distribution mechanism. So when this note
sends `crypto` and `http` to Level 3, it is sending them somewhere that is
currently a directory and a convention. §18 records that as the largest risk
here.

### 0.2 The criterion, and the two refinements it already has

> **The release-cycle rule.** If an algorithm may have to change for security or
> performance reasons without the language changing, it does not belong in the
> core — it belongs in a package with its own release cycle.

Both sibling notes reached the same correction to it independently and this note
adopts both rather than proposing a third:

- **`stdlib-core.md` §1.1**: *"Level 1 specifies observable behaviour. Where the
  algorithm is itself observable, the module is not Level 1."* Made mechanical by
  five questions in §1.2, of which question 5 — *does a fix have to outrun the
  compiler?* — is the one that decides most cases.
- **`stdlib-shape-and-packages.md` §5.6, Decision 5d**: *"no Level 1 or Level 2
  API's observable contract may name or imply an algorithm."* With three
  compliance rules: no documented output depends on the algorithm; anything with
  a CVE surface is replaceable at link time; anything whose algorithm is a moving
  target is named and versioned in its own API.

Two corollaries of my own do the remaining work, and both are ways a module that
violates the rule can still be shipped.

**Corollary A — a wrapper satisfies the rule that its contents violate.** Put the
changing part behind a boundary that updates on its own schedule. `crypto` is the
instance: libsodium's security releases arrive through the system package
manager, and Science's binding is a *name* for an algorithm rather than an
implementation of one. This is compliance rule 2, generalised.

**Corollary B — a design decision can retire half a violation.** `regex` violates
on security and on performance. A linear-time engine removes the security half
*by construction* — there is no exponential path to patch — leaving a permanent
capability gap rather than a recurring obligation. Converting a cadence problem
into a one-time capability loss brings a module inside the rule.

### 0.3 Where this note still disagrees with a sibling, and why

Three places, stated rather than silently diverged.

1. **The module is `testing`, not `test`.** `stdlib-shape-and-packages.md` §4.7
   lists `test` among the Level 2 module names, and §4.4's collision table
   assumes *"`assert(cond)` in a `test` module"*. §7 here makes `test` a **new
   reserved word and an item form**, which the author asked for directly. A
   keyword and a module cannot share a spelling — `use test (check)` would not
   parse — so one of the two must move. The item form is the thing that was
   asked for and the thing a naming convention cannot replace (§7.2), so the
   module is `testing`. This costs one letter and one row in §4.7's table.
2. **`crypto` does not bind BoringSSL.** Decision 5c of that note says libsodium
   *"and BoringSSL where RSA, X.509 or TLS interoperability is required."* §13.1
   here rejects BoringSSL on a mechanical ground that note did not have in front
   of it: Google states BoringSSL has no stable API or ABI and must not be used
   as a shared library, while `ffi-c-boundary.md` §5.1 makes dynamic linking the
   default *because* "HPC module systems work by swapping the shared object under
   a fixed name". A library that cannot be a shared object is disqualified by the
   FFI note's own linking model. The replacement proposed is OpenSSL's `libssl`
   under `when available`, for TLS only. Everything else in Decision 5c stands
   unchanged, including the decisive argument about constant-time code and LLVM.
3. **`random` is its own module, not `stats.random`.** §5.1 gives the reason:
   `stats` depends on `linalg` depends on BLAS, and a reproducible key must not
   require OpenBLAS. No name from `scientific-libraries.md` §7.3 moves.

Everything else here follows the policy note, including the parts that reverse an
earlier draft: the concurrency module is `thread` with that note's §3.6 surface,
`Mutex` has `with_lock` and not a guard, the marker interfaces are `Send` and
`Share`, regex lives at `text.regex`, and `crypto` is Level 3.

### 0.4 Syntax and drift

Every signature here is revision-2: `function … -> T`, `of` for generics,
`borrowed` / `mutable borrowed`, `is` / `is not` for equality, `> < >= <=` for
ordering, `print`, `interface`, `Type has:`, `T?`, and fallible functions
returning `-> (T, Error?)` with `?` as the presence test. Names follow
`stdlib-shape-and-packages.md` §4.1: `snake_case` functions,
`UpperCamelCase` types, acronyms capitalised as words (`HttpError`, `TcpStream`,
never `HTTPError`).

`ffi-c-boundary.md` and `scientific-libraries.md` predate revision 2 and write
`returns`, `Result of (T, E)` and `trait`. Per the README's conventions that is
drift, not disagreement; where this note quotes one of their signatures it
translates it and says so.

---

## 1. The verdict

Stability markers are `stdlib-shape-and-packages.md` §1.4's obligation 2: every
Level 2 module declares `stable`, `provisional` or `deprecated`, and
`provisional` means the compiler says so at the `use` site.

| Module | Level | Stability | Minimal surface | Science or linked C | Depends on | Release-cycle rule | Phase |
|---|---|---|---|---|---|---|---|
| `Instant`, `Duration` | **1** | — | Two types, I64 nanoseconds | Science over `clock_gettime` | — | Passes | F0 |
| `time` | 2 | stable | `Monotonic`, `Civil`, RFC-3339 | Science | Level 1 | Passes | F0 |
| `time.zones` | **3** | — | `Zone`, `Zoned`, tzdata | Science + tzdata | `time`, `os` | **Violates — evicted** | — |
| `os` | 2 | stable | `args`, `exit`, `env`, `run`, signals, machine facts | Science over a per-platform `extern` block | `fs`'s `Path` | Passes | F0 |
| `random` | 2 | **stable, and frozen** | `Key` (splittable) and `Stream` (sequential) | **Science. Never linked.** | Level 1 only | Passes *by being frozen* | F0 |
| `testing` + the `test` item | 2 + language | provisional | `check*`, `check_snapshot` | Science | `Inspect`, `os` | Passes | F0 |
| `logging` | 2 | provisional | five levels, one `Sink` interface, one sink | Science | `time`, `os`, `io` | Passes | F0 |
| `data.json` | 2 | — | **`data-io.md`'s, not this note's** | Science | `fs`, `text` | (theirs) | F0 |
| `text.regex` | 2 | provisional | `Pattern`, `find`, `matches`, `replace_all` | **Science**, linear-time engine | `text` | **Violates on performance; security half retired** | F1 |
| `thread` | 2 | provisional | `stdlib-shape-and-packages.md` §3.6, plus `Atomic` and `RwLock` | Science over pthreads / Win32 | `os` | Passes; its *scheduler* would not, so there is none | F2 |
| `net` | 2 | provisional | `TcpListener`, `TcpStream`, `UdpSocket` | Science over the platform sockets API + `getaddrinfo` | `thread`, `time` | Passes | F2 |
| `http` | **3** | — | HTTP/1.1 **client only** | Science over `net` + `crypto.tls` | `net`, `crypto`, `data.json` | **Violates** | F2 |
| `crypto` | **3** | — | hash, MAC, AEAD, signature, password, CSPRNG, TLS | **Linked: libsodium**; TLS: OpenSSL `when available` | `ffi`, `os` | **Violates — Corollary A is the answer** | F2 |

Four things violate the rule: **`crypto`, `http`, `text.regex` and
`time.zones`.** Three of the four leave Level 2 because of it. §14 collects them.

---

## 2. Names against §13 and the collision tables

`stdlib-shape-and-packages.md` §4.4 is the consolidated collision list and this
note does not duplicate it. What follows is only the rows this note's names touch,
plus the two it adds.

| Name wanted | Status | Verdict | What this note does |
|---|---|---|---|
| `time`, `os`, `env`, `random`, `logging`, `net`, `http`, `crypto`, `testing` | free | safe | As written |
| `match` | **in use** | member-position collision | `text.regex` uses `matches` / `find`. Chosen by `scientific-libraries.md` §3 and confirmed by §4.4 of the policy note. Not dependent on the dot rule. |
| `assert` | **reserved, not used** | free-function collision | **Not taken.** `reserved-words.md` §4.2 keeps it for a verification *construct*; `testing` uses `check` (§7.4). |
| `signal` | free, **but owned by a sibling** | module-name collision | `scientific-libraries.md` §9 owns `signal` for DSP. POSIX signals are `os.signals`, type `os.Signal`. Plural, deliberately: a program that analyses a signal and handles a signal is an instrument control loop, not a contrived case. |
| `send`, `receive` | **reserved** (F3) | member position | `Sender.send` / `Receiver.receive`, per policy §3.6. **Depends on the dot rule.** Fallback: `put` / `take`. |
| `spawn` | **reserved** (F3) | free-function position, unreachable | Not taken. `Thread.start(…)`, per policy §4.4's own recommendation. |
| `async`, `await` | reserved | — | Not taken, and §11 explains that Decision 3 means they never will be. |
| **`test`** | **not on any list** | **a new reserved word** | §7 takes it and §7.3 prices it. **One word added to §13.** |
| `info`, `debug` | free | violates policy §4.1's no-abbreviation rule | §8.1 asks for both to be added to that rule's closed list, as the universal logging vocabulary. Named rather than quietly broken. |
| `timeout:`, `into:`, `tolerance:`, `over:`, `running:`, `named:`, `because:`, `minimum:` | free | argument labels | All free, so this note does not depend on policy §4.3's **label rule** — but it endorses it, because `with:` and `on:` are the natural labels for two of the signatures below and were renamed to avoid them. |

Boolean questions follow policy §4.1's `is_*` / `has_*` / `can_*` rule, which is
why `os.signals.is_interrupted()` and `logging.is_enabled(level)` are spelled the
way they are.

One naming hazard is not a reserved-word hazard and neither collision table will
catch it: **`random.Key` beside a cryptographic key.** §13.4 handles it with four
cumulative mechanisms and `SC0265`.

---

## 3. `time`

### 3.1 Decision: the minimum goes to Level 1, calendars are Level 2, zones are evicted

`data-io.md` §11.3 asks for "`Instant` and `Duration`, minimal: I64 nanoseconds,
UTC, no calendars" and §5.2 of that note calls dates "the biggest single gap in
the CSV story". That ask is granted exactly as written, and granted **into Level
1**, because `data` is below `time` in the dependency order and a CSV reader
cannot `use` a module that depends on it.

Level 1 is `stdlib-core.md`'s, and its §9 table does not yet have a row for time.
So this is an ask of that note (§16) rather than a decision taken here.

**Decision.**

- **Level 1**: `Instant` — I64 nanoseconds since the Unix epoch, UTC, no calendar
  — and `Duration` — signed I64 nanoseconds. Both pass `stdlib-core.md` §1.2's
  five questions cleanly: the epoch is pinned, there is no adversary, no
  performance product, no variants, and no fix that must outrun the compiler.
- **Level 2, `time`**: `Monotonic`, `Civil`, RFC-3339 and ISO-8601 parsing and
  formatting, and the clock functions.
- **Level 3, `time.zones`**: `Zone`, `Zoned`, local-time conversion, the IANA
  database. **Not shipped.**

### 3.2 Why the tz database is the criterion's clearest case

The IANA time-zone database releases several times a year — 2024a, 2024b, 2025a —
and it releases *because a parliament moved a daylight-saving boundary*. The
change has nothing to do with Science, arrives on a foreign schedule, and is data
rather than code. It is `stdlib-core.md` §1.2's question 5 with the answer as loud
as it gets.

If `time` embedded tzdata, "which time-zone rules does this binary believe"
becomes "which `sciencec` built it". A result computed in 2027 with `sciencec` 1.2
and recomputed with 1.3 would differ, for a reason invisible in the source text.
That is precisely the reproducibility failure §1 of the core spec exists to
prevent, arriving through the back door as a dependency-management failure rather
than a language one.

**Decision: zones are Level 3 and do not ship.** When `time.zones` exists it
carries its tzdata version in its own version number, so `time.zones-2026a` is
something a paper's methods section can cite. That is compliance rule 3 of
Decision 5d, applied to data instead of to an algorithm.

**Rejected alternative — read the system database at run time**
(`/usr/share/zoneinfo`, as Go's `time` and Rust's `jiff` do by default). It is the
right *default for a package*, because the OS update cadence handles it and no
stale copy is carried. It is the wrong thing at Level 2: the file is absent on
Windows and in most containers, so the module would be platform-conditional in a
way nothing else at Level 2 is; and a program needing *reproducible* zone
behaviour must pin the data, which is a package manager's job. When `time.zones`
ships, the system database is its default and a vendored copy is its opt-in.

**Cost, stated.** Until Level 3 exists, a Science program cannot convert a
timestamp to local time. That is real and it will be the most-reported missing
feature in this note. The mitigation is that essentially all scientific data is
timestamped in UTC or at a fixed offset, both of which `Civil` and a `Duration`
handle; and that the alternative's failure mode — silently wrong local times,
three years after the binary was built — is worse than the absence.

### 3.3 Monotonic time is a different type, and that is the point

`Instant` is a wall clock. It jumps: NTP steps it, a user sets it, a VM resumes
with a different one. Subtracting two `Instant`s to measure elapsed time is the
most common timing bug in every language that allows it, and it produces negative
durations in production a few times a year.

**Decision: `Monotonic` is a separate type with no conversion to or from
`Instant`.** It can be subtracted from another `Monotonic` to give a `Duration`
and it can do nothing else — no epoch, no calendar, no `Display` beyond a
debugging form. Comparing or subtracting across the two kinds is `SC0267`.

This is §1 of the core spec applied to a two-line library decision: the mistake is
caught before the run starts, at the cost of one extra type.

**Leap seconds.** `Instant` counts nanoseconds since the epoch *ignoring* leap
seconds, POSIX-style. An `Instant` difference across a leap second is therefore
not true elapsed SI time, and is off by one second. This is documented rather than
fixed, because fixing it requires a leap-second table, which is data on a foreign
cadence, which is §3.2 again. **`Monotonic` is the answer for anyone who needs
elapsed SI time**, and that is a second reason the type exists.

### 3.4 What is deliberately absent

- **`strftime`-style format strings.** A date format mini-language is a
  mini-language, with its own parser, its own locale question and its own bug
  cadence — and it fails `stdlib-core.md` §1.2's question 4 on variants alone.
  `time` offers `parse_rfc3339` and `format_rfc3339`. A general formatter is
  Level 3, and `strings-formatting-and-docs.md` §2 owns the one format
  mini-language this language gets.
- **Locales.** No month names outside English.
- **Calendar-unit durations.** `Duration` is nanoseconds. "One month" is not a
  duration, because its length depends on which month; `Civil.add_months(n)`
  exists and `Duration.months()` does not.

### 3.5 Written or linked

**Written**, over one small `extern` block per platform: `clock_gettime` with
`CLOCK_REALTIME` and `CLOCK_MONOTONIC` on Unix,
`GetSystemTimePreciseAsFileTime` and `QueryPerformanceCounter` on Windows. The
civil-calendar arithmetic is the proleptic Gregorian days-from-civil algorithm —
forty lines of integer arithmetic, published, and exactly
`scientific-libraries.md` §2's *"write where the artefact is a formula"*.

### 3.6 Five signatures

```science
## The current wall-clock time, UTC. Jumps when the system clock is set.
def now() -> Instant

Monotonic has:
    ## A reading from a clock that does not go backwards. Has no epoch.
    def now() -> Monotonic

    ## Elapsed time between two readings. The only thing a Monotonic does.
    def since(self, earlier: Monotonic) -> Duration

Instant has:
    ## Proleptic Gregorian civil time in UTC. Total: every Instant has one.
    def to_civil_utc(self) -> Civil

## RFC-3339 with a mandatory offset. The one text format `time` knows.
def parse_rfc3339(text: borrowed String) -> (Instant, TimeError?)
```

### 3.7 Example

```science
use time (Instant, Monotonic, now)

def main():
    let started_at be now()
    let clock be Monotonic.now()

    let frame be simulate(1_000_000)

    let elapsed be Monotonic.now().since(clock)
    print(f"started {started_at.to_civil_utc()} UTC")
    print(f"{frame.count()} steps in {elapsed.seconds():.3f} s")
```

---

## 4. `os` and `env`

### 4.1 Decision: one module, and `script-mode.md`'s two deferrals are answered

`script-mode.md` §10 leaves two things open and names this note's territory as
their home:

> **Command-line arguments.** A script that wants `argv` has nothing here. It is
> a library question, and it belongs wherever `data-io.md`'s successor puts the
> process environment.

**Decision: `os.args()` and `os.exit(code)` live in `os`, at Level 2.** Neither
joins the Level 1 free-function list, which is what `script-mode.md` §10
explicitly asked not to happen and what `stdlib-core.md` §9 identifies as the
surface most likely to stop being small. A script that wants an exit code writes
`use os (exit)`.

### 4.2 Decision: writing the environment is `unsafe`

`setenv` is not thread-safe on POSIX. It reallocates the `environ` array while any
other thread may be reading it through `getenv`, and there is no lock. This is not
theoretical: it is why Rust made `std::env::set_var` unsafe in its 2024 edition,
after a decade of it looking safe. `stdlib-shape-and-packages.md` Decision 3 puts
real OS threads in the language, so the hazard is live rather than hypothetical.

**Decision: `os.env.set` and `os.env.unset` are `unsafe def`s**, in
`ffi-c-boundary.md` §3.3's exact sense — the compiler cannot check the claim, so
the caller signs for it. Reading is safe.

This is the one place in this note where an existing language mechanism is reused
for something that is not FFI, and it is the right reuse: §3 of that note defines
`unsafe` as the half of a claim that does not live in Science, and "no other
thread is reading the environment right now" is exactly such a claim.

**Rejected alternative — a `Mutex`-guarded environment inside `science-rt`.** It
does not work: the lock would be Science's, and `getenv` calls from inside a
linked C library do not take it. A lock that only half the callers respect is
worse than no lock, because it makes the race look handled.

**Windows.** Environment names are case-insensitive on Windows and case-sensitive
on Unix. `os.env` does not paper over this; `get` uses the platform's rule and
says so, and a program needing portability normalises names itself. Papering over
it would mean a lookup that succeeds on one platform and fails on the other for a
reason invisible in the source.

### 4.3 Decision: signals are polled, never handled

A POSIX signal handler runs on a borrowed stack, at an arbitrary instruction, and
may call only async-signal-safe functions. Running Science code there is
`ffi-c-boundary.md` §4.4's registered-callback hole in its worst form: a callback
installed once, invoked from a program point nothing names, with no
unregistration, and with the interrupted frame's borrows live.

**Decision: `os` offers no handler registration. It offers
`os.signals.is_interrupted() -> Bool`**, which installs a minimal C-level handler
on first call — one that sets a `volatile sig_atomic_t` and returns — and then
reports it. A long simulation checkpoints on Ctrl-C by testing the flag at the top
of its loop.

```science
loop:
    if os.signals.is_interrupted():
        checkpoint(state, "interrupted.npy")
        break
    state be step(state)
```

**Rejected alternative — arbitrary handlers, as Python's `signal.signal`
offers.** Python gets away with it by *not* running the handler in the signal
context: it sets a flag and runs the Python callback at the next bytecode
boundary. Science has no bytecode boundary and no interpreter loop to hook, so the
equivalent is a check inserted at every backward branch — a codegen feature with a
measurable cost on every loop in the language, paid by every program to serve a
rare one. Refused.

**Cost.** A program that must react to `SIGTERM` within milliseconds rather than
at the next loop iteration cannot. That is a service's requirement, and §12
declines to serve services for the same reason.

### 4.4 Surface

`args`, `exit`, `env.get`, `env.all`, `unsafe env.set`, `unsafe env.unset`, `run`,
`working_directory`, `set_working_directory`, `hostname`, `process_id`,
`platform`, `signals.is_interrupted`.

There is no `os.cpu_count`. `stdlib-shape-and-packages.md` §3.6 puts
`Thread.available_parallelism()` in `thread`, and a second spelling of the same
machine fact in a second module is exactly the duplication §1.4 of that note
warns about. `os` reports who and where the process is; `thread` reports how wide
the machine is.

`run` is the minimum subprocess surface: run a program to completion, collect its
output. No pipes, no streaming, no shell. A scientist calling an external tool
wants exactly this, and everything past it is a process-management library.

### 4.5 Written or linked

**Written**, over a per-platform `extern` block against `library "c"` on Unix and
the Win32 imports on Windows. `os` is the only module here whose implementation
differs structurally by platform, and that is inherent: it is the module whose job
*is* the platform.

### 4.6 Five signatures

```science
## The command line, including argument zero. Answers `script-mode.md` §10.
def args() -> Array of String

# in module os.env
## Null when the variable is unset. An empty variable is set and empty.
def get(name: borrowed String) -> String?

# in module os.env
## Unsafe because `setenv` races any concurrent `getenv`, including one
## inside a linked C library, and Decision 3 gives us threads. §4.2.
unsafe def set(name: borrowed String, value: borrowed String)

## Run a program to completion and collect its output. No shell, no pipes.
def run(program: borrowed Path, arguments: borrowed Array of String)
        -> (Output, OsError?)

# in module os.signals
## Polled, never handled. `is_` per the policy note's §4.1 rule. §4.3.
def is_interrupted() -> Bool
```

### 4.7 Example

```science
use os (args, exit)
use os.env

def main():
    let root be os.env.get("EXPERIMENT_ROOT")
    if root is null:
        print("set EXPERIMENT_ROOT to the data directory")
        exit(2)

    let inputs be args().iterate().skip(1).collect()
    for name in inputs:
        process(Path.of(root).join(name))
```

---

## 5. `random`

This is the decision that cannot be changed later without breaking the
reproducibility of every result computed in the language, and the section is
written at that weight.

### 5.1 What is already decided, and what is reconciled

`scientific-libraries.md` §7.3 specifies `Key`, `split`, `split_many` and
`fold_in`, and §2 of that note rules that the generator is **written in Science,
never linked**, because §1 of the core spec names *"random keys that cannot be
reused"* as a motivating verification case and that property is unavailable if the
generator is C state behind a pointer.

`stdlib-shape-and-packages.md` §5.6 then names the tension honestly — the
criterion says a PRNG is an algorithm that changes for performance — and resolves
it: *"the guarantee is about the type, and the criterion is about the algorithm
[...] the module should expose the generator as a named, versioned choice
(`Philox4x32_10`) rather than as 'the' generator."*

None of that is reopened. §5.5 here is the same resolution made mechanical. Two
things are reconciled.

**Reconciliation 1 — `random` is promoted out of `stats`.** §7.3 puts it inside
`stats`, and `stats` depends on `linalg`, which depends on BLAS and LAPACK through
the FFI. A program wanting a reproducible key would then need OpenBLAS installed.
That is the same argument `scientific-libraries.md` §1 makes for its own
`physics.units` — *"it must be usable from `chem` and `bio` without dragging in
BLAS"* — applied to a module with a stronger claim on it. **`random` becomes its
own Level 2 module depending on Level 1 and nothing else**, and `stats`'s
distribution samplers take a `random.Key`, which is what §7.3's own five
signatures already show them doing. No name from §7.3 moves.

**Reconciliation 2 — an inconsistency inside §7.9, named rather than inherited.**
That section writes

```
    def sample(self, key: borrowed Key) returns Self.Sample
```

and, four lines below, the comment

> The key is taken by value, not borrowed: consuming it is what makes reuse a
> compile error rather than a convention.

Both cannot be true. A `borrowed Key` can be used twice, which is the exact bug
the design exists to prevent. **`Distribution.sample` must take `key: Key` by
value**, and §16 files that as a correction to `scientific-libraries.md` rather
than silently writing a different signature here.

### 5.2 Decision: two faces, two vocabularies, no bridge

**Decision.** `random` exposes two generators. They share no function name, no
type, and no conversion.

| | `Key` | `Stream` |
|---|---|---|
| What it is | A splittable counter-based key | A sequential generator |
| Ownership | Neither `Copy` nor `Clone`; every operation consumes it | Owned, mutated in place, passed `mutable borrowed` |
| Reuse | `SC0301`, use after move | Legal and meaningless — a stream is *meant* to advance |
| Reproducible | Bitwise, on any machine, forever | Bitwise for a given seed and a given call order |
| Parallel | Split, no communication, no order dependence | Not splittable. One stream, one thread. |
| Draw functions | `uniform(key)`, `normal(key, …)`, `shuffle(key, …)` | `uniform_from(stream)`, `normal_from(stream, …)`, `shuffle_from(stream, …)` |
| Construction | `Key.from_seed(seed)` only | `Stream.from_seed(seed)` or `Stream.from_entropy()` |

**No function in `random` is overloaded across the two faces.** `uniform` takes a
`Key` and `uniform_from` takes a `Stream`. The `_from` suffix is slightly awkward
on purpose: it is a speed bump at every call site that says which generator the
reader is looking at. A shared name with an overload would put that information in
the type checker instead of on the screen, which for a reproducibility-critical
distinction is the wrong place.

### 5.3 Decision: how the convenient face avoids becoming `numpy.random.seed`

The hazard `numpy.random.seed` became is worth stating precisely, because the
usual diagnosis is wrong.

The problem is **not** that NumPy has a seeding function. The problem is that
`np.random.uniform()` *works without one*. A process-global generator exists,
seeded from entropy at import, and every draw in the program — including draws
inside libraries the author never read — advances it. A result is therefore a
function of the entire program's call order, including code that is not the
author's, and "seed it and you are fine" is false the moment a dependency draws
from the same global.

**Decision. There is no ambient generator.** `random` has no module-level state,
no `random.seed(…)`, and no free `random.uniform()` that takes no generator. Every
draw names its generator because the generator is an argument. The convenience
face is convenient in that it is *sequential and mutable* — it does not make the
caller thread a value through a computation the way `Key` does — not in that it is
invisible.

Three properties make this hold rather than being a style rule:

1. **A `Stream` is created from a named origin, and only two origins exist.**
   `Stream.from_seed(seed)` is reproducible, and the seed is a literal or a value
   from the run's configuration. `Stream.from_entropy()` is not reproducible, and
   **the name says so at the call site** — which is the entire difference from
   NumPy, where non-reproducibility is the default and is spelled by the *absence*
   of a call.

2. **`mutable borrowed random.Stream` in a signature is a compiler-checked
   declaration that a function is not deterministic.** A function that draws must
   say so in its type, because it cannot reach a generator any other way. This is
   an effect discipline obtained for free from §6's borrows, in a language whose
   §1 names an effect discipline as part of what it is betting on, and it is
   something NumPy structurally cannot offer.

3. **Two call sites cannot draw from one `Stream` at once.** Rule 4 of §6.1 —
   exclusive borrow, once — makes the aliasing that produces interleaved draws a
   compile error rather than a heisenbug. Under Decision 3's threads this matters
   more, not less: a `Stream` is not `Share`, so it cannot be drawn from by two
   threads at all.

**Decision: library code takes a `Key`, never a `Stream`.** A library function
that draws from a caller's `Stream` makes the caller's reproducibility depend on
how many times the library happened to draw, which is a version-to-version
implementation detail. A library taking a `Key` composes, because a split key is
independent of every other key by construction. This is checkable — a `public`
signature with a `mutable borrowed Stream` parameter — and it is `SC0270`, at
warning severity.

### 5.4 The rule for which one to reach for

One question decides it:

> **Will anyone ever need to obtain this exact number again?**

- **Yes → `Key`.** Anything whose output is a figure, a table, a fitted parameter,
  a benchmark number, a simulated dataset, a train/test split, or an
  initialisation. Anything that appears in a paper, a report or a regression test.
  Anything a reviewer might ask to see reproduced.
- **No → `Stream`.** Randomness incidental to the result: a jittered retry delay,
  a tie-break, a random probe in an interactive exploration, a shuffled order in a
  throwaway script, a fuzz input in a test that reports its own seed on failure.

The asymmetry is deliberate: the reproducible face is the default answer and the
convenient face is the exception, which is the reverse of every language the
audience is coming from. The documentation states the rule in that order and the
module's first example uses a `Key`.

### 5.5 Decision: the algorithm is frozen, and a new algorithm is a new type

This is what makes an irreversible decision survivable, and it is
`stdlib-shape-and-packages.md` §5.6's resolution made concrete.

**Decision.** These are part of the language's published contract, not
implementation details:

- **The generator, named in the type.** The recommendation is
  **Threefry-2x64-20**, because 64-bit words suit an audience computing in `F64`
  and because it is what JAX uses, so the test vectors already exist to check
  against. That note's example spells `Philox4x32_10`; the *naming convention* is
  the binding decision and the specific algorithm is not, so either is acceptable
  and the name must record which.
- **The counter layout** — which bits are the counter and which the path.
- **`split`'s exact derivation**, including the order of the two returned keys.
  `let a, b be k.split()` must give the same `a` on every machine, forever.
- **The bits-to-float mapping**: 53 bits, scaled by 2⁻⁵³, half-open `[0.0, 1.0)`.
- **A set of test vectors**, shipped from the first commit and checked by the
  suite.

Changing any of them changes every number every Science program has ever computed.
Therefore:

> **A new generator is a new type, never a new version of `Key`.**

A successor arrives as `random.KeyPhilox4x32_10` with its own functions, and
existing programs are untouched. `Key` records its algorithm identity in its
representation, so a serialised key cannot be read back by a different algorithm.
That is compliance rule 3 of Decision 5d, discharged.

**A consequence easy to get wrong: `normal` uses the inverse CDF, not Box–Muller
and not the ziggurat.** A rejection method consumes a variable number of random
words per draw, so the value depends on the rejection history, which destroys the
property that a key's output is a pure function of the key. The inverse CDF
consumes a fixed count. It is slower and less accurate in the far tail, and that
cost is accepted for the same reason JAX accepts it: a splittable generator whose
output depends on history is not splittable.

The `Stream` face has no such freeze. Its generator may be replaced, because
nothing built on it was meant to be reproduced — and §5.4's rule is what makes
that safe.

### 5.6 The criterion applied

`random` **passes, but not in the usual way.** The rule asks whether the algorithm
may have to change for security or performance. For `Key` the answer is that it
must be *forbidden* to change, which is stronger than the rule requires.
Performance improvements arrive as new types; security is not `random`'s job at
all, which is why `crypto` is a separate module with a separate API (§13.4).
`stdlib-shape-and-packages.md` §5.5 states the same boundary from the other side:
*"`stats.random` is **not** a CSPRNG and must never be used as one [...] the
documentation of each should name the other and say so."* §13.4 does more than
name it.

### 5.7 Five signatures

```science
Key has:
    ## The only way to make a key. There is no key from entropy: a key
    ## whose seed nobody recorded is a result nobody can reproduce.
    def from_seed(seed: U64) -> Key

    ## Consumes `self`. Reusing it afterwards is SC0301, and that is the
    ## whole verification claim of core spec §6.5.
    def split(self) -> (Key, Key)

## Translated from `scientific-libraries.md` §7.9 into revision-2 syntax.
## The key is taken by value. Inverse-CDF, per §5.5.
def normal(key: Key, mean: F64, standard_deviation: F64) -> F64

Stream has:
    ## Not reproducible, and the name is the warning. §5.3.
    def from_entropy() -> Stream

## `_from` names the sequential face at every call site, on purpose. §5.2.
def uniform_from(stream: mutable borrowed Stream) -> F64
```

### 5.8 Example

A bootstrap: one key per resample, split without communication, and the same
answer on every machine.

```science
use random (Key, uniform, normal)

def bootstrap(sample: borrowed Array of F64, key: Key, draws: Int)
        -> Array of F64:
    let keys be key.split_many(draws)
    keys.iterate()
        .map(k giving resample_mean(sample, k))
        .collect()

def main():
    let means be bootstrap(measurements, Key.from_seed(20260916), 10_000)
    print(f"bootstrap 95% CI: {quantile(means, 0.025):.4f} .. {quantile(means, 0.975):.4f}")
```

---

## 6. `json`

### 6.1 Decision: deferred, not specified

`data-io.md` §3 ranks JSON third of the formats it admits, budgets two weeks,
routes it through Science rather than a binding, and §2 of that note places it at
`data.json` alongside `data.csv`, `data.npy`, `data.safetensors` and
`data.parquet`. `stdlib-core.md` §9 carries the same row.

**Decision: there is no Level 2 module called `json`. `data.json` is it, it is
`data-io.md`'s to own, and this note specifies nothing about it.** A request for
"a JSON module" is answered with `use data.json`.

The placement is not arbitrary: JSON in this language is a *data format*, read
from and written to a `Path`, and it belongs with the other formats under the same
`Frame`/`Rows` abstractions. A separate `json` module would duplicate the reader,
the error type and the bad-record policy `data-io.md` §5.3 already designs.

### 6.2 What this note adds — three dependencies and one question

**Dependencies, recorded so `data.json`'s owner knows who is downstream.**
`http` (§12) for request and response bodies; `models-and-inference.md` for
safetensors headers and tokenizer files; and `logging` — *if* a structured sink is
ever added, which §8 declines to do.

**One question, which is `data-io.md`'s to answer and is filed in §16.** JSON
numbers have no type. `2^53 + 1` round-trips through an `F64` as `2^53`, and every
JSON library in existence has made a different choice. `data.json` must say, in
its specification and not in its implementation, what it does with an integer
literal too large for an `F64` and whether `1` and `1.0` are distinguishable.
`stdlib-shape-and-packages.md` §5.6 already lists `data.json`'s number parsing as
a criterion violation; this is the half of it that must be pinned as a contract
rather than left to the parser.

---

## 7. `testing`

### 7.1 Decision: a `test` item form, and ordinary functions for the assertions

**Decision.** "Built into the language" means exactly one language change:

```science
test "pooled deviation of two equal samples is their common deviation":
    let s be pooled_deviation(2.5, 8, 2.5, 12)
    testing.check_close(s, 2.5, tolerance: 1e-12)
```

`test` is a new item form and a new reserved word. Everything else — the
assertions, the report, the snapshot comparison — is an ordinary Level 2 module
called `testing`, written in Science, with no compiler knowledge.

The module is `testing` rather than `test` because the keyword takes the shorter
spelling; §0.3 records that as a divergence from
`stdlib-shape-and-packages.md` §4.7's module list.

### 7.2 Why an item form, against the two alternatives

**A naming convention (`def test_something()`) — rejected.** A convention is
not checkable. Go has exactly this, and `func testFoo(t *testing.T)` with a
lowercase `t` compiles, links, and silently never runs; so does a signature with
the wrong parameter type. The failure mode of a testing mechanism is a test that
does not run, and it is the one failure mode a testing mechanism must not have. A
convention cannot rule it out, because the compiler has no way to know a function
*meant* to be a test.

**An attribute (`@test`) — rejected, for a structural reason rather than a taste
one.** **Science has no attribute syntax at all.** §12 of the core spec reserves
macros and does not implement them, and there is no annotation grammar anywhere in
§4. Inventing one to get tests is a far larger language change than one item form:
it needs a lexical form, a placement rule, a resolution story, and a decision on
extensibility — which is the derive question the README's standing-asks table
already has two customers for. Whoever lands derive should land attributes; until
then an item form is strictly the smaller change.

**An item form — taken.** It costs one reserved word and buys four things a
function cannot:

1. **The name is a string, not an identifier.** Test names are sentences, which is
   what makes a failure report readable, and it kills the
   `test_handles_empty_input_when_the_header_is_missing` naming genre outright.
2. **Tests do not enter the release binary.** The item is skipped unless
   `sciencec test` is the driver. A function would have to be dead-code eliminated
   and its transitive dependencies — a snapshot corpus, a fixture directory —
   would still link.
3. **The compiler knows it is a test**, which is what lets `testing.check*` be
   restricted to test context (`SC0269`). An assertion that silently no-ops in a
   release build is the second-worst failure mode available, and this rules it out
   by making the call illegal instead.
4. **Each test is an entry point**, so region inference has a root and ownership
   diagnostics inside a test point at the test's own spans.

### 7.3 The cost of the new reserved word, stated

`reserved-words.md` proposes removing eight words and this note adds one back, on
the day of that proposal. The bill:

- **Binding position breaks.** `let test be …` and a parameter named `test` stop
  compiling. In the target audience's vocabulary this is much rarer than `model`
  or `shape`: a scientist's variable is a `trial`, a `run`, a `case` or a
  `sample`, and "test" is overwhelmingly part of a longer identifier.
- **`scientific-libraries.md` §7.4 is unaffected.** `t_test_two_sample`,
  `chi_squared_test`, `binomial_test` and the rest are single identifiers under
  the lexer's maximal munch. Not one name in the statistics catalogue collides.
- **Member position is safe** under `reserved-words.md` §0.1's dot rule
  (`suite.test(…)`), and safe without it for every name this note uses.
- **Declaration position is unambiguous.** `test` is followed by a string literal,
  which no other item form is, so it could be **contextual** rather than reserved
  if §13 prefers — `reserved-words.md` §3.2 prices the contextual version at one
  match arm. Either is affordable and the choice belongs to §13's owner.

### 7.4 `assert` is not taken, and the two words mean different things

`assert` is reserved and `reserved-words.md` §4.2 keeps it deliberately, for a
*construct* the compiler can sometimes discharge statically:

> An `assert` that is a language construct is a place where the compiler can
> sometimes discharge the condition statically — and where, when it cannot, it can
> record the assumption for the region and shape machinery to use.

**Decision: `testing` does not take `assert`.** Its assertions are `check`,
`check_equal`, `check_close`, `check_failed`, `check_ok`, `check_panics` and
`check_snapshot` — the name `scientific-libraries.md` §3 and
`stdlib-shape-and-packages.md` §4.4 both already chose under the same constraint.

This is not a workaround and should not be presented as one. The two words have
different jobs and would need different names even if both were free:

| | `check` | `assert` |
|---|---|---|
| Where | Test items only (`SC0269`) | Anywhere |
| On failure | Records a failure; the runner continues to the next test | Panics, and aborts (§8 of the core spec) |
| Compiler | Knows nothing | May discharge it statically, or record it as an assumption |
| Present | Now | When the verification construct is specified |

### 7.5 Decision: one runner, not a fifth mechanism

The repository already has four testing layers (§10 of the core spec) and
`strings-formatting-and-docs.md` §5.5 adds a fifth — doc tests — and prices it
plainly:

> **It is a fifth test layer** beside §10's four, with its own runner, its own
> reporting, and its own place in CI.

A sixth needs justifying, and the justification is a category difference. **§10's
four layers all test `sciencec`**: unit tests, AST/HIR/MIR snapshots, UI tests in
`tests/ui/` against `crates/science-testkit`, and execution tests are Rust-side
tests of the compiler. **Nothing tests a user's Science program.** `testing` is
the first mechanism in its category, not a fourth in an existing one.

But doc tests *are* in that category — Science code compiled and run by the
toolchain — and shipping two runners for one category would be the mistake.

**Decision: `sciencec test` is one runner with two sources of cases: `test` items
and doc-test fences.** One report format, one place in CI, one `--filter`, one
exit code. §16 files this as an amendment to
`strings-formatting-and-docs.md` §5.5, which currently assumes its own runner.

**Decision: snapshot testing for Science programs reuses the compiler's own
discipline exactly.** `testing.check_snapshot(value, named: "…")` writes a `.snap`
file beside the test on first run, compares byte for byte afterwards, and is
re-blessed by `SCIENCE_BLESS=1` — **the same environment variable**
`crates/science-testkit/src/lib.rs` already documents, with the same rule that a
failure prints a diff and blessing is only safe if a human reads it. A second
convention for the same act would fork a discipline the repository already has.

### 7.6 What is deliberately absent

- **Mocks, stubs, fakes, dependency injection.** A language with no reflection and
  no dynamic dispatch by default cannot offer them cheaply, and the audience's
  tests are numerical, not interaction-based.
- **Parameterised tests.** A `for` loop inside a test item covers it, losing
  per-case naming. When derive and attributes exist, that is where a table-driven
  form goes.
- **`setup` / `teardown`.** A function call at the top of the test. Ownership and
  `Drop` (§6.1 rule 6) already do teardown correctly, which is more than most
  frameworks' teardown achieves.
- **Benchmarks.** Timing is a `Monotonic` and a loop. A statistically honest
  harness — outlier rejection, warm-up, confidence intervals — is a `stats`
  consumer and belongs at Level 3.

### 7.7 Five signatures

```science
## The general assertion. `because` is required: a bare failed condition
## tells the reader nothing the source line did not.
def check(condition: Bool, because: borrowed String)

## `Inspect` is `strings-formatting-and-docs.md` §3.2 — it is what lets the
## failure report print both values.
def check_equal of T(actual: borrowed T, expected: borrowed T)
        where T: Eq + Inspect

## Floats do not get `check_equal`. The tolerance is not optional and has
## no default, because a default tolerance is a wrong answer in some unit.
def check_close(actual: F64, expected: F64, tolerance: F64)

## Asserts the error is present and hands it back, so the test can go on to
## check which error it was. The dual is `check_ok(err: Error?)`.
def check_failed(err: Error?) -> any Error

## Byte-for-byte against a `.snap` file, re-blessed with SCIENCE_BLESS=1.
def check_snapshot(value: borrowed any Inspect, named: borrowed String)
```

### 7.8 Example

```science
use testing (check, check_close, check_failed)

test "a spectrum with a zero-deviation row is passed through unchanged":
    let mutable rows be Array.of(constant_row(3.0), varying_row())
    standardize(rows)
    check_close(rows.get(0).get(0), 3.0, tolerance: 0.0)

test "reading a missing run reports the path":
    let run, err be load_run(Path.of("no/such/run.csv"))
    let failure be check_failed(err)
    check(failure.message().contains("no/such/run.csv"),
          because: "the path must be in the message")
```

---

## 8. `logging`

### 8.1 Decision: levels and one sink; everything else is Level 3

**Decision.** `logging` exposes five levels (`Trace`, `Debug`, `Info`, `Warning`,
`Error`), a `Sink` interface, one built-in sink writing a line per record to
standard error, and one configuration point. Nothing else.

Out, at Level 3: file rotation, network sinks, OpenTelemetry/OTLP, syslog,
journald, structured JSON output, sampling policies, per-module level
configuration by glob.

The criterion draws that line in an unusual place: the *levels* are decades
stable — every logging library since syslog has the same five — while the *sinks
and wire formats* churn constantly. The stable half ships and the churning half
does not.

**A naming exception, declared rather than smuggled.**
`stdlib-shape-and-packages.md` §4.1 forbids abbreviations outside a closed list of
mathematical notation. `info` and `debug` are abbreviations by that rule, and they
are also the universal logging vocabulary in every language the audience has used.
§16 asks for both to be added to that closed list. If the request is refused the
functions are `information` and `diagnostic`, and the module is worse for it.

### 8.2 Decision: there is a process-global logger, and here is why that is not §5.3

§5.3 spends a page arguing that ambient mutable state is what ruined
`numpy.random.seed`, and this section installs a process-global logger. The
difference is not a compromise; it is the test that separates the two cases:

> **Ambient state is acceptable exactly when it cannot change the program's
> computed result.**

A global RNG changes the answer. A global logger changes what is printed. The
first makes a *result* unreproducible; the second makes a *run* unreproducible in
its diagnostics, which is an operational annoyance and not a scientific one.

Two guards keep even that honest:

- **`logging.configure` may be called once.** A second call panics, naming the
  span of the first. There is no "who reconfigured my logger" mystery, because
  there is exactly one configuration and the program says where it was set.
- **The default configuration comes from `os.env`.** `SCIENCE_LOG=debug` sets the
  minimum level with no source change, which is what a user on a cluster needs at
  2am, and it means the common case requires no `configure` call at all.

### 8.3 Decision: logging is an effect, and it does not go inside a chain

`collections-and-chains.md` §7 point 7 is unambiguous:

> **No combinator has an observable effect.** [...] A chain whose links are pure
> can be reordered, fused, split and re-run; one with a `println` in the middle
> cannot.

That is why §1.5 of that note refuses `for_each` and `inspect`, and it applies to
`logging` with full force. **A `logging` call inside a closure passed to a chain
combinator is legal in F0 and becomes a compile error under `.parallel()` in F2**,
for the same reason and reported by the same check. The module's documentation
says so at the top, because the alternative is a user discovering it when their
per-row log lines interleave across eight workers.

More broadly, `logging` is the archetypal effect in a language whose §1 names an
effect discipline as part of its bet, and it is recorded here as **the effect
system's first customer** when one arrives. Until then the discipline is a
documented convention and the note does not pretend otherwise.

### 8.4 Written or linked, and dependencies

**Written.** Depends on `time` (a record carries an `Instant`), `os` (the
environment default), and Level 1's `Display` and stream machinery
(`strings-formatting-and-docs.md` §4.2 owns buffering and stderr).

`is_enabled(level)` exists for one reason worth stating: an `f"…"` interpolation
allocates (`strings-formatting-and-docs.md` §1.7), so
`logging.debug(f"{expensive_summary(state)}")` pays for the string even when debug
is off. `if logging.is_enabled(Debug):` around it is the escape hatch, and without
it a debug-logging idiom would be a performance bug.

### 8.5 Five signatures

```science
choice Level:
    Trace
    Debug
    Info
    Warning
    Error

## Callable once. A second call panics and names the first. §8.2.
def configure(minimum: Level, sink: Sink)

## The guard for expensive messages, because f"…" allocates. §8.4.
def is_enabled(level: Level) -> Bool

def info(message: borrowed String)

def warning(message: borrowed String)

## The structured form. Fields are strings: a typed field map needs the
## derive mechanism the README's standing asks already track.
def record(level: Level, message: borrowed String,
        fields: borrowed Map of (String, String))
```

### 8.6 Example

```science
use logging (Level, configure, info, warning, is_enabled)

def main():
    configure(minimum: Level.Info, sink: logging.standard_error())

    for epoch in 0..epochs:
        let loss be train_one_epoch(weights, batches)
        info(f"epoch {epoch}: loss {loss:.5f}")
        if loss.is_nan():
            warning(f"loss diverged at epoch {epoch}; stopping")
            break
```

---

## 9. `text.regex`

Named `text.regex` rather than `regex`, per `stdlib-shape-and-packages.md` §4.7,
which gives it as the worked example of a submodule and which §5.2 of that note
already cites as YAML's dependency. `stdlib-core.md` §1.5 independently finds that
regular expressions fail all five of its Level 1 questions, so Level 2 is the
first level at which this can live at all.

### 9.1 The criterion says this module does not belong here either

**`text.regex` violates the release-cycle rule on both counts.**

- **Security.** Catastrophic backtracking — ReDoS — is a CVE class, not a bug.
  Every backtracking engine has shipped patches for it, on a cadence set by
  whoever found the next pathological pattern.
- **Performance.** Regex engines are under permanent active development: SIMD
  literal prefilters, lazy DFA construction, memchr skipping, Teddy and
  Aho–Corasick multi-pattern search. An engine in 2030 will be materially faster
  than one in 2026 for reasons unrelated to the language.

It ships anyway, because the alternative is that every user writes an ad-hoc
parser or pulls an unaudited one from somewhere, and a scientific language whose
users cannot parse an instrument's filename is not usable. §9.2 is how it is
brought inside the rule.

### 9.2 Decision: a linear-time engine, which retires the security half

**Decision.** The engine is an RE2-style automaton — Thompson NFA with a lazy DFA
— with a guaranteed linear time bound in the length of the input and no
backtracking. Consequently:

- **No backreferences** (`\1`). They make matching NP-hard.
- **No lookahead or lookbehind.** The other source of exponential blowup.
- **The accepted syntax is frozen and specified**: character classes, Unicode
  classes, anchors, alternation, the three quantifiers with bounded repetition,
  non-capturing and named groups, and nothing else. Adding syntax later is
  additive and safe; removing is never.

This is Corollary B. **ReDoS is not patched, it is absent**, because there is no
exponential path to trigger. The security half of the violation stops being a
release-cadence obligation and becomes a one-time design decision.

It also satisfies Decision 5d's compliance rule 1. The linear-time bound is a
statement about *worst-case observable time*, not about the algorithm; any engine
meeting it may be substituted, and a backtracker may not. The contract pins the
property, not the implementation.

The performance half remains and is accepted rather than argued away: this engine
will be two to five times slower than PCRE2 with its JIT on literal-heavy patterns
for years. A user who needs that reaches for a Level 3 binding, when Level 3 is a
place things can be reached for.

### 9.3 Decision: written in Science, not linked

**Decision. Written.** The three candidates:

- **RE2 — rejected because it is C++.** `data-io.md` §2 is explicit that the split
  between the core and the `data.*` modules exists so "the base toolchain build[s]
  with no C++ compiler present", and §9 of that note prices the C++ dependency
  Parquet forces and puts it behind a build feature. Making the base toolchain's
  regex engine a C++ dependency spends that budget on something every program
  uses, which is backwards.
- **PCRE2 — rejected because it backtracks.** Binding it re-imports the ReDoS
  class §9.2 just designed out, in a language that markets itself on catching
  mistakes before the run starts.
- **Writing it** is `scientific-libraries.md` §2's rule applied honestly: the
  artefact is a published algorithm, not decades of per-microarchitecture tuning.
  Thompson construction is from 1968 and the lazy-DFA cache is well described.

**Cost, stated plainly.** This is the largest write-rather-than-link bet in the
whole standard library — a few thousand lines of real, subtle code, competing for
the same engineer as `linalg`. It is scheduled for F1 for that reason and §18
records it as a risk.

### 9.4 Decision: literal patterns are checked at compile time

**Decision.** A `Pattern.compile` whose argument is a string *literal* is
validated during type checking: a malformed pattern, and a capture group named in
code that the pattern does not contain, are both compile errors (`SC0268`). A
pattern built at run time uses the same function, which stays fallible, and the
check does not apply.

This is §1 of the core spec applied to a library: *"the mistakes a compiler can
catch before a run starts"*. A typo in a regex is otherwise found on the day the
input arrives.

**What it costs**: const evaluation over string literals, so the type checker can
see the pattern's text. §16 files it, where it joins the README's standing asks
with a second customer already present —
`strings-formatting-and-docs.md` §2.4 checks format specifications at compile time
and needs the same capability for the same reason.

### 9.5 Five signatures

```science
Pattern has:
    ## Fallible at run time; checked at compile time when `source` is a
    ## literal (§9.4, SC0268).
    def compile(source: borrowed String) -> (Pattern, PatternError?)

    ## `matches`, not `match`: the keyword is taken and these read better.
    def matches(self, text: borrowed String) -> Bool

    ## Null when there is no match, which is `T?` doing exactly its job.
    def find(self, text: borrowed String) -> Found?

    ## An `Iterate` source, so it composes with the chain vocabulary.
    def find_all(self, text: borrowed String) -> Matches

    def replace_all(self, text: borrowed String,
            replacement: borrowed String) -> String
```

### 9.6 Example

```science
use text.regex (Pattern)

def main():
    let names, err be Pattern.compile("^run_(?<run>\\d{4})_(?<channel>[a-z]+)\\.csv$")
    if err?:
        panic("unreachable: a literal pattern is checked at compile time")

    for path in directory.entries():
        let found be names.find(path.name())
        if found is null:
            continue
        print(f"run {found.group("run")}, channel {found.group("channel")}")
```

---

## 10. `thread`

### 10.1 The model is decided elsewhere, and this section does not reopen it

`stdlib-shape-and-packages.md` Decision 3 settles the concurrency model, and the
earlier draft of this note wrongly treated it as open:

> **Science gets OS threads and structured data parallelism, and no
> `async`/`await`. There is exactly one spelling of every I/O function in the
> standard library, and it blocks.**

with F2 bringing `.parallel()`, F3 bringing actors, and *"a `thread` module with
classic primitives [sitting] under both"*. The module name is `thread`, its
surface is §3.6 of that note, and this section adds to it rather than redesigning
it.

**Phase: F2.** That note's schedule and this one's agree, and the reason is worth
recording because it runs opposite to the intuitive ordering: F3's actors are
built *on* `thread`, not beside it, because an actor needs a mailbox and a mailbox
is a `Channel`. That is why §2 refuses to take `spawn`, `send` and `receive` in
free-function position even where the dot rule would allow it — those words belong
to F3's construct.

### 10.2 What this note adds: atomics and a read-write lock

§3.6 of the policy note gives `Thread`, `Channel` and `Mutex`. The author's
request also names atomics and `RwLock`, and neither is in that surface, so they
are specified here in its style.

**Decision: `Atomic of T` offers sequentially consistent operations and nothing
else.** No relaxed, acquire, release or acq-rel ordering; no standalone fence.

The C++11 memory model is the single hardest thing in systems programming, and §1
of the core spec names an audience that is not systems programmers. §5.1 already
establishes the house position that a numeric language owes its users defined
behaviour over speed — *"Results that differ between runs are opt-in, because this
is a language whose users publish"* — and memory ordering is that question with
sharper teeth.

`Atomic of T` is instantiable only for `Bool`, `I32`, `I64`, `U32`, `U64` and
pointer-sized integers, because those are what every target implements lock-free.
Anything else is `SC0266`.

**Cost**: a spin loop or a lock-free queue written in Science is slower than one
written with acquire/release. Accepted — a user writing a lock-free queue is
outside this module's audience, and `unsafe extern` is the escape hatch that
already exists.

**Decision: `RwLock of T` follows `Mutex`'s closure form**, with `with_read` and
`with_write`, for the reason §3.6 gives for `Mutex.with_lock`: a guard makes the
region engine responsible for lock scoping, and makes "forgot to drop the guard
early" a deadlock diagnosed by a region error. The closure form makes the critical
section a lexical block, which is what a user should see.

### 10.3 What this note asks for: a scope, and why

A value crossing a thread boundary is exactly the case regions must handle, and
§3.3 of the policy note answers it by moving an owned value in:

```science
let handle be Thread.start(over: data, running: d giving worker(d))
```

That shape is right and the reasoning behind it is right — the move happens at a
program point the region engine already understands, and it avoids the
zero-argument closure the core spec gives no spelling for.

**It leaves one case uncovered, and it is the numeric case.** Eight threads
reading one 40 GB array is the normal shape of this audience's parallel work.
`over:` moves, so the array can go to exactly one thread; the alternatives are to
clone it eight times, which is impossible at that size, or to wrap it in an `Arc`
Science does not have.

**Ask, not decision (§16).** A scoped form in the same shape:

```science
Scope has:
    def start of (T, U)(mutable self, over: borrowed T,
            running: def(borrowed T) -> U) -> Task of U

def scope of R(body: def(mutable borrowed Scope) -> R) -> R
```

Every task started in a scope is joined before the scope returns, so the join is a
program point the compiler can name, and a shared borrow may therefore cross. This
is §6.5's argument for ownership reused verbatim — *"ownership frees a device
buffer at a point the compiler can name"* — and it needs no new region syntax,
because the scope body is a call and the borrow is an argument to it.

It is filed as an ask because `thread`'s surface belongs to
`stdlib-shape-and-packages.md` §3.6, and a second note adding an entry to it
without saying so is how the collisions the README's allocation section documents
happened.

### 10.4 The reentrancy hole, and the one requirement that is this note's

§3.4 of the policy note already states the FFI interaction fully — most scientific
C libraries are not thread-safe, its three consequences, and the ask for a
`library "hdf5" single threaded` clause on an `extern` block. That is
`ffi-c-boundary.md`'s territory and neither note decides it. §16 seconds the ask,
which is the whole of this note's contribution to it.

One thing here *is* this note's, because §11 and §12 depend on it and nobody else
has named it: **`ffi-c-boundary.md` §5.2's `when available` table must be a
once-cell with an acquire/release pair, not a flag.** §3.4 of the policy note says
the same; this note restates it because `crypto.tls` (§13.3) is the first
`when available` block that a threaded program will actually hit, through `http`'s
connection pool, and "populated on first use" is a data race that will work in
testing.

### 10.5 Surface

From `stdlib-shape-and-packages.md` §3.6, unchanged: `Send`, `Share`,
`Thread.start(over:, running:)`, `Thread of T.join`,
`Thread.available_parallelism`, `Channel.bounded`, `Sender.send`,
`Receiver.receive`, `Mutex.new`, `Mutex.with_lock`.

Added here: `RwLock.new`, `RwLock.with_read`, `RwLock.with_write`, `Atomic of T`,
and `Barrier`.

Not present, per §3.6 and agreed: no `thread.sleep`, no exposed global pool
(`.parallel()` owns the only one), no unbounded channel. An unbounded channel is a
memory leak with a queue in front of it, and the producer outrunning its consumer
is the normal case in data work, not the exception; a producer that must not block
uses `Sender.try_send`, which is fallible and says so.

### 10.6 Five signatures

```science
## `stdlib-shape-and-packages.md` §3.6. The value moves at the call, which
## is a program point the region engine already understands.
Thread has:
    def start of (T, U)(over: T, running: def(T) -> U) -> Thread of U

Thread of T has:
    def join(self) -> (T, ThreadError?)

## Bounded. There is no unbounded constructor. §10.5.
Channel of T has:
    def bounded(capacity: Int) -> (Sender of T, Receiver of T)

## Added by this note, in `Mutex.with_lock`'s shape and for its reason.
RwLock of T has:
    def with_read of U(self, giving: def(borrowed T) -> U) -> U

## Added by this note. Sequentially consistent; there is no other
## ordering, and `T` outside the lock-free set is SC0266. §10.2.
Atomic of T has:
    def fetch_add(self, amount: T) -> T
```

The closure type `def(T) -> U` is the spelling the README's standing-asks
table already lists three customers for. This is the fourth.

### 10.7 Example

```science
use thread (Thread, Channel)

def total_counts(shards: Array of Shard) -> I64:
    let workers be shards
        .iterate()
        .map(shard giving Thread.start(over: shard,
                                       running: s giving count_events(s)))
        .collect()

    let mutable total be 0
    for worker in workers:
        let n, err be worker.join()
        if err?:
            panic(err.message())
        total be total + n
    total
```

---

## 11. `net`

`stdlib-shape-and-packages.md` §5.4 names this module as a prerequisite it could
not supply — *"there is no `net` module in any note in this repository, and a
PostgreSQL driver is blocked behind one. [...] It should be a note, and it is not
one this note can write."* This section is that note, and the driver in §5.4 is
its first consumer.

### 11.1 Decision: TCP and UDP, blocking, because Decision 3 says blocking

**Decision. `net` is blocking and synchronous. One connection per thread. It ships
in F2, with `thread`, and not before.**

An earlier draft of this note treated the async question as open and deferred it
to a note that did not exist. It is not open. Decision 3 settles it, with reasons
specific to this language that this note does not improve on: function colouring
produces two standard libraries; a future holding a borrow across an `await` is a
self-referential value in a language that rejected lifetime syntax; and green
threads tax the foreign call, which `ffi-c-boundary.md` §0 establishes *is*
Science's hot path.

So `net` has exactly one spelling of every operation and it blocks. `async` and
`await` stay reserved and unimplemented, and the honest statement Decision 3
already makes applies here without softening: **Science will be a bad language for
writing a program whose shape is ten thousand mostly-idle connections.** That is
not this audience's program.

What this note adds to that decision is one consequence for `net`'s own shape:
because there is one colour, `TcpStream` implements the same `Read` and `Write`
interfaces `stdlib-core.md` §9 puts at Level 1, so a socket and a file are
interchangeable everywhere a stream is wanted. In a two-colour language they could
not be, and that interchangeability is the concrete dividend of Decision 3.

### 11.2 Decision: every blocking operation takes a timeout, with no default

**Decision.** `accept`, `connect`, `read` and `write` each take a `Duration`, and
there is no overload without one.

The alternative is the default every socket library has and every socket library
regrets: a blocking read with no deadline, which hangs a program forever on a
half-open connection and produces the bug report "it worked yesterday". A
scientific program blocked on a socket at hour nine of a twelve-hour cluster
allocation has lost the allocation.

**Cost**: one more argument at every call site, and a `Duration` import in every
program that opens a socket. Accepted, and it is the cheapest correctness argument
in this note.

### 11.3 DNS is linked, everything else is written

**Decision.** Name resolution binds `getaddrinfo` through
`unsafe extern "C" library "c"`. The socket calls themselves are a thin binding
plus Science.

This is `scientific-libraries.md` §2's rule producing an unobvious answer.
Resolution *looks* like an algorithm and is not: it is OS policy. A written
resolver would have to parse `/etc/resolv.conf`, honour `nsswitch.conf`, know
about mDNS and about the corporate VPN's split-horizon setup, and get the
IPv4/IPv6 preference right. `getaddrinfo` is where that policy lives on every
platform, and reimplementing it means being wrong on exactly the machines that are
hardest to debug.

**TLS is not in `net`.** It is `crypto.tls` (§13.3), and `crypto` is Level 3.

### 11.4 Surface

`Address`, `resolve`, `TcpListener` with `bind` / `accept` / `local_address`,
`TcpStream` with `connect` / `read` / `write` / `shutdown`, `UdpSocket` with
`bind` / `send_to` / `receive_from`, and `NetError`.

Out: Unix domain sockets, raw sockets, multicast, socket options beyond
`SO_REUSEADDR` and the timeouts, and anything requiring privilege.

### 11.5 Five signatures

```science
TcpListener has:
    def bind(address: borrowed String) -> (TcpListener, NetError?)

    ## Every blocking operation takes a deadline. There is no overload
    ## without one. §11.2.
    def accept(mutable self, timeout: Duration)
            -> (TcpStream, NetError?)

TcpStream has:
    def connect(address: borrowed String, timeout: Duration)
            -> (TcpStream, NetError?)

    ## Returns the byte count, which may be short. The caller loops.
    ## This is Level 1's `Read`, so a socket and a file interchange. §11.1.
    def read(mutable self, into: mutable borrowed Array of U8,
            timeout: Duration) -> (Int, NetError?)

UdpSocket has:
    def send_to(mutable self, data: borrowed Array of U8,
            address: borrowed String) -> (Int, NetError?)
```

### 11.6 Example

Reading from an instrument that streams newline-delimited samples over TCP.

```science
use net (TcpStream)
use time (Duration)

def collect_samples(host: borrowed String, wanted: Int)
        -> (Array of F64, Error?):
    let stream, err be TcpStream.connect(host, timeout: Duration.seconds(5))
    if err?:
        return (Array.new(), err)

    let mutable samples be Array.with_capacity(wanted)
    loop:
        if samples.length() >= wanted:
            break
        let line, read_err be stream.read_line(timeout: Duration.seconds(1))
        if read_err?:
            return (samples, read_err)
        samples.push(parse_float(line))
    return (samples, null)
```

---

## 12. `http`

### 12.1 Decision: client only

**Decision. `http` is an HTTP/1.1 client. There is no server.**

Three reasons, in order of weight.

1. **A server is a concurrency architecture and this one would be the wrong
   one.** Decision 3 gives OS threads and refuses async, and that decision is
   correct *for this audience*; it is also the decision that makes Science a bad
   host for ten thousand connections, as §3.7 of that note states outright.
   Shipping a server on a thread-per-connection model is shipping the shape the
   concurrency decision was explicitly willing to be bad at.
2. **The audience's need is asymmetric.** Scientists download datasets, call model
   endpoints, query catalogues and fetch parameter files. They do not host
   services in the language they analyse data in — the service, if there is one,
   is somebody else's Python or Go. The demand for a server is real and it is not
   this language's demand.
3. **A server is an attack surface pointed at the open internet, maintained by a
   compiler team on a compiler's release cadence.** That is the release-cycle rule
   at its sharpest, and it is not a close call.

### 12.2 The criterion applied to the answer just given, which moves `http` to Level 3

The instruction was to apply the criterion to my own answer, and it convicts the
client too.

A client parses attacker-controlled input — response headers, chunked encodings,
redirect targets — so it is an attack surface pointed at the internet, just a
narrower one. Its redirect policy is an SSRF question; its cookie handling is a
scope-and-domain question with a history of CVEs; HTTP/2 and HTTP/3 are moving
targets and connection pooling is a performance question with its own cadence.
**`http` violates the release-cycle rule.**

An earlier draft shipped it at Level 2 with a label. That was wrong, and the thing
that makes it wrong is structural rather than a matter of degree: **`http` needs
TLS, TLS is `crypto.tls`, and `stdlib-shape-and-packages.md` Decision 5c puts
`crypto` at Level 3.** A Level 2 module cannot depend on a Level 3 package —
Level 2 is defined as present in the tarball and Level 3 as absent from a minimal
one — so `http` was never actually placeable at Level 2. The criterion and the
dependency graph give the same answer, which is the best evidence available that
it is the right one.

**Decision: `http` is Level 3.** With a frozen, minimal policy surface that it
carries wherever it lives:

- HTTP/1.1 only. No HTTP/2, no HTTP/3.
- **No cookie jar at all.**
- No automatic redirect following. `follow_redirects(n)` is an explicit, bounded
  opt-in, and a redirect to a different scheme or to a private address range is
  refused.
- No proxy auto-configuration. No authentication schemes beyond a bearer header
  the caller sets.
- Written against a **bounded** thread pool from the start, which is
  `stdlib-shape-and-packages.md` §3.7's explicit requirement: *"a thousand OS
  threads is several gigabytes of stack reservation, so the pool must be bounded
  and the standard client must be written against a bounded pool from the
  start."*

### 12.3 Decision: written over `net`, and the close call with libcurl

**Decision. Written**, over `net` and `crypto.tls`, with `data.json` for bodies.

**libcurl was seriously considered and it is the closest call in this note.** It is
the audited, universal, everything-supporting client; it is C, not C++; it is
installed everywhere; and binding it would satisfy Corollary A perfectly, with
security updates arriving through the system package manager exactly as `crypto`'s
do. Three things decided against it:

1. **Its API fights §6.** A global `curl_global_init`, an option bag set by a
   variadic function, write callbacks driven from inside the library, and an
   easy-versus-multi split that reintroduces the colour question Decision 3 just
   closed. The safe wrapper would be most of a rewrite.
2. **It chooses its own TLS backend**, which would sit beside `crypto`'s choice
   (§13.3) and give one program two TLS stacks with two trust stores. A program
   whose certificate validation depends on which backend curl was compiled with is
   not reproducible in the way this language cares about.
3. **It would have to be `when available`**, and a program that cannot download
   its input on a cluster node is useless — whereas TLS being unavailable degrades
   to plain HTTP against an internal data server, which is survivable.

**The condition that flips this decision, stated so it can be checked later:** if
`http` is ever asked to support HTTP/2, proxy configuration, or more than one
authentication scheme, stop writing and bind libcurl. Those three mark the point
at which writing stops being cheaper than binding, and past it Corollary A is the
better answer.

### 12.4 Naming

`send` is reserved for F3. `Client.fetch(request)` is used rather than
`Client.send(request)` — `fetch` costs nothing here and is arguably the better
word for a client. This is deliberately *not* the choice made for channels
(§10.5), where `send` and `receive` are a load-bearing symmetric pair and the dot
rule is worth depending on. Where the reserved word is free to avoid, avoid it;
where it carries meaning, depend on the dot rule. The asymmetry is the rule, not
an inconsistency.

### 12.5 Five signatures

```science
## The one-liner. Follows no redirects; see `Client` for policy.
def get(url: borrowed String) -> (Response, HttpError?)

## Streams to disk rather than into memory: a dataset does not fit in a
## `Response` body, and this is the module's most common use by far.
def download(url: borrowed String, into: borrowed Path)
        -> (Int, HttpError?)

Request has:
    def new(method: Method, url: borrowed String) -> Request

    def header(mutable self, name: borrowed String,
            value: borrowed String)

Client has:
    ## `fetch`, not `send`: `send` is reserved for F3's actors. §12.4.
    def fetch(mutable self, request: Request)
            -> (Response, HttpError?)
```

### 12.6 Example

Fetch a dataset and verify it against the checksum the catalogue published — the
`http` and `crypto` examples are deliberately the same program.

```science
use http (download)
use crypto (hash_file, equal_constant_time)

def fetch_verified(url: borrowed String, into: borrowed Path,
        expected: borrowed Digest) -> Error?:
    let bytes, err be download(url, into: into)
    if err?:
        return err
    let actual, hash_err be hash_file(into)
    if hash_err?:
        return hash_err
    if not equal_constant_time(actual.bytes(), expected.bytes()):
        return ChecksumError.Mismatch(expected, actual)
    print(f"{bytes} bytes verified")
    return null
```

---

## 13. `crypto`

### 13.1 Decision: wrap libsodium, and implement nothing

The instruction is explicit and correct: **do not implement primitives; wrap an
audited library.** `stdlib-shape-and-packages.md` Decision 5c has already ruled,
places `crypto` at Level 3, and gives the argument this note has nothing to add
to and would not have found:

> **Constant-time code is not expressible in a language with an optimiser.** [...]
> LLVM is entitled to turn a branchless constant-time select into a branch, and
> does. A Science implementation of AES would be correct, auditable at source
> level, and potentially leaking at machine level, with no notation in the
> language for saying otherwise.

That is decisive and it is adopted. **`crypto` is Level 3, binds libsodium, and
Science implements no primitive.**

One half of Decision 5c is disputed here, on a mechanical ground.

| Candidate | Verdict |
|---|---|
| **libsodium** | **Taken**, as Decision 5c takes it. Stable ABI with a published versioning policy; packaged on every platform this language targets; independently audited; and — decisively — **exactly one algorithm per job.** No cipher suite negotiation, no ENGINE, no configuration file, no algorithm agility. A binding to libsodium is a binding to a fixed set of primitives, which is the only kind that survives the release-cycle rule. |
| **BoringSSL** | **Rejected, disputing Decision 5c.** That note names it *"where RSA, X.509 or TLS interoperability is required."* Google states BoringSSL has no stable API or ABI and must not be used as a shared library. `ffi-c-boundary.md` §5.1 makes dynamic linking the **default**, and does so precisely because *"HPC module systems work by swapping the shared object under a fixed name — statically linking [it] defeats the mechanism the cluster administrator is relying on."* A library that cannot be a shared object is disqualified by the FFI note's own linking model, and vendoring it statically would put a CVE-bearing TLS stack inside every Science binary with no way to patch it except a recompile. |
| **OpenSSL** | **Taken for TLS only** (§13.3), replacing BoringSSL in Decision 5c's sentence. Not for primitives: its behaviour depends on a system-wide `openssl.cnf` the program did not write and cannot see, which is a reproducibility problem and not merely an ergonomic one, and the 1.1.1-to-3.x transition broke ABI and split the ecosystem for years. Its bad API is tolerable for one job with one entry point in a way it is not for nine primitives. |

**Corollary A in action.** `crypto` violates the criterion completely — its
algorithms *must* change for security; that is their nature. The wrapper is the
answer: the changing part lives inside libsodium, updates on libsodium's schedule
through the system package manager, and Science's binding is a name for an
algorithm rather than an implementation of one. This is
`stdlib-shape-and-packages.md` §5.6's compliance rule 2 — *"anything with a CVE
surface is replaceable at link time"* — and `crypto` is its clearest instance.

### 13.2 What is wrapped, and what stays out

**In** — one algorithm per job, and the algorithm is not selectable:

| Job | Algorithm | libsodium entry point |
|---|---|---|
| Hash | SHA-256, SHA-512, BLAKE2b | `crypto_hash_sha256`, `crypto_generichash` |
| Keyed MAC | HMAC-SHA-256 | `crypto_auth_hmacsha256` |
| Authenticated encryption, symmetric | XChaCha20-Poly1305 | `crypto_secretbox_*` |
| Authenticated encryption, to a public key | X25519 + XChaCha20-Poly1305 | `crypto_box_*` |
| Signature | Ed25519 | `crypto_sign_*` |
| Password hashing | Argon2id | `crypto_pwhash_str`, `_verify` |
| Key derivation | HKDF-SHA-256 | `crypto_kdf_hkdf_sha256_*` |
| CSPRNG | the OS source | `randombytes_buf` |
| Constant-time compare | — | `sodium_memcmp` |

**Out**, and the exclusions are the design:

- **Algorithm choice.** There is no `Cipher` enumeration, no `choose_hash(name)`,
  no suite negotiation. **Algorithm agility is the mechanism by which
  cryptographic code becomes insecure**, and a module with one algorithm per job
  has no downgrade path. When an algorithm must be replaced, a new function with a
  new name appears and the old one is deprecated — a source change a reviewer can
  see. This also discharges Decision 5d's compliance rule 1 in an unusual
  direction: the algorithm *is* the contract here, named in the function, and
  that is correct for cryptography and wrong for everything else in this note.
- **Raw block ciphers.** No AES-ECB, no CBC, no unauthenticated modes at all.
  Every encryption function in `crypto` is authenticated.
- **X.509 parsing and certificate validation.** Delegated entirely to the TLS
  stack (§13.3). A hand-rolled certificate parser is the second-worst idea in this
  note's vicinity.
- **JWT, OAuth, certificate pinning policy, key management, HSM interfaces.** All
  separate packages, if ever.
- **Caller-supplied nonces without the safe form beside them.** `secretbox`
  generates its nonce and prepends it; the bring-your-own form is named
  `secretbox_with_nonce` and is documented as requiring a counter the caller can
  prove never repeats.

Also out, and stated because `stdlib-core.md` §9 depends on it: **non-cryptographic
hashing is not here.** `Map`'s hash function, checksums and CRC32 are Level 1
infrastructure and must not depend on `crypto`.

### 13.3 TLS, and the cost of libsodium not having it

libsodium does not do TLS. That is its one significant gap and it must be paid
for.

**Decision. `crypto.tls` binds OpenSSL's `libssl`, `when available`, and nothing
else.**

```science
unsafe extern "C" library "ssl" when available via pkg-config "openssl":
    ...
```

This uses `ffi-c-boundary.md` §5.2 exactly as specified — a lazily-initialised
table of function pointers populated by `dlopen`/`dlsym` inside `science-rt`, a
generated `is_available() -> Bool`, and a clear panic if a function is called on a
machine without the library. The cost that section prices — *"one load, one branch,
one indirect call per foreign call"* — is nothing against a TLS handshake. §10.4
is the requirement that makes it safe once threads exist.

**Why OpenSSL here and not for the primitives.** On Linux, the platform that
matters for this audience, OpenSSL is already installed and already updated by the
distribution's security team. That is exactly where TLS updates should come from,
and it means `crypto.tls` has no update obligation of its own.

**Cost, stated plainly.** On Windows and macOS `libssl` is not present by default,
so `https` is unavailable until the user installs it. That is real and unpleasant.
It is stated rather than fixed, and the fix — a platform backend per OS, Schannel
and Network.framework behind the same `crypto.tls` interface — is named in §18 as
future work rather than promised here. A third binding is not a thing to design
speculatively.

### 13.4 Decision: `crypto` randomness and `random` are made hard to confuse

`stdlib-shape-and-packages.md` §5.5 states the requirement and stops at
documentation: *"the two have opposite requirements [...] The documentation of
each should name the other and say so."* Documentation is necessary and it is not
enough. Four cumulative mechanisms, three of which the compiler enforces.

1. **Different shape.** `crypto.random_bytes(count) -> Array of U8` and
   `crypto.random_u64() -> U64` are the entire surface. **There is no
   `crypto.uniform()`, no `crypto.normal()`, no distribution of any kind.** You
   cannot accidentally use `crypto` as a statistical generator, because it offers
   nothing a statistician wants.

2. **No conversion in either direction, and a diagnostic that explains why.**
   There is no `Key.from_crypto_bytes`, no `Stream.from_secure_entropy`, and no
   `crypto` function accepting a `random.Key`. `SC0265` fires on either attempt,
   and its message names the correct function *and the reason*: a `Key` is
   designed to be reproducible, which is exactly the property key material must
   not have.

3. **Secrets do not print; keys do.** **`random.Key` implements `Display` and
   `Inspect` and prints its bits**, because a reproducibility failure is debugged
   by looking at the key. **Every secret type in `crypto` implements `Display` to
   print a constant redaction and never the bytes** — `<secret>` — so a secret
   cannot reach a log through `f"{key}"`. That is a real security property in its
   own right and it doubles as the distinguisher: *if it prints, it is a
   `random.Key`; if it redacts, it is a secret.*

4. **No bare `Key` in `crypto`.** `random` owns the word —
   `scientific-libraries.md` §7.3 named it first and this note does not move it.
   `crypto`'s key types are always compound: `SigningKey`, `VerifyingKey`,
   `BoxSecretKey`, `BoxPublicKey`, `SecretBoxKey`. There is no type named
   `crypto.Key`.

One thing must be said precisely, because it is where a careful reader will push.
`random.Stream.from_entropy()` does read the OS CSPRNG. **The seed is secure; the
stream is not.** A sequential generator's internal state is recoverable from a
modest number of outputs, so a stream seeded from entropy is still unsuitable for
key material. Its doc comment says exactly that and names `crypto.random_bytes` as
the alternative.

### 13.5 How the binding uses `ffi-c-boundary.md`'s mechanism

No new mechanism. The primitives block is an ordinary `unsafe extern` block with a
`via pkg-config` discovery clause, translated into revision-2 syntax:

```science
unsafe extern "C" library "sodium" via pkg-config "libsodium":

    def crypto_generichash(
        out: ffi.MutableSpan of U8, out_length: ffi.CSizeT,
        input: ffi.Span of U8, input_length: ffi.CULongLong,
        key: ffi.Span of U8, key_length: ffi.CSizeT,
    ) -> ffi.CInt

    def sodium_memcmp(
        a: ffi.Span of U8, b: ffi.Span of U8, length: ffi.CSizeT,
    ) -> ffi.CInt
```

Four properties of that block are load-bearing, and each is the FFI note's
decision rather than a new one:

- **`ffi.Span of T`, never `borrowed Array of T`** (§1.3, `SC0421`). The length
  lives on the Science side so the safe wrapper's bounds precondition is an
  expression the compiler checks.
- **No by-value aggregates** (§5.4, `SC0429`). libsodium passes everything by
  pointer, so the first-implementation restriction costs nothing here.
- **`ffi.CSizeT` and `ffi.CULongLong` written out, not aliased to `Int`** (§1.6).
  libsodium's lengths are genuinely `size_t` and `unsigned long long`, and that
  section's whole argument about integer width applies.
- **The safe wrapper is the interface** (§1.7). No `ffi` type appears in any
  signature in §13.6.

**One-time initialisation.** libsodium requires `sodium_init()` before first use.
The safe wrapper performs it lazily through the same once-cell §10.4 requires for
the `when available` table.

**Version drift.** §5.5 of the FFI note recommends that generated bindings record
the header version and check it at startup. `crypto` takes that recommendation as
a **requirement**: the startup check calls `sodium_library_version_major()` and
panics on a mismatch. For a crypto binding, running against an ABI the
declarations were not written for is not a risk to accept.

### 13.6 Five signatures

```science
## No `ffi` type appears here, per FFI §1.7. `Digest` prints as lower-case
## hex and compares in constant time.
def sha256(data: borrowed Array of U8) -> Digest

## Streams the file; a dataset does not fit in memory. The common use.
def hash_file(path: borrowed Path) -> (Digest, CryptoError?)

## The entire CSPRNG surface. No distributions, on purpose. §13.4.
def random_bytes(count: Int) -> Array of U8

## Ed25519. `SigningKey` redacts under `Display`, and there is no
## `crypto.Key`. §13.4.
def sign(message: borrowed Array of U8, key: borrowed SigningKey)
        -> Signature

## Because `a is b` on byte arrays short-circuits and leaks timing.
def equal_constant_time(a: borrowed Array of U8,
        b: borrowed Array of U8) -> Bool
```

### 13.7 Example

Verify a signed parameter file before trusting it — the case a scientist actually
has, which is "did this come from my collaborator unmodified".

```science
use crypto (VerifyingKey, Signature, verify, hash_file)

def load_trusted(params: borrowed Path, signature: borrowed Path,
        author: borrowed VerifyingKey) -> (Parameters, Error?):
    let digest, err be hash_file(params)
    if err?:
        return (Parameters.empty(), err)

    let bytes, sig_err be read_bytes(signature)
    if sig_err?:
        return (Parameters.empty(), sig_err)

    if not verify(digest.bytes(), Signature.of(bytes), author):
        return (Parameters.empty(), TrustError.BadSignature(params))
    return parse_parameters(params)
```

---

## 14. Which modules violate the criterion

Collected, because the instruction was to name them and be direct.

| Module | Verdict | What is done about it |
|---|---|---|
| **`crypto`** | **Violates, completely.** Cryptographic algorithms must change for security; that is their nature. | **Corollary A, and Level 3.** Wrap libsodium. The changing part lives behind a boundary with its own release cycle, updated by the system package manager. Science's binding is a *name* for an algorithm. `stdlib-shape-and-packages.md` Decision 5c reached the same placement from the constant-time argument; §13.1 disputes only its BoringSSL half. |
| **`http`** | **Violates.** Parses attacker-controlled input; redirect and cookie policy are security questions; HTTP/2 and pooling are moving performance targets. | **Level 3.** And the dependency graph forces the same answer independently: `http` needs TLS, TLS is `crypto`, `crypto` is Level 3, and a Level 2 module cannot depend on a Level 3 package. The frozen policy surface of §12.2 travels with it. |
| **`text.regex`** | **Violates on performance; the security half is retired.** | **Corollary B, and it stays at Level 2.** A linear-time engine makes ReDoS *absent* rather than patched, and the linear bound is an observable rather than an algorithm, which satisfies Decision 5d's rule 1. The gap against PCRE2-JIT is accepted permanently. |
| **`time.zones`** | **Violates on data cadence.** tzdata releases several times a year, because a parliament moved a boundary. | **Evicted to Level 3, and not written.** The only place the criterion actually removes something from this note. Versioned by its tzdata release, so a paper can cite it. |
| **`net` — TLS** | Violates, via `crypto`. | Delegated to `crypto.tls`, which is delegated to OpenSSL `when available`. Same answer as `crypto`, one layer down, and the reason `net` itself can stay at Level 2: plain TCP and UDP have no such surface. |
| **`thread`** | **Does not violate** — mutex, channel and atomic semantics have been stable for decades. **Its scheduler would.** | So there is no scheduler. `stdlib-shape-and-packages.md` §3.6 already refuses a second pool and §5.3 puts the scheduling policy behind an interface; `.parallel()` owns the question and it is F2's. |
| `time` (Level 1 and Level 2), `os`, `random`, `testing`, `logging`, `json` | Pass. | Nothing. `random` passes in the unusual way §5.6 describes. |

The pattern worth extracting: **every violation has the same fix, which is to put
the changing part behind a boundary that updates on its own schedule.** For
`crypto` that boundary is a shared library. For `text.regex` it is a design choice
that removes the need to change. For `time.zones` it is a package that does not
exist, so the module does not ship. For `http` it is a level whose distribution
story is a directory and a convention. Only the last two are unsatisfying, and
they are unsatisfying for exactly one reason — §0.1's reason.

---

## 15. Diagnostics allocated

`SC0265`–`SC0270`, in the types-and-traits range of §9. Checked against the
README's allocation map and against the codes claimed by
`stdlib-core.md` (`SC0257`–`SC0259`), `collections-and-chains.md`
(`SC0271`–`SC0273`), `strings-formatting-and-docs.md` (`SC0274`–`SC0275`),
`stdlib-shape-and-packages.md` (`SC0276`, `SC0277`, `SC0279`) and
`indexing-and-array-literals.md` (`SC0280`–`SC0287`). No overlap.

| Code | Meaning | Where |
|---|---|---|
| `SC0265` | A `random.Key` used where cryptographic key material is required, or a `crypto` secret used where a `random.Key` is required. The message names the correct function and says *why* the two are separate: a `Key` is designed to be reproducible, which is the property key material must not have. | §5, §13.4 |
| `SC0266` | `Atomic of T` instantiated with a `T` no target implements lock-free. Names the six admissible types. | §10.2 |
| `SC0267` | A `Monotonic` reading compared with or subtracted from a wall-clock `Instant`. The message explains that the wall clock jumps and names `Monotonic.since` as the fix. | §3.3 |
| `SC0268` | A malformed regex literal, or a capture group named in code that the literal pattern does not contain. Only fires for patterns that are string literals. | §9.4 |
| `SC0269` | A `testing.check*` called outside a `test` item, or a `test` item nested inside another item. | §7.2 |
| `SC0270` | **Warning.** A `public` function takes a `mutable borrowed random.Stream`. A library that draws from its caller's stream makes the caller's reproducibility depend on how many times the library drew. Suggests `random.Key`. | §5.3 |

An earlier draft claimed a seventh meaning — a value crossing a thread boundary
without `Send` or `Share`. That is `stdlib-shape-and-packages.md`'s `SC0277` and
the claim is withdrawn.

---

## 16. What this note asks of the others

Ordered by how much is blocked behind each.

1. **`stdlib-core.md` §9: a row for `Instant` and `Duration` at Level 1.** This is
   `data-io.md` §11.3's ask, answered (§3.1). Without it the CSV story has no
   dates, which that note calls its biggest single gap, and both pass
   `stdlib-core.md` §1.2's five questions cleanly.
2. **Core spec §13: add `test`.** One word, argued in §7.2 and priced in §7.3. It
   may instead be contextual — it is always followed by a string literal — and
   that choice belongs to §13's owner.
3. **`stdlib-shape-and-packages.md` §4.7: the module is `testing`, not `test`.**
   §0.3 gives the reason: a keyword and a module cannot share a spelling.
4. **`stdlib-shape-and-packages.md` §3.6: add `Atomic`, `RwLock` and `Barrier`**
   (§10.2), **and consider a `Scope`** (§10.3). The scope is the one that matters:
   `Thread.start(over:)` moves, so eight threads reading one 40 GB array — the
   normal shape of this audience's parallel work — has no expression today except
   a clone that cannot fit in memory.
5. **`stdlib-shape-and-packages.md` Decision 5c: replace BoringSSL with OpenSSL
   `when available`, for TLS only** (§13.1). BoringSSL has no stable ABI and must
   not be a shared library, which `ffi-c-boundary.md` §5.1 makes the default.
6. **`strings-formatting-and-docs.md` §5.5: one runner, not two.** That section
   correctly prices doc tests as "a fifth test layer [...] with its own runner".
   This note asks that `test` items and doc-test fences be two *sources* for one
   runner with one report format (§7.5). Deciding it now is cheap; after both
   exist it is a migration.
7. **`ffi-c-boundary.md` §4.4: a thread-safety clause on an `extern` block.**
   Seconding `stdlib-shape-and-packages.md` §3.4's ask, with `crypto.tls` and
   `http`'s connection pool as two more customers. This is the largest unchecked
   hazard touching this note and it is not this note's to design.
8. **`science-rt`: the `when available` table must be a once-cell with an
   acquire/release pair** before F2 ships threads (§10.4). `ffi-c-boundary.md`
   §5.2 specifies it in a single-threaded world, and `crypto.tls` is the first
   block a threaded program will hit through it.
9. **`scientific-libraries.md` §7.3 and §7.9: two corrections** (§5.1). (a)
   `random` moves out of `stats` into its own module depending on Level 1 only, so
   that a reproducible key does not require OpenBLAS — the same argument that note
   makes for `physics.units`. No name changes. (b) §7.9's
   `sample(self, key: borrowed Key)` contradicts the comment four lines below it;
   the key must be taken **by value**, or the whole verification claim fails.
10. **`data-io.md`: specify `data.json`'s number model** — what happens to an
    integer literal too large for an `F64`, and whether `1` and `1.0` are
    distinguishable (§6.2). `http` and `models-and-inference.md` both depend on
    the answer, and `stdlib-shape-and-packages.md` §5.6 already flags the parsing
    side of it.
11. **Const evaluation over string literals**, so the type checker can see a
    literal pattern's text (§9.4). Joins the README's standing asks with two
    customers: `text.regex` here, and `strings-formatting-and-docs.md` §2.4's
    compile-time format checking, which needs the same capability for the same
    reason.
12. **`stdlib-shape-and-packages.md` §4.1: add `info` and `debug` to the
    no-abbreviation rule's closed list** (§8.1), as the universal logging
    vocabulary. If refused, the functions are `information` and `diagnostic`.
13. **The dot rule** (`reserved-words.md` §0.1) is a hard dependency for `thread`:
    `Sender.send` and `Receiver.receive` are member-position uses of words
    reserved for F3. Fallback `put` / `take`. **The label rule**
    (`stdlib-shape-and-packages.md` §4.3) is endorsed but not depended on — every
    label in this note is a free word, and two were chosen that way on purpose.
14. **`use module` without an item list.** §4.4 gives only
    `use text.parser (Token, lex)`. This note writes `use os.env` and then
    `os.env.get(…)` throughout, which assumes the module-name form exists.

---

## 17. Sequencing

| Wave | Contents | Gated on |
|---|---|---|
| **1** | `Instant` and `Duration` into Level 1; `time`; `os`; `random` | Nothing. All F0 today. |
| **2** | The `test` item form; `testing`; `logging` | Wave 1, plus §16 items 2, 3 and 6. |
| **3** | `text.regex` | F1. Competes with `linalg` for the same engineer (§18). |
| **4** | `thread` | F2, and §16 items 7 and 8. |
| **5** | `net` | Wave 4. Unblocks `stdlib-shape-and-packages.md` §5.4's PostgreSQL driver. |
| **6** | `crypto` (Level 3) | Wave 4. Needs no threads, but its one-time init does. |
| **7** | `http` (Level 3) | Waves 5 and 6, plus `data.json`. |
| — | `time.zones` | A package manager. |

Wave 1 is deliberately everything that needs nothing, and it contains the two
modules with the strongest claim on being right the first time: `random`, whose
decision §5.5 freezes forever, and `time`, whose `Monotonic`/`Instant` split
cannot be added later without breaking every timing call written before it.

---

## 18. Risks

**Two of this note's modules are sent to a level that is a directory and a
convention.** `stdlib-shape-and-packages.md` §6.2 lists the four failures of
Level 3 as it exists today, and `crypto` hits the worst of them square on: *"a
security patch requires a whole-toolchain upgrade [...] on a cluster where
`sciencec` is a module the administrator installed, that is a ticket with a
two-week latency."* A cryptographic library with a two-week patch latency is the
exact failure the criterion was invoked to prevent, reintroduced by the absence of
the mechanism the criterion assumes. The partial mitigation is real and should be
said: because `crypto` *links* libsodium rather than implementing it, the patch
that matters arrives through the system's own package manager and not through
Science at all. That is Corollary A earning its keep, and it is the only thing
standing between this design and the failure.

**`random`'s freeze is a promise for the language's whole life.** §5.5 commits the
generator, the counter layout, `split`'s derivation and the bits-to-float mapping,
forever. The only thing that makes that promise verifiable is the test vectors,
and **they must ship from the first commit**. A freeze with no test vectors is a
freeze nobody can check, which in practice is no freeze at all.

**`crypto` depends on a library the user must install, and `sciencec` cannot
install it.** `when available` turns a link failure into a runtime failure with a
clear message, which is the right shape and is still a bad first experience.
On Windows and macOS it is the *default* experience (§13.3). The fix is platform
TLS backends, and it is future work rather than a promise.

**Writing a regex engine is the largest write-rather-than-link bet in the
library.** `scientific-libraries.md` §2's rule justifies it and §9.3's reasoning
holds, but the work is real, subtle, and competes directly with `linalg` — the
module the language is named for the audience of. If the engineer is not there,
the honest fallback is to ship no `text.regex` rather than a backtracking one,
because a backtracking engine cannot later become linear without changing which
patterns are accepted.

**F2 is now the fattest phase by a wide margin.** It already holds automatic
differentiation, data parallelism, GPU targets and Python extension modules, and
this note adds `thread`, `net`, `crypto` and `http` to it. `thread` is not
optional there — `.parallel()` needs it — but the phase's contents should be
re-examined as a whole rather than grown one note at a time.

**`test` is a new reserved word added on the day another note removes eight.**
§7.3 argues it is affordable and the statistics catalogue confirms no collision,
but the direction is against the grain of `reserved-words.md`, and if that note's
recommendations are refused this one should be re-examined rather than inherited.

**Three notes wrote the standard library in one afternoon, and this is the third.**
`stdlib-shape-and-packages.md` §7 records that it and `stdlib-core.md` drafted the
same diagnostic for the same check, invisibly to each other — the fifth instance
of the failure the README's allocation section exists to stop. This note found
three collisions with those two after its first draft was complete and has
resolved all three by moving (§0.3), which is the right outcome and is not
evidence that there are no more. The three should be read together before any of
them is treated as settled.
