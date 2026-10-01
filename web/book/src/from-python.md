If you know Python, you already know most of what Science asks of you: indentation makes blocks, `for` walks a collection, f-strings build text. This page shows the Python you would write on the left and the Science on the right, then looks at the habits that need to change.

## Side by side

| Python | Science |
|---|---|
| `x = 5` | `let x be 5` |
| `x = 6` (reassign) | `x be 6` (after `let mutable x be 5`) |
| `def f(a, b):` | `def f(a: Int, b: Int) -> Int:` |
| `class Point:` | `type Point:` |
| `if x == 3:` | `if x is 3:` |
| `if x != 3:` | `if x is not 3:` |
| `!x` is `not x` | `not x` |
| `while cond:` | `loop:` and `break` |
| `for i in range(n):` | `for i in 0..n:` |
| `[1, 2, 3]` | `[1, 2, 3]` |
| `{"a": 1}` | `Map[String, Int]` |
| `None` | `null` |
| `Optional[int]` | `Int?` |
| `raise` / `try` / `except` | return `(value, Error?)` |
| `[x * 2 for x in xs]` | `xs.iterate().map(x giving x * 2).collect()` |
| `print(f"{x}")` | `print(f"{x}")` |
| `import math` | `use math` |
| `# comment` | `# comment` |

The rest of this page explains the rows that are not obvious.

## Names and assignment

In Python one symbol, `=`, creates a variable and changes it. In Science the word `be` does both, and `let` marks the first time:

```science run
def main():
    let total be 10
    let mutable count be 0
    count be count + 1
    print(total)
    print(count)
```

```output
10
1
```

A name made with `let` cannot be changed. If you will change it, say `let mutable`. That is the only way the compiler can tell, when you read code, which values stay fixed.

```science fails
def main():
    let count be 0
    count be count + 1
```

There is no `=` operator at all. Equality is the word `is`:

```science run
def main():
    let answer be 42
    if answer is 42:
        print("yes")
    if answer is not 7:
        print("not seven")
```

```output
yes
not seven
```

## Types are written down

Python lets a function take anything. In Science each parameter and the result have a type, and the compiler checks every call:

```science run
def area(width: F64, height: F64) -> F64:
    width * height

def main():
    print(area(3.0, 4.5))
```

```output
13.5
```

The last expression in a function is its result, so there is no `return`. You can write `return` to leave early, as in Python.

> **Note:** The Python habit of leaving types out does not carry over. Inside a function, though, you rarely need to write a type: `let total be 10` already knows it is an `Int`.

## No `while`, no `range`

Counting uses a range of numbers. `0..n` goes from 0 up to, but not including, `n`, the same as `range(n)`:

```science run
def main():
    for i in 0..3:
        print(i)
```

```output
0
1
2
```

A loop that repeats until something is true is `loop:` with a `break`. Put the test inside:

```science run
def main():
    let mutable n be 1
    loop:
        if n >= 100:
            break
        n be n * 2
    print(n)
```

```output
128
```

## Lists, and no comprehensions

A list is written the same way, but its name is `Array`, and every item has the same type. Where Python would use a comprehension, Science chains steps onto `.iterate()`:

```science run
def main():
    let numbers be [1, 2, 3, 4]
    let doubled be numbers.iterate().map(n giving n * 2).collect()
    for n in doubled:
        print(n)
```

```output
2
4
6
8
```

`map` changes each item, `discard` drops items for which the test is true, and `collect()` gathers the result into a new array. The word before `giving` names the item. [Chains and closures](chains.md) has the full set.

Python's `xs[5]` raises `IndexError` when the list is too short. Here `get` hands back a value that may be missing, which leads to the next section.

## `None` is `null`, and it is in the type

In Python any variable might be `None`, and you find out when the program crashes. In Science a value that may be missing has a `?` in its type, and you must check it before use:

```science run
def main():
    let scores be [3, 9, 4]
    let first be scores.get(0)
    if first?:
        print(f"first: {first}")
    let missing be scores.get(10)
    if not missing?:
        print("no tenth score")
```

```output
first: 3
no tenth score
```

`first?` asks "is there a value?". Inside the `if`, the value can be used directly. [Missing values and errors](errors.md) covers this in full.

## No exceptions

Python signals failure by raising and catching. Science functions that can fail return the value and an error side by side, and the caller checks:

```science run
def main():
    let value, err be "forty".parse_int()
    if err?:
        print("not a number")
    else:
        print(value)

    let ok, bad be "42".parse_int()
    if bad?:
        print("not a number")
    else:
        print(ok)
```

```output
not a number
42
```

There is no `try`, `except` or `raise`. The failure is part of the function's signature, so you cannot forget that it can happen. A function that can fail says `-> (Int, Error?)`.

## Indentation is the same

Blocks are the lines indented under a `:`. Four spaces is the convention, and tabs are refused. A short body can sit on the same line, when it is a single expression:

```science run
def main():
    let n be 5
    if n > 3: print("big") else: print("small")
```

```output
big
```

## Classes become types

Python puts data and methods together in `class`. Science separates them: `type` holds the fields, and a `has` block holds the methods.

```science run
type Point:
    x: F64
    y: F64

Point has:
    def length(self) -> F64:
        (self.x * self.x + self.y * self.y).sqrt()

def main():
    let p be Point(x: 3.0, y: 4.0)
    print(p.length())
```

```output
5.0
```

There is no `__init__`. You build a value by naming its fields, `Point(x: 3.0, y: 4.0)`. `self` is written as the first parameter of a method and is never passed by the caller. [Types](types.md) has more.

## Imports

`use` replaces `import`, and you list the names you want:

```science run
use complex (Complex)

def main():
    print(Complex.new(3.0, 4.0).modulus())
```

```output
5.0
```

[The standard library tour](std.md) lists what you can `use`.

## Summary

- `let x be v` creates a name; `x be v` changes it; `let mutable` allows change.
- Equality is `is`, not `==`. There is no `=` operator.
- No `while` and no `range`: use `loop:` with `break`, and `for i in 0..n:`.
- Types are written on parameters and results.
- `None` is `null`, and `T?` in a type means "might be missing".
- Failure is a returned pair, not an exception.
- Comprehensions are chains: `.iterate().map(...).collect()`.
- `class` becomes `type` plus a `has` block.

Next: [coming from Rust](from-rust.md), or the list of [keywords](keywords.md).
