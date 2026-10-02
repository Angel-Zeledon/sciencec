A **package** is a folder with a `science.toml` file. It is the unit you
build, test and share. [Your first project](first-project.md) created one;
this chapter explains the parts.

## The layout

```shell
sciencec new stats
```

```text
stats/
  science.toml        the package's description
  science.lock        written by sciencec; do not edit
  src/
    main.science      the entry point
    greeting.science  another module
  tests/
    greeting.science  one program per test
  target/             build output
```

(`science.lock` and `target/` appear after the first build.) Everything you
write for the program itself goes under `src/`. Every file there is a
[module](modules.md), and `main.science` is where the program starts.

## science.toml

```text
[package]
name     = "stats"
version  = "0.1.0"
language = "0.1"

[dependencies]
```

`name` is also the name other code uses to refer to the package. `version`
is yours to choose. `language` is the version of Science the package is
written for.

## Commands

Run these inside the package folder. None of them takes a file name:

| Command | What it does |
|---|---|
| `sciencec check` | Finds errors without building |
| `sciencec build` | Builds the program into `target/` |
| `sciencec run` | Builds, then runs it |
| `sciencec test` | Builds and runs each file in `tests/` |

Arguments for your program go after `--`:

```shell
sciencec run -- first "two words"
```

Inside the program, `args()` from the bundled `os` module returns them.

## Using your own modules

Inside a package, a module's path starts with the package name. If
`src/greeting.science` has a `public def greeting()`, any file in the
package reads it with:

```science norun
use stats.greeting (greeting)
```

This differs from a single-file program, where a path starts at the
directory. In a package it always starts at the package name.

## Depending on another package

To use code from a second package, list it under `[dependencies]` with the
folder it lives in. Suppose `mathx` sits beside `stats`:

```text
mathx/
  science.toml
  src/
    mod.science
    stats.science
stats/
  science.toml
  src/
    main.science
```

`mathx/src/mod.science` is the package's own top-level module:

```science
public def double(n: Int) -> Int:
    n * 2
```

and `mathx/src/stats.science` is a second module in it:

```science
public def mean(values: &Array[F64]) -> F64:
    let mutable total be 0.0
    for v in values:
        total be total + v
    total / values.length() as F64
```

Tell `stats` where to find it:

```text
[dependencies]
mathx = { path = "../mathx" }
```

Now `stats` can use the package by name. `mod.science` is reached by the
bare package name, and the other files by `package.module`:

```science norun
use mathx (double)
use mathx.stats (mean)

def main():
    print(double(21))
    print(mean([1.0, 2.0, 6.0]))
```

```shell
sciencec run
```

```output
42
3.0
```

A dependency's non-`public` items stay private, exactly as between modules.

> **Note:** Dependencies are found by `path` only. Downloading packages
> by version is not available yet.

## The lock file

The first build writes `science.lock`, a record of the packages that went
into the build. Commit it with your code, and leave editing to `sciencec`.

## Summary

- A package is a folder with `science.toml`, code in `src/`, and tests in
  `tests/`.
- `sciencec new`, `check`, `build`, `run` and `test` are the everyday
  commands.
- Inside a package, `use` paths start with the package name.
- `[dependencies]` lists other packages as `name = { path = "../folder" }`.

Next: [Testing](testing.md).
