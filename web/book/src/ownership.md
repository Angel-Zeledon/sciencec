Science manages memory without a garbage collector and without you freeing anything by hand. It does that with one idea: **every value has exactly one owner**. This chapter shows what that means in practice and the few rules that follow.

Think of a lab notebook. Only one person holds it at a time. They can hand it over, and then they no longer have it. They can also let a colleague *read* it for a while, or let one colleague *write* in it, but never both at once, or the pages would not agree. Ownership, shared borrows and exclusive borrows are those three situations.

## Numbers are simply copied

Small plain values such as `Int`, `F64` and `Bool` are copied whenever you use them, so nothing special happens:

```science run
def main():
    let a be 5
    let b be a
    print(a + b)
```

```output
10
```

Both `a` and `b` hold a 5. The rest of this chapter is about values that are bigger than that: text, arrays, and your own types.

## Passing a value hands it over

When you pass such a value to a function, the function becomes its owner. The caller no longer has it:

```science fails
type Sample:
    label: String
    hits: Int

def consume(s: Sample) -> Int:
    s.hits

def main():
    let a be Sample(label: "a", hits: 1)
    print(consume(a))
    print(a.hits)
```

The compiler reports `use of a moved value`: `a` was moved into `consume`, so the last line has nothing to read. Assigning to another name does the same, so after `let u be t` for a `String`, `t` is empty.

Handing over is right when the function really takes the value for good. Otherwise, lend it.

## Lending a value: &T

A parameter written `&T` only borrows the value. The caller keeps ownership, and you do not write anything special at the call:

```science run
type Sample:
    label: String
    hits: Int

def peek(s: &Sample) -> Int:
    s.hits

def main():
    let a be Sample(label: "a", hits: 1)
    print(peek(a))
    print(peek(a))
    print(a.hits)
```

```output
1
1
1
```

`a` is still there after both calls. Any number of readers may borrow the same value at the same time. Inside the function you use the borrow as if it were the value: `s.hits` just works.

The `&` is for the signature. If you want a borrow stored in a name, write `&` there too: `let view be &a`.

## Lending for writing: &mut T

A function that must change the value takes `&mut T`. The value has to be `let mutable` in the caller:

```science run
type Sample:
    label: String
    hits: Int

def bump(s: &mut Sample):
    s.hits be s.hits + 1

def main():
    let mutable a be Sample(label: "a", hits: 1)
    bump(a)
    bump(a)
    print(a.hits)
```

```output
3
```

Again nothing special at the call. The word `mutable` in `let mutable a` is your permission for it to change.

## Readers or one writer, never both

While someone is reading a value, nobody may write to it, and while someone is writing, nobody else may read. The compiler checks this:

```science fails
type Sample:
    label: String
    hits: Int

def bump(s: &mut Sample):
    s.hits be s.hits + 1

def main():
    let mutable a be Sample(label: "a", hits: 1)
    let view be &a
    bump(a)
    print(view.hits)
```

The error says `a` is borrowed shared and then borrowed exclusively while the first borrow is still needed. A borrow lasts only until its last use, so the fix is to read `view.hits` before calling `bump`, or not to keep `view` at all.

This rule is what makes surprises such as a value changing under a function that is reading it impossible.

## Tidying up automatically

When the owner of a value goes out of scope, the value is released. For most types you never see this. If a type implements `Drop`, its `drop` method runs at that moment, which is useful for closing a file or logging:

```science run
type Sample:
    label: String

Sample implements Drop:
    def drop(mutable self):
        print(f"dropping {self.label}")

def consume(s: Sample):
    print(f"using {s.label}")

def main():
    let a be Sample(label: "a")
    let b be Sample(label: "b")
    consume(b)
    print("end of main")
```

```output
using b
dropping b
end of main
dropping a
```

`b` was handed to `consume`, whose parameter goes away when it finishes, so `b` is dropped there. `a` is still owned by `main`, so it is dropped when `main` ends. You wrote no cleanup call; the single owner makes the moment obvious.

## Which should I use?

- Only look at it: `&T`.
- Change it in place: `&mut T`.
- The function keeps it, stores it or returns it: plain `T`.

When in doubt, start with `&T`; the compiler will tell you if you need more.

## Summary

- Every value has one owner. Passing a value to a function moves it; the old name is empty afterwards.
- Numbers, booleans and other small plain values are copied instead.
- `&T` lends read access, and any number of those may exist together. `&mut T` lends write access, and excludes everyone else.
- Calls borrow automatically; you only write `&` in signatures and when binding a borrow to a name.
- A borrow lasts until its last use.
- A value is released when its owner goes out of scope; `Drop` lets a type run code then.

Next: values that may be absent, and operations that may fail, in [Missing values and errors](errors.md).
