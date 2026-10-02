The traditional first program, and the three ways to run it.

## A script

Make a file called `hello.science`:

```science run
print("Hello, world!")
```

```output
Hello, world!
```

and run it:

```shell
sciencec hello.science
```

A file with statements at the top level is a **script**: the statements run
from top to bottom, like Python. That is the quickest way to try something.

## A program with `main`

Once a file grows, put the starting point in a function called `main`:

```science run
def main():
    let name be "Ada"
    print(f"Hello, {name}!")
```

```output
Hello, Ada!
```

`def` declares a function. The body is the indented block under it. `let
name be "Ada"` gives a name to a value, and `f"…"` is a *formatted string*:
whatever is inside `{…}` is evaluated and put into the text.

A file can have a `main` or top-level statements, not both.

## Running it like any script

Give the file a first line that names `sciencec`, and make it executable:

```text
#!/usr/bin/env sciencec
print("Hello from a script!")
```

```shell
chmod +x hello.science
./hello.science
```

Arguments after the file name reach the program, through `args()` in the
`os` module (the first one is the program's own name):

```shell
sciencec greet.science Ada Grace
```

The first run of a file compiles it, which takes a moment. Runs after that
reuse the compiled program until you change the file, so they start
immediately. Nothing is left next to your file; the compiled programs are
kept in a cache (`~/Library/Caches/science` on macOS, `~/.cache/science` on
Linux).

## Building an executable

`sciencec hello.science` compiles the file and runs it straight away. To keep the
executable instead, use `build`:

```shell
sciencec build hello.science
./hello
```

The result is an ordinary native program. It does not need `sciencec`, a
virtual machine or an interpreter to run; you can copy it to another machine
with the same operating system and run it there.

## Checking without building

`sciencec check` reads the file and reports every problem it finds, without
producing anything. It is fast, and it is what your editor will use.

```shell
sciencec check hello.science
```

When something is wrong, the compiler says where and usually how to fix it.
Science has no `=` for naming values, so this is a mistake:

```science fails
let name = "Ada"
print(name)
```

and the compiler tells you so, pointing at the `=` and offering the fix:

```error
error[SC0100]: expected `be`, found `=`
```

## Summary

- `sciencec FILE` compiles and runs, like `python FILE`.
- `sciencec build FILE` produces an executable.
- `sciencec check FILE` only looks for mistakes.
- A file is either a script (top-level statements) or a program (`def main()`).
