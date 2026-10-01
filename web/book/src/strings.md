Programs spend a lot of time building, searching and printing text. This
chapter covers formatted strings first, since you will use them constantly,
and then the methods that work on a `String`.

## Putting values into text

A string with an `f` in front is a **formatted string**. Anything inside
`{…}` is evaluated and written into the text:

```science run
let name be "Ada"
let year be 1815
print(f"{name} was born in {year}")
print(f"next year: {year + 1}")
```

```output
Ada was born in 1815
next year: 1816
```

Any expression works inside the braces, including calls like
`{name.length()}`. To print a brace itself, double it:

```science run
print(f"a set is written {{1, 2, 3}}")
```

```output
a set is written {1, 2, 3}
```

## Controlling the layout

After the expression, a colon and a **format spec** say how to show it. The
most common one is the number of decimal places:

```science run
let pi be 3.14159265
print(f"{pi}")
print(f"{pi:.2f}")
print(f"{pi:.4f}")
```

```output
3.14159265
3.14
3.1416
```

`.2f` means "two digits after the point". The number is rounded, not cut off.

A number after the colon is a minimum **width**. `>` aligns to the right, `<`
to the left and `^` to the center:

```science run
let n be 42
print(f"[{n:>6}]")
print(f"[{n:<6}]")
print(f"[{n:^6}]")
```

```output
[    42]
[42    ]
[  42  ]
```

Put a character in front of the alignment to use it as the padding, or a `0`
to pad numbers with zeros:

```science run
let n be 42
print(f"[{n:*>6}]")
print(f"[{n:06}]")
print(f"[{"hi":-^8}]")
```

```output
[****42]
[000042]
[---hi---]
```

Alignment works for text as well, which is how you make a table:

```science run
let names be ["Ada", "Grace", "Alan"]
let scores be [95.5, 88.25, 91.0]

for i in 0..3:
    let name be names.get(i)
    let score be scores.get(i)
    if name? and score?:
        print(f"{name:<8}{score:>8.2f}")
```

```output
Ada        95.50
Grace      88.25
Alan       91.00
```

> **Note:** `Array.get` returns a missing value when the index is out of
> range, which is why the example checks both with `?` first.
> [Missing values and errors](errors.md) explains it.

A few more specs: `,` adds thousands separators, `x` and `b` show an integer
in hexadecimal and binary, and `e` uses scientific notation.

```science run
print(f"{1234567:,}")
print(f"{255:x}")
print(f"{10:b}")
print(f"{12345.678:e}")
```

```output
1,234,567
ff
1010
1.234568e+04
```

## Length and emptiness

`.length()` is the size of a string in **bytes**, and `.is_empty()` tells you
whether there is anything in it:

```science run
let word be "hello"
print(word.length())
print(word.is_empty())
print("".is_empty())
```

```output
5
false
true
```

For plain English letters a byte is a letter. A letter such as `é` takes two
bytes, so `"héy".length()` is `4`.

## Searching

`contains`, `starts_with` and `ends_with` answer yes or no:

```science run
let file be "report-2026.csv"
print(file.contains("2026"))
print(file.starts_with("report"))
print(file.ends_with(".txt"))
```

```output
true
true
false
```

`find` says where something starts. The answer might not exist, so it comes
back as a value you test with `?`:

```science run
let file be "report-2026.csv"
let dot be file.find(".")
if dot?:
    print(f"the dot is at byte {dot}")
```

```output
the dot is at byte 11
```

## Changing text

Strings are not modified by these methods; each returns a new string.
`trim` removes spaces and line breaks at both ends, and `replace` swaps one
piece of text for another:

```science run
let raw be "   padded   "
print(f"[{raw.trim()}]")
print("a-b-c".replace("-", "+"))
```

```output
[padded]
a+b+c
```

To build a string up in place, make it `mutable` and add to it with
`push_str`:

```science run
let mutable text be "Hello"
text.push_str(", ")
text.push_str("world")
print(text)
```

```output
Hello, world
```

You can also join two strings with `+`: `"ab" + "cd"` is `"abcd"`.

## Splitting into pieces

`split` cuts a string at every separator. Walk through the pieces with `for`:

```science run
let line be "red,green,blue"
for color in line.split(","):
    print(color)
```

```output
red
green
blue
```

`lines` does the same for line breaks, which is handy for text that came from
a file:

```science run
let poem be "one\ntwo\nthree"
for line in poem.lines():
    print(line)
```

```output
one
two
three
```

`\n` inside a string is a line break, and `\t` is a tab.

## Going character by character

`chars` walks through the characters. Unlike `length`, it counts letters:

```science run
let mutable vowels be 0
for c in "education".chars():
    if c is 'a' or c is 'e' or c is 'i' or c is 'o' or c is 'u':
        vowels be vowels + 1
print(vowels)
```

```output
5
```

## Turning text into numbers

Text such as `"42"` is not a number until you convert it. `parse_int` and
`parse_float` do that. Because the text might not be a number at all, each
returns the result together with an error, which you check with `?`:

```science run
let value, err be "42".parse_int()
if err?:
    print("not a number")
else:
    print(value + 1)

let _bad, err2 be "forty".parse_int()
print(err2?)
```

```output
43
true
```

`err?` is `true` when something went wrong. A name that starts with `_`, like
`_bad`, says you are deliberately ignoring that value.
[Missing values and errors](errors.md) explains this pattern in full.

## Summary

- `f"…{expr}…"` puts values into text; `{{` and `}}` write literal braces.
- `{x:.2f}` fixes the decimals; `{x:>8}`, `{x:<8}` and `{x:^8}` align;
  `{x:08}` pads with zeros; `{x:,}` adds separators.
- `length`, `is_empty`, `contains`, `starts_with`, `ends_with` and `find`
  inspect a string.
- `trim` and `replace` return new strings; `push_str` appends to a `mutable`
  one.
- `split`, `lines` and `chars` give you the pieces to loop over.
- `parse_int` and `parse_float` convert text to numbers and report failure.

Next: [Types](types.md), where you define your own.
