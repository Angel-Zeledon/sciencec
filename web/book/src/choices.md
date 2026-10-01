A `type` holds several values at once. A `choice` holds exactly one of several alternatives. Use it when something can be one kind of thing or another, such as a unit, a shape or the result of a parse.

## Declaring a choice

List the alternatives, called **variants**, one per line:

```science
choice Unit:
    Meters
    Feet
    Miles
```

A value of type `Unit` is always one of those three. To make one, write the variant's name:

```science run
choice Unit:
    Meters
    Feet
    Miles

def main():
    let u be Feet
    print("made a unit")
```

```output
made a unit
```

## Looking inside with match

`match` asks which variant a value is and runs the branch for it. Each branch is a pattern, a colon and what to do:

```science run
choice Unit:
    Meters
    Feet
    Miles

def symbol(unit: Unit) -> String:
    match unit:
        Meters: "m"
        Feet: "ft"
        Miles: "mi"

def main():
    print(symbol(Feet))
    print(symbol(Miles))
```

```output
ft
mi
```

`match` is an expression: it produces the value of the branch that ran, so here it is the last line of the function and becomes its result.

## Variants that carry data

A variant can hold values. List their types in parentheses:

```science
choice Shape:
    Circle(F64)
    Rect(F64, F64)
    Point
```

A `Circle` carries a radius, a `Rect` carries a width and a height, and a `Point` carries nothing. You build one by calling the variant like a function:

```science run
choice Shape:
    Circle(F64)
    Rect(F64, F64)
    Point

def area(figure: Shape) -> F64:
    match figure:
        Circle(radius): 3.0 * radius * radius
        Rect(width, height): width * height
        Point: 0.0

def main():
    print(area(Rect(3.0, 4.0)))
    print(area(Circle(2.0)))
    print(area(Point))
```

```output
12.0
12.0
0.0
```

In the pattern `Rect(width, height)`, the two names are new: they are bound to the values inside, and you can use them in that branch only.

## No case can be forgotten

A `match` has to cover every variant. If you leave one out, the program does not compile:

```science fails
choice Unit:
    Meters
    Feet
    Miles

def symbol(unit: Unit) -> String:
    match unit:
        Meters: "m"
        Feet: "ft"
```

The error names the variant you missed. This is the main reason to prefer a `choice` over a handful of numeric codes: when you add a variant later, the compiler shows you every `match` that needs a new branch.

When you do not care about the rest, `_` matches anything:

```science run
choice Unit:
    Meters
    Feet
    Miles

def is_metric(unit: Unit) -> Bool:
    match unit:
        Meters: true
        _: false

def main():
    print(is_metric(Meters))
    print(is_metric(Feet))
```

```output
true
false
```

## Matching plain values

`match` is not only for choices. It works on numbers, text, characters and `Bool`s, and a branch can list several patterns with `|`:

```science run
def size_class(n: Int) -> String:
    match n:
        0: "none"
        1 | 2 | 3: "few"
        _: "many"

def main():
    print(size_class(0))
    print(size_class(2))
    print(size_class(40))
```

```output
none
few
many
```

Branches are tried from the top, and the first that fits wins.

## A choice for results

A very common use is a function that either works or explains why not:

```science run
choice Outcome:
    Value(Int)
    Failed(String)

def halve(n: Int) -> Outcome:
    if n % 2 is 0:
        Value(n / 2)
    else:
        Failed(f"{n} is odd")

def show(outcome: Outcome):
    match outcome:
        Value(n): print(f"half is {n}")
        Failed(reason): print(reason)

def main():
    show(halve(10))
    show(halve(7))
```

```output
half is 5
7 is odd
```

When a branch is a single statement it can sit on the same line as its pattern. [Missing values and errors](errors.md) shows the form the language itself prefers for failure; this is the general tool.

## Choices with parameters

Like a `type`, a `choice` can leave its payload type open, so one definition works for any payload:

```science run
choice Outcome[T, E]:
    Ok(T)
    Err(E)

def checked_divide(a: Int, b: Int) -> Outcome[Int, String]:
    if b is 0:
        Err("division by zero")
    else:
        Ok(a / b)

def main():
    match checked_divide(84, 2):
        Ok(value): print(value)
        Err(message): print(message)
    match checked_divide(1, 0):
        Ok(value): print(value)
        Err(message): print(message)
```

```output
42
division by zero
```

`Outcome[Int, String]` is the choice with `T` set to `Int` and `E` to `String`. [Generics](generics.md) explains how the open parts work.

## Choices inside a type, and the reverse

Choices and types combine freely. A type can have a choice as a field, and a variant can carry a whole type:

```science run
choice Unit:
    Meters
    Feet

type Length:
    amount: F64
    unit: Unit

def in_meters(length: &Length) -> F64:
    match length.unit:
        Meters: length.amount
        Feet: length.amount * 0.3048

def main():
    print(in_meters(Length(amount: 10.0, unit: Feet)))
    print(in_meters(Length(amount: 2.5, unit: Meters)))
```

```output
3.048
2.5
```

## Summary

- `choice Name:` lists variants; a value is exactly one of them.
- A variant can carry values: `Rect(F64, F64)`. Build it by calling the variant.
- `match` runs the branch for the variant and binds the data inside it. It is an expression.
- A `match` must cover every variant; `_` covers all the rest.
- `match` also works on numbers, text, characters and `Bool`s, and `|` joins patterns.
- `choice Outcome[T, E]` takes parameters, just as a `type` does.

Next: [arrays, maps and sets](collections.md), which hold many values of one type.
