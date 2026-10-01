A **generic** function or type works for many types at once. You write it
one time, with a placeholder for the type.

## A generic function

Square brackets after the name declare a placeholder, conventionally `T`:

```science run
def identity[T](value: T) -> T:
    value

def main():
    print(identity(5))
    print(identity("text"))
```

```output
5
text
```

`identity` takes a `T` and gives back a `T`. The compiler works out what `T`
is from the argument: `Int` in the first call, `String` in the second. It
makes a separate, fully typed copy of the function for each, so generics
cost nothing at run time.

## Bounds

A placeholder on its own can do almost nothing, because the compiler knows
nothing about it. A **bound** says what `T` must be able to do. Write it
after a colon:

```science run
def largest[T: Ord](items: &Array[T]) -> (&T)?:
    let mutable best be items.get(0)
    for item in items:
        if best?:
            if item > best:
                best be item
        else:
            best be item
    best

def main():
    let nums be [3, 9, 4]
    let top be largest(nums)
    if top?:
        print(top)

    let words: Array[String] be ["pear", "apple"]
    let first be largest(words)
    if first?:
        print(first)
```

```output
9
pear
```

`T: Ord` means "any type that can be compared with `<` and `>`", so the
body may use `>`. The result is `(&T)?`: a borrowed element, or `null` for
an empty array. [Missing values and errors](errors.md) explains the `?`.

Call a bounded function with a type that does not meet the bound and the
compiler refuses:

```science fails
interface Speak:
    def speak(self) -> String

type Rock:
    weight: Int

def announce[T: Speak](item: &T):
    print(item.speak())

def main():
    announce(Rock(weight: 3))
```

## Several bounds

Join bounds with `+`:

```science run
type Pair[A, B]:
    first: A
    second: B

def duplicate[T: Clone + Eq](value: &T) -> Pair[T, T]:
    Pair(first: value.clone(), second: value.clone())

def main():
    let copies be duplicate(5)
    print(copies.first + copies.second)
```

```output
10
```

## Where clauses

When the bounds make the signature long, move them to a `where` clause on
the next, indented line:

```science run
interface Summarize:
    def summarize(self) -> String

type Doc:
    title: String

Doc implements Summarize:
    def summarize(self) -> String:
        f"doc: {self.title}"

def both[A, B](left: &A, right: &B) -> String
        where A: Summarize, B: Summarize:
    f"{left.summarize()} and {right.summarize()}"

def main():
    let a be Doc(title: "one")
    let b be Doc(title: "two")
    print(both(a, b))
```

```output
doc: one and doc: two
```

Each placeholder can have its own bounds, separated by commas. Your own
[interfaces](interfaces.md) work as bounds just like the built-in ones.

## Generic types

Types take placeholders too:

```science run
type Pair[A, B]:
    first: A
    second: B

def main():
    let a be Pair(first: 1, second: "one")
    let b: Pair[Float, Bool] be Pair(first: 2.5, second: true)
    print(a.second)
    print(b.first)
```

```output
one
2.5
```

`Pair[Int, String]` and `Pair[Float, Bool]` are different types built from
one definition. Generic arguments go in square brackets, and they nest:
`Array[Pair[Int, String]]`.

## Methods on a generic type

Repeat the placeholders on the left of `has:`:

```science run
type Pair[A, B]:
    first: A
    second: B

Pair[A, B] has:
    def new(first: A, second: B) -> Pair[A, B]:
        Pair(first: first, second: second)

    def swapped(self: Self) -> Pair[B, A]:
        Pair(first: self.second, second: self.first)

def main():
    let p be Pair[Int, String].new(1, "one")
    let q be p.swapped()
    print(q.first)
    print(q.second)
```

```output
one
1
```

`self: Self` takes the value itself rather than borrowing it, so `swapped`
can move the fields into the new pair.

When you call a function like `new` through the type, give the type
arguments in brackets, as in `Pair[Int, String].new(...)`. Written as
`Pair.new(...)`, there is nothing to say what `A` and `B` are, and the
compiler asks you to spell them out.

## Numbers as parameters

A type can also be generic over a whole number. Mark it `const`:

```science run
type Grid[T, const ROWS: Int, const COLS: Int]:
    cells: Array[T]

Grid[T, const ROWS: Int, const COLS: Int] has:
    def rows(self) -> Int:
        ROWS

    def size(self) -> Int:
        ROWS * COLS

def main():
    let g: Grid[Int, 2, 3] be Grid(cells: [1, 2, 3, 4, 5, 6])
    print(g.rows())
    print(g.size())
```

```output
2
6
```

The size is part of the type, so a `Grid[Int, 2, 3]` and a `Grid[Int, 3, 2]`
are different types and cannot be mixed up. Inside the type, `ROWS` is an
ordinary `Int`.

## Summary

- `def name[T](...)` declares a generic function; `type Name[A, B]:` a
  generic type.
- `[T: Ord]` is a bound: `T` must implement that interface. Join bounds
  with `+`, or put them in a `where` clause.
- Methods on a generic type are written `Pair[A, B] has:`.
- Call associated functions with explicit arguments: `Pair[Int, String].new(...)`.
- `const N: Int` makes a number a parameter of a type.

Next: split a program across files with [modules](modules.md).
