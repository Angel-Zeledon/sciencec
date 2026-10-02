Science comes with a library. Some of it is always there; the rest you ask for by name. This chapter shows the difference and lists everything on offer.

## The prelude: always there

You have been using the prelude since the first page. These need no `use`:

- Types: `String`, `Array`, `Map`, `Set`, `Box` and `Range`.
- Interfaces: `Eq`, `Ord`, `Copy`, `Clone`, `Drop`, `Iterate`, `From`, `Display`, `Error`, and the operator interfaces such as `Add` and `Mul`.
- Functions: `print`, `write`, `print_error`, `write_error`, `flush`, `panic`, `read_file` and `write_file`.
- Math methods on numbers, like `x.sqrt()`.

```science run
def main():
    let words be ["one", "two"]
    print(words.length())
    print(2.0.sqrt())
```

```output
2
1.4142135623730951
```

## Modules: ask with `use`

Everything else lives in a module. A `use` line at the top of a file names the module and the things you want from it:

```science run
use complex (Complex)
use path (Path)

def main():
    print(Complex.new(3.0, 4.0).modulus())
    print(Path.from("results").join("run.csv").text())
```

```output
5.0
results/run.csv
```

A module is ordinary Science code that ships with the compiler. Using one adds nothing to your program except the parts you name.

> **Note:** If your project has a file with the same name as a bundled module, such as `io.science`, your file is used instead.

## Every bundled module

| Module | What it is for | Chapter |
|---|---|---|
| `io` | Files, buffered reading and writing, standard input | [Files and the system](std-io.md) |
| `fs` | Creating, listing, renaming and removing files and directories | [Files and the system](std-io.md) |
| `path` | `Path`: taking file locations apart and joining them | [Files and the system](std-io.md) |
| `os` | Command-line arguments and environment variables | [Files and the system](std-io.md) |
| `time` | `Duration`, `Monotonic`, `Instant`, `now`, `sleep`, RFC 3339 text | [Files and the system](std-io.md) |
| `complex` | `Complex` numbers | [Numbers and arrays](std-numeric.md) |
| `random` | Reproducible `Key`s and quick `Stream`s | [Numbers and arrays](std-numeric.md) |
| `stats` | Mean, variance, median, quantiles, correlation | [Numbers and arrays](std-numeric.md) |
| `ndarray` | `NdArray`: grids of numbers | [Numbers and arrays](std-numeric.md) |
| `linalg` | Solving, inverting and decomposing matrices | [Numbers and arrays](std-numeric.md) |
| `csv` | Reading and writing tables | [Data formats](std-formats.md) |
| `json` | Parsing, reading and writing JSON | [Data formats](std-formats.md) |
| `encoding` | Base64, hex, UTF-8 and UTF-16 | [Data formats](std-formats.md) |
| `string` | Case, padding, wrapping and edit distance | [Data formats](std-formats.md) |
| `collections` | `Deque`: a queue open at both ends | below |

## A queue open at both ends

`collections` is the one module without a chapter of its own. A `Deque` adds and removes items at either end quickly:

```science run
use collections (Deque)

def main():
    let mutable queue be Deque[Int].new()
    queue.push_back(2)
    queue.push_back(3)
    queue.push_front(1)
    print(queue.length())
    let first be queue.pop_front()
    if first?:
        print(first)
    let last be queue.pop_back()
    if last?:
        print(last)
    print(queue.length())
```

```output
3
1
3
1
```

`pop_front` and `pop_back` give a value that may be missing, since the queue could be empty. `first`, `last` and `get(index)` look without removing.

## Reading a module's own notes

Each module begins with a comment that says what it decided and what it leaves out. If you want to know exactly how a function behaves, that comment is the place to look.

## Summary

- The prelude needs no `use`: strings, arrays, maps, printing, simple file reading and number methods.
- Everything else is a module: `use name (Thing, other_thing)`.
- Files and the system are in [Files and the system](std-io.md), numbers in [Numbers and arrays](std-numeric.md), text formats in [Data formats](std-formats.md).
