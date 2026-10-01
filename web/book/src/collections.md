Most data comes in bulk: a series of measurements, a table of names and values, a collection of unique labels. Science has three built-in collections for this. An **array** is an ordered list, a **map** looks values up by key, and a **set** remembers which items are present.

## Arrays

An array literal is a comma-separated list in square brackets:

```science run
def main():
    let readings be [3, 9, 4]
    print(readings.length())
    print(readings[0])
    print(readings[2])
```

```output
3
3
4
```

`length()` is the number of elements, and `readings[i]` is the element at position `i`. Positions start at zero.

All elements share one type. `[3, 9, 4]` is an `Array[Int]`, and `["a", "b"]` is an `Array[String]`.

## Adding and removing elements

An array that changes must be declared `let mutable`. `push` adds to the end, and `pop` removes the last element:

```science run
def main():
    let mutable readings be [3, 9, 4]
    readings.push(7)
    print(readings.length())
    readings.pop()
    readings.pop()
    print(readings.length())
```

```output
4
2
```

You can also assign to a position, again on a mutable array:

```science run
def main():
    let mutable readings be [3, 9, 4]
    readings[0] be 100
    print(readings[0])
```

```output
100
```

## An empty array

An empty array has no elements to take its type from, so name the type, either on the binding or on `new`:

```science run
def main():
    let mutable names be Array[String].new()
    names.push("Ada")
    names.push("Grace")
    print(names.length())

    let nothing: Array[Int] be []
    print(nothing.is_empty())
```

```output
2
true
```

The type goes in square brackets after the collection's name.

## Going through an array

`for` visits each element in order:

```science run
def main():
    let readings be [3, 9, 4]
    let mutable total be 0
    for reading in readings:
        total be total + reading
    print(total)
```

```output
16
```

When you need the position as well, loop over a range of positions:

```science run
def main():
    let readings be [3, 9, 4]
    for i in 0..readings.length():
        print(f"reading {i} is {readings[i]}")
```

```output
reading 0 is 3
reading 1 is 9
reading 2 is 4
```

`0..n` counts from 0 up to, but not including, `n`.

## Asking for an element that may not exist

Indexing past the end stops the program with the message `index out of bounds`. When you are not sure a position exists, ask with `get`. It returns the element if it is there and `null` if it is not:

```science run
def main():
    let readings be [3, 9, 4]

    let second be readings.get(1)
    if second?:
        print(f"found {second}")

    let tenth be readings.get(10)
    if tenth?:
        print("found a tenth")
    else:
        print("there is no tenth")
```

```output
found 9
there is no tenth
```

The `?` after a name asks whether it holds a value. Inside `if second?:` you can use `second` as the number it holds. [Missing values and errors](errors.md) covers this in full.

## Maps

A map pairs keys with values. Name both types in the brackets, `Map[Key, Value]`, then `insert` and `get`:

```science run
def main():
    let mutable atomic_numbers be Map[String, Int].new()
    atomic_numbers.insert("hydrogen", 1)
    atomic_numbers.insert("helium", 2)

    let found be atomic_numbers.get("helium")
    if found?:
        print(found)

    print(atomic_numbers.contains("carbon"))
    print(atomic_numbers.length())
```

```output
2
false
2
```

Like an array's `get`, a map's `get` returns `null` when the key is absent, so you check with `?` before using it. `contains` answers yes or no.

Inserting a key that is already there replaces its value. `remove` takes a key out.

```science run
def main():
    let mutable counts be Map[String, Int].new()
    counts.insert("hits", 1)
    counts.insert("hits", 10)
    counts.insert("misses", 3)
    counts.remove("misses")

    let hits be counts.get("hits")
    if hits?:
        print(hits)
    print(counts.length())
```

```output
10
1
```

## Going through a map

A `for` over a map visits its entries in the order they were first inserted. Each entry has a `key` and a `value`:

```science run
def main():
    let mutable atomic_numbers be Map[String, Int].new()
    atomic_numbers.insert("hydrogen", 1)
    atomic_numbers.insert("helium", 2)
    atomic_numbers.insert("lithium", 3)

    for entry in atomic_numbers:
        print(f"{entry.key} has number {entry.value}")
```

```output
hydrogen has number 1
helium has number 2
lithium has number 3
```

`keys()` and `values()` give you just one side:

```science run
def main():
    let mutable stock be Map[String, Int].new()
    stock.insert("pipettes", 40)
    stock.insert("flasks", 12)

    for name in stock.keys():
        print(name)
    for amount in stock.values():
        print(amount)
```

```output
pipettes
flasks
40
12
```

## Sets

A set holds each item at most once. Adding something already present changes nothing:

```science run
def main():
    let mutable elements be Set[String].new()
    elements.insert("zinc")
    elements.insert("argon")
    elements.insert("zinc")

    print(elements.length())
    print(elements.contains("argon"))
    print(elements.contains("neon"))
```

```output
2
true
false
```

Use a set when the only question is "have I seen this?". Walk one with `for`, in insertion order, and take an item out with `remove`:

```science run
def main():
    let mutable seen be Set[Int].new()
    seen.insert(5)
    seen.insert(3)
    seen.insert(5)
    for item in seen:
        print(item)
```

```output
5
3
```

## Summary

- An array is `[a, b, c]`: one element type, positions from 0. `length()`, `push`, `pop`, `contains`, `is_empty`.
- Changing a collection needs `let mutable`.
- An empty collection needs its type: `Array[Int].new()` or `let xs: Array[Int] be []`.
- `xs[i]` stops the program if `i` is out of range; `xs.get(i)` returns `null` instead, which you test with `?`.
- `Map[K, V]` has `insert`, `get`, `contains`, `remove`, `keys()` and `values()`.
- A `for` over a map or set follows insertion order.
- `Set[T]` keeps each item once.

Next: doing work on whole collections at once, in [Chains and closures](chains.md).
