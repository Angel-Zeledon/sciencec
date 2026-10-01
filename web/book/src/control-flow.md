So far every line of a program has run once, in order. Control flow lets a
program choose between lines, or repeat them.

## Making a choice with `if`

`if` runs its body only when the condition is `true`:

```science run
let temperature be 31

if temperature > 30:
    print("hot")
```

```output
hot
```

The condition ends with a colon, and the body is indented underneath it. There
are no parentheses around the condition and no braces around the body.

## `else` and `else if`

Add `else` for the other case, and `else if` for more conditions. The first
one that is true wins:

```science run
let temperature be 18

if temperature > 30:
    print("hot")
else if temperature > 15:
    print("mild")
else:
    print("cold")
```

```output
mild
```

## `if` as a value

An `if` can produce a value, so you can use it on the right of `be`. When a
branch is short, write it on the same line:

```science run
let age be 20
let label be if age >= 18: "adult" else: "minor"
print(label)
```

```output
adult
```

Both branches must give the same type, and the `else` is required.

## Counting with `for`

`for` repeats its body once for each value in a range. `0..3` means 0, 1 and
2: it stops one short of the end.

```science run
for i in 0..3:
    print(i)
```

```output
0
1
2
```

Put `=` after the dots to include the last number:

```science run
for i in 1..=3:
    print(i)
```

```output
1
2
3
```

If the range is empty, as in `5..2`, the body never runs.

## Walking through an array

`for` also goes through the items of an array, one at a time:

```science run
let planets be ["Mercury", "Venus", "Earth"]
print(f"{planets.length()} planets:")

for planet in planets:
    print(planet)
```

```output
3 planets:
Mercury
Venus
Earth
```

Square brackets make an array. [Arrays, maps and sets](collections.md) covers
what you can do with one.

A loop is a good place to build up a result. Here a mutable variable collects
a sum:

```science run
let numbers be [3, 9, 4]
let mutable total be 0

for n in numbers:
    total be total + n

print(total)
```

```output
16
```

If you do not need the value, name it `_`:

```science run
for _ in 0..3:
    write("ha")
print("")
```

```output
hahaha
```

`write` prints without a newline at the end.

## Repeating until you stop: `loop`

`loop:` runs its body forever, until a `break`. Use it when you do not know in
advance how many times to go round:

```science run
let mutable n be 1

loop:
    if n > 100:
        break
    print(n)
    n be n * 3
```

```output
1
3
9
27
81
```

There is no `while` in Science. The condition goes inside the loop, as an
`if` followed by `break`.

## Skipping with `continue`

`continue` jumps to the next round without finishing the current one:

```science run
for i in 1..=6:
    if i % 2 is 0:
        continue
    print(i)
```

```output
1
3
5
```

`break` and `continue` work the same way in `for` and in `loop`. They apply to
the innermost loop they are inside.

## Choosing between many values with `match`

When you compare one value against several possibilities, `match` is tidier
than a ladder of `else if`:

```science run
def name_of(day: Int) -> String:
    match day:
        1: "Monday"
        2: "Tuesday"
        3: "Wednesday"
        other: f"day {other}"

def main():
    print(name_of(1))
    print(name_of(3))
    print(name_of(6))
```

```output
Monday
Wednesday
day 6
```

Each line is a pattern, a colon, and what to do. They are tried from top to
bottom. The last line, `other`, matches anything and gives it a name. If you
do not need the value, write `_` instead. A `match` must cover every
possibility, which is why that last line is needed for numbers.

Strings work the same way:

```science run
let answer be "yes"

match answer:
    "yes": print("go ahead")
    "no": print("stop")
    _: print("please answer yes or no")
```

```output
go ahead
```

`match` can do much more with the kinds of types you will define yourself; see
[Choices and match](choices.md).

## Summary

- `if`, `else if` and `else` choose between blocks. `if` can also be a value.
- `for i in 0..n:` counts from 0 to `n - 1`; `0..=n` includes `n`.
- `for item in array:` visits each item.
- `loop:` repeats until `break`; `continue` skips to the next round.
- There is no `while`: put the condition inside a `loop`.
- `match` compares a value against patterns and must cover every case.

Next: [strings and formatting](strings.md).
