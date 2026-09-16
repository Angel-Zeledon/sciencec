# Science — Design: chemistry and biology intrinsics, prelude and modules

Date: 2026-09-16
Status: draft for review
Owns: the tiering of every chemistry and biology name — compiler intrinsic,
prelude, or imported — and the data-versus-code split that chemistry and biology
force and that no other domain in this project forces as hard.
Deepens, and does not repeat: `scientific-libraries.md` §10 (`chem`) and §11
(`bio`). Those sections catalogue; this one tiers, packages, and prices.
Applies: `stdlib-core.md` §1 (the Level 1 criterion), `unit-literals.md` (the
literal and the unit namespace), `reproducibility.md` §5 (the provenance
record), `const-expression-arithmetic.md` §2.1 (the five-operator grammar),
`reserved-words.md` (the audit), `syntax-revision-2.md` (the syntax every
signature here is written in).
Claims no diagnostic code. See §13.4.

---

## 0. What this note is

`scientific-libraries.md` §10 and §11 answered *what functions exist*. They did
not answer three questions that turn out to decide how the modules ship:

1. **Which of these names are in scope in an empty file?** The catalogue is a
   flat list with no levels on it. `stdlib-core.md` §1 supplies a criterion and
   nobody has run it over chemistry or biology.
2. **Which of these names are functions at all?** A periodic table is not an
   algorithm. A genetic code is not an algorithm. A substitution matrix is not an
   algorithm. They are *data with a release number*, and data with a release
   number is a different packaging problem from code — `package-manager.md` §9
   item 8 already discovered this for tzdata and did not notice it had found a
   general case.
3. **What do the units actually buy here?** `scientific-libraries.md` §12 decided
   units belong in the type. It proved the point on `kinetic_energy`. Chemistry
   is where the decision either earns its cost or does not, because a chemist
   writes six dimensions in a line and a physicist writes two.

This note answers those three, with a long catalogue underneath so the answers
are not hypothetical.

### 0.1 What it is not

It is not a schedule. `scientific-libraries.md` §13 sequences these modules into
waves 2 and 6 and that sequencing stands unamended. Nothing here is a request to
start building; §12 lists what should **never** be built, which is the more
useful half of a catalogue this size.

It is also not a second copy of §10 and §11. Where a name appears in both, this
note adds a tier, a marker and — where it differs — a reason. Where this note
proposes a name §10 or §11 does not have, it says so. Where it *renames*
something they have, §13 records the ask rather than the rename being silent.

### 0.2 The sibling, and what this note inherits from it

`intrinsics-math-physics.md` owns the tier definitions and the naming rule, and
this note follows both rather than setting a second version of either.

- **The tiers** are that note's §1.1 and §1.2, adopted unchanged. §1.1 below
  restates them only so this note is readable on its own, and adds one
  subdivision of tier 3 that chemistry and biology force and mathematics does
  not (§1.2).
- **The naming rule** is that note's **Decision 1 — N1, N2, N3, N4** — adopted in
  full. Its §2.6 anticipates this note by name and lists the cases it expected;
  §1.4 below applies the rule and §1.5 records every name in this catalogue that
  the rule changes, including three where it changes a name
  `scientific-libraries.md` §10 or §11 already published.

Two places where this note **adds** to the sibling rather than restating it, and
both are things mathematics and physics did not need:

1. **The data/code axis** (§1.3 and §4). The sibling's tier 3 is undifferentiated
   because almost nothing in mathematics has a release number. Chemistry and
   biology are mostly tables, and a table is a different packaging problem from a
   function.
2. **Five spelling conventions** (§1.4, S1–S5) covering case, abbreviation and
   dialect. They are orthogonal to N1–N4, they are read off what
   `scientific-libraries.md` §10 and §11 already do, and if the sibling later
   states its own they should be reconciled in its favour.

One place where this note **disagrees** with the sibling, and says so rather than
diverging quietly: §1.5's last paragraph, on N2 applied to sequence alignment.

---

## 1. The tiers, and the naming rule

**A note on numbering.** This note's decisions are numbered by the section they
live in — Decision 4.3 is in §4.3. `intrinsics-math-physics.md` numbers its
decisions flat, 1 through 13. Where this note writes *the sibling's Decision 9*
it means that note's, and where it writes *Decision 9.1* it means its own.

### 1.1 The three tiers

> **Decision 1.1. `intrinsics-math-physics.md` §1.1 and §1.2 are adopted
> unchanged.** Every name in this catalogue is in exactly one of three tiers.
>
> - **Tier 1 — compiler intrinsic.** `sciencec` emits a machine instruction, or
>   an LLVM intrinsic guaranteed to become one on the target. No call, no stack
>   frame, no library symbol. The sibling's §1.2 gives the four-question hardware
>   test and §2.1 below applies it.
> - **Tier 2 — prelude.** Ordinary Science code in `science-rt`, in scope with no
>   `use` line, in every file, forever. Level 1 in `stdlib-core.md`'s vocabulary.
>   A tier 2 name is an ABI commitment and a permanent global name.
> - **Tier 3 — imported.** Reached through `use chem.kinetics`, `use bio.align`.
>   Absent from a binary that does not name it, per
>   `stdlib-shape-and-packages.md` §2 item 1.

The sibling's warning applies here too and is worth repeating because this note's
answer makes it easy to trip over: **tiers 1 and 2 are not alternatives.** A tier
1 operation is also in the prelude — `x.sqrt()` is written the same way whether it
lowers to `sqrtsd` or to a call. *Tier 1 is a statement about cost, tier 2 is a
statement about scope.* So §2's finding (tier 1 empty) and §3's finding (tier 2 is
five units) are two independent facts, not one fact said twice.

The three are not a quality ranking either. They are three different *promises*:
tier 1 promises a lowering, tier 2 promises permanence, tier 3 promises nothing
except that the module exists and is versioned. Most good code belongs in tier 3
and that is not a demotion.

**Tier 3 subdivides once**, and the subdivision is §4's whole subject:

| | Ships with | Versioned by |
|---|---|---|
| **3a — code module** | the toolchain (Level 2) or a package (Level 3) | the toolchain version or the package version |
| **3b — data package** | `package-manager.md`'s content-addressed archives | the *table's own* release, independently of any code |

A function in 3a whose answer depends on a table in 3b inherits 3b's versioning.
That inheritance is the single most important structural fact about chemistry and
biology as libraries, and §4 is about it.

### 1.2 The default tier, stated once instead of six hundred times

> **Decision 1.2.** **Every name in §§7–10 is tier 3 unless it is marked.** Tier 1
> and tier 2 are enumerated *exhaustively* in §2.4 and §3.4 — those two lists are
> short and complete, so the default is unambiguous and every name does have a
> tier.

**Rejected: annotating all ~640 names individually.** It would triple the
catalogue's length to carry one bit whose value is the same for 635 of them, and
it would hide the finding rather than show it. The finding *is* that tier 1 is
empty and tier 2 is five units: a reader should be able to see that in two short
tables, not reconstruct it from a wall of annotations.

**Cost.** A reader skimming §10.4 sees no tier marker and must know the default.
Mitigated by repeating the default in one line at the head of each of §7 and §9.

### 1.3 The second axis: the markers

Tier is one axis. It is not the interesting one for chemistry and biology, because
the answer is nearly constant. These markers carry the rest:

| Marker | Meaning |
|---|---|
| `†` | **Data-backed.** The answer depends on a versioned table. Tier 3b is in its dependency closure and §4's rules apply to it. |
| `‡` | **Linked.** Implemented by binding an existing C or Fortran library through `ffi-c-boundary.md`, not by writing the algorithm. |
| `◆` | **Prelude** (tier 2). Enumerated in §3.4. |
| `▲` | **Compiler intrinsic** (tier 1). Never used in this note. §2 says why. |
| `✗` | **Refused.** Named so that the refusal is visible; §12 gives the reason. |
| `~` | **Variant-bearing.** The method is part of the observable and must appear in the name or in an argument, per `stdlib-core.md` §1.2 Q4. |

`†` and `~` do most of the work, and they are close cousins: a `†` name's answer
moves when a table is reissued, a `~` name's answer moves when the caller picks a
different method. Both are reasons a name cannot be tier 2 and both are reasons a
result must record what it used.

### 1.4 The naming rule — N1–N4 adopted, S1–S5 added

> **Decision 1.4. `intrinsics-math-physics.md`'s Decision 1 — N1, N2, N3, N4 — is
> adopted unchanged and governs every name in this catalogue. This note adds five
> spelling conventions, S1–S5, which are orthogonal to it.**

Restated in one line each so this note is readable alone; the sibling's §2.2 is
the authority and carries the rejected alternatives:

- **N1.** No function is named by a bare eponym. Every function name contains the
  operation it performs.
- **N2.** Where an eponym selects among *interchangeable* methods for one
  operation — same inputs, same output type, differing in cost and convergence —
  the operation is the function and the method is an `UpperCamelCase` variant
  passed as an argument.
- **N3.** Where an eponym names a distinct *object* — a different model, a
  different transform, a different statistic — the eponym is part of the name, as
  `eponym_noun` in `snake_case`.
- **N4.** An eponym is a type only when the thing it names has state and a
  lifetime.

The five additions, which the sibling does not state and which §10 and §11 of
`scientific-libraries.md` already follow in practice:

> - **S1 — case.** Functions and modules are `snake_case`. Types are
>   `UpperCamelCase`. Constants are `SCREAMING_SNAKE_CASE`. Unit names inside
>   `< >` are the discipline's own symbol, case-sensitive, and are governed by
>   `unit-literals.md` §3.3, not by this rule — which is also what protects
>   `Dalton`, `Gray`, `Sievert`, `Curie` and `Angstrom` from N1, exactly as the
>   sibling's §2.3 protects `Newton`.
> - **S2 — the name says what it returns.** `melting_temperature`, not
>   `tm_nearest_neighbour`. Where the method is itself observable the name also
>   carries it, which is N3 rather than an exception to S2.
> - **S3 — British spelling**, because §10 and §11 already chose it:
>   `ionisation_energy`, `normalise_counts`, `centre_of_mass`, `colligative`,
>   `minimiser`, `ladderise`. Consistency with the existing catalogue beats
>   preference.
> - **S4 — no abbreviation unless the abbreviation is the discipline's primary
>   written form.** `gc_content`, `ph`, `rmsd`, `tpm`, `pdb`, `nmr` pass. `mw`,
>   `conc`, `temp`, `seq` do not — they are `molecular_weight`, `concentration`,
>   `temperature`, `sequence`.
> - **S5 — a name that reads a table says nothing about the table.**
>   `atomic_mass(element)`, not `atomic_mass_iupac_2021(element)`. The release is
>   a property of the *package*, not of the call (§4.3), and putting it in the
>   name would make every table reissue a source change in every caller.

**S4 has one ugly consequence that should be admitted rather than smoothed over.**
`scientific-libraries.md` §10.5 writes `p_ka` and `p_kb` beside `ph` and `poh`.
Four names, two conventions, and `p_ka` is neither the discipline's spelling
(`pKa`) nor a clean transliteration. This note keeps `p_ka` — changing it is a
rename with no gain, and S1 forbids the capital — but records it as the worst
name in the catalogue and the price of holding S1 and S4 together.

### 1.5 What N1–N4 renames in this catalogue

The sibling's §2.6 predicted the cases. It was right about all of them, and the
bill is larger than it estimated because chemistry and biology are the two most
eponym-dense disciplines in the corpus. The full list, so the rename is one edit
and not an argument:

**N3 — eponym plus the operation noun.** About sixty names. The pattern is
mechanical and the representative ones are:

| `scientific-libraries.md` §10/§11, and drafts of this note | This catalogue |
|---|---|
| `arrhenius` | `arrhenius_rate` |
| `eyring` | `eyring_rate` |
| `nernst` | `nernst_potential` |
| `henderson_hasselbalch` | `henderson_hasselbalch_ph` |
| `michaelis_menten` | `michaelis_menten_rate` |
| `van_t_hoff` | `van_t_hoff_constant` |
| `clausius_clapeyron` | split into `clausius_clapeyron_slope` (dP/dT) and `clausius_clapeyron_pressure` (integrated) |
| `kirchhoff` | `kirchhoff_enthalpy` |
| `raoult` | `raoult_pressure` |
| `antoine` | `antoine_pressure` |
| `butler_volmer` | `butler_volmer_current` |
| `cottrell`, `randles_sevcik`, `koutecky_levich` | `…_current` |
| `kohlrausch` | `kohlrausch_conductivity` |
| `beer_lambert` | dropped — `absorbance` is the operation and already exists |
| `born_haber` | `born_haber_lattice_energy` |
| `lineweaver_burk`, `hanes_woolf`, `eadie_hofstee` | `…_transform` |
| `jukes_cantor`, `kimura_two_parameter`, `tamura_nei`, … | `…_distance` |
| `robinson_foulds` | `robinson_foulds_distance` |
| `nei_gojobori`, `yang_nielsen` | `…_dn_ds` |
| `mcdonald_kreitman`, `ewens_watterson`, `hudson_kreitman_aguade` | `…_test` |
| `bray_curtis`, `morisita_horn`, `canberra`, `aitchison`, `unifrac` | `…_distance` |
| `sorensen`, `berger_parker` | `…_index` |
| `chao1`, `chao2`, `ace`, `ice` | `…_richness` |
| `kaplan_meier` | `kaplan_meier_curve` |
| `nelson_aalen` | `nelson_aalen_hazard` |
| `aalen_johansen` | `aalen_johansen_incidence` |
| `gtr`, `hky85`, `tn93`, `jc69`, `k80`, `f81` | `…_matrix` |
| `goldman_yang`, `muse_gaut` | `…_matrix` |
| `kjeldahl`, `karl_fischer` | `kjeldahl_nitrogen`, `karl_fischer_water` |
| `russell_saunders` | `russell_saunders_term` |
| `woodward_fieser` | `woodward_fieser_maximum` |
| `benesi_hildebrand` | `benesi_hildebrand_constant` |
| `debye_waller`, `lorentz_polarisation` | `…_factor` |

**Kept as they are, and why.** `tajima_d`, `fu_li_d`, `fu_li_f`, `fay_wu_h`,
`zeng_e`, `fu_fs`, `jost_d`, `watterson_theta`. In each the letter or Greek
symbol **is** the statistic's name — Tajima's *D* is the object, not an
abbreviation of one — so the name already contains its noun and satisfies N3. The
same reading covers `f_st`, `g_st`, `f_is` and `f_it`.

**N2 — the method becomes an argument.** Four families, and these are the changes
that touch signatures rather than only names:

| Was | Is |
|---|---|
| `smith_waterman`, `needleman_wunsch`, `gotoh`, `hirschberg`, `align_banded` | `align_local` / `align_global` with `method: AlignMethod.…` |
| `neighbour_joining`, `bionj`, `upgma`, `wpgma`, `fitch_margoliash`, `minimum_evolution` | `tree_from_distances(…, method: TreeMethod.…)` |
| `louvain`, `leiden`, `label_propagation`, `girvan_newman` | `communities(…, method: CommunityMethod.…)` |
| `umap_like`, `tsne_like`, `diffusion_map` | `embedding(…, method: EmbeddingMethod.…)` |
| `gibbs_sampler`, `expectation_maximisation` (motif discovery) | `discover_motifs(…, method: MotifSearch.…)` |

**Two of these overwrite signatures `scientific-libraries.md` already published**
— §11.7's `smith_waterman of T(…)` and §11.7's `neighbour_joining of (const N:
Int)(…)`. That note is not edited here; §13 records the ask, and it is the same
ask the sibling's §2.5 already makes for `rk4` and `brent`.

**The one place this note disagrees with N2, stated rather than diverged from.**
N2's own test is *"same inputs, same output type, differing only in cost and
convergence"*. `align_local` and `align_global` pass that test against each
other's *algorithms* — Gotoh and Hirschberg compute the identical optimal
alignment with different memory — and that is a clean N2 case, adopted above.
But **Smith–Waterman and Needleman–Wunsch are not two methods; they are local and
global alignment, which are two questions.** Collapsing them into
`align(a, b, method: …)` would put a semantic distinction in a parameter named
`method`, which is the thing N2's own rejected-alternative paragraph warns
against for `bessel_j` and `bessel_y`.

So this catalogue keeps **two operations** — `align_local` and `align_global` —
each fully N1-compliant because neither contains an eponym at all, and puts the
*algorithm* in the `method:` argument underneath. That is N2 applied one level
lower than the sibling's §2.6 assumed, and it is a refinement of that note's
example rather than a rejection of its rule. If the sibling disagrees, its
version wins and the cost is two names.

---

## 2. Tier 1 is empty

### 2.1 The test an intrinsic must pass

`intrinsics-math-physics.md` §1.2 states the test as four questions: is there a
named LLVM intrinsic; does it lower to a **bounded, small, fixed** instruction
sequence on every target; is the result **exactly specified** by IEEE-754 or by
two's complement; and is the cost honest on the worst target. Its §3.1 adds the
trap that does the real work — *an LLVM intrinsic is not an instruction*, because
`llvm.sin` becomes a libcall.

Chemistry and biology never reach question 3. **Nothing in either domain gets past
question 1**, because no ISA has an instruction whose semantics is a chemical or
biological operation. So this section applies the sibling's test and then states
two corollaries that the sibling did not need and that these two domains run
straight into:

- **An intrinsic must not pin an algorithm.** The moment the compiler knows *how*
  to compute something that has more than one reasonable how, the how has left the
  library and cannot be replaced without a compiler release. That is
  `stdlib-core.md` §1.2 Q3 and Q5, applied one level lower than that note applies
  them.
- **An intrinsic must not carry a domain name.** If the instruction is general,
  the intrinsic must be general, or the next domain that wants the same
  instruction has to get its own spelling of it.

### 2.2 The five candidates, each rejected

These are the only serious ones. Each is rejected for a stated reason rather than
waved off.

**(a) `hamming_distance` over packed nucleotides.** The hot loop in every
short-read tool is: pack two bases per byte, `xor`, mask, `popcnt`, accumulate.
On AVX-512 it is `VPOPCNTDQ` and it is genuinely one instruction per 64 lanes.

Rejected. The instruction is `popcount`, which is general, already belongs in the
mathematics note's tier 1, and is wanted by set intersection, Bloom filters,
sparse matrices and bit-set statistics. An intrinsic named `hamming_distance`
would be a domain name on a general instruction, failing the second corollary.
`bio.seq.hamming_distance` is a tier 3 function that *calls* the tier 1
`popcount`, which is the correct layering and costs nothing.

**(b) Striped Smith-Waterman.** Farrar's algorithm is
`saturating_sub_u8x16` + `max_u8x16` + a lane shift, and the SIMD version is 6–10×
the scalar one. It is the single most-executed inner loop in bioinformatics.

Rejected, twice over. The primitives are SIMD intrinsics and general (§2.2(a)'s
argument). And the *algorithm* is a `~` name: striped, diagonal, anti-diagonal and
wavefront layouts all exist, the best one depends on the query length and the
alphabet, and a better one will be published. Pinning it in the compiler is
exactly the Q3/Q5 failure `stdlib-core.md` §1.5 catalogues for `base64`.

**(c) A 2-bit packed sequence representation the compiler understands.** If
`DnaSequence` were a language type, `seq[i]` could lower to a shift-and-mask with
no bounds arithmetic and no library call.

Rejected, and this is the one worth redirecting rather than refusing. What is
actually wanted is **bit-packed arrays as a general feature** — `Array of U2`,
or a `PackedArray of (T, const BITS: Int)` — which serves DNA, quantised model
weights (`models-and-inference.md`), bitsets, and colour channels. That is a
request to `indexing-and-array-literals.md`, not a chemistry or biology intrinsic,
and §13 makes it as one. `DnaSequence` stays a library type over whatever that
note decides.

**(d) `exp`, `ln`, `log10`, `pow`.** Arrhenius is `exp`, Nernst is `log`, pH is
`log10`, Boltzmann factors are `exp`, the Hill equation is `pow`, every
likelihood in phylogenetics is `ln`. Chemistry and biology are, numerically,
mostly transcendental function calls.

Rejected as *chemistry* intrinsics, because they are already `math`'s and they are
tier 1 *there*. This is worth recording, not skipping: it is the reason chemistry
looks like it should have intrinsics and does not. The transcendental content of
chemistry is real and it is entirely borrowed.

**(e) A domain accelerator.** DRAGEN's FPGA does alignment; Parabricks does
variant calling on a GPU; there are ASICs for MD force evaluation.

Rejected. None of these is an instruction in a general-purpose ISA. They are
devices, they are reached through a vendor library, and `ffi-c-boundary.md` and
`native-dependencies.md` already own how a device-backed library is found, linked
and recorded. A tier 1 name for a device would be a compiler that fails to build
on machines without the device.

### 2.3 The two honest near-misses

Neither becomes tier 1, and both are close enough that not naming them would be a
gap.

**(i) The unit literal is already compile-time, and chemistry uses it hardest.**
`1.5<mol/L>` has no function call in it at all: `unit-literals.md` §5 folds the
scale at compile time and the dimension is a type. In the sense that matters for
performance, chemistry's most common operation — attaching units to a number — is
*already* free and already the compiler's own work.

But the intrinsic there is the **literal form**, which `unit-literals.md` owns,
and the chemistry content of it — `Da`, `M`, `eq`, `L` — is a `unit` declaration
written in Science per that note's §3.4. The compiler knows no unit. So chemistry
touches compile-time machinery without owning any of it, which is the right
outcome and not a tier 1 entry.

**(ii) Since 2019, some of chemistry's constants are exact.** This is the sharpest
thing in §2 and it is genuinely new to this project's notes.

The 2019 SI redefinition fixed seven constants *by definition and with no
uncertainty*. `intrinsics-math-physics.md` §4.5 tabulates all seven; the five
that chemistry uses are:

| Constant | Defined value |
|---|---|
| `AVOGADRO` | 6.02214076 × 10²³ mol⁻¹ |
| `BOLTZMANN` | 1.380649 × 10⁻²³ J·K⁻¹ |
| `ELEMENTARY_CHARGE` | 1.602176634 × 10⁻¹⁹ C |
| `PLANCK` | 6.62607015 × 10⁻³⁴ J·s |
| `SPEED_OF_LIGHT` | 299 792 458 m·s⁻¹ |

and therefore, as exact products of exact numbers:

| Derived | Exact value |
|---|---|
| `GAS_CONSTANT` = N_A · k_B | 8.31446261815324 J·mol⁻¹·K⁻¹ |
| `FARADAY` = N_A · e | 96485.33212331001 C·mol⁻¹ |
| `STEFAN_BOLTZMANN` = 2π⁵k⁴/(15h³c²) | exact up to π, i.e. exact to any precision |

These pass `stdlib-core.md` §1.2 Q1 — *pinned, fixed by something outside this
language, nobody is going to revise it* — **for the first time in the history of
the quantities**. Before 2019 the gas constant was a measured number with a
CODATA release attached; now it is a terminating decimal.

They are still **not tier 1**, and not tier 2 either, for three reasons that
stack. The first is this section's: a constant is not an instruction. It is a
value the compiler can fold, which is what `const-expression-arithmetic.md` and
the literal folding of `unit-literals.md` §5 already do for any literal. The
other two are `intrinsics-math-physics.md`'s **Decision 9**, which this note
seconds without qualification: the *names* are single letters a user will bind in
their first ten lines, and a user who imports `SPEED_OF_LIGHT` from one place and
`ELECTRON_MASS` from another because one is exact and one is not has been handed a
distinction that helps nobody at the call site.

So `GAS_CONSTANT` is a tier 3 name in `physics.constants` that happens to be
exactly representable, and §4.7 records that it is one of only three things in
this entire catalogue that is not `†`. The sibling's Decision 9 adds the part
this note needs: **the module carries a CODATA vintage in its version**, which
makes `physics.constants` a data package in §4.2's sense and brings it under
§4.4's run record. That is the seventh table in §4.1's list and it arrived from
the other note.

**Cost of getting this wrong in the other direction.** If someone promotes the
exact constants to tier 2 on the strength of Q1, they will want to promote the
*measured* ones beside them for symmetry — `ELECTRON_MASS`, `ATOMIC_MASS_UNIT`,
`RYDBERG` — and those are `†`. A prelude holding half a CODATA release is worse
than a prelude holding none of it.

### 2.4 Decision

> **Decision 2.4. Tier 1 is empty for chemistry and biology. There is no
> compiler intrinsic in either domain, there is no candidate for one, and the
> list is expected to stay empty permanently.**
>
> The complete tier 1 inventory for `chem` and `bio`:
>
> | Name | — |
> |---|---|
> | *(none)* | |

**Why saying so is worth a section.** A catalogue that quietly has no tier 1
entries reads as an oversight; a catalogue that says the tier is empty and gives
the test it is empty *against* tells the next person not to go looking. It also
sets the precedent for every future domain note: the question "does geology get
an intrinsic?" now has a procedure rather than a taste.

**And the positive form of the finding matters more than the negative one.**
Chemistry and biology are, computationally, `popcount`, `exp`, SIMD `max`, a
dense matrix multiply and a sort. Every one of those is general, every one is
already owned, and the domains' *entire* contribution to the performance story is
choosing which of them to call and in what order. That is a library's job. It is
the strongest available argument that these modules should never be in the
compiler at all, and it should be quoted the first time someone proposes a
`#[chem]` attribute.

**The one thing tier 1 does owe these domains** is the general primitives, and
§13 asks for them by name: bit-packed arrays (§2.2(c)), `popcount` and the SIMD
saturating integer operations, and const-argument *inference* (§5.2 depends on
it). None is a chemistry feature and all three have chemistry or biology as their
most demanding consumer.

---

## 3. Tier 2 is five units and nothing else

### 3.1 What chemistry and biology already get from the prelude, without asking

`unit-literals.md` §3.3 puts in scope with no `use`: the seven SI base units, the
22 coherent derived units with special names, and the prefix table.

Read that list as a chemist and a biologist rather than as a physicist, and it is
not neutral. Five of the units in it exist for these two domains and for nobody
else:

| Unit | Dimension | Whose |
|---|---|---|
| `mol` | amount | chemistry's base dimension, and the reason `g/mol` and `mol/L` type-check at all |
| `kat` | mol·s⁻¹ | the katal — **enzyme activity**, biochemistry's own SI unit |
| `Bq` | s⁻¹ | becquerel — radiochemistry |
| `Gy` | J·kg⁻¹ | gray — absorbed dose |
| `Sv` | J·kg⁻¹ | sievert — equivalent dose, radiobiology |

`Gy` and `Sv` have the same dimension and different meanings, which is a hole in
the seven-exponent representation and not a new one — `physics` has the identical
problem with torque and energy. It is noted, not fixed; fixing it means tagged
dimensions and `scientific-libraries.md` §12.6 already declined that complexity
for unit *systems* for the same reason.

So the answer to "what is in the prelude for chemistry and biology" is: **the
mole, the katal, and three units of radiation dose, and every one of them is
there because the CGPM put it there and not because this note asked.** That is a
genuinely small footprint and a good one.

### 3.2 The litre problem, which is real and worth one decision

`mol` is prelude. `L` is not — the litre is an SI-*accepted* non-SI unit (SI
Brochure Table 8), not one of the 22 coherent derived units, so
`unit-literals.md` §3.3's rule leaves it out.

The consequence:

```science
let c be 1.5<mol/m^3>      # works in an empty file
let c be 1.5<mol/L>        # unknown unit `L` — needs a `use`
```

Every chemistry concentration in the world is written `mol/L` or `M`. A language
in which the coherent SI spelling works in an empty file and the one chemists
actually write does not is a language that will be described as "unit-obsessed"
on its first day, and the description will be fair.

> **Decision 3.2. `L` (and its alias `l`, and therefore `mL`, `µL`, `dL` through
> the prefix rule) joins the prelude. It is the only addition this note asks for,
> and it is asked of `unit-literals.md` §3.3 rather than taken here.**

The argument for it, in the terms §3.3 of that note uses: the prelude exists so
that `9.8<m/s^2>` needs no import, and the litre is to chemistry exactly what
`m/s^2` is to mechanics. SI Table 8 units are a bounded, stable, tiny set — the
minute, hour, day, degree, hectare, litre, tonne, dalton, electronvolt and
astronomical unit — and `unit-literals.md` will face the same question for `h`,
`d`, `°`, `t` and `eV` the moment anyone writes a half-life or a band gap.

**Rejected: put the whole of SI Table 8 in the prelude.** It is defensible and it
is a bigger decision than this note should take alone. Two of its entries are
`†` — the dalton and the electronvolt have measured conversion factors (§3.3
below) — and a prelude containing measured data is precisely what §2.3(ii)
argues against. The litre's factor is exactly 10⁻³ m³ by definition, so it
carries no such problem.

**Rejected: make `chem` re-export the litre.** It works and it means `use chem`
before a concentration literal, which is the same friction one `use` further
down. It also creates the possibility of two modules declaring `L`, which
`unit-literals.md` §3.3 makes an error requiring qualification — `<si.L>` — and
that is a worse first experience than the one being fixed.

**Cost.** One more name in a namespace that is global and permanent, and `l` as
an alias is a genuine legibility hazard next to `1` in several fonts. The
mitigation is that `sciencec fmt` normalises `l` to `L`, which is the same
mechanism §3.2 of `unit-literals.md` already uses for `·` and `μ`.

### 3.3 Two unit declarations that are themselves data

A finding that `unit-literals.md` does not currently carry, and should.

That note's §3.4 declaration form is `unit NAME is scalar? unit-expr`. It assumes
the scalar is a number the author writes. For two units chemistry and biology
need constantly, **the scalar is a measured constant with a CODATA uncertainty**:

| Unit | Definition | Scalar |
|---|---|---|
| `Da` (dalton, u) | 1/12 the mass of an unbound ¹²C atom at rest in its ground state | 1.66053906892(52) × 10⁻²⁷ kg — **measured** |
| `eV` | the work to move one elementary charge through one volt | 1.602176634 × 10⁻¹⁹ J — **exact since 2019**, because `e` is |

So `Da` is a `†` unit: the conversion factor from daltons to kilograms has a
release and an uncertainty, and it moves. `eV` is not, for the same reason the
gas constant is not (§2.3(ii)).

This matters because the dalton is *the* mass unit of both mass spectrometry and
structural biology. `write_xyz` writes ångströms and daltons; a protein's mass is
quoted in daltons; every MS spectrum's x-axis is m/z in daltons per elementary
charge.

> **Decision 3.3. A `unit` declaration whose scalar is a measured constant is
> `†`, and the module declaring it is in the dependency closure of the table the
> scalar comes from. `chem.units` declares `Da`, and `chem.units` is therefore a
> `†` module.**

**Cost, and it is not small.** It means a unit conversion can change between two
releases of a data package. `1<Da>` in kilograms is not bit-identical across a
CODATA revision. `reproducibility.md` §2's named-channel claim survives — the
channel is named and recorded (§4.4) — but "the unit system is pinned by the
lockfile" is a sentence somebody will be surprised by, and it is better said here
than discovered.

**Rejected: freeze `Da` at one value forever and call it exact.** It is what most
software does and it is wrong in a way that shows up in high-resolution mass
spectrometry, where the fifth decimal place of an exact mass is the point.
Chemists who care about that number care about which release it came from.

### 3.4 Decision — the exhaustive tier 2 list

> **Decision 3.4. The complete chemistry and biology contribution to the prelude
> is five unit names that SI already put there, plus `L` if §3.2 is granted. No
> function, no type, no constant from either domain is in the prelude. Not
> `Element`, not `Formula`, not `DnaSequence`, not `molar_mass`, not
> `reverse_complement`, not `AVOGADRO`.**
>
> | Name | Tier 2 by |
> |---|---|
> | `mol` ◆ | SI base unit; `unit-literals.md` §3.3 |
> | `kat` ◆ | SI coherent derived unit |
> | `Bq` ◆ | SI coherent derived unit |
> | `Gy` ◆ | SI coherent derived unit |
> | `Sv` ◆ | SI coherent derived unit |
> | `L` ◆ | **asked for** in §3.2 |

**Why nothing else, when some candidates pass §1.2's five questions.** Several do
pass. `reverse_complement` is pinned — A↔T, C↔G, and the IUPAC ambiguity codes
have not moved since 1985 — has no adversary, no performance product anybody
chooses, no variants, and no fix that must outrun the compiler. It is *eligible*.

It fails `stdlib-core.md` §1.3's **second gate**, and this is the gate that
decides the whole of §3:

> eligible by §1.2, and **written by enough programs to earn a permanent global
> name**.

A tier 2 name is in scope in every Science program ever written, including the
ones that compute payroll. `reverse_complement`, `gc_content`, `molar_mass` and
`ph` are written by a large and important minority and by nobody else. One `use
bio.seq` is the entire cost of excluding them, and the benefit is that the global
namespace does not acquire six hundred names from two domains that a given
program has a 95% chance of not being about.

**Rejected: a `use science.prelude.chem` opt-in prelude**, a "batteries you can
switch on". It is what Rust's `std::prelude::v1` and Python's `from x import *`
both amount to and it has the defect both have: the reader of a file cannot tell
where a name came from without knowing the prelude's contents, and the prelude's
contents are a version-dependent list. `stdlib-shape-and-packages.md` §1's
batteries argument is about *modules being present*, not about names being
ambient, and this note reads it that way.

**Cost, stated plainly.** A chemist's first program has three `use` lines before
it has one statement:

```science
use chem.formula
use chem.elements
use chem.units

function main():
    let m be formula.molar_mass(formula.parse("C6H12O6"))
    print(f"{m}")
```

Three lines is more than Python's `from rdkit import Chem` and less than the four
imports the same program needs in Julia. It is the right trade and it is a real
one.

---

## 4. Data versus code

This is the section the axis was worth adding for.

### 4.1 The tables, named

Seven things in this catalogue are **not code**. They are tables with a release
number, and every function that reads one inherits the release.

| Table | Issued by | Cadence | What moves |
|---|---|---|---|
| **Standard atomic weights** | IUPAC CIAAW | ~2 years | Values *and* their form: since 2009 fourteen elements have an **interval** `[a, b]`, not a number, because their isotopic composition varies by source. Sulfur is `[32.059, 32.076]`. |
| **Atomic masses and isotopic abundances** | AME (Wang *et al.*) / CIAAW | ~3 years | Every nuclide's mass to 10⁻¹¹ relative, and the abundances that turn masses into weights. |
| **CODATA fundamental constants** | CODATA TGFC | ~4 years | Everything not fixed by the 2019 SI (§2.3(ii)). `ELECTRON_MASS`, `RYDBERG`, `BOHR_RADIUS`, `FINE_STRUCTURE`, and the dalton's scalar. |
| **Thermochemical data** | NIST-JANAF, NIST WebBook, CODATA key values | irregular, per-species | ΔfH°, S°, Shomate coefficients. Species are added and revised individually. |
| **Genetic codes** | NCBI | irregular, still growing | Tables 1–33 with gaps; table 33 was added in 2018. Which codons are starts is revised more often than which are stops. |
| **Substitution matrices** | NCBI / the original authors | effectively frozen, but *forked* | BLOSUM62 as shipped by NCBI was computed from a miscounted clustering and the error stood for fifteen years (Styczynski *et al.*, 2008). The corrected matrix performs **worse**. Both are in use. |
| **Space groups** | *International Tables for Crystallography* | stable, but re-typeset | The 230 groups and their generators, Hall and Hermann–Mauguin symbols, and the settings — of which there are more than 230 because a group has several. |

Two more that are data but are *not* shipped, for reasons in §12: gene ontology
and pathway databases (GO, KEGG, Reactome — licensing, and monthly releases), and
molecular-mechanics force-field parameters (AMBER, CHARMM, OPLS — licensing, and
the parameters are a research artefact).

### 4.2 Decision — data is a package, not a module

> **Decision 4.2. Every table in §4.1 ships as a **data package** in
> `package-manager.md`'s sense: a content-addressed archive, resolved by minimal
> version selection, pinned in `science.lock`, containing no Science code. The
> code module that reads it declares it as an ordinary dependency.**

```toml
# science.toml
[dependencies]
chem = "1.0"
"chem.data.ciaaw" = "2021.0"      # standard atomic weights, the 2021 release
"bio.data.ncbi-codes" = "2018.1"  # genetic code tables
```

This is not a new mechanism. `package-manager.md` §9 item 8 already established
it, for tzdata, and called tzdata *"the clearest case in the library of a
dependency that is data rather than code"*. It is the clearest case **because it
was the only one anybody had found**. Chemistry and biology supply six more, and
the finding generalises: **the mechanism the time zone database needed is a
general mechanism for scientific data, and it should be documented as one rather
than as a special case for time.**

**Rejected: vendor the tables inside the code module.** Simplest, and it couples
two release cadences that have nothing to do with each other. A bug fix in
`chem.formula`'s parser would then require a new release carrying an atomic-weight
table nobody asked to change, and an atomic-weight reissue would require a code
release. Worse, it makes the *code* module's version the only thing recorded, so a
result cannot say which table it used without the reader knowing the module's
changelog.

**Rejected: download the table at first use, cached.** This is what several
bioinformatics tools do and it is the reason their results are not reproducible.
It makes the table a *runtime* input reached over the network, which
`effects.md`'s `external` bit would mark and `reproducibility.md` §4.2's
`--deterministic` would reject outright. The rejection would be correct and the
feature would then be unusable in the mode this project is built around.

**Rejected: a table as a first-class language concept** — `table` as a
declaration form, with the compiler doing the versioning. It is a real design and
it is a whole feature; the package manager already does content addressing,
already writes the lockfile, and already reports into the provenance record. A
second mechanism would have to do all three again.

**Cost.** A user who wants a molar mass now has two dependencies where they
expected one, and the second one has a version number they have no opinion about.
Mitigated by §4.3.

### 4.3 Decision — a default, pinned by the lockfile and never by the machine

> **Decision 4.3. Each `†` module exposes a module-level default bound to a named
> release, and the free-function form reads it. The default is selected by the
> *package version in the lockfile*, so it is identical on every machine, in
> every run, forever, for a given lock. There is no ambient, settable, global
> table.**

```science
use chem.elements

# The free function. Reads `elements.DEFAULT`, whose release is
# whatever `science.lock` pinned. Identical everywhere.
let m be elements.atomic_mass(elements.by_symbol("S"))

# The explicit form, for a paper that must name the release in its methods.
let table be elements.release(elements.CIAAW_2021)
let m2 be table.atomic_mass(table.by_symbol("S"))
```

Both forms exist and they are not redundant: the first is what makes a one-line
script bearable, the second is what makes a methods section citable. They have
the same answer whenever the lock names `CIAAW_2021`.

**Rejected: a mutable global the program sets at startup** —
`elements.set_table(...)`. It is what `pint`, `Biopython` and most of the field do.
It makes `atomic_mass` a function whose answer depends on program-wide mutable
state, which is an `ambient` effect under `effects.md`, which `--deterministic`
must then either reject or record. Rejecting it would break every such program;
recording it would mean the provenance record carries a *sequence of assignments*
rather than a fact. Neither is acceptable and the feature buys only the
convenience §4.3's explicit form already gives.

**Rejected: no default at all — the table is always an argument.** Purest, and it
is the right answer for two cases specifically (§4.5), but as a blanket rule it
makes `molar_mass(f)` into `molar_mass(f, table)` in a hundred signatures and
makes the one-line script three lines. The lockfile pinning recovers the
determinism that the argument was there to guarantee.

**Cost.** Updating a data package silently changes results. That is the same
property a code dependency has, and it is why `package-manager.md` exists; it is
worth one sentence in the data package's documentation and one field in the run
record, which is §4.4.

### 4.4 Decision — the run record names every table actually read

> **Decision 4.4. The run record of `reproducibility.md` §5.2 gains a `tables`
> array. An entry is written for each `†` table that was **actually read** during
> the run, never for one that was merely linked, mirroring exactly how
> `allowed_used` distinguishes a permitted allowance from an exercised one.**

```json
"run": {
  "tables": [
    { "name": "ciaaw-standard-atomic-weights", "release": "2021",
      "sha256": "…", "entries_read": 4 },
    { "name": "ncbi-genetic-codes", "release": "2018.1",
      "sha256": "…", "table_id": 11 }
  ]
}
```

Three properties, each deliberate:

- **Read, not linked.** A program that depends on `chem.data.ciaaw` and computes
  no molar mass gets no entry. `reproducibility.md` §5.2 argues for exactly this
  distinction in one line — *"an allowance that was permitted and never exercised
  is different from one that was"* — and this is the second consumer of that
  argument, which is the evidence it was the right shape.
- **`table_id` where a table has sub-selections.** Translating with NCBI table 11
  and translating with table 1 are different results from the same package. The
  reviewer needs the 11.
- **No floats.** `reproducibility.md` §5.1 forbids them in the record and this
  addition does not smuggle any in: the values read are not recorded, only which
  table and how much of it.

The payoff is the sentence a methods section can now write, and it is the sharpest
thing this note produces:

> Masses from CIAAW 2021; translation with NCBI table 11; alignment with
> NCBI BLOSUM62. Provenance `pv:9f3a1c04e7b2`.

and a reviewer who runs `sciencec provenance --id pv:9f3a1c04e7b2 ./a.out` gets
yes or no on all three at once.

**Cost.** Every `†` function must touch a recorder on first read. That is one
atomic flag per table per process, set once, and it is the same shape as the
`allowed_used` bookkeeping `reproducibility.md` already requires. It is not free
and it is close to it.

### 4.5 Decision — two tables have no default, deliberately

> **Decision 4.5. The genetic code and the substitution matrix are **required
> arguments** with no default. There is no `translate(sequence)` and no
> `align_local(a, b)` — both take the table explicitly.**

`scientific-libraries.md` §11.7 already wrote `translate` this way. This note is
saying *why*, and extending it to matrices.

**The genetic code.** Translating mitochondrial or *Mycoplasma* DNA with the
standard code does not fail. It produces a protein sequence of the right length,
composed of real amino acids, that is wrong — because `TGA` is a stop in table 1
and tryptophan in tables 2, 3, 4, 5 and 9. The output is silently, plausibly,
publishably wrong, which is the worst failure mode a function can have. A default
converts "the user picked the wrong table" into "the user did not know there was a
table", and only the first of those is recoverable.

**The substitution matrix.** §4.1's BLOSUM62 story is the argument in full: *the
name does not identify the matrix*. NCBI's BLOSUM62 and the corrected BLOSUM62 are
different matrices, both are in use, and the miscomputed one is the one that
almost every published alignment used and the one that benchmarks better. A
default would have to pick one, silently, and whichever it picked would be wrong
for half the literature.

> **Corollary. A matrix is named by its provenance, not by its number:
> `blosum.ncbi_62`, `blosum.corrected_62`, `pam.dayhoff_250`. There is no
> `blosum(62)`.**

**Cost.** Two characters and a decision at every call site, and a user who does
not know which genetic code they want is stopped rather than served. That is the
intent. The mitigation is a diagnostic-quality error message on the *package*
side — `bio.code.STANDARD` exists and is spelled `STANDARD`, not `DEFAULT`, so a
user who genuinely wants table 1 says so in one word and a reader can see they
said it.

### 4.6 The criterion, extended — where the *data* is observable

`stdlib-core.md` §1.1 sharpens the core criterion to:

> **Level 1 specifies observable behaviour. Where the algorithm is itself
> observable, the module is not Level 1.**

Run the five questions over `atomic_mass`:

| | | |
|---|---|---|
| Q1 **Pinned?** | **No** | IUPAC revises on a two-year cadence, and for fourteen elements the published value is an *interval*, so there is not even one number to pin. |
| Q2 **Adversary?** | No | Nobody attacks a periodic table. |
| Q3 **Performance product?** | No | It is an array lookup. |
| Q4 **Variants?** | **Yes** | Conventional atomic weight, the abridged four-digit form, the interval, and the sample-specific value are four different answers to "the atomic mass of sulfur". |
| Q5 **Fix outruns the compiler?** | **Yes** | A CIAAW reissue must reach users without a `sciencec` release. This is the whole of §4.2. |

Three failures out of five, and the interesting part is *which* three. There is
no algorithm here at all. `atomic_mass` is a lookup; it has no clever
implementation, no adversarial input, and no faster version. It fails the
criterion entirely on the strength of the **table**.

> **Decision 4.6. `stdlib-core.md` §1.1's sharpening should be stated as two
> halves rather than one:**
>
> > **Level 1 specifies observable behaviour. Where the algorithm is itself
> > observable, the module is not Level 1 — and where the *data* is itself
> > observable, neither is it, for the same reason and with the same force.**

This is offered to `stdlib-core.md` as an amendment (§13), not taken unilaterally.
The argument for it is that the existing sentence is about *how* a function
computes, and a whole class of scientific functions do not compute at all: they
report. `atomic_mass`, `codon_to_amino_acid`, `space_group_generators` and
`substitution_score` are all lookups, all fail the criterion, and none of them
fails it for a reason the current sentence names. Chemistry and biology are where
that gap becomes visible, because they are the domains that are mostly tables.

**And the general form, which is the useful output of §4:**

> **A function whose answer changes when a table is reissued cannot be in a core
> whose observable behaviour is fixed — not "not yet", but never.** No amount of
> maturity moves it. The table will be reissued again.

### 4.7 The three things that are not `†`

For completeness, and because a list of exceptions is how a reader checks a rule
is being applied rather than asserted. Everything in §§7–10 is `†` except:

1. **The 2019-exact constants** (§2.3(ii)) — `AVOGADRO`, `BOLTZMANN`,
   `ELEMENTARY_CHARGE`, `PLANCK`, `SPEED_OF_LIGHT`, and the exact products
   `GAS_CONSTANT`, `FARADAY`. Pinned by definition.
2. **Complementary base pairing** — A↔T/U, C↔G, and the IUPAC ambiguity codes.
   Pinned by chemistry and by a 1985 nomenclature that will not move. So
   `complement` and `reverse_complement` are not `†`, which is why they are the
   most tempting tier 2 candidates in the whole catalogue (§3.4).
3. **The twenty canonical amino acids and their one- and three-letter codes.**
   Pinned by IUPAC-IUB 1968/1983. Selenocysteine (`U`) and pyrrolysine (`O`) are
   *additions* to the alphabet, not revisions of it, and adding a letter is a
   package version bump with no existing answer changing.

Everything else — every mass, every energy, every score, every codon assignment,
every space group setting — is `†`.

---

## 5. Where units earn their place

`scientific-libraries.md` §12.2 decided units go in the type and proved it on
`kinetic_energy`, where the check is that you did not confuse a mass with a
velocity. That is a real check and it is a weak example, because nobody confuses
a mass with a velocity.

Chemistry is where the decision is actually tested, because chemistry's errors are
*dimensional errors between quantities that look alike*: a molarity and a
molality, a molar enthalpy and an enthalpy, a first-order and a second-order rate
constant. Those are mistakes that competent people make and that no amount of
care prevents, and they are mistakes a seven-exponent vector catches for free.

### 5.1 The chemistry aliases, and a seam with §10.7

First, a discrepancy that has to be resolved before a signature can be written.

`scientific-libraries.md` §12.3 defines `Quantity` with **eight** type parameters —
a value type and seven exponents — and gives aliases of the form
`type Mass of T is Quantity of (T, 0, 1, 0, 0, 0, 0, 0)`.

`scientific-libraries.md` §10.7 then writes `Quantity of (F64, GramsPerMole)` and
`Quantity of (F64, MolesPerLitre)` — a **two**-parameter form, with a named unit
in the second slot. That form is not defined anywhere and is not compatible with
§12.3's.

> **Decision 5.1. §12.3's eight-parameter form is authoritative. §10.7's
> `Quantity of (F64, GramsPerMole)` spelling is drift from an earlier draft and
> every signature in this note uses aliases over the eight-parameter form.** §13
> asks `scientific-libraries.md` to correct §10.7's five signatures.

The aliases chemistry and biology need, on top of §12.3's nine:

```science
type Amount of T             is Quantity of (T,  0,  0,  0,  0,  0,  1, 0)  # mol
type MolarMass of T          is Quantity of (T,  0,  1,  0,  0,  0, -1, 0)  # kg/mol
type Concentration of T      is Quantity of (T, -3,  0,  0,  0,  0,  1, 0)  # mol/m^3
type Molality of T           is Quantity of (T,  0, -1,  0,  0,  0,  1, 0)  # mol/kg
type MolarVolume of T        is Quantity of (T,  3,  0,  0,  0,  0, -1, 0)  # m^3/mol
type MolarEnergy of T        is Quantity of (T,  2,  1, -2,  0,  0, -1, 0)  # J/mol
type MolarEntropy of T       is Quantity of (T,  2,  1, -2,  0, -1, -1, 0)  # J/(mol K)
type MolarHeatCapacity of T  is Quantity of (T,  2,  1, -2,  0, -1, -1, 0)  # J/(mol K)
type CatalyticActivity of T  is Quantity of (T,  0,  0, -1,  0,  0,  1, 0)  # kat
type Frequency of T          is Quantity of (T,  0,  0, -1,  0,  0,  0, 0)  # s^-1
type Charge of T             is Quantity of (T,  0,  0,  1,  1,  0,  0, 0)  # C
type Potential of T          is Quantity of (T,  2,  1, -3, -1,  0,  0, 0)  # V
type MolarConductivity of T  is Quantity of (T,  0, -1,  3,  2,  0, -1, 0)  # S m^2/mol
```

`MolarEnergy` and `MolarEntropy` differ only in the temperature exponent, which is
exactly the pair a chemist mixes up when reading ΔG = ΔH − TΔS off a table where
one column is in kJ/mol and the next is in J/(mol·K). The type catches it.

`MolarHeatCapacity` and `MolarEntropy` are the *same* dimension and are two names
for the reader, not two types for the checker. That is the same limitation as
`Gy`/`Sv` in §3.1 and it is accepted for the same reason.

### 5.2 The rate constant carries the reaction order — the best thing units do here

For a reaction of order *n*, rate = k[A]ⁿ, so

- rate has mol·L⁻¹·s⁻¹
- [A]ⁿ has (mol·L⁻¹)ⁿ
- therefore **k has (mol·L⁻¹)^(1−n)·s⁻¹**

| Order | Units of *k* | Exponent vector (L, M, T, I, Θ, N, J) |
|---|---|---|
| 0 | mol·L⁻¹·s⁻¹ | (−3, 0, −1, 0, 0, 1, 0) |
| 1 | s⁻¹ | (0, 0, −1, 0, 0, 0, 0) |
| 2 | L·mol⁻¹·s⁻¹ | (3, 0, −1, 0, 0, −1, 0) |
| 3 | L²·mol⁻²·s⁻¹ | (6, 0, −1, 0, 0, −2, 0) |

Read the amount column: it is **1 − n**. Read the length column: it is
**−3 + 3n**. Both are linear in the order, which means the reaction order can be a
const parameter and the dimension can be computed from it:

```science
type RateConstant of (T, const ORDER: Int) is
    Quantity of (T, -3 + 3 * ORDER, 0, -1, 0, 0, 1 - ORDER, 0)
```

**This parses.** `const-expression-arithmetic.md` §2.1's grammar admits
`IntLiteral '*' ConstTerm` and `ConstExpr '-' ConstTerm`, so `-3 + 3 * ORDER` and
`1 - ORDER` are both in the language, both normalise to `k + Σ cᵢ·aᵢ`, and
equality on them is §3.3's structural equality on the normal form.

Two things follow that are worth stating as findings rather than as consequences.

> **Finding A. This is the project's first consumer that needs an integer
> coefficient other than ±1.** Every const expression in
> `broadcasting.md`, `indexing-and-array-literals.md` and
> `scientific-libraries.md` §12.4 is a sum or difference of parameters with unit
> coefficients — `L1 + L2`, `A + B`, `N - 2`, `N / 2 + 1`. `3 * ORDER` exercises
> `SCALE` in §3.2's normalisation table, which is implemented and until now had no
> caller. It is worth a test case naming this note.

> **Finding B. The reaction order is *recoverable* from the dimension, and the
> compiler can therefore name it in a diagnostic.** Solving `1 - ORDER = -1` for
> `ORDER` gives 2. That is a linear equation in one unknown with coefficient −1 —
> the invertible case that `const-expression-arithmetic.md` §7's const-argument
> inference handles. The compiler does not need to be taught chemistry to say
> "that is a second-order rate constant"; it needs only to invert a normal form
> the library already wrote.

**Cost.** Every kinetics signature acquires a const parameter, and a caller who
does not want to think about order has to write `RateConstant of (F64, 1)` or let
inference find it. Inference does find it, from the argument's dimension, which is
the point. The genuinely annoying case is a reaction with a *fractional* order —
1.5 is common in radical chain mechanisms — and `scientific-libraries.md` §12.6
excludes rational exponents. §5.5 addresses that honestly rather than pretending
it does not happen.

### 5.3 The worked diagnostic

A real mistake, made by real people, several times a year.

```science
use chem.kinetics
use chem.units            # brings `M`, `Da` into the unit namespace

function main():
    # Fitted from a plot of 1/[A] against t — so this is second order.
    let k be 0.042<L/mol/s>

    let t_half be kinetics.half_life(k)
    print(f"half-life: {t_half}")
```

with the library declaring:

```science
function half_life(rate_constant: RateConstant of (F64, 1)) -> Time of F64
```

The compiler produces:

```
error: dimension mismatch in argument 1 of `kinetics.half_life`
  --> decay.science:9:36
   |
 6 |     let k be 0.042<L/mol/s>
   |              -------------- `k` has dimension  m^3 · mol^-1 · s^-1
   |
 9 |     let t_half be kinetics.half_life(k)
   |                                      ^ expected  s^-1
   |
   = note: `half_life` is declared
           `function half_life(rate_constant: RateConstant of (F64, 1)) -> Time of F64`
   = note: `RateConstant of (T, ORDER)` has amount exponent `1 - ORDER`.
           The argument's amount exponent is -1, so `ORDER` would have to be 2.
   = help: `k` is a second-order rate constant. `half_life` is the first-order
           half-life and is not defined for order 2 — a second-order half-life
           depends on the initial concentration:
               kinetics.half_life_second_order(k, initial)
```

**Why this is the language earning the domain rather than shipping a library.**

Nothing in the compiler knows what a reaction is. It knows that a type alias was
declared with an amount exponent of `1 - ORDER`, that the argument's amount
exponent is `-1`, and that inverting a linear form gives `ORDER = 2`. Every step
is `const-expression-arithmetic.md`'s machinery, already required for tensor
shapes, applied to a chemistry alias the library wrote in one line.

The library supplies the last line — a `help` string attached to the signature —
and the library is the right place for it, because "a second-order half-life
depends on the initial concentration" is chemistry and belongs to chemists.

**What a language without units does here.** Python with `scipy`: the program runs,
prints `16.5`, and the number is meaningless. R: the same. Julia with
`Unitful.jl`: caught, at runtime, on the line that divides — which is better than
nothing and is the comparison `scientific-libraries.md` §12.2 makes against Julia,
now with an example where it bites. The mistake has no runtime signature at all
without units: `k * c` is a perfectly good multiplication of two floats.

### 5.4 Two more the dimensions catch

**(a) Molarity where molality belongs.** Freezing-point depression is
ΔT_f = K_f · b, where **b is molality** (mol·kg⁻¹ of solvent), not molarity
(mol·L⁻¹ of solution). Using molarity is the single most common error in
undergraduate colligative-property work and it survives into real laboratory
notebooks, because at low concentration in water the two numbers are nearly equal
and the error is invisible until the solvent is not water or the concentration is
not low.

```science
let c be 0.50<mol/L>                              # Concentration
let kf be 1.86<K*kg/mol>                          # water's cryoscopic constant
let drop be solution.colligative_freezing(kf, c)  # error
```

The signature is
`function colligative_freezing(cryoscopic: Quantity of (F64, 0, -1, 0, 0, 1, -1, 0), molality: Molality of F64) -> Temperature of F64`,
and `Concentration` is mol·m⁻³ while `Molality` is mol·kg⁻¹. The mass and length
exponents disagree, so it does not compile. The library's `help` says why:
*freezing-point depression is defined over molality because the solvent's mass
does not change with temperature and its volume does.*

**(b) Molar mass added to a mass.** `molar_mass(f) + sample_mass` — kg·mol⁻¹
plus kg. Addition requires identical exponents, which is ordinary type equality
and works in F0 today with no const arithmetic at all
(`scientific-libraries.md` §12.4's opening sentence). It is the cheapest check in
the system and it catches the mistake that produces a number off by Avogadro's
number.

**(c) Amount where mass belongs, in stoichiometry.** `moles_from_mass` returns
`Amount`; `mass_from_moles` returns `Mass`. Feeding the first into a function
expecting the second is caught. This is the check that makes `limiting_reagent`
safe to write, because a limiting-reagent calculation that compares a mass to a
mole count is the classic wrong answer and has no runtime signature.

### 5.5 What the dimensions do not catch, said plainly

A units section that lists only successes is an advertisement. Five failures,
each real:

1. **Fractional reaction orders.** Order 1.5 is ordinary in radical chain
   kinetics. `scientific-libraries.md` §12.6 excludes rational exponents and
   §5.2's `ORDER` is an `Int`. So `RateConstant of (F64, 1.5)` cannot be written,
   and a fractional-order rate constant must be a bare `F64` with a documented
   unit. **This is a genuine gap and the catalogue does not pretend otherwise:**
   `chem.kinetics` provides `rate_law_fractional` taking `F64` and states in its
   documentation that it is unchecked. Halving the exponent vector would need
   rational coefficients, which `const-expression-arithmetic.md` §4.1 rejects as
   unsound against truncating integer division.
2. **Equilibrium constants.** K_c for a reaction with Δn ≠ 0 has units unless
   divided by a standard state, and every textbook quietly does the division
   without saying so. The honest type is
   `EquilibriumConstant of (T, const DELTA_N: Int)` with dimension linear in Δn —
   the same trick as §5.2 — and the honest default is that `equilibrium_constant`
   returns a dimensionless `F64` **and takes the standard state as an argument**,
   so that the division is visible. The catalogue does the latter (§8.7) and
   notes the former as available if anyone wants it.
3. **Same-dimension, different-meaning pairs.** `MolarEntropy` and
   `MolarHeatCapacity`; `Gy` and `Sv`; torque and energy. No seven-exponent
   system distinguishes these and this one does not either.
4. **Chemically wrong, dimensionally fine.** A balanced equation for the wrong
   reaction; the pKa of the wrong conjugate pair; a sign error on ΔG; the wrong
   isotope. The type system is silent on all of them and should be expected to be.
5. **Counts.** `unit-literals.md` §6.5 refuses a unit for a bare count, and
   biology's most-written "unit" is the **base pair**. `sequence.length()` is an
   `Int`, not `Quantity`, and `<bp>`, `<reads>`, `<cells>` and `<CFU>` do not
   exist. This is correct — `5<reads> + 3<cells>` type-checking would be exactly
   the false comfort §6.5 refuses to sell — and it means the unit system does
   essentially nothing for sequence bioinformatics. **That is the honest summary:
   units earn their cost in chemistry and earn nearly nothing in `bio.seq`,
   `bio.align` and `bio.phylo`.** They earn it again in `bio.epi` (rates per unit
   time) and `chem.kinetics`-adjacent enzymology (`kat`, `M`, `s⁻¹`).

### 5.6 Decision

> **Decision 5.6. Chemistry signatures are typed in `Quantity` aliases throughout,
> including in wave 2 where units are not yet *checked*. Biology signatures are
> typed in `Quantity` only where a dimension exists — rates, concentrations,
> activities, doses, times — and in plain `Int`, `F64` and sequence types
> everywhere else, with no attempt to invent dimensions for counts.**

`scientific-libraries.md` §13's wave 2 ships `chem` with quantities *unchecked*,
as `F64` with a documented unit, and retrofits the checking in wave 5. This note
adopts that and sharpens it: **the signatures should be written with the alias
names from the start, as type aliases that are `F64` in wave 2 and become real
`Quantity` in wave 5.** Then the retrofit is a change to thirteen `type … is …`
lines and nothing else — no call site moves, no signature moves, and the wave 5
diff is reviewable.

**Rejected: write `F64` in wave 2 and change the signatures in wave 5.** It is the
obvious reading of §13 and it makes wave 5 a hundred-file rename with no
mechanical check that the renames are right.

**Cost of the adopted form.** In wave 2 `MolarMass of F64` is a lie — it is an
alias for `F64` and adding it to a `Mass of F64` compiles. Documentation must say
so in the module header, and it must say when it stops being a lie.

---

## 6. The reserved-word damage, in full

`scientific-libraries.md` §3 gives eleven renames forced by the reserved list.
This section does two things with that: it states what chemistry and biology
specifically lose, and it adds **six collisions §3 does not have**, four of which
did not exist when §3 was written because `syntax-revision-2.md` created them the
same day.

### 6.1 `yield`

`yield` is the word. Not *a* word — chemistry's central quantitative noun for
the result of a reaction:

> theoretical yield · actual yield · percent yield · isolated yield ·
> quantum yield · radiochemical yield · yield strength

and in biology and biochemistry:

> ATP yield · biomass yield · crop yield · yield coefficient (Y_X/S)

`scientific-libraries.md` §10.2 is forced to write `theoretical_produced` and
`percent_produced`, and its own note says this is *"the single most-visible cost
of that reservation anywhere in the catalogue"*. It is worse than that note makes
it look, because the damage is not confined to two function names. Every one of
these is a line a chemist writes:

```science
let yield be actual / theoretical            # binding position — refused
type Reaction:
    yield: F64                               # field name — refused
let quantum_yield be emitted / absorbed      # fine, one identifier
reaction.yield                               # member — fine after the dot rule
```

So the reservation costs: the binding, the field name, and nothing else. The dot
rule of `reserved-words.md` §0.1 recovers member position, and compound
identifiers were never at risk. **That is a smaller loss than it first appears
and it is still the worst one in the catalogue**, because the binding and the
field name are the two places a reader looks to find out what a value *is*.

`reserved-words.md` §2.2 recommends freeing `yield` and gives the argument in
full: it is reserved for a generator form no phase asks for, F3 is actors and F5
is durability, and `Iterate` is how Science produces sequences. This note
seconds it without qualification and supplies the chemistry half of the evidence.

> **Decision 6.1. This catalogue uses `produced` and `efficiency` today, exactly
> as `scientific-libraries.md` §3 requires, and lists in §6.6 the eleven names
> that revert to `yield` on the day `reserved-words.md` §5 item 2 lands. The
> revert is mechanical and this note pre-commits to it so that it is one edit
> rather than a debate.**

### 6.2 The table

`K` = keyword in use. `R` = reserved, unused. `N` = new keyword introduced by
`syntax-revision-2.md`. Position is the *tightest* one the domain vocabulary
needs.

| Word | Now | Chemistry / biology meaning | Position | Today | If `reserved-words.md` §5 lands | Dot rule alone? |
|---|---|---|---|---|---|---|
| `yield` | R | reaction, quantum, ATP, crop yield | binding, field | `produced`, `efficiency` | **`yield`** | member only |
| `kernel` | R | NMR apodisation window; KDE over a chromosome; string kernels for sequence SVMs; **and the nullspace in `balance`** | binding, param | `window`, `smoother`, `nullspace` | **`kernel`** | member only |
| `union` | R | interval and feature-set union; gene-set union; conformer-set union | member, free fn | `merged` | **`union`** | **yes** for `a.union(b)` |
| `model` | R | model of evolution (GTR *is* "a model"); kinetic model; QSAR model; compartment model; `let model be sir(...)` | binding | `Fit`, `fitted`, `scheme` | **`model`** | member only |
| `shape` | R | array shape — **and molecular shape**: bent, trigonal planar, tetrahedral. `molecule.shape` is chemistry vocabulary, not array vocabulary | member, binding | `dims`, `geometry` | **`shape`** | **yes** for `m.shape` |
| `tensor` | R | NMR **shielding tensor**, quadrupole coupling tensor, polarisability tensor, inertia tensor | binding | `array`, `matrix` | **`tensor`** | member only |
| `mod` | R | ring-position arithmetic; periodicity | free fn | `remainder` | **`mod`** | **yes** |
| `match` | K | **alignment match/mismatch** — `scoring.match`, `let match be ...`, a `Match` column in a CIGAR | member, binding | `matches`, `identical` | keep — the dot rule is enough for members; binding stays broken | member only |
| `any` | K | `any(mask)` over a per-site boolean mask | free fn | `any_of` / `all_of` | keep `any_of` | member only |
| `at` | — | `spectrum.at(wavelength)`, `alignment.at(column)` | member | **`at` — already free** | already free | **yes** |
| `interface` | **N** | **protein–protein interface** — `interface_area`, `interface_residues`, `contact.interface`. Structural biology's own word for a contact surface | binding, member | `contact_surface`, `binding_surface` | not addressed by `reserved-words.md` | member only |
| `null` | **N** | **null hypothesis**, `null_distribution`, `null_model`, and population genetics' **null allele** | binding, member | `baseline`, `reference` | not addressed | member only |
| `loop` | **N** | a **loop** region of a protein — helix / sheet / **loop** / turn; a reaction-network loop; Ampère's loop | binding, member | `coil`, `loop_region` | not addressed | member only |
| `pure` | R | **pure substance**, `is_pure`, `pure_component` | binding | `pure_substance` (compound) | not addressed | member only |
| `const` | K | a `constants` module | module | `physics.constants`, `chem.constants` | keep — the long name is clearer | n/a |
| `assert` | R | a check in a test | free fn | `check` | keep reserved, per §4.2 | n/a |
| `type` | K | **`bond.type`** (single/double/triple/aromatic), `atom.type` (force-field type), `cell.type` | member | `kind`, `order` | keep — the dot rule is enough | **yes** |
| `parallel` | R | **parallel evolution**, parallel substitutions in homoplasy | binding | `convergent` | not addressed | member only |
| `break` | K | **bond breaking** | free fn | `break_bond` (compound) | keep | n/a |
| `with` | R | `with_gaps`, `with_capacity` | binding | compound identifiers | keep | n/a |

### 6.3 The six §3 does not have, and why that matters

`scientific-libraries.md` §3 has eleven rows. This table has twenty, and six are
new:

**`match`, `type`, `pure`, `parallel`** — §3 checked the words a *numerics*
library wants. `match` appears there only as `re.match`. It did not check
alignment vocabulary, where `match` is one of four operation kinds and appears as
a field in every alignment summary, nor molecular-file vocabulary, where
`bond.type` is the field name in MOL, SDF, PDB and every force-field topology.

**`interface`, `null`, `loop`** — these three are different in kind and worth a
separate paragraph, because **they did not exist when §3 was written.**
`syntax-revision-2.md` introduced all three the same day: `interface` in its §6
(replacing `trait`), `null` in its §3.1 (the literal), and `loop` in its §2.2
(replacing `while`). Its §7 summary table counts the words that *leave* the
reserved list — nine — and the two that join it: `interface` and `null`. `loop`
is not counted at all, because §13 already carried it.

None of the three was checked against scientific vocabulary, and all three collide:

- **`interface`** is structural biology's word for the contact surface between two
  chains. `interface_area`, `interface_residues`, `buried_interface` are standard;
  `let interface be contact_surface(a, b)` is a line somebody writes.
- **`null`** is statistics' word and population genetics' word. `null_distribution`
  and `null_model` are in every permutation test; a **null allele** is a specific,
  named object in a genotyping pipeline.
- **`loop`** is one of the four secondary-structure classes. `Helix`, `Sheet`,
  `Loop`, `Turn` — capitalised as a `choice`, which is fine — but `structure.loop`
  and `let loop be residues[10..18]` are not.

> **Decision 6.3. This note records the three as *findings*, not as a request to
> reverse `syntax-revision-2.md`. All three words earn their keyword status on the
> merits and none of the three collisions is in a position the dot rule leaves
> broken *and* that has no compound alternative. The catalogue uses
> `contact_surface`, `baseline` and `coil`.**

The reason to record them anyway is the one `README.md`'s diagnostic-code section
makes about collisions: *every one of them was invisible to the note that caused
it, because each had honestly checked against everything that existed when it
started.* `syntax-revision-2.md` is a good note and it checked the reserved list
it was shrinking. It did not check the scientific vocabulary, because
`scientific-libraries.md` §3 — the only place that vocabulary is written down —
was already written and looked complete.

**The general ask, which is cheap:** `reserved-words.md` should carry the
scientific-vocabulary check as a *standing test* that any note adding a keyword
runs, rather than as a one-time audit. Three words in one day slipped past it.

### 6.4 What the dot rule alone recovers

Of the twenty rows, the dot rule of `reserved-words.md` §0.1 — *after a `.`,
accept any word as a member name* — fully closes four: `union`, `shape` (member),
`at`, `type`, `mod`. It partially closes eleven more, leaving only binding and
field positions broken.

Reading §6.2's last column as a whole: **the dot rule is worth more to chemistry
and biology than every other reserved-word decision combined**, because these
domains are object-heavy — `molecule.shape`, `bond.type`, `reaction.yield`,
`contact.interface`, `scoring.match`, `spectrum.at(λ)`, `structure.loop` — and
almost all of the pressure is in member position.

> **Decision 6.4. This note's strongest single ask on the reserved-word question
> is the dot rule, not the freeing of any individual word.** It is five lines of
> parser, `reserved-words.md` §5 item 1 already requests it *"regardless of every
> other decision here"*, and it recovers seven of this section's twenty
> collisions outright.

### 6.5 The cost of the reservations, totalled

Stated as one number, because §3's purpose was to make the cost visible and it has
never been totalled in one place.

| | Count |
|---|---|
| Chemistry / biology names this catalogue would spell differently if `reserved-words.md` §5 item 2 landed | **34** |
| …of which are the `yield` family | 11 |
| …the `kernel` family | 7 |
| …the `model` family | 6 |
| …the `shape` / `tensor` / `union` / `mod` families | 10 |
| Collisions the dot rule closes outright | 5 words, ~40 call sites |
| Collisions with no fix under any proposal on the table | **3** — `match`, `null`, `interface` in *binding* position |

Thirty-four names is about 5% of the catalogue. That is not catastrophic and it is
not negligible, and the shape of it is the point: the renames are concentrated in
the four or five words that name the *results* of these disciplines, so a reader
of Science chemistry code will notice them immediately and continuously.

### 6.6 The revert list, pre-committed

The eleven names that change the day `yield` is freed, so the edit is mechanical:

| Today | Then |
|---|---|
| `theoretical_produced` | `theoretical_yield` |
| `percent_produced` | `percent_yield` |
| `actual_produced` | `actual_yield` |
| `isolated_produced` | `isolated_yield` |
| `quantum_produced` | `quantum_yield` |
| `radiochemical_produced` | `radiochemical_yield` |
| `produced_strength` | `yield_strength` |
| `atp_produced` | `atp_yield` |
| `biomass_produced` | `biomass_yield` |
| `produced_coefficient` | `yield_coefficient` |
| `Reaction.efficiency` | `Reaction.yield` |

Five of those — `quantum_produced`, `radiochemical_produced`, `produced_strength`,
`atp_produced`, `produced_coefficient` — are names no reader will parse on first
sight. `produced_strength` for the stress at which a material yields is not a
rename, it is a different concept expressed by accident.

---

## 7. `chem` — the module map

**Every name in §8 is tier 3 unless marked `◆`.** Markers are §1.3's.

`scientific-libraries.md` §10 catalogues `chem` as one flat module with six
subsections. This note splits it.

> **Decision 7.1. `chem` is a namespace, not a module. The code lives in thirteen
> submodules and the data in six data packages. `use chem` brings in the shared
> types — `Element`, `Formula`, `Reaction`, `Molecule` — and nothing else.**

**Rejected: one flat `chem` module.** It is what §10 implies and it has two
defects that only appear at this catalogue's size. First,
`stdlib-shape-and-packages.md` §2 item 1 promises a Level 2 module is absent from
a binary that does not name it; a flat `chem` means a program computing a molar
mass links the crystallography tables, the space-group generators and the
spectroscopy code. Second, the dependency closure becomes the union of all
thirteen — `chem` would depend on `linalg`, `optimize`, `signal` and four data
packages to answer `molar_mass("H2O")`, which is a BLAS installation to add up
eighteen atomic weights.

**Cost.** Thirteen `use` lines instead of one, and a user who does not know which
submodule a function is in has to look. Mitigated by keeping **the subsection
boundaries of §10 as the module boundaries** wherever possible, so the catalogue a
reader already has is the map.

| Module | Depends on | `†` | Contents |
|---|---|---|---|
| `chem` | core | no | shared types only |
| `chem.units` | `physics.units` | **yes** (`Da`, §3.3) | `M`, `Da`, `eq`, `amu`, `torr`, `atm`, `bar`, `cal`, `kcal`, `debye`, `angstrom` |
| `chem.elements` | `chem.units`, `chem.data.ciaaw`, `chem.data.ame` | **yes** | the periodic table, isotopes |
| `chem.formula` | `chem.elements` | **yes** (via masses) | parsing, molar mass, isotope patterns |
| `chem.stoich` | `chem.formula`, `linalg` | **yes** | balancing, limiting reagent, conversions |
| `chem.solution` | `chem.formula` | no | molarity, molality, dilution, colligative |
| `chem.acid_base` | `chem.solution`, `chem.data.pka`, `optimize` | **yes** | pH, buffers, titration |
| `chem.thermo` | `chem.units`, `chem.data.janaf` | **yes** | enthalpy, entropy, Gibbs, Hess |
| `chem.equilibrium` | `chem.thermo`, `optimize` | **yes** | K, Q, ICE, Gibbs minimisation |
| `chem.kinetics` | `chem.solution`, `math` (ODE), `optimize` | no | rate laws, Arrhenius, Eyring, enzymes |
| `chem.electro` | `chem.equilibrium`, `physics.constants` | **yes** | Nernst, cells, electrolysis |
| `chem.quantum` | `linalg`, `chem.data.basis` ‡ | **yes** ‡ | analytic orbitals, Hückel, tight binding |
| `chem.spectra` | `signal`, `chem.formula` | **yes** | NMR, IR, UV-Vis, MS |
| `chem.structure` | `linalg`, `chem.elements` | **yes** | `Molecule`, geometry, formats, RMSD |
| `chem.crystal` | `linalg`, `chem.data.spacegroups` | **yes** | cells, symmetry, Bragg, powder patterns |

Data packages: `chem.data.ciaaw` (standard atomic weights), `chem.data.ame`
(nuclide masses and abundances), `chem.data.janaf` (thermochemistry),
`chem.data.pka` (dissociation constants), `chem.data.spacegroups` (the 230
groups), `chem.data.basis` (Gaussian basis sets).

**`chem.kinetics` is the only submodule that is not `†`**, which is a fact worth
noticing: kinetics is the one part of chemistry that is entirely formulas over
the caller's own measurements. Everything else reads a table.

---

## 8. The chemistry catalogue

### 8.1 Elements, isotopes and the periodic table

**Types.** `Element` `Isotope` `Nuclide` `PeriodicTable` `Block` `ElementGroup`
`ElementCategory` `OxidationState` `ElectronConfiguration` `Orbital`

**Lookup.** `by_symbol` `by_number` `by_name` `by_cas` `all_elements`
`group_of` `period_of` `block_of` `category_of` `is_metal` `is_metalloid`
`is_nonmetal` `is_noble_gas` `is_halogen` `is_lanthanide` `is_actinide`
`is_transition_metal` `is_radioactive` `is_synthetic`

**Properties, all `†`.** `atomic_number` `atomic_mass †`
`standard_atomic_weight †` `atomic_weight_interval †`
`abridged_atomic_weight †` `electron_configuration †` `valence_electrons †`
`electronegativity_pauling †` `electronegativity_allen †`
`electronegativity_mulliken †` `covalent_radius †` `van_der_waals_radius †`
`ionic_radius †` `metallic_radius †` `ionisation_energy †`
`ionisation_energies †` `electron_affinity †` `oxidation_states †`
`common_oxidation_states †` `melting_point †` `boiling_point †`
`density_solid †` `density_gas †` `heat_of_fusion †`
`heat_of_vaporisation †` `specific_heat †` `thermal_conductivity †`
`electrical_resistivity †` `crystal_structure †` `magnetic_ordering †`
`colour †` `discovery_year †` `abundance_crust †` `abundance_universe †`

**Isotopes, all `†`.** `isotopes_of †` `isotope †` `isotope_mass †`
`isotope_abundance †` `natural_isotopes †` `stable_isotopes †` `is_stable †`
`half_life †` `decay_mode †` `decay_energy †` `decay_chain †`
`binding_energy †` `binding_energy_per_nucleon †` `mass_excess †`
`mass_defect †` `neutron_number` `mass_number` `nuclear_spin †`
`magnetic_moment †` `quadrupole_moment †` `nmr_frequency †`
`nmr_receptivity †` `radioactive_decay` `activity` `becquerel_from_mass †`

**Not shipped.** `✗ nuclear_cross_section` — ENDF, JEFF and JENDL are three
incompatible evaluated libraries, each hundreds of megabytes, each versioned
independently, and the answer is energy-dependent and temperature-broadened. That
is a data package somebody else should ship and a nuclear-physics problem, not a
chemistry lookup.

**Five signatures.**

```science
# The table is a value. `by_symbol` fails on an unknown symbol rather than
# panicking, because a symbol usually comes from user input or a file.
function by_symbol(table: borrowed PeriodicTable, symbol: borrowed String)
    -> (Element, UnknownElement?)

# The free form reads the lockfile-pinned default (Decision 4.3). Its answer is
# `†` and the run record says which release it came from (Decision 4.4).
function atomic_mass(element: Element) -> MolarMass of F64

# Fourteen elements have an interval, not a value. A caller that wants the
# interval asks for it; a caller that wants a number gets the conventional one.
function atomic_weight_interval(element: Element)
    -> (MolarMass of F64, MolarMass of F64)?

# Radioactive decay is exact arithmetic over a `†` half-life.
function activity(nuclide: Nuclide, amount: Amount of F64)
    -> Quantity of (F64, 0, 0, -1, 0, 0, 0, 0)

function isotopes_of(element: Element) -> Array of Isotope
```

**Example.** The average mass of natural chlorine, from its isotopes, checked
against the tabulated weight:

```science
use chem.elements

function main():
    let cl, err be elements.by_symbol(elements.DEFAULT, "Cl")
    if err?:
        print("no such element")
        return

    let mutable total be 0.0<g/mol>
    for isotope in elements.isotopes_of(cl):
        total be total
            + elements.isotope_mass(isotope) * elements.isotope_abundance(isotope)

    print(f"computed {total}, tabulated {elements.atomic_mass(cl)}")
```

### 8.2 Formulas and molecular weight

**Types.** `Formula` `FormulaTerm` `Compound` `Hydrate` `ChargeState`
`IsotopePattern` `IsotopePeak`

**Parsing and printing.** `parse ~` `parse_hill` `parse_condensed` `format ~`
`format_hill` `format_latex` `format_unicode` `normalise` `expand_groups`
`to_hill_order` `to_alphabetical`

**Composition.** `element_counts` `atom_count` `distinct_elements`
`contains_element` `mass_fractions †` `mass_percent †` `atom_percent`
`empirical_formula` `molecular_formula_from_empirical`
`empirical_from_percentages †` `combustion_analysis †`

**Mass.** `molar_mass †` `average_mass †` `monoisotopic_mass †` `exact_mass †`
`nominal_mass` `most_abundant_mass †` `mass_of_charge_state †` `mz †`
`neutral_mass_from_mz †`

**Isotope patterns.** `isotope_pattern †` `isotope_pattern_centroid †`
`isotope_pattern_at_resolution †` `pattern_fine_structure †`
`enrichment_pattern †` `label_pattern †`

**Structural counting.** `degree_of_unsaturation` `rings_plus_double_bonds`
`hydrogen_deficiency` `formal_charge` `oxidation_number_sum`

**Not shipped.** `✗ formula_from_name` — IUPAC nomenclature parsing. The grammar
is enormous, the standard is prose, and the open implementations disagree on
ordinary inputs.

**Five signatures.**

```science
# Parsing fails with a position, because a formula usually comes from a file.
function parse(text: borrowed String) -> (Formula, FormulaError?)

# The payoff of §5: a molar mass is kg/mol and cannot be added to a mass.
function molar_mass(formula: borrowed Formula) -> MolarMass of F64

# Monoisotopic mass uses the most abundant isotope of each element, which is a
# different `†` table from the standard atomic weight and a different answer.
function monoisotopic_mass(formula: borrowed Formula) -> MolarMass of F64

# The pattern is a function of the resolution, which is a `~` variant: the
# caller's instrument decides how many peaks are resolved.
function isotope_pattern_at_resolution(
    formula: borrowed Formula,
    resolution: F64,
    minimum_abundance: F64,
) -> Array of IsotopePeak

function degree_of_unsaturation(formula: borrowed Formula)
    -> (F64, MixedValenceError?)
```

**Example.**

```science
use chem.formula

function main():
    let glucose, err be formula.parse("C6H12O6")
    if err?:
        print("bad formula")
        return

    print(f"{formula.format_hill(glucose)}: {formula.molar_mass(glucose)}")
    print(f"monoisotopic: {formula.monoisotopic_mass(glucose)}")
```

### 8.3 Stoichiometry and balancing

**Types.** `Reaction` `ReactionSide` `Species` `Coefficient` `PhaseLabel`

**Parsing.** `parse_reaction` `format_reaction` `reverse` `combine`
`half_reactions` `net_ionic` `spectator_ions`

**Balancing.** `balance` `balance_redox ~` `balance_with_charge` `is_balanced`
`element_balance` `charge_balance` `nullspace_coefficients`
`smallest_integer_coefficients` `oxidation_states_in ~`

**Conversions.** `moles_from_mass †` `mass_from_moles †` `moles_from_volume`
`volume_from_moles` `moles_from_particles` `particles_from_moles`
`moles_from_pressure_volume` `stoichiometric_ratio` `scale_reaction`

**Limiting reagent and product.** `limiting_reagent †` `excess_reagent †`
`excess_amount †` `theoretical_produced †` `actual_from_percent †`
`percent_produced †` `atom_economy †` `mass_balance †` `conversion`
`selectivity` `reaction_extent`

**Gas laws in the stoichiometric context.** `molar_volume_stp` `gas_volume_at`
`partial_pressure` `mole_fraction_from_pressures`

**`balance` is the function that cannot say what it does.** It computes the
**kernel** of the element-count matrix. `kernel` is reserved, so the internal
name is `nullspace_coefficients`, which is the same thing said less well by a
library that was not allowed to use the word. §6.2's `kernel` row.

**Five signatures.**

```science
# Balancing is the nullspace of the element-count matrix over the rationals,
# then cleared to smallest integers. It fails when the nullspace is empty
# (unbalanceable) or has dimension > 1 (underdetermined) — two different errors
# that a chemist needs told apart.
function balance(reaction: borrowed Reaction) -> (Reaction, BalanceError?)

# Where §5.4(c) earns its keep: `available` is an Amount, not a Mass, so a
# caller who forgot to divide by the molar mass is stopped.
function limiting_reagent(
    reaction: borrowed Reaction,
    available: borrowed Map of (Species, Amount of F64),
) -> (Species, NoReagentsError?)

# `produced`, not `yield` (§6.1). Reverts to `theoretical_yield`.
function theoretical_produced(
    reaction: borrowed Reaction,
    limiting: Species,
    available: Amount of F64,
    product: Species,
) -> Amount of F64

function percent_produced(actual: Amount of F64, theoretical: Amount of F64) -> F64

# Redox balancing needs a medium, which is a `~` variant with two answers.
function balance_redox(reaction: borrowed Reaction, medium: Medium)
    -> (Reaction, BalanceError?)
```

**Example.** The combustion of propane, balanced:

```science
use chem.stoich

function main():
    let raw, parse_err be stoich.parse_reaction("C3H8 + O2 -> CO2 + H2O")
    if parse_err?:
        print("could not parse")
        return

    let balanced, err be stoich.balance(raw)
    if err?:
        print(f"cannot balance: {err.message()}")
        return

    print(stoich.format_reaction(balanced))     # C3H8 + 5 O2 -> 3 CO2 + 4 H2O
```

### 8.4 Solutions and colligative properties

**Types.** `Solution` `Solute` `Solvent` `Dilution` `SaturationState`

**Concentration.** `molarity` `molality` `mole_fraction` `mass_fraction`
`mass_percent` `volume_percent` `mass_per_volume` `parts_per_million`
`parts_per_billion` `formality` `osmolarity` `osmolality` `ionic_strength`
`activity_coefficient_debye_huckel ~` `activity_coefficient_davies ~`
`activity_coefficient_pitzer ~ †` `activity` `mean_ionic_activity`
`normality ✗`

**Conversions between them.** `molarity_to_molality †` `molality_to_molarity †`
`mole_fraction_to_molality` `mass_percent_to_molarity †` `ppm_to_molarity †`

**Dilution and mixing.** `dilution` `dilution_volume` `serial_dilution`
`mix_solutions` `mixing_concentration` `stock_volume_needed`

**Solubility.** `solubility_product` `ion_product` `is_saturated`
`molar_solubility †` `common_ion_effect †` `solubility_temperature ~ †`
`henry_constant †` `henry_solubility †`

**Colligative.** `colligative_freezing †` `colligative_boiling †`
`vapour_pressure_lowering †` `osmotic_pressure` `vant_hoff_factor †` `raoult_pressure`
`raoult_deviation` `ideal_solution_pressure`

> **Decision 8.4. `normality` and a `<N>` unit are refused. `chem.units` declares
> `eq` only as a per-reaction equivalence factor, never as a concentration unit.**

Normality is not a property of a solution. One molar sulfuric acid is 2 N for an
acid–base titration, 2 N for a two-electron redox, and something else for a
precipitation. The number depends on the reaction, which is not in the solution.
**A unit whose value depends on what you later do with the thing is not a unit**,
and `<N>` would additionally collide with the newton. Callers compute equivalents
explicitly from a stated reaction, which is what the quantity always meant.

**Cost.** Analytical chemists who learned normality have to write one more line.
The compensation is that the line names the reaction, which is the information
normality was hiding.

**Five signatures.**

```science
function molarity(amount: Amount of F64, volume: Volume of F64)
    -> Concentration of F64

# Molality is per kilogram of *solvent*, which is why it takes a solvent mass
# and not a solution mass. Getting that wrong is the other half of §5.4(a).
function molality(amount: Amount of F64, solvent_mass: Mass of F64)
    -> Molality of F64

# The one in §5.4(a). Molality, not molarity, and the type says so.
function colligative_freezing(
    cryoscopic: Quantity of (F64, 0, -1, 0, 0, 1, -1, 0),
    molality: Molality of F64,
    vant_hoff: F64,
) -> Temperature of F64

function dilution(
    stock: Concentration of F64,
    stock_volume: Volume of F64,
    final_volume: Volume of F64,
) -> (Concentration of F64, DilutionError?)

# `~`: three activity models with three validity ranges, three functions.
function activity_coefficient_davies(charge: Int, ionic_strength: Molality of F64)
    -> F64
```

**Example.**

```science
use chem.solution
use chem.units

function main():
    let stock be 2.5<mol/L>
    let final, err be solution.dilution(stock, 10.0<mL>, 250.0<mL>)
    if err?:
        print("that is not a dilution")
        return
    print(f"final: {final}")
```

### 8.5 Acids, bases, buffers and titration

**Types.** `AcidBasePair` `PolyproticAcid` `Buffer` `TitrationCurve`
`TitrationPoint` `Indicator` `SpeciationResult`

**Constants, all `†`.** `k_a †` `k_b †` `p_ka †` `p_kb †` `k_w ~ †` `p_kw ~ †`
`conjugate_base` `conjugate_acid` `is_strong_acid †` `is_strong_base †`
`acid_strength_order †`

**pH and speciation.** `ph` `poh` `ph_from_poh` `hydronium_from_ph`
`hydroxide_from_ph` `ph_strong_acid` `ph_strong_base` `ph_weak_acid`
`ph_weak_base` `ph_polyprotic` `ph_salt` `ph_amphiprotic` `speciation ~`
`fraction_deprotonated` `alpha_fractions` `charge_balance_ph`
`proton_condition`

**Buffers.** `henderson_hasselbalch_ph` `buffer_ph` `buffer_capacity`
`buffer_range †` `buffer_from_target †` `buffer_dilution_effect`
`buffer_after_addition` `best_buffer_for †`

**Titration.** `titration_curve ~` `titration_point` `equivalence_point`
`equivalence_volume` `half_equivalence_point` `titration_derivative`
`gran_plot` `indicator_choice †` `indicator_range †` `titration_error`
`back_titration` `kjeldahl_nitrogen` `karl_fischer_water`

**pH's two ends, both honest.** `unit-literals.md` §6.3 already refuses `<pH>` as
a unit, because a logarithm does not compose with products. This catalogue adopts
that and adds the input side: **`ph` takes a `Concentration of F64`, never a bare
`F64`.** A bare number cannot become a concentration without an explicit
conversion (`scientific-libraries.md` §12.7), so `ph(0.01)` does not compile and
`ph(0.01<mol/L>)` does. Dimensioned in, dimensionless out — the logarithm is
where the dimension is discarded, and it is discarded on purpose.

The residual dishonesty, named rather than hidden: pH is defined over *activity*,
not concentration, so `ph(c)` assumes an activity coefficient of 1.
`ph_from_activity` sits beside it. Nothing in the type system distinguishes them,
because both arguments are dimensionally identical. This is §5.5(4).

**Five signatures.**

```science
# Dimensioned in, dimensionless out. `ph(0.01)` does not compile.
function ph(hydronium: Concentration of F64) -> F64

function henderson_hasselbalch_ph(p_ka: F64, base: Amount of F64, acid: Amount of F64)
    -> F64

# Weak-acid pH is a quadratic in general and a cubic once water's autoionisation
# matters; the solver can fail to bracket, so it returns an error.
function ph_weak_acid(k_a: F64, concentration: Concentration of F64)
    -> (F64, SolveError?)

# A curve is `~`: the titrant, the model and the step count are all choices.
function titration_curve(
    analyte: borrowed Solution,
    titrant: borrowed Solution,
    steps: Int,
) -> (TitrationCurve, TitrationError?)

function buffer_capacity(buffer: borrowed Buffer, at_ph: F64) -> Molality of F64
```

**Example.**

```science
use chem.acid_base
use chem.units

function main():
    # An acetate buffer: 0.10 mol acid, 0.15 mol conjugate base.
    let value be acid_base.henderson_hasselbalch_ph(4.76, 0.15<mol>, 0.10<mol>)
    print(f"pH = {value:.2}")
```

### 8.6 Thermodynamics

**Types.** `ThermoState` `Species` `ShomateCoefficients` `StandardState` `Phase`
`PathProcess`

**Standard data, all `†`.** `enthalpy_of_formation †` `entropy_standard †`
`gibbs_of_formation †` `heat_capacity_standard †` `shomate_coefficients †`
`nist_species †` `bond_enthalpy †` `lattice_energy †` `hydration_enthalpy †`
`sublimation_enthalpy †` `fusion_enthalpy †` `vaporisation_enthalpy †`

**Reaction quantities.** `enthalpy_of_reaction †` `entropy_of_reaction †`
`gibbs_of_reaction †` `heat_capacity_of_reaction †` `hess_law`
`gibbs_from_enthalpy_entropy` `spontaneous_at` `crossover_temperature`
`born_haber_lattice_energy †` `bond_enthalpy_estimate †`

**Temperature and pressure dependence.** `enthalpy_at ~` `entropy_at ~`
`gibbs_at ~` `heat_capacity_shomate` `heat_capacity_polynomial ~` `kirchhoff_enthalpy`
`van_t_hoff_constant` `gibbs_helmholtz` `clausius_clapeyron_slope`
`clausius_clapeyron_pressure` `antoine_pressure ~ †` `pressure_correction`

**Work, heat and processes.** `work_isothermal` `work_isobaric` `work_adiabatic`
`heat_isobaric` `heat_isochoric` `internal_energy_change` `enthalpy_change`
`entropy_change_isothermal` `entropy_change_heating` `entropy_of_mixing`
`gibbs_of_mixing` `adiabatic_flame_temperature †` `joule_thomson_coefficient †`
`inversion_temperature †`

**Non-ideality.** `fugacity ~` `fugacity_coefficient ~` `activity_coefficient ~`
`excess_gibbs ~` `nrtl ~ †` `uniquac ~ †` `wilson ~ †` `unifac ✗`

**Not shipped.** `✗ unifac` — a group-contribution method whose entire content is
a parameter matrix that is partly commercial, revised continuously, and not
redistributable in full.

**Five signatures.**

```science
# Hess's law over a set of reactions with known enthalpies. It fails when the
# target is not in the span — a linear-algebra failure with a chemical meaning,
# which deserves its own error rather than a generic solve failure.
function hess_law(
    known: borrowed Array of (Reaction, MolarEnergy of F64),
    target: borrowed Reaction,
) -> (MolarEnergy of F64, NotInSpanError?)

# `MolarEnergy` and `MolarEntropy` differ in the temperature exponent, which is
# the mix-up §5.1 exists to catch: kJ/mol read against J/(mol K).
function gibbs_from_enthalpy_entropy(
    enthalpy: MolarEnergy of F64,
    entropy: MolarEntropy of F64,
    temperature: Temperature of F64,
) -> MolarEnergy of F64

# Shomate is a `†` seven-coefficient fit with a stated temperature range;
# outside it the answer is nonsense, so the range is checked.
function heat_capacity_shomate(
    coefficients: borrowed ShomateCoefficients,
    temperature: Temperature of F64,
) -> (MolarHeatCapacity of F64, OutOfRangeError?)

function van_t_hoff_constant(
    k_1: F64,
    temperature_1: Temperature of F64,
    enthalpy: MolarEnergy of F64,
    temperature_2: Temperature of F64,
) -> F64

function clausius_clapeyron_pressure(
    pressure_1: Pressure of F64,
    temperature_1: Temperature of F64,
    vaporisation: MolarEnergy of F64,
    temperature_2: Temperature of F64,
) -> Pressure of F64
```

**Example.**

```science
use chem.thermo
use chem.units

function main():
    let enthalpy be -890.3<kJ/mol>
    let entropy be -242.8<J/mol/K>
    let g be thermo.gibbs_from_enthalpy_entropy(enthalpy, entropy, 298.15<K>)
    print(f"dG = {g}, spontaneous: {g < 0.0<kJ/mol>}")
```

### 8.7 Equilibrium and Le Chatelier

**Types.** `Equilibrium` `IceTable` `IceRow` `StandardState`
`EquilibriumComposition` `Perturbation`

**Constants.** `equilibrium_constant †` `k_from_gibbs` `gibbs_from_k`
`k_c_from_k_p` `k_p_from_k_c` `reaction_quotient` `direction_of_shift`
`is_at_equilibrium` `k_at_temperature †` `k_combined`

**Solving.** `ice_table` `solve_ice` `equilibrium_composition ~`
`equilibrium_extent` `gibbs_minimisation ~ †` `multiple_equilibria ~`
`degree_of_dissociation` `percent_dissociation`

**Perturbation.** `le_chatelier_shift` `shift_on_concentration`
`shift_on_pressure` `shift_on_temperature` `shift_on_inert_gas`
`new_equilibrium_after`

**Coupled systems.** `coupled_equilibria ~` `speciation_diagram ~`
`predominance_diagram` `pourbaix_data ~ †` `fraction_diagram`

> **Decision 8.7. `equilibrium_constant` takes the standard state as an argument
> and returns a dimensionless `F64`. There is no overload that infers it.**

§5.5(2) said K_c has units unless divided by a standard state, and that every
textbook hides the division. This makes it visible in the signature.

**Rejected: `EquilibriumConstant of (T, const DELTA_N: Int)`**, with the dimension
linear in Δn, exactly as §5.2 does for reaction order. It is more honest, and it
would make `k_1 * k_2` for coupled equilibria carry a computed Δn, which is
genuinely useful. It loses because the dimensionless convention is universal in
the literature: a K that printed as `mol^-1·m^3` would be right and would be
reported as a bug by every user on the first day. The const-parameter form is
recorded here as the design to reconsider if anyone ever wants dimensional
checking across a reaction network.

**Cost.** One extra argument on the module's most-called function, usually
`1.0<mol/L>`. The compensation is that a user working in partial pressures cannot
silently get a concentration-based K.

**Five signatures.**

```science
function equilibrium_constant(
    reaction: borrowed Reaction,
    concentrations: borrowed Map of (Species, Concentration of F64),
    standard_state: Concentration of F64,
) -> (F64, EquilibriumError?)

function reaction_quotient(
    reaction: borrowed Reaction,
    concentrations: borrowed Map of (Species, Concentration of F64),
    standard_state: Concentration of F64,
) -> (F64, EquilibriumError?)

# The ICE table is a value, not a printed thing, so it can be inspected.
function ice_table(
    reaction: borrowed Reaction,
    initial: borrowed Map of (Species, Concentration of F64),
) -> IceTable

# Gibbs minimisation over a species set is `optimize`'s job, can fail to
# converge, and is `†` through the formation energies it reads.
function gibbs_minimisation(
    species: borrowed Array of Species,
    temperature: Temperature of F64,
    pressure: Pressure of F64,
    tolerance: F64,
) -> (EquilibriumComposition, ConvergenceError?)

function le_chatelier_shift(reaction: borrowed Reaction, perturbation: Perturbation)
    -> Direction
```

**Example.**

```science
use chem.equilibrium

function main():
    let composition, err be equilibrium.solve_ice(haber, initial, 1.0e-9)
    if err?:
        print(f"no equilibrium found: {err.message()}")
        return
    for species, concentration in composition.entries():
        print(f"{species.formula()}: {concentration}")
```

### 8.8 Kinetics, including enzymes

**Types.** `RateLaw` `RateConstant of (T, const ORDER: Int)` `Mechanism`
`ElementaryStep` `ReactionNetwork` `KineticTrace` `EnzymeParameters`
`InhibitionMode` `ProgressCurve`

**Rate laws.** `rate_law` `rate_from_law` `order_of` `overall_order`
`rate_constant_units` `integrated_zero_order` `integrated_first_order`
`integrated_second_order` `integrated_second_order_mixed` `integrated_nth_order`
`rate_law_fractional` `half_life` `half_life_second_order` `half_life_zero_order`
`lifetime` `concentration_at` `time_to_concentration`

**Temperature dependence.** `arrhenius_rate` `arrhenius_fit` `activation_energy_from`
`pre_exponential_from` `eyring_rate` `eyring_fit` `activation_enthalpy`
`activation_entropy` `activation_gibbs` `q_ten` `kinetic_isotope_effect †`
`tunnelling_correction ~`

**Mechanisms.** `steady_state_approximation` `pre_equilibrium_approximation`
`rate_determining_step` `mechanism_rate_law` `is_consistent_with`
`collision_frequency †` `steric_factor` `collision_theory_rate †`
`transition_state_rate` `diffusion_limited_rate †` `encounter_rate †`

**Networks and integration.** `reaction_network` `stoichiometry_matrix`
`solve_kinetics ~` `solve_kinetics_stiff ~` `gillespie ~` `tau_leaping ~`
`sensitivity_analysis` `conservation_relations` `quasi_steady_state_species`

**Enzyme kinetics.** `michaelis_menten_rate` `michaelis_menten_fit ~`
`lineweaver_burk_transform` `hanes_woolf_transform` `eadie_hofstee_transform` `direct_linear_plot` `k_cat`
`catalytic_efficiency` `turnover_number` `specificity_constant` `hill_equation`
`hill_coefficient` `competitive_inhibition` `uncompetitive_inhibition`
`noncompetitive_inhibition` `mixed_inhibition` `substrate_inhibition` `ping_pong`
`ordered_bi_bi` `random_bi_bi` `ic50_to_ki` `progress_curve_fit ~`
`burst_kinetics`

> **Decision 8.8. `lineweaver_burk_transform`, `hanes_woolf_transform` and `eadie_hofstee_transform` produce
> *transformed data*, not fitted parameters. The fit is `michaelis_menten_fit`,
> which is a non-linear least squares and says so in its name.**

The linearisations exist because people fitted by hand on graph paper for sixty
years, and every one of them distorts the error structure — the Lineweaver–Burk
plot weights the lowest-concentration point most heavily, which is the least
reliable one. Naming them as fitting functions would ship a known-bad estimator
under an authoritative name. Naming them as transforms keeps them available for
the diagnostic plots people legitimately still draw.

**Rejected: not shipping them at all.** They are still in every textbook and
every reviewer's expectations; a library that omits them sends users to
spreadsheets, which is worse.

**Five signatures.**

```science
# The signature from §5.2 and §5.3. `ORDER` is a const parameter, the dimension
# is computed from it, and passing the wrong order is a compile error whose
# message names the order the argument actually has.
function half_life(rate_constant: RateConstant of (F64, 1)) -> Time of F64

# Arrhenius preserves the order, which is the clearest single demonstration
# that the order really is in the type.
function arrhenius_rate of (const ORDER: Int)(
    activation_energy: MolarEnergy of F64,
    temperature: Temperature of F64,
    pre_exponential: RateConstant of (F64, ORDER),
) -> RateConstant of (F64, ORDER)

# Eyring needs the transmission coefficient, which is a modelling choice and is
# therefore an argument rather than a hidden 1.0.
function eyring_rate(
    activation_gibbs: MolarEnergy of F64,
    temperature: Temperature of F64,
    transmission: F64,
) -> Frequency of F64

function michaelis_menten_rate(
    substrate: Concentration of F64,
    v_max: Quantity of (F64, -3, 0, -1, 0, 0, 1, 0),
    k_m: Concentration of F64,
) -> Quantity of (F64, -3, 0, -1, 0, 0, 1, 0)

# A stiff network integration is `math`'s ODE machinery with a chemical wrapper.
# It can fail, and a failed kinetics run must not return zeros.
function solve_kinetics_stiff(
    network: borrowed ReactionNetwork,
    initial: borrowed Map of (Species, Concentration of F64),
    span: (Time of F64, Time of F64),
    tolerance: F64,
) -> (KineticTrace, IntegrationError?)
```

**Example.** §5.3's program, written correctly:

```science
use chem.kinetics
use chem.units

function main():
    let k be 0.042<L/mol/s>                    # second order
    let initial be 0.15<mol/L>
    let t_half be kinetics.half_life_second_order(k, initial)
    print(f"half-life: {t_half}")
```

### 8.9 Electrochemistry

**Types.** `Electrode` `HalfCell` `GalvanicCell` `ElectrolyticCell`
`ReferenceElectrode` `PourbaixRegion`

**Potentials, all `†`.** `standard_potential †` `standard_reduction_potential †`
`standard_oxidation_potential †` `potential_versus ~ †` `reference_offset †`
`she_to_sce †` `she_to_agcl †`

**Cells.** `cell_potential` `standard_cell_potential` `nernst_potential`
`nernst_potential_at_temperature` `cell_gibbs` `cell_equilibrium_constant` `cell_notation`
`parse_cell_notation` `is_spontaneous` `concentration_cell`
`liquid_junction_potential ~`

**Electrolysis.** `faraday_electrolysis` `charge_from_current_time`
`moles_deposited` `mass_deposited †` `time_for_mass †` `current_efficiency`
`electrochemical_equivalent †` `energy_consumption`

**Kinetics at electrodes.** `butler_volmer_current` `tafel_slope` `tafel_fit`
`exchange_current_density` `overpotential` `ohmic_drop` `limiting_current`
`cottrell_current` `randles_sevcik_current` `koutecky_levich_current`

**Conductivity.** `molar_conductivity` `limiting_molar_conductivity †`
`kohlrausch_conductivity` `degree_of_ionisation` `transport_number †` `ionic_mobility †`
`conductivity_from_concentration †`

**Pourbaix.** `pourbaix_data ~ †` `stability_region` `water_stability_lines`
`immunity_region` `passivation_region`

**`faraday_electrolysis` is one of the three non-`†` functions in `chem`** (§4.7),
because the Faraday constant became exact in 2019.

**Five signatures.**

```science
# The electron count is an Int and is not inferable from the potentials, so it
# is an argument. Getting it wrong is the classic Nernst error and the type
# system cannot catch it — §5.5(4).
function nernst_potential(
    standard: Potential of F64,
    electrons: Int,
    reaction_quotient: F64,
    temperature: Temperature of F64,
) -> Potential of F64

function cell_potential(
    cathode: borrowed HalfCell,
    anode: borrowed HalfCell,
    temperature: Temperature of F64,
) -> (Potential of F64, CellError?)

# Charge in, amount out. Not `†`: the Faraday constant is exact (§2.3(ii)).
function faraday_electrolysis(charge: Charge of F64, electrons: Int)
    -> Amount of F64

function butler_volmer_current(
    exchange_current: Quantity of (F64, -2, 0, 0, 1, 0, 0, 0),
    overpotential: Potential of F64,
    transfer_coefficient: F64,
    electrons: Int,
    temperature: Temperature of F64,
) -> Quantity of (F64, -2, 0, 0, 1, 0, 0, 0)

function molar_conductivity(
    conductivity: Quantity of (F64, -3, -1, 3, 2, 0, 0, 0),
    concentration: Concentration of F64,
) -> MolarConductivity of F64
```

**Example.**

```science
use chem.electro
use chem.units

function main():
    let e be electro.nernst_potential(1.10<V>, 2, 0.01, 298.15<K>)
    print(f"cell potential: {e}")
```

### 8.10 Quantum and computational chemistry

The section where the honest answer is mostly "no". §12.1 gives the argument.
What is shipped is analytic and semi-empirical; what is linked needs an integral
engine; everything else is refused.

**Analytic one-electron results — written in Science.**
`particle_in_box_energy` `particle_in_box_wavefunction`
`harmonic_oscillator_energy` `harmonic_oscillator_wavefunction`
`rigid_rotor_energy` `morse_potential` `morse_energy` `hydrogen_energy`
`hydrogen_radial` `hydrogen_wavefunction` `hydrogenic_energy`
`radial_distribution` `orbital_node_count` `spherical_harmonic`
`real_spherical_harmonic` `slater_orbital` `gaussian_orbital`
`orbital_overlap_analytic`

**Electronic structure heuristics — written in Science.**
`slater_rules_shielding` `effective_nuclear_charge` `aufbau_configuration †`
`hund_ground_term` `term_symbol` `russell_saunders_term` `madelung_order`
`huckel_matrix` `huckel_energies` `huckel_coefficients` `huckel_bond_order`
`extended_huckel ~ †` `tight_binding ~` `particle_on_ring`
`free_electron_model` `woodward_hoffmann_symmetry`

**Molecular descriptors — written in Science.** `dipole_from_charges`
`polarisability_from_field` `hardness` `softness`
`electronegativity_mulliken` `electrophilicity_index` `fukui_from_charges`
`homo_lumo_gap` `ionisation_from_koopmans`

**Linked, `‡`.** `scf_energy ‡` `dft_energy ‡` `mp2_energy ‡` `ccsd_energy ‡`
`geometry_optimise ‡` `frequencies ‡` `nmr_shielding ‡`
`population_analysis ‡` `basis_set †‡` `exchange_correlation_functional †‡`

**Refused.** `✗` integral engine (ERIs, Obara–Saika, McMurchie–Davidson, Rys
quadrature); `✗ scf_iteration` written in Science; `✗ exchange_correlation`
functionals written in Science; `✗ molecular_dynamics` integrator. §12.1.

**Five signatures.**

```science
# Written in Science, analytic and exact.
function particle_in_box_energy(level: Int, mass: Mass of F64, length: Length of F64)
    -> Energy of F64

# Hückel is a symmetric eigenproblem over a connectivity matrix — small, exact,
# and the one piece of real electronic structure worth writing rather than
# linking.
function huckel_energies of (const N: Int)(
    connectivity: borrowed Symmetric of (F64, N),
    alpha: Energy of F64,
    beta: Energy of F64,
) -> Array of Energy of F64

# `†`: the aufbau order has documented exceptions (Cr, Cu, Pd, …) that come
# from a table and not from the rule.
function aufbau_configuration(element: Element) -> ElectronConfiguration

# Linked. Science owns the molecule and the result type; the C library owns
# everything between them.
function scf_energy(
    molecule: borrowed Molecule,
    basis: borrowed BasisSet,
    settings: borrowed ScfSettings,
) -> (Energy of F64, ScfError?)

function homo_lumo_gap(orbitals: borrowed Array of Energy of F64, electrons: Int)
    -> (Energy of F64, NoGapError?)
```

**Example.**

```science
use chem.quantum
use chem.units

function main():
    let m be 9.1093837139e-31<kg>
    let e1 be quantum.particle_in_box_energy(1, m, 1.0<nm>)
    let e2 be quantum.particle_in_box_energy(2, m, 1.0<nm>)
    print(f"lowest transition: {e2 - e1}")
```

### 8.11 Spectroscopy

**Shared.** `Spectrum` `Peak` `PeakList` `Baseline` `Window`
`absorbance` `transmittance` `path_length_from` `concentration_from_absorbance`
`molar_absorptivity †` `baseline_correct ~` `smooth ~` `peak_pick ~`
`peak_integrate ~` `peak_fit ~` `deconvolve ~` `resolution` `signal_to_noise`
`wavenumber_to_wavelength` `wavelength_to_frequency` `energy_to_wavenumber`

**NMR.** `nmr_chemical_shift †` `nmr_reference †` `chemical_shift_range †`
`coupling_constant †` `multiplicity` `pascal_triangle_intensities`
`first_order_multiplet` `spin_system` `simulate_spectrum ~` `integration_ratio`
`noe_distance` `t1_fit` `t2_fit` `cosy_peaks` `hsqc_peaks`
`nucleus_properties †` `larmor_frequency †` `shift_prediction ✗`

**IR and Raman.** `ir_frequencies ~ †` `group_frequency †` `fingerprint_region`
`hooke_frequency` `reduced_mass` `force_constant_from_frequency` `isotope_shift`
`raman_shift` `depolarisation_ratio` `normal_mode_count` `selection_rule_ir`
`selection_rule_raman` `mutual_exclusion`

**UV-Vis.** `uv_vis_gap` `woodward_fieser_maximum †` `chromophore_absorption †`
`conjugation_length_estimate` `isosbestic_point` `job_plot`
`benesi_hildebrand_constant` `tauc_plot ~`

**Mass spectrometry.** `mz †` `charge_state_from †` `deconvolve_charge ~ †`
`isotope_pattern †` `nitrogen_rule` `ring_double_bond_equivalent`
`fragment_formulas †` `neutral_losses †` `mass_defect_filter †`
`formula_from_mass ~ †` `ppm_error` `mass_accuracy`
`spectral_library_search ✗` `de_novo_sequencing ✗`

**Refused.** `✗ shift_prediction` — HOSE codes and ML shift predictors are
research artefacts whose answers change per model release; a `†` marker cannot
carry a neural network, and `models-and-inference.md` owns anything that can.
`✗ spectral_library_search`, `✗ de_novo_sequencing` — a database plus a scoring
heuristic, and the heuristic is the subject of ongoing competition.

**The NMR section is where `tensor` hurts** (§6.2): the shielding tensor, the
quadrupole coupling tensor and the g-tensor are the objects, and none of them can
be bound to a variable called `tensor`.

**Five signatures.**

```science
# Beer–Lambert with all three dimensions checked: absorptivity is L/(mol·cm),
# path is a length, concentration is mol/m^3, absorbance is dimensionless.
function absorbance(
    molar_absorptivity: Quantity of (F64, 2, 0, 0, 0, 0, -1, 0),
    path: Length of F64,
    concentration: Concentration of F64,
) -> F64

# Peak picking is `~` in every respect: threshold, width, and whether a
# shoulder is a peak. Three arguments make the choices visible.
function peak_pick(spectrum: borrowed Spectrum, threshold: F64, minimum_width: F64)
    -> PeakList

function larmor_frequency(
    nucleus: Nuclide,
    field: Quantity of (F64, 0, 1, -2, -1, 0, 0, 0),
) -> Frequency of F64

# `mz` is `†` twice over: the isotope masses and the electron mass.
function mz(formula: borrowed Formula, charge: Int) -> (MolarMass of F64, ChargeError?)

# Candidate formulas within a mass tolerance, constrained by element budgets.
# `~` because the constraint set changes the answer entirely.
function formula_from_mass(
    target: MolarMass of F64,
    tolerance_ppm: F64,
    budget: borrowed Map of (Element, (Int, Int)),
) -> Array of Formula
```

**Example.**

```science
use chem.spectra
use chem.units

function main():
    let a be spectra.absorbance(13900.0<L/mol/cm>, 1.0<cm>, 1.2e-5<mol/L>)
    print(f"A = {a:.3}")
```

### 8.12 Molecular structure, conformers and file formats

**Types.** `Molecule` `Atom` `Bond` `BondOrder` `Conformer` `ConformerSet` `Ring`
`RingSystem` `Stereocentre` `Chirality` `PointGroup` `Fragment`

**Geometry.** `bond_length` `bond_angle` `dihedral` `improper_dihedral`
`distance_matrix` `centre_of_mass` `centroid` `inertia_tensor` `principal_axes`
`rotational_constants` `radius_of_gyration` `molecular_volume ~`
`solvent_accessible_surface ~` `bounding_box` `translate` `rotate`
`align_principal_axes` `rmsd` `rmsd_best_fit` `kabsch_superimpose`
`align_structures` `symmetry_corrected_rmsd`

**Connectivity.** `perceive_bonds ~ †` `bond_order_from_length †`
`ring_perception ~` `smallest_set_of_smallest_rings` `aromaticity ~`
`hybridisation ~` `formal_charges` `implicit_hydrogens` `add_hydrogens ~`
`remove_hydrogens` `canonical_ranking ~` `graph_automorphisms`
`maximum_common_substructure ~` `fragment_on_bonds`

**Stereochemistry.** `stereocentres` `cip_priority ~` `assign_r_s ~`
`assign_e_z ~` `enantiomer` `diastereomers` `invert_centre`

**Conformers.** `generate_conformers ~` `distance_geometry ~` `torsion_scan`
`energy_minimise ‡` `cluster_conformers ~` `boltzmann_weights`
`conformer_ensemble_average` `rotatable_bonds`

**Symmetry.** `point_group ~` `symmetry_operations` `symmetry_number` `is_chiral`
`has_inversion_centre` `character_table †`

**Formats.** `read_xyz` `write_xyz` `read_mol` `write_mol` `read_sdf` `write_sdf`
`read_mol2` `read_pdb` `write_pdb` `read_cif` `read_cml` `smiles_parse`
`smiles_write ~` `smarts_match ~` `inchi ‡` `inchi_key ‡`
`canonical_smiles ✗`

> **Decision 8.12. `inchi` and `inchi_key` bind the IUPAC reference
> implementation and are never reimplemented. `smiles_parse` is written in
> Science; `smiles_write` is written in Science and documented as producing a
> *valid, non-canonical* SMILES. There is no `canonical_smiles` function.**

InChI's specification *is* its C implementation — there is no prose standard from
which a second implementation could be derived, and two InChI strings that differ
are a correctness failure with no arbiter. `ffi-c-boundary.md` exists for this
case exactly.

SMILES is different: the syntax is specified well enough (OpenSMILES) to parse
confidently, and *canonicalisation* is not specified at all. RDKit and Open Babel
produce different canonical SMILES for the same molecule, both correct by their
own rules, neither standard. Shipping `canonical_smiles` would promise a property
the format cannot deliver. Users who need a canonical identifier use the InChIKey,
which is what it is for.

**Cost.** Users arriving from RDKit will look for `canonical_smiles`, not find it,
and need the paragraph above. That is a documentation cost and it is cheaper than
a function that silently disagrees with the tool they came from.

**`molecule.shape` is chemistry vocabulary** — bent, trigonal planar,
tetrahedral — and it is a *second*, non-array meaning of a reserved word that
§6.2 is the first place in this project to record.

**Five signatures.**

```science
# Kabsch superposition is an SVD, which is why `chem.structure` depends on
# `linalg` and waits for F1. It fails on fewer than three non-collinear points.
function kabsch_superimpose(
    mobile: borrowed Array of Position,
    reference: borrowed Array of Position,
) -> (Transform, DegenerateError?)

function dihedral(a: borrowed Atom, b: borrowed Atom, c: borrowed Atom, d: borrowed Atom)
    -> Angle of F64

# Bond perception is `~` and `†`: it reads covalent radii and applies a
# tolerance, and two tolerances give two different molecules.
function perceive_bonds(molecule: borrowed Molecule, tolerance: F64) -> Molecule

# Linked. The InChI string is whatever the reference implementation says.
function inchi(molecule: borrowed Molecule) -> (String, InchiError?)

function read_pdb(path: borrowed Path) -> (Array of Molecule, ParseError?)
```

**Example.**

```science
use chem.structure

function main():
    let molecules, err be structure.read_pdb(Path.new("1ubq.pdb"))
    if err?:
        print(f"could not read: {err.message()}")
        return

    let m be molecules.get(0)
    print(f"{m.atom_count()} atoms, Rg = {structure.radius_of_gyration(m)}")
```

### 8.13 Crystallography

**Types.** `UnitCell` `Lattice` `SpaceGroup` `SymmetryOperation`
`AsymmetricUnit` `Reflection` `PowderPattern` `CrystalStructure`

**Cells and lattices.** `unit_cell` `cell_volume` `reciprocal_cell`
`metric_tensor` `fractional_to_cartesian` `cartesian_to_fractional` `d_spacing`
`bragg_angle` `bragg_wavelength` `lattice_system` `bravais_lattice`
`niggli_reduce` `delaunay_reduce` `cell_transform` `supercell`

**Symmetry, all `†`.** `space_group †` `space_group_by_number †`
`space_group_by_symbol †` `hall_symbol †` `hermann_mauguin †`
`symmetry_operations †` `generators †` `wyckoff_positions †` `site_symmetry †`
`point_group_of †` `laue_class †` `is_centrosymmetric †`
`systematic_absences †` `equivalent_positions †` `asymmetric_unit †`
`setting_choice ~ †`

**Diffraction.** `structure_factor †` `structure_factor_phase`
`atomic_scattering_factor ~ †` `anomalous_scattering †` `debye_waller_factor`
`temperature_factor` `multiplicity_of` `lorentz_polarisation_factor`
`absorption_correction ~` `extinction_correction ~` `powder_pattern ~ †`
`peak_positions` `peak_intensities †` `indexing ~` `pawley_fit ~`
`le_bail_fit ~` `rietveld ✗`

**Analysis.** `bond_valence_sum †` `coordination_number ~` `voronoi_polyhedra`
`packing_fraction` `density_from_cell †` `unit_cell_contents` `r_factor`
`goodness_of_fit`

**Refused.** `✗ rietveld`, `✗ structure_solution`, `✗ phasing` (direct methods,
Patterson, molecular replacement), `✗ refinement`. §12.1.

**The space groups are the cleanest `†` in the catalogue**, and the subtlest: a
space group has more than one *setting*, so "number 62" does not identify a
coordinate system. `setting_choice` is `~` as well as `†`, and a structure read
in one setting and compared against another is a silent wrong answer with no
dimensional signature.

**Five signatures.**

```science
# The 230 groups and their generators are a `†` table, and the setting is a
# required argument because the number alone does not fix the axes.
function space_group_by_number(number: Int, setting: Setting)
    -> (SpaceGroup, UnknownGroupError?)

function d_spacing(cell: borrowed UnitCell, h: Int, k: Int, l: Int) -> Length of F64

# Bragg fails when the reflection is beyond the limiting sphere, which is an
# ordinary condition and not an exceptional one, so it is in the return type.
function bragg_angle(d: Length of F64, wavelength: Length of F64, order: Int)
    -> (Angle of F64, BeyondSphereError?)

# A structure factor is complex and `†` through the scattering factors.
function structure_factor(structure: borrowed CrystalStructure, h: Int, k: Int, l: Int)
    -> Complex of F64

# A powder pattern is a simulation with a profile function — `~` in the peak
# shape, the width model and the background.
function powder_pattern(
    structure: borrowed CrystalStructure,
    wavelength: Length of F64,
    profile: ProfileFunction,
    two_theta_range: (Angle of F64, Angle of F64),
) -> PowderPattern
```

**Example.**

```science
use chem.crystal
use chem.units

function main():
    let cell be crystal.unit_cell(5.431<angstrom>, 5.431<angstrom>, 5.431<angstrom>,
                                  90.0<deg>, 90.0<deg>, 90.0<deg>)
    let d be crystal.d_spacing(cell, 1, 1, 1)
    let theta, err be crystal.bragg_angle(d, 1.5406<angstrom>, 1)
    if err?:
        print("reflection out of range")
        return
    print(f"Si(111) at 2theta = {2.0 * theta}")
```

---

## 9. `bio` — the module map

**Every name in §10 is tier 3 unless marked `◆`. None is marked `◆`.**

> **Decision 9.1. `bio` splits the same way `chem` does, and for the same two
> reasons. `bio.seq` and `bio.align` depend on nothing but the core, which is the
> property that matters most: sequence work is the part with the earliest payoff
> (`scientific-libraries.md` §13, wave 1) and it must not drag in `stats`.**

`scientific-libraries.md` §1 says *"`bio` depends on `stats`, heavily"*. That is
true of five of the twelve submodules and false of the other seven, and the flat
reading would put a statistics dependency under `reverse_complement`. The
layering below is a refinement of §1's sentence, not a contradiction of it.

| Module | Depends on | `†` | Contents |
|---|---|---|---|
| `bio.seq` | core | no | `DnaSequence`, `RnaSequence`, `ProteinSequence`, composition |
| `bio.code` | `bio.seq`, `bio.data.ncbi-codes` | **yes** | genetic codes, translation |
| `bio.align` | `bio.seq`, `bio.data.matrices` | **yes** | pairwise, multiple, scoring |
| `bio.motif` | `bio.seq`, `stats` | no | PWMs, consensus, scanning |
| `bio.phylo` | `bio.align`, `linalg`, `optimize` | no | trees, distances, models |
| `bio.evolution` | `bio.phylo`, `bio.code`, `stats` | **yes** | dN/dS, codon models, clocks |
| `bio.popgen` | `stats` | no | allele frequencies, diversity, selection |
| `bio.expression` | `stats`, `statistical-validity`'s `PValue` | no | counts, normalisation, DE |
| `bio.singlecell` | `bio.expression`, `linalg`, `optimize` | no | embeddings, clustering, trajectories |
| `bio.network` | `stats` | no | graphs, enrichment, pathway scores |
| `bio.structure` | `linalg`, `chem.structure` | **yes** | PDB, secondary structure, contacts |
| `bio.ecology` | `stats`, optionally `bio.phylo` | no | diversity indices, ordination |
| `bio.epi` | `math` (ODE), `stats`, `linalg` | no | compartment models, R₀, survival |

Data packages: `bio.data.ncbi-codes` (genetic code tables), `bio.data.matrices`
(BLOSUM, PAM, and the nucleotide matrices), `bio.data.codon-usage` (per-organism
codon frequency tables), `bio.data.aaindex` (amino-acid property scales).

**Seven of thirteen are not `†`**, which is the mirror image of `chem` (§7, where
only `chem.kinetics` escaped). The reason is structural and worth stating:
**chemistry's content is mostly measurements of the world, and biology's content
is mostly combinatorics over sequences the user supplies.** A Smith–Waterman
alignment has no table in it once the matrix is an argument; a molar mass is
nothing but a table.

---

## 10. The biology catalogue

### 10.1 Sequences and alphabets

**Types.** `DnaSequence` `RnaSequence` `ProteinSequence` `Alphabet`
`Nucleotide` `AminoAcid` `Strand` `SequenceRecord` `SequenceId` `Quality`
`QualityEncoding` `Region` `Feature` `Annotation`

**Construction and conversion.** `from_text` `from_bytes` `to_text`
`with_alphabet` `validate` `is_valid` `sanitise ~` `upper` `lower`
`subsequence` `slice_region` `concatenate` `repeat` `pad`

**Alphabets.** `dna_alphabet` `dna_ambiguous_alphabet` `rna_alphabet`
`protein_alphabet` `protein_extended_alphabet` `iupac_codes`
`expand_ambiguity` `collapse_to_ambiguity` `is_ambiguous` `is_gap`
`is_purine` `is_pyrimidine` `degenerate_count`

**Strand operations.** `complement` `reverse_complement` `reverse`
`both_strands` `strand_of` `canonical_kmer`

**Files.** `read_fasta` `write_fasta` `read_fastq` `write_fastq`
`read_genbank` `read_embl` `read_gff` `read_bed` `read_sam ‡` `read_bam ‡`
`read_vcf` `write_vcf` `index_fasta` `fetch_region`

**Quality.** `phred_from_char ~` `char_from_phred ~` `error_probability`
`average_quality` `trim_by_quality ~` `expected_errors`

**Mutation.** `substitute` `insert` `delete` `apply_variant` `mutate_random`
`shuffle_sequence ~` `shuffle_preserving_dinucleotides ~`

**Decision — the sequence types are distinct and conversion is explicit.**

`scientific-libraries.md` §11.7 already writes `translate` so that a DNA sequence
and a protein sequence are different types. This catalogue extends it: `DnaSequence`,
`RnaSequence` and `ProteinSequence` are three types with no implicit conversion
between any pair, and `transcribe`, `back_transcribe` and `translate` are the only
bridges. The mistake being prevented is real and common: running a DNA motif
scanner over a protein sequence produces a number rather than an error, because
`A`, `C`, `G` and `T` are all valid amino acids.

**Cost.** A generic function over "a sequence" needs the `Sequence` interface and
a type parameter, which §10.4's `align_local` demonstrates. That is one
`where` clause.

**The base pair is not a unit** (§5.5(5)). `length()` returns `Int`.

**Five signatures.**

```science
interface Sequence:
    function length(self) -> Int
    function alphabet(self) -> Alphabet
    function symbol(self, index: Int) -> U8

    function is_empty(self) -> Bool:
        self.length() is 0

# Parsing validates against the alphabet and reports the first offending
# position, because a sequence usually comes from a file somebody else made.
function from_text(text: borrowed String, alphabet: Alphabet)
    -> (DnaSequence, AlphabetError?)

# Not `†` (§4.7): complementarity is pinned by chemistry and the IUPAC
# ambiguity codes have not moved since 1985. The most tempting tier 2
# candidate in the catalogue, and still tier 3 — §3.4.
function reverse_complement(sequence: borrowed DnaSequence) -> DnaSequence

# Reading a FASTA is streaming, not slurping: a reference genome does not fit
# in memory and `collections-and-chains.md`'s `Iterate` is the right shape.
function read_fasta(path: borrowed Path) -> (Iterate of SequenceRecord, IoError?)

# Phred encoding is `~`: Sanger, Illumina 1.3 and Illumina 1.5 use three
# offsets and the file does not say which.
function phred_from_char(character: U8, encoding: QualityEncoding) -> (Int, RangeError?)
```

**Example.**

```science
use bio.seq

function main():
    let records, err be seq.read_fasta(Path.new("genome.fa"))
    if err?:
        print(f"could not open: {err.message()}")
        return

    for record in records:
        let rc be seq.reverse_complement(record.sequence())
        print(f"{record.id()}: {record.sequence().length()} bp")
```

### 10.2 Transcription, translation and the genetic codes

**Types.** `GeneticCode` `Codon` `CodonTable` `ReadingFrame`
`OpenReadingFrame` `TranslationResult` `StartCodonPolicy`

**The codes, all `†`.** `ncbi †` `standard †` `vertebrate_mitochondrial †`
`yeast_mitochondrial †` `mould_mitochondrial †` `invertebrate_mitochondrial †`
`ciliate_nuclear †` `bacterial_and_plastid †` `alternative_yeast †`
`ascidian_mitochondrial †` `all_codes †` `code_by_id †` `code_name †`
`start_codons †` `stop_codons †` `is_start †` `is_stop †`
`codon_to_amino_acid †` `amino_acids_for †` `synonymous_codons †`
`degeneracy †`

**Transcription.** `transcribe` `back_transcribe` `splice` `remove_introns`
`mrna_from_gene` `polyadenylate` `cap`

**Translation.** `translate` `translate_frame` `translate_all_frames`
`translate_to_stop` `open_reading_frames ~` `longest_orf ~`
`find_genes_naive ~` `six_frame_translation` `codon_at` `codons`
`frame_of` `is_in_frame`

**Codon usage.** `codon_usage` `codon_usage_table †` `relative_synonymous_usage`
`codon_adaptation_index †` `effective_number_of_codons` `gc3_content`
`codon_bias_distance` `optimise_codons ~ †` `harmonise_codons ~ †`

> **Decision 10.2. `translate` takes the genetic code as a required argument.
> There is no default and no `translate(sequence)` overload. This is Decision 4.5
> applied, and `scientific-libraries.md` §11.7 already wrote the signature this
> way.**

The argument in full is in §4.5 and it is the strongest single case for explicit
data in the catalogue: translating mitochondrial or *Mycoplasma* DNA with the
standard code does not fail, it produces a plausible protein of the right length
that is wrong, because `TGA` is a stop in table 1 and tryptophan in tables 2, 3,
4, 5 and 9.

**The default that does exist is spelled `STANDARD`, not `DEFAULT`**, so a
program that genuinely wants table 1 says the word and a reader can see it said.

**Cost.** Every translation call is one argument longer, forever. Roughly fifteen
characters, against a class of silent wrong answers that is currently detected by
someone noticing the protein looks odd.

**Five signatures.**

```science
# The code is required. `†` through the NCBI table release.
function translate(sequence: borrowed DnaSequence, code: borrowed GeneticCode)
    -> (ProteinSequence, TranslationError?)

# Frames are 0, 1, 2 on the forward strand and 3, 4, 5 on the reverse. An
# out-of-range frame is a programming error, so it is an error return and not
# a panic — the frame usually comes from a loop bound somebody wrote.
function translate_frame(
    sequence: borrowed DnaSequence,
    frame: Int,
    code: borrowed GeneticCode,
) -> (ProteinSequence, FrameError?)

# ORF finding is `~`: minimum length, whether to require a start codon, and
# whether nested ORFs count are three choices with three different answers.
function open_reading_frames(
    sequence: borrowed DnaSequence,
    code: borrowed GeneticCode,
    minimum_length: Int,
    start_policy: StartCodonPolicy,
) -> Array of OpenReadingFrame

# The CAI reference set is `†` and organism-specific, and it is an argument
# because "the" codon usage of an organism depends on which genes you counted.
function codon_adaptation_index(
    sequence: borrowed DnaSequence,
    reference: borrowed CodonTable,
    code: borrowed GeneticCode,
) -> (F64, LengthError?)

function transcribe(sequence: borrowed DnaSequence) -> RnaSequence
```

**Example.** The mistake Decision 10.2 prevents, and the fix:

```science
use bio.seq
use bio.code

function main():
    let mito, err be seq.from_text("ATGTGAGGCTAA", seq.dna_alphabet())
    if err?:
        return

    # Wrong: TGA is a stop in the standard code.
    let wrong, _ be code.translate(mito, code.standard())

    # Right: in the vertebrate mitochondrial code TGA is tryptophan.
    let right, translate_err be code.translate(mito, code.vertebrate_mitochondrial())
    if translate_err?:
        print("could not translate")
        return

    print(f"{wrong.to_text()} vs {right.to_text()}")
```

### 10.3 Composition and complexity

**Composition.** `base_counts` `base_frequencies` `gc_content` `at_content`
`gc_skew` `at_skew` `cumulative_gc_skew` `purine_content` `gc3_content`
`amino_acid_counts` `amino_acid_frequencies` `molecular_weight_sequence †`
`isoelectric_point ~ †` `net_charge_at_ph ~ †` `extinction_coefficient †`
`instability_index †` `aliphatic_index` `hydropathy ~ †` `gravy †`
`aromaticity` `flexibility ~ †` `secondary_structure_propensity ~ †`

**Melting and hybridisation.** `melting_temperature ~ †`
`melting_temperature_wallace` `melting_temperature_nearest_neighbour ~ †`
`salt_correction ~` `formamide_correction` `hybridisation_free_energy †`
`self_complementarity` `hairpin_potential` `primer_dimer_potential`

**k-mers.** `kmer_counts` `kmer_frequencies` `kmer_spectrum` `canonical_kmers`
`minimiser ~` `syncmer ~` `kmer_distance ~` `jaccard_from_kmers`
`minhash_sketch ~` `bloom_from_kmers ~` `distinct_kmer_estimate ~`

**Complexity.** `entropy_of_sequence` `linguistic_complexity`
`dust_score ~` `low_complexity_regions ~` `tandem_repeats ~`
`homopolymer_runs` `compression_complexity ~` `wootton_federhen_complexity ~`

**Windows.** `sliding_window` `window_gc` `window_entropy`
`window_statistic` `cumulative_statistic`

**`window`, not `kernel`.** §6.2: a smoothing kernel over a chromosome is
`window`, and it reverts to `kernel` if `reserved-words.md` §5 item 2 lands.
Biology uses "window" for a genuinely different thing — a genomic interval — so
this rename is worse here than in `signal`, because it collides with an existing
term of art rather than merely being a second choice.

**Five signatures.**

```science
function gc_content(sequence: borrowed DnaSequence) -> F64

# Tm is `~` and `†`: four published methods, and the nearest-neighbour method
# reads a thermodynamic table. The bare name is the one people ask for, and it
# is documented as an alias for the nearest-neighbour method with stated salt.
function melting_temperature_nearest_neighbour(
    sequence: borrowed DnaSequence,
    sodium: Concentration of F64,
    primer: Concentration of F64,
) -> (Temperature of F64, TooShortError?)

# Generic over the sequence type: k-mer counting is the same operation on DNA,
# RNA and protein, and the alphabet comes from the value.
function kmer_counts of T(sequence: borrowed T, k: Int)
    -> (Map of (String, Int), KTooLargeError?) where T: Sequence

function entropy_of_sequence of T(sequence: borrowed T) -> F64 where T: Sequence

# A window is a genomic interval here, and the statistic is a closure, which is
# the `function(T) -> U` spelling `scientific-libraries.md` §14.2 asks for.
function window_statistic of T(
    sequence: borrowed T,
    width: Int,
    step: Int,
    statistic: function(borrowed T) -> F64,
) -> Array of F64 where T: Sequence
```

**Example.**

```science
use bio.seq
use bio.composition

function main():
    let s, err be seq.from_text("GATTACAGATTACA", seq.dna_alphabet())
    if err?:
        return
    print(f"GC = {composition.gc_content(s):.3}")
    print(f"H  = {composition.entropy_of_sequence(s):.3}")
```

### 10.4 Alignment and the scoring matrices

**Types.** `Alignment` `AlignmentColumn` `AlignOperation` `Cigar` `Scoring`
`GapModel` `SubstitutionMatrix` `MultipleAlignment` `Profile` `GuideTree`

**Pairwise.** `align_global` `align_local` `align_semiglobal` `align_overlap`
`align_free_ends` `AlignMethod.NeedlemanWunsch` `AlignMethod.SmithWaterman`
`AlignMethod.Gotoh` `AlignMethod.Hirschberg` `AlignMethod.Banded` `align_affine` `align_linear_gaps`
`align_with_traceback` `score_only` `best_local_scores`

**Distances.** `hamming_distance` `edit_distance` `levenshtein_distance`
`damerau_levenshtein_distance` `weighted_edit_distance` `pairwise_identity ~`
`percent_identity ~` `similarity_fraction` `normalised_score`
`bit_score` `expect_value ~`

**Multiple.** `align_multiple ~` `progressive_align ~` `profile_align`
`iterative_refine ~` `consensus ~` `column_conservation` `gap_fraction`
`alignment_to_profile` `trim_alignment ~` `remove_gap_columns`
`sum_of_pairs_score` `column_score`

**Matrices, all `†`.** `blosum_matrix †` `pam_matrix †` `nucleotide_matrix †`
`identity_matrix` `transition_transversion_matrix` `gonnet_matrix †`
`jtt_matrix †` `wag_matrix †` `lg_matrix †` `matrix_entry †` `matrix_alphabet †`
`expected_score †` `relative_entropy †` `matrix_scale †`

**Decision — a matrix is named by its provenance, not its number.**

> **Decision 10.4. There is no `blosum(62)`. The matrices are
> `matrices.blosum.ncbi_62`, `matrices.blosum.corrected_62`,
> `matrices.pam.dayhoff_250`, and `align_local` takes a `Scoring` with no
> default.**

This is Decision 4.5's second half and §4.1's BLOSUM62 row is the argument:
NCBI's BLOSUM62 was computed from a miscounted clustering, the error stood for
fifteen years, the corrected matrix performs measurably *worse*, and both are in
current use. **The number does not identify the matrix.** A default would have to
pick one silently and would be wrong for half the literature either way.

**Rejected: default to `ncbi_62`** on the grounds that it is what everyone uses.
It is, and that is a reason to make it easy to name, not a reason to make it
invisible. A result computed with the miscomputed matrix is still a valid result —
it is comparable to thirty years of published alignments, which is often the
point — but it must be *stated*, and the run record states it (Decision 4.4).

**Cost.** One argument, and a new user must make a decision before their first
alignment. Mitigated by the module documentation naming `ncbi_62` as the
conventional choice and saying why in one sentence.

**Five signatures.**

```science
# N1 and N2 (§1.5): the operation is `align_local`, the algorithm is an
# argument, and no eponym appears in either. Generic over the sequence type,
# because the same dynamic program aligns DNA and protein and the alphabet
# comes from the scoring. This replaces `scientific-libraries.md` §11.7's
# `smith_waterman of T(…)`, which §13 asks that note to amend.
function align_local of T(
    first: borrowed T,
    second: borrowed T,
    scoring: borrowed Scoring,
    method: AlignMethod,
) -> Alignment where T: Sequence

# Affine gaps are the default in practice, and the model is a value so that
# open and extend cannot be swapped by position.
function align_global of T(
    first: borrowed T,
    second: borrowed T,
    matrix: borrowed SubstitutionMatrix,
    gaps: GapModel,
) -> Alignment where T: Sequence

# Hamming needs equal lengths, which is a precondition the type cannot carry
# for runtime-length sequences, so it is an error return.
function hamming_distance of T(first: borrowed T, second: borrowed T)
    -> (Int, LengthMismatch?) where T: Sequence

# Progressive MSA needs a guide tree, which is itself a choice; passing it in
# rather than building it silently is what makes the result reproducible.
function progressive_align of T(
    sequences: borrowed Array of T,
    guide: borrowed GuideTree,
    scoring: borrowed Scoring,
) -> (MultipleAlignment, AlignError?) where T: Sequence

# `matches`, not `match` (§6.2). Percent identity is `~`: the denominator can
# be the alignment length, the shorter sequence, or the ungapped columns.
function percent_identity(alignment: borrowed Alignment, denominator: IdentityBasis) -> F64
```

**Example.**

```science
use bio.seq
use bio.align

function main():
    let scoring be align.Scoring.new(
        align.matrices.blosum.ncbi_62,
        align.GapModel.affine(-11, -1),
    )

    let hit be align.align_local(query, subject, scoring, align.AlignMethod.Gotoh)
    print(f"score {hit.score()}, identity {align.percent_identity(hit, align.ALIGNED_COLUMNS):.1}%")
```

### 10.5 Motifs and position weight matrices

**Types.** `Motif` `PositionCountMatrix` `PositionFrequencyMatrix`
`PositionWeightMatrix` `Background` `MotifHit` `Pseudocount`

**Construction.** `counts_from_sites` `frequencies_from_counts`
`weights_from_frequencies ~` `pwm_from_sites ~` `pseudocount_laplace`
`pseudocount_bayesian ~` `pseudocount_from_background`
`reverse_complement_pwm` `trim_motif ~` `pad_motif`

**Scoring and scanning.** `score_motif` `score_at` `scan_sequence`
`scan_both_strands` `best_hit` `hits_above ~` `score_distribution`
`p_value_of_score ~` `threshold_from_p_value ~` `threshold_from_fpr ~`
`log_odds` `information_content` `total_information` `consensus_of`
`degenerate_consensus ~` `sequence_logo_data`

**Discovery.** `discover_motifs ~` `MotifSearch.Enumerate` `MotifSearch.Gibbs` `MotifSearch.ExpectationMaximisation`
`meme_like ✗` `motif_similarity ~` `cluster_motifs ~` `align_motifs ~`

**Literal patterns.** `find_motif` `find_all` `find_with_mismatches`
`find_ambiguous` `prosite_pattern` `regex_from_iupac`

**Not shipped.** `✗ meme_like` under that name — the MEME algorithm's behaviour
is specified by its implementation and its objective function has changed between
versions. `expectation_maximisation` with explicit parameters is shipped and is
the honest form of the same thing.

**The background is never implicit.** A PWM's log-odds scores are relative to a
background nucleotide composition, and scanning a GC-rich genome with a uniform
background inflates every score. `weights_from_frequencies` takes a `Background`
and there is no zero-argument form. This is the same decision as §4.5 in a case
where the "table" is computed from the user's own data rather than shipped —
which is why it is a `~` and not a `†`.

**Five signatures.**

```science
# Pseudocounts and the background are both required: a PWM with neither is a
# division by zero waiting for its first unobserved base.
function weights_from_frequencies(
    frequencies: borrowed PositionFrequencyMatrix,
    background: borrowed Background,
    pseudocount: Pseudocount,
) -> PositionWeightMatrix

function scan_sequence(
    pwm: borrowed PositionWeightMatrix,
    sequence: borrowed DnaSequence,
    threshold: F64,
) -> Array of MotifHit

# The score distribution is exact by dynamic programming over discretised
# scores, which is why a p-value from a PWM does not need simulation.
function p_value_of_score(
    pwm: borrowed PositionWeightMatrix,
    background: borrowed Background,
    score: F64,
) -> PValue

function information_content(pwm: borrowed PositionWeightMatrix, position: Int)
    -> (F64, RangeError?)

# `find_motif` handles IUPAC ambiguity in the pattern, which is what separates
# it from an ordinary substring search.
function find_ambiguous(
    sequence: borrowed DnaSequence,
    pattern: borrowed String,
    maximum_mismatches: Int,
) -> (Array of Region, PatternError?)
```

**Example.**

```science
use bio.motif

function main():
    let pwm be motif.weights_from_frequencies(
        frequencies,
        motif.Background.from_sequence(genome),
        motif.pseudocount_laplace(),
    )

    let threshold be motif.threshold_from_p_value(pwm, background, 1.0e-4)
    for hit in motif.scan_both_strands(pwm, promoter, threshold):
        print(f"{hit.position()} {hit.strand()} {hit.score():.2}")
```

### 10.6 Phylogenetics

**Types.** `Tree` `Node` `Clade` `Branch` `Split` `DistanceMatrix`
`GuideTree` `RootPolicy` `BranchSupport`

**Distances.** `distance_matrix` `p_distance` `jukes_cantor_distance ~`
`kimura_two_parameter_distance ~` `tamura_distance ~` `tamura_nei_distance ~` `felsenstein_81_distance ~`
`hasegawa_kishino_yano_distance ~` `general_time_reversible ~` `log_det_distance ~`
`gamma_corrected ~` `protein_distance ~ †` `corrected_distance ~`
`saturation_test`

**Tree building.** `tree_from_distances ~` `TreeMethod.NeighbourJoining` `TreeMethod.Bionj` `TreeMethod.Upgma` `TreeMethod.Wpgma`
`TreeMethod.FitchMargoliash` `TreeMethod.MinimumEvolution` `maximum_parsimony ~`
`fitch_parsimony` `sankoff_parsimony` `branch_and_bound_parsimony`
`likelihood_of_tree` `optimise_branch_lengths` `nni_search ~`
`spr_search ~` `maximum_likelihood_tree ~`

**Support and comparison.** `bootstrap_tree ~` `jackknife_tree ~`
`approximate_likelihood_ratio` `consensus_tree ~` `majority_rule_consensus`
`strict_consensus` `robinson_foulds_distance` `weighted_robinson_foulds_distance`
`quartet_distance` `path_difference` `matching_split_distance`
`tree_certainty`

**Manipulation.** `root_at_midpoint` `root_with_outgroup` `unroot`
`ladderise` `sort_clades` `prune_taxa` `extract_clade` `collapse_short_branches`
`resolve_polytomies ~` `is_binary` `is_ultrametric` `tree_length`
`total_path_length` `node_depths` `tip_labels` `most_recent_common_ancestor`
`monophyly_test`

**Time and coalescent.** `molecular_clock_test` `strict_clock_dates ~`
`relaxed_clock_dates ~` `coalescent_likelihood ~` `skyline_plot ~`
`birth_death_likelihood ~` `lineage_through_time`

**Files.** `read_newick` `write_newick` `read_nexus` `write_nexus`
`read_phyloxml` `write_phyloxml`

**Not shipped.** `✗ bayesian_tree` under that name — MCMC tree inference is
MrBayes and BEAST, and both are research programs whose priors, proposals and
convergence diagnostics are the science rather than the implementation. §12.2.

**`model` is the word this module cannot use.** GTR, HKY and JC69 are called
*models of evolution* by every textbook and every paper; `let model be
phylo.general_time_reversible(...)` is the natural line and does not compile.
The catalogue uses `scheme` for the binding and keeps the eponyms as function
names. §6.2.

**Five signatures.**

```science
# N2 (§1.5): `neighbour_joining`, `upgma` and `bionj` take the same distance
# matrix and return the same type, differing in the joining criterion, so the
# method is an argument. This replaces `scientific-libraries.md` §11.7's
# `neighbour_joining of (const N: Int)(…)`; §13 asks that note to amend it.
# The shape is still checked: the matrix must be symmetric and its size must
# match the label count.
function tree_from_distances of (const N: Int)(
    distances: borrowed Symmetric of (F64, N),
    labels: borrowed Array of String,
    method: TreeMethod,
) -> (Tree, TreeError?)

# N3, not N2: JC69 and K2P are two *models* and give two different numbers, so
# the eponym is in the name and they are two functions.
function jukes_cantor_distance(observed: F64) -> (F64, SaturatedError?)

# Robinson–Foulds needs matching leaf sets, which is the error case that
# actually happens, so it is in the return type.
function robinson_foulds_distance(first: borrowed Tree, second: borrowed Tree)
    -> (Int, LeafSetMismatch?)

# Bootstrap is `~` in the replicate count and needs a random key, which is
# taken by value so that reuse is a compile error (`scientific-libraries.md`
# §7.9's argument, applied here).
function bootstrap_tree of T(
    alignment: borrowed MultipleAlignment,
    replicates: Int,
    key: RandomKey,
    build: function(borrowed DistanceMatrix) -> (Tree, TreeError?),
) -> (Tree, TreeError?)

function read_newick(text: borrowed String) -> (Tree, NewickError?)
```

**Example.**

```science
use bio.phylo

function main():
    let distances be phylo.distance_matrix(alignment, phylo.KIMURA_TWO_PARAMETER)
    let tree, err be phylo.tree_from_distances(
        distances, labels, phylo.TreeMethod.NeighbourJoining)
    if err?:
        print(f"could not build a tree: {err.message()}")
        return

    print(phylo.write_newick(phylo.ladderise(phylo.root_at_midpoint(tree))))
```

### 10.7 Molecular evolution

**Substitution models.** `jc69_matrix` `k80_matrix` `f81_matrix` `hky85_matrix` `tn93_matrix` `gtr_matrix`
`rate_matrix` `equilibrium_frequencies` `transition_probabilities`
`gamma_rate_categories ~` `invariant_sites` `free_rates ~`
`model_selection ~` `akaike_for_model` `bayesian_information_for_model`
`likelihood_ratio_test`

**Codon models.** `codon_frequencies ~` `goldman_yang_matrix ~` `muse_gaut_matrix ~`
`omega` `dn_ds ~ †` `nei_gojobori_dn_ds ~ †` `yang_nielsen_dn_ds ~ †`
`counting_method ~ †` `site_model ~` `branch_model ~`
`branch_site_model ~` `positive_selection_sites ~`

**Rates and clocks.** `substitution_rate` `synonymous_rate †`
`nonsynonymous_rate †` `relative_rate_test` `tajima_relative_rate`
`rate_heterogeneity` `covarion_test` `saturation_plot`

**Ancestral inference.** `ancestral_states_parsimony ~`
`ancestral_states_likelihood ~` `marginal_reconstruction`
`joint_reconstruction` `state_changes_on_branch`

**Selection at the protein level.** `hydrophobicity_change †`
`grantham_distance †` `miyata_distance †` `radical_conservative ~ †`
`epistasis_pairs ~`

**`dn_ds` is `†` and `~` at once**, which is worth naming because it is the
catalogue's clearest instance of both markers on one function: it needs a genetic
code (a table) *and* a counting convention (a method), and the two leading
conventions — Nei–Gojobori and Yang–Nielsen — disagree by enough to change
conclusions.

**Five signatures.**

```science
# Two conventions, two functions, per N2. Both take the code explicitly.
function nei_gojobori_dn_ds(
    first: borrowed DnaSequence,
    second: borrowed DnaSequence,
    code: borrowed GeneticCode,
) -> (F64, DnDsError?)

# A rate matrix is a value with row sums of zero; constructing it can fail on
# frequencies that do not sum to one.
function gtr(
    exchangeabilities: borrowed Array of F64,
    frequencies: borrowed Array of F64,
) -> (RateMatrix, ModelError?)

# The matrix exponential is `linalg`'s, and this is the function every
# likelihood calculation calls a million times.
function transition_probabilities(
    rates: borrowed RateMatrix,
    time: F64,
) -> Matrix of (F64, 4, 4)

# Model selection is `~` in the criterion and returns the ranked list rather
# than one winner, because "the best model" is a choice the user makes.
function model_selection(
    alignment: borrowed MultipleAlignment,
    candidates: borrowed Array of RateMatrix,
    criterion: Criterion,
) -> Array of (RateMatrix, F64)

function ancestral_states_likelihood(
    tree: borrowed Tree,
    alignment: borrowed MultipleAlignment,
    rates: borrowed RateMatrix,
) -> (Map of (Node, Array of F64), ReconstructionError?)
```

**Example.**

```science
use bio.evolution
use bio.code

function main():
    let ratio, err be evolution.nei_gojobori_dn_ds(gene_a, gene_b, code.standard())
    if err?:
        print("cannot compute dN/dS on these sequences")
        return
    if ratio > 1.0:
        print(f"dN/dS = {ratio:.2} — consistent with positive selection")
```

### 10.8 Population genetics

**Frequencies and equilibrium.** `allele_frequency` `genotype_frequency`
`allele_counts` `genotype_counts` `hardy_weinberg_expected`
`hardy_weinberg_test` `hardy_weinberg_exact` `inbreeding_coefficient`
`wright_f_statistics` `f_is` `f_st` `f_it` `g_st` `jost_d` `nei_d`
`reynolds_distance` `cavalli_sforza_distance`

**Diversity.** `heterozygosity_observed` `heterozygosity_expected`
`nucleotide_diversity` `watterson_theta` `theta_pi` `theta_h`
`allelic_richness ~` `private_alleles` `segregating_sites`
`haplotype_diversity` `haplotype_count` `effective_number_of_alleles`

**Neutrality tests.** `tajima_d` `fu_li_d` `fu_li_f` `fay_wu_h`
`zeng_e` `fu_fs` `ewens_watterson_test` `hudson_kreitman_aguade_test`
`mcdonald_kreitman_test` `neutrality_index` `direction_of_selection`

**Linkage.** `linkage_disequilibrium` `d_prime` `r_squared_ld`
`ld_decay ~` `haplotype_blocks ~` `four_gamete_test`
`recombination_rate ~` `hudson_kaplan_minimum` `extended_haplotype_homozygosity`
`integrated_ehh` `ihs` `xp_ehh` `nsl`

**Demography and selection.** `site_frequency_spectrum` `folded_spectrum`
`unfolded_spectrum ~` `effective_population_size ~` `ne_from_heterozygosity`
`ne_from_ld` `ne_temporal` `selection_coefficient` `fitness_from_frequencies`
`wright_fisher_step` `wright_fisher_trajectory` `moran_step`
`fixation_probability` `time_to_fixation` `mutation_selection_balance`
`admixture_proportions ~` `f3_statistic` `f4_statistic` `d_statistic`
`treemix_like ✗`

**Structure.** `pca_of_genotypes` `admixture_like ~` `structure_like ~`
`identity_by_state` `identity_by_descent ~` `kinship_matrix ~`
`relatedness ~` `runs_of_homozygosity ~`

**Not shipped.** `✗ treemix_like`, `✗ structure_like` under those names —
graphical-model demographic inference is a research program per implementation.
`admixture_like` with explicit `K` and an explicit optimiser is shipped, because
it is a constrained matrix factorisation and that is a stated algorithm.

**`null allele` is a term of art this module cannot bind.** §6.3: a null allele —
one that fails to amplify and is therefore scored as a homozygote — is standard
genotyping vocabulary, and `null` is a keyword since `syntax-revision-2.md` §3.1.
The catalogue uses `dropout_allele`, which is a real synonym and is arguably
clearer, so this is the least painful of the three new collisions.

**Five signatures.**

```science
# `scientific-libraries.md` §11.7's signature, in revision-2 syntax. Tajima's D
# is undefined below four sequences and the error says so.
function tajima_d(sequences: borrowed Array of DnaSequence)
    -> (F64, TooFewSequences?)

# Hardy–Weinberg yields a PValue, not an F64, so it cannot be thresholded
# before correction — `statistical-validity.md` Decision 1.
function hardy_weinberg_test(observed: borrowed Map of (Genotype, Int))
    -> (PValue, TooFewGenotypes?)

function nucleotide_diversity(sequences: borrowed Array of DnaSequence)
    -> (F64, TooFewSequences?)

# Fst is `~`: Wright's, Weir–Cockerham's and Hudson's estimators are three
# numbers, and which one a paper used is the first question a reviewer asks.
function f_st_weir_cockerham(
    populations: borrowed Array of Array of Genotype,
) -> (F64, StructureError?)

# The Wright–Fisher step takes the random key by value, so a reused key is a
# compile error rather than a silently repeated trajectory.
function wright_fisher_step(
    frequency: F64,
    population_size: Int,
    selection: F64,
    key: RandomKey,
) -> F64
```

**Example.**

```science
use bio.popgen
use stats

function main():
    let mutable pvalues be Array of PValue.new()
    for locus in loci:
        let p, err be popgen.hardy_weinberg_test(locus.genotypes())
        if err?:
            continue
        pvalues.push(p)

    # Only an AdjustedPValue may be thresholded.
    for adjusted in stats.holm(pvalues):
        if adjusted.below(0.05):
            print("departure from equilibrium")
```

### 10.9 Gene expression and differential analysis

**Types.** `ExpressionMatrix` `CountMatrix` `SampleTable` `Design`
`Contrast` `DifferentialResult` `Dispersion` `SizeFactors`

**Normalisation.** `counts_per_million` `tpm` `fpkm` `rpkm`
`median_of_ratios` `trimmed_mean_of_m ~` `upper_quartile`
`quantile_normalise` `rlog ~` `variance_stabilising ~`
`size_factors ~` `spike_in_normalise` `gc_bias_correct ~`
`length_correct` `batch_correct ~` `remove_unwanted_variation ~`

**Filtering.** `filter_low_counts ~` `filter_by_expression ~`
`filter_variable_genes ~` `detected_genes` `library_sizes`
`complexity_curve`

**Dispersion and models.** `estimate_dispersion ~` `dispersion_trend ~`
`shrink_dispersion ~` `negative_binomial_fit` `quasi_likelihood_fit`
`limma_voom_weights` `design_matrix` `contrast_matrix`

**Differential expression.** `differential_expression ~`
`wald_test` `likelihood_ratio_test` `exact_test`
`moderated_t_test` `shrink_log_fold_change ~`
`independent_filtering ~` `volcano_data` `ma_data`
`top_genes ~` `summarise_results`

**Enrichment.** `over_representation` `hypergeometric_test`
`gene_set_enrichment ~` `running_sum_statistic` `leading_edge`
`pathway_score ~` `single_sample_gsea ~`
`read_gmt` `write_gmt`

**Clustering.** `cluster_samples ~` `cluster_genes ~`
`correlation_matrix ~` `sample_distance ~` `hierarchical_from_distances ~`
`consensus_cluster ~` `silhouette`

**Decision — the gene sets are the user's file, never a shipped database.**

> **Decision 10.9. `bio.expression` ships `read_gmt` and the enrichment
> algorithms and ships **no** gene-set database. GO, KEGG, Reactome and MSigDB
> are not data packages in this project.**

Two reasons, and the second is the binding one. The releases are monthly and the
answer changes with every one, which §4 could handle. But KEGG is not
redistributable for commercial use, MSigDB's licence is non-commercial for parts
of its collection, and a package manager that ships them acquires a licensing
obligation on behalf of every user who types `use`. `package-manager.md` has no
mechanism for a per-package licence gate and should not acquire one for this.

**Cost.** A user must download a GMT file themselves, which is one manual step
and is what every enrichment tool already requires.

**`PValue` all the way through.** Every test here returns
`statistical-validity.md`'s `PValue`, every correction returns
`AdjustedPValue`, and `volcano_data` takes `AdjustedPValue`. A differential
expression pipeline is the single largest producer of uncorrected p-values in
science and it is the exact case that note was written for.

**Five signatures.**

```science
# A DE result carries adjusted p-values by construction: there is no way to
# get a `DifferentialResult` whose p-values have not been through a correction.
function differential_expression(
    counts: borrowed CountMatrix,
    design: borrowed Design,
    contrast: borrowed Contrast,
    correction: Correction,
) -> (DifferentialResult, ModelError?)

# `†`-free and `~`: three normalisations with three different answers, three
# functions, per N2.
function median_of_ratios(counts: borrowed CountMatrix)
    -> (SizeFactors, ZeroGeneError?)

# TPM needs gene lengths, and forgetting them is the classic error that turns
# TPM into CPM; making them an argument makes forgetting impossible.
function tpm(counts: borrowed CountMatrix, lengths: borrowed Array of Int)
    -> (ExpressionMatrix, LengthMismatch?)

# ORA over a user-supplied gene set. Returns a PValue; thresholding it before
# correction does not compile.
function over_representation(
    selected: borrowed Set of GeneId,
    universe: borrowed Set of GeneId,
    gene_set: borrowed Set of GeneId,
) -> (PValue, EmptySetError?)

# GSEA's permutation scheme is `~` — gene permutation and phenotype
# permutation give different nulls — and it takes a key by value.
function gene_set_enrichment(
    ranked: borrowed Array of (GeneId, F64),
    gene_sets: borrowed Map of (String, Set of GeneId),
    permutations: Int,
    scheme: PermutationScheme,
    key: RandomKey,
) -> (Array of EnrichmentResult, EnrichmentError?)
```

**Example.**

```science
use bio.expression
use stats

function main():
    let factors, size_err be expression.median_of_ratios(counts)
    if size_err?:
        print("a gene has a zero in every sample")
        return

    let result, err be expression.differential_expression(
        counts, design, contrast, stats.BENJAMINI_HOCHBERG)
    if err?:
        print(f"model failed: {err.message()}")
        return

    for gene in expression.top_genes(result, 20):
        print(f"{gene.id()} {gene.log_fold_change():.2} {gene.adjusted()}")
```

### 10.10 Single-cell

**Types.** `CellMatrix` `CellId` `GeneId` `Embedding` `Neighbourhood`
`ClusterAssignment` `Trajectory` `Pseudotime` `CellTypeScore`

**Quality control.** `cells_per_gene` `genes_per_cell` `mitochondrial_fraction`
`ribosomal_fraction` `filter_cells ~` `filter_genes ~` `doublet_score ~`
`ambient_rna_estimate ~` `empty_droplet_test ~` `saturation_curve`

**Normalisation.** `normalise_total` `log_normalise` `scran_pooling ~`
`sctransform_like ~` `regress_out ~` `scale_features ~`
`highly_variable_genes ~` `select_features ~`

**Dimensionality.** `pca_cells` `truncated_svd` `harmony_like ~`
`nearest_neighbours ~` `shared_nearest_neighbours` `embedding ~`
`EmbeddingMethod.Umap` `EmbeddingMethod.Tsne` `EmbeddingMethod.DiffusionMap` `force_directed_layout ~`

**Clustering.** `communities ~` `CommunityMethod.Leiden` `CommunityMethod.Louvain` `graph_from_neighbours ~`
`cluster_resolution_sweep` `cluster_stability ~` `marker_genes ~`
`rank_genes_by_group ~` `score_gene_set` `cell_type_score ~`
`label_transfer ~`

**Trajectories.** `pseudotime ~` `principal_curve ~` `minimum_spanning_graph`
`rna_velocity ✗` `branch_detection ~` `expression_along_pseudotime`
`fit_smooth_trend ~`

**Integration.** `integrate_batches ~` `mutual_nearest_neighbours ~`
`anchor_transfer ~` `batch_mixing_metric` `local_inverse_simpson`

**Not shipped.** `✗ rna_velocity` — the estimate depends on the spliced/unspliced
quantification upstream, the kinetic model, and a steady-state assumption whose
validity is actively disputed; there are three incompatible formulations in
current use and none is settled. Shipping it under one name would pick a side in
an open scientific argument.

**Every name in this section is `~`.** That is not an accident of drafting: single
cell analysis is, at this date, a field of methods rather than a field of
results, and a library that hid the method behind a clean name would be claiming
a consensus that does not exist. The `~` marker exists for exactly this and this
is the section where it applies to everything.

**Five signatures.**

```science
# N2: Leiden and Louvain differ in the refinement step and nothing else, so
# the method is an argument. `~` in the resolution, which changes the number of
# clusters and is the single most consequential free parameter in the field.
function communities(
    graph: borrowed Neighbourhood,
    resolution: F64,
    method: CommunityMethod,
    key: RandomKey,
) -> (ClusterAssignment, GraphError?)

function highly_variable_genes(
    matrix: borrowed CellMatrix,
    count: Int,
    method: VariabilityMethod,
) -> (Array of GeneId, TooFewGenesError?)

# N2: UMAP, t-SNE and diffusion maps take the same components and return the
# same type, so the method is an argument. The embedding is a value with its
# parameters attached, so a plot can say how it was made.
function embedding of (const N: Int, const K: Int)(
    components: borrowed Matrix of (F64, N, K),
    neighbours: Int,
    minimum_distance: F64,
    method: EmbeddingMethod,
    key: RandomKey,
) -> (Embedding, EmbeddingError?)

# Marker genes yields PValues, and the correction is a required argument
# because a marker list is the most over-thresholded object in the field.
function marker_genes(
    matrix: borrowed CellMatrix,
    clusters: borrowed ClusterAssignment,
    test: MarkerTest,
    correction: Correction,
) -> (Map of (ClusterId, Array of (GeneId, AdjustedPValue)), MarkerError?)

function mitochondrial_fraction(
    matrix: borrowed CellMatrix,
    mitochondrial: borrowed Set of GeneId,
) -> Array of F64
```

**Example.**

```science
use bio.singlecell

function main():
    let variable, err be singlecell.highly_variable_genes(
        matrix, 2000, singlecell.SEURAT_V3)
    if err?:
        return

    let components be singlecell.pca_cells(matrix, variable, 50)
    let graph be singlecell.nearest_neighbours(components, 15)
    let clusters, cluster_err be singlecell.communities(
        graph, 1.0, singlecell.CommunityMethod.Leiden, key)
    if cluster_err?:
        print("clustering failed")
        return
    print(f"{clusters.count()} clusters")
```

### 10.11 Pathways and networks

**Types.** `Network` `Node` `Edge` `EdgeWeight` `Pathway` `Reaction`
`Community` `Centrality`

**Construction.** `network_from_edges` `network_from_adjacency`
`network_from_correlations ~` `co_expression_network ~`
`weighted_network ~` `signed_network` `bipartite_projection`
`merged` `intersect` `subtract` `induced_subgraph` `largest_component`

**Topology.** `degree` `weighted_degree` `degree_distribution`
`clustering_coefficient` `transitivity` `path_length` `shortest_paths`
`diameter` `betweenness` `closeness` `eigenvector_centrality`
`pagerank` `katz_centrality` `hub_score` `authority_score`
`assortativity` `modularity` `rich_club`

**Communities.** `communities ~` `CommunityMethod.Louvain` `CommunityMethod.Leiden` `CommunityMethod.LabelPropagation`
`CommunityMethod.Spectral` `CommunityMethod.GirvanNewman` `clique_percolation ~`
`community_significance ~` `module_eigengene` `module_membership`

**Motifs and comparison.** `network_motifs ~` `triad_census`
`graph_edit_distance ~` `network_alignment ~` `degree_preserving_shuffle`
`configuration_model` `null_comparison ~`

**Pathways.** `read_sbml` `read_biopax` `read_kgml` `read_gpml`
`pathway_topology_score ~` `impact_analysis ~` `signal_propagation ~`
`flux_balance_analysis ~ ‡` `elementary_flux_modes ~`
`stoichiometry_matrix` `conserved_moieties`

**`merged`, not `union`** (§6.2). Set algebra on feature intervals and on gene
sets is the most-called operation in this module and it is the one that cannot
use its own word.

**`null_comparison`, not `null_model`** (§6.3).

**Flux balance is `‡`**: it is a linear program, and `optimize` will have a
simplex or interior-point solver, but the production-quality LP solvers are
decades of work in the same sense BLAS is. `scientific-libraries.md` §2's rule
applies unchanged.

**Five signatures.**

```science
# `merged`, not `union` (§6.2). Reverts to `union` if `reserved-words.md` §5
# item 2 lands.
function merged(first: borrowed Network, second: borrowed Network) -> Network

function betweenness(network: borrowed Network, normalised: Bool) -> Map of (Node, F64)

# N2 again, on a graph rather than a cell neighbourhood. Community detection
# is `~` and needs a key; the resolution changes the answer and is not hidden.
function communities(
    network: borrowed Network,
    resolution: F64,
    method: CommunityMethod,
    key: RandomKey,
) -> (Array of Community, GraphError?)

# A co-expression network is a thresholded correlation matrix, and the
# threshold is the whole method, so it is an argument with no default.
function co_expression_network(
    expression: borrowed ExpressionMatrix,
    correlation: CorrelationKind,
    threshold: F64,
) -> (Network, TooFewSamplesError?)

# FBA is linked: it is an LP, and the LP solver is the artefact.
function flux_balance_analysis(
    stoichiometry: borrowed Matrix of (F64, M, N),
    bounds: borrowed Array of (F64, F64),
    objective: borrowed Array of F64,
) -> (Array of F64, LinearProgramError?)
```

**Example.**

```science
use bio.network

function main():
    let net, err be network.co_expression_network(
        expression, network.SPEARMAN, 0.75)
    if err?:
        print("not enough samples for a stable correlation")
        return

    let modules, module_err be network.communities(
        net, 1.0, network.CommunityMethod.Louvain, key)
    if module_err?:
        return
    print(f"{modules.length()} modules, modularity {network.modularity(net, modules):.3}")
```

### 10.12 Protein structure

**Types.** `ProteinStructure` `Chain` `Residue` `AtomRecord` `SecondaryStructure`
`ContactMap` `Ramachandran` `Domain` `BindingSite` `ContactSurface`

**Files.** `read_pdb_structure` `read_mmcif` `read_pdbqt` `write_pdb_structure`
`write_mmcif` `fetch_structure ✗` `select_chain` `select_residues`
`remove_waters` `remove_hydrogens` `first_model` `all_models`
`occupancy_filter` `altloc_filter ~`

**Geometry.** `phi_psi` `omega_angle` `chi_angles` `ramachandran_data`
`ramachandran_outliers ~ †` `ca_coordinates` `backbone_atoms`
`sidechain_centroid` `distance_map` `contact_map ~` `residue_depth ~`
`solvent_accessibility ~ †` `relative_accessibility †` `burial_fraction`

**Secondary structure.** `secondary_structure ~` `dssp_like ~`
`stride_like ~` `helix_segments` `sheet_segments` `coil_segments`
`turn_segments` `secondary_structure_content` `helix_kink`
`sheet_topology`

**Interactions.** `hydrogen_bonds ~` `salt_bridges ~` `disulphide_bonds`
`hydrophobic_contacts ~` `pi_stacking ~` `cation_pi ~`
`contact_surface_area ~` `contact_surface_residues ~` `interface_energy ✗`
`binding_site_prediction ~ †`

**Comparison.** `superimpose ~` `rmsd_structure` `rmsd_per_residue`
`tm_score` `gdt_ts` `gdt_ha` `lddt` `q_score` `structural_alignment ~`
`fold_classification ✗`

**Ensembles and dynamics.** `radius_of_gyration` `end_to_end_distance`
`principal_component_analysis` `normal_modes ~` `elastic_network_model ~`
`b_factor_correlation` `rmsf` `read_trajectory ‡` `contact_lifetime`

**Not shipped.** `✗ fetch_structure` — a network call inside a library function
is an `external` effect that `--deterministic` must reject
(`reproducibility.md` §4.2), and a structure fetched today is not the structure
fetched next year, because the PDB revises entries. `✗ interface_energy` and
`✗ fold_classification` — both are predictors, both change per model release,
and `models-and-inference.md` owns anything that is a model.

**`interface` is the word this module cannot use** (§6.3): protein–protein
interface is the standard term and `interface` is a keyword since
`syntax-revision-2.md` §6. The catalogue uses `ContactSurface` and
`contact_surface_area`, which is a real synonym and slightly longer.

**Five signatures.**

```science
function read_pdb_structure(path: borrowed Path)
    -> (ProteinStructure, ParseError?)

# `~`: DSSP and STRIDE assign different secondary structure to the same
# coordinates, and which one a figure used is a real question.
function dssp_like(structure: borrowed ProteinStructure)
    -> (Array of SecondaryStructure, GeometryError?)

# TM-score is length-normalised and needs an alignment; passing an unaligned
# pair is the mistake, so the alignment is an argument.
function tm_score(
    mobile: borrowed ProteinStructure,
    reference: borrowed ProteinStructure,
    correspondence: borrowed Array of (Int, Int),
) -> (F64, LengthError?)

# `~` in the cutoff and in whether the contact is CA–CA, CB–CB or any-atom.
function contact_map(
    structure: borrowed ProteinStructure,
    cutoff: Length of F64,
    definition: ContactDefinition,
) -> ContactMap

# `ContactSurface`, not `Interface` (§6.3).
function contact_surface_residues(
    first: borrowed Chain,
    second: borrowed Chain,
    cutoff: Length of F64,
) -> Array of (Residue, Residue)
```

**Example.**

```science
use bio.structure
use chem.units

function main():
    let s, err be structure.read_pdb_structure(Path.new("1hho.pdb"))
    if err?:
        print(f"could not read: {err.message()}")
        return

    let contacts be structure.contact_surface_residues(
        s.chain("A"), s.chain("B"), 4.5<angstrom>)
    print(f"{contacts.length()} contacting residue pairs")
```

### 10.13 Ecology and biodiversity

**Types.** `Community` `AbundanceTable` `SpeciesId` `Site`
`RarefactionCurve` `Ordination`

**Alpha diversity.** `species_richness` `shannon_index` `simpson_index`
`inverse_simpson` `gini_simpson` `pielou_evenness` `berger_parker_index`
`fisher_alpha` `hill_number` `hill_profile` `chao1_richness` `chao2_richness` `ace_richness`
`ice_richness` `jackknife_richness ~` `good_turing_coverage`
`rarefaction ~` `extrapolation ~` `coverage_standardised ~`

**Beta diversity.** `bray_curtis_distance` `jaccard_index` `sorensen_index`
`morisita_horn_distance` `canberra_distance` `euclidean_community` `chord_distance`
`hellinger` `aitchison_distance ~` `unifrac_distance ~` `weighted_unifrac_distance ~`
`beta_diversity_matrix` `turnover_component` `nestedness_component`
`whittaker_beta`

**Ordination and testing.** `principal_coordinates` `nonmetric_scaling ~`
`correspondence_analysis` `canonical_correspondence ~`
`redundancy_analysis ~` `permanova ~` `anosim ~` `mantel_test ~`
`indicator_species ~` `species_accumulation ~`

**Population and community dynamics.** `logistic_growth` `exponential_growth`
`lotka_volterra_step` `competition_model` `functional_response ~`
`life_table` `leslie_matrix` `population_projection` `intrinsic_rate`
`generation_time` `reproductive_value` `elasticity_analysis`
`sensitivity_matrix`

**`unifrac_distance` is the one function here that reaches into `bio.phylo`**, which is
why §9 lists that dependency as optional: a user doing community ecology without
a phylogeny should not link the tree code.

**Five signatures.**

```science
function shannon_index(abundances: borrowed Array of F64) -> (F64, EmptyCommunity?)

# Hill numbers unify richness, Shannon and Simpson at q = 0, 1, 2, which is why
# they are one function with an order rather than three functions — the order
# is a continuous parameter, not a method choice, so N2's `~` does not apply.
function hill_number(abundances: borrowed Array of F64, order: F64)
    -> (F64, EmptyCommunity?)

function bray_curtis_distance(first: borrowed Array of F64, second: borrowed Array of F64)
    -> (F64, LengthMismatch?)

# UniFrac needs a tree whose tips match the species, which is the error that
# actually happens.
function weighted_unifrac_distance(
    first: borrowed Community,
    second: borrowed Community,
    tree: borrowed Tree,
) -> (F64, TipMismatch?)

# PERMANOVA yields a PValue from a permutation null and takes a key by value.
function permanova(
    distances: borrowed Symmetric of (F64, N),
    groups: borrowed Array of Int,
    permutations: Int,
    key: RandomKey,
) -> (PValue, DesignError?)
```

**Example.**

```science
use bio.ecology

function main():
    for site in sites:
        let h, err be ecology.shannon_index(site.abundances())
        if err?:
            continue
        print(f"{site.name()}: richness {ecology.species_richness(site.abundances())}, H' {h:.3}")
```

### 10.14 Epidemiology

**Types.** `CompartmentModel` `Compartment` `Flow` `EpidemicCurve`
`ContactMatrix` `SurvivalData` `RiskSet` `Cohort`

**Compartment models.** `sir` `sir_demography` `sis` `seir` `seirs`
`seir_age_structured` `sird` `vaccination_model` `waning_immunity_model`
`metapopulation ~` `stochastic_sir ~` `agent_based ✗`
`solve_epidemic ~` `equilibrium_prevalence` `epidemic_peak`
`final_size` `attack_rate` `herd_immunity_threshold`

**Reproduction numbers.** `basic_reproduction_number`
`next_generation_matrix` `effective_reproduction_number ~`
`instantaneous_reproduction ~` `case_reproduction ~`
`growth_rate_to_r ~` `serial_interval_distribution ~`
`generation_interval ~` `doubling_time` `renewal_equation`

**Measures of association.** `incidence_rate` `prevalence`
`cumulative_incidence` `risk_ratio` `rate_ratio` `odds_ratio`
`risk_difference` `attributable_fraction` `population_attributable_fraction`
`number_needed_to_treat` `standardised_rate ~` `age_standardise ~`
`confidence_interval_for ~`

**Survival analysis — re-exported, not reimplemented.** `kaplan_meier_curve`
`nelson_aalen_hazard` `log_rank_test` `cox_proportional_hazards`
`schoenfeld_residuals` `time_varying_covariates` `competing_risks ~`
`aalen_johansen_incidence` `parametric_survival ~` `accelerated_failure_time ~`
`restricted_mean_survival`

**Diagnostics and screening.** `sensitivity` `specificity`
`positive_predictive_value` `negative_predictive_value`
`likelihood_ratio_positive` `likelihood_ratio_negative`
`youden_index` `roc_data` `auc` `prevalence_adjusted`
`rogan_gladen_correction`

**Outbreak analysis.** `epidemic_curve` `onset_to_report_delay ~`
`nowcast ~` `back_calculate ~` `cluster_detection ~`
`transmission_tree ~` `superspreading_dispersion`

**Not shipped.** `✗ agent_based` — an agent-based epidemic simulator is a model,
not a function; its behaviour is its parameterisation and it belongs in a user's
own program. `✗ forecast` in any form — an epidemic forecast is a research claim.

> **Decision 10.14. The survival-analysis names above are `stats`
> re-exports, not reimplementations. `kaplan_meier_curve` and
> `cox_proportional_hazards` live in `stats.survival` and `bio.epi` names them
> so that an epidemiologist finds them where they look.**

**Rejected: implementing them in `bio.epi`.** Survival analysis is not
biological; it is used identically in reliability engineering, finance and
clinical trials. Two implementations would drift.

**Rejected: not re-exporting, and telling users to `use stats.survival`.** It is
purer and it means the epidemiology module is missing the half of epidemiology
that most epidemiologists spend their time on. The re-export costs one line per
name and no second implementation.

**R₀ is an eigenvalue.** `basic_reproduction_number` is the spectral radius of
the next-generation matrix, which is `linalg`'s, which is why `bio.epi` depends
on it. Writing it any other way — the closed forms in textbooks — works only for
the two-compartment case and silently gives the wrong answer for age-structured
or multi-strain models.

**Five signatures.**

```science
# The SIR system is an ODE; `math`'s integrator does the work and a stiff or
# badly-scaled system can fail to converge.
function solve_epidemic(
    model: borrowed CompartmentModel,
    initial: borrowed Map of (Compartment, F64),
    span: (Time of F64, Time of F64),
    tolerance: F64,
) -> (EpidemicCurve, IntegrationError?)

# R0 is the spectral radius of the next-generation matrix — not a closed form.
function basic_reproduction_number of (const N: Int)(
    transmission: borrowed Matrix of (F64, N, N),
    transition: borrowed Matrix of (F64, N, N),
) -> (F64, SingularError?)

# Rt is `~`: Wallinga–Teunis and Cori give different curves from the same data
# and answer different questions.
function instantaneous_reproduction(
    incidence: borrowed Array of Int,
    serial_interval: borrowed Distribution,
    window: Int,
) -> (Array of F64, TooShortError?)

# Re-exported from `stats.survival`.
function kaplan_meier_curve(data: borrowed SurvivalData) -> (SurvivalCurve, EmptyRiskSet?)

# The log-rank test yields a PValue, like every other test in the catalogue.
function log_rank_test(groups: borrowed Array of SurvivalData)
    -> (PValue, TooFewEventsError?)
```

**Example.**

```science
use bio.epi
use chem.units

function main():
    let model be epi.seir(0.6, 0.2, 0.1)
    let curve, err be epi.solve_epidemic(
        model, initial, (0.0<d>, 365.0<d>), 1.0e-8)
    if err?:
        print(f"the system did not integrate: {err.message()}")
        return

    print(f"peak at day {curve.peak_time()}, attack rate {curve.attack_rate():.2}")
```

---

## 11. The seams with the rest of the catalogue

Four places where `chem` and `bio` touch a module somebody else owns. Each is a
decision about where the code lives, not about what it computes.

### 11.1 Enzyme kinetics belongs to `chem.kinetics`, and `bio` does not copy it

Michaelis–Menten, the inhibition modes, Hill, `k_cat` and the linearising
transforms are in **`chem.kinetics`** (§8.8), not in a `bio.enzyme`.

> **Decision 11.1. Enzyme kinetics is chemistry. `bio` re-exports nothing from
> it and points at it in documentation.**

The mathematics is a rate law with a saturating denominator; it is the same
object whether the catalyst is an enzyme, a zeolite or a metal surface, and the
Langmuir–Hinshelwood form used in heterogeneous catalysis is the same equation
with different letters. Two implementations would drift and the drift would be
invisible, because both would be right for their own callers.

**Rejected: a `bio.enzyme` that re-exports**, as §10.14 does for survival
analysis. Survival analysis earns its re-export because epidemiologists genuinely
look for `kaplan_meier_curve` under epidemiology and would not think to look under
`stats`. A biochemist looking for Michaelis–Menten *does* think of it as
chemistry, so the re-export would buy nothing and would add a name.

**Cost.** A structural biologist computing a `k_cat` writes `use chem.kinetics`
in a program that otherwise touches no chemistry. One line.

### 11.2 `PValue` flows through every test in both domains

`statistical-validity.md` Decision 1 makes `PValue` and `AdjustedPValue` distinct
types, and only an `AdjustedPValue` may be thresholded. Every hypothesis test in
this catalogue returns a `PValue`:

`hardy_weinberg_test`, `tajima_d`'s significance, `mcdonald_kreitman_test`,
`log_rank_test`, `permanova`, `mantel_test`, `anosim`, `over_representation`,
`gene_set_enrichment`, `marker_genes`, `p_value_of_score`, `differential_expression`,
`likelihood_ratio_test`, `hudson_kreitman_aguade_test`, `ewens_watterson_test`.

**Fifteen producers in one note**, which is more than any other note in the
corpus, and `bio.expression` alone produces one per gene — twenty thousand
uncorrected p-values from a single function call. That note's flow analysis was
written for exactly this and this catalogue is its largest customer. Recorded
here so the dependency is visible from both ends.

The one design consequence: **`differential_expression` takes the correction as a
required argument** (§10.9) rather than returning raw p-values and trusting the
caller. A function that produces twenty thousand `PValue`s and hands them back is
a function that has moved the obligation to the least careful place.

### 11.3 `chem.structure` and `bio.structure` are two modules over one file format

Both read PDB. They are not the same module, and merging them is wrong in a way
worth stating.

`chem.structure`'s `Molecule` is atoms and bonds with no polymer structure — it
is the right type for a ligand, a solvent molecule and a small organic. `bio.structure`'s
`ProteinStructure` is models, chains, residues and insertion codes, and its
operations are per-residue. A PDB file contains both, and the two modules read the
same bytes into two types deliberately.

> **Decision 11.3. `bio.structure` depends on `chem.structure` and converts:
> `ProteinStructure.ligands()` yields `Molecule`s. The dependency does not run
> the other way.**

**Rejected: one `Structure` type carrying both.** It would carry the residue
machinery into every cheminformatics program and the bond-perception machinery
into every structural-biology one, and the union type's invariants ("a residue
may be null") would be false half the time.

### 11.4 `data-io.md` owns the readers, and this note owns the parsers

`read_fasta`, `read_pdb`, `read_cif`, `read_vcf` and the rest are *format*
parsers, and `data-io.md` §2 owns `data.*` for formats. The split adopted here:

> **Decision 11.4. A domain format lives in the domain module, not in `data.*`.
> `data.*` holds the general formats — CSV, JSON, Parquet, Arrow — whose users
> are everybody. FASTA, PDB, mmCIF, Newick, VCF, SDF and CIF have exactly one
> audience each and live with that audience. They use `data-io.md`'s `Path`,
> `File` and `Rows`, and they add no format to `data.*`.**

The test is the one `stdlib-core.md` §1.3 uses for the prelude, one level down: a
format belongs in the shared namespace when its users are not a domain. Nobody
outside structural biology reads an mmCIF.

**Cost.** A user looking for "the file formats" finds them in two places. The
compensation is that `data.*` does not grow eleven scientific formats whose
maintainers are all in one field.

---

## 12. What is not shipped, and why

The most useful half of a catalogue this size. Each refusal names what is shipped
instead, because "no" without an alternative is not a design decision.

### 12.1 Electronic structure and molecular dynamics

**Refused: an integral engine.** Electron repulsion integrals over contracted
Gaussians — Obara–Saika, McMurchie–Davidson, Rys quadrature — plus the screening,
the density fitting and the resolution of the identity. This is the core of every
quantum chemistry program and it is thirty years of work per program. Libint and
Libcint exist, are BSD-ish, and are exactly the case `ffi-c-boundary.md` §0 was
written for.

**Refused: exchange–correlation functionals.** Libxc implements around six
hundred of them. They are *data and code together*: each functional is a
published parameterisation plus its derivatives, and new ones appear monthly.
Writing even the common dozen would produce a library that is wrong for the
thirteenth.

**Refused: an SCF loop in Science.** Given the integrals are linked, the SCF
iteration itself is a few hundred lines and is tempting. It is refused because
convergence is the entire difficulty — DIIS, level shifting, second-order
methods, fractional occupation, broken symmetry — and a naive SCF that fails to
converge on ordinary transition-metal systems would be worse than no SCF, because
it would look like a feature.

**Refused: a molecular dynamics integrator.** Not because Velocity Verlet is
hard — it is ten lines — but because an MD program is a force field plus a
thermostat plus a barostat plus constraint algorithms plus neighbour lists plus
Ewald summation, and the *force field parameters* are licensed data that a
package manager cannot ship (§10.9's argument, applied to AMBER and CHARMM).

**Shipped instead**: the analytic and semi-empirical results of §8.10, which are
real quantum chemistry that fits in a library; the `‡` bindings; and the
*analysis* of trajectories somebody else produced — RMSD, radius of gyration,
contact maps, RDFs, principal components — which is where most users actually
spend their time and which needs no force field at all.

### 12.2 Tree search, structure solution and everything else that is a heuristic search

**Refused: maximum-likelihood and Bayesian tree *search*.** Likelihood
*evaluation* on a given tree is shipped (`likelihood_of_tree`) and is well
defined. The search — NNI, SPR, TBR, the starting tree, the stopping rule — is
what RAxML, IQ-TREE and FastTree differ on, and it is the reason the same
alignment gives three trees. `nni_search` and `spr_search` are shipped as
*named moves* so a user can build a search; `maximum_likelihood_tree ~` is
shipped with every parameter explicit; nothing is called "the ML tree".

**Refused: MCMC phylogenetics.** BEAST and MrBayes are research programs: the
priors, the proposal kernels and the convergence diagnostics are the science.

**Refused: crystallographic phasing and refinement.** Direct methods, Patterson
search, molecular replacement, Rietveld. Every one is a search with published
heuristics and per-program behaviour.

**Refused: read mapping, assembly and variant calling.** The *data structures* —
FM-index, BWT, suffix automaton, minimiser sketches — are shipped in §10.3 and
are well-defined objects with one correct answer. A competitive mapper is ten
years of heuristics on top of them and GATK's variant model changes per release.

**The general rule this section is applying**, stated so the next refusal does
not need its own argument:

> **A heuristic search is not a library function. Where the answer depends on
> the search strategy rather than on the input, the library ships the *moves*
> and the *objective* and refuses to ship "the answer".**

This is `stdlib-core.md` §1.2 Q3 and Q4 at the scale of a whole program rather
than a function, and it is why every entry in this section is also `~`.

### 12.3 Anything whose specification is a model

`✗ shift_prediction` (NMR), `✗ interface_energy`, `✗ fold_classification`,
`✗ binding_affinity`, `✗ rna_velocity`, `✗ agent_based`, `✗ forecast`,
`✗ structure_prediction`.

Each of these is, today, a trained model. `models-and-inference.md` owns loading
and running trained models and is the right home for all of them: the *function*
is `run(model, input)` and the model is a versioned artefact, which is §4's
machinery with weights instead of a table. A `bio` module that shipped a
predictor would be shipping a model release under a function name, and the answer
would change when the model changed with nothing in the signature to say so.

### 12.4 Databases that are not redistributable

`✗ GO`, `✗ KEGG`, `✗ Reactome`, `✗ MSigDB`, `✗ ChEMBL`, `✗ PDB mirror`,
`✗ UniProt`, `✗ force-field parameter sets`.

§10.9's decision, generalised. Some are licensing (KEGG, MSigDB's C2), some are
size (a PDB mirror is hundreds of gigabytes), and all of them are monthly
releases. The catalogue ships the **format readers** — `read_gmt`, `read_obo`,
`read_sbml`, `read_pdb_structure` — so a user brings their own copy and the run
record names its digest through the ordinary input-hashing of
`reproducibility.md` §5.2's `inputs` array. That is a *better* provenance story
than a shipped database, because a shipped database would be pinned by a package
version and a downloaded one is pinned by its bytes.

### 12.5 Things refused on units grounds

`✗ normality` and the `<N>` unit (§8.4). `✗ <bp>`, `✗ <reads>`, `✗ <cells>`,
`✗ <CFU>` — counts are not units, per `unit-literals.md` §6.5. `✗ <pH>`,
`✗ <dB>` — logarithmic, per that note's §6.3. `✗ fractional reaction order` as a
const parameter (§5.5(1)), because `ORDER` is an `Int` and rational exponents are
excluded by `scientific-libraries.md` §12.6.

---

## 13. What this note asks of the others

Ordered by how much is blocked behind each. Items 1, 2 and 3 already have other
askers; items 4 through 9 are new.

1. **Const-expression arithmetic in type position**, with **integer coefficients
   other than ±1**. `const-expression-arithmetic.md` §2.1's grammar already
   admits `3 * ORDER` and §3.2's `SCALE` already implements it; this note is the
   **first caller**, through `RateConstant of (T, const ORDER: Int)` (§5.2,
   Finding A). The ask is one test case, named after this note, so the path is
   exercised.

2. **Const-argument inference**, that note's §7. §5.3's diagnostic depends on
   inverting `1 - ORDER = -1` to recover `ORDER = 2`, which is the invertible
   single-unknown case. Without it the diagnostic degrades to an exponent-vector
   mismatch and the sentence *"`k` is a second-order rate constant"* cannot be
   printed. This note is the second asker after §7's own customers and the one
   with the most legible payoff.

3. **Closure types spelled `function(T) -> U`.** `window_statistic` (§10.3),
   `bootstrap_tree` (§10.6) and `solve_kinetics_stiff`'s user-supplied rate laws
   need it. Fifth asker; no new argument, one more customer.

4. **Of `unit-literals.md`:**
   - **Add `L` (and `l`) to the prelude** (§3.2). The litre is SI Table 8, its
     factor is exactly 10⁻³ m³, and `mol/L` is what every chemist writes while
     `mol/m^3` is what the current rule allows. One entry, and `sciencec fmt`
     normalises `l` to `L` by the mechanism §3.2 of that note already has for
     `·` and `μ`.
   - **Record that a `unit` declaration's scalar may itself be measured data**
     (§3.3). The dalton's conversion factor has a CODATA uncertainty and moves
     between releases, so `chem.units` is a `†` module and a unit conversion is
     not invariant across a data-package update. That note's §3.4 declaration
     form assumes an author-written scalar and should say what happens when the
     scalar is a citation.
   - **Extend the dimension-mismatch message** — that note owns `SC0252`–`SC0256`
     — with the `note` line §5.3 shows: where a parameter's dimension is a const
     expression, print the equation that would have to hold and the value the
     parameter would need. This note takes no code and asks only for the
     rendering. The sibling `intrinsics-math-physics.md` §11 item 6 makes a
     closely related ask on the same code; **the two should be read together**,
     and this note's addition is the const-parameter line only.

5. **Of `stdlib-core.md` §1.1** — state the criterion as two halves (§4.6):
   *where the algorithm is itself observable, the module is not Level 1 — and
   where the **data** is itself observable, neither is it.* `atomic_mass` is a
   lookup with no algorithm at all and fails the criterion entirely on the
   strength of its table, which the current sentence does not describe.

6. **Of `reproducibility.md` §5.2** — add a `tables` array to the run record
   (§4.4), written only for tables actually read, carrying name, release, digest
   and any sub-selection (`table_id` for a genetic code). It is the same
   read-versus-permitted distinction that note already makes for `allowed_used`,
   and it is what lets a methods section say *"masses from CIAAW 2021,
   translation with NCBI table 11"* and have a reviewer check it.

7. **Of `package-manager.md`** — document the **data package** as a general
   mechanism rather than as a special case for time. §9 item 8 of that note
   calls tzdata *"the clearest case in the library of a dependency that is data
   rather than code"*; §4.1 here supplies six more, from two domains, and the
   mechanism is identical. One section, no new machinery.

8. **Of `scientific-libraries.md`:**
   - **§10.7's five signatures use a `Quantity of (F64, GramsPerMole)` spelling
     that does not exist** (§5.1). §12.3 defines an eight-parameter `Quantity`;
     §10.7 writes a two-parameter one. The eight-parameter form is authoritative
     and §10.7 should be rewritten against the aliases of §5.1 here.
   - **§11.7's `smith_waterman` and `neighbour_joining` signatures are renamed**
     by the sibling's Decision 1 (§1.5 here). The same amendment the sibling's
     §2.5 asks for `rk4` and `brent`, applied to §11.
   - **§3's collision table has twenty rows, not eleven** (§6.2), and four of the
     new ones — `match`, `interface`, `null`, `loop` — were created or re-priced
     by `syntax-revision-2.md` after §3 was written.
   - **§10 and §11 should carry the submodule split** of §7 and §9, or say why
     they do not. A flat `chem` links BLAS to compute a molar mass.

9. **Of `reserved-words.md`** — carry the scientific-vocabulary check as a
   **standing test** that any note adding a keyword runs, not as a one-time
   audit (§6.3). Three keywords landed in one day and all three collide with
   chemistry or biology vocabulary, and every one was invisible to the note that
   introduced it. Also: this note seconds §5 item 1 (the dot rule) as its single
   highest-value ask on that note, and §5 item 2's freeing of `yield`, with the
   thirty-four-name bill of §6.5 as the evidence.

### 13.1 What this note asks of the sibling

- **Confirm or overrule §1.5's last paragraph**, on `align_local` and
  `align_global` as two operations rather than one `align` with a `method:`. This
  note reads Smith–Waterman versus Needleman–Wunsch as a semantic distinction
  rather than a method choice, which is a refinement of that note's §2.6 example
  and not a rejection of N2.
- **Adopt or replace S1–S5** (§1.4). They are spelling conventions read off the
  existing catalogue and they are orthogonal to N1–N4; if the sibling states its
  own, its version wins.
- **Note that tier 1 is empty here** (§2.4) and that the sibling's tier 1 list is
  what chemistry and biology consume: `popcount` and the SIMD integer operations
  under sequence work, `exp`, `ln`, `log10` and `pow` under every rate law. The
  domains' entire performance story is which of the sibling's intrinsics they
  call and in what order.

### 13.2 What this note asks of the compiler

**Bit-packed arrays** — `Array of U2`, or `PackedArray of (T, const BITS: Int)`.
§2.2(c). Four customers: two-bit nucleotides, quantised model weights, bitsets,
and colour channels. It is the one language feature a sequence library genuinely
wants, it is *not* a biology feature, and the ask belongs to
`indexing-and-array-literals.md`. Raised here because biology is the demanding
consumer and nobody else has asked.

### 13.3 What this note does not ask for

**No new language feature specific to chemistry or biology.** Not a `table`
declaration form (§4.2), not a domain intrinsic (§2.4), not a prelude entry
beyond a unit SI already defines (§3.4), not rational exponents (§5.5(1)). Every
ask above is either an existing cross-note ask with one more customer, or a
documentation or record-keeping change. **That is the result this note most wants
read**: two entire scientific disciplines fit in the language as it is already
planned, and the places they do not fit are two reserved words and a litre.

### 13.4 Diagnostic codes

**This note claims none**, per `README.md`'s instruction and its own history of
collisions. The one diagnostic it renders (§5.3) is an extension of a message
`unit-literals.md` owns, and the ask is in item 4 above. The README's allocation
table needs no row for this note.

---

## 14. Risks

**The catalogue is a commitment surface, and this one is larger than its
parent's.** `scientific-libraries.md` §15 already says this about eight modules;
this note has twenty-seven submodules and roughly six hundred and forty names,
and it is half of one domain pair. Publishing it without §12 and without
`scientific-libraries.md` §13's wave table invites the reading that all of it is
planned. **§12 is the antidote and should travel with the catalogue**, because
the refusals are what make the rest credible.

**The data packages are a distribution problem nobody owns.** §4.2 says the
tables ship as content-addressed packages and `package-manager.md` has the
mechanism. It does not have a *publisher*. Somebody must convert CIAAW's
published table into an archive, re-cut it every two years, and be accountable
for the conversion being faithful. That is an ongoing editorial obligation, not a
one-time engineering task, and it is the same obligation the tz database has a
maintainer for. A `chem` whose atomic weights are three releases stale is worse
than no `chem`, because the staleness is invisible until the run record is read.

**`†` is only as good as the discipline behind it.** Decision 4.4 records the
tables a run actually read. It works if every `†` function touches the recorder.
One that forgets produces a run record that is *confidently wrong* — it asserts
the complete list of tables used and omits one. That is a worse failure than no
record, and it is a failure a test can catch only if the test knows the full `†`
list. The mitigation is that the `†` marker should be a property the compiler or
the module system can check, not a convention in a comment; how, is not solved
here.

**Wave 2 ships `chem` with unchecked units, and §5.6 makes that a lie with a type
alias on it.** `MolarMass of F64` aliased to `F64` compiles when added to a
`Mass of F64`, and a user who reads the signature will believe the check is
happening. The documentation must say so at the top of every wave 2 module, and
documentation is the weakest enforcement mechanism in the project. The
alternative — no aliases until wave 5 — makes the retrofit a hundred-file rename
with no check, which is worse, but the risk should not be minimised.

**The sibling's N2 and this note's `~` marker can disagree.** N2 moves an
interchangeable method into an argument; `~` says the method is observable and
must be visible. They agree in every case in this note — an argument *is* visible
— but a future editor who reads `~` as "must be in the name" will start undoing
N2. §1.5 states the reading; it should be restated wherever `~` is defined if
that section moves.

**Eleven names revert the day `yield` is freed** (§6.6), and nothing forces the
revert to happen. A pre-committed list in a design note is not a migration. If
`reserved-words.md` §5 item 2 lands after `chem` has users, `percent_produced`
becomes permanent by inertia and the catalogue keeps a name nobody in the
discipline uses, forever. The cheapest moment to take that decision is before
`chem` ships, which is the same argument `syntax-revision-2.md` §10 makes about
its own timing and with the same expiry date.

**Two modules read PDB and one file format now has two parsers** (§11.3). The
decision is defended and it is still two parsers over one grammar, and they will
drift on the corner cases — insertion codes, altlocs, multi-model files,
non-standard residues — which are exactly the cases that matter. A shared
low-level tokeniser with two typed layers on top would avoid it and is not
specified here.
