Every value has a type, which says what kind of thing it is and what you can
do with it. This chapter covers the types you will use most: numbers, text
and true/false values.

## Whole numbers

A whole number without a decimal point is an `Int`:

```science run
let apples be 3
let oranges be 4
print(apples + oranges)
print(apples * oranges)
print(oranges - apples)
```

```output
7
12
1
```

`Int` is a 64-bit signed integer. You can write large numbers with
underscores to make them readable, and use `0x`, `0b` or `0o` for
hexadecimal, binary or octal:

```science run
print(1_000_000)
print(0xff)
print(0b1010)
```

```output
1000000
255
10
```

## Division and remainder

Dividing two integers gives an integer, with the fraction dropped. `%` gives
what is left over:

```science run
print(17 / 5)
print(17 % 5)
print(-17 / 5)
print(2 ** 10)
```

```output
3
2
-3
1024
```

`**` raises a number to a power. The usual order applies: `**` first, then
`*`, `/` and `%`, then `+` and `-`. Use parentheses when you want another
order.

```science run
print(1 + 2 * 3)
print((1 + 2) * 3)
```

```output
7
9
```

## Other sizes of integer

When the size matters, there are integers of fixed width. `I8`, `I16`, `I32`
and `I64` are signed; `U8`, `U16`, `U32` and `U64` are unsigned. A suffix on
a literal picks the type:

```science run
let byte be 255u8
let small be 42i32
print(byte)
print(small * 2)
```

```output
255
84
```

You can also write the type after the name: `let level: U8 be 3`. Most of the
time `Int` is the right choice.

## Decimal numbers

A number with a decimal point is an `F64`, a 64-bit floating-point number.
`F32` is the smaller version.

```science run
let radius be 2.0
let area be 3.14159 * radius * radius
print(area)
print(7.0 / 2.0)
print(1.5e3)
```

```output
12.56636
3.5
1500.0
```

A float always prints with a decimal point, so `1500.0` and `1500` are
visibly different values. [Strings and formatting](strings.md) shows how to
control the number of digits.

## Numbers do not mix on their own

Science never converts between number types silently. Adding an `Int` to an
`F64` is an error:

```science fails
let whole: Int be 3
let part: F64 be 0.5
print(whole + part)
```

```error
error[SC0525]: expected `I64`, found `F64`
```

## Converting with `as`

Say what you want with `as`:

```science run
let whole: Int be 3
let part: F64 be 0.5
print(whole as F64 + part)

let measured be 7.9
print(measured as Int)

let big: Int be 300
print(big as U8)
```

```output
3.5
7
44
```

Turning a float into an integer drops the fraction; it does not round. And
converting to a smaller integer keeps only the low bits, so `300` becomes
`44` in a `U8`, which holds 0 to 255.

## True and false

`Bool` has two values, `true` and `false`. Comparisons produce them:

```science run
let age be 20
print(age >= 18)
print(age < 18)
print(age is 20)
print(age is not 20)
```

```output
true
false
true
false
```

Equality is spelled with words: `is` and `is not`. The ordering comparisons use
symbols: `<`, `>`, `<=`, `>=`. There is no `==` or `!=`.

## Combining conditions

`and`, `or` and `not` work on `Bool`s:

```science run
let age be 20
let has_ticket be true

print(age >= 18 and has_ticket)
print(age < 12 or age > 65)
print(not has_ticket)
```

```output
true
false
false
```

`and` is evaluated before `or`, and `not` before both. Comparisons bind
tighter than any of them, so `age >= 18 and has_ticket` needs no parentheses.

A condition must be a `Bool`. Unlike in some languages, a number is never
treated as true or false, so `if count:` is an error; write `if count > 0:`
instead.

## Text

Text in double quotes is a `String`:

```science run
let first be "Ada"
let last be "Lovelace"
print(first + " " + last)
print(first.length())
print(first is "Ada")
```

```output
Ada Lovelace
3
true
```

`+` joins two strings, `.length()` counts the bytes, and `is` compares the
contents. Strings can be compared in order too: `"apple" < "banana"` is
`true`. There is much more to say about strings, and
[Strings and formatting](strings.md) says it.

## A single character

A `Char` is one character, written in single quotes:

```science run
let letter be 'x'
print(letter)
print(letter is 'x')
```

```output
x
true
```

## Nothing

Some functions do not produce a value at all. Their result is the *unit*
type, written `()`. You will meet it in [Functions](functions.md); for now
it is enough to know that `print` is one of them.

## Summary

- `Int` is the usual whole number; `I8` to `I64` and `U8` to `U64` are the
  sized ones. A suffix such as `255u8` picks one.
- `F64` is the usual decimal number; `F32` is the smaller one.
- Integer division drops the fraction; `%` is the remainder; `**` is a power.
- Number types never mix on their own. Convert with `as`.
- `Bool` is `true` or `false`. Compare with `is`, `is not`, `<`, `>`, `<=`,
  `>=`, and combine with `and`, `or`, `not`.
- `String` is text and `Char` is one character.

Next: [functions](functions.md).
