//! The bundled `string` module, **built, linked, run**.
//!
//! `string.science` is DREAM §13.3's missing `String` operations plus case
//! mapping, padding, wrapping and edit distance, in Science over
//! `encoding.code_points`. Each test builds a program that `use`s the module,
//! runs it, and pins what it printed. The expected text is derived by hand from
//! the code points and the header's decisions, not copied from a run; results
//! are shown in `[...]` so an empty string or a stray space is visible.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("string", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

const USE: &str = "use string (to_upper, to_lower, title_case, equals_ignore_case, repeat, \
trim_start, trim_end, rfind, chars_count, push_char, pad_start, pad_end, center, wrap, \
split_whitespace, levenshtein, graphemes, lower_of, upper_of)\n";

const HELPERS: &str = r#"
def at(found: Int?) -> String:
    if found?:
        return f"{found}"
    "null"

def words(parts: &Array[String]) -> String:
    let mutable out be f"{parts.length()}:"
    for part in parts:
        out.push_str(f" [{part}]")
    out
"#;

fn program(name: &str, body: &str) -> String {
    prints(name, &format!("{USE}{HELPERS}\ndef main():\n{body}"))
}

/// ASCII, Latin-1 (including the two letters that leave the block), Latin
/// Extended-A in all four of its parity regions, Greek with its accented
/// letters and final sigma, and Cyrillic with its three ranges. `ß` and `×`
/// are unchanged, and `ς` lowers to nothing new but uppers to `Σ`.
#[test]
fn upper_and_lower_across_the_covered_scripts() {
    assert_eq!(
        program(
            "case",
            r#"    print(f"[{to_upper("Hello, World 123")}]")
    print(f"[{to_lower("Hello, World 123")}]")
    print(f"[{to_upper("àéîõü ÿ µ ß ×")}]")
    print(f"[{to_lower("ÀÉÎÕÜ Ÿ ß ×")}]")
    print(f"[{to_upper("ąćęłńśźż ĳ ŉ")}]")
    print(f"[{to_lower("ĀĒİIŁŃŚŹŻ")}]")
    print(f"[{to_upper("αβγ άέήίόύώ ςσ ϊϋ")}]")
    print(f"[{to_lower("ΑΒΓ ΆΈΉΊΌΎΏ ΣΊΣΥΦΟΣ ΪΫ")}]")
    print(f"[{to_upper("привет ёжик ђ ѣ ӏ ԁ")}]")
    print(f"[{to_lower("ПРИВЕТ ЁЖИК Ђ Ѣ Ӏ Ԁ")}]")
    print(f"[{to_upper("")}]")
    print(f"[{to_upper("日本語")}]")
"#
        ),
        "\
[HELLO, WORLD 123]
[hello, world 123]
[ÀÉÎÕÜ Ÿ Μ ß ×]
[àéîõü ÿ ß ×]
[ĄĆĘŁŃŚŹŻ Ĳ ŉ]
[āēiiłńśźż]
[ΑΒΓ ΆΈΉΊΌΎΏ ΣΣ ΪΫ]
[αβγ άέήίόύώ σίσυφοσ ϊϋ]
[ПРИВЕТ ЁЖИК Ђ Ѣ Ӏ Ԁ]
[привет ёжик ђ ѣ ӏ ԁ]
[]
[日本語]
"
    );
}

/// For every code point the tables touch, `upper_of(lower_of(c))` is `c`
/// except the one letter whose lower case is shared (`İ`), and
/// `lower_of(upper_of(c))` is `c` except the four lower-case letters whose
/// upper case belongs to another (`µ`, `ı`, `ſ`, `ς`).
#[test]
fn the_two_mappings_are_inverse_except_where_documented() {
    assert_eq!(
        program(
            "inverse",
            r#"    let mutable lost_upper be ""
    let mutable lost_lower be ""
    for c in 0..1400:
        if lower_of(c) is not c and upper_of(lower_of(c)) is not c:
            lost_upper.push_str(f" {c}")
        if upper_of(c) is not c and lower_of(upper_of(c)) is not c:
            lost_lower.push_str(f" {c}")
    print(f"upper(lower(c)) differs:{lost_upper}")
    print(f"lower(upper(c)) differs:{lost_lower}")
"#
        ),
        "\
upper(lower(c)) differs: 304
lower(upper(c)) differs: 181 305 383 962
"
    );
}

#[test]
fn title_case_capitalises_each_word() {
    assert_eq!(
        program(
            "title",
            r#"    print(f"[{title_case("hello wORLD")}]")
    print(f"[{title_case("o'neil-smith  über straße")}]")
    print(f"[{title_case("ελληνικά ΚΕΊΜΕΝΟ")}]")
    print(f"[{title_case("привет, МИР")}]")
    print(f"[{title_case("3rd place_winner")}]")
    print(f"[{title_case("")}]")
"#
        ),
        "\
[Hello World]
[O'neil-Smith  Über Straße]
[Ελληνικά Κείμενο]
[Привет, Мир]
[3rd Place_Winner]
[]
"
    );
}

#[test]
fn case_insensitive_comparison() {
    assert_eq!(
        program(
            "fold",
            r#"    print(equals_ignore_case("Straße", "STRAßE"))
    print(equals_ignore_case("ÀÉ", "àé"))
    print(equals_ignore_case("Привет", "пРИВЕТ"))
    print(equals_ignore_case("abc", "abd"))
    print(equals_ignore_case("abc", "abcd"))
    print(equals_ignore_case("", ""))
    print(equals_ignore_case("ΣΑΣ", "σας"))
"#
        ),
        "true\ntrue\ntrue\nfalse\nfalse\ntrue\nfalse\n"
    );
}

#[test]
fn repeat_chars_count_and_push_char() {
    assert_eq!(
        program(
            "measure",
            r#"    print(f"[{repeat("ab", 3)}] [{repeat("x", 0)}] [{repeat("é", 2)}] [{repeat("x", -1)}]")
    let text be "héllo"
    print(f"{text.length()} {chars_count(text)}")
    print(f"{chars_count("日本語")} {chars_count("")} {chars_count("🙂a")}")
    let mutable out be String.new()
    push_char(out, 'a')
    push_char(out, 'é')
    push_char(out, '日')
    print(f"[{out}] {out.length()} {chars_count(out)}")
"#
        ),
        "\
[ababab] [] [éé] []
6 5
3 0 2
[aé日] 6 3
"
    );
}

#[test]
fn trim_halves_and_rfind() {
    assert_eq!(
        program(
            "trim",
            r#"    print(f"[{trim_start("  \t hi  ")}] [{trim_end("  \t hi  ")}]")
    print(f"[{trim_start("   \n")}] [{trim_end("   \n")}] [{trim_start("")}]")
    print(f"[{trim_end("é \r\n")}] [{trim_start("\n é")}]")
"#
        ),
        "[hi  ] [  \t hi]\n[] [] []\n[é] [é]\n"
    );
    assert_eq!(
        program(
            "rfind",
            r#"    print(at(rfind("a.b.c", ".")))
    print(at(rfind("abc", "x")))
    print(at(rfind("héllo héllo", "é")))
    print(at(rfind("abc", "")))
    print(at(rfind("ab", "abc")))
    print(at(rfind("aaa", "aa")))
    print(at(rfind("abc", "a")))
    print(at(rfind("", "")))
"#
        ),
        "3\nnull\n8\n3\nnull\n1\n0\n0\n"
    );
}

#[test]
fn padding_and_centering() {
    assert_eq!(
        program(
            "pad",
            r#"    print(f"[{pad_start("42", 5, "0")}] [{pad_end("ab", 6, "-=")}]")
    print(f"[{center("hi", 7, "*")}] [{center("é", 4, " ")}] [{center("ab", 5, ".")}]")
    print(f"[{pad_start("long", 2, "x")}] [{pad_start("x", 4, "")}] [{pad_end("abc", 3, "-")}]")
    print(f"[{pad_start("日", 3, "é")}] [{pad_end("日", 3, "é")}] [{center("日", 4, "é")}]")
"#
        ),
        "\
[00042] [ab-=-=]
[**hi***] [ é  ] [.ab..]
[long] [x] [abc]
[éé日] [日éé] [é日éé]
"
    );
}

#[test]
fn splitting_on_whitespace_and_wrapping() {
    assert_eq!(
        program(
            "wrap",
            r#"    print(words(split_whitespace("  a \t b\nc  ")))
    print(words(split_whitespace("")))
    print(words(split_whitespace("日本　語 ")))
    print(words(wrap("the quick brown fox jumps over the lazy dog", 10)))
    print(words(wrap("a  verylongword b", 4)))
    print(words(wrap("", 5)))
    print(words(wrap("  \n ", 5)))
    print(words(wrap("aa\nbb cc", 5)))
    print(words(wrap("él día", 5)))
"#
        ),
        "\
3: [a] [b] [c]
0:
2: [日本] [語]
5: [the quick] [brown fox] [jumps over] [the lazy] [dog]
3: [a] [verylongword] [b]
0:
0:
2: [aa bb] [cc]
2: [él] [día]
"
    );
}

#[test]
fn edit_distance() {
    assert_eq!(
        program(
            "distance",
            r#"    print(levenshtein("kitten", "sitting"))
    print(levenshtein("", "abc"))
    print(levenshtein("abc", ""))
    print(levenshtein("abc", "abc"))
    print(levenshtein("flaw", "lawn"))
    print(levenshtein("saturday", "sunday"))
    print(levenshtein("café", "cafe"))
    print(levenshtein("日本語", "日本人"))
    print(levenshtein("Abc", "abc"))
"#
        ),
        "3\n3\n3\n0\n2\n3\n1\n1\n1\n"
    );
}

/// Cluster sizes, in code points: a base with a combining mark, CR LF, two
/// flags, a ZWJ family, and an emoji with a skin-tone modifier.
#[test]
fn graphemes_approximate_clusters() {
    let body = [
        "    let samples be [\"ae\u{301}b\", \"a\\r\\nb\", \"\u{1F1E8}\u{1F1F7}\u{1F1FA}\u{1F1F8}\", \"\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}x\", \"\u{1F44D}\u{1F3FD}!\", \"\"]",
        "    for sample in samples:",
        "        let mutable sizes be \"\"",
        "        for cluster in graphemes(sample):",
        "            sizes.push_str(f\" {chars_count(cluster)}\")",
        "        print(f\"{sizes}\")",
        "",
    ]
    .join("\n");
    assert_eq!(
        program("graphemes", &body),
        " 1 2 1\n 1 2 1\n 2 2\n 5 1\n 2 1\n\n"
    );
}
