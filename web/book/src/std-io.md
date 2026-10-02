Programs talk to the outside world: they print, read and write files, look at their command line, and watch the clock. This chapter covers each of those. The first few functions are always available; the rest are modules you bring in with `use`.

## Printing

`print` writes a value and a newline. `write` writes without the newline, so you can build a line in pieces:

```science run
def main():
    write("one ")
    write("two ")
    print("three")
```

```output
one two three
```

Anything that can be shown as text can be printed: numbers, strings, booleans. For messages that are not part of your program's result, such as warnings, use `print_error`. It writes to standard error, so it does not mix with output that another program might read:

```science run
def main():
    print_error("this goes to standard error")
    print("this goes to standard output")
```

```output
this goes to standard output
```

`write_error` is the same without the newline.

## Reading and writing a whole file

`write_file` stores a string in a file, and `read_file` brings it back. Both can fail, so both report an error beside the result, the way [Missing values and errors](errors.md) describes:

```science run
def main():
    let write_err be write_file("greeting.txt", "hello\nworld\n")
    if write_err?:
        print("could not write")

    let text, read_err be read_file("greeting.txt")
    if read_err?:
        print("could not read")
    else:
        print(text.length())
        for line in text.lines():
            print(line)
```

```output
12
hello
world
```

`write_file` has no value to give back, so it returns only the error. `read_file` returns the text and an error. Check the error before you use the text.

The error can tell you what went wrong:

```science run
def main():
    let text, err be read_file("no_such_file.txt")
    if err?:
        print(err.message())
```

```output
not found
```

The same function reports `permission denied`, `already exists`, `invalid data` and `input/output error` for the other cases.

## Files, a piece at a time

`read_file` loads everything at once. For a big file, or when you want to write as you go, use the `io` module. `File.create` opens a file for writing and `File.open` opens one for reading. Wrap a file in a `BufferedWriter` to collect small writes and send them in larger pieces:

```science run
use io (File, BufferedWriter)

def main():
    let file, err be File.create("notes.txt")
    if err?:
        print("could not create")
        return
    let mutable out be BufferedWriter.new(file, 4096)
    out.write(&"first line\n".bytes())
    out.write(&"second line\n".bytes())
    let flush_err be out.flush()
    if flush_err?:
        print("could not flush")

    let reader, open_err be File.open("notes.txt")
    if open_err?:
        print("could not open")
        return
    for line in reader.lines():
        print(line)
```

```output
first line
second line
```

`write` takes bytes, and `"text".bytes()` turns a string into them. `BufferedWriter.new(file, 4096)` holds up to 4096 bytes before it hands them to the file. `flush()` sends whatever is waiting right now and tells you if that failed; a writer that goes out of scope flushes too, but it has no way to tell you about a failure, so call `flush()` when the data matters.

`reader.lines()` gives one line at a time without loading the whole file. The newline at the end of each line is removed.

To read lines typed at the keyboard, use `Stdin.new().lines()` in the same way.

## Directories

The `fs` module works with directories and removes things. It is a separate module so that a program that deletes files says so at the top of the file:

```science norun
use fs (create_directory, list_directory, remove_file, remove_directory, rename)
```

Each of these returns an error, or a value and an error. `create_directory`, `remove_file`, `remove_directory` and `rename` return only the error. `list_directory` returns the names inside a folder, sorted, so the order is the same on every machine.

## Paths

A `Path` is a file location that knows how to be taken apart. It is safer than joining strings with `/` by hand:

```science run
use fs (create_directory, list_directory, remove_file, remove_directory)
use path (Path)

def main():
    let made be create_directory("scratch_dir")
    if made?:
        print("mkdir failed")
    let file be Path.from("scratch_dir").join("data.csv")
    print(file.text())
    let ext be file.extension()
    if ext?:
        print(ext)
    let stem be file.stem()
    if stem?:
        print(stem)
    print(file.exists())

    let wrote be write_file(file.text(), "a,b\n")
    if wrote?:
        print("write failed")
    let names, listed be list_directory("scratch_dir")
    if listed?:
        print("list failed")
    for name in names:
        print(name)
    print(file.exists())
    let size, sized be file.size()
    if sized?:
        print("size failed")
    print(size)
    print(file.with_extension("json").text())

    let gone be remove_file(file.text())
    if gone?:
        print("remove failed")
    let removed be remove_directory("scratch_dir")
    if removed?:
        print("rmdir failed")
    print(file.exists())
```

```output
scratch_dir/data.csv
csv
data
false
data.csv
true
4
scratch_dir/data.json
false
```

`extension()` and `stem()` give a string that may be missing, so you test them with `?` first. Other methods are `name()`, `parent()`, `is_absolute()`, `is_directory()` and `absolute()`.

> **Note:** The file functions take a `String`, so pass `file.text()` where a path is needed.

## The command line and environment

The `os` module gives a program its arguments and its environment variables. `args()` is an array of strings. The first item is the program's own name, and the rest are what the user typed after it. `env(name)` gives a variable's value, or `null` when it is not set:

```science run
use os (args, env)

def main():
    let arguments be args()
    print(arguments.length() >= 1)
    let value be env("SCIENCE_NO_SUCH_VARIABLE")
    if not value?:
        print("not set")
```

```output
true
not set
```

## Time

The `time` module measures and formats time. A `Duration` is a length of time:

```science run
use time (Duration)

def main():
    let span be Duration.from_milliseconds(1500)
    print(span)
    print(span.seconds())
    print(span.milliseconds())
    print(span + Duration.from_seconds(1))
    print(span < Duration.from_seconds(2))
```

```output
1.5s
1.5
1500
2.5s
true
```

Durations add and compare with the usual operators. To time how long something takes, take a `Monotonic` reading before and ask how much has passed. `sleep` pauses the program:

```science run
use time (Duration, Monotonic, sleep)

def main():
    let start be Monotonic.now()
    sleep(Duration.from_milliseconds(5))
    print(start.elapsed() >= Duration.from_milliseconds(5))
```

```output
true
```

A `Monotonic` clock never goes backwards, which makes it the right tool for measuring. For a date and time, use `Instant`, a point on the calendar counted from 1 January 1970 UTC. `now()` reads the clock, `Instant.from_unix_seconds` builds one, and `format_rfc3339` and `parse_rfc3339` convert to and from text:

```science run
use time (Duration, Instant, format_rfc3339, parse_rfc3339)

def main():
    let epoch be Instant.from_unix_seconds(0)
    print(epoch)
    let tomorrow be epoch + Duration.from_seconds(86400)
    print(format_rfc3339(tomorrow))

    let parsed, err be parse_rfc3339("2026-09-16T12:30:00Z")
    if err?:
        print(err.message())
    else:
        print(parsed.unix_seconds())
```

```output
1970-01-01T00:00:00Z
1970-01-02T00:00:00Z
1789561800
```

> **Note:** Times are always UTC. There is no local time zone support yet.

## Summary

- `print`, `write`, `print_error` and `write_error` are always available.
- `read_file` and `write_file` move a whole file; check the error each returns.
- `use io` gives `File`, `BufferedWriter` and line-by-line reading.
- `use fs` and `use path` work with directories and file locations.
- `use os` gives `args()` and `env(name)`.
- `use time` gives `Duration`, `Monotonic` for measuring, and `Instant` for dates.

Next: [Numbers and arrays](std-numeric.md).
