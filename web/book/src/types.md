A type groups several values under one name. Use it whenever a few numbers or strings belong together, such as a measurement, a sample or a point.

## Declaring a type

A `type` lists its **fields**, one per line, each with a name and a type:

```science
type Reading:
    channel: Int
    counts: Int
```

This declares a new type called `Reading` with two fields. Nothing is created yet; you have only described what a `Reading` looks like.

## Creating a value

To make a `Reading`, write its name and give every field a value by name:

```science run
type Reading:
    channel: Int
    counts: Int

def main():
    let r be Reading(channel: 1, counts: 4900)
    print(r.channel)
    print(r.counts)
```

```output
1
4900
```

The names are required, so a call never leaves you guessing which number is which. They can come in any order:

```science run
type Reading:
    channel: Int
    counts: Int

def main():
    let r be Reading(counts: 120, channel: 2)
    print(f"channel {r.channel} saw {r.counts} counts")
```

```output
channel 2 saw 120 counts
```

A field name that the type does not have is an error, so a typo is caught before the program runs.

## Reading and changing fields

`value.field` reads a field. To change one, the value has to be declared `let mutable`, and you assign with `be` like any other name:

```science run
type Reading:
    channel: Int
    counts: Int

def main():
    let mutable r be Reading(channel: 1, counts: 0)
    r.counts be r.counts + 5
    r.counts be r.counts + 5
    print(r.counts)
```

```output
10
```

A value that is not `mutable` cannot be changed:

```science fails
type Reading:
    channel: Int
    counts: Int

def main():
    let r be Reading(channel: 1, counts: 0)
    r.counts be 5
    print(r.counts)
```

The compiler says `r` is not mutable and suggests `let mutable r`.

## Types inside types

A field can have any type, including another type you wrote:

```science run
type Point:
    x: F64
    y: F64

type Segment:
    start: Point
    end: Point

def main():
    let s be Segment(start: Point(x: 0.0, y: 0.0), end: Point(x: 3.0, y: 4.0))
    print(s.end.x)
    print(s.end.y)
```

```output
3.0
4.0
```

Read the dots left to right: the `end` of `s`, then its `x`.

## Methods

A function that belongs to a type is a **method**. Write them in a block that starts with the type's name and `has:`:

```science run
type Point:
    x: F64
    y: F64

Point has:
    def norm_squared(self) -> F64:
        self.x * self.x + self.y * self.y

def main():
    let p be Point(x: 3.0, y: 4.0)
    print(p.norm_squared())
```

```output
25.0
```

The first parameter, `self`, is the value the method was called on. You do not pass it when you call: `p.norm_squared()` hands `p` to the method as `self`. Inside the method you reach the fields through `self`.

Methods take ordinary parameters after `self`:

```science run
type Point:
    x: F64
    y: F64

Point has:
    def distance(self, other: &Point) -> F64:
        let dx be self.x - other.x
        let dy be self.y - other.y
        (dx * dx + dy * dy).sqrt()

def main():
    let a be Point(x: 0.0, y: 0.0)
    let b be Point(x: 3.0, y: 4.0)
    print(a.distance(b))
```

```output
5.0
```

The `&` in `other: &Point` means the method only looks at `other` and does not take it. [Ownership and borrowing](ownership.md) explains that properly; for now, it is enough to know that you write plain `b` at the call.

## Methods that change the value

A method that modifies its value says `mutable self`, and can then only be called on a `let mutable` value:

```science run
type Counter:
    total: Int

Counter has:
    def add(mutable self, n: Int):
        self.total be self.total + n

def main():
    let mutable c be Counter(total: 0)
    c.add(3)
    c.add(4)
    print(c.total)
```

```output
7
```

A plain `self` reads; `mutable self` writes. The method has no `-> Type` because it returns nothing.

## Associated functions

A function in a `has:` block with no `self` is called on the type itself, not on a value. The usual one is `new`, which builds a value with a sensible starting state:

```science run
type Counter:
    total: Int

Counter has:
    def new() -> Counter:
        Counter(total: 0)

    def add(mutable self, n: Int):
        self.total be self.total + n

def main():
    let mutable c be Counter.new()
    c.add(10)
    print(c.total)
```

```output
10
```

`new` is only a name that everyone uses; the language gives it no special meaning. You can have any number of such functions, for example `Counter.starting_at(5)`.

## Types with parameters

A type can leave one of its field types open, to be chosen each time it is used. The open part goes in square brackets:

```science run
type Pair[A, B]:
    first: A
    second: B

def main():
    let p be Pair(first: 1, second: "one")
    print(p.first)
    print(p.second)
```

```output
1
one
```

Here `A` became `Int` and `B` became `String`, worked out from the values you gave. You can also say it yourself: `Pair[Int, Int](first: 1, second: 2)`. [Generics](generics.md) goes further.

## Summary

- `type Name:` followed by indented `field: Type` lines declares a type.
- Build a value with `Name(field: value, ...)`: fields by name, in any order.
- `value.field` reads a field; assigning to one needs `let mutable`.
- `Name has:` holds methods. `self` reads the value, `mutable self` changes it.
- A function without `self` is called on the type: `Counter.new()`.
- `Pair[A, B]` declares a type with open parts.

Next: types whose value is one of several shapes, in [Choices and match](choices.md).
