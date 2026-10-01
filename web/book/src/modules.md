A **module** is a `.science` file. When a program outgrows one file, you
move code into other files and bring it back with `use`.

## Your first module

Next to `main.science`, make a file called `geometry.science`:

```science
public type Point:
    x: F64
    y: F64

public def distance(a: &Point, b: &Point) -> F64:
    let dx be a.x - b.x
    let dy be a.y - b.y
    helper(dx * dx + dy * dy)

def helper(v: F64) -> F64:
    v.sqrt()
```

The file name is the module name. Now `main.science` can use it:

```science norun
use geometry (Point, distance)

def main():
    let a be Point(x: 0.0, y: 0.0)
    let b be Point(x: 3.0, y: 4.0)
    print(distance(a, b))
```

```shell
sciencec run main.science
```

```output
5.0
```

`use geometry (Point, distance)` means: from the module `geometry`, bring in
the names `Point` and `distance`. Both files sit in the same directory, and
that directory is the root every `use` path starts from.

## What `public` does

Nothing in a module is visible to other files unless it is marked
`public`. In `geometry.science`, `Point` and `distance` are public and
`helper` is not, so it is an implementation detail that `main` cannot reach:

```shell
sciencec check bad.science
```

```error
error[SC0222]: a function `geometry.helper` is private to module `geometry`
  --> bad.science:1:15
   |
 1 | use geometry (helper)
   |               ^^^^^^ not visible from module `bad`
```

`public` goes in front of the declaration, and works on `def`, `type`,
`choice` and `interface`. Methods inside a `has:` block are marked one by
one:

```science
public type Circle:
    radius: F64

Circle has:
    public def new(radius: F64) -> Circle:
        Circle(radius: radius)

    public def area(self) -> F64:
        3.0 * self.radius * self.radius

    def secret(self) -> F64:
        self.radius
```

## Choosing names

You can list as many names as you like, and break a long list over several
lines:

```science norun
use geometry (
    Point,
    distance,
)
```

Or import the module itself, and write its name in front each time:

```science norun
use geometry

def main():
    let origin be geometry.Point(x: 0.0, y: 0.0)
    print(geometry.distance(origin, origin))
```

Listing names keeps call sites short. Importing the whole module keeps it
obvious where each name came from.

## Folders

A folder groups modules. The path in `use` follows the folders, separated
by dots. With this layout:

```text
main.science
geometry.science
shapes/
  circle.science
```

a file in `shapes/` is reached as `shapes.circle`:

```science norun
use shapes.circle (Circle)

def main():
    let c be Circle.new(2.0)
    print(c.area())
```

```output
12.0
```

A folder can also have a `mod.science` file. It is the module for the
folder itself, so `use shapes (count)` reads from `shapes/mod.science`.

## The standard library's modules

Some modules ship with the compiler. You reach them with the same `use`:

```science run
use complex (Complex)
use path (Path)

def main():
    print(Complex.new(3.0, 4.0).modulus())
    print(Path.from("results").join("run.csv").text())
```

```output
5.0
results/run.csv
```

The bundled modules are `io`, `fs`, `path`, `os`, `time`, `complex`,
`random` and `collections`. [A tour of the standard library](std.md) covers
them. Anything in the prelude, such as `String`, `Array` and `print`, needs
no `use` at all.

> **Note:** If you make a file called `path.science` beside your main file,
> it replaces the bundled `path` module. Pick other names for your own
> modules.

## Summary

- Each `.science` file is a module, named after the file.
- `use module (Name, Other)` imports names; `use module` imports the module
  and you write `module.name`.
- Only `public` items can be used from other files.
- Folders become dotted paths: `use shapes.circle (Circle)`.
- A package puts modules under `src/`; see [Packages](packages.md).
