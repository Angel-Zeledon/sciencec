Science is built for numerical work, so the library for it is large. This chapter starts with the math you can do on a single number, then moves to complex numbers, random numbers, statistics, and finally whole arrays and linear algebra.

## Math on numbers

Floating-point numbers (`F64`) and integers (`Int`) have methods for the common operations. You call them with a dot:

```science run
def main():
    let x be 2.0
    print(x.sqrt())
    print(x.pow(10.0))
    let negative be -3.5
    print(negative.abs())
    let height be 2.7
    print(height.floor())
    print(3.0.hypot(4.0))
    print(7.5.clamp(0.0, 5.0))
```

```output
1.4142135623730951
1024.0
3.5
2.0
5.0
5.0
```

The usual functions are there: `sqrt`, `cbrt`, `exp`, `ln`, `log2`, `log10`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `sinh`, `cosh`, `tanh`, `floor`, `ceil`, `round`, `trunc`, `min`, `max` and `clamp`. `to_degrees` and `to_radians` convert angles.

Integers have `abs`, `pow`, `min`, `max`, `clamp` and `rem_euclid`. Division of two integers gives an integer, and `%` gives the remainder:

```science run
def main():
    let n be -7
    print(n.abs())
    print(2.pow(10))
    print(n.rem_euclid(3))
    print(10 / 4)
    print(10 % 4)
    print(7 as F64 / 2.0)
```

```output
7
1024
2
2
2
3.5
```

`as F64` converts an integer to a float so that the division keeps its fraction.

Floating-point arithmetic is not exact, so never test two results for equality with `is`. Use `is_close`:

```science run
def main():
    let sum be 0.1 + 0.2
    print(sum)
    print(sum is 0.3)
    print(sum.is_close(0.3))
```

```output
0.30000000000000004
false
true
```

## Complex numbers

The `complex` module has a `Complex` type with a real part `re` and an imaginary part `im`. Arithmetic uses the ordinary operators:

```science run
use complex (Complex)

def main():
    let z be Complex.new(3.0, 4.0)
    let w be Complex.new(1.0, -2.0)
    print(z.modulus())
    print(z + w)
    print(z * w)
    print(z.conjugate())
    print(z * 2.0)
    print(z.re)
```

```output
5.0
4.0+2.0i
11.0-2.0i
3.0-4.0i
6.0+8.0i
3.0
```

`modulus()` is the distance from zero. Other methods include `argument`, `exp`, `ln`, `sqrt`, `sin`, `cos`, `pow` and `from_polar`.

## Random numbers

Computers do not make truly random numbers. They make a sequence that is fixed by a starting value, so that you can repeat a result exactly. The `random` module gives you two ways to use that.

The first is a `Key`. A key is made from a seed, and the same seed always produces the same number, on every machine:

```science run
use random (Key, uniform, integer)

def main():
    print(uniform(Key.from_seed(42)))
    print(uniform(Key.from_seed(42)))
    print(uniform(Key.from_seed(43)))
    print(integer(Key.from_seed(1), 1, 7))
```

```output
0.8449312962951291
0.8449312962951291
0.5326758737214929
3
```

`uniform` gives a number from 0 up to 1. `integer(key, low, high)` gives a whole number from `low` up to but not including `high`. `normal(key, mean, standard_deviation)` gives a bell-curve number.

A key is used up by the draw: using the same key twice is refused, so you cannot accidentally reuse a number you meant to be different. When you need several independent numbers from one seed, `split` makes two new keys:

```science run
use random (Key, uniform)

def main():
    let left, right be Key.from_seed(42).split()
    print(uniform(left))
    print(uniform(right))
```

```output
0.7156521212346931
0.4892723909023393
```

`split_many(n)` makes `n` keys, and `fold_in(step)` makes a key for step number `step`, which is handy inside a loop.

Keys also shuffle:

```science run
use random (Key, shuffle, permutation)

def main():
    let mutable cards be [1, 2, 3, 4, 5]
    shuffle(Key.from_seed(7), &mut cards)
    for card in cards:
        write(f"{card} ")
    print("")
    for index in permutation(Key.from_seed(1), 5):
        write(f"{index} ")
    print("")
```

```output
4 3 5 2 1 
0 2 1 4 3 
```

The second way is a `Stream`, which keeps its own position so you do not have to split keys. Use it for things that do not need to be repeated, like a quick experiment:

```science run
use random (Stream, uniform_from, integer_from)

def main():
    let mutable stream be Stream.from_seed(5)
    let a be uniform_from(&mut stream)
    let b be integer_from(&mut stream, 0, 10)
    print(a < 1.0)
    print(b < 10)
```

```output
true
true
```

> **Note:** Neither kind is secure. Do not use them for passwords or keys that protect anything.

## Statistics

The `stats` module computes summaries of an array of `F64`. Each function returns a value that may be missing, because the mean of no numbers is not defined:

```science run
use stats (mean, variance, std, median, quantile, min, correlation)

def main():
    let xs be [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]
    let m be mean(xs)
    if m?:
        print(m)
    let v be variance(xs, 0)
    if v?:
        print(v)
    let spread be std(xs, 1)
    if spread?:
        print(spread)
    let mid be median(xs)
    if mid?:
        print(mid)
    let top be quantile(xs, 0.9)
    if top?:
        print(top)
    let lowest be min(xs)
    if lowest?:
        print(lowest)
    let ys be [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]
    let r be correlation(xs, ys)
    if r?:
        print(r)
```

```output
5.0
4.0
2.138089935299395
4.5
7.6
2.0
0.9274260335029677
```

`variance` and `std` take a second number, `ddof`. Use 0 to divide by the count and 1 to divide by the count minus one, the sample variance.

To summarize numbers one at a time, without keeping them all, use `Welford`:

```science run
use stats (Welford)

def main():
    let mutable running be Welford.new()
    for x in [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]:
        running.add(x)
    print(running.count())
    let m be running.mean()
    if m?:
        print(m)
```

```output
8
5.0
```

## Arrays of numbers

An `NdArray` is a grid of `F64` with any number of dimensions. It lives in the `ndarray` module. `from_array` takes the values and the dimensions. It checks that the count matches, so it returns an error too:

```science run
use ndarray (NdArray)

def main():
    let a, err be NdArray.from_array([1.0, 2.0, 3.0, 4.0], [2, 2])
    if err?:
        print("wrong number of values")
        return
    print(a)
    print(a.size())
    print(a.sum())
    print(a.mean())
```

```output
[[1.0 2.0]
 [3.0 4.0]]
4
10.0
2.5
```

Here `[2, 2]` means two rows and two columns. The values are stored row by row.

The operators work element by element, and a single number applies to every element:

```science run
use ndarray (NdArray)

def main():
    let a, err be NdArray.from_array([1.0, 2.0, 3.0, 4.0], [2, 2])
    if err?:
        return
    print(a * 2.0)
    print(a + a)
    print(a * a)
```

```output
[[2.0 4.0]
 [6.0 8.0]]
[[2.0 4.0]
 [6.0 8.0]]
[[1.0 4.0]
 [9.0 16.0]]
```

`a * a` multiplies matching elements. It is not the matrix product; for that, use `matmul`, shown below.

There are several ways to make an array without listing its values, and several ways to summarize one along a direction:

```science run
use ndarray (NdArray)

def main():
    print(NdArray.zeros([2, 3]))
    print(NdArray.linspace(0.0, 1.0, 5))
    let a, err be NdArray.from_array([1.0, 2.0, 3.0, 4.0], [2, 2])
    if err?:
        return
    let columns, axis_err be a.sum_axis(0)
    if not axis_err?:
        print(columns)
    let item be a.get_2d(1, 0)
    if item?:
        print(item)
    let flipped, flip_err be a.transpose()
    if not flip_err?:
        print(flipped)
```

```output
[[0.0 0.0 0.0]
 [0.0 0.0 0.0]]
[0.0 0.25 0.5 0.75 1.0]
[4.0 6.0]
3.0
[[1.0 3.0]
 [2.0 4.0]]
```

`sum_axis(0)` adds down the rows, giving one total per column. `get_2d(row, column)` reads one element and gives a value that may be missing, in case the position is outside the array. `ones`, `full` and `arange` also create arrays, and `reshape`, `min_axis`, `max_axis` and `mean_axis` do what their names say.

> **Note:** An array's dimensions are the field `a.dims`, an `Array[Int]`, so you cannot print it directly. Loop over it with `for`.

## Linear algebra

The `linalg` module works on `NdArray`s. Each function that can fail returns the result and an error. Here is a system of equations, 2x + y = 3 and x + 3y = 5:

```science run
use ndarray (NdArray)
use linalg (solve, determinant)

def main():
    let a, a_err be NdArray.from_array([2.0, 1.0, 1.0, 3.0], [2, 2])
    let b, b_err be NdArray.from_array([3.0, 5.0], [2, 1])
    if a_err? or b_err?:
        return
    let det, det_err be determinant(a)
    if not det_err?:
        print(det)
    let x, err be solve(a, b)
    if err?:
        print(err.message())
    else:
        print(x)
```

```output
5.0
[[0.8]
 [1.4]]
```

`solve(a, b)` finds the `x` that makes `a * x` equal `b`. A matrix with no solution is reported as an error rather than a wrong answer:

```science run
use ndarray (NdArray)
use linalg (solve)

def main():
    let flat, flat_err be NdArray.from_array([1.0, 2.0, 2.0, 4.0], [2, 2])
    let b, b_err be NdArray.from_array([1.0, 1.0], [2, 1])
    if flat_err? or b_err?:
        return
    let x, err be solve(flat, b)
    if err?:
        print(err.message())
```

```output
solve: the matrix is singular
```

The matrix product is `matmul(a, b)`. `linalg` also has `inverse`, `transpose`, `trace`, `identity(n)`, the decompositions `lu`, `qr` and `cholesky`, and the norms `norm_frobenius`, `norm_1`, `norm_inf` and `norm_2`.

> **Note:** These routines are written in Science and are meant to be correct and clear, not fast. Very large matrices will be slow.

## Summary

- `F64` and `Int` have math methods; use `is_close` to compare floats.
- `use complex (Complex)` gives complex numbers with the usual operators.
- `use random` gives reproducible `Key`s for repeatable results and `Stream`s for quick work. A key is used up by each draw.
- `use stats` summarizes arrays of `F64`; results that may not exist are checked with `?`.
- `use ndarray (NdArray)` is a grid of numbers with element-wise operators and axis sums.
- `use linalg` solves, inverts and decomposes matrices, and reports failure as an error.

Next: [Data formats](std-formats.md).
