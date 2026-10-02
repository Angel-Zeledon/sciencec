Some questions have no answer: the first negative number in a list of positives, or the square root of -1. Some operations fail: a file that is not there, text that is not a number. Science handles both with small, explicit tools, and none of them is hidden control flow.

## A value that may be absent

Add `?` to a type to say "this, or nothing": `Int?` is an `Int` or `null`. A function that might not find an answer returns one:

```science run
def first_negative(values: &Array[Int]) -> Int?:
    for value in values:
        if value < 0:
            return value
    null

def main():
    let found be first_negative([3, -2, 5])
    if found?:
        print(found)
    let missing be first_negative([1, 2])
    print(missing?)
```

```output
-2
false
```

`null` is the "nothing" value. After a name, `?` is a question: *is there a value here?* It gives `true` or `false`.

## Using what you found

Inside `if found?:` the compiler knows `found` is present, so you use it as a plain `Int`, with no unwrapping:

```science run
def describe(maybe: Int?) -> String:
    if maybe?:
        f"the value is {maybe}"
    else:
        "nothing there"

def main():
    print(describe(4))
    print(describe(null))
```

```output
the value is 4
nothing there
```

You can pass a plain `4` where an `Int?` is expected. Outside the `if`, using `maybe` as a number is an error, because it might be `null`. That is the point: you cannot forget the check.

You met the same idea in `get` on [arrays and maps](collections.md).

## Failure: a value and an error

A function that can fail returns two things: the result and an error that may be `null`. The type is written `(Result, Error?)`:

```science run
type RangeError:
    value: F64

RangeError implements Error:
    def message(self) -> String:
        f"{self.value} is below zero"

def checked_sqrt(x: F64) -> (F64, RangeError?):
    if x < 0.0:
        return (0.0, RangeError(value: x))
    (x.sqrt(), null)

def main():
    let root, err be checked_sqrt(9.0)
    if err?:
        print(err.message())
    else:
        print(root)

    let bad, err2 be checked_sqrt(-4.0)
    if err2?:
        print(err2.message())
```

```output
3.0
-4.0 is below zero
```

There is a lot here, so in pieces:

- `type RangeError` is your own error. It is an ordinary type; `RangeError implements Error:` with a `message` method is all it takes to be an error. [Interfaces](interfaces.md) explains `implements`.
- `-> (F64, RangeError?)` says: a number, plus possibly a `RangeError`.
- `return (0.0, RangeError(...))` returns a pair. On failure the number is a placeholder that nobody should use.
- `(x.sqrt(), null)` is success: the result and no error.
- `let root, err be ...` takes the pair apart into two names, and `err?` asks whether something went wrong.

## Library functions fail the same way

`parse_int` turns text into a number and can fail, so it returns the same pair shape:

```science run
def main():
    let n, err be "42".parse_int()
    if not err?:
        print(n + 1)

    let m, err2 be "forty".parse_int()
    if err2?:
        print(err2.message())
```

```output
43
not a number
```

Every error has a `message()` for people to read. If you bind an error and never test it, the compiler complains. When you really want to ignore it, name it with a leading underscore, such as `_err`.

## Passing an error up

When your function calls something that can fail, it usually tests the error and, on failure, returns its own pair carrying that error. Otherwise it carries on with the value. `total` below does exactly that.

## Putting it together

```science run
type CountError:
    text: String

CountError implements Error:
    def message(self) -> String:
        f"not a count: {self.text}"

def parse_count(text: &String) -> (Int, CountError?):
    let value, err be text.parse_int()
    if err?:
        return (0, CountError(text: f"{text}"))
    (value, null)

def total(first: &String, second: &String) -> (Int, CountError?):
    let a, err_a be parse_count(first)
    if err_a?:
        return (0, err_a)
    let b, err_b be parse_count(second)
    if err_b?:
        return (0, err_b)
    (a + b, null)

def main():
    let sum, err be total("40", "2")
    if not err?:
        print(sum)

    let _sum, err2 be total("40", "two")
    if err2?:
        print(err2.message())
```

```output
42
not a count: two
```

`f"{text}"` makes an owned copy of a borrowed `String`, which the error needs to keep.

## When it cannot go on: panic and assert

Some situations are bugs, not conditions to handle. `panic` stops the program at once with a message, and `assert` stops it if a condition you expected to hold does not:

```science norun
def main():
    let readings be [3, 9, 4]
    assert(readings.length() is 3, "expected three readings")
    panic("this should never happen")
```

The program ends with `panic: this should never happen` and a non-zero exit status. `assert(condition)` alone uses the message `assertion failed`. Indexing outside an array stops the program the same way.

Use `assert` in tests and to state what you believe to be true. Use `?` and error pairs for things that can reasonably go wrong, such as bad input.

## Summary

- `T?` is a `T` or `null`. `x?` asks whether it is present; inside `if x?:` you use `x` as a plain `T`.
- A function that can fail returns `(Result, Error?)`. Take it apart with `let value, err be f()`.
- Test the error with `if err?:`, and return early to pass it up.
- Your own error is a type with `implements Error:` and a `message` method.
- `panic(message)` and `assert(condition, message)` stop the program for bugs.

Next: how types share behaviour, in [Interfaces](interfaces.md).
