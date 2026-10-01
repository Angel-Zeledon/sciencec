A function is a named piece of code you can run as often as you like. You
have already used one, `main`; this chapter shows how to write your own.

## Defining and calling

`def` starts a function. The name is followed by parentheses, a colon, and an
indented body:

```science run
def greet():
    print("Hello!")

def main():
    greet()
    greet()
```

```output
Hello!
Hello!
```

Calling a function always uses parentheses, even when there is nothing inside
them. A function can be defined above or below the place that calls it.

## Parameters

Parameters go between the parentheses. Each has a name and a type:

```science run
def greet(name: String):
    print(f"Hello, {name}!")

def main():
    greet("Ada")
    greet("Grace")
```

```output
Hello, Ada!
Hello, Grace!
```

Unlike a `let`, a parameter's type is always written out. That is how the
compiler checks every call: passing a number to `greet` would be refused.

Several parameters are separated by commas, and the arguments are given in the
same order:

```science run
def describe(name: String, age: Int):
    print(f"{name} is {age}")

def main():
    describe("Ada", 36)
```

```output
Ada is 36
```

## Returning a value

To give a result back, write `->` and its type after the parameters. The
**last expression** in the body is the result:

```science run
def square(n: Int) -> Int:
    n * n

def main():
    print(square(7))
    let total be square(3) + square(4)
    print(total)
```

```output
49
25
```

There is no `return` here, and no semicolon. A line that is just an
expression, as the last line of a body, becomes the value of the function.

## Returning early

Use `return` when you want to leave the function before its last line:

```science run
def absolute(n: Int) -> Int:
    if n < 0:
        return -n
    n

def main():
    print(absolute(-5))
    print(absolute(8))
```

```output
5
8
```

`return` is only necessary for leaving early. The final `n` is still the
ordinary way to produce the result.

## Choosing between values

Because `if` can be used as an expression, a function can be as short as one
`if`/`else`:

```science run
def sign(n: Int) -> String:
    if n < 0:
        "negative"
    else if n is 0:
        "zero"
    else:
        "positive"

def main():
    print(sign(-4))
    print(sign(0))
    print(sign(12))
```

```output
negative
zero
positive
```

Every branch must produce a value of the same type, and the final `else` is
required so that there is always an answer.

## Returning more than one value

A function can return several values at once, as a tuple. Write the types in
parentheses, and receive the values by listing names after `let`:

```science run
def divide(a: Int, b: Int) -> (Int, Int):
    (a / b, a % b)

def main():
    let quotient, remainder be divide(17, 5)
    print(f"17 = 5 * {quotient} + {remainder}")
```

```output
17 = 5 * 3 + 2
```

If you want to keep the pair together, bind it to a single name and take the
parts out by position:

```science run
def divide(a: Int, b: Int) -> (Int, Int):
    (a / b, a % b)

def main():
    let pair be divide(17, 5)
    print(pair.0)
    print(pair.1)
```

```output
3
2
```

## Functions that call themselves

A function may call itself, which is called **recursion**. There must be a
case where it stops:

```science run
def factorial(n: Int) -> Int:
    if n <= 1:
        return 1
    n * factorial(n - 1)

def main():
    print(factorial(5))
    print(factorial(10))
```

```output
120
3628800
```

## Mistakes the compiler catches

The compiler checks that every call matches the definition. Passing the wrong
type is refused before the program runs:

```science fails
def square(n: Int) -> Int:
    n * n

def main():
    print(square("seven"))
```

```error
error[SC0525]: expected `I64`, found `String`
```

> **Note:** Arguments are matched to parameters by position. A function call
> cannot name its arguments the way a record can; that form is for
> [types](types.md).

## Summary

- `def name(param: Type, ...) -> ReturnType:` defines a function.
- Parameter types are always written. Without `->`, a function returns
  nothing.
- The last expression is the result; `return` leaves early.
- `(A, B)` returns two values at once; receive them with `let a, b be f()`.
- A function can call itself.

Next: [control flow](control-flow.md).
