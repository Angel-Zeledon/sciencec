# Science — Design: intrinsics, and the mathematics and physics catalogue

Date: 2026-09-16
Status: draft for review
Written in the syntax of `syntax-revision-2.md` (`function` … `->`, `of`,
`borrowed`, `interface`, `Type has:`, `T?`, `is` / `is not`).

Owns: the **tier** of every mathematical and physical name — compiler intrinsic,
prelude, or imported — and the **naming rule** that makes a scientific catalogue
usable. Does not own the catalogue's contents, which is
`scientific-libraries.md`'s, or the Level 1 boundary, which is `stdlib-core.md`'s.

Allocates **no diagnostic code**. Where a diagnostic is wanted it is asked of the
note that owns the range (§11), per `README.md`'s allocation record.

Sibling: a companion note covers chemistry and biology and inherits §2's naming
rule unchanged.

---

## 0. The one sentence

> **"Built in" is three questions, not one — *is it an instruction*, *is it in
> scope without an import*, and *is it in the standard library at all* — and the
> only one of the three that hardware decides is the first.**

Everything below is the consequence. The value of the note is the *boundaries*,
not the lists; the lists exist so that the boundaries have something to cut.

---

## 1. The three tiers, defined so they cannot be confused

### 1.1 The tiers

| Tier | Name | What it means, mechanically | How the user reaches it |
|---|---|---|---|
| **1** | **Compiler intrinsic** | `sciencec` emits a machine instruction, or an LLVM intrinsic that is *guaranteed* to become one on the target. There is no call, no stack frame, no library symbol. | The same spelling as tier 2 — the user cannot see the difference, and that is the point |
| **2** | **Prelude** | Ordinary Science code in `science-rt`, in scope with no `use`. A call, possibly inlined. | Nothing. It is there. |
| **3** | **Imported** | Ordinary Science code, or a linked C library, in a module. | `use math (integrate)` |

Tiers 1 and 2 are *not* alternatives. A tier 1 operation is also in the prelude —
`x.sqrt()` is written the same way whether it lowers to `sqrtsd` or to a call.
**Tier 1 is a statement about cost, tier 2 is a statement about scope, and
conflating them is the mistake this note exists to prevent.**

### 1.2 The test for each tier

Three tests, each mechanical, applied in order.

**Tier 1 — the hardware test.** All four must be yes:

1. Does a named LLVM intrinsic exist for it?
2. Does that intrinsic lower to a **bounded, small, fixed** instruction sequence
   on every target Science ships for — not a libcall, not a table lookup, not a
   polynomial?
3. Is the result **exactly specified** by IEEE-754 (or, for integers, by two's
   complement), so that "which implementation" is not a question anybody can ask?
4. Is the *cost* of the lowering honest on the worst target, or, where it is
   not, can the worst target be named in one line?

Question 2 is the one that does the work, and §3.1 explains why it is not the
same question as question 1.

**Tier 2 — the prelude test.** `stdlib-core.md` §1.2's five questions, unchanged
(pinned / no adversary / no performance product / no variants / no fix that must
outrun the compiler), followed by §1.3's second gate — *written by enough programs
to earn a permanent global name.* This note adds nothing to that test. It adds
three **extension rules** in §4.2 that say when a name already inside the boundary
drags a neighbour in with it.

**Tier 3 — everything else.** Tier 3 is the default and needs no argument. What
needs an argument is promotion out of it.

### 1.3 Why the owner's three examples land in three different places

The request named `sqrt`, `Lagrange()` and `Fourier()` as if they were one kind of
thing. They are not, and the spread is the clearest possible illustration:

| Example | Tier | Why |
|---|---|---|
| `sqrt` | **1**, and in the prelude as a **method** | One instruction on every target; correctly rounded by IEEE-754; and `stdlib-core.md` §8.2 already made it `x.sqrt()` rather than `sqrt(x)` so that it costs no global name |
| `Lagrange()` | **3**, three times over, under three different names, and one of them is not a function | §2.3 |
| `Fourier()` | **3**, and it is not even Science code — `scientific-libraries.md` §2 links PocketFFT | §2.3 |

`Fourier()` is worth dwelling on because it is the strongest counter-example to
the whole idea of a large intrinsic surface: the fast Fourier transform is the
single most-wanted function in scientific computing, and it is *linked C*, it is
**F1** (its output shape is a function of its input shape), and it has variants
(normalisation convention, real versus complex, in-place versus out-of-place).
It fails every one of the tier 1 and tier 2 tests, and it fails them on the
grounds the tests were written for.

---

## 2. The naming rule

### 2.1 The problem, taken from the owner's own examples

An eponym names a *person*, and a person is not an operation. Four cases, all of
them in the catalogue already:

| Eponym | Means, at minimum |
|---|---|
| **Lagrange** | Lagrange interpolation (a polynomial through points); Lagrange multipliers (constrained optimisation); Lagrangian mechanics (`L = T - V`); the Lagrange points of a two-body system; Lagrange's four-square theorem |
| **Fourier** | The discrete transform; the continuous transform; the Fourier series; Fourier analysis as a field; Fourier's law of heat conduction |
| **Newton** | A root finder; a unit of force; three laws of motion; the law of gravitation; Newton's method in optimisation (a different algorithm from the root finder); Newton divided differences (interpolation); Newton's law of cooling; a non-Newtonian fluid |
| **Bessel** | Four function families (`J`, `Y`, `I`, `K`); a filter family in signal processing; Bessel's correction in statistics (the `n-1` denominator) |

A catalogue that spells any of these as a bare function name has one name for
five things, and the reader cannot recover which from the call site. This is how
a scientific library becomes unusable, and it is worse here than elsewhere for a
reason specific to this project: `llm-ergonomics.md` observes that Science has no
training corpus, so a model writing Science generalises from names. A name that
means five things teaches five wrong generalisations.

### 2.2 Decision 1 — the rule

> **Decision 1. Four clauses, applied to every name in this note and in the
> sibling chemistry and biology note.**
>
> **N1. No function is ever named by a bare eponym.** Every function name
> contains the operation it performs. `Lagrange`, `Fourier`, `Newton`, `Bessel`,
> `Brent`, `Hamilton` are not function names.
>
> **N2. Where the eponym selects among *interchangeable* methods for one
> operation, the eponym is a value, not a name.** The operation is a function,
> the method is an argument, and the eponym is an `UpperCamelCase` variant of a
> `choice`:
> ```science
> let x, err be root(f, bracket, method: RootMethod.Brent)
> let area, err be integrate(f, 0.0, 1.0, method: Quadrature.GaussLegendre(64))
> ```
> Interchangeable means: same inputs, same output type, differing only in cost
> and convergence. It is a test, not a judgement — if two candidates cannot be
> swapped without changing the call, they are not interchangeable and N3 applies.
>
> **N3. Where the eponym names a distinct mathematical *object* — a different
> function, a different polynomial family, a different transform — the eponym is
> part of the name, as `eponym_noun` in `snake_case`.** `bessel_j`,
> `legendre_p`, `hermite_h`, `chebyshev_t`, `hilbert_transform`,
> `mellin_transform`.
>
> **N4. An eponym is a type, in `UpperCamelCase`, only when the thing it names
> has state and a lifetime.** `Lagrangian`, `Hamiltonian`, `ChebyshevSeries`.
> Never a type that is merely a namespace for a verb.
>
> **Rejected: the operation only (`interpolate`, `transform`) with the method
> always an argument.** This is N2 taken to the whole catalogue, and it fails on
> N3's cases. `bessel_j` and `bessel_y` are not two methods of computing one
> thing; they are two different functions with different values, and
> `bessel(order: 2, x: 1.0, kind: Y)` hides a mathematical distinction behind a
> tuning-parameter spelling. It also makes the argument list the only
> documentation, which `strings-formatting-and-docs.md`'s `##` doc comments
> attach to names rather than to arguments.
>
> **Rejected: the eponym reserved for a type, with lower-case verbs for
> operations** (`Lagrange` the interpolator, `Fourier` the transformer). This is
> the design that looks cleanest and is worst. It does not disambiguate:
> `Lagrange` as a type is exactly as ambiguous as `Lagrange` as a function,
> because the ambiguity is in the *person*, not in the syntactic category. It
> also inflates the type count — `scientific-libraries.md` §5.7 alone would
> produce fifteen types whose only content is a method — and
> `stdlib-shape-and-packages.md` §4.1 already spends types on things with data.
>
> **Rejected: eponym plus noun, everywhere, with no `method:` argument**
> (`brent_root`, `newton_root`, `bisect_root`, …). Rejected on
> `syntax-revision-2.md` §1's ground, which is the best argument in the corpus
> against duplicate spellings: `scientific-libraries.md` §5.4 lists both
> `rk4` … `radau_iia` *and* `solve_ode(problem, method, tolerance)`, which is two
> ways to write one operation. Revision 2 removed `==`/`is` for exactly this
> reason, and a catalogue must not reintroduce the pattern the syntax just
> deleted.
>
> **Cost.** N2 costs a `choice` declaration per operation family — eight of them
> across §5 — and it costs the caller one argument at sites that used to name the
> method in the function. It also makes a method *not directly callable*: a user
> who wants Brent's method specifically writes `method: RootMethod.Brent` rather
> than `brent(…)`, which is four characters longer and one indirection further
> from the textbook. That is the price and it is paid once per call site.

### 2.3 The rule applied to the four hardest eponyms

This is the demonstration, and it is the part the sibling note should copy.

**Lagrange** becomes three *different kinds of name*, and none of them is
`Lagrange()`:

| Meaning | Spelling | Kind | Tier |
|---|---|---|---|
| Lagrange interpolation | `InterpolationMethod.Lagrange`, passed to `interpolate` | N2 variant | 3 |
| Lagrange multipliers | the `multipliers` field of a constrained `Solution` | not a name at all — an output field | 3 |
| Lagrangian mechanics | `Lagrangian of (T, N)`, a type | N4 type | 3 |
| Lagrange points | `lagrange_points(primary, secondary)` | N3 function | 3 |

Four meanings, four syntactic categories, zero ambiguity, and the reader of a
call site can tell which one they are looking at without knowing the library.

**Fourier**:

| Meaning | Spelling | Notes |
|---|---|---|
| Discrete transform | `fft`, `ifft`, `rfft`, `irfft` | On the closed abbreviation list of `stdlib-shape-and-packages.md` §4.1, which already contains `fft`. Not an eponym at the call site at all |
| The transform, as an operation over a sampled signal with a chosen convention | `dft(signal, norm: Normalisation.Unitary)` | N2 |
| Fourier series coefficients | `fourier_series(f, period, terms)` | N3 |
| Short-time analysis | `stft`, `spectrogram` | Not eponymous; better names |
| Fourier's law of heat conduction | `heat_flux_conduction` | **Not an eponym.** A physical law is named for what it computes |

**Newton** — the interesting one, because it collides across a *namespace*
boundary as well as within one:

| Meaning | Spelling | Why it does not collide |
|---|---|---|
| The root finder | `RootMethod.Newton` | N2 variant |
| Newton's method in optimisation | `MinimiseMethod.Newton` | A different `choice`; same word, different type, and the type is at the call site |
| Divided differences | `InterpolationMethod.NewtonDividedDifference` | N2 variant |
| The unit | `<N>` | **By construction.** `unit-literals.md` §3.3: unit names live in a namespace entered only inside `< >`, disjoint from values, types and modules. `300<N>` and `RootMethod.Newton` cannot see each other |
| Newton's second law | *no name* — it is `force be mass * acceleration` | §6.2 |
| Newton's law of cooling | `cooling_rate` | N3 does not apply; the operation has a name |
| Non-Newtonian fluid | `ViscosityModel.PowerLaw`, `ViscosityModel.Bingham`, … | The eponym was never the useful part |

The unit row is worth stating as a general finding: **`unit-literals.md` §3.3
removes the entire physical-unit half of the eponym problem for free.** Newton,
Pascal, Joule, Hertz, Kelvin, Coulomb, Tesla, Weber, Henry, Farad, Siemens, Gray,
Sievert and Becquerel are all people *and* all SI units, and none of them can
collide with anything, because the unit namespace is closed.

**Bessel** — the owner's implicit example and the one that proves N2 and N3 are
both needed:

| Meaning | Spelling | Clause |
|---|---|---|
| The four function families | `bessel_j`, `bessel_y`, `bessel_i`, `bessel_k` | N3 — four different functions |
| Spherical forms | `spherical_bessel_j`, `spherical_bessel_y` | N3 |
| Zeros | `bessel_j_zero(order, index)` | N3 |
| The filter family | `FilterFamily.Bessel`, passed to `filter_design` | N2 — interchangeable with Butterworth, Chebyshev, elliptic |
| Bessel's correction | *no name* — it is `Variance.Sample` versus `Variance.Population` | `stats`; the eponym describes a denominator, not an operation |

One word, three syntactic categories, and the rule assigned each of them without
a special case.

### 2.4 Checked against `reserved-words.md` and §13

Every name introduced in this note was checked against
`crates/science-lexer/src/token.rs` and against §13's three lists. The findings:

- **Nothing in §5 or §6 collides with a keyword.** Not one. The collisions
  `scientific-libraries.md` §3 had to work around are all in `stats`, `chem` and
  `signal` — `model`, `yield`, `kernel`, `union`, `shape` — and mathematics and
  physics use none of those words.
- **`at` is free again**, per `syntax-revision-2.md` §1.2, which removed the
  comparison phrases. This note nevertheless keeps `poly.evaluate(x)` rather than
  reverting to `poly.at(x)`, and the reason is not reservation: `at` is a
  preposition and reads as a location, while a polynomial evaluation is an
  operation. `scientific-libraries.md` §3 renamed it under duress and landed on
  the better name by accident. Named, so it is not silently reverted later.
- **`const` remains a keyword and the constants module remains `constants`**, per
  `reserved-words.md` §5.6, which needs no language change.
- **`mod` is claimed by §5.6's number theory** as `mod_inverse` and `mod_pow`,
  and, if `reserved-words.md` §2.1's recommendation lands, as `mod(a, b)`. This
  note writes `remainder` and `rem_euclid` today and records `mod` as a
  beneficiary of that recommendation — a fifth voice for it.
- **`pure`** is reserved and `effects.md` wants `pure function`. Almost every
  signature in this note is a pure function in that sense, and §8.3 says what
  that buys.
- **The one word this note wants and cannot have is `assert`**, for the tolerance
  assertions a numerics test suite is made of. `reserved-words.md` §4.2 keeps it
  reserved and asks for it to be *implemented*; this note is a consumer of that
  and asks for nothing new.

### 2.5 What Decision 1 changes in `scientific-libraries.md`, named

`README.md`'s convention is *say what you contradict*. Six changes, each with the
section and the reason:

| Section | Today | Under Decision 1 | Reason |
|---|---|---|---|
| **§5.3 quadrature** | `gauss_legendre`, `clenshaw_curtis`, `tanh_sinh`, `romberg`, `simpson`, `trapezoid` *and* `integrate` | `integrate(f, a, b, method:)` with those as variants; `gauss_legendre_nodes(n)` survives as a separate N3 function because it returns nodes and weights, which is a different operation | N2 interchangeability holds exactly; two spellings for one operation is what `syntax-revision-2.md` §1 removed |
| **§5.4 ODEs** | `rk4`, `bdf`, `radau_iia`, `verner`, … *and* `solve_ode(problem, method, tolerance)` | `solve_ode` only; the rest are `OdeMethod` variants | Same. §5.11's own signature already takes `method: Method` |
| **§5.5 interpolation** | twenty free functions | `interpolate` and `Interpolator.fit`, with the twenty as `InterpolationMethod` variants; `bspline_basis` survives (N3, different object) | Same |
| **§5.7 root finding** | `bisect`, `brent`, `newton`, `halley`, `ridders`, `toms748`, … | `root(f, bracket, method:)`; `find_all_roots` and `bracket_root` survive | Same, and `newton` as a bare name is N1's worst case |
| **§8.1–8.5 optimize** | `lbfgs`, `nelder_mead`, `adam`, … each with a signature | `minimise(objective, start, method:, settings:)` | See the cost below |
| **§9.1 transforms** | `hilbert`, `mellin`, `hartley`, `czt`, `cwt_morlet` | `hilbert_transform`, `mellin_transform`, `hartley_transform`, `chirp_z_transform`, `wavelet_continuous(signal, wavelet: Wavelet.Morlet, …)` | N1 and N3 |

**The optimize row costs something real and it is named rather than hidden.**
`scientific-libraries.md` §8.6 writes `lbfgs` with a `(function …)?`
gradient parameter and says in its own comment that this is *"a decision the
caller should see in the type."* Folding the methods into `minimise(…, method:)`
moves the gradient into `Settings`, one hop further from the signature. That is a
genuine loss. It is accepted because the alternative is forty entry points whose
signatures differ in ways the caller must learn one at a time, and it is mitigated
by making `Settings` a named type with a `gradient: (function(…) -> …)?` field, so
the choice is still in a type — just not in the function's own.

### 2.6 What the sibling note inherits

The chemistry and biology note applies N1–N4 unchanged. The cases it will hit:

- **N2**: `align(a, b, method: Alignment.SmithWaterman)` rather than
  `smith_waterman` and `needleman_wunsch` as two functions; `tree(distances,
  method: Phylogeny.NeighbourJoining)`; `equation_of_state(…, model: VanDerWaals)`.
- **N3**: `michaelis_menten_rate`, `arrhenius_rate`, `nernst_potential`,
  `hardy_weinberg_frequencies`, `henderson_hasselbalch_ph` — all eponym plus the
  operation noun, all unambiguous.
- **N4**: no eponymous type in either domain is obviously needed; if one appears,
  it needs state.
- **The unit namespace protects `Pascal`, `Kelvin`, `Curie`, `Gray`, `Sievert`,
  `Dalton`, `Angstrom`** the same way it protects `Newton` (§2.3).
- **One word the sibling must not use and this note does not touch**: `yield`, per
  `scientific-libraries.md` §3 and `reserved-words.md` §2.2. `produced` until the
  recommendation lands.

---

## 3. Tier 1 — what is intrinsic because of hardware

### 3.1 The trap: an LLVM intrinsic is not an instruction

This is the whole section, and everything else in §3 follows from it.

`llvm.sqrt.f64` and `llvm.sin.f64` are both LLVM intrinsics. They are spelled
identically, they are declared in the same file, and a design note that says
"lowers to an LLVM intrinsic" says nothing about either. What actually happens:

| Intrinsic | x86-64 | AArch64 |
|---|---|---|
| `llvm.sqrt.f64` | `sqrtsd` — one instruction, baseline SSE2 | `fsqrt d` — one instruction |
| `llvm.sin.f64` | `call sin` — **a libcall into libm**, a polynomial with argument reduction, tens to hundreds of cycles | `call sin` — the same |

So `sin` is a function call that happens to be written `llvm.sin` in the IR.
`scientific-libraries.md` §2's table entry — *"Elementary functions: **Write** over
LLVM intrinsics. `sin`, `exp`, `sqrt` lower to intrinsics. No FFI"* — is correct
about `sqrt` and misleading about `sin` and `exp`, because it uses one word for
two costs that differ by two orders of magnitude.

> **Decision 2. Tier 1 membership is decided by the *lowering*, not by the
> existence of an intrinsic. A name is tier 1 only if `sciencec` can point at the
> instruction it becomes, on every target in the support matrix, and name the
> target on which it cannot.**
>
> **Rejected: "tier 1 means there is an LLVM intrinsic."** It is the definition
> that produces the failure mode this note exists to avoid — a documented
> "intrinsic" `sin` that is a libcall, teaching every reader that transcendental
> functions are free. The whole value of the tier is that it is a **cost
> statement**, and a cost statement that is false is worse than no statement.
>
> **Rejected: "tier 1 means one instruction, exactly."** Too strict, and it
> excludes `min`, `abs` and `is_nan`, which are two or three instructions with no
> branch, no call and no memory traffic. The honest boundary is *bounded, small,
> branch-free, and no symbol*.
>
> **Cost.** The tier 1 list becomes target-dependent at the margin, which means
> the documentation carries a per-target column. §3.7 shows what that column looks
> like and why it is worth the space.

### 3.2 Decision 3 — the tier 1 floating-point list

> **Decision 3. Eleven floating-point operations are tier 1.**

| Science | LLVM | x86-64 baseline (v1, SSE2) | x86-64-v2 (SSE4.1) | x86-64-v3 (AVX2+FMA) | AArch64 | IEEE-754 exact? |
|---|---|---|---|---|---|---|
| `sqrt` | `llvm.sqrt` | `sqrtsd` | `sqrtsd` | `vsqrtsd` | `fsqrt` | **yes** (clause 5.4.1) |
| `fma` | `llvm.fma` | **libcall** — software, ~20–50× | **libcall** | `vfmadd213sd` | `fmadd` | **yes** (clause 5.4.1) |
| `abs` | `llvm.fabs` | `andpd` + mask | same | `vandpd` | `fabs` | **yes** (clause 5.5.1) |
| `copysign` | `llvm.copysign` | 2–3 bit ops | same | same | 2 ops | **yes** (clause 5.5.1) |
| `min` | `llvm.minimum` | cmp + select, 3–5 | same | same | `fmin` | **yes** (clause 9.6) |
| `max` | `llvm.maximum` | cmp + select, 3–5 | same | same | `fmax` | **yes** (clause 9.6) |
| `floor` | `llvm.floor` | **libcall** | `roundsd $1` | `vroundsd` | `frintm` | **yes** (clause 5.9) |
| `ceil` | `llvm.ceil` | **libcall** | `roundsd $2` | `vroundsd` | `frintp` | **yes** |
| `trunc` | `llvm.trunc` | **libcall** | `roundsd $3` | `vroundsd` | `frintz` | **yes** |
| `round_ties_even` | `llvm.roundeven` | **libcall** | `roundsd $0` | `vroundsd` | `frintn` | **yes** |
| `round` (ties away) | `llvm.round` | **libcall** | ~5 instructions | ~5 | `frinta` | **yes** |

Three things this table says that a prose sentence would have hidden.

**`fma` is the entry that is not free on baseline x86-64.** FMA3 arrived with
Haswell in 2013 and is in the x86-64-v3 microarchitecture level; below it, `fma`
is a software emulation — a double-double multiply with an exact-addition
sequence — and it is roughly twenty to fifty times a multiply-add. This matters
more here than in most languages because `reproducibility.md` Decision 3 turns
FMA *contraction* off and names explicit `fma` as the escape hatch a user takes
to recover the lost 10–30%. A user on a pre-2013 baseline build who takes that
escape hatch makes their code slower, not faster. **Naming this is the single
most load-bearing line in §3**, and §3.7 says what to do about it.

**`min` and `max` are exactly specified and are not one instruction on x86.**
x86's `minsd` is not IEEE minimum: it returns its second operand when either
input is NaN, and it does not order `-0.0` below `+0.0`. IEEE-754-2019's
`minimum` propagates NaN and orders the zeros. LLVM therefore emits a compare and
select sequence for `llvm.minimum` on x86 and a single `fmin` on AArch64. The
semantics are pinned and the cost is not. Both facts belong in the table; only
one of them would have survived a sentence.

**`round` is the one place where AArch64 is the cheap target.** `frinta` is
round-half-away-from-zero in hardware; x86 has no such rounding mode and needs a
sequence. This is the reverse of every other row, and it is why
`round_ties_even` is added to the prelude in §4.3: it is the single-instruction
form on *both* targets, and it is also the one that does not bias a sum.

### 3.3 Decision 4 — the tier 1 integer list

Number theory and combinatorics are the consumers here, and they are the reason
this list is not an afterthought: binary GCD is `trailing_zeros` in a loop,
population count is the inner loop of every bitset-based sieve, and modular
exponentiation wants `mul_high`.

> **Decision 4. Ten integer operations are tier 1.**

| Science | x86-64 baseline | x86-64-v2 / v3 | AArch64 | Notes |
|---|---|---|---|---|
| `abs` (signed) | 3 branch-free | same | 2 | Traps on `MIN`; `wrapping_abs` does not |
| `min` / `max` | `cmp` + `cmov` | same | `csel` | |
| `count_ones` | ~12 (SWAR) | `popcnt` (SSE4.2) | `cnt` + `addv` | The baseline fallback is the largest gap in the table |
| `leading_zeros` | `bsr` + fixup | `lzcnt` (BMI1) | `clz` | `bsr` is undefined on zero, hence the fixup |
| `trailing_zeros` | `bsf` + fixup | `tzcnt` (BMI1) | `rbit` + `clz` | |
| `reverse_bytes` | `bswap` | `bswap` | `rev` | |
| `rotate_left` / `rotate_right` | `rol` / `ror` | same | `ror` | |
| `wrapping_*` | the plain instruction | same | same | Two's complement is exact by definition |
| `checked_*` | instruction + flag test | same | flag test | One branch, and it is predictable |
| `mul_high` | `mulq` high half | same | `umulh` / `smulh` | The 128-bit intermediate is free in hardware and expensive in software |

`count_ones` on baseline x86-64 is the row that fails question 4 of §1.2's tier 1
test hardest: twelve instructions is not "small" by the standard the other rows
set. It is kept in tier 1 because `popcnt` has been present on every x86 part
shipped since 2008 and every AArch64 part ever, and because the fallback is
branch-free and constant-time. The honest statement is *tier 1 on every target
anybody builds for, with a named twelve-instruction fallback on a baseline nobody
targets*, and that statement is in the table rather than in a footnote.

### 3.4 Tier 1b — bit-pattern operations

A short list that is not "one instruction" but is emphatically not a library
call, and that would be miscategorised in either direction:

`is_nan` `is_infinite` `is_finite` `is_normal` `sign` `signum` `classify`
`fract` `clamp` `next_after` `ulp` `to_bits` `from_bits`

All of these are comparisons, masks and shifts on the bit pattern: two to six
instructions, branch-free except `classify`, no symbol, no table. They are
exactly specified — a bit pattern is a bit pattern — and therefore bit-identical
across targets.

They are called out as **1b** rather than folded into tier 1 for one reason: a
reader who sees `is_nan` beside `sqrt` in an "intrinsics" list will reasonably
conclude that `is_nan` is an instruction, and it is not. The sub-tier costs one
line of documentation and prevents a wrong inference about a function that
appears in every hot loop that guards against bad data.

### 3.5 Decision 5 — `sin` and `cos` are not intrinsic, and reciprocal square root is refused

These are the two candidates the request named as "on some targets", and both
answers are no, for different reasons.

> **Decision 5a. `sin`, `cos`, `tan`, `exp`, `ln`, `log2`, `log10`, `pow`,
> `hypot`, `cbrt`, `atan2` and every inverse and hyperbolic form are tier 2
> library code. `sciencec` never lowers them to a target's approximate
> transcendental instruction.**
>
> The hardware that looks like a counter-example does not survive inspection:
>
> - **x87 `fsin` / `fcos` / `f2xm1` / `fyl2x`.** These exist and are unusable.
>   `fsin`'s argument reduction uses a 66-bit approximation of π, so for
>   arguments near a multiple of π it loses every significant digit — Intel's own
>   documentation was corrected to state worst-case errors of billions of ULP.
>   They are also microcoded and slow, and they operate on the x87 stack, which
>   no SSE2 or AVX code path touches. The System V AMD64 ABI computes in SSE2
>   registers. There is no lowering.
> - **AVX-512 `vexp2pd`, `vgetexp`, `vscalef`, `vrndscale`.** These are *helpers*
>   for building `exp` — an exponent extract, a scale by a power of two, a
>   2^x with restricted accuracy. They are not `exp`, they are three of the
>   fifteen instructions a good `exp` is made of, and they are AVX-512, which is
>   not in any microarchitecture level Science can assume.
> - **GPU `sin.approx.f32` (PTX), `v_sin_f32` (AMD).** These are real
>   instructions and they are approximations: roughly 2 ULP over a restricted
>   argument range, with no guarantee outside it. Lowering `sin` to them would
>   make the same source produce different numbers on a GPU and a CPU, which is
>   precisely the class of change `reproducibility.md` Decision 3 forbids: *"a
>   value-changing floating-point transform"* enabled by nobody's decision.
>
> **Say it plainly, because the alternative misleads about cost: every elementary
> function except `sqrt` is library code that happens to be fast.** A good `exp`
> is an argument reduction, a degree-6 to degree-11 polynomial and a scale — tens
> of cycles, not one. A good `sin` over the full range is that plus a Payne–Hanek
> reduction for large arguments. Presenting them as intrinsics tells a user that a
> loop of a million `sin` calls costs the same as a loop of a million square
> roots, and it does not.
>
> **Rejected: a `--fast-transcendental` flag** that permits the approximate
> lowering. `reproducibility.md` Decision 3 already rejected `--fast-math` with
> the argument that *"every language that ships one discovers it in somebody's
> published build"*, and a narrower version of the same flag has the same fate.

> **Decision 5b. Reciprocal square root is not in the catalogue at all.**
>
> `rsqrtss` (x86, ~12 bits of mantissa), `vrsqrt14ps` (AVX-512, 14 bits) and
> `frsqrte` (AArch64, ~8 bits) are genuinely single instructions and genuinely
> fast, and they are *approximations of different quality on each target*. An
> `rsqrt` specified to 1 ULP is not the instruction — it is the instruction plus
> one or two Newton–Raphson refinement steps, which is five to ten instructions
> and is no longer obviously better than `1.0 / x.sqrt()`. An `rsqrt` specified as
> "the hardware approximation" is a function whose result differs between a
> laptop and a cluster by 2⁻⁸ versus 2⁻¹⁴, which is a four-thousand-fold
> difference in the answer's last digits and is exactly the reproducibility
> failure `reproducibility.md` §3.2 is about.
>
> **Rejected: `approximate_rsqrt` with a documented per-target error bound.** It
> is honest, it is what the games industry wants, and it is a name that will
> appear in a published simulation within a year of shipping. The audience of
> this language publishes. The 2–3× win on a normalisation-heavy inner loop is
> real and is refused.
>
> **Cost.** Normalising a large array of vectors is measurably slower than in C
> with `-ffast-math`, and somebody will benchmark it. The mitigation is honest:
> the comparison is against a C build that is also not reproducible, and the
> benchmark should say so.

### 3.6 Decision 6 — tier 1 is bit-identical, which refines `stdlib-core.md` §8.3

`stdlib-core.md` §8.3 states one contract for all of Level 1 math:

> Level 1 math functions are specified to within **1 ULP** of the correctly-rounded
> result. They are **not** guaranteed bit-identical across targets.

`reproducibility.md` §3.2's finding G1 calls this *"the most serious finding in
the note"*, because `random`'s frozen stream is built on functions that are not
frozen, and its Decision 2 answers by giving `random` its own correctly-rounded
`erfinv`, `ln`, `exp` and `sqrt`.

**Tier 1 is a strictly stronger contract than §8.3 states, and the difference is
useful to both notes.**

> **Decision 6. Every tier 1 and tier 1b operation is *exactly* specified and is
> bit-identical across every target. The 1-ULP contract of `stdlib-core.md` §8.3
> applies to tier 2 and tier 3 only.**
>
> The reason is not generosity, it is IEEE-754. Clause 5.4.1 requires `sqrt` and
> `fusedMultiplyAdd` to be correctly rounded; clause 5.5.1 makes `abs` and
> `copysign` sign-bit manipulations with no rounding at all; clause 5.9 makes the
> round-to-integral operations exact; clause 9.6 specifies `minimum` and
> `maximum` completely. There is no implementation freedom left to differ about.
> The integer list of §3.4 is exact for the same reason, one layer down.
>
> **This is not an amendment to §8.3; it is a partition of it.** §8.3's sentence
> is true of `sin` and false of `sqrt`, and it was written at a granularity that
> could not tell them apart. Stated as *"tier 1 is exact, tier 2 is 1 ULP"* it
> becomes true everywhere and it gives `reproducibility.md` something it currently
> lacks: a named set of functions a frozen contract may call.
>
> **A concrete saving for `reproducibility.md` §3.2's Decision 2.** That decision
> has `random` carry *"its own `erfinv` and its own `ln`, `exp` and `sqrt`"*.
> Under Decision 6, **`sqrt` does not need re-implementing** — it is correctly
> rounded by the hardware on every target, and a second software `sqrt` would be
> slower and no more exact. The list shortens to `erfinv`, `ln` and `exp`, which
> is three functions instead of four and removes the one that was most obviously
> redundant. This is offered as a correction to that note's §3.2 rather than taken
> as a decision here, because `random`'s contract is that note's to set.
>
> **Cost.** Two contracts instead of one, and a reader must know which tier a
> function is in to know which applies. Mitigated by the summary table of §9,
> which carries the tier of every name in the note, and by the fact that the
> strong contract covers exactly the operations whose names are shortest and most
> written.

### 3.7 What to do about `fma`'s target dependence

The problem, stated once: `reproducibility.md` Decision 3 makes explicit `fma`
the sanctioned way to get the performance that contraction would have given, and
on x86-64-v1 and v2 explicit `fma` is a slow software emulation. A user following
the note's advice makes their program slower on exactly the machines where they
cannot tell.

> **Decision 7. `fma` stays tier 1, and the *target's* FMA availability becomes a
> recorded field rather than a hidden one.**
>
> - The microarchitecture level `sciencec` targets is already a build input.
>   `package-manager.md` §2.3's `[build]` table and `reproducibility.md` §5.1's
>   provenance record both exist; this note asks that the target feature level
>   join them, beside the float policy that Decision 3 already put there (§11).
>   The cost is one string in a record that already exists.
> - `native-dependencies.md` already probes the machine for BLAS variants. The
>   same probe knows whether FMA is present. No new mechanism.
> - **A diagnostic is wanted and is not taken here.** A warning on a call to `fma`
>   in a build whose target level lacks hardware FMA — *"this lowers to a software
>   emulation roughly 20× the cost of a multiply and an add; raise the target
>   level or write `a * b + c`"* — is a codegen diagnostic, and `README.md` says
>   to check the table before allocating. §11 asks `native-dependencies.md`, which
>   owns `SC0471`–`SC0479` and has three free codes, to take it.
>
> **Rejected: demote `fma` to tier 2.** It is one instruction on every target
> shipped since 2013 and on every AArch64 part ever made. Demoting the common
> case to describe the uncommon one gets the cost statement wrong in the other
> direction.
>
> **Rejected: make `fma` fall back to `a * b + c` on targets without hardware.**
> This is what a naive implementation does and it is a correctness bug: `fma`'s
> single rounding is the *reason* it is called, in exactly the compensated-summation
> and double-double algorithms where a double rounding destroys the result. A slow
> `fma` is a correct `fma`; a fast one that rounds twice is not an `fma`.

---

## 4. Tier 2 — the prelude

### 4.1 `stdlib-core.md` §8.2 is adopted whole, and what it means for this catalogue

§8.2 decided that the Level 1 math surface is **methods on `F32`, `F64` and the
integer types**, not free functions, and gave the decisive reason:

> A Level 1 free function is a global name forever. The elementary set contains
> `abs`, `min`, `max`, `round`, `sign`, `clamp`, `pow` and `trunc` — eight words
> users want for their own functions […] As methods they cost nothing […] Zero
> new global names against roughly thirty-five is the entire argument.

This note adopts that without amendment, and three consequences run through
everything below.

**First: the prelude has no name budget, so the prelude question stops being a
namespace question and becomes purely a §1.2 question.** That is a bigger change
than it looks. The usual argument against a large prelude — *it steals the good
names* — does not apply to a prelude made of methods. What remains is only
"should this behaviour be pinned forever", which is a cleaner question and has
better answers.

**Second: it constrains what can be added.** A method needs a receiver. `atan2`
works (`y.atan2(x)`); `hypot` works (`x.hypot(y)`); `fma` works
(`a.fma(b, c)`, which reads as *a times b plus c* and must be documented as such
because the argument order is not obvious). An operation with no natural receiver
cannot be in the prelude at all under §8.2's rule — which is a large part of why
physics cannot be (§4.5).

**Third: `scientific-libraries.md` §5.11's `function sqrt(x: F64) returns F64`
is superseded**, both in spelling (`returns` → `->`, per `syntax-revision-2.md`
§4) and in form (free function → method). `stdlib-core.md` §8.2 already says so
and names the three-way drift it settled; this note is the fourth voice and
changes nothing.

### 4.2 Decision 8 — three extension rules

The request asked for the prelude boundary to be *argued*, not asserted.
`stdlib-core.md` §8.1 drew a line; this note extends it, and extends it by rule
rather than by taste, so that the sibling note and any future note can apply the
same rules without re-litigating.

> **Decision 8. Three rules for what crosses into the prelude beyond
> `stdlib-core.md` §8.1's list. A candidate needs one of them, and needs to pass
> §1.2 regardless.**
>
> **P1 — the prelude does not split a closed pair.** If one half of a
> universally-paired operation is in the prelude, both are. Half a pair in scope
> and half behind an import is a boundary nobody can remember, and it produces
> the specific failure where a user writes the import-free half and hand-rolls
> the other.
>
> *Admits:* `exp2` (because `log2` is in), `exp10` (because `log10` is in),
> `asinh`, `acosh`, `atanh` (because `sinh`, `cosh`, `tanh` are in).
>
> **P2 — the prelude carries the numerically stable form of an operation whose
> naive form it already carries.** If `ln` and `exp` are one keystroke away and
> the stable variants are behind an import, users will write the unstable
> expression, because they will not know there is anything to import. This is a
> correctness rule, not a convenience rule, and it is the only rule here that
> catches a bug.
>
> *Admits:* `ln1p` (`ln(1 + x)` loses every significant digit for small `x`),
> `expm1` (`exp(x) - 1` does the same), `fma` (a multiply-add without an
> intermediate rounding, which is what every compensated algorithm needs),
> `hypot` (already in §8.1; the rule confirms it — `(x*x + y*y).sqrt()`
> overflows at `x > 1.3e154` where `hypot` does not).
>
> **P3 — the prelude carries what the language took away.** Where a language
> decision removed a capability users had elsewhere, the sanctioned replacement
> is as available as the thing it replaced.
>
> *Admits:* `fma` again, and decisively. `reproducibility.md` Decision 3 turns
> off FMA contraction, which every C and Fortran toolchain does by default. The
> compiler stopped doing something for the user; the explicit form must not then
> cost an import. This is the strongest single prelude argument in the note
> because it is the only one grounded in another note's decision rather than in
> taste.
>
> *Also admits:* `round_ties_even`. `reproducibility.md`'s whole programme is
> about the same source producing the same number; `round` is
> half-away-from-zero, which biases a sum of rounded values away from zero, and
> ties-to-even is the IEEE default and the unbiased one. Having only the biased
> form in scope is the same shape of problem as having only `ln(1+x)`.
>
> **Rejected: P4, "anything a first-year physics problem set needs."** It was
> drafted and removed. It admits `gamma`, `erf`, `lerp` and eventually
> `integrate`, it has no edge, and it is the rule by which every language's
> prelude grows until nobody can hold it. The three rules above each name a
> mechanism; a rule that names an audience names nothing.
>
> **Cost.** Eleven names — `exp2`, `exp10`, `asinh`, `acosh`, `atanh`, `ln1p`,
> `expm1`, `fma`, `copysign`, `round_ties_even`, `MIN_POSITIVE` — added to a
> surface `stdlib-core.md` §8.1 sized at roughly thirty-five. Each is an ABI
> commitment in `science-rt` (§8 of the core spec: *"the representation of every
> library type"*) and a page in the reference. Against that: zero new global
> names, because §8.2 made them methods, and eleven fewer `use math` lines in the
> programs that need them.

### 4.3 The prelude surface, complete

Methods on `F64` and `F32`. Tier column: **1** = §3.2/§3.3, **1b** = §3.4, **2** =
library code.

| Group | Names | Tier | Source |
|---|---|---|---|
| Roots and powers | `sqrt` | 1 | §8.1 |
| | `cbrt` `hypot` `pow` | 2 | §8.1 |
| Exponentials | `exp` `ln` `log2` `log10` | 2 | §8.1 |
| | `exp2` `exp10` | 2 | **new**, P1 |
| | `expm1` `ln1p` | 2 | **new**, P2 |
| Trigonometry | `sin` `cos` `tan` `asin` `acos` `atan` `atan2` | 2 | §8.1 |
| | `to_degrees` `to_radians` | 2 | §8.1 |
| Hyperbolic | `sinh` `cosh` `tanh` | 2 | §8.1 |
| | `asinh` `acosh` `atanh` | 2 | **new**, P1 |
| Sign and magnitude | `abs` `sign` | 1 / 1b | §8.1 |
| | `copysign` | 1 | **new**, P1 (pairs with `sign`) |
| Rounding | `floor` `ceil` `trunc` `round` `fract` | 1 / 1b | §8.1 |
| | `round_ties_even` | 1 | **new**, P3 |
| Comparison | `min` `max` `clamp` | 1 / 1b | §8.1 |
| Division | `rem_euclid` | 2 | §8.1 |
| | `div_euclid` | 2 | **new**, P1 (pairs with `rem_euclid`) |
| Fused | `fma` | 1 | **new**, P2 and P3 |
| Predicates | `is_nan` `is_infinite` `is_finite` `is_close` | 1b / 2 | §8.1 |
| Constants | `PI` `E` `TAU` `INFINITY` `NAN` `EPSILON` `MIN` `MAX` | — | §8.1 |
| | `MIN_POSITIVE` | — | **new**, P1 (pairs with `MIN`, and `MIN` is the most-confused constant in every language that has both) |

Methods on the integer types:

| Group | Names | Tier |
|---|---|---|
| Arithmetic | `abs` `sign` `pow` `min` `max` `clamp` `rem_euclid` `div_euclid` | 1 / 1b / 2 |
| Bits | `count_ones` `count_zeros` `leading_zeros` `trailing_zeros` `reverse_bytes` `rotate_left` `rotate_right` | 1 |
| Overflow discipline | `wrapping_add` `wrapping_sub` `wrapping_mul` `checked_add` `checked_sub` `checked_mul` `saturating_add` `saturating_sub` `saturating_mul` | 1 |
| Constants | `MIN` `MAX` `BITS` | — |

The bit methods are not in `stdlib-core.md` §8.1, which did not enumerate the
integer surface. They are proposed here under P3 in its widest reading — the
language has fixed-width integers, and the operations that make a fixed-width
integer usable are not a mathematical library. `mul_high` is **not** in the
prelude: it is a number-theory primitive, it has no natural receiver reading
(`a.mul_high(b)` is defensible but obscure), and `use math` is where a user
computing modular products already is.

### 4.4 What is refused, and why each refusal is not arbitrary

| Candidate | Refused on | The concrete reason |
|---|---|---|
| `gamma`, `ln_gamma`, `erf`, `erfc`, `bessel_*`, every special function | §1.2 Q1, Q5 | `stdlib-core.md` §8.1 already refused these and this note does not reopen it. The refusal is *accuracy improves*: a better `bessel_j` is a library release, and `scientific-libraries.md` §2 explicitly wants these re-writable |
| `integrate`, `solve_ode`, `root`, `interpolate`, `minimise` | §1.2 Q4 | Decision 1's N2 makes the method a literal argument. A module whose variants are its parameters has variants by construction — and Decision 1 *increased* the number of operations that fail this way, which is a point in its favour |
| `Complex`, `Polynomial` | §1.3 | Pass §1.2, fail the second gate: twenty-five methods each on a type most programs never mention |
| `next_after`, `ulp`, `to_bits`, `from_bits` | §1.3 | Tier 1b, so cheap; refused anyway. These are for people writing numerics *libraries* and tolerance tests, not for people doing science. `use math` |
| `is_normal`, `classify` | §1.3 | Same |
| `lerp` | §1.2 Q1, Q4 | It has variants and they disagree. `a + t * (b - a)` is monotonic and not exact at `t = 1`; `(1 - t) * a + t * b` is exact at both ends and not monotonic. Two implementations, two answers, and the prelude has no site at which to express the choice. The cleanest §1.2 failure in the whole list |
| `sin_cos` | §1.3 | A performance fusion of two prelude methods, not a new operation. `use math` |
| `log_base` | §1.3 | `x.ln() / b.ln()` with no accuracy advantage, and unlike `exp2`/`exp10` there is no direct implementation to be more accurate than |
| `sum`, `mean`, `norm`, `dot` | not this note's | `collections-and-chains.md` owns the chain terminals; `scientific-libraries.md` §5.9 owns the free forms. Named so that no reader thinks this note forgot them |
| Physical constants | §4.5 | |

### 4.5 Why physics cannot be in the prelude at all

Three independent reasons, each sufficient.

**The names are unavailable and would be catastrophic if they were not.** The
prelude already owns `E` for Euler's number and `PI` for π. Physics wants `E` for
energy, `e` for the elementary charge, `c` for the speed of light, `h` for Planck's
constant, `k` for Boltzmann's constant, `G` for the gravitational constant and
`R` for the gas constant. Every one of those is a single letter that a user will
bind as a local variable in the first ten lines of a physics program. Under §8.2
a prelude constant is an associated constant on a *type*, which is why `PI` is
affordable — `F64.PI` is not a global name. `SPEED_OF_LIGHT` is not an associated
constant on any type, so it would have to be a global, and a global is what §8.2
spent its whole argument avoiding.

**CODATA is not pinned, and §1.2's first question is fatal.** The 2019 SI
redefinition fixed **seven** constants exactly, by definition:

| Constant | Exact value | Unit |
|---|---|---|
| `CAESIUM_FREQUENCY` | 9 192 631 770 | `<Hz>` |
| `SPEED_OF_LIGHT` | 299 792 458 | `<m/s>` |
| `PLANCK` | 6.626 070 15 × 10⁻³⁴ | `<J*s>` |
| `ELEMENTARY_CHARGE` | 1.602 176 634 × 10⁻¹⁹ | `<C>` |
| `BOLTZMANN` | 1.380 649 × 10⁻²³ | `<J/K>` |
| `AVOGADRO` | 6.022 140 76 × 10²³ | `<1/mol>` |
| `LUMINOUS_EFFICACY` | 683 | `<lm/W>` |

Every other constant in `scientific-libraries.md` §12.1's list is **measured**, and
measured values are revised: CODATA 2022 (published 2024) moved the fine-structure
constant, the electron mass and the Rydberg constant relative to CODATA 2018. The
gravitational constant `G` has a relative uncertainty of about 2.2 × 10⁻⁵ and is
the worst-known constant in physics; its recommended value has moved in every
adjustment. A prelude constant is a permanent promise about a number, and a
measured constant cannot be promised. This produces a decision worth stating in
its own right:

> **Decision 9. `physics.constants` is tier 3 in full, and the module is split
> internally into the seven exact constants and the measured remainder. The
> measured ones carry an uncertainty and the module carries a CODATA vintage in
> its version.**
>
> The seven exact constants would pass §1.2 Q1 — they cannot be revised without
> redefining the SI, which is a generational event. They are kept in tier 3 anyway,
> on the name argument above and on a second: a user who imports `SPEED_OF_LIGHT`
> and `ELECTRON_MASS` from two different places, because one is exact and one is
> not, has been handed a distinction that helps nobody at the call site.
>
> `uncertainty.md` §8.3 already asks for wave 2 to ship these *"with units and
> uncertainties […] the constant is `9.80665(15)<m/s^2>`"*. This decision is the
> same request seen from the other side and adds only the vintage: a result
> computed against CODATA 2018 and one computed against CODATA 2022 are different
> results, and `reproducibility.md` §5.1's provenance record should be able to say
> which. That is a package version, not a language feature.
>
> **Cost.** Every physics program has a `use physics.constants (…)` line. That is
> the correct cost and it is one line.

**The types do not exist yet.** Every physics signature in §6 takes a `Quantity`,
and `Quantity` needs const-expression arithmetic in type position —
`scientific-libraries.md` §12.4, `const-expression-arithmetic.md` in full. A
prelude name whose parameter type is not yet expressible cannot be a prelude
name. This is the reason that does not depend on taste at all, and it is why the
question does not even become askable until wave 5 of §13's sequencing.

---

## 5. Mathematics, catalogued and tiered

Seven domains. Each gives the catalogue with its tier, the five most
representative signatures, and one realistic example. Names follow Decision 1;
where a name differs from `scientific-libraries.md` §5, §2.5 says why.

Every signature is generic over `T: Real` where it can be, per §8, and uses
`->`, `of`, `borrowed` and `T?` per `syntax-revision-2.md` §§3–6.

### 5.1 Elementary and transcendental

This is the only domain with tier 1 members, and it is the domain where the
three-tier distinction does the most work.

| Group | Names | Tier |
|---|---|---|
| Exact, hardware | `sqrt` `fma` `abs` `copysign` `min` `max` `floor` `ceil` `trunc` `round` `round_ties_even` | **1** |
| Bit pattern | `sign` `fract` `clamp` `is_nan` `is_infinite` `is_finite` `is_normal` `classify` `next_after` `ulp` `to_bits` `from_bits` | **1b** |
| Exponential | `exp` `exp2` `exp10` `expm1` `ln` `ln1p` `log2` `log10` | **2** |
| | `log_base` `powi` `pow` | `pow` **2**, the rest **3** |
| Roots | `cbrt` `hypot` | **2** |
| | `hypot3` `nth_root` | **3** |
| Trigonometric | `sin` `cos` `tan` `asin` `acos` `atan` `atan2` `sinh` `cosh` `tanh` `asinh` `acosh` `atanh` `to_degrees` `to_radians` | **2** |
| Trigonometric, extra | `sin_cos` `sinpi` `cospi` `tanpi` `secant` `cosecant` `cotangent` `versine` `haversine` `exsecant` | **3** |
| Division and remainder | `rem_euclid` `div_euclid` | **2** |
| | `remainder` `fdim` `modf` `frexp` `ldexp` `scalbn` `logb` | **3** |
| Comparison | `is_close` | **2** |
| | `is_close_within` `total_order` `total_order_magnitude` | **3** |
| Scalar interpolation | `lerp` `inverse_lerp` `smoothstep` `remap` | **3** — §4.4 |

`sinpi`, `cospi` and `tanpi` deserve a word, because they look like clutter and
are not: `sin(PI * x)` reduces an argument that has already lost precision to the
rounding of `PI`, while `sinpi(x)` reduces against an exact integer. They are what
every special-function implementation uses internally — the reflection formula for
`gamma` is `PI / (sinpi(x) * gamma(1 - x))` — and exposing them is free once §5.2
exists.

`haversine` and `versine` are in the catalogue for one reason: geodesy, where
`haversine` is the numerically stable great-circle distance and the naive
spherical law of cosines is not. That is rule P2's criterion applied one tier
down, and it is the reason the tier boundary is a boundary rather than a cliff —
the same argument admits a name to tier 3's *documentation* that it would not
admit to tier 2.

**Five signatures.**

```science
## Tier 1. Lowers to `sqrtsd` / `fsqrt`. Correctly rounded by IEEE-754 §5.4.1,
## therefore bit-identical across targets (§3.6), unlike every other member of
## this section.
F64 has:
    function sqrt(self) -> F64

## Tier 1. `self * factor + addend`, with ONE rounding. The argument order is not
## obvious and the doc comment is not optional.
F64 has:
    function fma(self, factor: F64, addend: F64) -> F64

## Tier 2. Generic over `Real`, so `F32`, `F64` and `Uncertain of F64` all get it
## from one definition (§8). Specified to 1 ULP, not bit-identical (§3.6).
function exp of T(x: T) -> T where T: Real

## Tier 2, and this is rule P2's whole point: `ln(1.0 + x)` for x = 1e-16 returns
## 0.0, and this returns 1e-16.
function ln1p of T(x: T) -> T where T: Real

## Tier 3. Two results from one argument reduction. A fusion, not an operation,
## which is why it is imported and `sin` is not (§4.4).
function sin_cos of T(x: T) -> (T, T) where T: Real
```

**In use.**

```science
## The stable form of the logistic function. Both branches matter: the naive
## `1.0 / (1.0 + exp(-x))` underflows to zero below about x = -745, and the naive
## `exp(x) / (1.0 + exp(x))` overflows above about x = 709.
pure function logistic(x: F64) -> F64:
    if x >= 0.0:
        return 1.0 / (1.0 + (-x).exp())
    let z be x.exp()
    z / (1.0 + z)
```

No `use` line, because every name in it is a prelude method. That is what
`stdlib-core.md` §8.2 buys, restated for the domain that spends it most.

### 5.2 Special functions

Tier 3 in full, without exception, and the reason is `stdlib-core.md` §1.2 Q1 and
Q5 as §4.4 records: accuracy is implementation-defined and *improves*, so a better
implementation must be able to ship faster than the compiler does.

One consequence of §8 belongs here rather than in §8: **a special function written
generically over `Real` is differentiable for free.** A `gamma` composed from
`Real`'s primitives propagates a derivative through `Uncertain of F64` by the
chain rule without anybody writing down that the derivative of gamma is gamma
times digamma. That is `scientific-libraries.md` §2's stated reason for writing
these rather than linking them — *"generic over `F32`/`F64` and differentiable in
F2 — a linked C `erf` is neither"* — and it is worth restating because it is the
strongest argument in the corpus for a library being deliberately slower than C.

| Family | Names |
|---|---|
| Gamma | `gamma` `ln_gamma` `sign_gamma` `reciprocal_gamma` `digamma` `trigamma` `polygamma` `beta` `ln_beta` `factorial` `ln_factorial` `double_factorial` `rising_factorial` `falling_factorial` `binomial_real` |
| Incomplete gamma and beta | `gamma_lower` `gamma_upper` `gamma_regularised_p` `gamma_regularised_q` `gamma_regularised_p_inverse` `beta_incomplete` `beta_regularised` `beta_regularised_inverse` |
| Error | `erf` `erfc` `erf_inverse` `erfc_inverse` `erfc_scaled` `dawson` `faddeeva` `voigt_profile` `owen_t` |
| Bessel, cylindrical | `bessel_j` `bessel_y` `bessel_i` `bessel_k` `bessel_j_derivative` `bessel_y_derivative` `bessel_i_scaled` `bessel_k_scaled` |
| Bessel, spherical and related | `spherical_bessel_j` `spherical_bessel_y` `spherical_bessel_i` `spherical_bessel_k` `hankel_first` `hankel_second` `struve_h` `struve_l` `kelvin_ber` `kelvin_bei` |
| Bessel, zeros | `bessel_j_zero` `bessel_y_zero` `bessel_j_derivative_zero` |
| Airy | `airy_ai` `airy_bi` `airy_ai_derivative` `airy_bi_derivative` `airy_ai_zero` `airy_bi_zero` |
| Orthogonal polynomials | `legendre_p` `legendre_q` `associated_legendre_p` `chebyshev_t` `chebyshev_u` `chebyshev_v` `chebyshev_w` `hermite_h` `hermite_he` `laguerre_l` `associated_laguerre_l` `jacobi_p` `gegenbauer_c` `zernike_r` |
| Spherical harmonics | `spherical_harmonic` `real_spherical_harmonic` `wigner_d_small` `wigner_d` |
| Elliptic integrals | `elliptic_k` `elliptic_e` `elliptic_f` `elliptic_pi` `carlson_rf` `carlson_rd` `carlson_rj` `carlson_rc` |
| Elliptic functions | `jacobi_sn` `jacobi_cn` `jacobi_dn` `jacobi_amplitude` `weierstrass_p` `weierstrass_p_derivative` `theta_one` `theta_two` `theta_three` `theta_four` |
| Zeta and polylogarithms | `zeta` `zeta_minus_one` `hurwitz_zeta` `lerch_phi` `dirichlet_eta` `dirichlet_beta` `polylog` `dilog` `trilog` `clausen` |
| Exponential-type integrals | `exponential_integral_e1` `exponential_integral_ei` `exponential_integral_en` `logarithmic_integral` `sine_integral` `cosine_integral` `hyperbolic_sine_integral` `hyperbolic_cosine_integral` `fresnel_s` `fresnel_c` |
| Lambert | `lambert_w0` `lambert_w_minus_one` |
| Hypergeometric | `hypergeometric_0f1` `hypergeometric_1f1` `hypergeometric_1f2` `hypergeometric_2f1` `hypergeometric_pfq` `confluent_u` `whittaker_m` `whittaker_w` `meijer_g` |
| Number-theoretic special | `bernoulli_polynomial` `euler_polynomial` `bell_polynomial` |

Three naming notes, all N3 in action.

**`legendre_p` and `legendre_q`, not `legendre`.** Legendre named three unrelated
things in this table alone: the polynomials `P` and `Q`, the elliptic integrals of
the first, second and third kind, and — in §5.6 — the Legendre symbol. The
elliptic integrals are spelled `elliptic_f`, `elliptic_e` and `elliptic_pi`,
named for the operation rather than the person, exactly so that the collision
cannot arise; the symbol is `legendre_symbol`, which N3 disambiguates by the noun.

**`hermite_h` and `hermite_he`** distinguish the physicists' and the
probabilists' Hermite polynomials, which differ by a scaling and are confused
constantly. `scientific-libraries.md` §5.2's `hermite` / `hermite_probabilists`
is longer and says the same thing; either is fine, and the requirement is that
they be spelled as a *pair*, so that a reader who sees one looks for the other.

**`carlson_rf` rather than `elliptic_rf`.** The symmetric Carlson forms are a
different parameterisation with different numerical behaviour, not a variant of
the Legendre forms, so they take their own eponym under N3. This is a rename of
`scientific-libraries.md` §5.2's `elliptic_rf`, on the ground that `elliptic_f`
and `elliptic_rf` differ by one character and mean substantially different things.

**Five signatures.**

```science
## Generic over `Real`, per §8 and `uncertainty.md` §6.2. This is the signature
## `scientific-libraries.md` §5.11 writes as `function erf of T(x: T) returns T
## where T: Float`; the bound changes and nothing else does.
function erf of T(x: T) -> T where T: Real

## `ln_gamma` returns the log-magnitude and the sign separately, because gamma is
## negative on alternate intervals below zero and a logarithm that has thrown the
## sign away is a bug generator.
function ln_gamma of T(x: T) -> (T, Sign) where T: Real

## Integer order, real argument. Non-integer order is `bessel_j_real`, kept
## separate because the algorithm and the cost are different, not because the
## mathematics is.
function bessel_j of T(order: Int, x: T) -> T where T: Real

## Fallible, because the continued fraction for the incomplete beta does not
## converge for every parameter triple, and `stdlib-shape-and-packages.md` §2.3
## says nothing panics where an error will do.
function beta_regularised of T(a: T, b: T, x: T) -> (T, SpecialFunctionError?)
    where T: Real

## An N3 name with two eponyms in it and no ambiguity, because the operation noun
## is present.
function associated_legendre_p of T(degree: Int, order: Int, x: T) -> T
    where T: Real
```

**In use.**

```science
use math (erf)

## The fraction of a normal population within `k` standard deviations, which is
## the single most-asked question in an experimental write-up.
pure function within_sigma(k: F64) -> F64:
    erf(k / 2.0.sqrt())

function main():
    for k in 1..4:
        print(f"{k} sigma: {within_sigma(k as F64):.6f}")
```

### 5.3 Calculus — differentiation, quadrature, series acceleration

Tier 3 in full. Quadrature is `stdlib-core.md` §1.2 Q4's canonical failure, and
Decision 1's N2 makes the failure visible in the signature rather than in a
paragraph: the method is an argument, so the operation is one function.

**Differentiation.**

| Group | Names |
|---|---|
| Finite differences | `derivative` `derivative_order` `gradient_numeric` `jacobian_numeric` `hessian_numeric` `directional_derivative` |
| Stencils and step control | `forward_difference` `backward_difference` `central_difference` `five_point_stencil` `optimal_step` |
| Extrapolation | `richardson_extrapolate` `neville_extrapolate` |
| Complex step | `complex_step_derivative` |
| Derivatives of samples | `differences` `gradient_of_samples` `savitzky_golay_derivative` |

`complex_step_derivative` is worth one sentence: for an analytic `f`, the
imaginary part of `f(x + i·h)` divided by `h` is the derivative to machine
precision with *no* subtractive cancellation, so `h` can be 10⁻²⁰⁰. It is the only
numerical derivative that does not trade truncation error against round-off error,
it requires `f` to be written generically over `Complex`, and a language whose
numerical functions are generic over `Real` (§8) is one `Complex` implementation
away from giving it for free. Named because it is the clearest payoff of the
genericity decision outside automatic differentiation.

**Quadrature.** One function, `integrate`, and a `choice` of methods.

| Group | `Quadrature` variants |
|---|---|
| Newton–Cotes | `Trapezoid` `Simpson` `SimpsonThreeEighths` `Boole` `Romberg` |
| Gaussian | `GaussLegendre(Int)` `GaussKronrod(Int)` `GaussHermite(Int)` `GaussLaguerre(Int)` `GaussChebyshev(Int)` `GaussJacobi(Int, F64, F64)` `GaussLobatto(Int)` `GaussRadau(Int)` |
| Adaptive | `Adaptive` `AdaptiveGaussKronrod` `GlobalAdaptive` |
| Endpoint-singular and infinite | `TanhSinh` `DoubleExponential` `ClenshawCurtis(Int)` |
| Oscillatory | `Filon` `LevinOscillatory` `Oscillatory(F64)` |
| Multidimensional | `Cubature` `SparseGrid(Int)` `MonteCarlo(Int)` `QuasiMonteCarlo(Int)` `Vegas(Int)` `Miser(Int)` |

Separate N3 functions that are **not** methods of `integrate`, because they
return different things and N2's interchangeability test therefore fails:

`gauss_legendre_nodes` `gauss_hermite_nodes` `gauss_laguerre_nodes`
`clenshaw_curtis_nodes` `kronrod_extend` `integrate_samples`
`cumulative_integral` `integrate_2d` `integrate_nd`

**Series and acceleration.**

`series_sum` `series_sum_until` `aitken_delta_squared` `shanks_transform`
`levin_u` `levin_t` `levin_v` `wynn_epsilon` `wynn_rho` `euler_transform`
`richardson_series` `chebyshev_economise` `pade_approximant`
`continued_fraction` `continued_fraction_lentz` `borel_sum` `abel_sum`

`wynn_epsilon` and `levin_u` are the two that matter in practice and the two that
nobody knows exist. That is an argument for documentation, not for tier
promotion, and it is worth saying because the temptation runs the other way.

**Five signatures.**

```science
use math (integrate, integrate_estimated, derivative, accelerate)

## The N2 shape: one operation, the method as an argument. This replaces the
## thirteen free functions of `scientific-libraries.md` §5.3 (§2.5).
## The closure type `function(T) -> T` is the standing cross-note ask that
## `ffi-c-boundary.md` §10.1 raised and `scientific-libraries.md` §14.2 seconded;
## this note is the fourth asker (§11).
function integrate of T(
    f: function(T) -> T,
    lower: T,
    upper: T,
    method: Quadrature,
    tolerance: Tolerance,
) -> (T, QuadratureError?) where T: Real

## The error estimate is a second return, not a field on a result type and not a
## log line. A quadrature result without its estimate is a number nobody can
## publish, and a second return is the spelling that makes ignoring it visible.
function integrate_estimated of T(
    f: function(T) -> T,
    lower: T,
    upper: T,
    method: Quadrature,
    tolerance: Tolerance,
) -> (T, T, QuadratureError?) where T: Real

## Infinite limits are a domain argument rather than a second function, because
## the caller's code is otherwise identical.
function integrate_infinite of T(
    f: function(T) -> T,
    domain: InfiniteDomain of T,
    method: Quadrature,
    tolerance: Tolerance,
) -> (T, QuadratureError?) where T: Real

## Numerical differentiation returns a number and cannot fail in the error sense,
## so it returns the value and its estimated error and no error type. The step is
## nullable: given one, it is used; without one, `optimal_step` chooses.
function derivative of T(
    f: function(T) -> T,
    x: T,
    order: Int,
    step: T?,
) -> (T, T) where T: Real

## Acceleration consumes partial sums, not terms, because that is what the caller
## has and what every acceleration scheme actually reads.
function accelerate of T(
    partial_sums: borrowed Array of T,
    method: Acceleration,
) -> (T, T) where T: Real
```

**In use.**

```science
use math (integrate, Quadrature, Tolerance)
use physics.constants (BOLTZMANN, PLANCK, SPEED_OF_LIGHT)
use physics.units (watts_per_square_metre)

## Total emissive power of a black body, by integrating the Planck law over
## wavelength, so that it can be checked against the Stefan-Boltzmann constant.
function planck_total(temperature: Temperature of F64) -> (Irradiance of F64, Error?):
    let t be temperature.in(KELVIN)
    let h be PLANCK.magnitude()
    let c be SPEED_OF_LIGHT.magnitude()
    let k be BOLTZMANN.magnitude()

    let spectral be function(wavelength: F64) -> F64:
        let exponent be h * c / (wavelength * k * t)
        2.0 * h * c * c / (wavelength.powi(5) * exponent.expm1())

    let total, err be integrate(
        spectral,
        1.0e-9,
        1.0e-3,
        method: Quadrature.TanhSinh,
        tolerance: Tolerance.relative(1.0e-12),
    )
    if err?:
        return (watts_per_square_metre(0.0), err)

    (watts_per_square_metre(total * F64.PI), null)
```

Two things in that example are load-bearing. `exponent.expm1()` where the
textbook writes `exp(x) - 1`: at long wavelengths the exponent is small and the
naive form loses every significant digit, which is rule P2 earning its place in a
program somebody would actually write. And `.magnitude()`, which is the explicit
step out of the unit system — §7.4 is about why that step must be explicit and
where it is dangerous.

### 5.4 Differential equations — ordinary and partial

**Ordinary.** One entry point, `solve_ode`, per §2.5. `scientific-libraries.md`
§5.4 already lists `solve_ode(problem, method, tolerance)` *alongside* twenty free
functions; this note keeps the former and deletes the latter, on
`syntax-revision-2.md` §1's ground that one operation gets one spelling.

| Group | `OdeMethod` variants |
|---|---|
| Explicit Runge–Kutta | `Euler` `Midpoint` `Heun` `Ralston` `Rk4` `Rk38` `BogackiShampine23` `DormandPrince45` `CashKarp45` `Fehlberg45` `Tsitouras45` `DormandPrince853` `Verner65` `Verner98` |
| Explicit multistep | `AdamsBashforth(Int)` `AdamsMoulton(Int)` `PredictorCorrector(Int)` `Nordsieck(Int)` |
| Implicit and stiff | `BackwardEuler` `ImplicitMidpoint` `Trapezoidal` `Bdf(Int)` `RadauIia(Int)` `GaussLegendreIrk(Int)` `LobattoIiic(Int)` `Rosenbrock23` `Rosenbrock34` `Rodas4` `TrBdf2` `Sdirk(Int)` |
| Symplectic and geometric | `SymplecticEuler` `Leapfrog` `VelocityVerlet` `PositionVerlet` `ForestRuth` `Yoshida4` `Yoshida6` `McLachlan(Int)` `GaussLegendreSymplectic(Int)` |
| Exponential | `ExponentialEuler` `ExponentialRk(Int)` `Etdrk4` `Lawson(Int)` |
| Stochastic | `EulerMaruyama` `Milstein` `StochasticRk(Int)` `StratonovichHeun` |

Support, all tier 3: `OdeProblem` `OdeSolution` `DenseOutput` `StepControl`
`Tolerance` `EventHandler` `MassMatrix` `JacobianSource` `solve_ode`
`solve_ode_dense` `solve_ode_events` `solve_dae` `solve_bvp` `solve_sde`
`shooting_method` `multiple_shooting` `collocation` `finite_difference_bvp`
`continuation`

The symplectic row is not decoration. §6.2's Hamiltonian formulation is useless
without it: a non-symplectic integrator applied to a Hamiltonian system drifts in
energy monotonically, and the entire reason to write a problem as a Hamiltonian
is to integrate it with a method that does not. The two sections are one feature.

**Partial.** Absent from `scientific-libraries.md` §5 entirely; this note adds it
rather than contradicting anything. A PDE catalogue is organised by
*discretisation*, not by equation, which is the mirror image of the ODE
organisation and worth stating: an ODE method is chosen by the stiffness of the
problem, a PDE method by the geometry of the domain.

| Group | Names |
|---|---|
| Finite difference | `laplacian_stencil` `gradient_stencil` `divergence_stencil` `fd_matrix_1d` `fd_matrix_2d` `fd_matrix_3d` `ftcs_step` `btcs_step` `crank_nicolson_step` `lax_wendroff_step` `lax_friedrichs_step` `upwind_step` `adi_step` |
| Method of lines | `semidiscretise` `method_of_lines` |
| Finite volume | `godunov_flux` `roe_flux` `hll_flux` `hllc_flux` `rusanov_flux` `muscl_reconstruct` `weno_reconstruct` `minmod_limiter` `superbee_limiter` `van_leer_limiter` |
| Finite element | `Mesh` `Element` `assemble_stiffness` `assemble_mass` `assemble_load` `apply_dirichlet` `apply_neumann` `apply_robin` `solve_fem` `p1_basis` `p2_basis` `quadrature_triangle` `quadrature_tetrahedron` |
| Spectral | `chebyshev_differentiation_matrix` `fourier_differentiation_matrix` `collocation_spectral` `galerkin_spectral` `spectral_element` |
| Named problems | `poisson_solve` `heat_solve` `wave_solve` `advection_solve` `burgers_solve` `helmholtz_solve` |
| Boundary conditions | `BoundaryCondition` with variants `Dirichlet`, `Neumann`, `Robin`, `Periodic`, `Absorbing`, `Reflecting` |
| Multigrid and smoothing | `multigrid_v_cycle` `multigrid_w_cycle` `full_multigrid` `restrict` `prolong` `smooth_jacobi` `smooth_gauss_seidel` `smooth_sor` |

Every PDE entry is **F1**, without exception, by `scientific-libraries.md` §4's
rule: *"if the function's correctness depends on two arrays having compatible
shapes, it waits for F1."* A stencil applied to a grid is that rule's purest case,
and a PDE library written in F0 would be a PDE library rewritten in F1.

**Five signatures.**

```science
use math (solve_ode, solve_ode_dense, solve_ode_events, solve_bvp, heat_solve)

## The problem carries the right-hand side, the initial state, the span and
## optionally a Jacobian; the method and the tolerance are arguments because they
## are the two things a user changes without changing the problem.
function solve_ode of (T, const N: Int)(
    problem: borrowed OdeProblem of (T, N),
    method: OdeMethod,
    tolerance: Tolerance,
) -> (OdeSolution of (T, N), OdeError?) where T: Real

## Dense output is a separate entry point rather than a flag, because it changes
## the return type: the solution becomes callable at any time in the span.
function solve_ode_dense of (T, const N: Int)(
    problem: borrowed OdeProblem of (T, N),
    method: OdeMethod,
    tolerance: Tolerance,
) -> (DenseOutput of (T, N), OdeError?) where T: Real

## Event location. The event functions are root-found between steps, which is why
## this cannot be a post-processing pass over a finished solution.
function solve_ode_events of (T, const N: Int)(
    problem: borrowed OdeProblem of (T, N),
    events: borrowed Array of EventHandler of (T, N),
    method: OdeMethod,
    tolerance: Tolerance,
) -> (OdeSolution of (T, N), Array of Event of T, OdeError?) where T: Real

## A two-point boundary value problem needs an initial guess over a mesh, not an
## initial condition, and the type says so rather than the documentation.
function solve_bvp of (T, const N: Int, const M: Int)(
    residual: function(T, borrowed Vector of (T, N)) -> Vector of (T, N),
    boundary: function(borrowed Vector of (T, N), borrowed Vector of (T, N))
        -> Vector of (T, N),
    mesh: borrowed Vector of (T, M),
    guess: borrowed Matrix of (T, N, M),
    tolerance: Tolerance,
) -> (BvpSolution of (T, N, M), BvpError?) where T: Real

## F1. The output carries one row per recorded step and one column per grid
## point, both from the arguments, which is the const layer doing nothing clever
## and being necessary anyway.
function heat_solve of (T, const NX: Int, const NT: Int)(
    initial: borrowed Vector of (T, NX),
    diffusivity: T,
    boundary: BoundaryCondition of T,
    steps: borrowed StepPlan of NT,
    method: PdeMethod,
) -> (Matrix of (T, NT, NX), PdeError?) where T: Real
```

**In use.**

```science
use math (solve_ode, OdeProblem, OdeMethod, Tolerance)

## The Lorenz system, integrated with an adaptive Runge-Kutta pair.
function lorenz() -> (OdeSolution of (F64, 3), Error?):
    let sigma be 10.0
    let rho be 28.0
    let beta be 8.0 / 3.0

    let rhs be function(t: F64, s: borrowed Vector of (F64, 3)) -> Vector of (F64, 3):
        [
            sigma * (s[1] - s[0]),
            s[0] * (rho - s[2]) - s[1],
            s[0] * s[1] - beta * s[2],
        ]

    let problem be OdeProblem.new(rhs, start: [1.0, 1.0, 1.0], span: (0.0, 40.0))

    solve_ode(problem, method: OdeMethod.DormandPrince45,
              tolerance: Tolerance.relative(1.0e-10))
```

Swapping `DormandPrince45` for `Rodas4` when the system turns stiff is a
one-token edit that changes nothing else in the program. That is N2's payoff,
stated as a program rather than as a claim.

### 5.5 Interpolation, polynomials and bases

**Interpolation** is N2's second-clearest case after quadrature, with one wrinkle
that shapes the design: some methods are cheap to evaluate once and expensive to
build, so the operation has *two* shapes and both must exist.

| Group | `InterpolationMethod` variants |
|---|---|
| Piecewise | `Nearest` `Linear` `Previous` `Next` |
| Splines | `CubicNatural` `CubicClamped` `CubicNotAKnot` `CubicPeriodic` `MonotoneCubic` `Akima` `AkimaModified` `Pchip` `BSpline(Int)` `Quintic` |
| Polynomial | `Lagrange` `NewtonDividedDifference` `Barycentric` `Neville` `Hermite` |
| Rational | `RationalThieleC` `RationalFloaterHormann(Int)` |
| Scattered and multivariate | `ThinPlateSpline` `RadialBasis(Kernel)` `InverseDistance(F64)` `NaturalNeighbour` `Kriging(Variogram)` |
| Gridded, multivariate | `Bilinear` `Bicubic` `Trilinear` `Tricubic` `RegularGrid` |

Two entry points, and the split is the wrinkle:

```science
## Evaluate at a point, once. Builds and discards.
function interpolate_at(...)

## Build a reusable interpolant. The return is a value with state, which N4 says
## is when a type is justified.
function Interpolator.fit(...)
```

Supporting N3 functions that are objects rather than methods: `bspline_basis`
`bspline_knots` `bspline_design_matrix` `variogram_fit` `barycentric_weights`
`divided_difference_table`.

**Polynomials.** `Polynomial of T`, with `evaluate` rather than `at` — §2.4
explains why the rename survives even though `at` is free again.

| Group | Names |
|---|---|
| Evaluation | `evaluate` `evaluate_horner` `evaluate_clenshaw` `evaluate_derivative` `evaluate_all_derivatives` |
| Calculus | `derivative` `antiderivative` `integrate_over` |
| Arithmetic | `add` `subtract` `multiply` `multiply_fft` `divide` `remainder_of` `gcd` `extended_gcd` `power` `compose` `invert_series` |
| Transformations | `shift` `scale` `reverse` `deflate` `monic` |
| Structure | `degree` `coefficients` `leading` `is_zero` `content` `primitive_part` |
| Roots | `from_roots` `roots` `real_roots` `companion_matrix` `discriminant` `resultant` `sturm_sequence` `count_real_roots` `root_bounds_cauchy` `root_bounds_lagrange` |
| Bases | `chebyshev_fit` `legendre_fit` `hermite_fit` `laguerre_fit` `orthogonal_fit` `to_power_basis` `from_power_basis` `basis_convert` |
| Series types | `ChebyshevSeries` `LegendreSeries` `PowerSeries` `LaurentSeries` `PuiseuxSeries` |

`root_bounds_lagrange` and `root_bounds_cauchy` are N3 names for the two classical
bounds on the modulus of a polynomial's roots, and they are the fifth and sixth
distinct things called "Lagrange" in this note. The rule absorbed them without a
special case, which is the test any naming rule has to pass.

**Five signatures.**

```science
use math (interpolate_at, Interpolator, InterpolationMethod, Polynomial)

## The one-shot form. Fallible, because the sample points may be unsorted,
## duplicated, or too few for the requested method.
function interpolate_at of T(
    xs: borrowed Array of T,
    ys: borrowed Array of T,
    x: T,
    method: InterpolationMethod,
) -> (T, InterpolationError?) where T: Real

## The reusable form. N4's justification for a type: it holds the coefficients.
Interpolator of T has:
    function fit(
        xs: borrowed Array of T,
        ys: borrowed Array of T,
        method: InterpolationMethod,
    ) -> (Interpolator of T, InterpolationError?) where T: Real

    function evaluate(borrowed self, x: T) -> T
    function evaluate_derivative(borrowed self, x: T, order: Int) -> T

## `evaluate`, not `at`. See §2.4: the rename outlives the reservation that
## forced it.
Polynomial of T has:
    function evaluate(borrowed self, x: T) -> T where T: Real

## Roots are complex even for a real polynomial, which the return type says and
## the name does not have to.
Polynomial of T has:
    function roots(borrowed self, tolerance: T)
        -> (Array of Complex of T, RootError?) where T: Real

## A least-squares fit in the Chebyshev basis, which is what anybody fitting a
## smooth function on an interval should be doing and almost nobody is.
function chebyshev_fit of T(
    f: function(T) -> T,
    lower: T,
    upper: T,
    degree: Int,
) -> ChebyshevSeries of T where T: Real
```

**In use.**

```science
use math (Interpolator, InterpolationMethod)

## Resample an irregularly-sampled instrument trace onto a regular grid, with a
## shape-preserving method so that the resampling cannot invent an overshoot the
## instrument never saw.
function resample(
    times: borrowed Array of F64,
    values: borrowed Array of F64,
    grid: borrowed Array of F64,
) -> (Array of F64, Error?):
    let interp, err be Interpolator.fit(
        times, values, method: InterpolationMethod.MonotoneCubic)
    if err?:
        return ([], err)

    let mutable out be Array.with_capacity(grid.length())
    for t in grid:
        out.push(interp.evaluate(t))
    (out, null)
```

`MonotoneCubic` rather than `CubicNatural` is the entire point of the example. A
natural cubic spline through monotone data overshoots, and an overshoot in a
resampled instrument trace is a measurement that was never made. The method being
a named variant at the call site is what makes that choice reviewable.

### 5.6 Root finding, number theory, combinatorics

**Root finding** is N2's purest case in the whole note, and the one where the
existing catalogue's bare eponyms are worst: `scientific-libraries.md` §5.7's
`newton` is a single word that also names a unit, three laws and an optimisation
algorithm.

| Group | `RootMethod` variants |
|---|---|
| Bracketing | `Bisection` `FalsePosition` `Illinois` `AndersonBjorck` `Ridders` `Brent` `Toms748` `Chandrupatla` |
| Open | `Newton` `Secant` `Halley` `Steffensen` `Householder(Int)` `Muller` `InverseQuadratic` |
| All roots of a polynomial | `AberthEhrlich` `DurandKerner` `JenkinsTraub` `CompanionEigen` `VincentAkritasStrzebonski` |

N3 functions that are not methods of `root`, because they answer different
questions: `bracket_root` `bracket_expand` `find_all_roots` `sign_changes`
`root_count_sturm` `root_isolate`.

**Number theory.** Tier 3, and `stdlib-core.md` §8.1 refused it from Level 1 on
Q1 and Q2 — Miller–Rabin's answer depends on the witness set, and `factorise` on a
chosen 80-bit semiprime is a one-line denial of service. That refusal is adopted
without amendment.

| Group | Names |
|---|---|
| Divisibility | `gcd` `lcm` `extended_gcd` `binary_gcd` `gcd_all` `lcm_all` |
| Modular | `mod_inverse` `mod_pow` `mod_mul` `mod_sqrt` `chinese_remainder` `discrete_log` `order_of` `primitive_root` |
| Primality | `is_prime` `is_probable_prime` `miller_rabin` `baillie_psw` `lucas_test` `next_prime` `previous_prime` `nth_prime` `prime_count` `primes_below` `sieve_eratosthenes` `sieve_atkin` `segmented_sieve` |
| Factorisation | `factorise` `factorise_trial` `factorise_pollard_rho` `factorise_pollard_p_minus_one` `factorise_williams_p_plus_one` `factorise_ecm` `factorise_quadratic_sieve` `smallest_factor` |
| Divisor functions | `divisors` `divisor_count` `divisor_sum` `divisor_sigma` `aliquot_sum` |
| Multiplicative | `euler_totient` `mobius` `carmichael` `liouville` `mangoldt` `jordan_totient` |
| Symbols | `legendre_symbol` `jacobi_symbol` `kronecker_symbol` |
| Continued fractions and approximation | `continued_fraction_of` `convergents` `rational_approximate` `stern_brocot_search` `farey_neighbours` |
| Diophantine | `solve_linear_diophantine` `pell_fundamental` `sum_of_two_squares` `four_square_decomposition` |
| Integer sequences | `fibonacci` `lucas` `pell` `tribonacci` `collatz_steps` |

Note `binary_gcd` beside `gcd`: the binary algorithm is `trailing_zeros` and
subtraction in a loop, which is §3.3's tier 1 integer list being the reason a
tier 3 function is fast. That relationship — tier 1 as the *substrate* of tier 3
rather than as a competitor to it — is the normal case and is easy to lose sight
of when the tiers are presented as a ranking.

**Combinatorics.** Tier 3. The split that matters here is between *counting* and
*generating*, and the names must say which, because the cost differs by a
factorial.

| Group | Names |
|---|---|
| Counting | `binomial` `binomial_mod` `multinomial` `permutation_count` `combination_count` `derangement_count` `catalan` `bell` `stirling_first` `stirling_second` `eulerian` `narayana` `motzkin` `partition_count` `partition_count_into` `composition_count` |
| Sequences of numbers | `bernoulli_number` `euler_number` `tangent_number` `harmonic_number` `harmonic_number_generalised` |
| Generating | `permutations` `combinations` `combinations_with_repetition` `multiset_permutations` `power_set` `cartesian_product` `partitions` `compositions` `set_partitions` `derangements` `gray_code` `necklaces` |
| Ranking and unranking | `permutation_rank` `permutation_unrank` `combination_rank` `combination_unrank` |
| Polya and symmetry | `cycle_index` `burnside_count` `orbit_count` |

`permutation_count` returns an integer; `permutations` returns a lazy sequence, in
`collections-and-chains.md`'s `Iterate` sense, and must, because the eager form
of `permutations` over twelve elements is half a billion arrays. That is a naming
consequence of a memory fact and it deserves the extra word.

**Five signatures.**

```science
use math (root, find_all_roots, is_probable_prime, factorise, combinations)

## The N2 shape. `bracket` is a type rather than two floats, because a bracketing
## method needs a sign change and an open method does not, and the type is where
## that precondition can be checked once.
function root of T(
    f: function(T) -> T,
    bracket: Bracket of T,
    method: RootMethod,
    tolerance: Tolerance,
) -> (T, RootError?) where T: Real

## Newton's method needs a derivative, which an open-method `root` call supplies
## through this second entry point rather than through a nullable argument: the
## two have genuinely different preconditions and N2's interchangeability test
## therefore fails between them.
function root_with_derivative of T(
    f: function(T) -> T,
    derivative: function(T) -> T,
    start: T,
    method: RootMethod,
    tolerance: Tolerance,
) -> (T, RootError?) where T: Real

## Probabilistic, and the signature says so by taking the witness count. This is
## `stdlib-core.md` §1.2's Q1 failure rendered as an argument — the algorithm is
## the observable, so the algorithm is in the call.
function is_probable_prime(n: Int, witnesses: Int, key: borrowed Key) -> Bool

## Fallible on time, not on input: every integer factorises, and the error is
## that the budget ran out. Returning the partial factorisation with the error is
## what makes the budget usable.
function factorise(n: Int, budget: Budget) -> (Array of (Int, Int), FactorError?)

## Lazy. The eager form of this over twelve elements is half a billion arrays.
function combinations of T(
    items: borrowed Array of T,
    take: Int,
) -> Iterate of Array of T
```

**In use.**

```science
use math (root, RootMethod, Bracket, Tolerance)
use physics.constants (GRAVITATIONAL)

## The radius at which a satellite orbits with a given period: solve Kepler's
## third law for r. Bracketed, because the function is monotone and a bracket
## turns a convergence question into a guarantee.
function orbital_radius(period: F64, central_mass: F64) -> (F64, Error?):
    let mu be GRAVITATIONAL.magnitude() * central_mass

    let residual be function(r: F64) -> F64:
        2.0 * F64.PI * (r * r * r / mu).sqrt() - period

    let bracket, err be Bracket.new(1.0e5, 1.0e12, residual)
    if err?:
        return (0.0, err)

    root(residual, bracket, method: RootMethod.Brent,
         tolerance: Tolerance.relative(1.0e-12))
```

### 5.7 Complex analysis and the transforms

**Complex.** Tier 3, and `stdlib-core.md` §8.1's refusal of it from Level 1 is on
the second gate rather than the first: it passes §1.2 and fails §1.3, *"twenty-five
methods each on a type most programs never mention."* Adopted.

| Group | Names |
|---|---|
| Construction and parts | `Complex.new` `Complex.from_polar` `Complex.from_real` `real` `imaginary` `to_polar` `conjugate` |
| Magnitude and phase | `modulus` `modulus_squared` `argument` `argument_principal` `normalise` |
| Elementary | `exp` `ln` `ln_branch` `sqrt` `cbrt` `pow` `pow_complex` `nth_roots` |
| Trigonometric and hyperbolic | `sin` `cos` `tan` `asin` `acos` `atan` `sinh` `cosh` `tanh` `asinh` `acosh` `atanh` |
| Predicates | `is_nan` `is_finite` `is_real` `is_imaginary` `is_close` |
| Special, complex argument | `gamma_complex` `ln_gamma_complex` `zeta_complex` `erf_complex` `polylog_complex` `lambert_w_complex` |
| Analysis | `residue_at` `contour_integrate` `argument_principle_count` `winding_number` `analytic_continue_pade` `branch_cut_of` |
| Conformal maps | `mobius_transform` `joukowsky_transform` `schwarz_christoffel` |

`ln_branch` and `branch_cut_of` are there because the single most common complex
error in scientific code is an unnoticed branch cut, and a library that has a
`ln` and no way to ask which branch it took has made that error invisible.

**Transforms.** All tier 3, all **F1**, and the FFT family is *linked C*
(`scientific-libraries.md` §2: PocketFFT vendored, FFTW where available). This is
where the owner's `Fourier()` lands, and the distance between the request and the
answer is the point.

| Family | Names | Written or linked |
|---|---|---|
| Discrete Fourier | `fft` `ifft` `rfft` `irfft` `fft2` `ifft2` `fftn` `ifftn` `fftshift` `ifftshift` `fftfreq` `rfftfreq` `next_fast_length` | **Linked** |
| Related real transforms | `dct` `idct` `dst` `idst` `hartley_transform` `inverse_hartley_transform` | **Linked** (DCT/DST) or written |
| Generalised Fourier | `chirp_z_transform` `zoom_fft` `nonuniform_fft` `fractional_fourier_transform` | Written over the linked FFT |
| Fourier series | `fourier_series` `fourier_series_evaluate` `fourier_coefficients_of_samples` | Written |
| Short-time and time-frequency | `stft` `istft` `spectrogram` `wigner_ville` `reassigned_spectrogram` | Written |
| Hilbert | `hilbert_transform` `analytic_signal` `envelope` `instantaneous_phase` `instantaneous_frequency` | Written |
| Laplace | `laplace_transform_symbolic` `laplace_invert` with `LaplaceInversion` variants `Talbot`, `Stehfest`, `DeHoog`, `Weeks`, `Durbin` | Written |
| Z | `z_transform` `inverse_z_transform` `z_transform_of_filter` `bilinear_z` `matched_z` | Written |
| Mellin | `mellin_transform` `inverse_mellin_transform` `mellin_convolution` | Written |
| Wavelet | `wavelet_continuous` `wavelet_discrete` `wavelet_inverse` `wavelet_packet` `wavelet_denoise` `wavelet_scales` with `Wavelet` variants `Haar`, `Daubechies(Int)`, `Symlet(Int)`, `Coiflet(Int)`, `BiorthogonalSpline(Int, Int)`, `Morlet(F64)`, `MexicanHat`, `Meyer`, `Gaussian(Int)`, `Shannon` | Written |
| Radon and tomography | `radon_transform` `inverse_radon_filtered_back_projection` `sinogram` | Written |
| Integral transforms, general | `abel_transform` `inverse_abel_transform` `hankel_transform` `stieltjes_transform` | Written |

Two naming decisions visible in that table. **`hilbert_transform`, not
`hilbert`** — N1, and it matters because Hilbert also names a space, a matrix, a
curve and a transform, and `signal.hilbert` in SciPy is a name that has confused
people for twenty years. **The Laplace inversion methods are N2 variants of one
`laplace_invert`**, because they are genuinely interchangeable and because a user
choosing between Talbot and Stehfest is choosing a numerical method and nothing
else.

**Five signatures.**

```science
use signal (rfft, irfft, hilbert_transform, laplace_invert, wavelet_continuous)

## The clearest const-expression case in the corpus, and the one
## `scientific-libraries.md` §9.4 and `const-expression-arithmetic.md` §4 both
## build on: the output length is N/2+1, computed in type position.
function rfft of (const N: Int)(
    signal: borrowed Vector of (F64, N),
    norm: Normalisation,
) -> Vector of (Complex of F64, N / 2 + 1)

## The inverse takes the half-spectrum and the original length, because N/2+1
## does not determine N — `const-expression-arithmetic.md` §4.2's whole-form fold
## is one-way, and this is where that incompleteness shows at a call site.
function irfft of (const N: Int)(
    spectrum: borrowed Vector of (Complex of F64, N / 2 + 1),
    norm: Normalisation,
) -> Vector of (F64, N)

## N1 applied: `hilbert_transform`, not `hilbert`. Same length in, same out.
function hilbert_transform of (const N: Int)(
    signal: borrowed Vector of (F64, N),
) -> Vector of (F64, N)

## N2 applied: the inversion method is an argument. Numerical Laplace inversion
## is ill-posed, so the error is not optional and the tolerance is not a hint.
function laplace_invert(
    transform: function(Complex of F64) -> Complex of F64,
    time: F64,
    method: LaplaceInversion,
    tolerance: Tolerance,
) -> (F64, InversionError?)

## The wavelet family is a value, not part of the name, because the families are
## interchangeable in the N2 sense: same inputs, same output, different cost and
## different time-frequency trade-off.
function wavelet_continuous of (const N: Int, const S: Int)(
    signal: borrowed Vector of (F64, N),
    wavelet: Wavelet,
    scales: borrowed Vector of (F64, S),
) -> Matrix of (Complex of F64, S, N)
```

**In use.**

```science
use signal (rfft, rfftfreq, Normalisation)

## The dominant frequency of a sampled trace. The half-spectrum is the right
## transform for real input and its length is checked at compile time.
function dominant_frequency of (const N: Int)(
    trace: borrowed Vector of (F64, N),
    sample_rate: Frequency of F64,
) -> Frequency of F64:
    let spectrum be rfft(trace, norm: Normalisation.Unitary)
    let frequencies be rfftfreq(N, sample_rate)

    let mutable best be 1
    for i in 2..spectrum.length():
        if spectrum[i].modulus_squared() > spectrum[best].modulus_squared():
            best be i

    frequencies[best]
```

The loop starts at 1, not 0, because bin zero is the DC offset and is almost
never the answer anybody wants. That is the kind of thing a library should do for
the user, and the reason it is written out here is that `find_peaks` — which is
where it belongs — is `signal`'s and not this note's.

---

## 6. Physics, catalogued and tiered

**Every name in §6 is tier 3.** There is no tier 1 physics — no machine has a
`lorentz_factor` instruction — and §4.5 gives three independent reasons why there
is no tier 2 physics either. The catalogue is therefore tiered in one line, and
the interesting content is the *typing*: what each signature's `Quantity`
arguments buy, and where they cannot be used.

This section extends `scientific-libraries.md` §12.1 rather than contradicting
it. That section names `constants`, `mechanics`, `thermo`, `em`, `quantum`,
`relativity` and `units`. Three modules are added here — `physics.optics`,
`physics.fluids`, `physics.nuclear` — because optics folded into `em` loses
geometric and physical optics entirely, and fluids and nuclear physics were
absent. Added, and named as an addition.

### 6.1 Constants, and the exactness split

Decision 9 (§4.5) settles the tier. The catalogue, with the split that decision
requires:

**Exact by definition since the 2019 SI redefinition** — zero uncertainty, and
the value cannot change without a redefinition of the base units:

`CAESIUM_FREQUENCY` `SPEED_OF_LIGHT` `PLANCK` `ELEMENTARY_CHARGE` `BOLTZMANN`
`AVOGADRO` `LUMINOUS_EFFICACY`

**Exact by derivation from those seven** — a product or quotient of exact values,
so also exact: `REDUCED_PLANCK` `GAS_CONSTANT` `FARADAY` `STEFAN_BOLTZMANN`
`WIEN_DISPLACEMENT` `MOLAR_PLANCK` `CONDUCTANCE_QUANTUM` `MAGNETIC_FLUX_QUANTUM`
`JOSEPHSON` `VON_KLITZING` `FIRST_RADIATION` `SECOND_RADIATION`

**Measured, carrying a CODATA uncertainty and a vintage**:

| Group | Names |
|---|---|
| Gravitation | `GRAVITATIONAL` (relative uncertainty ≈ 2.2 × 10⁻⁵ — the worst-known constant in physics) |
| Masses | `ELECTRON_MASS` `PROTON_MASS` `NEUTRON_MASS` `MUON_MASS` `TAU_MASS` `DEUTERON_MASS` `ALPHA_MASS` `ATOMIC_MASS_UNIT` |
| Electromagnetic | `FINE_STRUCTURE` `VACUUM_PERMITTIVITY` `VACUUM_PERMEABILITY` `VACUUM_IMPEDANCE` `BOHR_MAGNETON` `NUCLEAR_MAGNETON` `ELECTRON_G_FACTOR` `CLASSICAL_ELECTRON_RADIUS` `THOMSON_CROSS_SECTION` |
| Atomic | `RYDBERG` `BOHR_RADIUS` `HARTREE_ENERGY` `COMPTON_WAVELENGTH` |
| Conventional, exact by convention rather than by nature | `STANDARD_GRAVITY` `STANDARD_ATMOSPHERE` `STANDARD_TEMPERATURE` |

Three notes on the split.

**`VACUUM_PERMEABILITY` moved sides in 2019 and this is why the vintage matters.**
Before the redefinition it was exactly 4π × 10⁻⁷ H/m by definition; after it, it
is measured, with a relative uncertainty of about 1.6 × 10⁻¹⁰ inherited from the
fine-structure constant. A constants table with no vintage cannot express that a
value changed category, and a reader of a ten-year-old program cannot tell which
definition it was compiled against. `reproducibility.md` §5.1's provenance record
already carries package versions; Decision 9 asks only that `physics.constants`
put its CODATA year in its version so that the record captures it for free.

**The uncertainties are not decoration.** `uncertainty.md` §8.3 asks for wave 2 to
ship these as `9.80665(15)<m/s^2>`, value and uncertainty both exact from the
published table. A program that propagates `GRAVITATIONAL`'s 2.2 × 10⁻⁵ through an
orbital calculation and reports six significant figures has reported four digits
of noise, and the type system is the only thing in the toolchain positioned to
notice.

**`STANDARD_GRAVITY` is exact and is not a measurement of anything.** 9.80665
m/s² is a defined conventional value, not the gravitational acceleration at any
particular place on Earth. Putting it in the same list as `GRAVITATIONAL` without
the category label is how a user comes to believe their local `g` is known to
nine digits.

**Five signatures.**

```science
use physics.constants (SPEED_OF_LIGHT, GRAVITATIONAL, PLANCK)
use physics.units (Quantity, metres, seconds)

## An exact constant. The type carries the dimension; the value carries no
## uncertainty, because there is none.
const SPEED_OF_LIGHT: Velocity of F64

## A measured constant. The representation parameter is `Uncertain of F64`, which
## is `uncertainty.md` Decision 5's ordering: the dimension outside, the
## uncertainty inside.
const GRAVITATIONAL: Quantity of (Uncertain of F64, 3, -1, -2, 0, 0, 0, 0)

## The vintage, as a value rather than as a comment, so that a provenance record
## can read it.
function codata_vintage() -> Int

## The only sanctioned way out of the unit system, and it is a named method
## rather than a field access, because §7.4 argues that leaving the system must
## be as visible as entering it.
Quantity of (T, L, M, T2, I, K, N, J) has:
    function magnitude(borrowed self) -> T

## Conversion into a named unit, which is checked for dimension and exact in its
## scale. `unit-literals.md` §7.4's `d.in(FOOT)`, adopted.
Quantity of (T, L, M, T2, I, K, N, J) has:
    function in(borrowed self, unit: UnitOf of (L, M, T2, I, K, N, J)) -> T
```

**In use.**

```science
use physics.constants (GRAVITATIONAL, SPEED_OF_LIGHT)

## The Schwarzschild radius of the Sun, with the uncertainty of G carried
## through. The result prints as 2953.25(7) m rather than as nine digits of
## false precision.
function solar_schwarzschild() -> Length of (Uncertain of F64):
    let solar_mass be 1.98847e30<kg>
    2.0 * GRAVITATIONAL * solar_mass / (SPEED_OF_LIGHT * SPEED_OF_LIGHT)
```

Four operators, no explicit types, and the return type is *computed* — `m³·kg⁻¹·s⁻²`
times `kg` divided by `m²·s⁻²` is `m`. That one line is what
`scientific-libraries.md` §12.4 is asking F0 for, and §7 is about what it buys.

### 6.2 Classical mechanics, Lagrangian and Hamiltonian

| Group | Names |
|---|---|
| Kinematics | `position_at` `velocity_at` `acceleration_at` `displacement` `average_velocity` `projectile_position` `projectile_range` `projectile_apex` `projectile_flight_time` `relative_velocity` |
| Dynamics | `momentum` `impulse` `newton_second_law_acceleration` `friction_force_static` `friction_force_kinetic` `drag_force_quadratic` `drag_force_stokes` `buoyant_force` `spring_force` `centripetal_force` |
| Energy and work | `kinetic_energy` `kinetic_energy_rotational` `potential_energy_gravitational` `potential_energy_gravitational_uniform` `potential_energy_spring` `work_constant_force` `work_along_path` `power_instantaneous` `power_average` `mechanical_energy` |
| Systems of particles | `centre_of_mass` `centre_of_mass_velocity` `reduced_mass` `total_momentum` `collision_elastic` `collision_inelastic` `coefficient_of_restitution` `rocket_delta_v` |
| Rigid bodies | `moment_of_inertia_point` `moment_of_inertia_rod` `moment_of_inertia_disc` `moment_of_inertia_sphere` `moment_of_inertia_shell` `moment_of_inertia_cylinder` `parallel_axis` `perpendicular_axis` `inertia_tensor` `principal_axes` `angular_momentum` `angular_momentum_rigid` `torque` `precession_rate` `gyroscopic_couple` |
| Oscillations | `simple_harmonic_position` `pendulum_period_small` `pendulum_period_exact` `physical_pendulum_period` `torsional_period` `damped_oscillator` `damping_ratio` `quality_factor` `driven_oscillator_amplitude` `resonance_frequency` `beat_frequency` `normal_modes` `coupled_oscillator_step` |
| Gravitation and orbits | `gravitational_force` `gravitational_field` `gravitational_potential` `escape_velocity` `orbital_velocity_circular` `orbital_period` `vis_viva` `kepler_equation_solve` `orbital_elements_from_state` `state_from_orbital_elements` `hohmann_transfer` `bi_elliptic_transfer` `sphere_of_influence` `lagrange_points` `tidal_force` `roche_limit` |
| N-body | `n_body_acceleration` `n_body_step` `barnes_hut_step` `two_body_solve` `three_body_restricted` `jacobi_constant` |
| Continuum | `stress_tensor` `strain_tensor` `youngs_modulus_from` `shear_modulus_from` `bulk_modulus_from` `poisson_ratio` `beam_deflection` `euler_buckling_load` `wave_speed_string` `wave_speed_rod` |

**`newton_second_law_acceleration` exists and `newton_second_law` does not**, and
the reason is worth a paragraph because it generalises.

Newton's second law is `F = m·a`. Written as a function it would be
`force(mass, acceleration) -> Force`, whose body is `mass * acceleration`. With
units in the type, that function **is** the multiplication operator: the
dimensions already compose, the result is already a `Force`, and the function adds
a name and a call and no checking whatever. The only genuinely useful form is the
one that is *not* a multiplication —

```science
let a be net_force / mass        # this needs no function
```

— which is also just an operator. So the honest catalogue entry is
`newton_second_law_acceleration` for the case where a caller has a list of forces
and wants the resultant acceleration, and nothing else.

> **Decision 10. A physical law that is a single arithmetic expression over
> dimensioned quantities gets no function. The operator is the law.**
>
> This removes several dozen entries that a physics catalogue would otherwise
> carry — `ohm(v, i)`, `density(m, v)`, `pressure(f, a)`, `momentum(m, v)` — and
> it is the clearest single thing units-in-the-type do for the *shape* of a
> library rather than for its safety. In a language without units, `momentum(m, v)`
> earns its place because the name is the only record that the multiplication was
> a momentum; with units, the type is that record and the name is redundant.
>
> **Kept anyway, deliberately:** `kinetic_energy(mass, velocity)`, because it is
> `0.5 * m * v²` and the one-half and the square are exactly the two things people
> get wrong; `vis_viva`, because the expression is four terms; `reduced_mass`,
> because `m1*m2/(m1+m2)` is easy to write inverted. **The test is whether the
> expression has more than one operator**, which is mechanical and can be applied
> by the sibling note without judgement.
>
> **Cost.** A user searching the reference for "momentum" finds no function and
> must know to write `mass * velocity`. That is a documentation burden, and it
> should be met by a documentation page for each domain that shows the operators,
> not by re-adding the functions.

**Lagrangian and Hamiltonian formulations**, and the finding the request's own
example produced:

| Group | Names |
|---|---|
| Types | `Lagrangian of (T, N)` `Hamiltonian of (T, N)` `GeneralisedCoordinates of (T, N)` `PhaseSpacePoint of (T, N)` `ConstraintSet of (T, N, M)` |
| Lagrangian | `lagrangian_from_energies` `euler_lagrange_residual` `generalised_momenta` `generalised_forces` `conserved_quantities` `noether_current` `cyclic_coordinates` |
| Constraints | `lagrange_multipliers` `holonomic_constraint` `nonholonomic_constraint` `dalembert_residual` `constraint_forces` |
| Hamiltonian | `hamiltonian_from_lagrangian` `legendre_transform_to_hamiltonian` `hamilton_equations` `poisson_bracket` `canonical_transform` `action_angle_variables` `hamilton_jacobi_separate` |
| Integration | `integrate_hamiltonian` (taking a symplectic `OdeMethod` from §5.4) `energy_drift` `symplectic_error` |

> **Decision 11. Generalised coordinates are dimensionless, scaled at the
> boundary. A `Lagrangian` and a `Hamiltonian` are generic over a plain `Real`,
> not over a `Quantity`.**
>
> **This is forced, not chosen.** The generalised coordinates of a double
> pendulum are two angles; of a bead on a wire, one arc length; of a spherical
> pendulum, a length and two angles. They are *dimensionally heterogeneous*, and
> a `Vector of (Quantity of (F64, …), N)` under `scientific-libraries.md` §12.3
> holds N values of **one** dimension — the exponents are const arguments of the
> element type, not of each element. There is no way to write a length and an
> angle into the same vector without a sum type, and a sum type in the inner loop
> of a symplectic integrator is not a numerical library.
>
> So the boundary is: a caller builds a `Lagrangian` by supplying kinetic and
> potential energy *functions* that take dimensionless coordinates and return an
> `Energy`, together with a `Scaling of N` that says what each coordinate's unit
> is. The units live on the scaling; the state vector is bare numbers.
>
> **Rejected: heterogeneous tuples of quantities.** It works for N = 2 and it
> does not generalise, and it makes every integrator generic over a tuple shape.
> **Rejected: an eighth "generalised" dimension.** `unit-literals.md` §6.2
> already rejected an angle dimension for related reasons, and this would be the
> same mistake with a worse name. **Rejected: rational exponents so that a
> single scaled coordinate can absorb the difference.**
> `scientific-libraries.md` §12.6 excludes rational exponents and
> `const-expression-arithmetic.md` §7.2 enforces the exclusion mechanically;
> reopening it for this is not worth it.
>
> **Cost, and it is the honest answer to the request's `Lagrange()` example.**
> The single place in physics where the eponym `Lagrange` is most load-bearing is
> the single place where units-in-the-type buy the *least*: the state vector is
> unchecked, and a user who puts an angle where a length belongs gets a wrong
> answer with no diagnostic. Dimensional analysis stops at the boundary of the
> formulation. This should be said in the module's documentation rather than
> discovered.

**Five signatures.**

```science
use physics.mechanics (kinetic_energy, Lagrangian, euler_lagrange_residual,
                       hamiltonian_from_lagrangian, integrate_hamiltonian)

## `scientific-libraries.md` §12.7's signature, in revision-2 syntax and generic
## over `Real` so that an uncertain mass propagates. The return dimension is
## computed from the arguments'.
function kinetic_energy of T(mass: Mass of T, velocity: Velocity of T)
    -> Energy of T where T: Real

## Decision 11: the coordinates are bare `T`, the energies are `Quantity`. The
## scaling is what reattaches the units, and it is a required argument rather
## than a default, because a default scaling is a silent unit assumption.
function lagrangian_from_energies of (T, const N: Int)(
    kinetic: function(borrowed Vector of (T, N), borrowed Vector of (T, N)) -> Energy of T,
    potential: function(borrowed Vector of (T, N)) -> Energy of T,
    scaling: borrowed Scaling of (T, N),
) -> Lagrangian of (T, N) where T: Real

## The Euler-Lagrange residual, which is zero on a physical path. Returning the
## residual rather than "the equations of motion" is what makes this usable by a
## solver: it is the function a BVP or a variational integrator consumes.
function euler_lagrange_residual of (T, const N: Int)(
    lagrangian: borrowed Lagrangian of (T, N),
    coordinates: borrowed Vector of (T, N),
    velocities: borrowed Vector of (T, N),
    accelerations: borrowed Vector of (T, N),
) -> Vector of (T, N) where T: Real

## The Legendre transform. Fallible, because it requires the kinetic form to be
## invertible in the velocities, which a constrained or degenerate Lagrangian
## does not satisfy.
function hamiltonian_from_lagrangian of (T, const N: Int)(
    lagrangian: borrowed Lagrangian of (T, N),
) -> (Hamiltonian of (T, N), MechanicsError?) where T: Real

## The symplectic requirement is in the type: this takes a `SymplecticMethod`,
## not an `OdeMethod`, so that a non-symplectic integrator cannot be handed to a
## Hamiltonian system by accident. §5.4's symplectic variants are that type.
function integrate_hamiltonian of (T, const N: Int)(
    hamiltonian: borrowed Hamiltonian of (T, N),
    start: borrowed PhaseSpacePoint of (T, N),
    span: (T, T),
    method: SymplecticMethod,
    step: T,
) -> (Array of PhaseSpacePoint of (T, N), MechanicsError?) where T: Real
```

The `SymplecticMethod` narrowing in the last signature is the one place in §6
where a type catches a *physics* error rather than a units error, and it is worth
naming: handing `DormandPrince45` to a hundred-million-step orbital integration
produces a secular energy drift that looks like physics and is not. A separate
type for the symplectic subset makes that unwritable, costs one `choice`
declaration, and is the sort of thing a domain library can do that a general
numerical library cannot.

**In use.**

```science
use physics.mechanics (lagrangian_from_energies, hamiltonian_from_lagrangian,
                       integrate_hamiltonian, SymplecticMethod, Scaling)

## A double pendulum. The coordinates are two angles, so Decision 11 applies and
## the state vector is dimensionless; the energies carry units and the scaling
## records that both coordinates are radians and both lengths are metres.
function double_pendulum() -> (Array of PhaseSpacePoint of (F64, 2), Error?):
    let m1 be 1.0<kg>
    let m2 be 1.0<kg>
    let l1 be 1.0<m>
    let l2 be 1.0<m>
    let g be 9.80665<m/s^2>

    let kinetic be function(q: borrowed Vector of (F64, 2),
                            qdot: borrowed Vector of (F64, 2)) -> Energy of F64:
        let t1 be 0.5 * (m1 + m2) * l1 * l1 * qdot[0] * qdot[0]
        let t2 be 0.5 * m2 * l2 * l2 * qdot[1] * qdot[1]
        let cross be m2 * l1 * l2 * qdot[0] * qdot[1] * (q[0] - q[1]).cos()
        t1 + t2 + cross

    let potential be function(q: borrowed Vector of (F64, 2)) -> Energy of F64:
        let v1 be -(m1 + m2) * g * l1 * q[0].cos()
        let v2 be -m2 * g * l2 * q[1].cos()
        v1 + v2

    let lagrangian be lagrangian_from_energies(
        kinetic, potential, scaling: Scaling.radians_and(l1))

    let hamiltonian, err be hamiltonian_from_lagrangian(lagrangian)
    if err?:
        return ([], err)

    integrate_hamiltonian(
        hamiltonian,
        start: PhaseSpacePoint.new([2.0, 1.0], [0.0, 0.0]),
        span: (0.0, 100.0),
        method: SymplecticMethod.Yoshida4,
        step: 1.0e-4,
    )
```

Inside the closures the multiplications are dimension-checked — `kg·m²·s⁻²` is an
`Energy` and the compiler knows it — and a dropped `0.5` or a `l1` written where
`l2` belongs is not caught, because both are lengths. That boundary is exactly
where §7 puts it, and stating it here rather than in the summary is the point.

### 6.3 Thermodynamics and statistical mechanics

| Group | Names |
|---|---|
| Ideal gas | `ideal_gas_pressure` `ideal_gas_volume` `ideal_gas_temperature` `ideal_gas_amount` `ideal_gas_density` `root_mean_square_speed` `mean_speed` `most_probable_speed` `mean_free_path` `collision_frequency` |
| Real gases | `equation_of_state` with `EquationOfState` variants `IdealGas`, `VanDerWaals`, `RedlichKwong`, `SoaveRedlichKwong`, `PengRobinson`, `Virial`, `BeattieBridgeman`, `BenedictWebbRubin`; plus `compressibility_factor` `critical_point` `acentric_factor` `reduced_properties` |
| First law | `internal_energy_change` `enthalpy` `heat_capacity_constant_volume` `heat_capacity_constant_pressure` `heat_capacity_ratio` `work_isothermal` `work_adiabatic` `work_isobaric` `work_polytropic` `joule_thomson_coefficient` |
| Second law | `entropy_change_ideal_gas` `entropy_change_phase` `entropy_of_mixing` `gibbs_free_energy` `helmholtz_free_energy` `maxwell_relation` `clausius_clapeyron` `chemical_potential` |
| Cycles | `carnot_efficiency` `otto_efficiency` `diesel_efficiency` `brayton_efficiency` `rankine_efficiency` `stirling_efficiency` `coefficient_of_performance_refrigerator` `coefficient_of_performance_heat_pump` `cycle_analyse` |
| Heat transfer | `conduction_flux` `conduction_resistance` `convection_flux` `radiation_flux` `radiation_exchange` `overall_heat_transfer_coefficient` `fin_efficiency` `lumped_capacitance_time` `biot_number` `fourier_number` `nusselt_from_correlation` |
| Radiation | `blackbody_spectral_wavelength` `blackbody_spectral_frequency` `stefan_boltzmann_power` `wien_peak_wavelength` `wien_peak_frequency` `emissivity_grey` `view_factor` `planck_integral_band` |
| Statistical mechanics | `partition_function_canonical` `partition_function_grand` `partition_function_translational` `partition_function_rotational` `partition_function_vibrational` `partition_function_electronic` `boltzmann_factor` `boltzmann_population` `ensemble_average` `fluctuation_dissipation` |
| Distributions | `maxwell_boltzmann_speed_pdf` `maxwell_boltzmann_energy_pdf` `fermi_dirac_occupation` `bose_einstein_occupation` `fermi_energy` `fermi_temperature` `chemical_potential_fermi` `bose_condensation_temperature` `debye_heat_capacity` `einstein_heat_capacity` |
| Phase and transport | `phase_of` `triple_point` `latent_heat` `vapour_pressure_antoine` `viscosity_sutherland` `thermal_conductivity_kinetic` `diffusion_coefficient_kinetic` `prandtl_number` |

**`Temperature` is the affine problem, and this note does not reopen it.**
`scientific-libraries.md` §12.6 excludes affine units and makes Celsius a separate
type with explicit conversions; `unit-literals.md` §6.1 disagrees in part,
permitting an affine unit *inside a literal and nowhere else*. Thermodynamics is
the module where that disagreement is felt on every line, and this note takes
`unit-literals.md`'s side for one reason it can add: the signatures below all
take `Temperature of T` in kelvin, and the *only* place a user writes a
temperature is a literal. `25<degC>` at the literal, converted exactly at compile
time to 298.15 K, gives the user the spelling they want and gives the library the
absolute scale it requires, and no arithmetic on Celsius ever exists. Named as a
preference between two siblings, with the reason, per `README.md`'s convention.

**Five signatures.**

```science
use physics.thermo (ideal_gas_pressure, equation_of_state, carnot_efficiency,
                    blackbody_spectral_wavelength, fermi_dirac_occupation)

## `scientific-libraries.md` §12.7's signature, with the dimension aliases rather
## than the raw exponent vectors, because `Amount of F64` is readable and
## `Quantity of (F64, 0, 0, 0, 0, 0, 1, 0)` is not.
function ideal_gas_pressure of T(
    amount: Amount of T,
    temperature: Temperature of T,
    volume: Volume of T,
) -> Pressure of T where T: Real

## N2: the equation of state is a value. The critical constants come with it,
## because van der Waals without `a` and `b` is not an equation of state.
function equation_of_state of T(
    model: EquationOfState of T,
    temperature: Temperature of T,
    molar_volume: MolarVolume of T,
) -> (Pressure of T, ThermoError?) where T: Real

## Dimensionless return, and the type says so: an efficiency is a ratio, which
## under `unit-literals.md` §6.2's rule for all-zero exponents is a plain `T`.
function carnot_efficiency of T(hot: Temperature of T, cold: Temperature of T)
    -> (T, ThermoError?) where T: Real

## Spectral radiance per unit wavelength — the dimension is W·m⁻²·sr⁻¹·m⁻¹, which
## is why the alias is named rather than spelled, and why getting it wrong is the
## most common error in radiometry.
function blackbody_spectral_wavelength of T(
    temperature: Temperature of T,
    wavelength: Length of T,
) -> SpectralRadianceWavelength of T where T: Real

## Statistical mechanics returns occupation numbers, which are dimensionless, and
## takes energies, which are not. The mixture is the normal case.
function fermi_dirac_occupation of T(
    energy: Energy of T,
    chemical_potential: Energy of T,
    temperature: Temperature of T,
) -> T where T: Real
```

**In use.**

```science
use physics.thermo (carnot_efficiency)

## The theoretical ceiling on a steam plant, written with the temperatures in the
## units an engineer uses and stored in the unit the physics requires.
function plant_ceiling() -> (F64, Error?):
    let boiler be 540<degC>
    let condenser be 33<degC>
    carnot_efficiency(boiler, condenser)
```

`540<degC>` becomes 813.15 K at compile time, exactly, with no runtime
arithmetic — `unit-literals.md` §5.2 — and `boiler - condenser` is not something
the program can write, because `Temperature` in Celsius never exists as a value.
Both halves of that sentence are the affine decision earning its cost.

### 6.4 Electromagnetism

| Group | Names |
|---|---|
| Electrostatics | `coulomb_force` `electric_field_point` `electric_field_dipole` `electric_field_line_charge` `electric_field_plane` `electric_field_ring` `electric_potential_point` `electric_potential_dipole` `potential_from_field` `field_from_potential` `gauss_flux` `electric_dipole_moment` `torque_on_dipole` |
| Conductors and dielectrics | `capacitance_parallel_plate` `capacitance_cylindrical` `capacitance_spherical` `capacitance_series` `capacitance_parallel` `energy_stored_capacitor` `energy_density_electric` `polarisation` `bound_charge_density` `relative_permittivity_from` `clausius_mossotti` |
| Current and circuits | `current_density` `drift_velocity` `resistance_from_resistivity` `resistivity_at_temperature` `resistance_series` `resistance_parallel` `power_dissipated` `emf_from_cell` `terminal_voltage` `kirchhoff_solve` `wheatstone_balance` |
| Magnetostatics | `biot_savart_element` `magnetic_field_wire` `magnetic_field_loop` `magnetic_field_solenoid` `magnetic_field_toroid` `magnetic_field_dipole` `ampere_loop_integral` `magnetic_flux` `magnetic_force_on_charge` `magnetic_force_on_wire` `lorentz_force` `cyclotron_radius` `cyclotron_frequency` `hall_voltage` `magnetic_dipole_moment` |
| Materials | `magnetisation` `relative_permeability_from` `susceptibility_magnetic` `hysteresis_loss` `curie_temperature_from` |
| Induction | `faraday_emf` `motional_emf` `lenz_sign` `self_inductance_solenoid` `mutual_inductance` `energy_stored_inductor` `energy_density_magnetic` `transformer_ratio` |
| AC circuits | `impedance_resistor` `impedance_capacitor` `impedance_inductor` `impedance_series` `impedance_parallel` `resonance_frequency_lc` `quality_factor_rlc` `bandwidth_rlc` `phase_angle` `power_factor` `rms_from_peak` `peak_from_rms` `real_power` `reactive_power` `apparent_power` |
| Waves and radiation | `wave_impedance_medium` `poynting_vector` `intensity_from_field` `radiation_pressure` `larmor_power` `dipole_radiation_pattern` `skin_depth` `plasma_frequency` `cutoff_frequency_waveguide` `transmission_line_impedance` `reflection_coefficient` `standing_wave_ratio` |
| Maxwell, as equations | `maxwell_residual_gauss` `maxwell_residual_gauss_magnetic` `maxwell_residual_faraday` `maxwell_residual_ampere` `fdtd_step_yee` |

**`lorentz_force` keeps the eponym and `coulomb_force` keeps its own**, and both
are N3 rather than Decision 10 casualties: `q(E + v × B)` has four operators and a
cross product, and `kq₁q₂/r²` has three and a square. Both pass the more-than-one-operator
test comfortably.

**`maxwell_residual_*` rather than `maxwell_equations`** is the same shape as
§6.2's `euler_lagrange_residual`: the useful form for a solver is the residual,
and a function that "returns the equations" returns nothing a program can use.

**Five signatures.**

```science
use physics.em (coulomb_force, lorentz_force, impedance_series, skin_depth,
                poynting_vector)

## Vector-valued, because a force has a direction and returning a magnitude
## silently makes superposition the caller's problem. `Vector3 of (Force of T)`
## is a fixed-size vector of a dimensioned scalar — Decision 11's heterogeneity
## problem does not arise here, because all three components are forces.
function coulomb_force of T(
    charge_one: Charge of T,
    charge_two: Charge of T,
    separation: Vector3 of (Length of T),
    permittivity: Permittivity of T,
) -> Vector3 of (Force of T) where T: Real

## The cross product is where units-in-the-type does something a scalar library
## cannot: `Velocity × MagneticFluxDensity` has the dimension of an electric
## field, and the compiler computes that rather than trusting the name.
function lorentz_force of T(
    charge: Charge of T,
    field_electric: Vector3 of (ElectricField of T),
    velocity: Vector3 of (Velocity of T),
    field_magnetic: Vector3 of (MagneticFluxDensity of T),
) -> Vector3 of (Force of T) where T: Real

## Complex impedance. `Complex of (Impedance of T)` rather than
## `Impedance of (Complex of T)` — the dimension is outside, exactly as
## `uncertainty.md` Decision 5 puts it for uncertainty, and for the same reason:
## a dimension is not complex.
function impedance_series of T(
    elements: borrowed Array of Complex of (Impedance of T),
) -> Complex of (Impedance of T) where T: Real

## Three arguments of three different dimensions returning a fourth. This is the
## signature a units-free library writes as `skin_depth(rho, f, mu)` and nobody
## can check.
function skin_depth of T(
    resistivity: Resistivity of T,
    frequency: Frequency of T,
    permeability: Permeability of T,
) -> Length of T where T: Real

## `E × H` has the dimension of power per unit area, computed rather than
## asserted.
function poynting_vector of T(
    field_electric: Vector3 of (ElectricField of T),
    field_magnetic: Vector3 of (MagneticFieldStrength of T),
) -> Vector3 of (Irradiance of T) where T: Real
```

**In use.**

```science
use physics.em (skin_depth)

## How thick a copper shield has to be at mains frequency, and at a megahertz.
function copper_shield():
    let rho be 1.68e-8<ohm*m>
    let mu be 4.0 * F64.PI * 1.0e-7<H/m>

    for f in [50<Hz>, 1<kHz>, 1<MHz>, 1<GHz>]:
        let delta be skin_depth(rho, f, mu)
        print(f"{f}: {delta.in(MICROMETRE):.1f} um")
```

### 6.5 Optics

Added to `scientific-libraries.md` §12.1, which folds a handful of optics names
into `physics.em` and thereby loses geometric optics, imaging and polarisation
entirely.

| Group | Names |
|---|---|
| Geometric | `snell_refraction_angle` `critical_angle` `brewster_angle` `deviation_prism` `thin_lens_image_distance` `thin_lens_magnification` `lensmaker_focal_length` `thick_lens_principal_planes` `mirror_image_distance` `spherical_mirror_focal_length` `two_lens_system` `ray_transfer_matrix` `abcd_propagate` `focal_ratio` `depth_of_field` `hyperfocal_distance` |
| Aberrations | `seidel_coefficients` `zernike_decompose` `zernike_evaluate` `strehl_ratio` `wavefront_error_rms` `chromatic_focal_shift` `abbe_number_from` |
| Wave optics | `optical_path_length` `phase_from_path` `two_beam_interference` `michelson_fringe_spacing` `newton_rings_radius` `thin_film_reflectance` `fabry_perot_transmittance` `finesse` `coherence_length` `coherence_time` |
| Diffraction | `single_slit_intensity` `double_slit_intensity` `grating_equation_angle` `grating_resolving_power` `circular_aperture_intensity` `airy_disc_radius` `rayleigh_criterion_angle` `sparrow_criterion_angle` `fresnel_number` `fresnel_diffraction` `fraunhofer_diffraction` `angular_spectrum_propagate` |
| Polarisation | `JonesVector` `JonesMatrix` `StokesVector` `MuellerMatrix` `malus_intensity` `jones_polariser` `jones_retarder` `jones_rotator` `stokes_from_jones` `degree_of_polarisation` `fresnel_coefficients_s` `fresnel_coefficients_p` `reflectance_unpolarised` |
| Gaussian beams | `beam_waist_at` `rayleigh_range` `beam_divergence` `radius_of_curvature_at` `gouy_phase` `beam_quality_m_squared` `abcd_gaussian_propagate` |
| Photometry and radiometry | `radiance_to_luminance` `luminous_flux_from_radiant` `illuminance_at_distance` `inverse_square_illuminance` `etendue` `photon_flux_from_power` |
| Fibres and waveguides | `numerical_aperture` `acceptance_angle` `v_number` `mode_count_step_index` `dispersion_material` `dispersion_waveguide` `attenuation_db_per_km` |

`airy_disc_radius` is N3 and is unambiguous only because the noun is there: Airy
also names the two functions in §5.2, and `airy_ai` and `airy_disc_radius` share a
prefix and nothing else. That is the rule working on a collision that would
otherwise be invisible until somebody imported both.

**Five signatures.**

```science
use physics.optics (snell_refraction_angle, thin_lens_image_distance,
                    airy_disc_radius, malus_intensity, rayleigh_range)

## Fallible, and this is the case for it: beyond the critical angle there is no
## refracted ray, and returning a NaN is how that becomes a wrong plot rather
## than an error. The angle is dimensionless per `unit-literals.md` §6.2, so it
## is a plain `T`, and the doc comment says radians because the type cannot.
function snell_refraction_angle of T(
    index_incident: T,
    index_transmitted: T,
    angle_incident: T,
) -> (T, OpticsError?) where T: Real

## Two lengths in, one length out, and the sign convention is the doc comment's
## job because a sign is not a dimension.
function thin_lens_image_distance of T(
    focal_length: Length of T,
    object_distance: Length of T,
) -> (Length of T, OpticsError?) where T: Real

## The resolution limit of an aperture. Wavelength and aperture are both lengths
## and the f-number is dimensionless, so the product is a length: the checker
## confirms the formula's shape and cannot confirm the 1.22.
function airy_disc_radius of T(
    wavelength: Length of T,
    aperture_diameter: Length of T,
    focal_length: Length of T,
) -> Length of T where T: Real

## Malus's law. Intensity in, intensity out, angle dimensionless.
function malus_intensity of T(
    incident: Irradiance of T,
    angle: T,
) -> Irradiance of T where T: Real

## A Gaussian beam's Rayleigh range. Two lengths and a refractive index in, one
## length out.
function rayleigh_range of T(
    waist: Length of T,
    wavelength: Length of T,
    refractive_index: T,
) -> Length of T where T: Real
```

**In use.**

```science
use physics.optics (airy_disc_radius, rayleigh_criterion_angle)

## Can this telescope resolve the two components of a binary?
function can_resolve(
    aperture: Length of F64,
    wavelength: Length of F64,
    separation: F64,
) -> Bool:
    let limit be rayleigh_criterion_angle(wavelength, aperture)
    separation > limit
```

`separation` is an angle, so it is a bare `F64` under `unit-literals.md` §6.2, and
the comparison against `limit` is therefore unchecked. That is the documented cost
of not making angle a dimension, repeated here because §6.5 is where it bites
most often, and because §6.2 of that note already priced it honestly: *"this
catches no type error. […] What it removes is the conversion error."* The
conversion error — arcseconds handed to a function expecting radians — is removed
by `1.5<arcsec>` at the literal, and that is the half worth having.

### 6.6 Quantum mechanics

| Group | Names |
|---|---|
| Old quantum theory | `de_broglie_wavelength` `photon_energy_from_frequency` `photon_energy_from_wavelength` `photon_momentum` `compton_shift` `photoelectric_kinetic_energy` `work_function_from_threshold` `bohr_energy_level` `bohr_radius_level` `bohr_velocity_level` `rydberg_transition_wavelength` `moseley_frequency` |
| Wave mechanics | `schrodinger_residual_time_independent` `schrodinger_step_split_operator` `schrodinger_step_crank_nicolson` `normalise_wavefunction` `probability_density` `probability_current` `expectation_value` `variance_of_observable` `uncertainty_product` `overlap_integral` `orthonormalise` |
| Exactly solvable systems | `particle_in_box_energy` `particle_in_box_wavefunction` `harmonic_oscillator_energy` `harmonic_oscillator_wavefunction` `finite_well_energies` `delta_well_energy` `hydrogen_energy` `hydrogen_radial_wavefunction` `hydrogen_wavefunction` `rigid_rotor_energy` `morse_potential_energy` |
| Barriers and scattering | `transmission_rectangular_barrier` `reflection_rectangular_barrier` `tunnelling_probability_wkb` `wkb_phase_integral` `transfer_matrix_step` `scattering_phase_shift` `partial_wave_cross_section` `born_approximation_amplitude` `resonance_breit_wigner` |
| Angular momentum and spin | `pauli_matrix` `spin_operator` `spin_expectation` `ladder_raise` `ladder_lower` `clebsch_gordan` `wigner_3j` `wigner_6j` `wigner_9j` `racah_w` `spherical_tensor_operator` `zeeman_shift` `hyperfine_splitting` `lande_g_factor` |
| Matrix and operator formulation | `commutator` `anticommutator` `matrix_element` `hamiltonian_matrix` `diagonalise_hamiltonian` `time_evolution_operator` `propagate_state` `perturbation_first_order_energy` `perturbation_second_order_energy` `perturbation_first_order_state` `variational_energy` `rayleigh_ritz` |
| Density matrices and information | `density_matrix_pure` `density_matrix_mixed` `partial_trace` `purity` `von_neumann_entropy` `linear_entropy` `fidelity` `trace_distance` `concurrence` `entanglement_entropy` `bloch_vector` `bloch_from_density` |
| Quantum computing primitives | `gate_hadamard` `gate_pauli_x` `gate_pauli_y` `gate_pauli_z` `gate_phase` `gate_rotation` `gate_cnot` `gate_toffoli` `apply_gate` `measure_in_basis` `bell_state` `ghz_state` |
| Many-body and field | `slater_determinant` `hartree_fock_step` `second_quantised_hamiltonian` `creation_operator` `annihilation_operator` `number_operator` `occupation_from_state` |

**Everything here is F1**, and more strongly than the rest of §6: a wavefunction
is an array, an operator is a matrix, and the correctness of `expectation_value`
is exactly the statement that the operator's shape matches the state's.
`scientific-libraries.md` §4's rule applies without argument.

**Units are unusually weak in this module, and the reason is instructive.** Almost
everything in quantum mechanics is customarily expressed in atomic units, where ħ
= mₑ = e = 4πε₀ = 1 and every quantity is dimensionless by construction. A
`Quantity`-typed quantum library therefore either fights its own audience's
convention or checks nothing. The resolution adopted here is the same as Decision
11's: **the *interface* carries units and the *computation* does not.** Energies
in and out are `Energy of T`; the matrices and state vectors inside are bare
`Real`, scaled at the boundary by a `UnitSystem` value that records whether the
caller is in SI, atomic or natural units. A user who mixes two conventions gets a
wrong answer, and there is no type that would have stopped them. Stated, not
hidden.

**Five signatures.**

```science
use physics.quantum (de_broglie_wavelength, particle_in_box_energy,
                     expectation_value, uncertainty_product, von_neumann_entropy)

## Two dimensioned arguments, one dimensioned result, and the constant is
## internal. This is the shape that works best in this module.
function de_broglie_wavelength of T(mass: Mass of T, velocity: Velocity of T)
    -> Length of T where T: Real

## Quantum numbers are `Int`, not `T`, and the type saying so removes the
## commonest transcription error in the module.
function particle_in_box_energy of T(
    level: Int,
    mass: Mass of T,
    width: Length of T,
) -> Energy of T where T: Real

## F1. The operator's shape must match the state's, twice, and that is the whole
## correctness condition. Bare `Complex of T` inside, per the unit-system note
## above.
function expectation_value of (T, const N: Int)(
    state: borrowed Vector of (Complex of T, N),
    operator: borrowed Matrix of (Complex of T, N, N),
) -> (T, QuantumError?) where T: Real

## Returns the product and the bound it must exceed, rather than a Bool, because
## a user checking Heisenberg wants to see by how much.
function uncertainty_product of (T, const N: Int)(
    state: borrowed Vector of (Complex of T, N),
    observable_one: borrowed Matrix of (Complex of T, N, N),
    observable_two: borrowed Matrix of (Complex of T, N, N),
) -> (T, T, QuantumError?) where T: Real

## Fallible on trace: a density matrix whose trace is not one is not a density
## matrix, and silently normalising it hides a bug upstream.
function von_neumann_entropy of (T, const N: Int)(
    density: borrowed Matrix of (Complex of T, N, N),
    base: EntropyBase,
) -> (T, QuantumError?) where T: Real
```

**In use.**

```science
use physics.quantum (particle_in_box_energy)
use physics.constants (ELECTRON_MASS)

## The first four levels of an electron in a one-nanometre box, in electron-volts.
function box_levels():
    let width be 1<nm>
    for n in 1..5:
        let e be particle_in_box_energy(n, ELECTRON_MASS, width)
        print(f"n = {n}: {e.in(ELECTRONVOLT):.4f} eV")
```

`ELECTRON_MASS` carries a CODATA uncertainty (§6.1), so `e` is an
`Energy of (Uncertain of F64)` and the formatter renders it with the right number
of digits — `publishable-output.md`'s PDG significant-figure rule — without the
program asking. That is three notes composing with no glue code, and it is the
best small demonstration in this document of why the types were worth it.

### 6.7 Relativity, special and general

| Group | Names |
|---|---|
| Kinematics | `lorentz_factor` `beta_from_velocity` `velocity_from_beta` `rapidity` `velocity_from_rapidity` `time_dilation` `proper_time` `length_contraction` `velocity_addition` `velocity_addition_transverse` `aberration_angle` |
| Dynamics | `relativistic_momentum` `relativistic_energy` `rest_energy` `kinetic_energy_relativistic` `total_energy_from_momentum` `momentum_from_energy` `invariant_mass` `relativistic_force` `relativistic_doppler_longitudinal` `relativistic_doppler_transverse` `headlight_angle` |
| Four-vectors and tensors | `FourVector of T` `FourMomentum of T` `MetricSignature` `minkowski_interval` `minkowski_dot` `lorentz_boost_matrix` `lorentz_rotation_matrix` `boost_four_vector` `wigner_rotation` `centre_of_momentum_frame` `mandelstam_s` `mandelstam_t` `mandelstam_u` |
| Electromagnetism, covariant | `field_strength_tensor` `transform_fields_boost` `four_current` `four_potential` |
| General relativity — metrics | `Metric of (T, const D: Int)` `schwarzschild_metric` `kerr_metric` `reissner_nordstrom_metric` `friedmann_robertson_walker_metric` `minkowski_metric` `metric_determinant` `metric_inverse` |
| General relativity — curvature | `christoffel_symbols` `riemann_tensor` `ricci_tensor` `ricci_scalar` `einstein_tensor` `weyl_tensor` `kretschmann_scalar` `geodesic_residual` `geodesic_step` `parallel_transport_step` |
| Black holes and compact objects | `schwarzschild_radius` `photon_sphere_radius` `innermost_stable_circular_orbit` `kerr_horizon_radius` `ergosphere_radius` `hawking_temperature` `bekenstein_hawking_entropy` `black_hole_evaporation_time` `gravitational_redshift` `shapiro_delay` `light_deflection_angle` `perihelion_precession` |
| Cosmology | `hubble_parameter_at` `scale_factor_at` `comoving_distance` `luminosity_distance` `angular_diameter_distance` `lookback_time` `age_of_universe` `critical_density` `density_parameter` `redshift_from_scale_factor` `friedmann_residual` |
| Gravitational waves | `chirp_mass` `inspiral_frequency_at` `inspiral_time_to_merger` `strain_amplitude` `quadrupole_luminosity` |

**`MetricSignature` is an explicit argument and never a default**, which is one of
the few places a physics library can prevent a real error with a type. The
(−,+,+,+) and (+,−,−,−) conventions are both universal, they differ by a sign on
every interval, and a program that mixes a general-relativity source using one
with a particle-physics source using the other produces plausible wrong numbers.
Making the signature a required argument of `minkowski_interval`,
`minkowski_dot` and every metric constructor costs one token per call and closes
the hole. This is Decision 11's cousin: where the type system cannot check the
physics, make the convention an argument so at least it is written down.

**Units here are the strongest in §6 and the weakest at the same time.** The
dimensioned signatures — `schwarzschild_radius(mass) -> Length` — are exactly
checkable. The tensor work is not: a Christoffel symbol has units that depend on
which indices are up and which are down, and expressing that would need a
dimension per index, which `scientific-libraries.md` §12.3's single exponent
vector per *type* cannot do. Geometrised units (G = c = 1) are the field's answer
and they are the same answer as atomic units in §6.6: **interface dimensioned,
computation bare, scaled at the boundary.** Three modules have now reached the
same conclusion independently, which suggests it is the general rule rather than
three exceptions, and §7.1 says so.

**Five signatures.**

```science
use physics.relativity (lorentz_factor, schwarzschild_radius, invariant_mass,
                        minkowski_interval, luminosity_distance)

## `scientific-libraries.md` §12.7's signature, in revision-2 syntax and generic
## over `Real`. Dimensionless return, so a plain `T`.
function lorentz_factor of T(velocity: Velocity of T) -> (T, RelativityError?)
    where T: Real

## §12.7's second signature. One dimensioned argument, one dimensioned result,
## two constants inside.
function schwarzschild_radius of T(mass: Mass of T) -> Length of T where T: Real

## A four-momentum carries an energy and three momenta, which are different
## dimensions, so `FourMomentum` is a struct with named fields and not a
## `Vector of (Quantity …, 4)` — Decision 11's heterogeneity constraint, again.
function invariant_mass of T(
    momenta: borrowed Array of FourMomentum of T,
    signature: MetricSignature,
) -> (Mass of T, RelativityError?) where T: Real

## The signature is required, not defaulted, for the reason above.
function minkowski_interval of T(
    a: borrowed FourVector of T,
    b: borrowed FourVector of T,
    signature: MetricSignature,
) -> Quantity of (T, 2, 0, -2, 0, 0, 0, 0) where T: Real

## Cosmology takes a cosmology, not seven loose parameters, because the seven
## are correlated and passing six of them is the commonest error in the field.
function luminosity_distance of T(
    redshift: T,
    cosmology: borrowed Cosmology of T,
) -> (Length of T, RelativityError?) where T: Real
```

The `minkowski_interval` return type is written out rather than aliased on
purpose: the squared interval has dimension m², and there is no conventional
one-word name for it. That is a small, real limitation of alias-based ergonomics
and it is better shown than described.

**In use.**

```science
use physics.relativity (lorentz_factor, time_dilation)

## How much slower does a clock run on the ISS? The answer is about 28
## microseconds a day from velocity alone, and the calculation is one line once
## the units are in the type.
function iss_clock_drift() -> (Time of F64, Error?):
    let orbital_speed be 7660<m/s>
    let day be 86400<s>

    let gamma, err be lorentz_factor(orbital_speed)
    if err?:
        return (0<s>, err)

    (day * (1.0 - 1.0 / gamma), null)
```

`day * (1.0 - 1.0 / gamma)` is a `Time` times a dimensionless number, which is a
`Time`, and the compiler knows it. If `day` had been written `86400` — a bare
number — the multiplication would still compile and the result would be a bare
number, and the error would surface at the return type rather than at the
mistake. §7.2's diagnostic is about exactly that distance.

### 6.8 Fluid dynamics

Added; absent from `scientific-libraries.md` §12.1 entirely.

| Group | Names |
|---|---|
| Fluid statics | `hydrostatic_pressure` `gauge_pressure` `absolute_pressure` `buoyant_force` `centre_of_pressure` `manometer_difference` `surface_tension_rise` `capillary_length` `laplace_pressure` |
| Dimensionless groups | `reynolds_number` `mach_number` `froude_number` `weber_number` `bond_number` `capillary_number` `strouhal_number` `euler_number_fluid` `knudsen_number` `peclet_number` `rayleigh_number` `grashof_number` `richardson_number` `schmidt_number` `sherwood_number` `stokes_number` |
| Ideal flow | `continuity_velocity` `bernoulli_pressure` `bernoulli_velocity` `stagnation_pressure` `dynamic_pressure` `venturi_flow_rate` `pitot_velocity` `torricelli_velocity` `stream_function_of` `velocity_potential_of` `circulation` `kutta_joukowsky_lift` |
| Viscous and pipe flow | `hagen_poiseuille_flow_rate` `pressure_drop_pipe` `friction_factor_darcy` `friction_factor_colebrook` `friction_factor_haaland` `moody_friction_factor` `entrance_length` `hydraulic_diameter` `minor_loss` `stokes_drag` `terminal_velocity` `settling_velocity` |
| Boundary layers | `boundary_layer_thickness_laminar` `boundary_layer_thickness_turbulent` `displacement_thickness` `momentum_thickness` `skin_friction_coefficient` `blasius_solve` `separation_point_estimate` |
| Compressible flow | `speed_of_sound_gas` `isentropic_pressure_ratio` `isentropic_temperature_ratio` `isentropic_density_ratio` `area_mach_relation` `normal_shock_relations` `oblique_shock_angle` `prandtl_meyer_angle` `fanno_line` `rayleigh_line` `choked_mass_flow` |
| Turbulence | `turbulent_kinetic_energy` `dissipation_rate` `kolmogorov_length` `kolmogorov_time` `taylor_microscale` `integral_length_scale` `mixing_length` `eddy_viscosity` `law_of_the_wall_velocity` `y_plus` |
| Navier–Stokes | `navier_stokes_residual` `navier_stokes_step_projection` `navier_stokes_step_simple` `vorticity_from_velocity` `strain_rate_tensor` `poisson_pressure_solve` `lattice_boltzmann_step` |
| Open channel and waves | `manning_velocity` `chezy_velocity` `specific_energy` `hydraulic_jump_depth` `wave_celerity_shallow` `wave_celerity_deep` `dispersion_relation_gravity_wave` |
| Aerodynamics | `lift_coefficient_from` `drag_coefficient_from` `lift_to_drag` `induced_drag_coefficient` `aspect_ratio` `oswald_efficiency` `dynamic_pressure_at_altitude` `standard_atmosphere_at` |

**Fluids is the domain that most wants rational exponents and does not get them.**
`scientific-libraries.md` §12.6 excludes them and prices the exclusion as *"real in
a few corners of fluid dynamics"*; those corners are here. The friction velocity
is √(τ/ρ); the Kolmogorov length is (ν³/ε)^(1/4); the speed of sound is √(γRT).
`const-expression-arithmetic.md` §7.2's `sqrt` works for even exponents and raises
`SC0254` for odd ones, so **√(τ/ρ) is fine** — `Pa/(kg/m³)` is m²·s⁻², an even
dimension, and the square root is a velocity. The genuine casualty is the quarter
power in the Kolmogorov scales, which cannot be written as a `Quantity` operation
at all and must be computed on magnitudes and re-attached. The catalogue therefore
carries `kolmogorov_length` as a function that does the re-attachment internally,
which is the right place for it: one library function contains the escape, and no
user code does. Named, so that the exclusion's cost is visible where it is paid.

**Five signatures.**

```science
use physics.fluids (reynolds_number, bernoulli_velocity, friction_factor_colebrook,
                    normal_shock_relations, terminal_velocity)

## Four dimensioned arguments collapsing to a dimensionless group. This is the
## single best advertisement for units in a type in the whole note: a Reynolds
## number that comes out with a leftover dimension is a Reynolds number computed
## with a diameter where a radius belonged, and the compiler says so.
function reynolds_number of T(
    density: Density of T,
    velocity: Velocity of T,
    length: Length of T,
    dynamic_viscosity: DynamicViscosity of T,
) -> T where T: Real

## Two pressures and a density in, a velocity out. The square root is of an even
## dimension, so `const-expression-arithmetic.md` §7.2 permits it.
function bernoulli_velocity of T(
    pressure_total: Pressure of T,
    pressure_static: Pressure of T,
    density: Density of T,
) -> (Velocity of T, FluidError?) where T: Real

## Implicit: Colebrook is solved by iteration, so it can fail to converge, and
## `root` from §5.6 is what it calls.
function friction_factor_colebrook of T(
    reynolds: T,
    relative_roughness: T,
    tolerance: Tolerance,
) -> (T, FluidError?) where T: Real

## Returns the whole set of ratios, because a caller who wants one almost always
## wants three, and five separate functions would recompute the same quantity.
function normal_shock_relations of T(
    mach_upstream: T,
    heat_capacity_ratio: T,
) -> (ShockRatios of T, FluidError?) where T: Real

## Stokes or Newton regime by Reynolds number, chosen inside, which is why this
## is one function and not two: the caller does not know the regime in advance,
## because the regime depends on the answer.
function terminal_velocity of T(
    particle_diameter: Length of T,
    particle_density: Density of T,
    fluid_density: Density of T,
    dynamic_viscosity: DynamicViscosity of T,
    gravity: Acceleration of T,
) -> (Velocity of T, FluidError?) where T: Real
```

**In use.**

```science
use physics.fluids (reynolds_number)

## Laminar or turbulent? Water at 20 C through a 25 mm pipe at 2 m/s.
function pipe_regime() -> String:
    let re be reynolds_number(
        density: 998.2<kg/m^3>,
        velocity: 2.0<m/s>,
        length: 25<mm>,
        dynamic_viscosity: 1.002<mPa*s>,
    )
    if re < 2300.0:
        "laminar"
    else if re > 4000.0:
        "turbulent"
    else:
        "transitional"
```

`25<mm>` and `1.002<mPa*s>` fold to 0.025 m and 1.002 × 10⁻³ Pa·s at compile
time, exactly, per `unit-literals.md` §5.2 — no runtime conversion, no rounding
introduced by the conversion, and the SI prefix composed from the table rather
than declared. The argument labels are part of the call under
`stdlib-shape-and-packages.md` §4.3, which matters here more than usual: four
arguments of four different dimensions in an order nobody remembers is exactly
the call site labels exist for, and a wrong *order* would now be caught twice —
once by the label and once by the dimension.

### 6.9 Nuclear and particle physics

Added; absent from `scientific-libraries.md` §12.1.

| Group | Names |
|---|---|
| Nuclear structure | `binding_energy` `binding_energy_per_nucleon` `mass_defect` `semi_empirical_mass_formula` `separation_energy_neutron` `separation_energy_proton` `nuclear_radius` `nuclear_density` `shell_model_magic` `deformation_parameter` |
| Decay | `decay_constant_from_half_life` `half_life_from_decay_constant` `mean_lifetime` `activity_at` `remaining_nuclei` `decay_chain_solve` `bateman_solution` `secular_equilibrium_activity` `transient_equilibrium_activity` `branching_ratio` |
| Decay modes | `q_value_alpha` `q_value_beta_minus` `q_value_beta_plus` `q_value_electron_capture` `geiger_nuttall_half_life` `fermi_beta_spectrum` `kurie_plot_value` `gamow_factor` `internal_conversion_coefficient` |
| Reactions | `q_value_reaction` `threshold_energy` `cross_section_from_rate` `reaction_rate` `resonance_breit_wigner_cross_section` `compound_nucleus_width` `optical_model_transmission` |
| Fission and fusion | `fission_energy_release` `neutron_multiplication_factor` `four_factor_formula` `six_factor_formula` `critical_mass_estimate` `neutron_diffusion_length` `delayed_neutron_fraction` `reactivity_from_k` `inhour_equation` `lawson_criterion` `fusion_triple_product` `coulomb_barrier` `tunnelling_fusion_rate` |
| Radiation interaction | `stopping_power_electronic` `stopping_power_nuclear` `bethe_bloch` `range_csda` `attenuation_coefficient_linear` `attenuation_coefficient_mass` `half_value_layer` `buildup_factor` `klein_nishina_cross_section` `pair_production_threshold` `bremsstrahlung_power` `cherenkov_angle` `cherenkov_threshold` |
| Dosimetry | `absorbed_dose` `equivalent_dose` `effective_dose` `dose_rate_point_source` `kerma` `exposure_from_fluence` `radiation_weighting_factor` `tissue_weighting_factor` |
| Particle kinematics | `invariant_mass_from_pair` `transverse_momentum` `rapidity_particle` `pseudorapidity` `missing_transverse_energy` `centre_of_mass_energy` `lab_to_cm_angle` `decay_two_body_momentum` `dalitz_boundaries` `phase_space_volume` |
| Particle properties | `ParticleId` `mass_of_particle` `charge_of_particle` `lifetime_of_particle` `spin_of_particle` `quark_content` `is_hadron` `is_lepton` `is_boson` `antiparticle_of` |
| Interactions | `coupling_constant_at_scale` `running_alpha_strong` `running_alpha_em` `weinberg_angle` `fermi_coupling` `ckm_element` `pmns_element` `neutrino_oscillation_probability` `mass_squared_difference` |

**`ParticleId` is a type, not a string**, and it is the one design decision in
this module worth arguing. The alternative is `mass_of_particle("pi+")`, which
puts a typo into a runtime error and a PDG code into a string literal. A `choice`
with several hundred variants is unwieldy; the design adopted is a newtype over
the PDG Monte Carlo numbering scheme with named constants — `ParticleId.PION_PLUS`
is 211 — so that the name is checked, the wire format is standard, and a particle
from a data file round-trips as an integer. This is `stdlib-shape-and-packages.md`
§5.7's *"named for the format, which is a standard"* applied one level down.

**Dosimetry is where the unit system earns its keep and also where it is
famously insufficient.** The gray and the sievert have the *same* dimension —
J/kg, m²·s⁻² — and mean entirely different things: absorbed dose and equivalent
dose differ by a weighting factor that depends on the radiation type. A
dimensional checker cannot tell them apart. This is the same class of problem as
§7.3's, it is the cleanest small example of it in the corpus, and the resolution
is the same: `AbsorbedDose` and `EquivalentDose` are **distinct named types over
the same dimension**, not aliases, so that assignment between them requires the
weighting function that converts them. `unit-literals.md` §3.4's open unit
vocabulary makes `Gy` and `Sv` distinct unit *names*; making them distinct
*types* is a library decision on top of that, and this note takes it.

> **Decision 12. Where two physical quantities share a dimension and differ in
> meaning, the library declares distinct types over the same exponent vector and
> supplies the conversion as a named function. The dimension check is a floor,
> not a ceiling.**
>
> Instances: `AbsorbedDose` / `EquivalentDose` / `Kerma` (all J/kg); `Torque` /
> `Energy` (both N·m); `Frequency` / `Becquerel` / `AngularVelocity` (all s⁻¹);
> `Entropy` / `HeatCapacity` (both J/K).
>
> **Rejected: rely on the dimension alone.** It is what `pint` and `uom` do and
> it lets a torque be added to an energy, which is meaningless and is in every
> introductory mechanics course's list of student errors.
>
> **Rejected: add a dimension for each.** An eighth and ninth exponent for "kind"
> breaks §12.3's seven-element vector for a distinction that is per-quantity
> rather than per-dimension, and there is no end to it.
>
> **Cost.** Two types where one would do, an explicit conversion at every
> boundary between them, and a user who has a `Torque` and wants to integrate it
> over an angle to get an `Energy` must say so. That last case is real and is the
> price. It is paid because the reverse error — reporting a torque as an energy —
> is the one that reaches a paper.

**Five signatures.**

```science
use physics.nuclear (activity_at, binding_energy_per_nucleon, bethe_bloch,
                     equivalent_dose, neutrino_oscillation_probability)

## Decay. Two dimensioned arguments and a time, returning an activity, which is
## becquerels — s⁻¹ with a meaning, per Decision 12.
function activity_at of T(
    initial_activity: Activity of T,
    half_life: Time of T,
    elapsed: Time of T,
) -> Activity of T where T: Real

## `ParticleId`-adjacent: a nuclide is two integers, and making them arguments
## rather than a parsed string is the same decision as `ParticleId`.
function binding_energy_per_nucleon of T(
    protons: Int,
    neutrons: Int,
) -> (Energy of T, NuclearError?) where T: Real

## Stopping power: energy per unit length, from a particle's charge, mass and
## energy and the medium's properties. Seven arguments of six dimensions, which
## is a signature nobody can check by eye and the compiler checks entirely.
function bethe_bloch of T(
    charge_number: Int,
    mass: Mass of T,
    kinetic_energy: Energy of T,
    medium_atomic_number: T,
    medium_mass_number: T,
    medium_density: Density of T,
    mean_excitation_energy: Energy of T,
) -> (StoppingPower of T, NuclearError?) where T: Real

## Decision 12 in a signature: the input and the output have the same dimension
## and different types, and the weighting factor is the only way across.
function equivalent_dose of T(
    absorbed: AbsorbedDose of T,
    radiation: RadiationKind,
) -> EquivalentDose of T where T: Real

## Two dimensioned arguments and a dimensionless mixing matrix, returning a
## probability. The baseline and the energy are the two things whose units get
## confused — kilometres and GeV are the field's convention and metres and joules
## are the SI — and the literal is where that is fixed.
function neutrino_oscillation_probability of T(
    from_flavour: NeutrinoFlavour,
    to_flavour: NeutrinoFlavour,
    baseline: Length of T,
    energy: Energy of T,
    mixing: borrowed PmnsMatrix of T,
) -> (T, NuclearError?) where T: Real
```

**In use.**

```science
use physics.nuclear (activity_at, equivalent_dose, RadiationKind)

## A sealed caesium-137 source, ten years on, and the equivalent dose from an
## hour at one metre. Two separate unit questions, both settled by the types.
function source_check() -> EquivalentDose of F64:
    let initial be 370<MBq>
    let half_life be 30.08<a>

    let now be activity_at(initial, half_life, elapsed: 10<a>)
    let absorbed be dose_rate_point_source(now, distance: 1<m>) * 1<h>

    equivalent_dose(absorbed, radiation: RadiationKind.Gamma)
```

The last two lines are Decision 12 doing the work. `dose_rate_point_source(…) *
1<h>` is grays; passing it where a sievert is wanted is a *type* error even
though it is not a dimension error, and the fix — naming the radiation kind — is
the physics the conversion actually requires.

---

## 7. What physics genuinely gets from units in the type

This is the section where the language earns the domain rather than shipping a
library, and the honest answer is narrower than the enthusiastic one.

### 7.1 Four candidate answers, and only two survive

A units library exists in every ecosystem. Python has `pint` and `astropy.units`;
Rust has `uom` and `dimensioned`; C++ has `boost::units` and `mp-units`; F# has
units of measure in the language. So the question is not *are units useful* — that
is settled — but **what does putting them in the language provide that a library
in this language could not.** Four candidates were considered.

**Candidate 1: the check itself. Rejected as a language contribution.** A static
units library in a language with const generics gets the dimensional check.
`uom` does, `mp-units` does, and `scientific-libraries.md` §12.5's own conclusion
is that the language's job is const-expression arithmetic and *"units then cost a
library and no further language work."* The check is a library's, once the
arithmetic exists. Claiming it as a language feature would be claiming credit for
the thing the note itself says is a library.

**Candidate 2: the literal. Survives, and it is the larger of the two.** A library
cannot add `9.8<m/s^2>` to the grammar. The alternatives are what the libraries
actually write:

| | Spelling |
|---|---|
| `pint` | `9.8 * ureg.meter / ureg.second ** 2` |
| `astropy` | `9.8 * u.m / u.s**2` |
| `uom` | `Acceleration::new::<meter_per_second_squared>(9.8)` |
| `boost::units` | `9.8 * si::meters / (si::seconds * si::seconds)` |
| Science | `9.8<m/s^2>` |

The gap is not aesthetic. `unit-literals.md` §3.3's decision — *"unit names live
in a namespace of their own, entered only inside `< >`"* — is the thing a library
cannot reproduce at any price, because a library's unit names are ordinary
identifiers in the ordinary namespace, so `m` is the metre *and* the user's mass,
and every units library in every language has this problem and solves it with a
prefix nobody wants to type. A closed sublanguage inside brackets removes it by
construction. **This is the single clearest thing the language provides**, it is
`unit-literals.md`'s and not this note's, and this note's contribution is to say
that it is the *bigger* half.

**Candidate 3: the diagnostic. Survives.** §7.2.

**Candidate 4: exact compile-time scale folding. Survives, marginally, and is
folded into candidate 2.** `unit-literals.md` §5.2 folds the conversion factor as
an exact rational at compile time, so `25<mm>` is 0.025 with no runtime multiply
and no rounding introduced by the conversion. A library converts at run time, in
floating point, and `reproducibility.md`'s whole programme cares about a rounding
that happens on one path and not another. This is real and it is small, and it is
listed under candidate 2 because it is a property of the literal.

So: **the language provides a literal and a diagnostic, and the library provides
the check.** That is a modest claim and it is the defensible one.

One further observation, because three sections reached it independently.
§6.2's Decision 11 (generalised coordinates), §6.6's atomic units and §6.7's
geometrised units all arrived at the same boundary: **interface dimensioned,
computation bare, scaled at the edge.** That is not three exceptions; it is the
general shape of a physics library with a seven-exponent type, and it should be
documented as the pattern rather than apologised for three times. The dimensioned
part is the part a *user* writes, and the bare part is the part a *solver* runs,
and those are different audiences.

### 7.2 The worked diagnostic

The program. A physicist computes the range of a projectile and mistypes the
acceleration due to gravity as a velocity — one character, `<m/s>` for `<m/s^2>`,
the single easiest error to make in the whole system:

```science
use physics.mechanics (projectile_range)

function shot() -> Length of F64:
    let speed be 20<m/s>
    let angle be 45<deg>
    let g be 9.80665<m/s>          # should be m/s^2
    projectile_range(speed, angle, g)
```

What a dynamic units library reports, at run time, after the simulation has run:

```
DimensionalityError: Cannot convert from 'meter / second ** 3'
([length] / [time] ** 3) to 'meter' ([length])
```

What a static units library in a general-purpose language reports, at compile
time, and this is `uom`'s actual shape:

```
error[E0308]: mismatched types
  expected struct `Quantity<dyn Dimension<L = PInt<UInt<UTerm, B1>>, M = Z0,
           T = NInt<UInt<UInt<UTerm, B1>, B0>>, I = Z0, Th = Z0, N = Z0, J = Z0>,
           dyn Units<f64, ...>, f64>`
     found struct `Quantity<dyn Dimension<L = PInt<UInt<UTerm, B1>>, M = Z0,
           T = NInt<UInt<UTerm, B1>>, ...>, dyn Units<f64, ...>, f64>`
```

Both are correct. Neither is a diagnostic a scientist reads. The first arrives too
late and names the wrong quantity — the caller sees a `m/s³` it never wrote,
because the error surfaced inside the library's arithmetic rather than at the
argument. The second arrives at the right time and renders the dimension as
type-level Peano integers.

What Science should print, and this is the section's actual content:

```
error[SC0256]: argument has the wrong dimension
 --> shot.science:7:37
  |
6 |     let g be 9.80665<m/s>          # should be m/s^2
  |              ------------ this has dimension m/s
7 |     projectile_range(speed, angle, g)
  |     ---------------- ^ expected m/s^2 for `gravity`
  |     |
  |     `projectile_range` declares
  |         gravity: Acceleration of F64        (m/s^2)
  |
  = note: m/s is a velocity; m/s^2 is an acceleration
  = help: did you mean `9.80665<m/s^2>`?
```

Five properties, and each is a thing the two real diagnostics above lack:

1. **The dimension is rendered as a unit symbol**, `m/s^2`, not as an exponent
   vector and not as type-level integers. `strings-formatting-and-docs.md` §3.3's
   `unit_symbol` associated function over the seven const exponents already
   exists for `Display`; the diagnostic uses the same renderer, so the compiler
   and the program print dimensions identically.
2. **The error is at the argument**, not inside the callee's arithmetic. The
   caller's own token is underlined.
3. **The literal that produced the wrong dimension is shown**, on its own line,
   which is possible only because the unit was in the literal — a library's
   `9.80665 * u.m / u.s` spans four tokens and three of them are the library's.
4. **The parameter's declared type is quoted from the signature**, so the fix is
   in the message rather than in the documentation.
5. **The `help` is a rewrite of the user's own text.** This is the property
   `llm-ergonomics.md` cares about most: a diagnostic that names the replacement
   token is a diagnostic a model can act on, and Science's only teaching channel
   is diagnostics.

Properties 1, 3 and 5 are **available only because the unit is in the literal and
the dimension has a printable name**. They are not available to a library in this
language or any other. That is the answer to the question, stated as a rendered
message rather than as a claim.

`SC0256` is **not allocated by this note** — `unit-literals.md` §10 owns
`SC0252`–`SC0256` and `SC0256` is its dimension-mismatch code. This section asks
that note to adopt the rendering above; §11 records the ask.

### 7.3 Mars Climate Orbiter was not a dimension error, and two subsections of `scientific-libraries.md` §12 disagree about it

`README.md`'s convention is *say what you contradict*. This one is a
contradiction *inside* a sibling, between two of its own subsections, and it is
load-bearing enough to name.

**§12.2** motivates the whole units decision with:

> Mars Climate Orbiter is the canonical loss, and it is a *type error* that a
> type system declined to have an opinion about.

**§12.6** then excludes unit *systems* from the type:

> A `Length` is a length; whether it is displayed in metres or feet is a
> formatting concern […] mixing metres and feet is already impossible because
> both are `Length` and conversion is explicit at the boundary.

The Mars Climate Orbiter loss was: the ground software produced total impulse in
**pound-force seconds** and the flight navigation software consumed it as
**newton-seconds**. Both are impulse. Both have dimension M·L·T⁻¹. **A dimensional
checker passes the program.** §12.2's motivating example is not caught by §12.6's
design, and the note says both things without noticing.

This does not overturn either subsection, and the resolution is a sharpening
rather than a reversal:

> **Decision 13. Three different errors are being conflated, they are caught by
> three different mechanisms, and only two of the three are caught at all.**
>
> | Error | Example | Caught by | When |
> |---|---|---|---|
> | **Dimension** | an acceleration where a velocity belongs | the type, `SC0256` | compile time |
> | **Conversion** | feet where metres belong, within one dimension | the *literal*, by exact folding at construction — `unit-literals.md` §5.2 | compile time, and it is caught by never existing: `3.5<ft>` is already metres in the value |
> | **Boundary** | a bare number crossing a file, a C ABI, a socket or a `print` and being re-interpreted | **nothing** | never |
>
> **Mars Climate Orbiter was the third kind.** The number crossed an interface
> between two programs written by two organisations, as a number in a file. No
> type in either program was wrong. Units in the type would not have caught it,
> and neither would `pint`, `uom` or F#.
>
> **§12.2 should be amended** to say that units-in-the-type catch the first kind,
> that the literal catches the second, and that the third is a serialisation
> problem. Keeping the Mars example is fine — it is the right motivation for
> *caring* — provided the note does not claim to catch it. §11 files this as an
> ask on `scientific-libraries.md` §12.2 rather than editing that note here.
>
> **Cost of naming this.** The units pitch gets less dramatic. That is the correct
> cost, and a pitch that survives contact with its own canonical example is worth
> more than one that does not.

### 7.4 Where units must be re-attached: the boundary nobody owns

Decision 13's third row is a hole, and it is worth saying who could close it,
because four notes are adjacent to it and none owns it.

Every place a `Quantity` becomes a bare number:

| Boundary | Note that owns it | What exists today |
|---|---|---|
| `magnitude()` | this note, §6.1 | An explicit method, deliberately named, deliberately not a field |
| A file — CSV, npy, Parquet | `data-io.md` | Nothing. A `Frame` column has a `dtype` and no unit |
| The C ABI | `ffi-c-boundary.md` | Nothing, and nothing can: C has no units. `extern` takes `F64` |
| A published table or figure | `publishable-output.md` | The typed table exists and a unit column is natural |
| `print` and `f"…"` | `strings-formatting-and-docs.md` §3.3 | `unit_symbol` exists, so printing carries the unit already |

Two of the five are closed and three are open. The one with the best
cost-to-value ratio is the second: **a column unit in `data-io.md`'s `Frame`**, so
that `read_csv of Measurement(…)` can attach `<m/s^2>` from a header or a schema
and a round-trip through a file does not lose the dimension. That is the
mechanism that would have caught Mars Climate Orbiter, it is a library feature
rather than a language one, and it is cheap because `Frame` already carries
per-column type information.

This note asks for it (§11) and does not design it. The reason for raising it
here rather than leaving it to `data-io.md` is that **the value of units in the
type is bounded by how far a quantity can travel without losing them**, and
today that distance is one program.

---

## 8. Signatures generic over `Real`

### 8.1 Decision 14 — `Real` is the default bound, and `Float` is the exception

`uncertainty.md` §6.2 proposes a `Real` interface —

```science
interface Real:
    function from_exact(value: F64) -> Self
    function nominal(borrowed self) -> F64

    function sqrt(borrowed self) -> Self
    function exp(borrowed self) -> Self
    function ln(borrowed self) -> Self
    function pow(borrowed self, exponent: borrowed Self) -> Self
    function sin(borrowed self) -> Self
    function cos(borrowed self) -> Self
    function atan2(borrowed self, other: borrowed Self) -> Self
    function abs(borrowed self) -> Self
```

— and observes in §6.5 that the edit to the catalogue *"was happening anyway"*,
because `scientific-libraries.md` §14.4 already asks for a `Float` interface for
the same signatures, so **the cost attributable to uncertainty is the choice of
which word goes in the bound.**

> **Decision 14. This note makes that choice for mathematics and physics: the
> bound is `Real`. `Float` appears only where the function needs a bit pattern or
> a width.**
>
> The functions that keep `Float`, and they are the entire list:
>
> ```science
> function ulp(x: F64) -> F64
> function next_after of T(x: T, towards: T) -> T where T: Float
> function to_bits of T(x: T) -> T::Bits where T: Float
> function from_bits of T(bits: T::Bits) -> T where T: Float
> function classify of T(x: T) -> FloatClass where T: Float
> function is_normal of T(x: T) -> Bool where T: Float
> function total_order of T(a: T, b: T) -> Ordering where T: Float
> ```
>
> Seven functions, all of them tier 1b, all of them in `math` rather than the
> prelude (§4.4), and every one of them meaningless for an uncertain quantity:
> the ULP of a measurement is not a question.
>
> **Rejected: `Float` as the default, with `Real` added later.**
> `uncertainty.md` §6.5 prices this exactly — *"making that choice after the
> catalogue is written costs a sweep of several hundred signatures and a
> compatibility break for anyone who wrote against `Float`."* This note is the
> catalogue in question, so it is the note that pays or avoids that cost.
>
> **Rejected: both, with `Real: Float`.** It makes `Uncertain of F64` implement
> `Float`, which requires it to have a ULP and a bit pattern. It does not.
>
> **Cost.** Every signature in §5 and §6 carries `where T: Real`, which is
> eleven characters of ceremony on several hundred lines, and a reader must know
> what `Real` is before reading any of them. Against that: one definition covers
> `F32`, `F64`, `Uncertain of F32`, `Uncertain of F64` and — when F2 lands —
> forward-mode dual numbers, with no second implementation and no `#[derive]`.

### 8.2 Three consequences for the catalogue

**Special functions become differentiable for free** (§5.2). A `gamma` written
over `Real`'s primitives propagates derivatives through `Uncertain` by the chain
rule with nobody writing a derivative rule. This is the payoff
`scientific-libraries.md` §2 predicted when it chose to *write* the special
functions rather than link them, and `Real` is what collects it.

**`complex_step_derivative` becomes available** (§5.3). It needs `f` generic over
its argument type; a signature written `function(F64) -> F64` cannot be evaluated
at a complex argument, and one written `function of T(T) -> T where T: Real`
can — once `Complex of F64` implements `Real`, which is the one extension this
note asks of `uncertainty.md`'s interface (§11).

**Physics signatures compose with uncertainty with no edit at all.** Every §6
signature is `Quantity of (T, …)` with `T: Real`, and `uncertainty.md` Decision 5
puts the dimension outside and the uncertainty inside, so
`Length of (Uncertain of F64)` is what a measured length already is. §6.6's
example — an electron mass with a CODATA uncertainty producing an energy with the
right number of digits — required no code in this note beyond choosing the bound.

### 8.3 `pure function`, and what the bound does not cover

`effects.md` asks that `pure function` be a declaration the compiler checks, and
its §3.6 uses `pure function erf(x: F64) -> F64` as its own example. Almost every
signature in §5 and §6 is pure in that sense, and declaring it buys the guarantee
the note names: *"a `pure function erf` cannot quietly acquire a call to a logging
facility."*

Three groups in this note are **not** pure and should not be declared so:

- Anything taking a `Key` — `is_probable_prime` (§5.6), `monte_carlo_integrate`
  (§5.3), the stochastic ODE methods (§5.4). A key is consumed, which is a move,
  and `stdlib-standard.md` §5.3's whole design is that this is visible.
- Anything with a budget or a wall-clock stopping rule. `factorise(n, budget)`
  is pure if the budget counts operations and not seconds; the catalogue
  specifies operations, precisely so that it stays pure and reproducible.
  `reproducibility.md` §3.2's G3 is why.
- Anything that reads `codata_vintage()` or a configuration. Nothing in §5 or §6
  does, and the entry exists so that a future addition does not do it silently.

That is a small list, and its smallness is the point: a mathematics and physics
library is the purest code in a scientific program, which is why
`effects.md`'s declaration is cheap here and why this note is one of its easiest
customers.

---

## 9. The summary table

Every group in the note, with its tier and the test it passed or failed.

| Group | Tier | Decided by |
|---|---|---|
| `sqrt` `fma` `abs` `copysign` `min` `max` `floor` `ceil` `trunc` `round` `round_ties_even` | **1** | §3.2 — an instruction on every target, exactly specified by IEEE-754 |
| Integer bits: `count_ones` `leading_zeros` `trailing_zeros` `reverse_bytes` `rotate_*` `wrapping_*` `checked_*` `saturating_*` `mul_high` | **1** | §3.3 |
| `sign` `fract` `clamp` `is_nan` `is_infinite` `is_finite` `is_normal` `classify` `next_after` `ulp` `to_bits` `from_bits` | **1b** | §3.4 — bit patterns, not instructions, and exactly specified |
| `exp` `ln` `log2` `log10` `pow` `cbrt` `hypot` and all trigonometry and hyperbolics | **2** | §3.5a — library code that happens to be fast |
| `exp2` `exp10` `expm1` `ln1p` `asinh` `acosh` `atanh` `div_euclid` `MIN_POSITIVE` | **2**, new | §4.2 rules P1 and P2 |
| `fma` `round_ties_even` `copysign` in the prelude | **2**, new | §4.2 rule P3 |
| Reciprocal square root | **not in the catalogue** | §3.5b — a per-target approximation |
| `sin_cos` `log_base` `lerp` `next_after` `ulp` `is_normal` `total_order` | **3** | §4.4 |
| §5.2 special functions, in full | **3** | `stdlib-core.md` §1.2 Q1 and Q5 |
| §5.3 differentiation, quadrature, series acceleration | **3** | Q4 — the method is an argument |
| §5.4 ODEs and PDEs | **3**, PDEs F1 | Q4, and `scientific-libraries.md` §4 |
| §5.5 interpolation, polynomials, bases | **3** | Q4, and §1.3's second gate for `Polynomial` |
| §5.6 root finding, number theory, combinatorics | **3** | Q4, and Q1/Q2 for primality and factorisation |
| §5.7 `Complex`, and the transforms | **3**, transforms F1 | §1.3 for `Complex`; the FFT family is linked C |
| §6.1 `physics.constants` | **3** | Decision 9 — CODATA is revised, and the names are single letters |
| §6.2–§6.9 all physics | **3** | §4.5 — no instruction, no name, and no type until const-expression arithmetic lands |

Counts, so the shape is visible: **21 names in tier 1 and 1b**, about **55 in
tier 2**, and roughly **1,400 in tier 3**. The ratio is the note's conclusion in
one line — the intrinsic surface is two percent of the catalogue, the
import-free surface is four percent, and calling the rest "built in" would be a
claim about a package manager rather than about a language.

---

## 10. Sequencing

Against `scientific-libraries.md` §13's waves, which this note does not
renumber.

| Wave | What this note adds | Gated on |
|---|---|---|
| **0, before the type checker** | Nothing. But §3's tier 1 list is a *codegen* commitment and should be written into the compiler's intrinsic table early, because `reproducibility.md` Decision 3's float policy and §3.6's exactness claim are the same table read twice | Nothing |
| **1** | The prelude extensions of §4.3 — eleven float methods and the integer bit set. All F0, all scalar, all method-position, and `fma` is the one that unblocks `reproducibility.md` Decision 3's escape hatch | Associated constants in a `has:` block (`stdlib-core.md` §11) |
| **1** | §5.1 elementary in full; §5.2 special functions; §5.6 number theory and combinatorics; §5.7 `Complex` | Nothing beyond the above |
| **2** | §5.3 calculus, §5.4 ODEs, §5.5 interpolation and polynomials, §5.6 root finding | Closure types as a spelling (§11) |
| **2** | `physics.constants` with units and uncertainties, per Decision 9 and `uncertainty.md` §8.3 | The unit literal, staged per `unit-literals.md` §9.2 |
| **3** | §6.2 mechanics, §6.3 thermodynamics, §6.4 EM, §6.5 optics — the scalar parts | `Quantity`, and therefore const-expression arithmetic |
| **4** | §5.4 PDEs, §5.7 transforms, §6.6 quantum, §6.8 fluids | F1 tensors |
| **5** | §6.7 relativity's curvature half, §6.9 nuclear and particle | F1, and Decision 12's distinct-types-over-one-dimension pattern |

Two observations about the order.

**§5.1 and §5.2 are wave 1 and are the whole foundation**, and neither needs
anything that does not exist. A `math` with elementary functions, special
functions, number theory and `Complex` is a usable library on its own and is
available before units, before tensors and before const expressions. That is
worth knowing because it means the catalogue's most-used third is not blocked
behind its largest ask.

**Nothing in §6 ships before `Quantity`, and that is a deliberate refusal of
`scientific-libraries.md` §13's wave 2 compromise** for physics specifically.
That note ships `chem` early with *"quantities as `F64` with a documented unit"*
and argues the retrofit is a type change with no call-site changes. For `chem`
that is right — stoichiometry is integer and string work with a thin numeric
layer. For `physics` it is wrong: every signature in §6 is *only* a type, the
body is one expression, and a `physics` module with bare `F64` everywhere is a
formula reference rather than a library. Shipping it early would teach users to
write bare numbers and would make the retrofit a source change after all. Named
as a disagreement with that note's wave 2 as applied to §6, with the reason.

---

## 11. What this note asks of the others

Ordered by how much is blocked behind each. Nothing here is new to the corpus
except items 6, 7 and 8; the rest are existing asks with one more asker, which is
the information worth adding.

1. **Const-expression arithmetic in type position.** The standing cross-note ask,
   already carried by `scientific-libraries.md` §12.4, `broadcasting.md` §11 and
   `unit-literals.md`, and designed by `const-expression-arithmetic.md`. **All of
   §6 is blocked behind it** and so are §5.4's PDEs and §5.7's transforms. This
   note is the fourth asker and the largest consumer by line count.

2. **Closure types spelled `function(T) -> U`.** `ffi-c-boundary.md` §10.1 raised
   it, `scientific-libraries.md` §14.2 and `broadcasting.md` seconded it. It
   appears in `integrate`, `derivative`, `root`, `solve_ode`, `solve_bvp`,
   `laplace_invert`, `lagrangian_from_energies` and `euler_lagrange_residual` —
   eight signatures in this note alone. Fourth asker.

3. **Explicit type arguments at call sites.** `data-io.md` §11.2,
   `scientific-libraries.md` §14.3 and `const-expression-arithmetic.md` §7.3 ask
   for it; the last calls it load-bearing. This note needs it for
   `Interpolator.fit of F64(…)` and for every const parameter that appears only
   in a return type. Fourth asker.

4. **Associated constants in a `has:` block.** `stdlib-core.md` §11 already asks,
   for `F64.PI`. §4.3 adds `MIN_POSITIVE` and §6.1's constants module wants the
   same mechanism at module scope. Second asker, same feature.

5. **Of `uncertainty.md`: adopt `Real` as the bound name**, which §8.1 decides
   for this catalogue and which that note's §6.5 explicitly leaves to be chosen
   once. Also: **`Complex of T` should implement `Real`** where `T: Real`, which
   §8.2 needs for `complex_step_derivative` and which nothing in that note
   forbids. That is an addition to its §6.2, not a change.

6. **Of `unit-literals.md`: adopt §7.2's rendering for `SC0256`.** That note owns
   `SC0252`–`SC0256`; this note takes no code and asks only that the
   dimension-mismatch message render the dimension through
   `strings-formatting-and-docs.md` §3.3's `unit_symbol`, underline the literal
   that produced it, quote the parameter's declared type, and offer the corrected
   literal as a `help`. §7.2 shows the target output. This is the single most
   valuable thing in the note that the note cannot do itself.

7. **Of `scientific-libraries.md`:**
   - **§12.2's Mars Climate Orbiter claim should be amended**, per Decision 13:
     units in the type catch dimension errors, the literal catches conversion
     errors, and MCO was neither. The example is still the right motivation and
     the claim is wrong as written.
   - **§5.3, §5.4, §5.5, §5.7, §8 and §9.1's bare-eponym free functions become
     `choice` variants**, per Decision 1 and the table in §2.5. Six subsections.
   - **§12.1 gains `physics.optics`, `physics.fluids` and `physics.nuclear`**,
     per §6.5, §6.8 and §6.9.
   - **§13's wave 2 should not include `physics` with unchecked units**, per §10.
   - **§14.4's `Float` ask is superseded by `Real`**, per Decision 14. The
     request is unchanged; the word changes.

8. **Of `native-dependencies.md`: a warning on `fma` without hardware FMA**, per
   Decision 7. That note owns `SC0471`–`SC0479` and has free codes; this note
   allocates none. The condition is target-feature-level knowledge that note's
   probe already has. Also: **the target microarchitecture level should join
   `package-manager.md` §2.3's `[build]` table and `reproducibility.md` §5.1's
   provenance record**, beside the float policy `reproducibility.md` Decision 3
   already put there.

9. **Of `reproducibility.md`: §3.2's Decision 2 can drop `sqrt`** from the list
   of functions `random` re-implements, per §3.6 — `sqrt` is correctly rounded by
   hardware on every target and a software version would be slower and no more
   exact. `erfinv`, `ln` and `exp` remain.

10. **Of `stdlib-core.md`: §8.3's accuracy contract should be stated per tier**,
    per §3.6. The sentence as written is true of `sin` and false of `sqrt`. This
    is a partition of the existing contract, not a change to it, and it gives
    `reproducibility.md` a named set of functions a frozen contract may call.

11. **Of `reserved-words.md`: nothing new.** §2.4 records that mathematics and
    physics collide with no keyword at all, and that `mod` in §5.6 is a fifth
    voice for that note's §2.1 recommendation. The audit's conclusion is
    confirmed by the two domains it did not sample.

12. **Of `data-io.md`: a unit on a `Frame` column.** §7.4's argument: the value
    of units in the type is bounded by how far a quantity travels without losing
    them, and today that distance is one program. This is the mechanism that
    would have caught Mars Climate Orbiter. Filed as an ask, not a design.

---

## 12. Risks

**The tier 1 list is a codegen promise and this note cannot keep it.** §3.2 and
§3.6 assert that eleven operations lower to instructions and are bit-identical
across targets. Both are true of LLVM as it is; both are properties of a backend
this note does not own. If `sciencec` ever targets a platform where `llvm.minimum`
lowers to a libcall — WebAssembly without the relevant proposal, or a soft-float
embedded target — the table is wrong on that target and the exactness claim of
§3.6 survives while the cost claim does not. **Mitigation:** the per-target column
in §3.2 is the mechanism; it must be maintained as targets are added, and a target
whose column cannot be filled in is a target where `sciencec` should say so at
build time rather than in a design note.

**Decision 1 costs the catalogue its searchability, and this is the biggest
practical risk in the note.** A user who knows they want Brent's method searches
the reference for `brent` and finds a `choice` variant rather than a function. A
model trained on SciPy emits `brentq(f, a, b)` and gets a resolution error. The
rule is right and the transition cost is real. **Mitigation:** the migration
diagnostic pattern `llm-ergonomics.md` §4 designed for exactly this — an
unresolved name that matches a known variant reports *"`brent` is
`RootMethod.Brent`; write `root(f, bracket, method: RootMethod.Brent)`"*. That is
a resolution diagnostic, `effects.md` and `script-mode.md` own parts of that
range, and this note takes no code. Without such a diagnostic, Decision 1 is a
worse experience than the thing it replaces for the first year.

**The prelude extensions are eleven ABI commitments taken on an argument about
pairs.** Rules P1 and P2 are defensible and they are not laws of nature. If they
turn out to admit too much, the removal path is the one
`stdlib-shape-and-packages.md` §1.4 describes and it is not free: a Level 1
method is in `science-rt`'s ABI. **Mitigation:** eleven is small, every one of
them is an operation with a fixed IEEE-754 or textbook definition, and none is a
type. The blast radius of being wrong is a deprecated method, not a redesign.

**Decision 11 is a real limitation sold as a design.** Analytical mechanics —
the single richest part of classical physics and the one the request named —
gets the *least* from the unit system, because generalised coordinates are
dimensionally heterogeneous and a seven-exponent `Quantity` cannot hold a mixed
vector. §6.6 and §6.7 reach the same place. A reader could reasonably conclude
that the units feature is strongest exactly where physics is simplest.
**Mitigation:** it is stated three times and generalised once (§7.1), which is
the most this note can do; the alternative — heterogeneous quantity vectors —
was considered and is a much larger type-system feature than units.

**Decision 12 doubles some types and the boundary is a judgement.** `Torque` and
`Energy` as distinct types over N·m is clearly right; `Frequency` and
`AngularVelocity` over s⁻¹ is arguable, and there will be a third case somebody
feels strongly about in each direction. **Mitigation:** the decision names its
instances explicitly rather than giving a rule, so disagreement is about a list
and not about a principle.

**The catalogue is roughly 1,400 names and §15 of `scientific-libraries.md`
already flags what that means.** This note adds PDEs, optics, fluids, nuclear and
particle to an inventory that note calls *"several hundred person-months"*. The
antidote is the same: §10's sequencing must travel with the catalogue, and the
tier column is a second antidote — a reader who sees that ninety-four percent of
the names are tier 3 understands that this is a package roadmap and not a
language surface.

**§7's claim is smaller than the pitch people will hear.** "Units in the type"
sounds like it prevents the famous disasters, and §7.3 shows that the most famous
one is not prevented. If the project ever markets the feature with the Mars
example, somebody will check, and the correction will land harder than the honest
claim would have. **Mitigation:** Decision 13's three-row table is short enough to
be the marketing, and "catches dimension errors at compile time, catches
conversion errors by never letting them exist, and does not cross a file
boundary" is a true sentence that fits on a slide.

---

## 13. Open questions

1. **Is `min`/`max` `llvm.minimum` or `llvm.minnum`?** §3.2 assumes IEEE-754-2019
   `minimum` — NaN propagates, −0 orders below +0 — because it is the version
   that is total and exactly specified, and because a scientific user who gets a
   number back from `min(x, NAN)` has had a missing value silently swallowed.
   The cost is a compare-and-select sequence on x86 rather than a single
   instruction. `minnum` is faster on AArch64 and is what C's `fmin` does. The
   decision here is for `minimum`; it is a genuine trade and somebody may want
   the other.

2. **Does `Complex of T` implement `Real`?** §8.2 needs it for
   `complex_step_derivative` and §11 asks for it, but `Real` requires `Ord`
   through its operator supertraits and the complex numbers are not ordered.
   Either `Real` splits into an ordered and an unordered half, or
   `complex_step_derivative` takes a separate bound. Not decided here because the
   interface is `uncertainty.md`'s.

3. **Should the tier of a function be visible in its documentation
   automatically?** `strings-formatting-and-docs.md`'s `##` doc comments are
   hand-written, and every tier 1 comment in this note says "Tier 1" by hand. A
   compiler that knows which names it lowers to intrinsics could emit the tier
   into the generated reference, which would keep the cost statement true as the
   backend changes. Cheap, and outside this note's territory.

4. **Where do `physics.optics`'s Jones and Mueller types live once `linalg`
   exists?** A Jones matrix is a 2×2 complex matrix and a Mueller matrix is 4×4
   real, so both are `linalg` types with a physics meaning. Decision 12's pattern
   says they should be distinct types rather than aliases; whether that is worth
   it for two fixed small shapes is not obvious.

5. **Is there a case for a tier between 2 and 3 — a `use math` that imports the
   whole module in one line?** `scientific-libraries.md` writes
   `use math (integrate)` with an explicit item list. A scientist writing a
   throwaway script wants `use math` bare. `script-mode.md` is the note where
   that question actually lives, and it interacts with this one: the prelude
   boundary matters much less if a whole-module import is one short line.
