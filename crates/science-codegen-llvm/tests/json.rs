//! The bundled `json` module, **built, linked, run**.
//!
//! Every program prints, and the expected text is pinned below. The number
//! texts were checked against Python's `json` and `repr(float)`; the error
//! positions are the module's contract (1-based line, column in characters).

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("json", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// `roundtrip` prints the compact form of a parse, or its error; `number`
/// prints the compact form of one number.
const SHARED: &str = "use json (Json, Member)
use json

def roundtrip(text: String):
    let value, err be json.parse(&text)
    if err?:
        print(f\"error: {err.message()}\")
    else:
        print(json.stringify(&value))

def kind(v: (&Json)?) -> String:
    if not v?:
        return \"absent\"
    kind_of(v)

def kind_of(v: &Json) -> String:
    match v:
        Json.Null: \"null\"
        Json.Boolean(b): f\"bool {b}\"
        Json.Number(x): f\"number {x}\"
        Json.Text(t): f\"text {t}\"
        Json.List(items): f\"list {items.length()}\"
        Json.Object(members): f\"object {members.length()}\"

def as_text(v: (&Json)?) -> String:
    if v?:
        let t be v.as_text()
        if t?:
            return t
    \"none\"

def as_f64(v: (&Json)?) -> String:
    if v?:
        let t be v.as_f64()
        if t?:
            return f\"{t}\"
    \"none\"

def as_int(v: (&Json)?) -> String:
    if v?:
        let t be v.as_int()
        if t?:
            return f\"{t}\"
    \"none\"

def as_bool(v: (&Json)?) -> String:
    if v?:
        let t be v.as_bool()
        if t?:
            return f\"{t}\"
    \"none\"

def opt_text(t: String?) -> String:
    if t?:
        return t
    \"none\"

def length(v: (&Json)?) -> Int:
    if v?:
        return v.length()
    -1

def parse_ok(text: String) -> Json:
    let value, err be json.parse(&text)
    if err?:
        panic(f\"fixture: {err.message()}\")
    value
";

fn program(body: &str) -> String {
    format!("{SHARED}\ndef main():\n{body}")
}

#[test]
fn documents_round_trip_compact_and_pretty() {
    let out = prints(
        "documents",
        &program(
            "    roundtrip(\"null\")
    roundtrip(\" true \")
    roundtrip(\"[]\")
    roundtrip(\"{}\")
    roundtrip(\"[ 1 , 2 ,3 ]\")
    roundtrip(\"{\\\"b\\\": 1, \\\"a\\\": [true, false, null], \\\"c\\\": {\\\"d\\\": \\\"e\\\"}}\")
    roundtrip(\"\\\"plain\\\"\")
    roundtrip(\"[[[]]]\")
    roundtrip(\"\\t\\r\\n[1]\\r\\n\")
    let doc be parse_ok(\"{\\\"name\\\":\\\"x\\\",\\\"list\\\":[1,[2,3],{}],\\\"empty\\\":[],\\\"o\\\":{\\\"k\\\":null}}\")
    print(json.pretty(&doc, 2))
    print(json.pretty(&doc, 0))
    let again be parse_ok(json.pretty(&doc, 4))
    print(again is doc)
    print(doc)
",
        ),
    );
    assert_eq!(
        out,
        "null\ntrue\n[]\n{}\n[1,2,3]\n{\"b\":1,\"a\":[true,false,null],\"c\":{\"d\":\"e\"}}\n\"plain\"\n[[[]]]\n[1]\n\
         {\n  \"name\": \"x\",\n  \"list\": [\n    1,\n    [\n      2,\n      3\n    ],\n    {}\n  ],\n  \"empty\": [],\n  \"o\": {\n    \"k\": null\n  }\n}\n\
         {\n\"name\": \"x\",\n\"list\": [\n1,\n[\n2,\n3\n],\n{}\n],\n\"empty\": [],\n\"o\": {\n\"k\": null\n}\n}\n\
         true\n\
         {\"name\":\"x\",\"list\":[1,[2,3],{}],\"empty\":[],\"o\":{\"k\":null}}\n"
    );
}

#[test]
fn numbers_follow_the_documented_model() {
    let out = prints(
        "numbers",
        &program(
            "    roundtrip(\"1\")
    roundtrip(\"1.0\")
    roundtrip(\"1e0\")
    roundtrip(\"-0\")
    roundtrip(\"-0.0\")
    roundtrip(\"0\")
    roundtrip(\"1E2\")
    roundtrip(\"1e+2\")
    roundtrip(\"0.1\")
    roundtrip(\"-12.5\")
    roundtrip(\"1.5e-7\")
    roundtrip(\"123456789.125\")
    roundtrip(\"9007199254740993\")
    roundtrip(\"100000000000000000000\")
    roundtrip(\"1e21\")
    roundtrip(\"1e15\")
    roundtrip(\"999999999999999\")
    roundtrip(\"5e-324\")
    roundtrip(\"1.7976931348623157e308\")
    roundtrip(\"1e999\")
    roundtrip(\"-1e999\")
    roundtrip(\"1e-999\")
    roundtrip(\"[0.30000000000000004, 3.141592653589793]\")
    let v be parse_ok(\"[2, 2.5, 9007199254740993, 1e300, -3, \\\"x\\\"]\")
    for i in 0..7:
        print(f\"{as_f64(v.at(i))} {as_int(v.at(i))}\")
    let mutable big be Array[Json].new()
    big.push(Json.Number(0.0 / 0.0))
    big.push(Json.Number(1.0 / 0.0))
    big.push(Json.Number(-1.0 / 0.0))
    big.push(Json.Number(7.0))
    print(json.stringify(&Json.List(big)))
",
        ),
    );
    assert_eq!(
        out,
        "1\n1\n1\n-0\n-0\n0\n100\n100\n0.1\n-12.5\n1.5e-7\n123456789.125\n9007199254740992\n1e20\n1e21\n\
         1000000000000000\n999999999999999\n5e-324\n1.7976931348623157e308\n\
         error: number out of range at line 1, column 1\n\
         error: number out of range at line 1, column 1\n0\n\
         [0.30000000000000004,3.141592653589793]\n\
         2.0 2\n2.5 none\n9007199254740992.0 none\n1e300 none\n-3.0 -3\nnone none\nnone none\n\
         [null,null,null,7]\n"
    );
}

#[test]
fn strings_escape_and_unescape() {
    let out = prints(
        "strings",
        &program(
            "    roundtrip(\"\\\"a\\\\nb\\\\t\\\\\\\"q\\\\\\\\ \\\\/ \\\\b\\\\f\\\\r\\\"\")
    roundtrip(\"\\\"\\\\u0041\\\\u00e9\\\\u20ac\\\"\")
    roundtrip(\"\\\"\\\\ud83d\\\\ude00\\\"\")
    roundtrip(\"\\\"h\\u{e9}llo \\u{20ac} \\u{1f600}\\\"\")
    roundtrip(\"\\\"\\\\u0001\\\\u001f\\\"\")
    roundtrip(\"\\\"\\\"\")
    let v be parse_ok(\"\\\"\\\\ud83d\\\\ude00!\\\"\")
    print(opt_text(v.as_text()))
    let w be parse_ok(\"\\\"\\\\u00e9\\\"\")
    print(opt_text(w.as_text()).length())
    roundtrip(\"\\\"abc\")
    roundtrip(\"\\\"\\\\x\\\"\")
    roundtrip(\"\\\"\\\\u12\\\"\")
    roundtrip(\"\\\"\\\\ud83d\\\"\")
    roundtrip(\"\\\"\\\\ud83dx\\\"\")
    roundtrip(\"\\\"\\\\ude00\\\"\")
    roundtrip(\"\\\"a\\nb\\\"\")
",
        ),
    );
    assert_eq!(
        out,
        "\"a\\nb\\t\\\"q\\\\ / \\b\\f\\r\"\n\"A\u{e9}\u{20ac}\"\n\"\u{1f600}\"\n\"h\u{e9}llo \u{20ac} \u{1f600}\"\n\
         \"\\u0001\\u001f\"\n\"\"\n\u{1f600}!\n2\n\
         error: unterminated string at line 1, column 1\n\
         error: unknown escape at line 1, column 2\n\
         error: a \\u escape needs four hex digits at line 1, column 2\n\
         error: a high surrogate escape must be followed by a low one at line 1, column 2\n\
         error: a high surrogate escape must be followed by a low one at line 1, column 2\n\
         error: a low surrogate escape without a high one at line 1, column 2\n\
         error: a control character in a string must be escaped at line 2, column 1\n"
            .replace("line 2, column 1", "line 1, column 3")
    );
}

#[test]
fn errors_name_the_line_and_column() {
    let out = prints(
        "errors",
        &program(
            "    roundtrip(\"\")
    roundtrip(\"   \")
    roundtrip(\"[1, 2,]\")
    roundtrip(\"[1 2]\")
    roundtrip(\"{\\\"a\\\" 1}\")
    roundtrip(\"{\\\"a\\\": 1,}\")
    roundtrip(\"{a: 1}\")
    roundtrip(\"{'a': 1}\")
    roundtrip(\"[1, 2\")
    roundtrip(\"{\\\"a\\\": 1\")
    roundtrip(\"nul\")
    roundtrip(\"nulll\")
    roundtrip(\"True\")
    roundtrip(\"01\")
    roundtrip(\"+1\")
    roundtrip(\".5\")
    roundtrip(\"5.\")
    roundtrip(\"1e\")
    roundtrip(\"1e+\")
    roundtrip(\"-\")
    roundtrip(\"NaN\")
    roundtrip(\"[1] [2]\")
    roundtrip(\"{\\n  \\\"a\\\": [1,\\n    oops]\\n}\")
    roundtrip(\"[\\\"\\u{e9}\\u{e9}\\\", x]\")
    roundtrip(\"// c\\n1\")
",
        ),
    );
    assert_eq!(
        out,
        "error: unexpected end of input at line 1, column 1\n\
         error: unexpected end of input at line 1, column 4\n\
         error: unexpected character at line 1, column 7\n\
         error: expected ',' or ']' at line 1, column 4\n\
         error: expected ':' after a key at line 1, column 6\n\
         error: expected a string key at line 1, column 9\n\
         error: expected a string key at line 1, column 2\n\
         error: expected a string key at line 1, column 2\n\
         error: unexpected end of input in an array at line 1, column 6\n\
         error: unexpected end of input in an object at line 1, column 8\n\
         error: invalid literal at line 1, column 1\n\
         error: unexpected characters after the value at line 1, column 5\n\
         error: unexpected character at line 1, column 1\n\
         error: a number may not have a leading zero at line 1, column 2\n\
         error: unexpected character at line 1, column 1\n\
         error: unexpected character at line 1, column 1\n\
         error: a fraction needs digits at line 1, column 3\n\
         error: an exponent needs digits at line 1, column 3\n\
         error: an exponent needs digits at line 1, column 4\n\
         error: invalid number at line 1, column 2\n\
         error: unexpected character at line 1, column 1\n\
         error: unexpected characters after the value at line 1, column 5\n\
         error: unexpected character at line 3, column 5\n\
         error: unexpected character at line 1, column 8\n\
         error: unexpected character at line 1, column 1\n"
    );
}

#[test]
fn accessors_find_and_change_values() {
    let out = prints(
        "accessors",
        &program(
            "    let doc be parse_ok(\"{\\\"name\\\": \\\"sc\\\", \\\"n\\\": 3, \\\"ok\\\": true, \\\"none\\\": null, \\\"tags\\\": [\\\"a\\\", \\\"b\\\"], \\\"k\\\": 1, \\\"k\\\": 2, \\\"a/b\\\": {\\\"~\\\": [10, 20]}}\")
    print(as_text(doc.get(\"name\")))
    print(as_f64(doc.get(\"n\")))
    print(as_int(doc.get(\"n\")))
    print(as_bool(doc.get(\"ok\")))
    print(as_bool(doc.get(\"name\")))
    print(kind(doc.get(\"none\")))
    print(doc.get(\"missing\")?)
    print(doc.contains_key(\"none\"))
    print(doc.contains_key(\"nope\"))
    print(as_int(doc.get(\"k\")))
    print(doc.length())
    print(length(doc.get(\"tags\")))
    print(as_text(doc.pointer(\"/tags/1\")))
    print(doc.pointer(\"/tags/2\")?)
    print(doc.at(0)?)
    for key in doc.keys():
        write(f\"{key} \")
    print(\"\")
    print(as_text(doc.pointer(\"/tags/0\")))
    print(as_int(doc.pointer(\"/a~1b/~0/1\")))
    print(doc.pointer(\"/tags/01\")?)
    print(doc.pointer(\"/tags/9\")?)
    print(doc.pointer(\"/n/x\")?)
    print(doc.pointer(\"tags\")?)
    print(doc.pointer(\"\")?)
    let mutable made be json.object()
    made.set(\"b\", Json.Number(1.0))
    made.set(\"a\", json.list())
    made.set(\"b\", Json.Text(\"two\"))
    let mutable inner be json.list()
    inner.push(Json.Boolean(true))
    inner.push(Json.Null)
    made.set(\"c\", inner)
    print(made)
    let copy be made.clone()
    made.set(\"b\", Json.Null)
    print(copy)
    print(made)
    print(copy is made)
    print(copy is copy.clone())
    let a be parse_ok(\"{\\\"x\\\": 1, \\\"y\\\": 2}\")
    let b be parse_ok(\"{\\\"y\\\": 2, \\\"x\\\": 1}\")
    print(a is b)
    print(a is not b)
",
        ),
    );
    assert_eq!(
        out,
        "sc\n3.0\n3\ntrue\nnone\nnull\nfalse\ntrue\nfalse\n2\n8\n2\nb\nfalse\nfalse\n\
         name n ok none tags k k a/b \n\
         a\n20\nfalse\nfalse\nfalse\nfalse\ntrue\n\
         {\"b\":\"two\",\"a\":[],\"c\":[true,null]}\n\
         {\"b\":\"two\",\"a\":[],\"c\":[true,null]}\n\
         {\"b\":null,\"a\":[],\"c\":[true,null]}\nfalse\ntrue\nfalse\ntrue\n"
    );
}

#[test]
fn nesting_is_bounded_and_deep_documents_work() {
    let out = prints(
        "depth",
        &program(
            "    let mutable ok be \"\"
    for i in 0..500:
        ok.push_str(\"[\")
    for i in 0..500:
        ok.push_str(\"]\")
    let v, e1 be json.parse(&ok)
    if e1?:
        print(e1.message())
    else:
        print(json.stringify(&v).length())
    let mutable deep be \"\"
    for i in 0..600:
        deep.push_str(\"[\")
    for i in 0..600:
        deep.push_str(\"]\")
    roundtrip(deep)
    let mutable objects be \"\"
    for i in 0..600:
        objects.push_str(\"{\\\"a\\\":\")
    roundtrip(objects)
    let mutable wide be \"[\"
    for i in 0..2000:
        if i > 0:
            wide.push_str(\",\")
        wide.push_str(f\"{i}\")
    wide.push_str(\"]\")
    let w, e2 be json.parse(&wide)
    if e2?:
        print(e2.message())
    else:
        print(w.length())
        print(json.stringify(&w) is wide)
",
        ),
    );
    assert_eq!(
        out,
        "1000\nerror: nesting is too deep at line 1, column 513\n\
         error: nesting is too deep at line 1, column 2561\n2000\ntrue\n"
    );
}

#[test]
fn a_byte_order_mark_is_skipped_and_unicode_text_survives() {
    let out = prints(
        "unicode",
        &program(
            "    roundtrip(\"\\u{feff}[1]\")
    let v be parse_ok(\"{\\\"k\\u{e9}y\\\": \\\"\\u{1f600} \\u{4e2d}\\u{6587}\\\"}\")
    print(v)
    print(as_text(v.get(\"k\\u{e9}y\")))
    roundtrip(\"[1,\\n\\u{e9}]\")
",
        ),
    );
    assert_eq!(
        out,
        "[1]\n{\"k\u{e9}y\":\"\u{1f600} \u{4e2d}\u{6587}\"}\n\u{1f600} \u{4e2d}\u{6587}\nerror: unexpected character at line 2, column 1\n"
    );
}
