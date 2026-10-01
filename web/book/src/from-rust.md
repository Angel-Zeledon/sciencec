If you know Rust, Science will feel familiar: ownership, borrowing, generics and interfaces all work the way you expect. What changes is the spelling, and a few places where the design is simpler. This page maps one to the other.

## Side by side

| Rust | Science |
|---|---|
| `fn f(x: i32) -> i32` | `def f(x: Int) -> Int` |
| `let x = 5;` | `let x be 5` |
| `let mut x = 5;` | `let mutable x be 5` |
| `x = 6;` | `x be 6` |
| `struct Doc { ... }` | `type Doc:` |
| `enum Format { ... }` | `choice Format:` |
| `impl Doc { ... }` | `Doc has:` |
| `impl Summarize for Doc { ... }` | `Doc implements Summarize:` |
| `trait Summarize { ... }` | `interface Summarize:` |
| `&self` / `&mut self` | `self` / `mutable self` |
| `dyn Shape` | `any Shape` |
| `a == b` / `a != b` | `a is b` / `a is not b` |
| `!flag` | `not flag` |
| `while cond { ... }` | `loop:` and `break` |
| `for i in 0..n { ... }` | `for i in 0..n:` |
| `Option<T>` | `T?` |
| `Result<T, E>` | `(T, E?)` |
| `pub fn` | `public def` |
| `use std::fs;` | `use fs` |
| `println!("{x}")` | `print(f"{x}")` |
| `Vec<T>`, `HashMap<K, V>` | `Array[T]`, `Map[K, V]` |

Blocks are indentation, not braces, and statements end at the end of the line, not at a semicolon. Generics use square brackets: `Array[Doc]`, `Map[String, Int]`.

## Types and methods are written apart

A Rust `struct` and its `impl` block become a `type` and a `has` block. The type comes first and the methods follow:

```science run
type Doc:
    title: String
    hits: Int

Doc has:
    def new(title: String) -> Doc:
        Doc(title: title, hits: 0)

    def popular(self) -> Bool:
        self.hits >= 10

def main():
    let doc be Doc.new("Regions")
    print(doc.popular())
```

```output
false
```

Fields are given by name when you build a value: `Doc(title: title, hits: 0)`. A method that reads the value takes `self`, which stands for Rust's `&self`. One that changes it takes `mutable self`, for `&mut self`. To take ownership, write `self: Self`.

## `impl Trait for Type` reads the other way

An interface is Rust's trait. When you implement one, the type comes first:

```science run
interface Summarize:
    def summarize(self) -> String

type Doc:
    title: String

Doc implements Summarize:
    def summarize(self) -> String:
        f"doc: {self.title}"

def main():
    let doc be Doc(title: "Regions")
    print(doc.summarize())
```

```output
doc: Regions
```

Read it aloud: "Doc implements Summarize". An interface can have default methods, and bounds on a generic go after `where`, as in Rust. `dyn Trait` is `any Interface`. [Interfaces](interfaces.md) and [Generics](generics.md) have the details.

## `Option` and `Result` are not types

Rust wraps a missing value in `Option<T>`. Science writes `T?` and uses `null` for the absent case. You test with `?` and, inside the `if`, the value is already unwrapped:

```science run
def main():
    let scores be [3, 9, 4]
    let first be scores.get(0)
    if first?:
        print(first)
    let none be scores.get(9)
    if not none?:
        print("empty")
```

```output
3
empty
```

There is no `Some` or `None` to write, and no `unwrap`.

Failure works the same way, with a pair instead of a `Result`. The value and the error come back side by side, and `?` tests the error:

```science run
def half(n: Int) -> (Int, Error?):
    (n / 2, null)

def main():
    let value, err be half(10)
    if err?:
        print("failed")
    else:
        print(value)
```

```output
5
```

> **Note:** `?` in Science is not Rust's propagation operator. It never returns from the function. It only answers "is there an error?" as a `Bool`. If you want to pass an error on, you write the `if` and the `return` yourself.

The error type is usually `Error?`, the interface, or a concrete type such as `IoError?`. [Missing values and errors](errors.md) explains when to use which.

## `?` as a test, `return` as the exit

Here is the shape you will write where Rust would write `let x = f()?;`:

```science run
def parse_pair(a: &String, b: &String) -> (Int, Error?):
    let x, err be a.parse_int()
    if err?:
        return (0, err)
    let y, err2 be b.parse_int()
    if err2?:
        return (0, err2)
    (x + y, null)

def main():
    let sum, err be parse_pair("4", "5")
    if err?:
        print("bad input")
    else:
        print(sum)
```

```output
9
```

It is longer than `?` in Rust, and every exit is visible on the page.

## Borrowing is automatic at call sites

Ownership and borrowing work as you know them: a value has one owner, and a reference cannot outlive it. The difference is that you do not write the `&` when you call. A parameter declared `&T` is borrowed for you:

```science run
def total(items: &Array[Int]) -> Int:
    let mutable sum be 0
    for item in items:
        sum be sum + item
    sum

def main():
    let scores be [3, 9, 4]
    print(total(scores))
    print(scores.length())
```

```output
16
3
```

`scores` is still usable after the call, because `total` only borrowed it. You write `&` where a borrow is not a call, such as `let view be &doc`. A mutable borrow is `&mut T`.

## Other spellings that surprise Rust readers

- Calls always have parentheses. `text.length()` is a call; `x.dims` is a field.
- There is no `;`, no `{}`, no lifetimes to name, and no macros: `print` and `assert` are ordinary-looking calls.
- `a is b` replaces `==`, and `not` replaces `!`.
- `String` has no `clone`. Write `f"{text}"` to get an owned copy of a borrowed string.
- The last expression of a block is its value, as in Rust, but there is no trailing-semicolon trick: nothing needs one.
- `loop:` exists and `while` does not. Put the test inside the loop with `break`.

## Summary

- `fn`, `struct`, `enum`, `impl`, `trait`, `dyn` become `def`, `type`, `choice`, `has` / `implements`, `interface`, `any`.
- `impl Trait for Type` becomes `Type implements Trait`: the order reverses.
- `Option<T>` is `T?`; `Result<T, E>` is a pair `(T, E?)`.
- `?` tests for an error and never returns.
- Borrowing is written in parameter types, not at call sites.
- `==` is `is`, `!` is `not`, and `while` is `loop:` with `break`.

Next: the list of [keywords](keywords.md).
