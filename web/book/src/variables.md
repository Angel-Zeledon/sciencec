A variable gives a name to a value, so you can use the value again later.
This chapter covers how to create one, how to change one, and how long a
name lasts.

## Naming a value

`let` creates a variable. The name comes first, then `be`, then the value:

```science run
let language be "Science"
let year be 2026
print(language)
print(year)
```

```output
Science
2026
```

Read `let language be "Science"` as "let `language` be `"Science"`". There is
no `=` in Science; `be` takes its place.

You never write the type. The compiler works it out from the value: `"Science"`
is a `String` and `2026` is an `Int`. The chapter on
[numbers, text and truth](basic-types.md) lists the types.

## Writing the type yourself

If you want the type on the page, put it after the name with a colon:

```science run
let count: Int be 10
let ratio: F64 be 0.75
print(count)
print(ratio)
```

```output
10
0.75
```

This is useful when you want a different type from the one the compiler would
pick. Without the annotation, `10` is an `Int`; with `let small: I8 be 10`
it is a one-byte integer.

## Variables do not change by default

A variable made with `let` keeps its value. Trying to give it a new one is a
mistake:

```science fails
let score be 1
score be 2
```

```error
error[SC0304]: `score` is not mutable
  = note: declare it `let mutable score`
```

This is on purpose. When you read code and see a plain `let`, you know that
name means the same thing everywhere below it.

## Variables that change

When a value really does need to change, say so with `mutable`:

```science run
let mutable score be 1
print(score)
score be 2
print(score)
score be score + 10
print(score)
```

```output
1
2
12
```

The first line creates the variable. After that, `score be 2` gives it a new
value. It is the same word, `be`, because it means the same thing: "this name
now means this value". Only the first use has `let`, because only the first
use creates the name.

A changed value must still have the same type. A `String` variable cannot
suddenly hold a number.

## Names that start fresh

You can use `let` again with a name you already have. This makes a *new*
variable that hides the old one:

```science run
let value be "42"
let value be 42
print(value + 1)
```

```output
43
```

The first `value` is a `String` and the second is an `Int`. This is called
**shadowing**. It is handy when you convert a value and have no more use for
the old form. Unlike `mutable`, shadowing can change the type.

## Where a name lives

A variable exists from its `let` to the end of the block it is in. A block is
an indented body: the inside of an `if`, a loop or a function.

```science run
let level be 1
if true:
    let level be 2
    print(level)
print(level)
```

```output
2
1
```

Inside the `if` the new `level` shadows the outer one. When the block ends,
the new `level` is gone and the outer one is visible again.

A name created inside a block cannot be used after it:

```science fails
if true:
    let inner be 5
print(inner)
```

```error
error[SC0200]: cannot find `inner` in this scope
```

## Constants

A value that never changes and is known before the program runs can be a
`const`. It sits at the top of a file, outside any function:

```science run
const SPEED_OF_LIGHT be 299_792_458

def main():
    print(SPEED_OF_LIGHT)
```

```output
299792458
```

By convention constant names are written in capitals. Underscores inside a
number, like `299_792_458`, are ignored; they are there for your eyes.

## Choosing names

Names are written in `snake_case`: lowercase words joined by underscores, as
in `max_speed`. A few words are taken by the language itself, such as `type`,
`loop` and `model`; if you pick one, the compiler tells you. The full list is
in [Keywords and reserved words](keywords.md).

## Summary

- `let name be value` creates a variable; the type is inferred.
- `let name: Type be value` writes the type out.
- A plain `let` cannot be changed. `let mutable` can, and you change it with
  `name be new_value`.
- A second `let` with the same name shadows the first and may change the type.
- A variable lasts until the end of its block.
- `const NAME be value` at the top of a file defines a constant.

Next: [numbers, text and truth](basic-types.md).
