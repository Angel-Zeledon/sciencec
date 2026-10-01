//! The bundled `path` module, **built, linked, run**.
//!
//! `path.science` is `data-io.md` §7's `Path` written in Science: eight of its
//! thirteen methods are string manipulation with no runtime at all, and four
//! ask the filesystem through two entry points of `science-rt`'s `fs.rs`. What
//! these tests pin beyond "it runs" is the module's decisions, each on the
//! inputs where a different decision would print something else — `""`, `/`,
//! `a/`, `.hidden`, `file.tar.gz`, `a/../b` among them: separators are
//! normalised and nothing else is, `..` is never resolved, the extension rule
//! is Rust's, and the empty path is the working directory.
//!
//! The filesystem tests run in a directory of each test's own, which the
//! program is started in, as `tests/fs.rs`'s do.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("path", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// What a program printed when started in an empty `work` directory, and that
/// directory's canonical name — what `getcwd` reports from inside it, symbolic
/// links in the scratch root resolved (on macOS `/var` is `/private/var`).
fn prints_in_a_directory(name: &str, source: &str) -> (String, String) {
    let root = scratch("path", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&root, name), OptLevel::O2);
    let work = root.join("work");
    std::fs::create_dir_all(&work).expect("a working directory");
    let output = std::process::Command::new(&built.executable)
        .current_dir(&work)
        .output()
        .expect("the program runs");
    let canonical = std::fs::canonicalize(&work).expect("the working directory exists");
    let _ = std::fs::remove_dir_all(&root);
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stderr), "", "nothing belongs on stderr");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        canonical.to_str().expect("a UTF-8 scratch path").to_string(),
    )
}

/// A program that shows every text-only query of `Path.from(text)` on one line
/// per input, `null` written out for an absent answer.
fn queries(inputs: &[&str]) -> String {
    let mut source = String::from(
        "use path (Path)

def shown(value: String?) -> String:
    if value?:
        return f\"\\\"{value}\\\"\"
    \"null\"

def show(text: String):
    let p be Path.from(&text)
    let up be p.parent()
    let mutable parent be \"null\"
    if up?:
        parent be f\"\\\"{up}\\\"\"
    print(f\"\\\"{text}\\\" is \\\"{p}\\\" parent {parent} name {shown(p.name())} stem {shown(p.stem())} extension {shown(p.extension())} absolute {p.is_absolute()}\")

def main():
",
    );
    for input in inputs {
        source.push_str(&format!("    show(\"{input}\")\n"));
    }
    source
}

/// Each component query on the edge inputs. Read a line as one decision:
///
/// - `""` and `/` have no name and no parent — there is no last component to
///   remove — and `/` is absolute.
/// - `a/` is `a`: a trailing separator is dropped, and the parent of a
///   one-component path is the empty path, which names the working directory.
/// - `.hidden` has no extension: a leading `.` does not start one.
/// - `file.tar.gz`'s extension is `gz` and its stem `file.tar` — only the last
///   `.` separates.
/// - `a/../b` is left as it is, and its parent is `a/..`: `..` is a component,
///   never resolved, because `a` may be a symbolic link.
/// - `//a//b/` is `/a/b`: every run of separators collapses.
/// - `a.` has the empty extension, which is not the same as none.
/// - `.` and `..` name no entry of their own, so neither has a name.
#[test]
fn every_component_query_on_the_edge_inputs() {
    assert_eq!(
        prints(
            "components",
            &queries(&["", "/", "a/", ".hidden", "file.tar.gz", "a/../b", "//a//b/", "a.", ".", "..", "/a", "runs/x.csv"])
        ),
        "\
\"\" is \"\" parent null name null stem null extension null absolute false
\"/\" is \"/\" parent null name null stem null extension null absolute true
\"a/\" is \"a\" parent \"\" name \"a\" stem \"a\" extension null absolute false
\".hidden\" is \".hidden\" parent \"\" name \".hidden\" stem \".hidden\" extension null absolute false
\"file.tar.gz\" is \"file.tar.gz\" parent \"\" name \"file.tar.gz\" stem \"file.tar\" extension \"gz\" absolute false
\"a/../b\" is \"a/../b\" parent \"a/..\" name \"b\" stem \"b\" extension null absolute false
\"//a//b/\" is \"/a/b\" parent \"/a\" name \"b\" stem \"b\" extension null absolute true
\"a.\" is \"a.\" parent \"\" name \"a.\" stem \"a\" extension \"\" absolute false
\".\" is \".\" parent \"\" name null stem null extension null absolute false
\"..\" is \"..\" parent \"\" name null stem null extension null absolute false
\"/a\" is \"/a\" parent \"/\" name \"a\" stem \"a\" extension null absolute true
\"runs/x.csv\" is \"runs/x.csv\" parent \"runs\" name \"x.csv\" stem \"x\" extension \"csv\" absolute false
"
    );
}

/// `join` appends components and normalises the result; an absolute right
/// side replaces the left, an empty one leaves it, and the empty path or `/`
/// on the left contributes nothing but what it is. `with_extension` replaces,
/// adds, or with `""` removes, and leaves a path with no name alone.
#[test]
fn join_and_with_extension() {
    assert_eq!(
        prints(
            "join",
            "use path (Path)

def main():
    let data be Path.from(\"data\")
    print(data.join(\"runs/a.csv\"))
    print(data.join(\"runs//b/\"))
    print(data.join(\"/etc/hosts\"))
    print(data.join(\"\"))
    print(data.join(\"../up\"))
    print(Path.from(\"\").join(\"x\"))
    print(Path.from(\"/\").join(\"x\"))
    print(Path.from(\"a/\").join(\"b\").join(\"c.txt\"))
    print(Path.from(\"a/b.txt\").with_extension(\"csv\"))
    print(Path.from(\"a/b\").with_extension(\"csv\"))
    print(Path.from(\"a/b.txt\").with_extension(\"\"))
    print(Path.from(\"file.tar.gz\").with_extension(\"zst\"))
    print(Path.from(\".hidden\").with_extension(\"txt\"))
    print(Path.from(\"/\").with_extension(\"txt\"))
    print(Path.from(\"..\").with_extension(\"txt\"))
"
        ),
        "\
data/runs/a.csv
data/runs/b
/etc/hosts
data
data/../up
x
/x
a/b/c.txt
a/b.csv
a/b.csv
a/b
file.tar.zst
.hidden.txt
/
..
"
    );
}

/// `Eq` compares normalised text, so two spellings of one path are equal;
/// `Ord`'s `less` is byte order, a prefix first; `Clone` is a separate value;
/// and `text()` is what a `String`-taking function is handed.
#[test]
fn equality_order_clone_and_text() {
    assert_eq!(
        prints(
            "interfaces",
            "use path (Path)

def main():
    print(Path.from(\"a/b\") is Path.from(\"a//b/\"))
    print(Path.from(\"a/b\") is not Path.from(\"a/c\"))
    print(Path.from(\"a/b\").less(&Path.from(\"a/c\")))
    print(Path.from(\"a\").less(&Path.from(\"a/b\")))
    print(Path.from(\"b\").less(&Path.from(\"a\")))
    print(Path.from(\"Z\").less(&Path.from(\"a\")))
    print(Path.from(\"a\").less(&Path.from(\"a\")))
    let original be Path.from(\"x/y\")
    let copy be original.clone()
    print(copy is original)
    print(copy.text().length())
"
        ),
        "true\ntrue\ntrue\ntrue\nfalse\ntrue\nfalse\ntrue\n3\n"
    );
}

/// The filesystem half: `exists`, `is_directory` and `size` on a directory, a
/// file, something missing and the empty path; a `Path` handed to
/// `write_file`, `read_file` and `fs` through `text()`; and `absolute()`,
/// which is the working directory joined with the path, `..` kept.
#[test]
fn the_filesystem_queries_and_absolute() {
    let (stdout, work) = prints_in_a_directory(
        "filesystem",
        "use path (Path)
use fs (create_directory)

def report(p: &Path):
    let size, err be p.size()
    let mutable shown be f\"{size}\"
    if err?:
        shown be err.message()
    print(f\"\\\"{p}\\\" exists {p.exists()} directory {p.is_directory()} size {shown}\")

def main():
    let data be Path.from(\"data\")
    let made be create_directory(data.join(\"inner\").text())
    if made?:
        panic(made.message())
    let five be data.join(\"five.txt\")
    let written be write_file(five.text(), \"12345\")
    if written?:
        panic(written.message())
    report(&five)
    report(&data.join(\"inner\"))
    report(&data.join(\"missing.txt\"))
    report(&Path.from(\"\"))
    let text, err be read_file(five.text())
    if err?:
        panic(err.message())
    print(text)
    let whole, failed be Path.from(\"data/../x\").absolute()
    if failed?:
        panic(failed.message())
    print(whole)
    let here, _here be Path.from(\"\").absolute()
    print(here)
    let root, _root be Path.from(\"/usr/../tmp\").absolute()
    print(root)
",
    );
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[0], "\"data/five.txt\" exists true directory false size 5");
    assert!(lines[1].starts_with("\"data/inner\" exists true directory true size "), "{}", lines[1]);
    assert_eq!(lines[2], "\"data/missing.txt\" exists false directory false size not found");
    assert!(lines[3].starts_with("\"\" exists true directory true size "), "{}", lines[3]);
    assert_eq!(lines[4], "12345");
    assert_eq!(lines[5], format!("{work}/data/../x"));
    assert_eq!(lines[6], work);
    assert_eq!(lines[7], "/usr/../tmp");
    assert_eq!(lines.len(), 8, "{stdout}");
}
