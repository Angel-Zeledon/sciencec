# Science — Design: publishable output

Date: 2026-09-16
Status: draft for review
Owns: the path from a computed quantity to a typeset artefact — the typed table,
the typed figure, the significant-figure rule, the methods record, and the
document record that backends render.
Diagnostics claimed: **`SR0001`–`SR0099`**, a namespace outside `SC` — §10 gives
the reason. No `SC` code is claimed; one is amended.
Related, and in reading order: `native-dependencies.md` §5.4 (Decision 9,
`.science.provenance`) and §10, whose ask on `data-io.md` is the seed of this
note; `reproducibility.md` §5, which designs the build and run records this note
builds on top of and does not duplicate; `uncertainty.md`, which supplies the `u`
that §6 rounds by and which adopted this note's §6 in return;
`statistical-validity.md` §6, which owns the methods record outright and with
which §7 has one explicit disagreement; `scientific-libraries.md` §12 and §7;
`unit-literals.md` §5.1, §7.2 and §7.3; `strings-formatting-and-docs.md` §2, §3.3
and §5; `const-expression-arithmetic.md`, which is now building the largest thing
§9.1 depends on; `data-io.md` §4 and §8; `stdlib-shape-and-packages.md` §5–§6.
Written in the syntax of `syntax-revision-2.md` and `syntax-revision-3.md`.

**A note on ordering.** `reproducibility.md`, `uncertainty.md`,
`statistical-validity.md`, `const-expression-arithmetic.md` and `effects.md` all
landed within an hour of this note's first draft. §3, §6.1, §7 and §12 were
rewritten against them; where this note still disagrees with a sibling — there is
exactly one place, §7.5 — it says so and gives the reason.

---

## 0. What this note is

`native-dependencies.md` §5.4 built a provenance record into every binary and
then, in §10, asked `data-io.md` to stamp it into HDF5, `.npz` and Parquet
metadata by default, calling that *"the single highest-value thing in this note"*.
That ask stops one step short of where it was going. A Parquet file that knows
what produced it is useful to the next program. It is not useful to the reader of
a paper, because the reader of a paper never opens the Parquet file.

This note carries the record the rest of the way: **to the document**.

The ambition it is designing for is a pipeline from which a paper comes out
directly — not a notebook that gets screenshotted, not numbers that get copied
into a manuscript, but the typeset artefact, with its tables, its figures, its
methods record and its provenance appendix, produced by the program that computed
them.

### 0.1 What it is not

It is not a literate-programming system, and §2 spends its length explaining why
building one would be a waste of the language's one advantage. It is not a
plotting library — §5 designs the *description* of a figure and names the
backends that draw it, and drawing is somebody else's note. It is not a
typesetting engine. It does not attempt to author anything a human would
recognise as writing, and §7 makes that a rule rather than a limitation.

### 0.2 The sentence this note is trying to earn

One sentence, narrow on purpose, meant to be quotable and meant to be true:

> **Every number in this document was produced by the program that produced this
> document, in the unit its type says, rounded by the rule stated in the
> appendix.**

Nothing beyond that sentence is claimed. §11 is about the distance between that
sentence and a paper, and the distance is large.

---

## 1. The failure this addresses, and its size

The boundary between an analysis and a manuscript is a copy-paste boundary, and
copy-paste boundaries leak. The leaks are documented, they are common, and they
are not the kind of error that peer review catches, because a reviewer reads the
number in the table and has no way to compare it with the number in the program.

The four shapes it takes:

1. **A number recomputed and never updated.** A reviewer asks a question, the
   analysis is re-run with one more exclusion, the abstract is updated, and the
   third row of Table 2 is not. Nothing in the manuscript records that the table
   and the abstract now disagree.
2. **A figure regenerated from different data than its caption claims.** The
   script was edited between the figure and the caption. The caption is prose and
   prose is not checked against anything.
3. **A unit converted twice, or not at all.** The value in the program is in
   millimoles; the column header says moles; somebody divided by a thousand
   somewhere and somebody else divided again.
4. **A number rounded to a precision its uncertainty does not support**, which is
   the most common of the four and the only one that is purely mechanical.

The prevalence of (1) and (4) is measurable, because a machine can re-derive them
from the published text alone, and the measurements are not reassuring.

- **`statcheck`** recomputes a p-value from the test statistic and degrees of
  freedom printed beside it. Nuijten et al. (2016, *Behavior Research Methods*)
  ran it over more than 250 000 p-values in eight psychology journals across
  1985–2013: **about half of the papers using null-hypothesis testing contained
  at least one inconsistency**, and **about one in eight contained a "grossly
  inconsistent" one** — an inconsistency large enough to flip the reported
  significance conclusion. Gross inconsistencies skewed toward p-values reported
  as significant.
- **GRIM** (Brown & Heathers, 2017) checks whether a mean is arithmetically
  possible given an integer-valued measure and a stated N. Of 260 articles, 71
  were testable; **about half of those had at least one impossible mean** and over
  a fifth had several.
- **The gene-name case is a unit-and-label failure in its purest form.** Ziemann
  et al. (2016, *Genome Biology*) found that **19.6% of papers with supplementary
  Excel gene lists** contained gene symbols silently converted to dates —
  `SEPT2` to a September date, `MARCH1` likewise. A 2021 follow-up found the rate
  had risen to roughly **30%**. The eventual fix was not a tool: in 2020 the HUGO
  Gene Nomenclature Committee **renamed about 27 genes** so that a spreadsheet
  could not misread them — `MARCH1` became `MARCHF1`, `SEPT1` became `SEPTIN1`.
  An entire naming authority changed the names of things in the world because a
  program's type system could not tell a symbol from a date.
- **Mars Climate Orbiter** remains the canonical unit loss: **$327 million**, lost
  in September 1999 because navigation software emitted thruster impulse in
  pound-force and the consumer assumed newtons.

The class of error at issue is not *statistics done badly*. It is **a number that
disagrees with another number in the same document**. That is a consistency
failure, and consistency failures are exactly what a compiler is for.

A note of scale, stated here so §11 does not have to argue it. The largest single
source of this class of error in the published literature is almost certainly the
spreadsheet — Panko's audit programme and the EuSpRIG literature put the fraction
of real-world spreadsheets containing at least one error at **86–94%**, with
per-cell error rates around 4–5%. This design does nothing about spreadsheets,
because a scientist using a spreadsheet is not using Science. Every claim below is
conditional on adoption and none of them is a claim about the literature as it
stands.

---

## 2. What the language contributes that a library cannot

This is the crux. If the answer is "nothing", the right thing is to say so and
stop, because literate programming is thirty years old and the incumbents are
good.

### 2.1 The incumbents, described accurately

Sweave (1997), knitr, R Markdown, Jupyter, Jupyter Book / MyST, Observable and
Quarto have all solved the transcription problem in its literal form. Quarto is
the strongest of them and is the one worth testing against, because anything true
of Quarto is true of the field.

What Quarto does today, checked rather than assumed:

- **Inline expressions splice a computed value into prose.** The cross-engine
  spelling since Quarto 1.4 is `` `{r} nrow(mpg)` `` and `` `{python} …` ``; the
  knitr-only `` `r …` `` still works. The number in the sentence and the number in
  the analysis are the same number. **There is no copy-paste boundary.**
- **Tables come from data frames** through `gt`, `tinytable`, `kableExtra` or
  `flextable` in R and `great_tables` in Python, with per-column formatting,
  spanners and footnotes.
- **Cross-references** are a reserved-prefix label system — `#tbl-rates`,
  `#fig-decay` — cited as `@tbl-rates`, auto-numbered and hyperlinked.
- **Multiple backends**: HTML, PDF through LaTeX or through Typst, native Typst,
  `docx`/`odt`/`rtf`, EPUB and **JATS**. Typst arrived in Quarto 1.4 and was
  substantially extended in 1.9 (March 2026) with Typst book projects, margin
  layout and offline package bundling.
- **Journal templates** through `quarto use template quarto-journals/<name>` —
  ACM, PLOS, Elsevier and others, community-maintained.
- **Manuscript projects** (1.4+) render the article into journal formats *and*
  publish the underlying notebooks as linked browsable HTML beside it, so a reader
  can inspect the code behind each result.
- **`freeze` and `renv`** give a re-execution story, and `sessionInfo()` gives a
  software appendix.

MyST-NB's `glue` mechanism does the same splicing for Jupyter Book, decoupled:
`glue("key", value)` in a cell, `` {glue:}`key` `` anywhere in the prose.

Two things the search did **not** find, offered as absence of evidence rather than
evidence of absence. Quarto has no semantic consistency check of any kind — no
mechanism that relates a value's unit or rounding to the caption or column header
beside it; its open linting work is about malformed captions and Markdown
structure. And Quarto's own documentation warns that `freeze` caches *code
re-execution only* and does not notice that an input data file changed — which is
§1's failure (1) reintroduced by the feature meant to prevent it, and is the
reason §8.4 exists here.

**Conclusion: weaving is solved, and Science should not try to out-weave
Quarto.** A Science literate-programming mode would be a worse Quarto with no
ecosystem, and the audience that already has Quarto would be right to ignore it.

### 2.2 The four things that survive

What none of the incumbents has is a *typed* quantity. Everything below follows
from that one absence, and each is a consequence a library in a dynamically typed
language cannot have.

**(a) The value that gets spliced is a bare double.** In Quarto,
`` `{r} round(mean(d$temp), 2)` `` yields a number with no unit, no uncertainty
and no identity. It will splice a mean in kelvin under a column header that says
`°C` without a murmur, because nothing in R connects the two. The header is a
string the author typed. The value is a double. They are not related objects and
no amount of library work makes them related, because R will always let you pass
a string.

This is the narrow, correct form of the claim, and it is worth stating precisely
because the loose form is false: **you can write a unit-aware table library in
Python.** People have; `pint` plus `great_tables` gets you most of the way. What
Python cannot do is make the disagreement *inexpressible*. A library can
discourage passing a wrong header; a type system can arrange that there is no
header parameter to pass. §4.3 is that arrangement, and the difference between
"the library discourages it" and "the program does not compile" is the whole
contribution.

**(b) The check happens before the run, not at render.** Quarto finds out at
render time, which is after the compute. A Science program whose figure axes
disagree fails at `sciencec build`, before the eight-hour job is submitted. For
the audience `native-dependencies.md` §6 describes — the one whose failed job
queued for two days first — that is not a small difference.

**(c) The precision is derived, not typed.** `round(x, 2)` is a number the author
chose. In a language where a measured quantity carries its standard uncertainty,
the number of digits is a function of the value, and §6 gives the function. A
library in R can offer that function; it cannot make it the default, because most
of the values flowing through it are bare doubles for which the function is
undefined.

**(d) The provenance is read, not self-reported.** `sessionInfo()` is a process
asking itself what it loaded, it lists R packages and not the BLAS underneath
them, and reading it requires running R. `sciencec provenance ./a.out` reads a
section out of an object file: it names the linked native libraries, their
variants, their probe strings and the verification tier, and it works on a
two-year-old binary on a machine with no GPU and no queue allocation
(`native-dependencies.md` §5.4).

There is a fifth, smaller one, and it is free: **a cross-reference can be a
binding rather than a string** (§4.7).

### 2.3 Decision 1

> **Decision 1. The contribution is the typed quantity, not the weave. This note
> designs the artefacts a typed quantity makes possible — a table whose headers
> are derived, a figure whose axes are derived, a rounding that is computed, a
> methods record that is read rather than written — and it designs no
> literate-programming system at all.**
>
> **Corollary: Science should be a good input to Quarto.** §8.3's data-package
> rendering exists partly so that a Quarto or LaTeX author can consume a Science
> analysis without adopting a Science document pipeline.

**Rejected alternative: a Science literate mode**, a `.sciencemd` file with
executable blocks, weaving and caching. Rejected for three reasons. It competes
where the incumbents are strongest and Science is weakest, namely ecosystem and
interactivity. It requires the interactive tier, which is F1 and not yet built.
And it would put the compiler in the business of caching cell outputs, which is
`freeze`'s problem and `freeze`'s well-known failure mode — a stale cached value
in a document that claims to be reproducible is precisely the error in §1's list
(1), reintroduced by the tool meant to prevent it.

**Rejected alternative: claim nothing and close the note.** Considered seriously,
because "Quarto already does this" is very nearly right. It is wrong on exactly
the four points of §2.2, and each of the four is a documented failure mode in §1's
list. That is enough to justify the note and not more.

---

## 3. The artefact is data, and it is the third record

Before the table and the figure, the container — and the container has to be
placed carefully, because `reproducibility.md` §5 has already designed two
records and `statistical-validity.md` Decision 6 has already refused to add a
third for its own domain.

### 3.1 What already exists

- **The build record** (`reproducibility.md` Decision 11, extending
  `native-dependencies.md` Decision 9): static, in `.science.provenance`,
  readable with `readelf` and no toolchain, canonical JSON with a `schema`
  integer, and identified by a **provenance id** — the SHA-256 of its canonical
  bytes, quoted as twelve hex characters, `pv:9f3a1c04e7b2`.
- **The run record**: embeds the build record verbatim and adds argv, the
  environment read, input digests, Tier C resolutions and the exit status.
  `data-io.md`'s writers stamp it into outputs by default.
- **The `statistics` object** (`statistical-validity.md` Decision 6): fields
  *inside* those two records, not a separate file, recording which tests ran,
  which corrections were applied to which families, and every escape taken.

**None of these carries a number**, by `reproducibility.md` §5.1's rule that the
record contains no floating-point values anywhere, which
`statistical-validity.md` §6.1 sharpens into a principle worth quoting:

> The record records the method, not the results. … A record that also carried
> the numbers would be a second, unverified copy of the results, and the first
> time it disagreed with the paper nobody would know which to believe.

### 3.2 Decision 2

> **Decision 2. There is a third record — the *document record* — and it is the
> one place in the design that does carry numbers, because it is not a record
> *about* the output, it **is** the output. Every rendered artefact, including
> LaTeX, is derived from it and none of them is the source of truth. It embeds
> the run record's `pv:` id by reference and copies nothing from it.**

**This answers `statistical-validity.md` §6.1's objection rather than ignoring
it.** That objection is about a *parallel* copy — a record beside the paper,
holding the same numbers, produced by a different path, able to disagree. The
document record is not beside the paper; it is upstream of it. The table in the
PDF is a rendering of the record, so the two cannot disagree for the same reason
a compiled binary cannot disagree with its own object files. The failure mode
described is real and it is the failure mode of *this note's rejected
alternative*, which is a `.tex` file plus a JSON sidecar that both claim to be
the results.

The record contains: the document's blocks in order; every table as columns of
cells; every figure as a description with its series and its data; the data
lineage; the `pv:` id of the run record; and, for each cell, three things rather
than one.

### 3.3 A cell is three strings, and no floats anywhere

> **Decision 2b. A numeric cell in the document record holds the value's shortest
> round-tripping decimal *string*, the uncertainty's string where there is one,
> and the *rendered* text produced by §6's rule. No field of any Science record is
> a JSON float, in this record or the other two. `reproducibility.md` §5.1's
> canonicalisation applies unchanged.**

```json
{ "value": "9.80665", "u": "0.00015", "text": "9.80665 ± 0.00015",
  "unit": "m/s²", "rule": "pdg-354/half-even", "derived": true }
```

Three properties fall out, and the third is the one that justifies the
awkwardness:

1. **The record canonicalises.** One rule for all three records, and a record
   about floating-point reproducibility is not itself sensitive to float
   formatting — `reproducibility.md` §5.1's argument, extended to the only record
   that has numbers in it.
2. **Two renderers cannot disagree**, because they do not round. The rounding
   happened once, in the program, and `text` is the answer. A LaTeX table and a
   Typst table of the same data are the same characters.
3. **The rounding is independently checkable.** `value`, `u`, `rule` and `text`
   are all present, so a `statcheck`-style tool can re-derive `text` from the
   first three and report a mismatch — which means §6's guarantee is verifiable
   by a third party rather than asserted by the tool that produced it. That is
   the difference between a claim and a check, and it costs two extra strings per
   cell.

It is JSON for the reason `data-io.md` §9 gives for its own formats and
`reproducibility.md` §5.1 gives for the provenance record: it must be readable by
somebody who does not have the toolchain. A reviewer with `jq` can extract the
third row of Table 2.

### 3.4 Why not render directly to LaTeX and skip the record

Three reasons, and the third is decisive.

1. **Multiple backends need one source.** A design that picks LaTeX today and
   wants Typst in eighteen months has to re-derive every rounding decision in a
   second renderer, and the two will drift.
2. **The record is checkable and a `.tex` file is not.** §3.3's rounding check,
   §8.4's staleness check and §7's honesty marker all operate on the record. A
   checker that had to parse LaTeX to find a number would be a worse `statcheck`.
3. **The most useful rendering is not a document** (§8.1). The author's own
   template is the thing that must not be touched, so the unit of delivery has to
   be the individual table, figure and number — which means the record has to be
   addressable at that granularity, which means it has to be a record.

**Rejected alternative: fold the document into the run record.** This is what
`statistical-validity.md` did for its domain, correctly, and it does not work
here for one measurable reason: that record lives in an object-file section under
`native-dependencies.md` §5.4's 8 KB cap, and its contents are O(call sites).
A document's contents are O(data) — a figure with its series inlined is megabytes
— and nothing that size belongs in a section that `readelf` prints. The
principle *"a second mechanism for the same kind of fact is a mistake"* holds;
this is a different kind of fact, and the size is the evidence.

**Rejected alternative: the record is the compiler's, emitted at build time.**
Most of what goes in it does not exist until the program runs. The compiler's
contribution is the static half — the doc comments, the schemas — and the program
merges the two when it writes the record.

**Cost.** A third artefact, a schema to version alongside two others, and a rule
for when the record's schema is newer than the renderer's: the renderer refuses
and names both versions (`SR0001`). And the discipline of §3.3 — no floats —
which is genuinely annoying to implement and is the right rule anyway.

---

## 4. The typed table

### 4.1 A column is a named array of one quantity type

```science
type Column of Q:
    name: String
    values: Array of Q
    note: String?
```

`name` is the author's noun — `"Rate constant"`, `"Plasma concentration"`. It is
the only string the author writes and it is deliberately the only one. Everything
else about the header comes from `Q`:

| Header component | Where it comes from |
|---|---|
| The noun | `name`, author-written |
| The unit symbol | `Q`'s exponent vector, through `unit_symbol()` (`unit-literals.md` §7.2) |
| The common power of ten | computed from `values`, shared across the column (§6.4) |
| The number of digits in each cell | computed from each cell's uncertainty (§6) |
| The footnote | `note`, or the field's `##` doc comment (§4.4) |

### 4.2 A table is heterogeneous, and that is the only hard part

A table's columns have different types, and F0 has no type-level lists. The
answer is the one the language already uses for `Error`:

```science
interface TableColumn:
    def header_noun(borrowed self) -> String
    def unit_symbol(borrowed self) -> String
    def len(borrowed self) -> U64
    def cell(borrowed self, index: U64) -> Cell
    def note(borrowed self) -> String?

Column of Q implements TableColumn where Q: DisplayNumber:
    ...
```

`Table` holds an `Array of (any TableColumn)`. Each `Column of Q` is
monomorphised — so `unit_symbol()` is a constant in each instantiation,
`unit-literals.md` §7.1's finding — and boxed at the moment it enters the table,
which is the moment its type stops mattering because the only remaining operation
is rendering.

```science
use report (Table, Column)

let rates be Table.new("Fitted first-order rate constants")
    .column("Temperature", run.columns().temperature)
    .column("Rate constant", fits.map(each.k))
    .column("Half-life", fits.map(each.half_life))
    .caption("Rate constants fitted to the decay of each replicate.")
```

`.column` is `def column of Q(mutable self, name: String, values: Array of Q)
-> Table where Q: DisplayNumber`, so it needs `data-io.md` §11.2's
explicit-type-arguments ask only in the cases where inference cannot see `Q` from
the argument, which is none of them. The builder consumes and returns `self`,
which is `collections-and-chains.md`'s ownership-through-a-chain shape.

**Rejected alternative: a variadic generic `Table of (Q1, Q2, …)`.** It would
keep the column types static all the way to rendering and it would let a
row-wise operation be typed. It needs type-level lists, which `broadcasting.md`
§11 already prices as the expensive half of the const-expression ask, and it buys
nothing, because a table has exactly one operation and that operation is
rendering. Boxing at the table boundary costs one indirection per cell in a
code path that is already going to write bytes to a file.

**Rejected alternative: a dynamic table, columns keyed by string.** This is
pandas, and `data-io.md` §4.3 already rejected it for `Frame` with the argument
that applies verbatim here: a `df["temprature"]` typo failing at render time is
the class of mistake the language exists to remove.

### 4.2.1 The gradient is dropped at the table boundary

> **Decision 2c. `Column.new` reduces an `Uncertain of T` to its nominal value and
> its *marginal* standard uncertainty, discarding the gradient. A `Cell` is a pair,
> not a derivation.**

`uncertainty.md` Decision 3.3 makes an `Uncertain of T` carry a nominal value and
a **gradient** over the runtime identities it was derived from, which is what
makes its correlations exact and is the reason §6's `±` can be trusted at all.
Its §3.5 prices that in bytes, and the price is per value.

A table column of ten thousand uncertain measurements would carry ten thousand
gradients into a rendering that needs two numbers from each. Worse, nothing
downstream can use them: once a value is in a table, no further propagation
happens, so the gradient is dead data in the largest artefact in the design.

Dropping it is therefore free of any loss **within** the document — and it is
exactly `uncertainty.md` §7.3's *"serialisation decorrelates silently"*, arriving
one hop earlier and more visibly. The consequence is stated in §11.2 rather than
buried: a reader who recomputes a derived quantity from two columns of this table
will get the independent-error answer, which is wrong whenever the columns are
correlated, and the table cannot warn them because the information was gone before
the table existed. The mitigation is that a *derived* quantity should be computed
in the program and given its own column, where its uncertainty is the propagated
one, rather than left for the reader to assemble.

### 4.3 The header's unit is derived, never written

> **Decision 3. A column header is not a string. It is a noun supplied by the
> author and a unit derived from the column's type, assembled by the renderer. A
> header that disagrees with its column's unit is not an error the compiler
> catches; it is a statement the language has no way to express.**

This is the direct answer to the question the brief asks — *what happens when a
column's unit and its header disagree, is that a compile error, and can it be?*

**It cannot be a compile error, and it does not need to be.** A compile error
requires two things to compare. Here there is one thing. The author never writes
`"Temperature (K)"`, so there is no `(K)` to check against the type; the renderer
writes the `(K)`, from the type, every time the document is produced. The
disagreement is removed by construction rather than detected.

That is a better outcome than a diagnostic and it is worth saying why: a
diagnostic is only as good as the author's willingness to act on it, and a
diagnostic that fires on a header string would fire on legitimate prose (`"Mass
(dry)"`, `"Yield (%)"`) often enough to be silenced.

**The one residue, and it is a real one.** An author can still write the unit
*into the noun*: `.column("Temperature (K)", …)`. Nothing in the type system
prevents a string from containing "(K)". This is a habit, not a type error, and
it needs a tool check rather than a compiler check:

> **`SR0003`.** At render time, if a column noun ends in a parenthesised or
> bracketed token that parses as a unit expression under `unit-literals.md` §3.1's
> grammar, the render fails and names the derived unit.
>
> ```
> error[SR0003]: the column noun "Temperature (K)" contains a unit
>   --> report.json: table "Fitted first-order rate constants", column 0
>    |
>    = the header's unit is derived from the column type and is `K`
>    = this column would be headed "Temperature (K) / K"
> help: write the noun alone
>    |    .column("Temperature", run.columns().temperature)
> ```

The check is decidable because `unit-literals.md` §3.1's unit grammar is small and
closed, and it is cheap because it runs once per column and not once per cell.
It is a heuristic in exactly one direction — it can reject a noun that legitimately
ends in a unit-shaped word — and the escape is `.column_raw_noun(…)`, which is
deliberately ugly.

**Cost of Decision 3, stated.** The author loses control of the header's layout.
The renderer decides whether the unit goes in parentheses, after a solidus, or on
a second line, and journals disagree about this. The answer is that the header
style is a property of the *renderer's template*, not of the table, which is
where a journal style belongs anyway — and it means that switching journals
restyles every header in the paper rather than requiring every `.column` call to
be edited. That is the trade and it is the right way round.

### 4.4 A `Frame` becomes a table through its record type

> **Decision 4. `Table.from_frame of R(frame)` derives the column order and the
> column nouns from `R`'s field declaration order and field names, and the column
> footnotes from the fields' `##` doc comments.**

```science
## A fitted decay, one per replicate.
type Fit:
    ## Replicate identifier, as written in the run log.
    replicate: String
    ## Bath temperature at the start of the run.
    temperature: Temperature of F64
    ## First-order rate constant from a weighted least-squares fit.
    rate: Quantity of (Uncertain of F64, 0, 0, -1, 0, 0, 0, 0)

Fit implements Record

let table be Table.from_frame of Fit(fits)
    .rename("replicate", "Replicate")
    .rename("temperature", "Bath temperature")
    .rename("rate", "Rate constant")
    .caption("…")
```

Two things fall out that no library in another language can have.

**A header cannot name a column that does not exist.** `.rename` takes a field
name, and a field name that is not a field of `R` is a resolution failure at
compile time, not a silent no-op at render time. This is `data-io.md` §4.3's
argument extended one hop.

**The field's documentation becomes the table's footnote.** The `##` run on
`rate` — *"First-order rate constant from a weighted least-squares fit"* — is
already preserved into a metadata section by `strings-formatting-and-docs.md` §5.3
and is already the thing the author wrote to explain that field to a reader of the
code. The reader of the paper needs the same sentence. Making the documentation of
the data structure be the documentation of the table means there is one place to
change it, which is the same argument §5.4 of that note used to reject `@param`
tags.

This is the single clearest example of the language contributing something a
library cannot, and it is worth being precise about why: **in Python a DataFrame
column has no docstring.** There is nowhere for the sentence to live except a
second dictionary that the author maintains by hand and that goes stale. In
Science the sentence lives on the field, the compiler carries it, and the renderer
reads it.

**The ask this creates** is small and specific: the doc-comment metadata table of
§5.3 must be indexed by *field* and *variant*, not only by function and type.
§12 states it.

### 4.5 The display unit is a value, and it closes a seam

`unit-literals.md` §5.1 stores every `Quantity` in SI coherent base units, so a
column of `Length of F64` prints in metres whether or not metres is what the
reader wants. §7.3 of that note calls this out as *"the only place in the design
where a unit can still be wrong at runtime"*, because the workaround is
`f"{d.in(FOOT)} ft"` and the `ft` in the string is unchecked.

For a table the seam closes without touching the format grammar:

> **Decision 5. The display unit is an optional value argument to the column, and
> the header symbol is derived from that same value. One expression determines
> both the conversion and the label.**

```science
use physics.units (MILLIMETRE, HOUR)

let t be Table.new("Sizes")
    .column_in("Grain diameter", sizes, MILLIMETRE)
    .column_in("Residence time", times, HOUR)
```

`column_in of Q(name, values, unit: UnitOf of Q)` converts each value once and
takes the header symbol from `unit`. There is no second place to write `mm`, so
there is nothing to keep in sync.

**This is evidence bearing on `unit-literals.md` §7.3's open request, not a
contradiction of it.** That note asks the formatting note for a unit selector in
the format spec and states its own position that it is worth a spec code. This
note observes that the case which motivates the request most strongly — a column
of numbers sharing one unit — is served better by a value argument than by a
format code, because a format code would have to be repeated per cell and would
be a *string* rather than the `UnitOf` constant. Whether prose still needs a
format selector remains that note's question and the formatting note's call. This
note takes no position on it beyond removing one of its customers.

### 4.6 What is still not checkable, named

Honesty about the limit of Decision 3:

- **A column of bare `F64`.** If the author's `Frame` has `temperature: F32`
  rather than `Temperature of F32`, the column has no unit and the header has no
  unit, and nothing is wrong and nothing is guaranteed. The language cannot
  compel the author to use `Quantity`, and `unit-literals.md` §1 is a whole note
  about making that choice cheap enough that they do.
- **A wrong unit correctly labelled.** A value that is genuinely in millimoles
  but was constructed as `5.0<mol>` is a `Quantity` of the wrong magnitude, and
  the header will faithfully say `mol`. The type system checks dimension, not
  correctness of measurement.
- **Two columns whose rows are not aligned.** `Column` holds an `Array`, and two
  arrays of equal length say nothing about whether row 7 of one describes the same
  specimen as row 7 of the other. `Table.from_frame` does guarantee it, because a
  `Frame` is one table; `.column` from two independent arrays does not. The rule
  the design can enforce is a length check (`SR0004`) and nothing more, and
  `from_frame` is therefore the form the documentation should show.

### 4.7 Cross-references are bindings

> **Decision 6. A table or figure yields a handle, and a reference is a use of
> that handle. There is no string label namespace.**

```science
let rates be Table.new("…")…
let decay be Figure.new()…

doc.text(f"The fitted constants are given in {rates.reference()}, and the "
         f"decay curves in {decay.reference()}.")
```

`\ref{tab:rats}` renders as `??` in a PDF and is a warning that everyone has
learned to ignore. Quarto is better — its labels are checked and an unresolved
`@tbl-rats` is reported — but it is still a *string namespace* resolved at render
time, which means the error is found after the compute rather than before it, and
means the check lives in the document tool rather than in the language.
`rates.reference()` on an undefined `rates` is `SC0201`, an unresolved name,
reported by the compiler, and the build stops. The numbering — "Table 1" — is
assigned by the renderer in document order, which is the only party that knows the
order.

This is free. It requires no new language machinery; it requires only that the
report library never expose a `label: String` parameter. Naming it as a decision
matters because the string-label API is the one every existing system has and the
one a port of `gt` would copy.

**Cost.** A reference to a table defined later in the file is a forward reference
to a binding, which in a language with no hoisting means the table must be
constructed before the prose that refers to it. In practice the document is built
by a function that constructs its artefacts and then assembles its text, which is
a reasonable shape and the one the worked example uses. Where it is genuinely
inconvenient the escape is to construct the table early and place it late:
`doc.place(rates)`.

---

## 5. The typed figure

### 5.1 The axes are type parameters

> **Decision 7. A figure is `Figure of (X, Y)`. Every series plotted on it has
> type `Series of (X, Y)`. An axis label is a noun from the author and a unit
> derived from the type parameter, exactly as a column header is. Plotting a
> series whose ordinate is in a different unit on the same axis is a type error
> with no new machinery — ordinary generic unification rejects it.**

```science
use report (Figure, Series)

type Series of (X, Y):
    name: String
    x: Array of X
    y: Array of Y

let fig be Figure.new()
    .line(Series.new("Replicate 1", times, concentration_1))
    .line(Series.new("Replicate 2", times, concentration_2))
    .x_name("Time since injection")
    .y_name("Plasma concentration")
    .caption("Plasma concentration against time for each replicate.")
```

`times` is `Array of (Time of F64)` and `concentration_1` is
`Array of (Concentration of F64)`, so `fig` is
`Figure of (Time of F64, Concentration of F64)`, and the axis labels render as
*Time since injection / s* and *Plasma concentration / mol m⁻³*, with the unit
from the type and the noun from the author.

A third series whose ordinate is a `Temperature of F64` does not unify and the
call does not compile. This is the clearest single statement of §2's claim:
`matplotlib`'s `ax.plot(x, y)` takes two arrays of doubles and cannot know; the
error is not detectable in Python at any cost, because the information was thrown
away at the point the value was created.

**Two axes with different units.** `.right of Y2(…)` yields a
`TwinFigure of (X, Y, Y2)` with two ordinate types and two derived labels. Beyond
two, the design refuses: a figure with three ordinate scales is a figure that
should be three panels, and `.panel(fig)` composes them.

**Scales.** `.y_scale(Scale.Log10)` is a rendering property and does not change
the type. A quantity that is *already* logarithmic — decibels, nepers, pH, which
`unit-literals.md` §6.3 gives their own treatment — carries that in its type, and
its axis is labelled and ticked accordingly without the author saying so.

### 5.2 A figure is a description; rendering is a backend

> **Decision 8. `Figure` is a description that serialises into the document
> record. The default backend emits the typesetter's own plotting source with the
> data inline — PGFPlots for LaTeX, CeTZ/`lilaq` for Typst, SVG for Markdown and
> HTML. Above a point threshold (default 20 000 marks per series) the backend
> falls back to a rasterised or path-simplified image and records that it did.**

Emitting the typesetter's source is the right default for a paper for four
reasons: the figure uses the document's fonts and sizes rather than matplotlib's;
it is vector and stays vector through the journal's production pipeline; the
*data* is present in the document source, so a reader who wants the numbers behind
Figure 3 can extract them; and it means Science ships no rasteriser.

The threshold exists because inlining a 2-million-point scatter into a `.tex` file
produces a file no typesetter will compile. The fallback is recorded in the
document record, because a reader should be able to tell whether the figure they
are looking at contains its data or a picture of it.

**Cost, stated plainly.** Three backends with different capabilities, which will
not be pixel-identical and should not pretend to be. The design's response is to
keep the description deliberately small — lines, points, bars, errorbars, bands,
step, histogram, heatmap — and to refuse anything that cannot be expressed in all
three. A figure that needs more than that is an image the author produces
elsewhere and attaches with `doc.image(path, caption:)`, which is an explicit
un-guaranteed escape and is documented as one.

**Rejected alternative: Science ships a plotting library that rasterises.** It is
a large piece of work with no connection to the language's advantage, it makes
fonts wrong, and `stdlib-shape-and-packages.md` §5.7's criterion — a component
that changes for reasons unrelated to the language — puts it outside the
toolchain regardless.

### 5.3 What a figure's record makes falsifiable

§1's failure (2) — a figure regenerated from different data than its caption
claims — is not preventable, because the caption is prose. What the record can do
is make the claim checkable:

- every series records the hash of the `Frame` or array it came from, and its
  length after any filtering;
- a table and a figure built from the same binding carry the same input hash, so
  *"Figure 2 and Table 1 describe the same 41 specimens"* is a claim a reader can
  verify with `jq` and no access to the data;
- the exclusion count is `frame.rejected_count()` plus the difference each filter
  made, which `data-io.md` §4 already tracks and which is the true value of the
  sentence *"we excluded N observations"*.

That is less than checking the caption and it is more than any current tool does.

---

## 6. Significant figures

This is small, and it is the difference between a table a journal accepts and one
a reviewer sends back.

### 6.1 The rule

> **Decision 9.** For a value `v` with standard uncertainty `u`, both `F64` and
> both in the same stored unit:
>
> 1. If `u` is **zero**, the pair renders as `v` alone, with the default float
>    rendering of `strings-formatting-and-docs.md` §2.3, and no `±`. An exact
>    quantity has no uncertainty to match. If `u` is **not finite**, the pair
>    renders as `v ± nan` or `v ± inf`, spelled as §2.3 spells them.
> 2. Otherwise let `n = floor(log10(u))` and let `d` be `u / 10^(n-2)` rounded to
>    an integer — the three leading decimal digits of `u`, in `[100, 999]`. If
>    the rounding carries to 1000, increment `n` and set `d = 100`.
> 3. The number of significant digits kept in `u`, and the last retained decimal
>    place `p`:
>
>    | `d` | digits kept in `u` | `p` |
>    |---|---|---|
>    | `100 ≤ d ≤ 354` | two | `n - 1` |
>    | `355 ≤ d ≤ 949` | one | `n` |
>    | `950 ≤ d ≤ 999` | `u` rounds up to `1.0 × 10^(n+1)`; two | `n` |
>
> 4. Round **both** `u` and `v` to decimal place `p`, half to even.
> 5. Render both in fixed point with exactly `max(0, -p)` decimal places.
>    Trailing zeros are retained because they are significant.

**Step 1's split is `uncertainty.md` §7.2's amendment, accepted.** This note's
first draft grouped zero with non-finite and rendered both as the bare value. That
note flagged it, on `strings-formatting-and-docs.md` §2.3's own reasoning for
`-0.0` — *"it is a different float and hiding that has cost people days"* — and it
is right: a NaN uncertainty means the propagation hit something undefined, and a
bare `9.8` in a published table is the one rendering that looks fine and is not.
Zero means *exact*; NaN means *broken*; printing them the same way is the error.
The flag was raised there and is decided here, which is the correct division and
is recorded so the exchange is visible.

**Why the PDG rule and not the GUM's.** Step 3 is the Particle Data Group's
"354 rule", and it is chosen because it is the only widely used convention that
is *decidable*. The GUM (JCGM 100:2008 §7) says an uncertainty should usually be
given to **at most two significant digits** and, on which way to round, that
*"common sense should prevail"* — guidance for a human writing a calibration
certificate, not an algorithm a compiler can execute. The PDG rule stays inside
the GUM's "at most two digits" and makes the choice mechanical, so that two runs
of the same program produce the same table and a checker can reproduce it.

Step 4 is the part that matters and is the part most often got wrong: the value
and the uncertainty must end in the same decimal place. That rule is standard
practice consistent with the GUM and ILAC P14 rather than a sentence quoted
verbatim from the GUM, and this note states it as the convention it is.

Worked, against the brief's three cases and three more:

| `v`, `u` | `n` | `d` | digits | `p` | rendered |
|---|---|---|---|---|---|
| `9.80665`, `0.00015` | −4 | 150 | two | −5 | `9.80665 ± 0.00015` |
| ″ | | | | | not `9.806650000 ± 0.000150000` — step 5 fixes the count |
| ″ | | | | | not `9.81 ± 0.00015` — step 4 rounds *both* to `p` |
| `6.67430e-11`, `1.5e-15` | −15 | 150 | two | −16 | `6.674 30(15) × 10⁻¹¹` under `#u` |
| `1.23456`, `0.048` | −2 | 480 | one | −2 | `1.23 ± 0.05` |
| `1.23456`, `0.0096` | −3 | 960 | two | −3 | `1.235 ± 0.010` |
| `0.0021`, `0.15` | −1 | 150 | two | −2 | `0.00 ± 0.15` |
| `123456`, `2000` | 3 | 200 | two | 2 | `123500 ± 2000` |

Row 4 is CODATA 2022's value for the Newtonian constant of gravitation, which is
what the parenthetical form is *for*, and it is the reason `#u` exists.

The last two rows are the ones that look wrong and are right. `0.00 ± 0.15` is the
honest rendering of a measurement consistent with zero, and a design that
special-cased it into `0.0021 ± 0.15` would be reporting four digits the
measurement does not support.

**A correction to the brief's example, because it matters for step 1.** The
constant `9.80665 m/s²` is **exact by definition** — standard gravity was fixed by
the 3rd CGPM in 1901 and is codified in ISO 80000 — and so is the Planck constant
since the 2019 SI redefinition. Neither has an uncertainty, and
`physics.constants.STANDARD_GRAVITY` therefore takes step 1's branch and renders
as `9.80665 m/s²` with no `±` at all. The rows above are a *local gravimeter
reading* of 9.80665 with a standard uncertainty of 0.00015, which is a real
measurement and the right shape of example. Getting this distinction wrong in the
constants table would be exactly the kind of false precision §6 exists to remove,
and `scientific-libraries.md` §12.1's `physics.constants` must carry the
exact-versus-measured distinction in the data, not only the uncertainty value.

**Asymmetric uncertainties** — `v +a −b` — take `p` from the *smaller* of `a` and
`b`, so that neither is over-rounded, and render both to `p`.

**Half to even**, not half away from zero, because a table is many roundings and
half-away-from-zero biases their sum upward. The mode is recorded in the document
record as the `rule` field of §3.3 so that a `statcheck`-style tool can reproduce
the rounding exactly
rather than allowing a tolerance.

### 6.2 The format code

> **Decision 10. One new code in `strings-formatting-and-docs.md` §2.1's closed
> set: `u`. `#` selects the concise parenthetical form. Nothing else in the
> grammar changes.**

`g_local` below is a gravimeter reading, `9.80665 ± 0.00015 m/s²`:

| Spec | Renders |
|---|---|
| `f"{g_local:u}"` | `9.80665 ± 0.00015 m/s²` |
| `f"{g_local:#u}"` | `9.80665(15) m/s²` |
| `f"{g_local:.1u}"` | `9.8067 ± 0.0002 m/s²` — force one digit in `u` |
| `f"{g_local:.2u}"` | `9.80665 ± 0.00015 m/s²` — force two |
| `f"{g_local}"` | `9.80665 ± 0.00015 m/s²` — `u` is the default code for an uncertain quantity |
| `f"{STANDARD_GRAVITY:u}"` | `9.80665 m/s²` — exact by definition, so step 1 applies and there is no `±` |

Rules, in the shape of §2.4's table:

- `u` is legal when the type is `Uncertain of T`, or a `Quantity` over one.
  Applied to a plain float it is `SC0274`, with the fix *"this value has no
  uncertainty; use `.3g` and say so"*.
- `.precision` with `u` means **significant digits of the uncertainty**, and the
  legal values are `1` and `2`. `.0u` and `.3u` are `SC0274`.
- `#u` implies a common power of ten chosen so that `p ≤ 0`, because the
  parenthetical digits are only meaningful as trailing digits of the mantissa:
  `1234 ± 20` renders `#u` as `1.234(2) × 10³`.
- `u` composes with fill, alignment and width, which is what makes a column of
  them line up.

The check is `strings-formatting-and-docs.md` §2.4's existing compile-time check
and the diagnostic is its existing `SC0274`. This note claims no code and amends
one, which the README's convention permits.

**Rejected alternative: a second code for the scientific form**, `U` or `eu`.
Rejected because the common power of ten is a property of a *column*, not of a
cell: a scientific table puts `× 10⁻¹¹` in the header and not in forty rows. §6.4
makes it a column property and the second code stops being needed.

**Rejected alternative: `±` as a separate `Display` for a pair.** It would avoid
touching the format grammar. It fails §2's point (c): the whole value of this
section is that the *default* rendering of an uncertain quantity is the correct
one, and a rendering reachable only through a named function is a rendering nobody
reaches — which is `unit-literals.md` §1's argument, applied again.

### 6.3 A column with no uncertainty

> **Decision 11. A column whose element type carries no uncertainty requires an
> explicit `digits:` argument. The document record marks that column's precision
> as author-declared rather than derived, and `sciencec report --strict` refuses
> to render a document containing one.**

```science
.column("Bath temperature", temps, digits: 3)
```

Most real columns are like this: a measured temperature read off an instrument
with no uncertainty attached. The design must not pretend those are checked. The
marker in the record is what keeps §0.2's sentence true — *"rounded by the rule
stated in the appendix"* — because the appendix then states two rules, one derived
and one declared, and says which columns used which.

`--strict` exists for a journal or a group that wants the total guarantee, and it
is opt-in because a project that cannot meet it should still be able to use the
tool.

### 6.4 The column's common exponent

A column of values spanning one order of magnitude gets one power of ten, factored
into the header: *Rate constant / 10⁻³ s⁻¹*. The exponent is the one that puts the
largest magnitude in `[1, 1000)`, and a column whose values span more than three
orders of magnitude gets no common exponent and renders each cell in scientific
form, because factoring would produce a column of leading zeros.

This is a rendering decision and it lives in the record so that two renderers make
the same one.

---

## 7. The methods section

The compiler and the runtime between them know which statistical tests ran, which
corrections were applied, which random keys were derived, which files were read
and which libraries were linked. All of that can be emitted and all of it is true
by construction. The question this section exists to answer is *how much of it
should be emitted as prose*, and the answer is almost none.

### 7.1 Most of this is already owned, and this note defers

`statistical-validity.md` §6 and `reproducibility.md` §5 landed while this note
was drafting, and between them they own the methods record outright. Restated only
far enough to be checkable, and **adopted without amendment**:

- The record is the build record and the run record; there is **no separate
  methods artefact** (`statistical-validity.md` Decision 6). The `statistics`
  object lives inside them and inherits every rule about canonical JSON, the 8 KB
  cap, the degradation order, `--provenance=minimal` and stamping.
- The record records the **method, not the results**, and carries no floats.
- `sciencec methods` is a fourth reviewer verb over the same bytes
  (`statistical-validity.md` §6.2), rendering the `statistics` object as a
  human-readable listing: tests reached from `main`, corrections with the sites
  where their families were assembled, significance claims with their α,
  screening, and every uncorrected escape with its `because:` string.
- A **`Not checked` block** is emitted by `sciencec` and **cannot be suppressed**
  (`statistical-validity.md` Decision 7).
- The random-key lineage comes from the same place. `Key` is a value that is
  consumed (`scientific-libraries.md` §7.3), so the split tree is a static finite
  object and `sciencec methods` prints it. No other language can print this,
  because in every other language the seed is a global and the lineage does not
  exist as an object.

**This note therefore designs no methods record.** What it designs is the *last
hop* — how that record reaches a document — and it has three things to contribute,
in §7.2, §7.3 and §7.5.

### 7.2 What this note adds: the lineage the document needs and the record cannot carry

Three facts belong to the document rather than to the provenance record, because
they are O(data) rather than O(call sites) and because a reader of the *paper*
needs them where the paper is.

**The exclusion accounting.** `frame.rejected_count()`, the per-`RowError` reasons
(`data-io.md` §5.3), and the number of rows each filter removed. This is the true
value of the sentence *"we excluded N observations"*, which is very often wrong in
published papers because it is maintained by hand. It is a number, so by §3.1's
rule it cannot go in the provenance record; it goes in the document record as a
cell like any other, and is therefore subject to §6 and §3.3 like any other.

**The per-artefact input identity.** Each table and each series records the hash of
the `Frame` it came from and its length after filtering. The run record already
carries the *program's* input digests; what it cannot carry is which of those
inputs produced Figure 2, because it does not know what Figure 2 is. §5.3 is what
this buys.

**Software citations.** BibTeX entries for the linked native libraries and the
Science packages, generated from the manifest and the build record. It is the most
boring item in the note and the most likely to be used daily.

### 7.3 What this note adds: the appendix is included, not copied

The mechanism is one line of generated LaTeX:

```latex
\newcommand{\ProvenanceId}{pv:9f3a1c04e7b2}
```

`reproducibility.md` §5.1 makes the provenance id the quotable artefact and shows
a methods sentence citing it. The gap that leaves is that the author *types* the
twelve hex characters — and a hand-typed hash is a transcription boundary, §1's
entire subject, reintroduced in the one sentence that exists to close it.

So the id is a macro in §8.1's bundle, alongside the numbers, and the author writes
`provenance \ProvenanceId` and cannot mistype it. If the analysis is re-run, the
macro changes and the paper recompiles with the new id. If the analysis is deleted,
LaTeX fails with *undefined control sequence*.

The same mechanism handles the appendix: `sciencec report` writes `methods.tex`
from `sciencec methods`'s output and the author writes `\input{methods.tex}`, so
the `Not checked` block arrives in the document by inclusion rather than by paste.
That distinction is the whole of §7.5.

### 7.4 What must not be generated

> **Decision 12. This note's renderers — the table, figure and document pipeline —
> emit no running prose of any kind. There is no prose renderer in `report` and
> none in `sciencec report`. The one place in the whole design that generates a
> paragraph is `statistical-validity.md`'s `sciencec methods --draft`, which is a
> fixed template and is that note's decision, not this note's.**

The hazard, stated so the line has a reason. A generated methods paragraph that
reads as though a human wrote it is two separate problems.

The **plagiarism** problem is that the author submits, under their name, prose they
did not write and did not read carefully. The register of a fluent generated
methods paragraph is indistinguishable from the register of a written one, which is
exactly what makes it attractive and exactly what makes it a submission the author
cannot honestly sign.

The **honesty** problem is worse and is specific to methods. A methods section is
not a log; it is an account of what was done *and why*, written by somebody who
knows what they intended. A generated paragraph can only report what executed. It
will say *"Normality was assessed by the Shapiro–Wilk test"* when what happened was
that `shapiro_wilk` was called, and it cannot say whether the result was acted on.
A true sentence in a genre whose conventions imply more than it says is a
misleading sentence.

Four categories, refused in this note's own output:

1. **Any sentence with an interpretive verb** — *suggests*, *is consistent with*,
   *significant*, *supports*, *indicates*. These are claims and claims belong to
   the author.
2. **Any statement of rationale.** *"Welch's correction was used because variances
   were unequal"* asserts a reason for a choice; the tool knows the choice, never
   the reason.
3. **Any assertion that an assumption was checked.** The tool knows a test ran; it
   does not know whether the result changed anything, and the sentence implies it
   did.
4. **Anything in the register of the surrounding prose.** The generated material
   must be visibly machine-written, and looking machine-written is a feature.

**The mechanism, not just the policy.** A policy that appears only in documentation
erodes. There is no prose renderer in this note's pipeline, so the dishonest
artefact cannot be produced by asking the tool for it; every generated block carries
`"generated": true` in the document record and renders inside a named environment a
journal or a checker can find; and the appendix is a separate document part, so it
can be submitted as supplementary material, which is where it belongs.

**What none of this accomplishes**, said plainly: an author determined to pass off
a generated record as their prose can retype it. The design makes the honest path
the default and the dishonest path deliberate work. It cannot make the dishonest
path impossible and it should not claim to.

### 7.5 The one disagreement: `sciencec methods --draft`

`statistical-validity.md` §6.2 emits a generated paragraph behind a flag, and §6.3
defends it: it is a **template with holes**, so it can only say what it has slots
for; the italicised disclaimer is compiler-emitted and cannot be suppressed; and it
explicitly rejects model-generated prose for the reason §7.4 above gives.

**That is a materially stronger design than the caricature §7.4 argues against, and
three of §7.4's four categories are already impossible in it** — a template has no
slot for a rationale, none for an interpretive verb, and none for an assumption
claim. This note was wrong to imply that its rule covered a fixed template, and
says so here rather than leaving the overlap to be discovered.

So the disagreement is narrow, and it is about the disclaimer's **survivability**,
not its existence.

> **Disagreement with `statistical-validity.md` Decision 7, stated rather than left
> silent.** The disclaimer is a trailing italicised *sentence*. The author pastes
> the paragraph into their manuscript; in a methods section the trailing sentence
> reads as tooling noise; and it will be deleted — not dishonestly, but because it
> looks like something that was not meant to ship. Decision 7 guarantees that
> `sciencec` **emits** it. Nothing guarantees that it **arrives**, and the paragraph
> is designed to be pasted.

The amendment proposed, and it is small:

> **The draft paragraph's first sentence carries the provenance id** — *"Analysis
> performed with Science (provenance `pv:9f3a1c04e7b2`); the limits of this record
> are listed in the supplementary provenance appendix."* — **rather than carrying
> the caveat only in a trailing italic.**

Three reasons it is better:

1. **An author deletes a caveat and keeps a citation.** The id looks like the thing
   a methods section is supposed to contain. The italic looks like the thing it is
   supposed to remove.
2. **It is recoverable.** A reader with the id can fetch the whole `Not checked`
   block; a reader with a deleted italic has nothing. Decision 7's guarantee becomes
   *the limits are always reachable* instead of *the limits were always printed*,
   and the first is the one that survives contact with a word processor.
3. **It composes with §7.3.** The id is a macro in the bundle, so it is neither
   typed nor stale.

**What this note does not propose: removing `--draft`.** A template that can only
emit its slots, behind an explicit flag, with a reachable statement of limits, is a
defensible artefact. §7.4's four categories are this note's rule for its **own**
renderers, not a veto on a sibling's. §12 records the amendment as an ask.

### 7.6 The failure the record does not defend against

The `statistics` object contains every test that ran **in the compiled program**.
It does not contain the eleven earlier versions of the program. A researcher who
ran twelve analyses and kept the one with the smallest p-value produces a record
that is completely true and completely uninformative about the thing that matters.

**The record is not a garden-of-forking-paths defence and must never be advertised
as one.** `statistical-validity.md` §1.4 reaches the same conclusion from the
compiler's side — whether the hypotheses were chosen before the data were seen is
not in the program, ever — and its `Not checked` block says so inside the artefact,
which is the right place for it. Preregistration is the defence and it is a social
mechanism, not a technical one.

What the record does do is make the number of comparisons *within one run* explicit
and correctable, which is the part that is mechanical.

---

## 8. The output format

### 8.1 The primary deliverable is not a document

> **Decision 13. The primary rendering is an *asset bundle* the author's own
> template includes: a macro file of named values, one file per table, one file
> per figure, and a bibliography. Whole-document rendering exists and is
> secondary.**

This is the least impressive decision in the note and probably the most useful
one, and the brief's suspicion about it is correct.

The reason is that journals do not want *a* LaTeX document, they want a document
in `elsarticle.cls` or `revtex4-2` or `iopart`, with their section structure,
their reference style and their front matter. A tool that generates a whole paper
therefore generates a paper that has to be pasted into the journal's class — which
reintroduces the copy-paste boundary at a coarser granularity and loses everything
this note is for.

So the unit of delivery is the individual number:

```latex
% results.tex — generated by sciencec report, do not edit
\newcommand{\RateConstantMean}{\ensuremath{2.47(13)\times10^{-3}\,\mathrm{s^{-1}}}}
\newcommand{\TreatedN}{41}
\newcommand{\ExcludedN}{3}
\newcommand{\ProvenanceId}{pv:9f3a1c04e7b2}
```

and the author writes, in their own manuscript, in their own class:

```latex
The fitted rate constant was \RateConstantMean, from \TreatedN{} specimens
(\ExcludedN{} excluded; \Cref{tab:rates}). Analysis was performed with
Science (provenance \ProvenanceId).
```

Three properties, and the second is the one worth the whole section:

- **The number carries its unit and its rounding**, because the macro was expanded
  from a typed quantity by §6's rule.
- **A stale reference is a build error in the author's own build.** If the
  analysis that defined `\RateConstantMean` is deleted or renamed,
  `pdflatex` fails with *undefined control sequence* and names the line. §1's
  failure (1) — the number recomputed and never updated — becomes impossible in
  the direction that matters, and it becomes impossible in a toolchain the author
  already runs, without them adopting anything.
- **Nothing about the author's document changes.** They keep their class, their
  editor, their co-authors' track changes, and their `\input` line.

`\input{tables/rates.tex}` and `\input{figures/decay.tex}` are the same mechanism
for the larger artefacts. The Typst equivalent is `#import "results.typ"` and a
module of bindings; the Markdown equivalent is a front-matter block and a set of
fenced includes.

### 8.2 The whole-document renderers, and which

For the cases where a whole document *is* the artefact — a thesis chapter, an
internal report, a supplementary methods document, a preprint — three renderers:

| Backend | Why |
|---|---|
| **Typst** | The primary one. Its compiler is a library with a stable CLI, it compiles in milliseconds, its error messages have spans, and it can be driven programmatically without a TeX installation. For a tool that must render a document as part of a build, that matters more than anything LaTeX offers. |
| **LaTeX** | Because journals demand it. The renderer emits a minimal preamble and a body, and the body is designed to survive being moved into somebody else's class. |
| **Markdown** | CommonMark with the tables extension, for a README, a lab wiki, or a pipe into Pandoc or Quarto. |

**On Typst versus LaTeX, honestly, and without inflating Typst's position.** Typst
is the better engineering choice. Its acceptance by publishers is, as of early
2026, negligible: a survey that April found **two** journals accepting native
Typst submission (IJIMAI and JUTI), while NeurIPS, ICML, ACL and the traditional
journals all still require `.tex` source. (Whether arXiv accepts Typst source was
not confirmed and should be checked before anyone writes it down as fact.) The
common practice is to draft in Typst and convert to LaTeX to submit — and
conversion is a copy-paste boundary, which is the thing this note exists to
remove. So: both backends, with Typst first because it is what the *tool* can rely
on being able to run in a build, and LaTeX because it is what the *author* submits.

**Rejected alternative: JATS XML as a backend.** JATS (NISO Z39.96) is what
publishers actually ingest and it would be the most "correct" target. Rejected for
now, for three reasons: no author writes it or reads it; publishers generate it in
their own production pipeline rather than accepting it from submissions; and
Quarto already emits it, so it is not a place where Science would be contributing
anything. The document record (§3) is structured enough that a JATS renderer is a
later addition and not a redesign, and that is the right place to leave it.

**Rejected alternative: `.docx`.** It is what a large share of the audience's
co-authors use and refusing it is a real cost. It is refused anyway, because
producing `.docx` well means a large dependency and producing it badly means an
artefact that looks editable and breaks when edited. The route is Markdown through
Pandoc, which is one command the author runs and which puts the badness in a tool
that is honest about it.

### 8.3 The data rendering, which may be the one people use

> **Decision 13b. The document record also renders as a data package: one CSV or
> Parquet file per table with a `datapackage.json` Table Schema describing its
> columns, their units, their uncertainties and their precisions.**

This is for the author who will never adopt a Science document pipeline and wants
the numbers. It is also what makes Decision 1's corollary real: a Quarto or
R Markdown author points `read_csv` at it and gets a frame whose columns are
documented, and the unit lives in the schema rather than in a column name.

Frictionless Table Schema is the target because it is a small, stable, widely
implemented JSON schema for exactly this — Data Package v2, June 2024, under the
Open Knowledge Foundation — and because a repository deposit of the table plus its
schema is a better supplementary file than a spreadsheet, which given §1's 86–94%
figure is not a high bar to clear.

**Honesty about what this is not.** No journal ingests `datapackage.json` as a
table submission format, and no widely adopted author-facing standard for
"submit the table as data plus schema" exists; Frictionless is domain-agnostic
open-data tooling and JATS is downstream production tooling. This rendering is for
deposit, for supplementary material, and for the author's own next tool. It is not
a submission channel and should not be described as one.

### 8.4 The staleness check, which is `sciencec verify` with one more argument

`reproducibility.md` Decision 13 gives a reviewer three verbs, and the second —
`sciencec verify results.parquet`, which reads a stamped run record and prints
what differs from this machine — is nine tenths of what this note needs. The
document record embeds the run record's `pv:` id and adds per-artefact input
hashes (§7.2), so:

> `sciencec report --check` is `sciencec verify` applied to the document record,
> plus one comparison that record makes possible: **the per-table and per-figure
> input hashes against the files on disk**. A record produced from a file that has
> since changed is `SR0005`, and the message names the artefact, the file and both
> hashes.

The addition over `verify` is small and it is the one that matters for §1's
failure (1): `verify` answers *"would this binary behave differently here"*, and
`--check` answers *"is Figure 2 older than the data it claims to show"*. It is also
the answer to the gap Quarto's own documentation admits in `freeze` — that the
cache notices a changed source file and not a changed input data file (§2.1).

**Not a new verb.** Per `reproducibility.md`'s framing and
`statistical-validity.md`'s, this is a fifth rendering over the same bytes rather
than a fifth artefact, and it should be implemented inside `verify` rather than
beside it.

---

## 9. Where this lives

> **Decision 14. Two pieces, split by whether the work needs the program's values
> or only its artefacts.**
>
> - **`report` is a library** — `Table`, `Column`, `Figure`, `Series`,
>   `Document`, the recorder, and the writer of the document record. It runs
>   inside the user's program because that is where the values are.
> - **`sciencec report` is a subcommand** — it reads a document record and a
>   binary and renders. It does not run the program.

The split is not arbitrary. Rendering needs three things the running program does
not have to hand: the doc-comment metadata table out of the object file (§4.4),
the `.science.provenance` section (§3.1), and a template that a journal or a group
maintains separately from the analysis. And it must work when the program cannot
be run, which is `native-dependencies.md` §5.4's argument verbatim — a reviewer
reconstructing a two-year-old result may not have a node with the right GPU, and a
record that requires executing the program to render is a record they cannot
render.

**Which level.** `report` meets `stdlib-shape-and-packages.md` §5.7's criterion for
Level 3 exactly: it changes when journal conventions change and when typesetters
change, neither of which has anything to do with the language. But that note's §6.4
is blunt that *"Level 3 is a category whose defining property — separate
distribution — is not implemented"*, and this note will not pretend otherwise:
until a package manager exists, `report` ships in the toolchain tarball, in its own
directory, with its own version number, and the documentation says that rather than
implying an ecosystem. It is designed as a Level 3 package and delivered as a
Level 2 module, and the seam is where §6.4 says it is.

### 9.1 What F0 must provide, and must not preclude

This is the part that belongs in F0, and it is short, because almost everything
above is a library over features other notes have already asked for. This note's
service is that it converts four speculative standing asks into asks with a named
consumer.

1. **Const-expression arithmetic in type position.** Without it `Quantity`
   multiplication does not typecheck (`scientific-libraries.md` §12.4) and there
   is no typed table worth the name — every derived quantity in a paper is a
   product or a quotient. This is no longer a speculative ask:
   `const-expression-arithmetic.md` now owns it, with a normal form
   (`k + Σ cᵢ·aᵢ`) that is total because the grammar admits nothing else. This
   note is a **fifth** customer and adds no requirement the other four did not
   already make — the exponent addition of §12.4 is the whole of what a typed
   table needs, and the `Shape` list layer is not.
2. **An uncertainty type.** Also no longer speculative: `uncertainty.md` now owns
   it and has adopted §6 here for its rendering, closing the loop. Its Decision
   3.3 makes an `Uncertain of T` carry a **gradient**, which is what makes the `±`
   in a table the propagated uncertainty rather than a magnitude carried alongside
   — what §4.2.1 and §11.2 between them call the difference between a `±` and a
   lie. §4.2.1 is the one place this note constrains that design, and it
   constrains it by *dropping* something rather than adding.
3. **Doc comments preserved and indexed by field and variant**, not only by
   function and type. `strings-formatting-and-docs.md` §5.3 already puts them in a
   metadata section and already says the F0 obligation is to lex, attach and
   preserve. The addition is that the index key must reach a field, because §4.4's
   footnotes are field documentation.
4. **`Record` derivation exposing field names in declaration order**
   (`data-io.md` §4.3). Already asked for; §4.4 needs the ordering as well as the
   names.
5. **Explicit type arguments at call sites** (`data-io.md` §11.2) for
   `Table.from_frame of Fit(…)`. Fourth customer.
6. **Room for one more format code.** §2.1 of the formatting note fixes a closed
   set; it must be closed in the sense of "checked" and not in the sense of "never
   extended". Adding `u` is the only extension this note wants.
7. **Trait objects over a generic type** — `any TableColumn` — which
   `syntax-revision-2.md` §3.4 already provides for `any Error`.

Nothing on that list is new work created by this note. Items 1, 2 and 5 are work
already required by siblings, and the contribution here is a fourth argument for
building them.

---

## 10. Diagnostics

**This note claims no `SC` code.** It claims **`SR0001`–`SR0099`**, a namespace
outside `SC`, following `package-manager.md`'s precedent and for the same reason:
these failures happen in a tool reading an artefact, after compilation and usually
after the program has run. They are not compiler-phase diagnostics and §9 of the
core spec's ranges do not describe them. The README's own history — five `SC`
collisions in one day — is a second argument for not adding a claimant.

| Code | Meaning |
|---|---|
| `SR0001` | The document record's schema version is newer than this renderer |
| `SR0002` | The named template is missing, or the backend cannot express a block the record contains |
| `SR0003` | A column noun contains a unit (§4.3) |
| `SR0004` | Columns of unequal length in one table (§4.6) |
| `SR0005` | An input file has changed since the record was written (§8.4) |
| `SR0006` | `--strict` with an author-declared precision (§6.3) |
| `SR0007` | A figure exceeded the inline-point threshold and was rasterised — a warning, not an error (§5.2) |
| `SR0008`–`SR0099` | Reserved |

**One amendment, not a claim.** `strings-formatting-and-docs.md`'s `SC0274` gains
the rules of §6.2: `u` applied to a type with no uncertainty, and `.0u` / `.3u`.
That note owns the code; this note adds cases to it, which the README's convention
explicitly permits.

---

## 11. The honest limit

A paper is an argument, not a rendering. This is the section that keeps the rest
of the note from overclaiming, and the ambition is better served by a narrow claim
that holds than by a broad one that does not.

### 11.1 What is guaranteed

Five things, all mechanical, all checkable by a third party from the document
record alone:

1. Every number in the artefact was produced by the program that produced the
   artefact. There is no transcription step to fail.
2. Its unit is the unit its type says, and the header, axis label or macro that
   accompanies it was derived from that same type rather than written beside it.
3. Its rounding follows §6's rule from its own uncertainty, or the document
   records that a human chose the precision.
4. The methods record lists what ran, with its parameters and its inputs'
   hashes.
5. The provenance appendix names what was linked, at what version, verified how.

### 11.2 What is not

- **That the analysis was appropriate.** A correctly rounded t-test on data that
  violates every assumption is correctly rounded. The design has no opinion about
  whether the test was the right one and cannot acquire one.
- **That the data means what the author says.** Garbage in, provenanced garbage
  out. The record proves the file's hash, never the file's truth.
- **That the model is identified, the exclusions were principled, the sample was
  representative, or the figure's axis range is not chosen to flatter.** Every one
  of these is a judgement and every one of them is where papers actually go wrong.
- **That a number the reader derives from the table is correct.** §4.2.1 drops the
  gradient at the table boundary, so a column carries its *marginal* uncertainty
  and nothing about which sources produced it. A reader who subtracts column 3
  from column 2 and combines the error bars in quadrature gets the
  independent-error answer, which is wrong whenever the two are correlated — and
  they usually are, because they usually came from the same instrument. This is
  `uncertainty.md` §7.3's *"serialisation decorrelates silently"*, and it is the
  one limit in this list that the design **creates** rather than merely fails to
  remove. The mitigation is that a derived quantity should be computed in the
  program and given its own column, where its uncertainty is propagated; the
  design cannot stop a reader doing arithmetic on a printed table.
- **That the analysis in the record is the only analysis that was run** (§7.6).
- **That the conclusion follows.** The discussion is the paper. The language
  cannot write it, must not attempt it (§7.4), and the closest it comes is
  guaranteeing that the numbers the argument rests on are the numbers that were
  computed.

### 11.3 The new failure mode this design creates

This is the item most likely to be omitted and it should not be.

**A provenance appendix invites a reader to trust the pipeline instead of reading
the methods.** A paper carrying six pages of hashes, library variants and key
lineages looks rigorous, and looking rigorous is not being rigorous. The
appendix's presence says nothing about whether the analysis was any good, and a
reviewer who reads it and skips the methods has been made worse at their job by a
tool meant to help.

Two mitigations, neither complete. The appendix is supplementary material, not
part of the paper's body (§7.3, §7.4). And the documentation should state §11.2 in the
same breath as §11.1, every time, including in whatever marketing this eventually
acquires — because *"generated by Science"* becoming a quality signal is the
outcome that would do the most damage, and the only defence against it is the
project refusing to encourage it.

---

## 12. What this note asks of the other notes

**Of `strings-formatting-and-docs.md`:**

1. **One new code, `u`**, in §2.1's grammar, with §6.2's rules and `#` as the
   concise form, checked by §2.4's existing mechanism and reported through its
   existing `SC0274`. This is the only grammar change the note asks for anywhere.
   **`uncertainty.md` §7.1 and §7.4 ask for the same character and withdrew its own
   version in favour of this one**, and both notes add the same rider: it should be
   decided **jointly** with `unit-literals.md` §7.3's unit selector, because two
   notes now want to extend one production and deciding them apart is how a grammar
   acquires two incompatible extensions. They compose — `f"{d:u ft}"` is coherent
   under either shape — and the request is only that the decision be made once.
2. **That §5.3's doc-comment metadata index reach fields and choice variants**, not
   only functions and types. §4.4's table footnotes are field documentation and
   there is nowhere else for them to come from. This is the only *new* language
   obligation in the whole note, and it is an index key rather than a feature.
3. A note in §3.3 pointing here. That section says uncertainty *"is a value with an
   uncertainty, not a formatting of a value… it belongs in `Quantity`'s design if
   anywhere"*, which was right; §6 here is where the rendering landed and
   `uncertainty.md` §5 is where the type landed, and a reader of §3.3 should be sent
   to both.

**Of `unit-literals.md`:**

4. Nothing that changes its text. §4.5 supplies evidence bearing on §7.3's open
   request — the column-unit case is better served by a value argument than by a
   format code, because a format code would be a *string* repeated per cell where an
   `UnitOf` constant is one expression per column — and it explicitly leaves the
   prose case to that note and the formatting note. Named here so the interaction is
   visible rather than silent, and so that item 1's joint decision has both customers
   in view.

**Of `uncertainty.md`:**

5. Nothing that changes its text, and one thing accepted from it. Its §7.2 flags
   that Decision 9 step 1 should not group a **non-finite** uncertainty with a zero
   one. **Accepted, and §6.1 is amended accordingly** — zero means exact and NaN
   means the propagation broke, and a bare value in a published table is the one
   rendering that looks fine and is not.
6. One constraint, in §4.2.1: `Column.new` drops the gradient and keeps the
   marginal σ, because a table of ten thousand values would otherwise carry ten
   thousand derivation graphs into a rendering that needs two numbers from each, and
   nothing downstream can use them. The consequence — a reader recombining two
   columns gets the independent-error answer — is that note's §7.3 arriving one hop
   earlier, and §11.2 states it rather than burying it.

**Of `statistical-validity.md`:**

7. **One amendment to Decision 7**, argued in §7.5 and offered rather than
   asserted: that `sciencec methods --draft`'s **first sentence carry the provenance
   id** rather than the limits appearing only in a trailing italic. An author deletes
   a caveat and keeps a citation, and the id makes the `Not checked` block
   *reachable* even from a paragraph whose italic did not survive the paste. Decision
   7 guarantees emission; this makes the guarantee survive a word processor. That
   note owns the decision.
8. Otherwise, adoption without amendment. §7.1 here defers to Decision 6 entirely:
   there is no methods artefact in this note, the `statistics` object is where the
   method lives, and `sciencec methods` is its rendering. This note was drafted
   assuming it would have to design that record and is better for not having to.

**Of `reproducibility.md`:**

9. Nothing that changes its text. §3 here adds a **third** record and justifies it
   against Decision 11's two on a measurable ground — a document is O(data) and
   cannot live under §5.4's 8 KB section cap — and §3.3 extends §5.1's *no floats
   anywhere* rule to the one record that has numbers in it, by storing them as
   strings. §8.4 asks that `--check`'s per-artefact hash comparison be implemented
   **inside** `sciencec verify` rather than beside it, per that note's own framing of
   the reviewer's verbs.

**Of `scientific-libraries.md`:**

10. That **`physics.constants` carry the exact-versus-measured distinction in the
    data**, not merely a zero in an uncertainty field. Since the 2019 SI redefinition
    a growing share of §12.1's catalogue is exact by definition — the Planck
    constant, the elementary charge, the Boltzmann constant, the speed of light,
    Avogadro's number — while others remain measured, and `STANDARD_GRAVITY` and
    `STANDARD_ATMOSPHERE` are conventional definitions that were never measurements.
    §6.1 step 1 renders an exact constant with no `±`, which is correct, but a table
    footnote should be able to say *why*, and only the constants table knows.
11. That the `stats` functions of §7.4 and §7.5 write into the run-scoped recorder
    `statistical-validity.md` Decision 6 defines. That note specifies the record's
    contents; this is only the observation that it has to be decided **before** the
    module is written, because retrofitting it means touching every function in a
    catalogue of several hundred.

**Of `data-io.md`:**

12. That `Record` derivation preserve **declaration order** as well as field names
    (§4.4), and that `Frame.rejected_count()` and per-filter removal counts be
    reachable for §7.2's exclusion accounting. Both are implied by §4.3 of that note;
    this makes them explicit because §4.4 and §7.2 depend on them.
13. That the per-`Frame` input identity survive the pipeline, so that a table and a
    figure built from one binding carry one hash (§5.3). `reproducibility.md`
    Decision 12 already stamps the run record into outputs; this is the same fact
    flowing in the other direction, from the input into the artefact.

**Of `native-dependencies.md`:**

14. Nothing that changes its text. §7.3 here is the consumer its §5.4 was written
    for, and this note is a second piece of evidence that Decision 9's *"readable
    without running the binary"* property is load-bearing: a reviewer rendering a
    two-year-old document has exactly the constraint that decision anticipated.

**Of `const-expression-arithmetic.md`:**

15. Nothing. This note is a fifth customer for exponent addition in type position
    and adds no requirement the other four did not make. Recorded so the customer
    count in that note's §1.1 is right.

**Of `README.md`:**

16. A row in the index and a row in the diagnostic table, recording
    **`SR0001`–`SR0099`** and **no `SC` code**. As with `native-dependencies.md`,
    this note could not add its own row, because its brief forbade editing any file
    but its own. **That row is missing and should be added by whoever lands this.**
17. That the two standing-ask rows for **const-expression arithmetic** and **an
    uncertainty type** be marked as *owned* rather than *asked for*, since
    `const-expression-arithmetic.md` and `uncertainty.md` now exist. This note is a
    customer of both and neither is speculative any more.

---

## 13. Risks

**The whole design is conditional on the uncertainty type, which now exists on
paper and not in a compiler.** §6 is the section that decides whether output is
publishable, and it is entirely conditional on `uncertainty.md`'s `Uncertain of T`.
That note now specifies it, which retires the version of this risk the first draft
carried, and replaces it with a sharper one: `uncertainty.md` §3.5 prices a
gradient per value and §6.3 counts what it does to every signature in `stats`, and
that cost is paid before a single table is rendered. If the uncertainty work is
descoped or deferred, §6.3's author-declared precision is *all* there is,
`--strict` is unimplementable, and this note's central claim degrades from "rounded
by a rule" to "rounded by a number the author typed" — which is what every existing
tool already offers. Not this note's to mitigate, and the largest single dependency
in it.

**Four notes now touch the same artefact and none of them owns all of it.** The
build record is `native-dependencies.md`'s, extended by `reproducibility.md`, with
fields added by `statistical-validity.md`, referenced by this note's document
record, and stamped into outputs by `data-io.md`'s writers. That is five notes and
one byte stream. The design is coherent today because each note deferred to the one
before it; it will stop being coherent the first time two of them add a field in
the same week. The mitigation is that `reproducibility.md` §5.1 owns the schema and
the canonicalisation, and every other note should be adding fields under its rules
rather than negotiating format — which is what all of them have in fact done, and
which should be written down as the rule rather than left as a coincidence.

**Three figure backends is a maintenance surface with no end.** PGFPlots, CeTZ and
SVG have different capabilities, different bugs and different release cadences, and
the pressure to add "just one more mark type" is constant. The mitigation in §5.2 —
a deliberately small description, and an explicit escape to an attached image — is
the right one and it will be unpopular the first time somebody wants a violin plot.

**The asset-bundle deliverable is unglamorous and may be ignored internally.**
§8.1 is the decision most likely to be overridden by somebody who wants a demo in
which a whole paper falls out of a compiler. The demo is more impressive and the
bundle is more useful, and the project should expect to have this argument more
than once.

**Decision 12's line will be argued about, and it has already moved once.** This
note's first draft forbade generated prose outright; §7.5 narrows that to this
note's own renderers, because `statistical-validity.md`'s fixed template turned out
to be defensible and the blanket rule was not. That is the right correction and it
is also the first step of exactly the erosion the decision exists to resist. The
next request will be for *one more slot* in the template, and the one after that
for a sentence that explains a choice. The defence is that §7.4's four categories
are written down as categories rather than as a list of banned phrasings, and that
the `Not checked` block is compiler-emitted; the honest statement is that a line
which has moved once can move again.

**`report` is a Level 3 package in a world with no Level 3.**
`stdlib-shape-and-packages.md` §6.4 already states this and the consequence lands
here: a library whose entire reason to be separate is that journal conventions
change on their own schedule will ship on the compiler's schedule instead. Every
template fix will require a toolchain release. This is a known, stated, accepted
cost and it expires when the package manager does.

**Adoption is the binding constraint and nothing in the design affects it.** §1's
error class is real and measurable, and the population committing it is
overwhelmingly using spreadsheets, R and Python. A guarantee available only to
people who rewrote their analysis in a new language is a guarantee with a very
small denominator. §8.3's data-package rendering and Decision 1's corollary — be a
good input to Quarto — are the only two things in the note that address this, and
they address it partially.

---

## 14. Open questions

1. ~~**Does `Uncertain of T` propagate, and how?**~~ **Answered while this note was
   drafting.** `uncertainty.md` Decision 3.3 carries a gradient, makes correlations
   exact, confines the feature to scalars, and prices it. §4.2.1 is this note's only
   interaction with that answer. Left in the list, struck, because the question was
   live when §6 was written and a reader should see that it closed rather than
   wonder whether it was overlooked.
2. ~~**Should the document record be signed?**~~ **Answered.**
   `reproducibility.md` §5.4 decides that the record is **not** signed in F0–F1,
   because signing needs key distribution, which needs the registry, which is F2 —
   and that claiming more would be security theatre. The document record inherits
   that decision along with the rest of §5.1's format rules.
3. **Should `sciencec report` be able to re-render from the binary alone**,
   without the record, by re-running the program? It would make the artefact
   self-regenerating. It also makes the tool a program runner, which
   `native-dependencies.md` §5.4 spent a decision arguing against for related
   reasons.
4. **What does a multi-panel figure's type look like?** §5.1 composes panels with
   `.panel(fig)`, which erases the axis types at the composition boundary. A
   shared-axis panel grid — the common case — wants to keep them, and that is a
   type-level list again.
5. **Does the analysis recorder need a scope mechanism?** §7.1 assumes a
   run-scoped recorder. A program that fits a hundred models in a loop produces a
   hundred records, and the useful artefact is a summary of them. Where the
   aggregation happens — the library, the renderer, or the author — is not decided.

---

## 15. Decisions, with what each rejected

| # | Decision | Rejected alternative | Why | Cost |
|---|---|---|---|---|
| 1 | The contribution is the typed quantity, not the weave | A Science literate mode; or claiming nothing | Weaving is solved by Quarto and competing loses; but §2.2's four points are real and documented | Concedes the most visible feature to an incumbent |
| 2 | A third record — the document record — is the output, not a record about it; renderers consume it | Render directly to LaTeX; or fold it into `reproducibility.md`'s run record | Multiple backends need one source; delivery at the granularity of one number; and a document is O(data), which does not fit an 8 KB object-file section | A third artefact and a third schema to version |
| 2b | A cell holds value, uncertainty and rendered text, all as strings; no record holds a float | Store floats and round at render | `reproducibility.md` §5.1's rule, extended to the one record with numbers; two renderers cannot disagree; the rounding becomes third-party checkable | Two extra strings per cell, and the discipline of never writing a float |
| 2c | `Column.new` drops `Uncertain`'s gradient and keeps the marginal σ | Carry the gradient into the table | Ten thousand dead derivation graphs in the largest artefact in the design | A reader recombining two columns gets the independent-error answer (§11.2) |
| 3 | A header's unit is derived from the type and never written | A checked header string | There is nothing to check when there is only one source | The author loses header layout control to the template |
| 4 | `Frame of R` becomes a table through `R` — field names are headers, `##` comments are footnotes | Author-supplied headers and notes | One place to change; a header cannot name a missing column | Needs the doc index to reach fields |
| 5 | The display unit is a value argument; the header symbol derives from it | A format-spec unit selector per cell | A column shares one unit; a value argument cannot disagree with itself | One more builder method |
| 6 | Cross-references are bindings | A string label namespace | `\ref{tab:rats}` renders as `??`; an undefined binding is `SC0201` | Forward references need `doc.place` |
| 7 | `Figure of (X, Y)`; a second unit on one axis is a type error | Axes labelled by string | Ordinary unification; no new machinery; impossible in Python at any cost | Twin axes need a second type; three need panels |
| 8 | Figures render to the typesetter's own plotting source, data inline | Ship a rasteriser | Right fonts, vector, data extractable, no rasteriser to maintain | Three backends that are not pixel-identical |
| 9 | The PDG digit rule, matched decimal places, fixed decimal count | The GUM's "one or two digits" | The GUM's rule is not decidable by a compiler | Half-to-even must be recorded to be verifiable |
| 10 | One new format code, `u`, with `#` for the concise form | A second code for the scientific form; a named function | The common exponent is a column property; a rendering nobody reaches by default is not reached | One character of grammar in a closed set |
| 11 | No uncertainty means an author-declared precision, marked as such; `--strict` refuses it | Silently pick a precision | §0.2's sentence must stay true | Most real columns will be marked |
| 12 | *This note's* renderers emit no running prose; `statistical-validity.md`'s fixed template is that note's call | A blanket ban on generated prose anywhere | Plagiarism and honesty hazards are real for free-form generation; a template with no slot for a rationale is a different artefact and the blanket rule was wrong | The line has moved once and can move again (§13) |
| 13 | The primary rendering is an asset bundle for the author's own template | A whole generated paper | Journals want *their* class; a whole paper reintroduces the paste boundary | Unglamorous; no demo |
| 13b | The record also renders as a Frictionless data package | Only documents | It is what an R or Python author will actually consume | One more renderer |
| 14 | `report` is a library; `sciencec report` is the renderer | One or the other | Values need the program; metadata, provenance and templates need the artefact | Two things to version |
| 15 | No `SC` code; `SR0001`–`SR0099` | A block in the `SC` codegen range | Render-time failures are not compiler phases, and `SC` has produced five collisions already | A second namespace for users to learn |
