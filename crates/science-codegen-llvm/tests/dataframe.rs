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

/// Runs `source` in a fresh `work` directory and returns what it printed.
fn prints_in_work_dir(name: &str, source: &str) -> String {
    let root = scratch("dataframe", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&root, name), OptLevel::O2);
    let work = root.join("work");
    std::fs::create_dir_all(&work).expect("a working directory");
    let output = std::process::Command::new(&built.executable)
        .current_dir(&work)
        .output()
        .expect("the program runs");
    let _ = std::fs::remove_dir_all(&root);
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stderr), "", "nothing belongs on stderr");
    String::from_utf8_lossy(&output.stdout).into_owned()
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
use ndarray (NdArray)

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

#[test]
fn grouping_and_aggregating() {
    check(
        "group",
        r#"    let df be sample()
    print(must(df.aggregate("dept", [Agg.rows(), Agg.count("age"), Agg.sum("age"), Agg.mean("score"), Agg.min("name"), Agg.max("score")])))
    print(must(df.aggregate("age", [Agg.rows(), Agg.sum("score"), Agg.min("age"), Agg.max("ok")])))
    print(must(df.aggregate("ok", [Agg.min("dept"), Agg.max("dept"), Agg.mean("age")])))
    print(must(df.aggregate("name", [Agg.sum("score"), Agg.mean("score"), Agg.min("score"), Agg.count("score")])))
    print(must(df.aggregate("dept", &Array[Agg].new())))
    refused(df.aggregate("dept", [Agg.sum("name")]))
    refused(df.aggregate("dept", [Agg.mean("ok")]))
    refused(df.aggregate("nope", [Agg.rows()]))
    refused(df.aggregate("dept", [Agg.count("nope")]))
"#,
        "dept  rows  age_count  age_sum  score_mean  name_min  score_max
----  ----  ---------  -------  ----------  --------  ---------
eng      3          3       94       75.75  ada            91.5
ops      2          1       41      79.125  bob           88.25
 age  rows  score_sum  age_min  ok_max
----  ----  ---------  -------  ------
  36     1       91.5       36  true
null     1       70.0     null  false
  29     2       60.0       29  true
  41     1      88.25       41  true
ok     dept_min  dept_max            age_mean
-----  --------  --------  ------------------
true   eng       ops       35.333333333333336
false  eng       ops                     29.0
name  score_sum  score_mean  score_min  score_count
----  ---------  ----------  ---------  -----------
ada        91.5        91.5       91.5            1
bob        70.0        70.0       70.0            1
cy          0.0        null       null            0
dee       88.25       88.25      88.25            1
eve        60.0        60.0       60.0            1
dept
----
eng
ops
refused: cannot take the name_sum of a String column
refused: cannot take the ok_mean of a Bool column
refused: no column named \"nope\"
refused: no column named \"nope\"
",
    );
}

#[test]
fn inner_join() {
    check(
        "join",
        r#"    let df be sample()
    let depts be must(DataFrame.from_csv_text("dept,boss,floor\neng,Zed,3\nops,Yan,5\nhr,Xi,1\n"))
    print(must(df.inner_join(depts, "dept")))
    let tags be must(DataFrame.from_csv_text("name,tag,score\nada,a1,1.5\nada,a2,2.5\nzed,z,3\ncy,c,\n"))
    print(must(df.inner_join(tags, "name")))
    let ages be must(DataFrame.from_csv_text("age,label\n29,young\n29,also\n41,old\n,none\n"))
    print(must(df.inner_join(ages, "age")))
    let floats be must(DataFrame.from_csv_text("age,x\n29.5,1\n"))
    refused(df.inner_join(floats, "age"))
    refused(df.inner_join(ages, "name"))
    refused(df.inner_join(depts, "nope"))
    print(must(df.inner_join(must(DataFrame.from_csv_text("dept,z\nxx,1\n")), "dept")))
"#,
        "name  dept   age  score  ok     boss  floor
----  ----  ----  -----  -----  ----  -----
ada   eng     36   91.5  true   Zed       3
bob   ops   null   70.0  false  Yan       5
cy    eng     29   null  true   Zed       3
dee   ops     41  88.25  true   Yan       5
eve   eng     29   60.0  false  Zed       3
name  dept  age  score  ok    tag  score_right
----  ----  ---  -----  ----  ---  -----------
ada   eng    36   91.5  true  a1           1.5
ada   eng    36   91.5  true  a2           2.5
cy    eng    29   null  true  c           null
name  dept  age  score  ok     label
----  ----  ---  -----  -----  -----
cy    eng    29   null  true   young
cy    eng    29   null  true   also
dee   ops    41  88.25  true   old
eve   eng    29   60.0  false  young
eve   eng    29   60.0  false  also
refused: column \"age\" is Int on the left and F64 on the right
refused: both frames need a column named \"name\"
refused: both frames need a column named \"nope\"
name  dept  age  score  ok  z
----  ----  ---  -----  --  -
",
    );
}

#[test]
fn describe_numeric_columns() {
    check(
        "describe",
        r#"    let df be sample()
    print(df.describe())
    let mutable d be DataFrame.new()
    report(d.add(Column.of_texts("t", ["a", "b", "c"])))
    report(d.add(Column.of_floats("one", [5.0, 0.0, 0.0]).masked([true, false, false])))
    report(d.add(Column.of_ints("none", [0, 0, 0]).masked([false, false, false])))
    report(d.add(Column.of_ints("ints", [1, 2, 3])))
    print(d.describe())
    print(DataFrame.new().describe().rows())
    print(must(df.select(["name", "ok"])).describe().rows())
"#,
        "column  count     mean                std   min   max
------  -----  -------  -----------------  ----  ----
age         4    33.75  5.852349955359813  29.0  41.0
score       4  77.4375  14.98940598111435  60.0  91.5
ok
ok
ok
ok
column  count  mean   std   min   max
------  -----  ----  ----  ----  ----
one         1   5.0  null   5.0   5.0
none        0  null  null  null  null
ints        3   2.0   1.0   1.0   3.0
0
0
",
    );
}

#[test]
fn to_and_from_ndarray() {
    check(
        "ndarray",
        r#"def show(pair: (NdArray, Error?)):
    let a, err be pair
    if err?:
        print(f"refused: {err.message()}")
    else:
        print(a)

def main():
    let df be sample()
    show(df.to_ndarray(["age", "score"]))
    show(df.to_ndarray(["score"]))
    show(df.to_ndarray_filling(["age", "score"], 0.0))
    show(df.to_ndarray(["name"]))
    show(df.to_ndarray(["zzz"]))
    show(df.to_ndarray_filling(["ok"], 0.0))
    let full be df.filter((d, i) giving not d.is_missing("age", i) and not d.is_missing("score", i))
    let matrix, err be full.to_ndarray(["score", "age"])
    if err?:
        panic(err.message())
    print(matrix)
    let back be must(DataFrame.from_ndarray(matrix, ["x", "y"]))
    print(back)
    refused(DataFrame.from_ndarray(matrix, ["x"]))
    refused(DataFrame.from_ndarray(NdArray.zeros(&[3]), ["x"]))
    refused(DataFrame.from_ndarray(matrix, ["x", "x"]))
"#,
        "refused: column \"age\", row 2: the cell is missing
refused: column \"score\", row 3: the cell is missing
[[36.0 91.5]
 [0.0 70.0]
 [29.0 0.0]
 [41.0 88.25]
 [29.0 60.0]]
refused: column \"name\" is String, not numeric
refused: no column named \"zzz\"
refused: column \"ok\" is Bool, not numeric
[[91.5 36.0]
 [88.25 41.0]
 [60.0 29.0]]
    x     y
-----  ----
 91.5  36.0
88.25  41.0
 60.0  29.0
refused: the array has 2 columns and 1 names were given
refused: expected a 2-D array, found 1 dimensions
refused: there is already a column named \"x\"
",
    );
}

#[test]
fn csv_text_files_and_round_trip() {
    let got = prints_in_work_dir(
        "csvfile",
        &program(
            r#"    let df be sample()
    print(df.to_csv_text())
    let mutable t be DataFrame.new()
    report(t.add(Column.of_texts("note", ["plain", "has,comma", "has \"quote\"", "two\nlines"])))
    report(t.add(Column.of_ints("n", [1, 2, 3, 4]).masked([true, false, true, true])))
    print(t.to_csv_text())
    let back be must(DataFrame.from_csv_text(t.to_csv_text()))
    print(back.rows())
    let line be back.text_at("note", 3)
    if line?:
        print(line.length())
    print(back.is_missing("n", 1))
    report(df.write_csv("out.csv"))
    let again be must(DataFrame.read_csv("out.csv"))
    print(again)
    print(again.to_csv_text() is df.to_csv_text())
    print(df.to_table().row_count())
    refused(DataFrame.read_csv("missing.csv"))
"#,
        ),
    );
    assert_eq!(got, "name,dept,age,score,ok
ada,eng,36,91.5,true
bob,ops,,70.0,false
cy,eng,29,,true
dee,ops,41,88.25,true
eve,eng,29,60.0,false

ok
ok
note,n
plain,1
\"has,comma\",
\"has \"\"quote\"\"\",3
\"two
lines\",4

4
9
true
ok
name  dept   age  score  ok
----  ----  ----  -----  -----
ada   eng     36   91.5  true
bob   ops   null   70.0  false
cy    eng     29   null  true
dee   ops     41  88.25  true
eve   eng     29   60.0  false
true
5
refused: missing.csv: not found
", "printed:\n{got}");
}
