//! `String.trim`, `String.slice` and `String.split`, **built, linked, run**:
//! the last three of `stdlib-core.md` §6.9's nineteen — and `String.from_bytes`
//! beside them, which landed with the bundled `os` module, run here on the
//! inputs `slice`'s tests use.
//!
//! `trim` and `slice` hand back **owned** copies where §6.9 says `borrowed
//! String`; `science-resolve`'s `builtins.rs` argues the deviation at the
//! `String` block. `split` returns a `Split` that owns copies of both of its
//! operands, so these programs also exercise the first iterator with drop
//! glue — a `Split` built from a temporary separator and walked after the
//! statement that built it is the case that copy exists for.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("string_more", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// ASCII whitespace from both ends, the six bytes §6.9 pins — and nothing
/// from the middle, and no Unicode whitespace.
#[test]
fn trim_removes_ascii_whitespace_from_both_ends() {
    assert_eq!(
        prints(
            "trim",
            "def main():
    print(f\"[{\"  héllo, world \\t\\n\".trim()}]\")
    print(f\"[{\"\".trim()}]\")
    print(f\"[{\" \\t\\r\\n \".trim()}]\")
    print(f\"[{\"a  b\".trim()}]\")
",
        ),
        "[héllo, world]\n[]\n[]\n[a  b]\n"
    );
}

/// `trim`'s result is owned, so a function may return it — the program
/// §6.9's `borrowed String` would have had to refuse.
#[test]
fn a_trimmed_string_can_outlive_its_source() {
    assert_eq!(
        prints(
            "trim_returned",
            "def cleaned(text: &String) -> String:
    text.trim()

def main():
    let kept be cleaned(\"  kept  \")
    print(f\"[{kept}]\")
",
        ),
        "[kept]\n"
    );
}

/// A byte range on character boundaries, exclusive and inclusive; inside a
/// character it is `NotACharacterBoundary`, outside the string `OutOfRange`,
/// and the value half is empty on both failing paths.
#[test]
fn slice_is_a_byte_range_that_refuses_to_split_a_character() {
    assert_eq!(
        prints(
            "slice",
            "def show(text: &String, low: Int, high: Int):
    let part, err be text.slice(low..high)
    if err?:
        print(f\"{low}..{high}: {err.message()} [{part}]\")
    else:
        print(f\"{low}..{high}: [{part}]\")

def main():
    let text be \"héllo\"
    show(text, 0, 1)
    show(text, 1, 3)
    show(text, 0, 2)
    show(text, 3, 3)
    show(text, 4, 9)
    show(text, 3, 1)
    let whole, err be text.slice(0..=5)
    if not err?:
        print(f\"[{whole}]\")
",
        ),
        "0..1: [h]\n\
         1..3: [é]\n\
         0..2: not a character boundary []\n\
         3..3: []\n\
         4..9: number out of range []\n\
         3..1: number out of range []\n\
         [héllo]\n"
    );
}

/// A range held in a local, then passed: the range is an ordinary value.
#[test]
fn a_range_in_a_local_can_be_passed_to_slice() {
    assert_eq!(
        prints(
            "slice_local_range",
            "def main():
    let bytes be 1..4
    let part, err be \"abcdef\".slice(bytes)
    if not err?:
        print(part)
",
        ),
        "bcd\n"
    );
}

/// Every separator occurrence separates and every empty piece is kept; an
/// empty separator cuts at every character boundary.
#[test]
fn split_keeps_empty_pieces() {
    assert_eq!(
        prints(
            "split",
            "def show(text: &String, separator: &String):
    let mutable out be String.new()
    for piece in text.split(separator):
        out.push_str(f\"<{piece}>\")
    print(out)

def main():
    show(\"a,b,,c\", \",\")
    show(\",x,\", \",\")
    show(\"\", \",\")
    show(\"one::two\", \"::\")
    show(\"é€\", \"\")
",
        ),
        "<a><b><><c>\n<><x><>\n<>\n<one><two>\n<><é><€><>\n"
    );
}

/// The case `Split` owns its operands for: the separator is a literal, a
/// temporary dropped at the end of the `let`, and the iterator is walked on
/// the lines after.
#[test]
fn a_split_held_in_a_local_outlives_its_temporary_separator() {
    assert_eq!(
        prints(
            "split_held",
            "def main():
    let pieces be \"x;y;z\".split(\";\")
    let mutable n be 0
    for piece in pieces:
        n be n + piece.length()
        print(piece)
    print(n)
",
        ),
        "x\ny\nz\n3\n"
    );
}

/// `from_bytes` validates and copies: a string's own bytes round-trip, and a
/// truncated sequence is `NotUtf8` with an empty value.
#[test]
fn from_bytes_validates_utf8() {
    assert_eq!(
        prints(
            "from_bytes",
            "def main():
    let text be \"naïve\"
    let copy, err be String.from_bytes(text.bytes())
    if not err?:
        print(f\"[{copy}] {copy.length()}\")
    let broken: Array[U8] be [104, 195]
    let none, bad be String.from_bytes(broken)
    if bad?:
        print(f\"{bad.message()} [{none}]\")
",
        ),
        "[naïve] 6\nnot valid UTF-8 []\n"
    );
}

/// §6.11's worked example, in this compiler's spelling: `lines`, `trim`,
/// `is_empty`, `starts_with` and `parse_float` together.
#[test]
fn the_readings_example_of_section_6_11_runs() {
    assert_eq!(
        prints(
            "readings",
            "def readings(text: &String) -> (Array[F64], TextError?):
    let mutable values be Array[F64].new()
    for line in text.lines():
        let field be line.trim()
        if field.is_empty() or field.starts_with(\"#\"):
            continue
        let value, err be field.parse_float()
        if err?:
            return (Array[F64].new(), err)
        values.push(value)
    return (values, null)

def main():
    let values, err be readings(\"  1.5\\n# header\\n\\n\\t2.25 \\n\")
    if err?:
        print(err.message())
    for v in values:
        print(v)
",
        ),
        "1.5\n2.25\n"
    );
}
