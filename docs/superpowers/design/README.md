# Science — design notes

Each note owns one area, states its decisions with the alternatives it rejected,
and prices what it asks of the others. The core spec —
`../specs/2026-09-16-science-f0-core-design.md` — is the authority; a note that
disagrees with it says so explicitly and gives the reason.

## The notes

| Note | Owns | Phase |
|---|---|---|
| `syntax-revision-2.md` | The current syntax: comparisons, loops, the Go-style error model, `->`, `has`, `interface`, `print`. **Read this first** — the other notes are written in the syntax it defines. The core spec **has** since been amended to it (§4.2, §4.3, §4.5, §4.6, §5.4, §5.5, §8, §13), which discharges §9's item 1; §13 is the one place the amendment is incomplete — see the conventions below. | F0 |
| `reserved-words.md` | The reserved-word list audited against the scientific vocabulary; the dot rule; the case for freeing `shape`, `model` and `tensor`. | F0 |
| `llm-ergonomics.md` | Diagnostics as the only teaching channel for a language with no training corpus; the migration-diagnostic inventory; canonicalisation for `sciencec fmt`. | F0 |
| `indexing-and-array-literals.md` | `a[i]`, `m[i, j]`, slicing, `Slice of T`, array and matrix literals, zero-based indexing, and the rejection of comprehensions. | F0 / F1 |
| `script-mode.md` | Statements at the top level of a file, the implicit script body, entry-file rules, and why the grammar decision belongs in F0. | F0 |
| `strings-formatting-and-docs.md` | `f"…"` interpolation, the format mini-language, `Display` / `Inspect` / `DisplayNumber`, `print`, `##` doc comments, Unicode policy. | F0 |
| `unit-literals.md` | `9.8<m/s^2>`, the unit namespace, SI prefix composition, exact rational folding, affine and logarithmic units. | F0 (staged) |
| `broadcasting.md` | Element-wise operators, the dotted forms, the alignment algorithm, `with … as …` for dynamic shapes, fusion. | F1 |
| `def-and-lambda.md` | Whether `function` becomes `def` and whether `lambda` is added. Both rejected, with the surviving half of §4.1 stated as one clause. | F0 |
| `stdlib-core.md` | Level 1: what is available with no import. The core criterion, `String`'s surface, `Read`/`Write`, the concrete error inventory. | F0 |
| `stdlib-standard.md` | Level 2: `time`, `os`, `random`, `testing`, `logging`, `text.regex`, `thread`, `net`. | F0 / F1 |
| `stdlib-shape-and-packages.md` | The policy the three stdlib notes live under: batteries included, the error model's consequences, threads over async, naming, and Level 3. **Read before the other two.** | F0 |
| `scientific-libraries.md` | The catalogue: `math`, `linalg`, `stats`, `optimize`, `signal`, `chem`, `bio`, `physics`; what links against C; the units-in-the-type decision. | F1+ |
| `type-checking-and-mir.md` | Everything between the resolver and codegen: bidirectional checking over annotated signatures, THIR and MIR, flow narrowing and `SC0140` — the half of the error model that shipped without its meaning — and what the type system owes the ML ecosystem, including the `python:` region that is the only way scikit-learn is reachable at all. | F0 design, F1 build |
| `region-inference.md` | The language's central claim, specified: what a region is, why a *type* gets one variable per borrowed field, what happens when a signature is ambiguous with no syntax to disambiguate it, and the contradiction between interprocedural inference and separate compilation. **Read with `examples/21_compiler_shapes.science`.** | F0 design, F1 build |
| `self-hosting.md` | Writing the Science compiler in Science: the gates, the differential bootstrap against the existing corpus, and the argument that the adversarial test is ~200 lines rather than 17,305. **Read before planning any phase order.** | F0 design, F2+ execution |
| `mcp-servers.md` | `tool` as a declaration of *a callable exposed to a model*, protocol-agnostic, with the JSON Schema generated from the signature; why `prompt` stays reserved and unused. | F1 |
| `intrinsics-math-physics.md` | Which mathematical and physical operations are tier 1 (a hardware instruction), tier 2 (a compiler obligation) or tier 3 (an ordinary library function). Establishes the tier test and the N1–N4 naming rule the sibling note follows. | F1+ |
| `intrinsics-chem-bio.md` | The `chem` and `bio` catalogues under that same test: tier 1 is empty, tier 2 is five SI units, and seven reference tables are *data with releases*, shipped as content-addressed packages pinned by the lockfile. | F1+ |
| `equations.md` | `equation` as a closed-grammar body that always has a rendering and a derivative; why dimensional checking is *not* what earns it; the LaTeX closure. | F1, conditional |
| `data-leakage.md` | `Train`/`Tune`/`Holdout`/`Whole` as distinct types; why fitting a scaler before the split cannot compile; and what no compiler can catch. | F1 |
| `publishable-output.md` | The typed table and figure, PDG significant figures, and the asset bundle a journal template consumes. Concedes literate programming to Quarto and says what survives. | F1+ |
| `uncertainty.md` | `Uncertain of T` at value level, exact correlation by forward-mode gradients, and why the type-level design is a smaller win than it looks. | F0 lexer + F1 |
| `statistical-validity.md` | `PValue` and `AdjustedPValue`: making an uncorrected p-value impossible to threshold silently, by flow rather than by counting. | F1 |
| `reproducibility.md` | What Science can actually promise about reruns, the five float channels, `--deterministic`, and the provenance artefact a reviewer checks. | F0 one-way doors |
| `const-expression-arithmetic.md` | The five-operator grammar, the `k + Σ cᵢ·aᵢ` normal form, type-level shapes, and the eight things F0 must commit to before the type checker is written. **Blocks the type checker.** | F0 |
| `effects.md` | Three inferred bits — `python`, `ambient`, `external`; `pure function` as the one declaration; how much the effect discipline already falls out of ownership. | F0 reserves |
| `collections-and-chains.md` | `Iterate`, the chain vocabulary, laziness, ownership through a chain, parallelism. | F0 |
| `data-io.md` | `Path`, `File`, `Frame`, `Rows`, and the `data.*` format modules. | F0 |
| `ffi-c-boundary.md` | `extern` blocks, the ownership boundary, `unsafe`, callbacks, linking, binding generation. | F0 |
| `package-manager.md` | `science.toml`, minimal version selection, the content-addressed lockfile, flat naming, no build scripts, and the `SP` diagnostic namespace. | F0 staged |
| `rust-binding-generation.md` | Measured rustdoc-JSON coverage over regex, polars and serde_json; the four buckets; the policy file; bind-the-format as a rule. | F0 / F1 |
| `c-binding-coverage.md` | What fraction of real C actually gets through the `extern` boundary, measured over five libraries; the macro taxonomy; the exported-global and complex-type gaps. | F0 |
| `native-dependencies.md` | How a program gets the C and Fortran libraries it links: provider order, the BLAS variant probe, reproducibility tiers, `.science.provenance`, HPC and GPU. | F0 / F1 |
| `rust-interop.md` | What "compatible with Rust" can mean; the shim-crate mechanism; `science_abi.rs` layout assertions; what crate consumption really buys. | F0 / F1 |
| `python-from-science.md` | The other direction: hosted coverage, standalone reopened, the `python err:` region, and stubs as documentation only. | F2 |
| `python-interop.md` | Science compiled as a CPython extension; DLPack; the GIL; errors and tracebacks across the boundary. | F2 |
| `models-and-inference.md` | Loading and running trained models, weight formats, tokenizers. | F2 |

## Diagnostic code allocation

§9 of the core spec fixes the ranges:

| Range | Phase |
|---|---|
| `SC0001`–`SC0099` | Lexical |
| `SC0100`–`SC0199` | Syntax |
| `SC0200`–`SC0249` | Resolution |
| `SC0250`–`SC0299` | Types and traits |
| `SC0300`–`SC0399` | Ownership and regions |
| `SC0400`–`SC0499` | Codegen and linking |
| `SC0500`–`SC0799` | Types and traits, continued |

**Check this table before allocating a code.** Five of these notes were written
in parallel and four collisions resulted — `SC0010` against the shipped lexer,
`SC0150`–`SC0159` claimed twice, `SC0251` and `SC0255` claimed twice each. Every
one of them was invisible to the note that caused it, because each had honestly
checked against everything that existed when it started.

### Shipped, in the compiler today

| Codes | Where |
|---|---|
| `SC0001`, `SC0003`–`SC0011`, `SC0016`–`SC0017` | `crates/science-lexer/src/lexer.rs` |
| `SC0100`–`SC0116`, `SC0118`, `SC0138`–`SC0139`, `SC0141`–`SC0144` | `crates/science-parser/src/parser.rs` |

`SC0002` is unallocated. `SC0115` (nested `each`), `SC0116` (an ambiguous
`Array of Doc.new()`) and `SC0118` (`returns` written where `->` belongs) are
the newest and are in use. `SC0118` is the first code in this table that was
allocated by a design note and then implemented the same day.

### Claimed by notes

| Note | Lexical | Syntax | Resolution | Types | Ownership | Codegen |
|---|---|---|---|---|---|---|
| `llm-ergonomics.md` | — | `SC0120`–`SC0134` | — | — | — | — |
| `indexing-and-array-literals.md` | — | `SC0150`–`SC0154` | — | `SC0280`–`SC0287` | — | — |
| `broadcasting.md` | — | `SC0160` | — | `SC0290`–`SC0298` | — | — |
| `strings-formatting-and-docs.md` | `SC0015` | `SC0170`–`SC0177` | — | `SC0274`–`SC0275` | — | — |
| `unit-literals.md` | `SC0012`–`SC0014` | — | `SC0230`–`SC0236` | `SC0252`–`SC0256` | — | — |
| `script-mode.md` | — | `SC0117` | `SC0212`–`SC0213` | — | — | `SC0458` |
| `def-and-lambda.md` | — | `SC0119`, `SC0135`–`SC0137` | — | — | — | — |
| `stdlib-core.md` | — | — | — | `SC0257`–`SC0259` | — | — |
| `stdlib-standard.md` | — | — | — | `SC0265`–`SC0270` | — | — |
| `stdlib-shape-and-packages.md` | — | — | — | `SC0276`, `SC0279` | — | — |
| `syntax-revision-2.md` | — | — | — | `SC0140` (unchecked error) | — | — |
| `equations.md` | — | — | `SC0246`–`SC0249` | — | — | — |
| `data-leakage.md` | — | — | — | `SC0500`–`SC0503` | — | — |
| `publishable-output.md` | asks for `SR0001`–`SR0099`, a namespace outside `SC` | | | | | |
| `uncertainty.md` | — | — | — | `SC0277`–`SC0278` | — | — |
| `statistical-validity.md` | — | — | — | `SC0288`–`SC0289` | — | — |
| `reproducibility.md` | — | — | `SC0237`–`SC0240` | — | — | — |
| `const-expression-arithmetic.md` | — | — | `SC0220`–`SC0221` | `SC0260`–`SC0262` | — | — |
| `effects.md` | — | — | `SC0214`–`SC0219` | — | — | — |
| `collections-and-chains.md` | — | — | — | `SC0271`–`SC0273` | `SC0331`–`SC0332` | — |
| `models-and-inference.md` | — | — | — | `SC0251`, `SC0263`–`SC0264` | — | — |
| `ffi-c-boundary.md` | — | — | — | — | `SC0301`–`SC0302`, `SC0380` | `SC0410`–`SC0461` |
| `rust-interop.md` | — | `SC0145`–`SC0149` | — | — | — | `SC0462`–`SC0470` |
| `native-dependencies.md` | — | — | — | — | — | `SC0471`–`SC0479` |
| `rust-binding-generation.md` | — | — | — | — | — | `SC0480`–`SC0489` |
| `c-binding-coverage.md` | — | — | — | — | — | `SC0490`–`SC0499` |
| `package-manager.md` | claims no `SC` code at all — see below | | | | | |
| `self-hosting.md` | claims no `SC` code at all: seed and build mismatches reuse `SP0021`/`SP0022`, and the two-region rejection belongs to the unwritten region-inference note | | | | | |
| `python-interop.md` | — | — | — | — | — | `SC0450`–`SC0458` |
| `python-from-science.md` | — | `SC0180`–`SC0189` | — | — | — | `SC0459` |
| `mcp-servers.md` | — | `SC0190`–`SC0199` | — | `SC0504`–`SC0519` | — | — |
| `region-inference.md` | — | — | — | — | `SC0330`, `SC0333`–`SC0379` | — |
| `type-checking-and-mir.md` | — | — | — | `SC0520`–`SC0579` | — | — |

### A namespace outside `SC`

`package-manager.md` claims **`SP0001`–`SP0999`** and no `SC` code. The argument
is that manifest and resolution failures happen *before* any source is read, so
they are not compiler-phase diagnostics and the §9 ranges do not describe them.
A useful side effect, given the history above: it removes one more claimant from
a space that has now produced five collisions in one day.

### The codegen range, partitioned

`SC0400`–`SC0499` had a three-way overlap: `ffi-c-boundary.md` §8 reserves
`SC0460`–`SC0479` for linking, `python-interop.md` §9 claims `SC0450`–`SC0479`
and says in its own text that the range *"needs reconciling"*, and
`rust-interop.md` then claimed `SC0462`–`SC0470` between them.

**No code actually in use collided** — the three notes use disjoint numbers.
Only the declared ranges overlapped, which is the cheapest moment to find such a
thing and the most expensive to leave. The partition below is authoritative and
supersedes the sub-range claims inside those two notes:

| Sub-range | Owner |
|---|---|
| `SC0400`–`SC0409` | General codegen. Unclaimed. |
| `SC0410`–`SC0449` | `ffi-c-boundary.md` — `extern` declarations and FFI types |
| `SC0450`–`SC0459` | `python-interop.md` — the Python boundary |
| `SC0460`–`SC0461` | `ffi-c-boundary.md` — linking |
| `SC0462`–`SC0470` | `rust-interop.md` |
| `SC0471`–`SC0479` | `native-dependencies.md` |
| `SC0480`–`SC0499` | Free |

Notes also *amend* codes they do not own — `unit-literals.md` adds a note to the
existing `SC0009`, `script-mode.md` changes `SC0101`'s wording and reuses
`SC0107`. Amending is fine; claiming is what needs checking.

### Free as of this writing

| Range | Free |
|---|---|
| Lexical | `SC0002`, `SC0018`–`SC0099` |
| Syntax | `SC0156`–`SC0159`, `SC0161`–`SC0169`, `SC0178`–`SC0179` |
| Resolution | `SC0200`–`SC0211`, `SC0222`–`SC0229`, `SC0241`–`SC0245` |
| Types | `SC0250`, `SC0299`, and `SC0504`–`SC0799` in the second band |
| Ownership | everything but `SC0301`–`SC0302`, `SC0331`–`SC0332`, `SC0380` |
| Codegen | `SC0400`–`SC0409` only — see the partition above |

The notes index above is the allocation record. A note that claims a block adds
its row before writing, not after.

The Syntax row was wrong until now and said so in both directions at once: it
offered `SC0138`–`SC0149` as free while the table above recorded `SC0138`–`SC0139`
and `SC0141`–`SC0144` in `parser.rs`, `SC0140` held by `syntax-revision-2.md`
and `SC0145`–`SC0149` claimed by `rust-interop.md`. `SC0155` has since gone to
the `try` migration and `SC0190`–`SC0199` to `mcp-servers.md`. **Check this row
against the table above before trusting it** — the table is the record and this
is a convenience.

## Standing cross-note asks

Requests that more than one note depends on, which is what makes them worth
building rather than deferring.

| Ask | Asked by | Notes |
|---|---|---|
| **Const-expression arithmetic in type position** | `scientific-libraries.md` §12.4, `broadcasting.md` §11, `unit-literals.md` | The single largest one. Units need exponent addition; shapes need output-shape computation. Build once, two customers. `broadcasting.md` adds that shapes also need type-level *lists*, which is a bigger ask than §12.4 priced. |
| **Closure types spelled `function(T) -> U`** | `ffi-c-boundary.md` §10.1, `scientific-libraries.md` §14.2, `broadcasting.md` | Three notes. The core spec defines closure *expressions* and never their types. |
| **Explicit type arguments at call sites** | `data-io.md` §11.2, `scientific-libraries.md` §14.3 | `read_csv of Measurement(...)`. |
| **A derive mechanism** | `data-io.md` §11.1 (`Record`), `strings-formatting-and-docs.md` (`Inspect`) | Two customers, against §12's "macros are reserved, not implemented". |
| **An uncertainty type** | `scientific-libraries.md` §14.6, `unit-literals.md`, `strings-formatting-and-docs.md` | Three notes now point at this hole. It is lexical (`9.80665(15)<m/s^2>`) as much as it is a type question, and it gets more expensive as `physics.constants` approaches. |
| **The dot rule** (any word after `.` is a member name) | `reserved-words.md` §0.1, `unit-literals.md` (`d.in(FOOT)`) | Five lines of parser; removes a whole class of reserved-word collisions. |
| **`Float` interface over `F32`/`F64`** | `scientific-libraries.md` §14.4 | Nearly every numeric signature is generic over it; §5.4 does not list one. |

## Conventions

- **Revision-2 syntax.** Notes written before `syntax-revision-2.md` use the
  older spelling (`returns`, `trait`, `for each`, `println`, `is at least`).
  That is drift, not disagreement; when one of those notes is next edited, its
  code samples should move. **The error model is the exception and is no longer
  drift**: `Result of (T, E)`, `try`, `Option of T`, `Some`, `None`, `Ok` and
  `Err` have been migrated out of every note, to `(T, E?)`, the `if err?:` shape,
  `T?` and `null`. Where a note's *argument* depended on `Result` being a type
  rather than merely on its spelling, the note now says so in place rather than
  quietly restating the claim — `collections-and-chains.md` §6.3 and
  `ffi-c-boundary.md` §6 are the two worth reading before relying on the model.
- **`null` is not yet on the reserved list.** Core spec §4.2 gained the literal
  and §13 did not gain the word; §13's own prose says revision 2 "added
  `interface`" where `syntax-revision-2.md` §7 says it added `interface` **and**
  `null`. `stdlib-shape-and-packages.md` §4.4 records the same ask. This is an
  open spec edit, not a note-level one.
- **Say what you contradict.** A note that disagrees with a sibling names the
  section and gives the reason. Several already do, and it is the only reason
  the seams are visible at all.
- **Price the cost.** Every decision states what it costs, including the ones
  that are obviously right.
