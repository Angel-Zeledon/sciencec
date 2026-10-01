//! The bundled `dataframe` module, **built, linked, run**.
//!
//! `dataframe.science` is a table of named, typed columns over `csv` and
//! `ndarray`. Every test here compiles a Science program that uses it, runs the
//! executable, and compares the whole of what it printed to a string written
//! out below. The expected text is worked out from the data in the program, not
//! copied from the module: the means, deviations, orderings and joins in each
//! test are small enough to do by hand, and the comment above each says how.
//!
//! The shared data is `SAMPLE`, five people:
//!
//! ```text
//! name  dept  age   score  ok
//! ada   eng   36    91.5   true
//! bob   ops   -     70     false      (age missing)
//! cy    eng   29    -      true       (score missing)
//! dee   ops   41    88.25  true
//! eve   eng   29    60     false
//! ```

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("dataframe", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// The imports and helpers every program shares, then `main`'s body (indented
/// four spaces by the caller) or a whole set of definitions starting `def `.
///
/// `must` unwraps a refusal into a panic; `sample()` is `SAMPLE` read from CSV
/// text; `refused(pair)` prints `refused: <message>` for an error and `ok`
/// otherwise.
fn program(body: &str) -> String {
    let mut text = String::from(
        r#"use dataframe (DataFrame, Column, Agg)

def must(pair: (DataFrame, Error?)) -> DataFrame:
    let value, err be pair
    if err?:
        panic(err.message())
    value

def refused(pair: (DataFrame, Error?)):
    let _value, err be pair
    if err?:
        print(f"refused: {err.message()}")
    else:
        print("ok")

def report(err: Error?):
    if err?:
        print(f"refused: {err.message()}")
    else:
        print("ok")

def sample() -> DataFrame:
    let text be "name,dept,age,score,ok\nada,eng,36,91.5,true\nbob,ops,,70,false\ncy,eng,29,,true\ndee,ops,41,88.25,true\neve,eng,29,60,false\n"
    must(DataFrame.from_csv_text(text))

"#,
    );
    if !body.starts_with("def ") {
        text.push_str("def main():\n");
    }
    text.push_str(body);
    text
}

fn check(name: &str, body: &str, want: &str) {
    let got = prints(name, &program(body));
    assert_eq!(got, want, "`{name}` printed:\n{got}");
}

/// Types come from the fields: `age` parses as `Int` though one cell is empty,
/// `score` is `F64` (`70` parses as a float), `ok` is `Bool`, and an empty
/// field is a missing cell whatever the type.
#[test]
fn csv_columns_are_typed_and_empty_fields_are_missing() {
    check(
        "infer",
        r#"    let df be sample()
    print(f"{df.rows()} rows, {df.columns()} columns")
    for name in df.names():
        let kind be df.type_of(name)
        if kind?:
            print(f"{name}: {kind}")
    print(df.is_missing("age", 1))
    print(df.is_missing("age", 0))
    print(df.is_missing("score", 2))
    print(df.is_missing("nope", 0))
    print(df.is_missing("age", 9))
    let age be df.int_at("age", 0)
    if age?:
        print(age)
    let gone be df.int_at("age", 1)
    print(gone?)
    let score be df.float_at("score", 1)
    if score?:
        print(score)
    let who be df.text_at("name", 4)
    if who?:
        print(who)
    let ok be df.bool_at("ok", 1)
    if ok?:
        print(ok)
    print(df.float_at("age", 0)?)
    print(df.type_of("nope")?)
"#,
        "5 rows, 5 columns
name: String
dept: String
age: Int
score: F64
ok: Bool
true
false
true
true
true
36
false
70.0
eve
false
false
false
",
    );
}

/// Inference order is Int, F64, Bool, String; a column with no present field is
/// String; a short record leaves its trailing cells missing; no header means
/// `column_1`...
#[test]
fn inference_edge_cases() {
    check(
        "edges",
        r#"    let a be must(DataFrame.from_csv_text("i,f,b,t,empty\n1,1,true,x,\n2,2.5,false,3,\n,3,true,4,\n"))
    for name in a.names():
        let kind be a.type_of(name)
        if kind?:
            print(f"{name}: {kind}")
    print(a.is_missing("empty", 0))
    print(a)
    refused(DataFrame.from_csv_text("x,x\n1,2\n"))
    refused(DataFrame.from_csv_text("a,b\n1,\"oops\n"))
    let none be must(DataFrame.from_csv_text("a,b\n"))
    print(f"{none.rows()} {none.columns()}")
"#,
        "i: Int
f: F64
b: Bool
t: String
empty: String
true
   i    f  b      t  empty
----  ---  -----  -  -----
   1  1.0  true   x  null
   2  2.5  false  3  null
null  3.0  true   4  null
refused: there is already a column named \"x\"
refused: line 2: a quoted field is never closed
0 2
",
    );
}

/// A frame built from columns: a mask marks a missing cell, and a name that is
/// already there, a column of another length, or a mask of another length is
/// refused when it is added.
#[test]
fn building_from_columns() {
    check(
        "build",
        r#"    let mutable df be DataFrame.new()
    print(f"{df.rows()} {df.columns()}")
    report(df.add(Column.of_texts("city", ["Oslo", "Lima", "Quito"])))
    report(df.add(Column.of_floats("temp", [3.5, 19.0, 14.25]).masked([true, false, true])))
    report(df.add(Column.of_ints("pop", [700, 10000, 2800])))
    report(df.add(Column.of_bools("coastal", [true, true, false])))
    report(df.add(Column.of_ints("pop", [1, 2, 3])))
    report(df.add(Column.of_ints("short", [1, 2])))
    report(df.add(Column.of_ints("masked", [1, 2, 3]).masked([true, true])))
    print(f"{df.rows()} {df.columns()}")
    print(df)
    print(df.is_missing("temp", 1))
    let t be df.float_at("temp", 2)
    if t?:
        print(t)
"#,
        "0 0
ok
ok
ok
ok
refused: there is already a column named \"pop\"
refused: column \"short\" has 2 rows, the frame has 3
refused: column \"masked\" has 3 values and a mask of 2
3 4
city    temp    pop  coastal
-----  -----  -----  -------
Oslo     3.5    700  true
Lima    null  10000  true
Quito  14.25   2800  false
true
14.25
",
    );
}

#[test]
fn select_head_and_take() {
    check(
        "slice",
        r#"    let df be sample()
    print(must(df.select(["score", "name"])))
    refused(df.select(["name", "nope"]))
    refused(df.select(["name", "name"]))
    print(df.head(2))
    print(df.head(0).rows())
    print(df.head(99).rows())
    print(df.head(-3).rows())
    print(must(df.take([4, 0, 0])))
    refused(df.take([1, 5]))
    refused(df.take([-1]))
    print(DataFrame.new().head(3).rows())
"#,
        "score  name
-----  ----
 91.5  ada
 70.0  bob
 null  cy
88.25  dee
 60.0  eve
refused: no column named \"nope\"
refused: there is already a column named \"name\"
name  dept   age  score  ok
----  ----  ----  -----  -----
ada   eng     36   91.5  true
bob   ops   null   70.0  false
0
5
0
name  dept  age  score  ok
----  ----  ---  -----  -----
eve   eng    29   60.0  false
ada   eng    36   91.5  true
ada   eng    36   91.5  true
refused: no row 5
refused: no row -1
0
",
    );
}

#[test]
fn filtering_rows() {
    check(
        "filter",
        r#"    let df be sample()
    print(df.filter((d, i) giving d.is_missing("score", i)))
    print(df.filter((d, i) giving not d.is_missing("age", i) and i > 1).rows())
    print(must(df.filter_float("score", s giving s > 80.0)))
    print(must(df.filter_int("age", a giving a is 29)))
    print(must(df.filter_text("name", s giving s.starts_with("d"))))
    print(must(df.filter_float("score", s giving s > 1000.0)))
    refused(df.filter_float("age", s giving s > 1.0))
    refused(df.filter_int("nope", a giving a > 1))
    refused(df.filter_text("age", s giving s.is_empty()))
"#,
        "name  dept  age  score  ok
----  ----  ---  -----  ----
cy    eng    29   null  true
3
name  dept  age  score  ok
----  ----  ---  -----  ----
ada   eng    36   91.5  true
dee   ops    41  88.25  true
name  dept  age  score  ok
----  ----  ---  -----  -----
cy    eng    29   null  true
eve   eng    29   60.0  false
name  dept  age  score  ok
----  ----  ---  -----  ----
dee   ops    41  88.25  true
name  dept  age  score  ok
----  ----  ---  -----  --
refused: column \"age\" is Int, not F64
refused: no column named \"nope\"
refused: column \"age\" is Int, not String
",
    );
}

#[test]
fn sorting_is_stable_with_missing_last() {
    check(
        "sort",
        r#"    let df be sample()
    print(must(df.sort_by("name", true)))
    print(must(df.sort_by("age", false)))
    print(must(df.sort_by("age", true)))
    print(must(df.sort_by("score", false)))
    print(must(df.sort_by("ok", false)))
    print(must(df.sort_by("dept", false)))
    refused(df.sort_by("nope", false))
    refused(DataFrame.new().sort_by("a", false))
"#,
        "name  dept   age  score  ok
----  ----  ----  -----  -----
eve   eng     29   60.0  false
dee   ops     41  88.25  true
cy    eng     29   null  true
bob   ops   null   70.0  false
ada   eng     36   91.5  true
name  dept   age  score  ok
----  ----  ----  -----  -----
cy    eng     29   null  true
eve   eng     29   60.0  false
ada   eng     36   91.5  true
dee   ops     41  88.25  true
bob   ops   null   70.0  false
name  dept   age  score  ok
----  ----  ----  -----  -----
dee   ops     41  88.25  true
ada   eng     36   91.5  true
cy    eng     29   null  true
eve   eng     29   60.0  false
bob   ops   null   70.0  false
name  dept   age  score  ok
----  ----  ----  -----  -----
eve   eng     29   60.0  false
bob   ops   null   70.0  false
dee   ops     41  88.25  true
ada   eng     36   91.5  true
cy    eng     29   null  true
name  dept   age  score  ok
----  ----  ----  -----  -----
bob   ops   null   70.0  false
eve   eng     29   60.0  false
ada   eng     36   91.5  true
cy    eng     29   null  true
dee   ops     41  88.25  true
name  dept   age  score  ok
----  ----  ----  -----  -----
ada   eng     36   91.5  true
cy    eng     29   null  true
eve   eng     29   60.0  false
bob   ops   null   70.0  false
dee   ops     41  88.25  true
refused: no column named \"nope\"
refused: no column named \"a\"
",
    );
}
