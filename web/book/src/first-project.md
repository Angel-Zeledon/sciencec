A single file is fine for an experiment. Anything bigger — more than one
file, tests, other people's code — belongs in a **package**.

## Creating a package

```shell
sciencec new greet
cd greet
```

That makes this layout:

```text
greet/
  science.toml          the package's name, version and dependencies
  src/
    main.science        where the program starts
    greeting.science    a module, used by main
  tests/
    greeting.science    a test
```

`science.toml` describes the package:

```text
[package]
name     = "greet"
version  = "0.1.0"
language = "0.1"

[dependencies]
```

## Running it

Inside the package directory, the commands take no file name:

```shell
sciencec run
```

```output
Hello, world!
```

```shell
sciencec test
```

```output
test tests/greeting.science ... ok
```

## Reading the code

`src/greeting.science` defines one function, marked `public` so other files
can use it:

```science
public def greeting() -> String:
    "Hello, world!"
```

`-> String` says what the function returns. The last expression in a body
is its result, so there is no `return` here.

`src/main.science` imports it with `use` and calls it:

```science norun
use greet.greeting (greeting)

def main():
    print(greeting())
```

`use greet.greeting (greeting)` reads as: from the module `greeting` in the
package `greet`, bring in the name `greeting`. [Modules](modules.md)
covers this properly.

`tests/greeting.science` is a program too. It checks something with
`assert`, and `sciencec test` reports whether it ran to the end:

```science norun
use greet.greeting (greeting)

def main():
    assert(greeting() is "Hello, world!", "the greeting changed")
```

Note `is`: equality in Science is written with a word, not `==`.

## Summary

- `sciencec new NAME` creates a package.
- `sciencec run` and `sciencec test` work on the package you are in.
- Code lives in `src/`, tests in `tests/`.

The next part of the book is the language itself, starting with
[variables](variables.md).
