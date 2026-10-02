Much of the world's numerical code is written in C: math libraries, BLAS,
HDF5. Science can call it directly, with no glue code.

The compiler cannot see inside C, so it cannot check what a C function
really does. Science makes that visible: you declare the function, and you
call it inside an `unsafe` block, which says "I am vouching for this call".

## Declaring a C function

An `unsafe extern "C"` block names a library and lists the functions you
want from it:

```science run
unsafe extern "C" library "m":
    def cos(x: CDouble) -> CDouble

def main():
    unsafe:
        let c be cos(0.0 as CDouble)
        print(c as F64)
```

```output
1.0
```

- `"C"` is the calling convention.
- `library "m"` is the library to link, here the C math library
  (`-lm`).
- Each `def` is a signature only. There is no body, because the code lives
  in the library.
- `unsafe:` opens a block in which foreign calls are made. Keep it as small
  as the call.

## C types

C's integers and floats have sizes that depend on the platform, so Science
has its own names for them: `CInt`, `CLong`, `CDouble`, `CSizeT` and the
like. Convert with `as`:

```science run
unsafe extern "C" library "c":
    def labs(x: CLong) -> CLong

def main():
    unsafe:
        let n be labs(-12 as CLong)
        print(n as Int)
```

```output
12
```

Convert back to `Int` or `F64` with `as` before you do anything else with
the result.

## Wrap it in a safe function

Callers should not have to think about `unsafe`. Put the foreign call in an
ordinary function with ordinary types, and have everyone else call that:

```science run
unsafe extern "C" library "m":
    def sqrt(x: CDouble) -> CDouble
    def pow(base: CDouble, exponent: CDouble) -> CDouble

def root(x: F64) -> F64:
    unsafe:
        sqrt(x as CDouble) as F64

def power(base: F64, exponent: F64) -> F64:
    unsafe:
        pow(base as CDouble, exponent as CDouble) as F64

def main():
    print(root(2.25))
    print(power(2.0, 10.0))
```

```output
1.5
1024.0
```

No C type appears in the signatures of `root` and `power`. This is the
pattern to follow: the `unsafe` code is short, sits in one place, and is
where you check the arguments are sensible before handing them to C.

## Passing arrays

C functions that take a pointer and a length are declared with
`ffi.Span[T]` (read only) or `ffi.MutableSpan[T]` (read and write). An
array gives you one with `.span()` or `.span_mutably()`:

```science run
unsafe extern "C" library "c":
    def memset(target: ffi.MutableSpan[U8], byte: CInt, count: CSizeT)

def clear(buf: &mut Array[U8]):
    unsafe:
        memset(buf.span_mutably(), 0 as CInt, buf.length() as CSizeT)

def main():
    let mutable buf: Array[U8] be [1 as U8, 2 as U8, 3 as U8]
    clear(buf)
    for b in buf:
        print(b)
```

```output
0
0
0
```

Only the pointer reaches C. The length stays on the Science side, which is
why the call also passes `buf.length()` as the count. A function that takes
a `Span` can only read the memory, so the compiler will not let a read-only
borrow be passed where a `MutableSpan` is wanted.

## Finding libraries

By default the block links `library "name"` the usual way. Two options
change that:

```science norun
# ask pkg-config for the right flags on this machine
unsafe extern "C" library "openblas" via pkg-config "openblas":
    def cblas_ddot(n: CInt, x: ffi.Span[F64], incx: CInt, y: ffi.Span[F64], incy: CInt) -> CDouble

# link statically
unsafe extern "C" library "mylib" kind static:
    def my_function(x: CInt) -> CInt
```

`via pkg-config` is how you get BLAS right across machines, since the
library's name and flags differ between them. A block can also say `when
available` to make the library optional, loaded the first time it is used.

Besides `def`, a block can declare constants (`const NAME be 101 as CInt`),
type aliases for C typedefs (`type BlasInt is I32`), exported globals
(`static`) and opaque unions. `examples/20_extern.science` in the Science
repository shows every form.

## Summary

- `unsafe extern "C" library "name":` declares C functions as signatures.
- Use the `C` types (`CInt`, `CDouble`, `CSizeT`, ...) and convert with
  `as`.
- Make foreign calls inside `unsafe:`, and keep the block small.
- Hide the foreign call behind an ordinary function with ordinary types.
- `ffi.Span[T]` and `ffi.MutableSpan[T]` pass arrays; get them with
  `.span()` and `.span_mutably()`.

Back to [packages](packages.md), or on to [keywords](keywords.md).
