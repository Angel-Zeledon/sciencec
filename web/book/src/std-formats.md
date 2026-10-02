Data comes in text formats: tables of values, nested records, encoded bytes. This chapter covers the modules that read and write them, and one that helps with ordinary text.

## CSV: tables

A CSV file is rows of values separated by commas, usually with a header row naming the columns. The `csv` module reads one into a `Table`. You give it the text and an `Options` value that says how the text is laid out:

```science run
use csv (Table, Options)

def main():
    let text be "name,score,age\nAda,9.5,36\nGrace,8.0,45\n"
    let table, err be Table.parse(text, Options.new())
    if err?:
        print(err.message())
        return
    print(table.row_count())
    print(table.column_count())
    let name, name_err be table.get(1, "name")
    if not name_err?:
        print(name)
```

```output
2
3
Grace
```

`Options.new()` means a comma between values and a header row on the first line. The header row is not counted in `row_count()`. `table.get(row, column_name)` reads one cell. Rows are numbered from 0.

A whole column can come out as text, floats or integers. The conversion fails with an error if a cell is not a number:

```science run
use csv (Table, Options)

def main():
    let text be "name,score,age\nAda,9.5,36\nGrace,8.0,45\n"
    let table, err be Table.parse(text, Options.new())
    if err?:
        return
    let scores, score_err be table.floats("score")
    if not score_err?:
        for score in scores:
            print(score)
    let ages, age_err be table.ints("age")
    if not age_err?:
        let mutable total be 0
        for age in ages:
            total be total + age
        print(total)
```

```output
9.5
8.0
81
```

Real files are rarely perfect. The reader checks that every row has as many values as the header, and reports the line of the first row that does not:

```science run
use csv (Table, Options)

def main():
    let table, err be Table.parse("a,b\n1\n", Options.new())
    if err?:
        print(err.message())
```

```output
line 2: expected 2 fields, found 1
```

Other layouts are set on the options. `Options.new().delimiter(";")` splits on semicolons, `header(false)` says there is no header row, and `flexible()` allows rows of different lengths.

To read a file instead of a string, use `Table.read(path, options)`. To write a table, build a `Writer` and give it records. It adds the quotes a value needs:

```science run
use csv (Table, Options, Writer)

def main():
    let mutable writer be Writer.new(Options.new())
    writer.write_record(&["id", "note"])
    writer.write_record(&["1", "plain"])
    writer.write_record(&["2", "has, comma"])
    write(writer.text())

    let saved be writer.save("out.csv")
    if saved?:
        print("could not save")
        return
    let table, err be Table.read("out.csv", Options.new())
    if not err?:
        print(table.row_count())
```

```output
id,note
1,plain
2,"has, comma"
2
```

## JSON: nested data

JSON describes values that nest: objects, lists, strings, numbers, booleans and null. The `json` module parses text into a `Json` value, and turns one back into text:

```science run
use json (parse, stringify)

def main():
    let text be "{\"name\": \"Ada\", \"age\": 36, \"tags\": [\"math\", \"code\"]}"
    let doc, err be parse(text)
    if err?:
        print(err.message())
        return
    print(stringify(doc))
```

```output
{"name":"Ada","age":36,"tags":["math","code"]}
```

`stringify` writes compact text. `pretty(doc, 2)` indents by two spaces a level:

```science run
use json (parse, pretty)

def main():
    let doc, err be parse("{\"name\": \"Ada\", \"tags\": [\"math\", \"code\"]}")
    if err?:
        return
    print(pretty(doc, 2))
```

```output
{
  "name": "Ada",
  "tags": [
    "math",
    "code"
  ]
}
```

To look inside a value, use `get(key)` for an object, `at(index)` for a list, or `pointer("/tags/1")` to follow a whole path. Each returns a value that may be missing. Then convert it with `as_text()`, `as_int()`, `as_f64()` or `as_bool()`, which are also optional because the value may be of a different kind:

```science run
use json (parse)

def main():
    let doc, err be parse("{\"name\": \"Ada\", \"age\": 36, \"tags\": [\"math\", \"code\"]}")
    if err?:
        return
    let name be doc.get("name")
    if name?:
        let text be name.as_text()
        if text?:
            print(text)
    let age be doc.get("age")
    if age?:
        let years be age.as_int()
        if years?:
            print(years + 1)
    let tag be doc.pointer("/tags/1")
    if tag?:
        let text be tag.as_text()
        if text?:
            print(text)
    print(doc.contains_key("email"))
```

```output
Ada
37
code
false
```

A parse error says where the problem is:

```science run
use json (parse)

def main():
    let doc, err be parse("{\"a\": }")
    if err?:
        print(err.message())
```

```output
unexpected character at line 1, column 7
```

To build a value, start from `object()` or `list()` and add to it. The variants of `Json` are `Json.Text`, `Json.Number`, `Json.Boolean` and `Json.Null`:

```science run
use json (Json, object, list, stringify)

def main():
    let mutable out be object()
    out.set("title", Json.Text("Regions"))
    out.set("hits", Json.Number(3.0))
    let mutable items be list()
    items.push(Json.Boolean(true))
    items.push(Json.Null)
    out.set("items", items)
    print(stringify(out))
```

```output
{"title":"Regions","hits":3,"items":[true,null]}
```

> **Note:** Every JSON number is an `F64`, so integers larger than 2^53 lose precision. An object keeps its members in the order they were written.

## Encoding: bytes and text

The `encoding` module turns bytes into text forms and back. Base64 and hex are the common ones for sending binary data through text. `utf8_encode` gives the bytes of a string, and `utf8_decode` reads them back, checking that they are valid:

```science run
use encoding (utf8_encode, utf8_decode, base64_encode, base64_decode, hex_encode)

def main():
    let bytes be utf8_encode("Hi!")
    print(bytes.length())
    let encoded be base64_encode(bytes)
    print(encoded)
    print(hex_encode(bytes))

    let back, err be base64_decode(encoded)
    if not err?:
        let text, text_err be utf8_decode(back)
        if not text_err?:
            print(text)
```

```output
3
SGkh
486921
Hi!
```

Decoding is strict, so bad input is an error and not a guess:

```science run
use encoding (hex_decode)

def main():
    let bytes, err be hex_decode("486")
    if err?:
        print("not valid hex")
```

```output
not valid hex
```

`hex_decode` and `base64_decode` also have `hex_encode_upper` and `base64_url_encode` partners for the variants you meet on the web. `code_points(text)` lists the Unicode code points of a string, and `utf16_encode` and `utf16_decode` handle UTF-16.

## The string module

A `String` has the methods you saw in [Strings and formatting](strings.md). The `string` module adds more, as plain functions that take the string as their first argument:

```science run
use string (to_upper, to_lower, title_case, pad_start, pad_end, center, repeat)

def main():
    print(to_upper("hello"))
    print(to_lower("ÉCOLE"))
    print(title_case("the science book"))
    print(pad_start("7", 3, "0"))
    print(pad_end("ab", 5, "."))
    print(center("hi", 6, "*"))
    print(repeat("ab", 3))
```

```output
HELLO
école
The Science Book
007
ab...
**hi**
ababab
```

Some are about the shape of text. `wrap` breaks text into lines of a given width, and `chars_count` counts characters where `length()` counts bytes:

```science run
use string (wrap, chars_count, split_whitespace, levenshtein, equals_ignore_case)

def main():
    for line in wrap("the quick brown fox jumps over the lazy dog", 15):
        print(line)
    print(chars_count("héllo"))
    print("héllo".length())
    print(split_whitespace("  a   b c ").length())
    print(levenshtein("kitten", "sitting"))
    print(equals_ignore_case("Science", "SCIENCE"))
```

```output
the quick brown
fox jumps over
the lazy dog
5
6
3
3
true
```

`levenshtein` is the edit distance: how many single-letter changes turn one word into the other. It is useful for suggesting a correction when someone mistypes a name.

> **Note:** Case changes cover Latin, Greek and Cyrillic letters one for one. `ß` stays as it is.

## Summary

- `use csv` reads and writes tables; check the error from `Table.parse` and from each column conversion.
- `use json` parses text into a `Json` value, reads it with `get`, `at` and `pointer`, and writes it with `stringify` or `pretty`.
- `use encoding` converts between bytes and base64, hex or UTF-8, and decoding reports bad input.
- `use string` adds case, padding, wrapping and distance functions that take the string as the first argument.

For the full list of what is available, see the [tour of the standard library](std.md).
