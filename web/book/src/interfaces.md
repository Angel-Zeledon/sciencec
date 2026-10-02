An **interface** names a set of methods. Any type that provides those methods
can be used wherever the interface is asked for.

## Defining an interface

An interface lists method signatures with no body:

```science
interface Speak:
    def speak(self) -> String
```

`self` is the value the method is called on. This interface says: a type
that is `Speak` has a method `speak` that returns a `String`.

## Implementing it

`Type implements Interface:` gives a type its methods:

```science run
type Dog:
    name: String

type Robot:
    id: Int

interface Speak:
    def speak(self) -> String

Dog implements Speak:
    def speak(self) -> String:
        f"{self.name} says woof"

Robot implements Speak:
    def speak(self) -> String:
        f"unit {self.id} says beep"

def main():
    print(Dog(name: "Rex").speak())
    print(Robot(id: 7).speak())
```

```output
Rex says woof
unit 7 says beep
```

The type comes first, then `implements`, then the interface. Two unrelated
types now share one method name.

## Default methods

An interface can give a method a body. Every implementer gets it for free,
and may replace it:

```science run
type Dog:
    name: String

type Robot:
    id: Int

interface Speak:
    def speak(self) -> String

    def shout(self) -> String:
        f"{self.speak()}!"

Dog implements Speak:
    def speak(self) -> String:
        f"{self.name} says woof"

Robot implements Speak:
    def speak(self) -> String:
        f"unit {self.id} says beep"

    def shout(self) -> String:
        "BEEP"

def main():
    print(Dog(name: "Rex").shout())
    print(Robot(id: 7).shout())
```

```output
Rex says woof!
BEEP
```

`Dog` only wrote `speak`, so its `shout` is the default, which calls
`speak`. `Robot` wrote its own `shout`.

## Methods that change the value

`mutable self` lets a method modify the value. The caller needs a `let
mutable` binding:

```science run
type Counter:
    n: Int

interface Reset:
    def reset(mutable self)

Counter implements Reset:
    def reset(mutable self):
        self.n be 0

def main():
    let mutable c be Counter(n: 5)
    c.reset()
    print(c.n)
```

```output
0
```

## Methods without an interface

For methods that belong to one type and no interface, use `has:`. A
function with no `self` is called through the type, which is how you write
constructors:

```science run
type Counter:
    n: Int

Counter has:
    def new() -> Counter:
        Counter(n: 5)

    def doubled(self) -> Int:
        self.n * 2

def main():
    let c be Counter.new()
    print(c.doubled())
```

```output
10
```

## Using an interface as a bound

A generic function can ask for "any type that is `Speak`". The `[T: Speak]`
after the name is the bound; [Generics](generics.md) covers it fully:

```science run
type Dog:
    name: String

interface Speak:
    def speak(self) -> String

Dog implements Speak:
    def speak(self) -> String:
        f"{self.name} says woof"

def announce[T: Speak](item: &T):
    print(item.speak())

def main():
    announce(Dog(name: "Rex"))
```

```output
Rex says woof
```

The compiler builds a copy of `announce` for each type it is used with, so
this costs nothing at run time.

## Mixing types: `any`

A bound fixes one type per call. To hold different types in one collection,
use `any Interface` inside a `Box`:

```science run
type Dog:
    name: String

type Robot:
    id: Int

interface Speak:
    def speak(self) -> String

Dog implements Speak:
    def speak(self) -> String:
        f"{self.name} says woof"

Robot implements Speak:
    def speak(self) -> String:
        f"unit {self.id} says beep"

def main():
    let crowd: Array[Box[any Speak]] be [Box.new(Dog(name: "A")), Box.new(Robot(id: 2))]
    for member in crowd:
        print(member.speak())
```

```output
A says woof
unit 2 says beep
```

`Box.new(value)` puts a value on the heap, and `any Speak` forgets which
type it was, remembering only that it can `speak`. The right method is
picked while the program runs.

A function can also take a borrowed `&any Speak` without a `Box`.

## Interfaces from the prelude

Some interfaces are built in. Implementing them lets your type work with the
language itself.

### Display

`print` and `f"…"` need `Display`. Its method writes into a `Formatter`:

```science run
type Point:
    x: F64
    y: F64

Point implements Display:
    def display(self, into: &mut Formatter):
        into.raw("(")
        into.number(self.x)
        into.raw(", ")
        into.number(self.y)
        into.raw(")")

def main():
    let p be Point(x: 1.0, y: 2.5)
    print(p)
    print(f"point: {p}")
```

```output
(1.0, 2.5)
point: (1.0, 2.5)
```

### Clone

`Clone` gives an explicit way to copy a value:

```science run
type Note:
    text: String

Note implements Clone:
    def clone(self) -> Note:
        Note(text: f"{self.text}")

def main():
    let a be Note(text: "hi")
    let b be a.clone()
    print(b.text)
```

```output
hi
```

`f"{self.text}"` is the way to make an owned copy of a borrowed `String`.

### Eq and Ord

`Eq` defines `is`; `Ord` defines `<`, `>`, `<=` and `>=` from one method,
`less`:

```science run
type Note:
    text: String

Note implements Eq:
    def eq(self, other: &Note) -> Bool:
        self.text is other.text

Note implements Ord:
    def less(self, other: &Note) -> Bool:
        self.text.length() < other.text.length()

def main():
    let a be Note(text: "hi")
    let b be Note(text: "longer")
    print(a < b)
    print(b < a)
    print(a is b)
```

```output
true
false
false
```

> **Note:** `is` hands its right-hand side to `eq` by value, so a value
> that is not `Copy` cannot be used again afterwards. Compare last, or
> give the type a `Copy` implementation if it is small.

## Operator overloading

The operators are interfaces too: `Add` for `+`, `Sub` for `-`, `Mul` for
`*`, `Neg` for unary `-`. Implement one and the operator works on your type:

```science run
type Vector2:
    x: F64
    y: F64

Vector2 implements Add:
    def add(self, other: Vector2) -> Vector2:
        Vector2(x: self.x + other.x, y: self.y + other.y)

Vector2 implements Mul:
    def mul(self, scale: F64) -> Vector2:
        Vector2(x: self.x * scale, y: self.y * scale)

Vector2 implements Display:
    def display(self, into: &mut Formatter):
        into.number(self.x)
        into.raw(" ")
        into.number(self.y)

def main():
    let a be Vector2(x: 1.0, y: 2.0)
    let b be Vector2(x: 0.5, y: 0.5)
    print(a + b)
    print(a * 3.0)
```

```output
1.5 2.5
3.0 6.0
```

## A missing implementation

If a type does not implement the interface, the compiler says so before the
program runs:

```science fails
type Rock:
    weight: Int

interface Speak:
    def speak(self) -> String

def announce[T: Speak](item: &T):
    print(item.speak())

def main():
    announce(Rock(weight: 3))
```

## Summary

- `interface Name:` lists methods; a method with a body is a default.
- `Type implements Interface:` provides them. Methods that belong to one
  type go in `Type has:`.
- `[T: Interface]` accepts any type that implements it; `Box[any
  Interface]` holds a mix of types.
- `Display`, `Clone`, `Eq`, `Ord` and the operator interfaces (`Add`, `Sub`,
  `Mul`, `Neg`) connect your types to `print`, `is`, `<` and `+`.

Next: [Generics](generics.md).
