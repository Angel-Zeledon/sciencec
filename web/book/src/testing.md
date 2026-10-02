In Science a test is a program that stops when something is wrong. You write
the check with `assert`, and `sciencec test` runs the program and tells you
whether it finished.

## assert

`assert(condition)` does nothing when the condition is true. When it is
false, the program stops:

```science run
def square(n: Int) -> Int:
    n * n

def main():
    assert(square(3) is 9)
    print("square works")
```

```output
square works
```

Add a second argument to say what went wrong. The message is printed when
the assertion fails:

```science norun
assert(square(2) is 5, "square(2) should be 4")
```

## Running a test file

`sciencec test FILE` builds the file, runs it once, and reports the result:

```shell
sciencec test square.science
```

```output
test square.science ... ok
```

When an `assert` fails, you see its message and the file is reported as
failed:

```shell
sciencec test square.science
```

```error
panic: square(2) should be 4
test square.science ... FAILED (signal 6)
```

`sciencec test` exits with a non-zero status after a failure, so a script or
a continuous-integration job can rely on it.

## Several tests in one file

Give each check a function and call them from `main`:

```science run
def mean(values: &Array[F64]) -> F64:
    let mutable total be 0.0
    for v in values:
        total be total + v
    total / values.length() as F64

def close(a: F64, b: F64) -> Bool:
    (a - b).abs() < 0.000001

def test_mean_of_pair():
    assert(close(mean([2.0, 4.0]), 3.0), "mean of 2 and 4")

def test_mean_of_one():
    assert(close(mean([5.0]), 5.0), "mean of one value")

def main():
    test_mean_of_pair()
    test_mean_of_one()
    print("all passed")
```

```output
all passed
```

The `test_` names are only a habit, so you can tell the checks from the
code they check. The first failing `assert` stops the program, so fix
failures one at a time.

Two habits are worth having. Compare decimal numbers with a small
tolerance, as `close` does, because `0.1 + 0.2` is not exactly `0.3`. And
use `is` to compare, not `==`.

## Testing failures

A function that can fail returns a pair of a value and an error. A test can
check both outcomes:

```science run
def test_parse():
    let value, err be "42".parse_int()
    assert(not err?, "42 should parse")
    assert(value is 42)

    let _bad, bad_err be "forty".parse_int()
    assert(bad_err?, "forty should not parse")

def main():
    test_parse()
    print("parse ok")
```

```output
parse ok
```

`err?` is true when there is an error. Names that start with `_` mark a
value you bind but deliberately do not use.

## Tests in a package

In a [package](packages.md), each file in `tests/` is one test program, and
`sciencec test` with no file name runs them all:

```text
stats/
  src/
    main.science
    stats.science
  tests/
    mean.science
    parsing.science
```

A test file imports the package's modules like any other code:

```science norun
use stats.stats (mean)

def main():
    assert(mean([2.0, 4.0]) is 3.0, "mean of 2 and 4")
```

```shell
sciencec test
```

```output
test tests/mean.science ... ok
test tests/parsing.science ... ok
```

Every file runs, even after one fails, and each gets its own line. To run
one file, name it: `sciencec test tests/mean.science`.

> **Note:** A test file named on its own is built as a single file, so it
> cannot use the package's modules. Use plain `sciencec test` for tests that
> import them.

## Summary

- `assert(condition)` stops the program when the condition is false;
  `assert(condition, message)` says why.
- `sciencec test FILE` runs a file and reports `ok` or `FAILED`.
- Group checks in functions and call them from `main`.
- In a package, each file in `tests/` is a test, and `sciencec test` runs
  them all.

Next: the [standard library](std.md).
