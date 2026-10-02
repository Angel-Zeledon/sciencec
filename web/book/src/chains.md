The usual way to process a collection is a loop. For common jobs, such as keeping some items, transforming each one or adding them up, a **chain** says the same thing in one line.

## Starting a chain

Call `iterate()` on an array. That gives you a stream of its elements, which you can pass through steps, one `.step()` after another, until a final step produces a result.

```science run
def main():
    let readings be [5, 2, 8, 1]
    print(readings.iterate().sum())
    print(readings.iterate().count())
```

```output
16
4
```

`sum()` adds the elements and `count()` counts them. These are *final* steps: they use up the stream and give back a plain value.

## Transforming each item with map

`map` makes a new stream by applying something to every element. The thing you apply is a **closure**, a tiny function written inline. The simplest form uses the word `each` for the element:

```science run
def main():
    let readings be [5, 2, 8, 1]
    let doubled be readings.iterate().map(each * 2).collect()
    for value in doubled:
        print(value)
```

```output
10
4
16
2
```

`collect()` is the other common final step: it gathers the stream back into an array.

## Choosing items with keep and discard

`keep` passes only the elements for which the closure is true. `discard` is the opposite:

```science run
def main():
    let readings be [5, 2, 8, 1]
    let high be readings.iterate().keep(each > 2).collect()
    let low be readings.iterate().discard(each > 2).collect()
    print(high.length())
    print(low.length())
```

```output
2
2
```

## Fields and methods

`each` can be followed by a field or a method call. Given a type with fields, this pulls one out of every element:

```science run
type Sample:
    label: String
    hits: Int

def main():
    let samples be [Sample(label: "a", hits: 3), Sample(label: "b", hits: 10)]
    let total be samples.iterate().map(each.hits).sum()
    print(total)

    let labels be samples.iterate().map(each.label).collect()
    for label in labels:
        print(f"{label}")
```

```output
13
a
b
```

## Chaining several steps

The steps compose from left to right. When a chain gets long, put each step on its own line starting with a dot:

```science run
def main():
    let readings be [5, 2, 8, 1]
    let total_of_big_squares be readings
        .iterate()
        .keep(each > 1)
        .map(each * each)
        .sum()
    print(total_of_big_squares)
```

```output
93
```

Read it as a sentence: take the readings, keep those above 1, square each, add them up. That is `25 + 4 + 64`.

## When `each` is not enough: giving

`each` works when the element appears once in the closure. If you need it twice, or want to give it a name, write the name, then `giving`, then the expression:

```science run
def main():
    let readings be [5, 2, 8, 1]
    let in_range be readings.iterate().keep(x giving x > 1 and x < 8).collect()
    print(in_range.length())
```

```output
2
```

`x giving x > 1 and x < 8` reads as "given `x`, produce `x > 1 and x < 8`". The name is yours to choose.

## Sorting

`sorted(by: ...)` orders the stream by a number you compute from each element. Smaller numbers come first:

```science run
def main():
    let words be ["pear", "fig", "banana"]
    let by_length be words.iterate().sorted(by: w giving w.length()).collect()
    for word in by_length:
        print(f"{word}")
```

```output
fig
pear
banana
```

To sort from largest to smallest, sort by the negative: `by: n giving 0 - n`.

## Other final steps

A few more steps that finish a chain:

```science run
def main():
    let readings be [5, 2, 8, 1]
    print(readings.iterate().product())
    print(readings.iterate().has_any(x giving x > 7))
    print(readings.iterate().has_all(x giving x > 7))

    let biggest be readings.iterate().maximum(by: x giving x)
    if biggest?:
        print(biggest)

    let first_large be readings.iterate().find(x giving x > 4)
    if first_large?:
        print(first_large)
```

```output
80
true
false
8
5
```

`maximum`, `minimum`, `find` and `first` might find nothing, for instance on an empty array, so they give back a value that may be `null`. Test it with `?` before use, as you did with `get` in [Arrays, maps and sets](collections.md).

`reduce` folds the whole stream into one value, starting from a value you supply. Its closure takes two names, the running result and the next element:

```science run
def main():
    let readings be [5, 2, 8, 1]
    let sum_of_squares be readings.iterate().reduce(0, (acc, x) giving acc + x * x)
    print(sum_of_squares)
```

```output
94
```

## Trimming a stream

`take(n)` keeps the first `n` elements, `skip(n)` drops them, and `reverse()` flips the order:

```science run
def main():
    let readings be [5, 2, 8, 1]
    let middle be readings.iterate().skip(1).take(2).collect()
    for value in middle:
        print(value)
```

```output
2
8
```

`numbered()` pairs every element with its position. Each pair has an `index` (starting at 0) and the `item`:

```science run
def main():
    let names be ["Ada", "Grace"]
    for entry in names.iterate().numbered().collect():
        print(f"{entry.index}: {entry.item}")
```

```output
0: Ada
1: Grace
```

## Closures as values

A closure is a value, so you can give it a name with `let` and call it like a function:

```science run
def main():
    let add_one be x giving x + 1
    print(add_one(3))

    let offset be 10
    let shifted be x giving x + offset
    print(shifted(1))
```

```output
4
11
```

The second one uses `offset` from the code around it. A closure may read values from where it was written.

You can also hand a closure to a function that expects one. A parameter of type `(Int) -> Int` takes an `Int` and returns an `Int`:

```science run
def apply_twice(f: (Int) -> Int, n: Int) -> Int:
    f(f(n))

def main():
    let add_one be x giving x + 1
    print(apply_twice(add_one, 5))
    print(apply_twice(n giving n * 3, 2))
```

```output
7
18
```

> **Note:** A chain's closure receives each array element as a borrow (see [Ownership and borrowing](ownership.md)), so a closure stored with `let` is for calling directly or passing to functions. Inside a chain, write the closure in place.

## Summary

- `array.iterate()` starts a chain; a final step such as `sum()`, `count()` or `collect()` ends it.
- `map` transforms, `keep` and `discard` filter, `sorted(by:)` orders, `take` and `skip` trim.
- `each` names the element when it is used once; `name giving expression` when it is used more than once.
- `maximum`, `minimum`, `find` and `first` may find nothing, so test them with `?`.
- `reduce(start, (acc, x) giving ...)` folds a stream into one value.
- A closure is a value: store it with `let`, call it, or pass it to a function.

Next: how Science decides who owns a value and who may borrow it, in [Ownership and borrowing](ownership.md).
