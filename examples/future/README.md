# The library corpus — what does not exist yet

These files are **not part of the working corpus** and none of them builds.
They show what the libraries in [`docs/DREAM.md`](../../docs/DREAM.md) Part IV
should look like in real code, written in **today's syntax**, so that the API
is argued about before it is implemented rather than after.

`examples/*.science` one directory up is the opposite: every file there is
accepted by the compiler, and anything that fails to lex, parse, resolve or
type-check is a bug. Nothing here makes that claim.

## Why this is a subdirectory and not a suffix

Nine test suites `read_dir` the top level of `examples/` and take every
`*.science` they find, then lex, parse, resolve, type-check and format-check
it. A file here naming `tensor` or `linalg` would fail to resolve, and those
suites read a resolution failure as a compiler bug. A subdirectory is invisible
to eight of them.

**The ninth is `science-lexer`'s corpus, which recurses on purpose**, so these
files must lex clean and end in a dedent to column zero. That is a constraint
worth having: it guarantees everything here is at least lexically Science, and
it is checked on every run rather than promised in a comment.

## What each file covers

| File | DREAM | Libraries |
|---|---|---|
| `01_collections_and_chains.science` | §13.3, §13.4 | `string`, `collections`, the chain vocabulary |
| `02_math_and_stats.science` | §14 | `math`, `random`, `stats` |
| `03_tensor_and_linalg.science` | §15, §16 | `tensor`, broadcasting, `linalg` |
| `04_machine_learning.science` | §18 | `autograd`, `nn`, `optim`, `model` |
| `05_data_and_io.science` | §19, §21 | `dataframe`, `csv`, `json`, `fs`, `path` |
| `06_system_net_and_concurrency.science` | §17.4, §21, §22 | `os`, `time`, `http`, `thread`, `sync`, `parallel` |
| `07_interop.science` | §23 | `c-ffi` (works today), `python-ffi` (designed) |
| `08_agents_and_web.science` | §26, §27 | `llm`, `tools`, `agents`, `http-server`, `router` |
| `09_testing_and_plotting.science` | §20, §25 | `test`, `bench`, `plot` |

## The rules these files follow

They are design arguments, so they are held to the same standard as the notes:

- **Today's syntax, not the Part II proposals.** `use` and not `import`,
  `0..n` and not `range(n)`, `&T` and `&mut T`, `let a, b, err be f()`,
  `each` and `name giving expression` for closures. Appendix A of `DREAM.md`
  uses the proposed syntax instead and says so.
- **Every fallible call answers a pair.** No variant panics on a missing file,
  a singular matrix or a failed parse — those are ordinary inputs here.
- **The comment says why, not what.** A signature that needs no defence gets
  no comment; one that rejects an obvious alternative explains the rejection.

When a library lands for real, its file here is deleted and a proper example
takes its place one directory up, with its output pinned byte for byte.
