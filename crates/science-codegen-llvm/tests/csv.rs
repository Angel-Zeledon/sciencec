//! The bundled `csv` module, **built, linked, run**.
//!
//! `csv.science` is RFC 4180 reading and writing written in Science, with no
//! runtime beyond `read_file` and `write_file`. What these tests pin is each
//! decision of its header on the input where a different decision would print
//! something else: quoted fields with delimiters, doubled quotes and embedded
//! CRLF; blank lines; a quote inside an unquoted field; strict widths; the
//! physical line a record starts on (which a multi-line field moves); the
//! writer's quote-only-when-needed rule; and a write-then-read round trip.
//!
//! The expected text is hand-derived from the RFC and the header, not copied
//! from a run. Fields are shown in `[...]`, so an empty field and a field with
//! a newline in it are visible.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("csv", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// Runs `source` in a fresh `work` directory that `setup` has filled, and
/// returns what it printed.
fn prints_with_files(name: &str, setup: impl Fn(&std::path::Path), source: &str) -> String {
    let root = scratch("csv", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&root, name), OptLevel::O2);
    let work = root.join("work");
    std::fs::create_dir_all(&work).expect("a working directory");
    setup(&work);
    let output = std::process::Command::new(&built.executable)
        .current_dir(&work)
        .output()
        .expect("the program runs");
    let _ = std::fs::remove_dir_all(&root);
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stderr), "", "nothing belongs on stderr");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// `show(text, options)` prints the header, then each record as `line: [f][f]`
/// at the line it began on, then the error if there was one.
const SHOW: &str = r#"use csv (Table, Options, Reader, Writer)

def show(text: String, options: Options):
    let table, err be Table.parse(&text, &options)
    let mutable head be ""
    for name in table.header:
        head.push_str(f"<{name}>")
    print(f"header {head}")
    for i in 0..table.row_count():
        let mutable line be ""
        for f in table.rows[i]:
            line.push_str(f"[{f.replace("\r", "<CR>")}]")
        print(f"{table.lines[i]}: {line}")
    if err?:
        print(f"error {err.message()}")
"#;

fn show(name: &str, body: &str) -> String {
    prints(name, &format!("{SHOW}\ndef main():\n{body}"))
}

/// Quoted fields hold delimiters, doubled quotes and line breaks; every
/// terminator is accepted; a field is empty where nothing is; blank lines are
/// skipped; and a record's line is where it *starts*, so the one after a
/// two-line field is three lines later.
#[test]
fn rfc_4180_in_its_awkward_corners() {
    assert_eq!(
        show(
            "rfc",
            r#"    show("a,b,c\n1,\"x,y\",3\r\n\"he said \"\"hi\"\"\",\"multi\nline\",z\n\n4,5,6", Options.new())
    show("a,b\r\n\"cr\r\nlf\",2\r\n", Options.new())
    show("a,b,c\n1,,\n,,\n", Options.new())
    show("a,b\n\"\",\"\"\"\"\n", Options.new())
    show("a,b\n1,2\r3,4\r", Options.new())
    show("a,b\n5\"6,x\"y\n", Options.new())
    show("a\n\n\n1\n", Options.new())
    show("x,y", Options.new())
    show("", Options.new())
"#
        ),
        "\
header <a><b><c>
2: [1][x,y][3]
3: [he said \"hi\"][multi
line][z]
6: [4][5][6]
header <a><b>
2: [cr<CR>
lf][2]
header <a><b><c>
2: [1][][]
3: [][][]
header <a><b>
2: [][\"]
header <a><b>
2: [1][2]
3: [3][4]
header <a><b>
2: [5\"6][x\"y]
header <a>
4: [1]
header <x><y>
header 
"
    );
}

/// The delimiter is configurable, and a quoted field still protects it; a tab
/// and a semicolon work, and an unusable delimiter is an error before any
/// record is read.
#[test]
fn delimiters() {
    assert_eq!(
        show(
            "delimiters",
            r#"    show("a;b\n1;\"2;3\"\n", Options.new().delimiter(";"))
    show("a\tb\n1,5\t2\n", Options.new().delimiter("\t"))
    show("a,b\n1,2\n", Options.new().delimiter(";"))
    show("a,b\n", Options.new().delimiter(""))
    show("a,b\n", Options.new().delimiter(",,"))
    show("a,b\n", Options.new().delimiter("\""))
    show("a,b\n", Options.new().delimiter("\n"))
"#
        ),
        "\
header <a><b>
2: [1][2;3]
header <a><b>
2: [1,5][2]
header <a,b>
2: [1,2]
header 
error the delimiter must be exactly one character
header 
error the delimiter must be exactly one character
header 
error the delimiter must be an ASCII character other than a quote, CR or LF
header 
error the delimiter must be an ASCII character other than a quote, CR or LF
"
    );
}

/// Errors carry the line they belong to: a width mismatch is at the record's
/// first line even after a multi-line field; an unclosed quote is at the line
/// it opened on; a closing quote followed by text is at the line it closes on.
#[test]
fn errors_carry_line_numbers() {
    assert_eq!(
        show(
            "errors",
            r#"    show("a,b\n1,2\n\"x\ny\",2,3\n", Options.new())
    show("a,b\n1,2\n3,\"never\nclosed\n5,6\n", Options.new())
    show("a,b\n\"x\ny\"z,2\n", Options.new())
    show("a,b\n1\n", Options.new())
    show("a,b\n1,2\n3\n4,5,6\n", Options.new().flexible())
"#
        ),
        "\
header <a><b>
2: [1][2]
error line 3: expected 2 fields, found 3
header <a><b>
2: [1][2]
error line 3: a quoted field is never closed
header <a><b>
error line 3: a closing quote must be followed by a delimiter or the end of the record
header <a><b>
error line 2: expected 2 fields, found 1
header <a><b>
2: [1][2]
3: [3]
4: [4][5][6]
"
    );
}

/// Without a header the first record is data, and the width it sets is the
/// width of the rest.
#[test]
fn no_header() {
    assert_eq!(
        show(
            "headerless",
            r#"    show("1,2\n3,4\n", Options.new().header(false))
    show("1,2\n3\n", Options.new().header(false))
"#
        ),
        "\
header 
1: [1][2]
2: [3][4]
header 
1: [1][2]
error line 2: expected 2 fields, found 1
"
    );
}

/// The `Reader` is an iterator of records, with the header read up front, and
/// says why it stopped.
#[test]
fn the_reader_iterates() {
    assert_eq!(
        prints(
            "reader",
            r#"use csv (Reader, Options)

def main():
    let mutable reader, err be Reader.from_text(&"id,name\n1,\"Smith, J\"\n2,Lee\n3,\"bad\n", &Options.new())
    if err?:
        print("open failed")
    print(f"header {reader.header.length()}: {reader.header[0]} {reader.header[1]}")
    for record in reader:
        print(f"line {reader.last_line}: {record[0]} / {record[1]}")
    print(reader.failure())
    if reader.error?:
        print(reader.error.message())
"#
        ),
        "\
header 2: id name
line 2: 1 / Smith, J
line 3: 2 / Lee
true
line 4: a quoted field is never closed
"
    );
}

/// Columns by name: `column_index`, `get`, `column`, and the errors for a
/// name or row that is not there.
#[test]
fn columns_by_name() {
    assert_eq!(
        prints(
            "columns",
            r#"use csv (Table, Options)

def main():
    let table, err be Table.parse(&"id,name,note\n1,Ann,\"a,b\"\n2,Bo,\n", &Options.new())
    if err?:
        print("bad")
    print(f"{table.row_count()} rows, {table.column_count()} columns")
    let at be table.column_index(&"name")
    let nowhere be table.column_index(&"nope")
    print(at?)
    print(nowhere?)
    let cell, e1 be table.get(0, &"note")
    print(f"[{cell}] {e1?}")
    let names, e2 be table.column(&"name")
    print(f"{names[0]} {names[1]} {e2?}")
    let none, e3 be table.column(&"nope")
    if e3?:
        print(e3.message())
    let far, e4 be table.get(5, &"id")
    if e4?:
        print(e4.message())
    let second, e5 be table.column_at(1)
    print(f"{second.length()} {e5?}")
"#
        ),
        "\
2 rows, 3 columns
true
false
[a,b] false
Ann Bo false
no column named \"nope\"
no row 6
2 false
"
    );
}

/// The typed helpers parse a column and, on the first bad field, name the
/// line, the data row, the column and the text; an empty field is an error.
#[test]
fn typed_columns() {
    assert_eq!(
        prints(
            "typed",
            r#"use csv (Table, Options)

def report(text: String, options: Options):
    let table, perr be Table.parse(&text, &options)
    if perr?:
        print("parse failed")
    let xs, e1 be table.floats(&"x")
    if e1?:
        print(e1.message())
    else:
        print(f"floats {xs.length()} first {xs[0]} last {xs[xs.length() - 1]}")
    let ns, e2 be table.ints(&"n")
    if e2?:
        print(e2.message())
    else:
        print(f"ints {ns.length()} first {ns[0]}")

def main():
    report("x,n\n1.5,2\n-0.25e1,40\n", Options.new())
    report("x,n\n1.5,2\n\n1.5,x\n", Options.new())
    report("x,n\n1.5,2\nabc,1\n7,2.5\n", Options.new())
    report("n,x\n1,\n", Options.new())
    report("p,q\n1,2\n", Options.new())
    let table, perr be Table.parse(&"1,two\n3,4\n", &Options.new().header(false))
    if perr?:
        print("parse failed")
    let ys, e3 be table.ints_at(1)
    if e3?:
        print(e3.message())
    let zs, e4 be table.floats_at(0)
    print(f"{zs.length()} {e4?}")
"#
        ),
        "\
floats 2 first 1.5 last -2.5
ints 2 first 2
floats 2 first 1.5 last 1.5
line 4: row 2, column \"n\": cannot read \"x\" as an integer
line 3: row 2, column \"x\": cannot read \"abc\" as a float
line 4: row 3, column \"n\": cannot read \"2.5\" as an integer
line 2: row 1, column \"x\": cannot read \"\" as a float
ints 1 first 1
no column named \"x\"
no column named \"n\"
line 1: row 1, column 2: cannot read \"two\" as an integer
2 false
"
    );
}

/// The writer quotes only when it must: a delimiter, a quote, CR or LF in the
/// field — and a lone empty field, which would otherwise be a blank line. A
/// plain space or empty field among others is left bare.
#[test]
fn the_writer_quotes_only_when_needed() {
    assert_eq!(
        prints(
            "writer",
            r#"use csv (Writer, Options)

def row(values: Array[String]) -> Array[String]:
    values

def main():
    let mutable w be Writer.new(&Options.new())
    let mutable a be Array[String].new()
    a.push("plain")
    a.push("with space")
    a.push("")
    a.push("com,ma")
    a.push("quo\"te")
    a.push("line\nbreak")
    a.push("cr\rhere")
    w.write_record(&a)
    let mutable lone be Array[String].new()
    lone.push("")
    w.write_record(&lone)
    let mutable one be Array[String].new()
    one.push("x")
    w.write_record(&one)
    print(w.text())
    let mutable semi be Writer.new(&Options.new().delimiter(";").crlf())
    let mutable b be Array[String].new()
    b.push("a,b")
    b.push("c;d")
    semi.write_record(&b)
    let text be semi.text()
    print(text.length())
    print(text.replace("\r", "<CR>").replace("\n", "<LF>"))
"#
        ),
        "plain,with space,,\"com,ma\",\"quo\"\"te\",\"line\nbreak\",\"cr\rhere\"\n\"\"\nx\n\n11\na,b;\"c;d\"<CR><LF>\n"
    );
}

/// Whatever the writer writes the reader reads back to the same fields — the
/// hostile ones included — and the line numbers account for the newlines the
/// fields carry.
#[test]
fn write_then_read_round_trips() {
    assert_eq!(
        prints(
            "roundtrip",
            r#"use csv (Table, Options, Writer)

def add(records: &mut Array[Array[String]], a: String, b: String, c: String):
    let mutable record be Array[String].new()
    record.push(a)
    record.push(b)
    record.push(c)
    records.push(record)

def main():
    let options be Options.new()
    let mutable w be Writer.new(&options)
    let mutable head be Array[String].new()
    head.push("one")
    head.push("two, with comma")
    head.push("th\"ree")
    w.write_record(&head)
    let mutable rows be Array[Array[String]].new()
    add(&mut rows, "", "", "")
    add(&mut rows, "a\nb", "c\r\nd", "\"")
    add(&mut rows, " lead", "trail ", ",")
    add(&mut rows, "é", "日本", "x\"\"y")
    for record in rows:
        w.write_record(record)
    let table, err be Table.parse(w.text(), &options)
    if err?:
        print(err.message())
    print(table.row_count())
    print(table.header[1])
    print(table.header[2])
    for i in 0..rows.length():
        for j in 0..3:
            if table.rows[i][j] is not rows[i][j]:
                print(f"row {i} column {j} differs")
    print(table.lines[0])
    print(table.lines[1])
    print(table.lines[2])
    print(table.lines[3])
    print("done")
"#
        ),
        "4\ntwo, with comma\nth\"ree\n2\n3\n6\n7\ndone\n"
    );
}

/// Files: `Table.read` and `Reader.open` on a file, a file with a byte order
/// mark and CRLF, `Writer.save` then `Table.read`, and the error text for a
/// file that is not there.
#[test]
fn files_in_a_directory() {
    let stdout = prints_with_files(
        "files",
        |work| {
            std::fs::write(work.join("in.csv"), "\u{feff}id,name\r\n1,\"Ann, A\"\r\n2,Bo\r\n").unwrap();
        },
        r#"use csv (Table, Reader, Writer, Options)

def main():
    let options be Options.new()
    let table, err be Table.read(&"in.csv", &options)
    if err?:
        print(err.message())
    print(table.header[0])
    print(table.rows[0][1])
    let ids, e2 be table.ints(&"id")
    print(f"{ids[0]} {ids[1]} {e2?}")

    let mutable reader, e3 be Reader.open(&"in.csv", &options)
    if e3?:
        print("open failed")
    let mutable count be 0
    for record in reader:
        count be count + 1
    print(count)

    let mutable w be Writer.new(&options)
    w.write_table(&table)
    let saved be w.save(&"out.csv")
    if saved?:
        print(saved.message())
    let back, e4 be Table.read(&"out.csv", &options)
    if e4?:
        print(e4.message())
    print(f"{back.row_count()} {back.rows[0][1]}")

    let ghost, e5 be Table.read(&"missing.csv", &options)
    if e5?:
        print(e5.message())
    let nothing, e6 be Reader.open(&"missing.csv", &options)
    if e6?:
        print("reader: not found")
"#,
    );
    assert_eq!(stdout, "id\nAnn, A\n1 2 false\n2\n2 Ann, A\nmissing.csv: not found\nreader: not found\n");
}
