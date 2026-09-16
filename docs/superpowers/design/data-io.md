# Science — Design: data input and output

Date: 2026-09-16
Status: draft
Amends: `docs/superpowers/specs/2026-09-16-science-f0-core-design.md` §8, whose
library method sets are declared closed. This note is the spec change that opens
them, and it says exactly how far.

Scope: the surface by which a Science program gets data off a disk and puts it
back. Not: arithmetic on tables (group-by, join, pivot), not tensors, not
networks. Where this note stops, §11 says who it stops in favour of.

---

## 1. The sentence this note is trying to earn

*A pipeline is mostly glue.* Loading and saving is the first and last third of
it, and today Science cannot read a CSV. Everything below is chosen for how much
of a working scientist's day it unblocks per unit of compiler and library work,
and nothing below adds a type-system feature that a data file does not force.

One thing is worth saying before the detail, because it is the reason this is
worth doing in Science rather than in Python. `frame.columns().temperature` is
an `Array of F32` — the actual contiguous buffer, not a copy, not a view object
with a `.values` on it. Handing it to a model is a field access. The first and
last third of the pipeline touch the middle third with no marshalling at all.
That is the whole argument, and the rest of this note is what it costs.

---

## 2. What the core library gains

The closed list of §8 (`print`, `write`, `panic`, `read_file`, `write_file`)
grows by exactly this much in the core:

- `Path`, and the filesystem free functions of §7.
- `read_bytes`, `write_bytes`, `read_lines`, `write_lines`.
- `File`, `TempDir`, `TempFile`.
- The error types `IoError`, `DataError`, `RowError`.

Everything else — `Frame`, `Rows`, `Chunks`, and every format — lives in modules
`data`, `data.csv`, `data.json`, `data.npy`, `data.safetensors`, `data.arrow`,
`data.parquet`, shipped with the toolchain but not part of the core's closed
list. This split matters for §9: it is what lets the base toolchain build with
no C++ compiler present.

---

## 3. Which formats are in, and which are not

Ranked by audience unblocked per unit of work. "Audience" means people who
cannot currently do their job in Science at all.

| Rank | Format | Unblocks | Work | Route | Verdict |
|---|---|---|---|---|---|
| 1 | Bytes and text lines | Everyone, as the floor | Days | Science | **In** |
| 2 | CSV and delimited text | Instruments, surveys, experiment logs, every teaching dataset, every "export" button ever built | ~3 weeks | Science | **In** |
| 3 | JSON and JSON Lines | Configs, run metadata, model outputs, API payloads, per-epoch logs | ~2 weeks | Science | **In** |
| 4 | npy / npz | The universal "here is the array NumPy made" handoff | ~4 days | Science (+ vendored `miniz` for npz) | **In** |
| 5 | safetensors | Model weights; the format PyTorch and HF actually ship in | ~3 days, on top of JSON | Science | **In** |
| 6 | Parquet | Every dataset above a gigabyte that anyone still curates; the reason people leave CSV | ~2 weeks of binding, plus a C++ dependency | Bound (Arrow C++) | **In**, behind a build feature |
| 7 | Arrow IPC / Feather | Free once Parquet is bound; the zero-copy handoff to pandas, polars, DuckDB | ~2 days on top of Parquet | Bound (same library) | **In** |
| — | HDF5 | Physics, astronomy, neuroscience, MATLAB exports | Months | Would be bound | **Out** |
| — | NetCDF | Climate, oceanography, atmospheric science | Months, and needs labelled dimensions first | — | **Out** |
| — | Images (PNG/JPEG/TIFF) | Vision work, microscopy | ~1 week for the cheap version | Would bind `stb_image` | **Out** |
| — | Excel `.xlsx` | Biology, chemistry, anything with a collaborator | ~2 weeks | Would bind | **Out** |
| — | Zarr | Cloud-native chunked array storage | Months | — | **Out** |
| — | MATLAB `.mat` v7.3 | Is HDF5 wearing a hat | — | — | **Out** |

### Why the four biggest exclusions are excluded

**HDF5 is out, and it is the most painful call in this note.** It is the native
format of exactly the audience Science is named for. It is also a
600-thousand-line C library with a build system, default-thread-unsafe global
state, a filter plugin mechanism that loads shared objects at runtime, and an
object model — hierarchical groups, attributes on everything, unlimited
dimensions, chunk caches — that has no analogue anywhere else in this note. A
faithful binding is not a binding, it is a second library. Shipping a
one-tenth-faithful binding is worse than shipping none, because a scientist who
opens their group's file and gets `UnsupportedFeature` concludes the language is
a toy. Deferred to a second version, bound properly or not at all. In the
meantime the honest answer to an HDF5 user is: `extern` FFI, or `h5dump` to
something else.

**NetCDF is out because it is at the wrong layer.** NetCDF-4 sits on HDF5, so it
inherits that cost, and on top of it adds CF conventions and named dimensions.
Reading a NetCDF file into an unlabelled `Frame` throws away the only part that
made it NetCDF. Labelled dimensions are a *tensor* design — they belong with F1's
typed shapes, where a dimension name can be a const generic argument and
`temperature` can carry the static shape `(time, lat, lon)`. Building it here,
before shapes exist, would mean building it twice.

**Images are out because they are a domain, not a format.** The decoder is
cheap; what is not cheap is that the moment a program can open a PNG, it needs
colour spaces, bit depths, EXIF orientation, and resizing, and none of that is
data IO. When it arrives it will be one vendored single-header C decoder
producing an `Array of U8` with a shape, and it will be its own note.

**Excel is out, and this is the most-requested thing being refused.** `.xlsx` is
a zip of XML with a date epoch bug, merged cells, formulas whose cached values
may disagree with the formula, and a 1900-versus-1904 calendar switch. Every
value read from it is a value someone could have typed over. The answer is:
export to CSV. That answer is unpopular and it is correct, and stating it plainly
now is better than a half-binding people build pipelines on.

---

## 4. The core abstraction

This is the consequential decision, so it gets its reasoning at length.

### 4.1 Three types, not one

There is no single "dataset" type. There are three, and which one you have is
visible in the signature:

```science
Rows of R        # a lazy stream of records; bounded memory; Item is (R, RowError?)
Chunks of R      # a lazy stream of batches;  bounded memory; Item is (Frame of R, DataError?)
Frame of R       # a columnar table, fully in memory, owning its columns
```

`R` is a user-declared record `type`. It *is* the schema. There is no separate
schema object the user writes.

A `Frame of R` is **columnar** — a struct of arrays, not an array of structs.
Each column is a contiguous `Array of T`. Row-major was rejected: a model wants
a column, Parquet and Arrow are columnar, and a column that already *is* an
`Array of F32` costs nothing to hand onward, whereas a column extracted from an
array of structs is a gather and a copy on every use.

`Rows` is row-at-a-time because that is what a text parser produces and what a
predicate consumes. The transposition from rows to columns happens at exactly
one call, `.collect()`, and that call is the point where the program commits to
holding the data in memory. Making that point a visible method with a visible
name is the entire ergonomic payoff of splitting the types.

### 4.2 What each step of a pipeline produces

The pipeline in the brief — read a CSV, filter it, feed an array to a model — is
this, with the type of every intermediate:

| Step | Expression | Type |
|---|---|---|
| Name the file | `Path.from("m.csv")` | `Path` |
| Configure | `CsvOptions.new().delimiter(',')` | `CsvOptions` |
| Open — reads the header, checks the schema, parses no rows | `read_csv of Measurement(path, options)` | `(CsvReader of Measurement, DataError?)` |
| Stream | `.rows()` | `Rows of Measurement` |
| Filter — still lazy, still bounded memory | `.keep(each.temperature >= 0.0f32)` | `Rows of Measurement` |
| Materialize | `.collect()` | `(Frame of Measurement, DataError?)` |
| Take a column | `.columns().temperature` | `borrowed Array of F32` |
| Feed a model (F1) | `Tensor.viewing(column)` | `Tensor of (F32, (n,))`, zero copy |

Two things are worth noticing. First, the filter is on `Rows`, not on `Frame`:
you filter before you allocate, which is the only order that works on a file
bigger than memory. Second, the last two rows involve no copy and no conversion
function — the column *is* the buffer, laid out to satisfy both Arrow's C Data
Interface and the DLPack layout §8 of the core spec already committed to.

### 4.3 How a record type becomes a schema

A record type needs two things the compiler must supply: a runtime description
of its fields, and a companion struct-of-arrays type for `Frame` to hold.

```science
type Measurement:
    station: String
    timestamp: I64
    temperature: F32
    humidity: F32?

Measurement implements Record
```

`Record` is a **compiler-derived interface**, written on one line like a marker
(§4.4 of the core spec), and the compiler synthesizes:

```science
interface Record:
    type Columns
    function schema() -> Schema
```

For `Measurement`, `Measurement.Columns` is a type whose fields are
`station: Array of String`, `timestamp: Array of I64`,
`temperature: Array of F32`, and `humidity: MaybeColumn of F32`; `schema()`
returns the field names and types as data.

`Frame` then needs no new expression syntax at all — column access is ordinary
field access on an ordinary type:

```science
Frame of R has where R: Record:
    function len(self) -> U64
    function columns(self) -> borrowed R.Columns
    function columns_mutable(mutable self) -> mutable borrowed R.Columns
    function row(self, index: U64) -> R
    function slice(self, range: Range of U64) -> Frame of R
    function append(mutable self, other: Frame of R) -> ((), DataError?)
    function rejected(self) -> borrowed Array of RowError
    function rejected_count(self) -> U64
```

**This derivation is the one piece of new language machinery this note asks
for, and it is a real cost.** §12 of the core spec says macros are reserved and
not implemented, and a derive mechanism is a macro with the scary parts removed.
The justification is that the two alternatives are both worse:

- *Make the user write the schema by hand.* Every load then needs a `Schema`
  literal plus a per-field constructor, which is forty lines of boilerplate to
  read a four-column CSV. That boilerplate is precisely what sends people back
  to Python.
- *Make `Frame` dynamic* — `Map of (String, Column)`, columns tagged at runtime,
  access by string. This is what pandas does, and a `df["temprature"]` typo
  failing at runtime is exactly the class of mistake Science exists to catch
  before the run starts (§1 of the core spec). Adopting it in the data layer
  would concede the language's premise in the one place users spend most of
  their time.

The derivation is strictly smaller than macros: it maps a record type to a fixed
shape, takes no user-written code, and has no expansion the user can see or
control. It should be specified as that, not as the first step toward macros.

### 4.4 The escape hatch, and its boundary

Exploration is real: sometimes you do not know the columns. `Frame of Dynamic`
exists for that.

```science
let frame, err be read_csv of Dynamic(path, CsvOptions.new())
if err?:
    return (Frame.empty(), err)

let column, err be frame.column("temperature")   # checked at runtime, not before it
if err?:
    return (Frame.empty(), err)

let typed, err be frame.cast of Measurement()    # one check, then static forever
if err?:
    return (Frame.empty(), err)
```

`Dynamic` is a `Record` whose `Columns` is a name-to-column map. Every access
hands back a `DataError?` beside its value. `cast of R()` validates once and
hands back a `Frame of R`, after which everything is static again.

The boundary is deliberate: `Dynamic` exists for the first five minutes with a
new file, and for F1's notebook tier, where a cell genuinely does not know its
input. It is not the default, it is not what the worked example uses, and no
library function returns it unless you ask for it by name.

---

## 5. Schema and typing

### 5.1 Declared, checked, and inferred — and which two Science does

**Declared.** The record type is the schema. This is the only way a program
expresses a schema.

**Checked, at open, before the first row.** `read_csv` reads the header line;
`read_parquet` reads the footer. Either compares what is on disk to `R.schema()`
and returns `SchemaMismatch` as the error of the *open* call. Nothing is parsed
until the schema agrees. This is the single most important ergonomic property of the
whole design: a schema error surfaces in the first millisecond, not at row four
million of a nine-hour job.

The mismatch error names every discrepancy at once — missing columns, type
conflicts, and (if configured to care) unexpected extras — rather than the first
one. It is rendered the way a compiler diagnostic is rendered, with the header
line quoted and the offending column underlined. A data error and a type error
are the same kind of event and should look the same.

Column matching is **by name, order-independent**, when there is a header.
Missing columns are always an error. Extra columns are ignored by default
(`extra_columns(Extra.Ignore)`), because a real CSV has a `notes` column nobody
asked for; `Extra.Error` is one call away for pipelines that want to be told when
their input changes shape. With `header(false)`, matching is by position.

**Inferred — but not at runtime.** Schema inference is a *tool*, not a library
function:

```
sciencec infer-schema measurements.csv
```

which prints a `type` declaration and an `implements Record` line, and you paste
them into your program. Two reasons, both about this audience:

1. Runtime inference makes a program's types depend on its data, which a
   statically typed language cannot do except through the dynamic path of §4.4.
2. Inference that samples the first N rows gives a different answer for a
   different N. A pipeline whose column is `I64` on Monday and `F64` on Tuesday
   because row 1001 had a decimal point is a reproducibility bug — and §5.1 of
   the core spec already makes exactly this call about reduction order.

The cost is one extra command before your first run. The benefit is that the
schema lives in the source, under version control, reviewable in a diff, and
visible in the error when the upstream data changes. That is what a publishing
audience actually needs from inference.

### 5.2 Text to type

For CSV and delimited text:

| Declared type | Accepted | Notes |
|---|---|---|
| `I8`–`I64`, `U8`–`U64` | optional sign, decimal digits | out of range is a row error, never a wrap |
| `F32`, `F64` | decimal, exponent, `inf`, `-inf`, `nan` | see below |
| `Bool` | `true`, `false` | extra spellings only via `bool_values(...)` |
| `Char` | exactly one Unicode scalar | |
| `String` | the field, quotes removed, `""` unescaped | |
| `T?` | a null token gives `null`, else `T` | this is how "missing" is spelled |
| anything else | — | compile error, at the `implements Record` line |

**Float parsing is Science's own, not the platform's `strtod`.** Different C
libraries round differently at the last bit. A number that differs between a
laptop and a cluster because of its libc is a published-results bug, and a
correctly-rounded parser is a few hundred lines written once.

**Nested and repeated columns are not readable in v1.** A CSV cell is a scalar,
and a Parquet file with a `LIST` or `STRUCT` column fails at open with
`UnsupportedSchema`. Flat schemas only. This is a real limitation and it blocks
a real minority of Parquet files.

**Nullability is `T?`, and there is no other absence.** §5.5 of the core spec
says absence is `T?`, whose one absent value is the literal `null` (§4.2); the
data layer inherits that rather than inventing a NULL of its own. Arrow and
Parquet validity bitmaps map to and from `T?` at the column level. A
non-nullable column that meets an empty field produces a row error — the file
disagreed with what you declared, and that is worth hearing.

That spelling now collides with the file's own vocabulary, and the two must be
kept apart. A *null token* is a string in the input — `""`, `"NA"`, `"-999"` —
named in `null_value(...)`, and it is a property of the file. `null` is the
Science literal, and it is a value in the program. A null token in an `F32?`
column becomes `null`; the same token in an `F32` column becomes a row error.
Nothing on disk is ever the literal itself.

**Text encoding is UTF-8, plus Latin-1 on request.** Everything else fails with
`NotUnicode` and a byte offset. Latin-1 is a 256-entry table and covers most of
the legacy instrument-export pain for essentially no work; Shift-JIS and the rest
do not, and are refused rather than half-supported. Line endings `\n`
and `\r\n` are both terminators; a lone `\r` is not.

**Dates and times are the acknowledged hole.** There is no `Instant` type in the
language today, so in v1 a timestamp column is declared `I64` (epoch units of
your choosing) or `String`. This is the biggest single gap in the CSV story,
because a large fraction of real scientific CSVs have a date column. What this
note needs from the core-library note is the minimum: `Instant` as I64
nanoseconds since the Unix epoch in UTC, and `Duration` as I64 nanoseconds — no
calendars, no time zones, no formatting. That minimum is forced anyway, because
Parquet and Arrow have native timestamp types with units and the mapping has to
land somewhere. The moment it exists,
`CsvOptions.timestamp_format(...)` and the Parquet mapping follow in days.

### 5.3 Bad rows

The policy is a reader setting, and it is not optional to think about:

```science
choice BadRow:
    Stop
    Skip(max: U64)
```

`Stop` is the default: the stream yields the row error and ends, and
`.collect()` returns it as its error.

`Skip` **requires a cap.** `BadRow.Skip(500)` means "up to five hundred bad rows
is what I expect from this instrument; more than that and my assumptions about
this file are wrong, so stop and tell me." Exceeding the cap is
`TooManyBadRows`, carrying the first rejects as evidence.

This is the decision in this note I am most confident about. Every dynamic data
library offers "skip bad rows" as an unbounded flag, and every one of them has
produced a published number computed from 12% of the intended data without anyone
noticing. A cap costs the user one integer and converts a silent wrong answer
into a loud stop.

A third policy — "replace a bad value with `null`" — was considered and
**rejected**. Coercing an unparseable value into missingness is exactly how a
unit error becomes a number in a paper. Note which `null` that is: the proposal
was to substitute the *literal* for a value the file got wrong, which is not the
same thing as honouring a null *token* the file deliberately wrote. The
legitimate version of that need is served properly and truthfully: declare the
field `F32?` and list the file's null tokens in `null_value(...)`.

Rejects that fall under the cap are kept, not discarded: `frame.rejected()` is an
`Array of RowError`, bounded by a configurable retention limit, and
`frame.rejected_count()` is the true count.

### 5.4 The error types

```science
choice DataError:
    Io(IoError)
    SchemaMismatch(SchemaReport)
    UnsupportedSchema(String)
    TooManyBadRows(RowError, U64)
    Row(RowError)
    Truncated(Path, U64)
    NotUnicode(Path, U64)
    Backend(String)

type RowError:
    file: Path
    line: U64
    byte_offset: U64
    column: String?
    expected: String
    found: String            # truncated to 64 characters
    code: String             # for example "DATA0007"
```

`RowError` carries a code so errors are greppable and documentable. It
deliberately does *not* use the `SC0xxx` range of §9: those are compiler
diagnostics produced before a run, and conflating them with runtime data errors
would make the ranges mean two different things.

`RowError implements Display`, and its rendering is the compiler's rendering —
file, line, column, the offending text quoted, what was expected. The standard §9
sets for diagnostics is the standard this layer is held to.

---

## 6. Streaming and size

### 6.1 Laziness is in the type

`Rows` and `Chunks` are lazy by construction. `Frame` is strict by construction.
There is no lazy `Frame` and no `.lazy()` method. You know which you have by
reading the signature, and `.collect()` is the only transition.

The alternative — a lazy frame with a deferred expression graph and a query
optimizer, the Polars and Spark model — was rejected. It is a second compiler
living inside the library: an expression IR, predicate pushdown, projection
pruning, join reordering, and a cost model. The owner's framing is that the value
here is glue, not machinery, and the machinery would be roughly the size of the
rest of this note squared.

**The cost is stated plainly:** there is no automatic predicate pushdown. If you
write `.keep(each.temperature > 40.0f32)` over a Parquet file, every row
group is read and decoded and then most rows are thrown away.

**The mitigation is that the pushdowns that actually matter are explicit
arguments to the reader**, not inferred from a predicate:

```science
let reader, err be read_parquet of Measurement(path, ParquetOptions.new()
    .columns_used("station", "temperature")                    # other columns never decoded
    .row_groups_where("temperature", Bound.above(40.0f32)))    # skip by row-group statistics
if err?:
    return (Frame.empty(), err)
```

Column projection is the pushdown worth most of the benefit and it is trivially
expressible. Row-group skipping by min/max statistics on one column is the next
slice. Neither needs an optimizer. What is lost is the long tail, and the long
tail is where the optimizer's complexity lives.

### 6.2 Larger than memory

Three shapes, covering three real situations:

```science
CsvReader of R has:
    function rows(self) -> Rows of R
    function chunks(self, size: U64) -> Chunks of R
    function collect(self) -> (Frame of R, DataError?)
```

- **`Rows of R`** — one record at a time, one IO buffer, constant memory
  regardless of file size. This handles any single-pass job: filter and rewrite,
  fold to a scalar, count, transcode. Its combinators are error-transparent:
  `keep`, `discard`, `map_row` and `fold` apply to the record and forward a row
  error untouched. `.iterate()` escapes to the ordinary `Iterate` chain over
  `(R, RowError?)` when you want the general tools. This is why `Rows` is its own
  type with its own closed method set rather than a plain iterator:
  `docs.discard(each.is_empty())` reads correctly only if `each` is the record,
  not a pair wrapping it.
- **`Chunks of R`** — a `Frame of R` at a time, default 65 536 rows. This is the
  shape that matters for ML, because a training loop wants batches, and each
  chunk's `.columns()` is already the contiguous array the model eats. Memory is
  bounded by the chunk, not the file.
- **`Frame of R`** — everything, when everything fits.

**Memory mapping** is available for the fixed-width formats where no decoding
happens: `map_npy` and `map_arrow` return a `Frame` whose columns *borrow* the
mapping rather than owning heap copies. The region engine (§6.2 of the core spec)
then guarantees that no column outlives its mapping, which is one of the places
ownership pays for itself in this layer rather than costing. The unsoundness is
stated rather than papered over: if another process rewrites the file while it is
mapped, the contents change underneath. Science documents this as a precondition
and takes no lock, the same choice every other language makes.

**What is not provided, explicitly:** no spill-to-disk, no external sort, no
out-of-core group-by or join. Any operation needing more than one pass over data
larger than memory is out. That is a database, and the correct advice is to use
one and read its Parquet output.

---

## 7. Paths and the filesystem

`Path` is its own type, not a `String`.

```science
type Path            # owned, UTF-8, normalized separators
Path implements Display
Path implements Clone
Path implements Eq
Path implements Ord
```

Three reasons it is not a `String`: separator handling is platform business and
belongs behind a method; a function taking `borrowed Path` cannot be handed a
column name or a URL by accident; and F2 will want object-store paths, and a type
gives those somewhere to live without breaking every signature.

**Paths are UTF-8, and a non-Unicode path is a `NotUnicode` error.** On Windows a
path is UTF-16 and may contain unpaired surrogates; on Unix it is bytes and may
not be UTF-8. Rust's answer is a second string type (`OsString`) that infects
every signature it touches. Go's and Python's answer is to declare UTF-8 and
cope. Science takes the second: the cost is that a vanishingly small number of
real files become unreachable and need `extern` FFI; the benefit is that the
language keeps exactly one string type, which is worth far more.

```science
Path has:
    function from(text: borrowed String) -> Path
    function join(self, part: borrowed String) -> Path
    function parent(self) -> Path?
    function name(self) -> String?
    function stem(self) -> String?
    function extension(self) -> String?
    function with_extension(self, extension: borrowed String) -> Path
    function is_absolute(self) -> Bool
    function absolute(self) -> (Path, IoError?)
    function exists(self) -> Bool
    function is_directory(self) -> Bool
    function size(self) -> (U64, IoError?)
    function text(self) -> borrowed String
```

Free functions:

```science
function current_directory() -> (Path, IoError?)
function create_directory(path: borrowed Path) -> ((), IoError?)
function remove_file(path: borrowed Path) -> ((), IoError?)
function remove_directory(path: borrowed Path) -> ((), IoError?)
function rename(from: borrowed Path, to: borrowed Path) -> ((), IoError?)
function list_directory(path: borrowed Path) -> (Array of Path, IoError?)
function glob(pattern: borrowed String) -> (Array of Path, GlobError?)
```

`create_directory` creates parents; a "one level only" variant is not worth a
second function. `remove_directory` refuses a non-empty directory, and there is
deliberately **no recursive delete** in the core — a bug in a path expression
should not be able to erase a results directory.

**`glob` returns results sorted in byte order, always.** This is not a
convenience. A program whose input file order depends on the host filesystem's
directory iteration produces different output on a laptop and on a cluster, and
for an audience that publishes, deterministic ordering is a correctness property,
not a nicety. Supported syntax is `*`, `?`, `[a-z]`, and `**` for recursive
descent. Brace expansion is not supported, and `GlobError` says so rather
than silently treating the braces as literal characters.

### Temporary files

```science
type TempDir         # Drop removes the directory and everything in it
type TempFile        # Drop removes the file

function temporary_directory() -> (TempDir, IoError?)
function temporary_file(suffix: borrowed String) -> (TempFile, IoError?)

TempDir has:
    function path(self) -> borrowed Path
    function keep(self) -> Path              # consumes self, disarming the Drop
```

This is the small place where ownership visibly beats every dynamic language:
cleanup happens at a program point the compiler names, and `keep()` *consumes*
the value, so a program cannot both retain the directory and have it deleted. The
`try`/`finally` a Python script forgets is not expressible as a mistake.

### Files

```science
File has:
    function open(path: borrowed Path) -> (File, IoError?)
    function create(path: borrowed Path) -> (File, IoError?)
    function append(path: borrowed Path) -> (File, IoError?)
    function read(mutable self, into: mutable borrowed Array of U8) -> (U64, IoError?)
    function write(mutable self, bytes: borrowed Array of U8) -> (U64, IoError?)
    function flush(mutable self) -> ((), IoError?)
    function close(self) -> ((), IoError?)
```

`close` **consumes** the file and returns an `IoError?`, and you are expected to
call it on anything you wrote. `Drop` closes too, as a safety net, but `Drop`
cannot return an error, and a flush that failed at close is a real data-loss bug
that must not be swallowed. The library says this in one line rather than
pretending RAII solves it.

---

## 8. Writing

```science
function write_csv of R(frame: borrowed Frame of R, path: borrowed Path, options: CsvOptions) -> ((), DataError?)
function write_json_lines of R(frame: borrowed Frame of R, path: borrowed Path) -> ((), DataError?)
function write_npy(column: borrowed Array of F32, path: borrowed Path) -> ((), DataError?)
function write_safetensors(tensors: borrowed Map of (String, Array of F32), path: borrowed Path) -> ((), DataError?)
function write_parquet of R(frame: borrowed Frame of R, path: borrowed Path, options: ParquetOptions) -> ((), DataError?)
```

**Every whole-file write is atomic.** The bytes go to a sibling temporary in the
same directory, are flushed, and the file is renamed into place. A crash leaves
either the old file or the new one, never a half-written Parquet footer. The cost
is transient double space and a requirement that the destination directory be
writable; the benefit is that a crash at hour nine of a run does not also destroy
the input needed for the rerun.

**Streaming writers** exist for output bigger than memory:

```science
ParquetWriter of R has:
    function create(path: borrowed Path, options: ParquetOptions) -> (ParquetWriter of R, DataError?)
    function write_chunk(mutable self, frame: borrowed Frame of R) -> ((), DataError?)
    function close(self) -> ((), DataError?)
```

`close` consumes the writer, for the same reason `File.close` does — and with
Parquet it is worse, because the footer is written at close and a file without
one is unreadable.

**Compression.** On read, `.gz` and `.zst` on a CSV or JSONL are decompressed
transparently by extension, because scientific text data ships compressed
constantly and refusing it is pure friction. On write, compression is explicit and
never implied by the extension. The asymmetry is deliberate: reading is about
accepting what you were handed, writing is about doing exactly what you said.

**Append** is real for CSV and JSON Lines and impossible for Parquet, npy and
safetensors, whose headers or footers describe the whole file. `write_parquet` to
an existing path replaces it; there is no `append_parquet`, and the substitute is
a directory of part files, which every Parquet reader already understands.

---

## 9. Where the implementation comes from, and what it does to the build

| Format | Route | Dependency | Build impact |
|---|---|---|---|
| bytes, lines, `Path`, `File` | Science, over platform syscalls in `science-rt` | none | none |
| CSV and delimited | Science | none | none |
| JSON / JSONL | Science | none | none |
| npy | Science | none | none |
| npz | Science | vendored `miniz` (one public-domain C file) | one C file, no build system |
| safetensors | Science, on the JSON reader | none | none |
| gzip streams | vendored `miniz` | as above | as above |
| zstd streams | comes with Arrow | see below | see below |
| Parquet | **Bound** | Apache Arrow C++ (`libarrow`, `libparquet`) | a C++ toolchain — see below |
| Arrow IPC / Feather | **Bound**, same library | same | same |

### The Parquet binding

The binding is not to Arrow's C++ API. It is to the **Arrow C Data Interface**
and **C Stream Interface** — three plain C structs (`ArrowSchema`, `ArrowArray`,
`ArrowArrayStream`) with a release callback, a published stable ABI, and no C++
symbols crossing the boundary. This is the same bet §8 of the core spec already
made with DLPack, and for the same reason: match a small stable ABI and
interchange becomes a cast; miss it and every exchange is a copy forever.

Concretely, `science-rt` gains a thin C shim that opens a Parquet file and hands
back an `ArrowArrayStream`; each chunk that comes out is an `ArrowArray` whose
buffers are already exactly the layout a `Frame` column wants — contiguous, one
dtype, with a validity bitmap for the nullable ones. A `Frame` can therefore
*adopt* an Arrow chunk without copying, holding the release callback and calling
it on `Drop`. Ownership makes that safe rather than a leak waiting to happen.

The consequence for nullable columns: an `F32?` column carries Arrow's validity
bitmap, and DLPack has no concept of validity. So a nullable column **cannot**
become a tensor directly — it needs `.fill(value)` or `.drop_missing()` first, and
that is a compile-time distinction rather than a runtime surprise. This is exactly
the NaN-versus-NA confusion that costs pandas users days, resolved by the type.

### What the C++ dependency costs, and the three ways out

Arrow C++ is the only heavyweight dependency in this note, and it is heavy: a
CMake build, a C++17 toolchain, several minutes of compilation, and somewhere
between 15 and 40 MB of static library.

- *Link a system `libarrow` dynamically.* Smallest binary, but §3 of the core
  spec promises self-contained binaries, and this breaks that promise for any
  program that reads a Parquet file. Rejected as the default.
- *Vendor and statically link it always.* Keeps the promise, and makes every
  Science installation require a C++ compiler, including the ones that will never
  open a Parquet file. Rejected.
- **Make it a build feature of the toolchain distribution.** `sciencec` ships
  without it and needs no C++ compiler at all; `sciencec --features parquet`, or
  equivalently a second published toolchain build, includes it. A program that
  `use`s `data.parquet` without the feature fails at link with a diagnostic in the
  `SC0400` range that names the feature and how to get it. **Chosen.**

§12 of the core spec puts the package manager out of scope for F0, which is why
this is a toolchain feature rather than a dependency. When the package manager
arrives, this becomes an ordinary dependency and **the module path does not
change** — `use data.parquet (read_parquet)` is the same line before and after.
That forward-compatibility is the reason for putting formats in modules outside
the core's closed list (§2).

---

## 10. A complete worked example

Read a month of measurement CSVs, drop invalid rows, normalize a column, write
the result as Parquet.

```science
use fs (Path, glob)
use data (Frame, Record, BadRow)
use data.csv (read_csv, CsvOptions)
use data.parquet (write_parquet, ParquetOptions, Compression)

type Measurement:
    station: String
    timestamp: I64            # epoch seconds; see §5.2 on dates
    temperature: F32
    humidity: F32?

Measurement implements Record

function normalize(values: mutable borrowed Array of F32):
    let count be values.len() as F32
    if count is 0.0f32: return

    let mutable total be 0.0f32
    for v in values:
        total be total + v
    let mean be total / count

    let mutable squares be 0.0f32
    for v in values:
        squares be squares + (v - mean) ** 2
    let deviation be (squares / count).square_root()
    if deviation is 0.0f32: return

    for i in 0..values.len():
        values.set(i, (values.get(i) - mean) / deviation)

function main() -> ((), Error?):
    let files, err be glob("measurements/2026-09-*.csv")
    if err?:
        return ((), err)

    let options be CsvOptions.new()
        .header(true)
        .delimiter(',')
        .null_value("")
        .null_value("NA")
        .null_value("-999")
        .on_bad_row(BadRow.Skip(500))

    let mutable frame, err be read_csv of Measurement(files, options)
        .rows()
        .keep(each.temperature >= -80.0f32)
        .keep(each.temperature <= 70.0f32)
        .keep(each.humidity?)
        .collect()
    if err?:
        return ((), err)

    print("kept rows:", frame.len())
    print("rejected rows:", frame.rejected_count())

    normalize(frame.columns_mutable().temperature)

    let destination be Path.from("measurements").join("clean.parquet")
    let _, err be write_parquet(frame, destination, ParquetOptions.new()
        .compression(Compression.Zstd(3))
        .row_group_rows(1_000_000))
    if err?:
        return ((), err)

    return ((), null)
```

Seven things this example is making a case for:

1. **The schema is four lines of ordinary type declaration** plus one marker
   line. There is no schema literal, no builder, no string-keyed anything.
2. **`read_csv of Measurement(files, ...)` takes a list of files.** One file per
   day is how instruments and pipelines actually write CSV, and a reader that
   cannot span files makes every caller write the concatenation loop.
3. **The filters run on `Rows`, before `.collect()`.** Memory is bounded by the
   IO buffer, not by the input, no matter how many September files there are.
4. **The comparisons are symbols now, and that was this item's case.** In
   revision 1 the filters read `is at least` and `is not None`, and §4.6 of the
   core spec argued for exactly that, in the code where most of an audience's
   time is spent. `syntax-revision-2.md` §1 replaced the word-forms with `>=`
   and `<=`, and §3.1 replaced `is not None` with the postfix `?`. This item's
   case no longer holds, and it is recorded here as lost rather than restated
   with a different justification.
5. **`BadRow.Skip(500)` is a promise with a bound on it.** If the instrument was
   misconfigured and ten thousand rows are unparseable, this program stops and
   says so instead of quietly normalizing the survivors.
6. **`normalize(frame.columns_mutable().temperature)` mutates the column in
   place, through a `mutable borrowed Array of F32`.** No copy, no `.values`, no
   `.to_numpy()`. While that borrow is live the frame cannot be appended to or
   dropped — which means the pandas `SettingWithCopyWarning`, the most notorious
   footgun in data science, is a compile error in the `SC0300` range here rather
   than a warning nobody reads. §6.5 of the core spec argues that ownership earns
   its cost on device memory and random keys; this is the third case, and it is
   the one users meet on day one.
7. **`write_parquet` is atomic**, so a crash during the write leaves the previous
   `clean.parquet` intact.

And the continuation into F1, for completeness — no copy anywhere on this line:

```science
let x be Tensor.viewing(frame.columns().temperature)     # Tensor of (F32, (n,))
```

---

## 11. What this note asks of the other notes

Stated explicitly so the seams are visible rather than assumed.

1. **`Record` derivation** (§4.3). The one language-level ask. It amends §8 of
   the core spec and sits next to §12's "macros are reserved, not implemented";
   that boundary needs drawing.
2. **Explicit type arguments at call sites** — `read_csv of Measurement(...)`.
   §4.3 of the core spec gives `of T` in declaration position and
   `Array of Doc .new()` in type position, but not at a call. It is needed
   because `R` appears only in the return type, and requiring a binding
   annotation instead would break every chain: `.rows().keep(each.temperature
   ...)` cannot resolve `.keep`'s closure until `R` is known, and local inference
   (§5.2) would have to run to the end of the chain first.
3. **`Instant` and `Duration`** (§5.2), minimal: I64 nanoseconds, UTC, no
   calendars. Without them the CSV story is missing dates, which is a large hole.
4. **Numeric methods** — `square_root()` and the reductions — belong to the
   numerics note; the worked example uses one and would be written differently if
   the answer is a free function `square_root(x)`.
5. **Formatting** — the example's `print("kept rows:", frame.len())` assumes a
   variadic `print` over `Display`. If formatting takes a different shape, the
   example changes and nothing else does.
6. **Array literals** — this note avoids them (`null_value` is called three times
   rather than taking a list) because §4.2 of the core spec lists no array
   literal. If one is added, these builders should take it.
7. **Table operations** — group-by, join, sort, aggregate over a `Frame` — are
   not in this note. If another note designs them, `Frame of R` is the type they
   operate on, and the columnar layout of §4.1 is what makes them cheap.
8. **F1 tensors** should offer `Tensor.viewing(borrowed Array of T)` as a
   constructor, or this note's zero-copy claim does not hold.

---

## 12. Deliberately out of scope

- HDF5, NetCDF, images, Excel, Zarr, MATLAB — §3, with reasons.
- Nested and repeated Parquet columns — §5.2.
- Any network or object-store path. Everything is a local `Path`. Remote reads
  arrive when `Path` gains a scheme, and nothing in this API changes shape then.
- Out-of-core sort, join, or group-by — §6.2.
- A query optimizer or a lazy expression graph — §6.1.
- Text encodings other than UTF-8 and Latin-1 — §5.2.
- Recursive directory deletion — §7.
- Runtime schema inference — §5.1. The tool, not the library.
