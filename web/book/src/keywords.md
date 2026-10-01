A few words belong to the language, so you cannot use them as names for your own variables, functions or fields. This page lists them all, and what to write instead when you reach for one.

## Words with a meaning today

These are in use. Each one is covered in the chapter named beside it.

| Group | Words | Chapter |
|---|---|---|
| Bindings | `let` `be` `mutable` `const` | [Variables](variables.md) |
| Functions | `def` `return` `giving` `each` | [Functions](functions.md), [Chains and closures](chains.md) |
| Control flow | `if` `else` `match` `for` `in` `loop` `break` `continue` | [Control flow](control-flow.md) |
| Types | `type` `choice` `interface` `implements` `has` `any` `Self` `self` `where` `as` | [Types](types.md), [Interfaces](interfaces.md), [Generics](generics.md) |
| Values | `true` `false` `null` | [Numbers, text and truth](basic-types.md) |
| Logic | `and` `or` `not` `is` | [Control flow](control-flow.md) |
| Checking | `assert` | [Testing](testing.md) |
| Modules | `use` `public` | [Modules](modules.md) |
| Low level | `extern` `unsafe` `tool` | [Calling C](ffi.md) |

Two things are easy to miss:

- `is` is how you test equality: `a is b`, `a is not b`. There is no `==`.
- `each` is not a loop word. It names the item in a one-use closure, as in `docs.iterate().map(each.title)`.

## Words that only give a better error

`of`, `borrowed` and `mut` are not part of everyday Science. They are reserved so that code written in another style gets a message that names the right spelling. `mut` appears in one place that is valid, the type `&mut T`.

## Words kept for later

These words do nothing yet, but the language keeps them for features that are planned. Using one as a name is an error.

| Area | Words |
|---|---|
| Concurrency | `async` `await` `parallel` `spawn` `send` `receive` `supervise` `pure` |
| Agents and state | `agent` `prompt` `durable` `checkpoint` `resume` |
| Numerics | `tensor` `shape` `kernel` `equation` `model` |
| Language | `mod` `macro` `static` `move` `union` `yield` `import` `on` `with` |

## Choosing another name

The reserved words you are most likely to want are ordinary English. Here is what to write instead.

| You want | It is reserved | Write |
|---|---|---|
| a `send` function or field | `send` | `deliver`, `post`, `transmit` |
| a variable called `model` | `model` | `weights` |
| the dimensions of an array | `shape` | `dims` |
| a parameter called `tensor` | `tensor` | `values`, `input` |
| a remainder function | `mod` | `remainder` |
| a set operation | `union` | `merged` |
| a reaction `yield` | `yield` | `produced`, `efficiency` |
| a convolution `kernel` | `kernel` | `window`, `weights` |
| parameters called `with` or `on` | `with`, `on` | `using`, `target` |
| a module called `const` | `const` | `constants` |
| a field called `def` | `def` | `definition` |
| a field called `tool` | `tool` | `helper`, `utility` |

Using a reserved word as a name is refused:

```science fails
def main():
    let model be 3
    print(model)
```

Rename it, and the program is fine:

```science run
def main():
    let weights be 3
    print(weights)
```

```output
3
```

## Words that look reserved but are not

Some words are common in other languages and are free here. You can use them as names, and some are the names of things you will use all the time:

- `while`, `try`, `trait`, `function`, `returns`, `methods`, `at`, `above`, `below`, `most` and `least`.
- `grad`, `dim`, `dims`, `axis`, `device`, `dtype`, `unit` and `alias`.
- `print`, `write` and `panic` are ordinary functions, not keywords.

Even so, writing `while` or `function` where the language expects its own syntax gets a message that points at the Science form: `loop:` for `while`, `def` for `function`.

## Summary

- Keywords in use have chapters of their own; the table above says which.
- `of`, `borrowed` and `mut` are kept so that old spellings get a helpful error.
- The "later" words are refused as names now so that the features can arrive without breaking your code.
- When a name is taken, use the table above to pick another.

Back to the [start of the book](index.md).
