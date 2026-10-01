Science is a compiled language for scientific computing. You write it like a
script, it runs like C, and the compiler checks the things that usually go
wrong quietly in numerical code: memory, missing values, and errors nobody
looked at.

```science run
let samples be [2.5, 3.1, 2.8, 3.4]
let mutable total be 0.0
for x in samples:
    total be total + x
print(f"mean = {total / 4.0:.2f}")
```

```output
mean = 2.95
```

This book teaches the language from the beginning. Read it in order: each
chapter uses only what came before it. Every example on these pages is real
code that the compiler checks every time the book is built, and the ones
that show output were actually run.

## Who this book is for

You know at least one programming language — Python, R, Julia, C, Rust,
anything. You do not need to know anything about compilers or memory
management; the chapters that need those ideas explain them.

## How to use this book

- **New to Science?** Start with [Installation](installation.md) and keep
  pressing *Next*. The left and right arrow keys work too.
- **Coming from Python or Rust?** Skim [Coming from Python](from-python.md)
  or [Coming from Rust](from-rust.md) first. Science looks like both and is
  neither, and those pages list the habits that will trip you up.
- **Looking something up?** The [standard library tour](std.md) lists every
  module, and the [reference](../reference.html) describes every construct
  in detail.

## What Science looks like

A taste, before the details. Don't worry about understanding every line yet;
each one is explained in a later chapter.

```science run
type Reading:
    sensor: String
    value: F64

def warmest(readings: &Array[Reading]) -> String:
    let mutable best be ""
    let mutable top be -1000.0
    for r in readings:
        if r.value > top:
            top be r.value
            best be f"{r.sensor}"
    best

def main():
    let readings be [
        Reading(sensor: "north", value: 12.5),
        Reading(sensor: "south", value: 18.25),
        Reading(sensor: "east", value: 15.0),
    ]
    print(f"warmest: {warmest(readings)}")
```

```output
warmest: south
```

A few things stand out, and the book comes back to each of them:

- `let x be value` names a value, and `x be value` changes it. There is no `=`.
- Blocks are indented, like Python. There are no braces.
- Types are checked before the program runs, like Rust, but you rarely have
  to write them inside a function.
- `&Array[Reading]` means the function *borrows* the array: it can read it,
  and the caller keeps it.
