//! The bundled `fs` module, **built, linked, run** — in a directory of each
//! test's own, which the program then creates things in, lists, renames and
//! removes.
//!
//! `fs.science` is Science over `science-rt`'s `fs.rs`, reached by `use fs`
//! through the harness's `collect_crate` fallback, which is the driver's. What
//! these tests pin beyond "it runs" is the module's decisions: a listing is
//! **names, sorted in byte order**; `create_directory` creates parents;
//! `remove_directory` refuses a directory with entries and says so in a
//! sentence of its own; every failure reads `io`'s `FileError` sentences.
//!
//! `list_directory` is also the first caller of §6.9's `String.from_bytes`:
//! each name comes back from the runtime as bytes copied into an `Array[U8]`,
//! and that is the only way this compiler has to make them a `String`.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, scratch};
use science_codegen::target::OptLevel;

/// What running a program in a fresh working directory produced.
struct Ran {
    stdout: String,
    /// The working directory the program ran in, still on disk so a test can
    /// look at what it left; removed when this is dropped.
    work: std::path::PathBuf,
    root: std::path::PathBuf,
    executable: std::path::PathBuf,
}

impl Drop for Ran {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Builds `source` and runs it with an empty `work` directory as its working
/// directory — the executable lives one level up, so a listing of `.` sees
/// only what the program made.
fn runs(name: &str, source: &str) -> Ran {
    let root = scratch("fs", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&root, name), OptLevel::O2);
    let work = root.join("work");
    std::fs::create_dir_all(&work).expect("a working directory");
    let output = std::process::Command::new(&built.executable)
        .current_dir(&work)
        .output()
        .expect("the program runs");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stderr), "", "nothing belongs on stderr");
    Ran {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        work,
        root,
        executable: built.executable,
    }
}

/// The whole life of a directory: made with a parent that did not exist,
/// filled with `write_file`, listed, a file renamed, listed again, emptied and
/// removed — and the listing of what is no longer there fails.
///
/// The names are chosen so that byte order is visible: `Z` sorts before `a`,
/// and `é` (two bytes, `0xC3 0xA9`) after every ASCII letter. A listing in
/// the order the filesystem returned would match this only by luck, and a
/// `from_bytes` that mishandled a multi-byte name would print it wrong.
#[test]
fn a_directory_is_made_filled_listed_renamed_and_removed() {
    let ran = runs(
        "lifecycle",
        "use fs (create_directory, list_directory, remove_directory, remove_file, rename)

def show(path: &String):
    let names, err be list_directory(path)
    if err?:
        print(f\"{path}: {err.message()}\")
        return
    let mutable line be f\"{path}:\"
    for name in names:
        line.push_str(\" \")
        line.push_str(name)
    print(line)

def main():
    let made be create_directory(\"data/runs\")
    if made?:
        panic(made.message())
    for name in [\"b.txt\", \"a.txt\", \"Z.txt\", \"\u{e9}.txt\"]:
        let written be write_file(f\"data/{name}\", name)
        if written?:
            panic(written.message())
    show(\"data\")
    show(\"data/runs\")
    let moved be rename(\"data/a.txt\", \"data/runs/c.txt\")
    if moved?:
        panic(moved.message())
    show(\"data\")
    show(\"data/runs\")
    let text, read be read_file(\"data/runs/c.txt\")
    if read?:
        panic(read.message())
    print(text)
    for name in [\"b.txt\", \"Z.txt\", \"\u{e9}.txt\", \"runs/c.txt\"]:
        let removed be remove_file(f\"data/{name}\")
        if removed?:
            panic(removed.message())
    let runs be remove_directory(\"data/runs\")
    let data be remove_directory(\"data\")
    if runs? or data?:
        panic(\"the directories were empty\")
    show(\"data\")
    show(\".\")
",
    );
    assert_eq!(
        ran.stdout,
        "data: Z.txt a.txt b.txt runs é.txt\n\
         data/runs:\n\
         data: Z.txt b.txt runs é.txt\n\
         data/runs: c.txt\n\
         a.txt\n\
         data: not found\n\
         .:\n"
    );
    assert!(!ran.work.join("data").exists(), "`remove_directory` left `data` behind");
}

/// Each refusal, with the sentence it reads as — and the state it leaves,
/// which for every one of them is *nothing changed*.
///
/// `remove_directory` on a directory with entries is the one `data-io.md` §7
/// makes deliberate (*"there is deliberately no recursive delete"*), and it
/// reads *"directory not empty"* rather than `IoError`'s catch-all
/// *"input/output error"*, which would send the reader to the disk.
#[test]
fn every_failure_reports_why_and_changes_nothing() {
    let ran = runs(
        "failures",
        "use fs (create_directory, list_directory, remove_directory, remove_file, rename)

def report(what: &String, err: Error?):
    if err?:
        print(f\"{what}: {err.message()}\")
    else:
        print(f\"{what}: ok\")

def main():
    report(\"remove_file missing\", remove_file(\"missing.txt\"))
    report(\"remove_directory missing\", remove_directory(\"missing\"))
    report(\"rename missing\", rename(\"missing.txt\", \"other.txt\"))
    let made be create_directory(\"full\")
    if made?:
        panic(made.message())
    let written be write_file(\"full/kept.txt\", \"kept\")
    if written?:
        panic(written.message())
    report(\"remove_directory full\", remove_directory(\"full\"))
    report(\"create_directory again\", create_directory(\"full\"))
    report(\"create_directory through a file\", create_directory(\"full/kept.txt/below\"))
    let names, err be list_directory(\"missing\")
    report(\"list_directory missing\", err)
    print(names.length())
",
    );
    assert_eq!(
        ran.stdout,
        "remove_file missing: not found\n\
         remove_directory missing: not found\n\
         rename missing: not found\n\
         remove_directory full: directory not empty\n\
         create_directory again: ok\n\
         create_directory through a file: input/output error\n\
         list_directory missing: not found\n\
         0\n"
    );
    assert_eq!(
        std::fs::read_to_string(ran.work.join("full").join("kept.txt")).ok().as_deref(),
        Some("kept"),
        "a refused `remove_directory` removed something"
    );
    assert!(!ran.work.join("other.txt").exists());
}

/// `current_directory` is the directory the process runs in, as the operating
/// system spells it — on macOS that is the resolved `/private/var/…`, not the
/// `/var/…` a temporary path is handed out as, so the comparison is against
/// the canonical path.
#[test]
fn the_current_directory_is_where_the_program_runs() {
    let ran = runs(
        "current",
        "use fs (current_directory)

def main():
    let here, err be current_directory()
    if err?:
        panic(err.message())
    print(here)
",
    );
    let expected = std::fs::canonicalize(&ran.work).expect("the working directory exists");
    assert_eq!(ran.stdout, format!("{}\n", expected.display()));
}

/// Two thousand listings — each a handle opened, read entry by entry into
/// `String`s and closed by `Listing`'s `drop` — plus two thousand that fail
/// and two thousand `current_directory` calls, and nothing is left behind.
///
/// The live set is constant: the same three names every time, dropped at the
/// end of each iteration, so anything `leaks` finds is a handle that was not
/// closed or a buffer that was not freed, not a value legitimately retained.
#[test]
fn a_listing_loop_leaks_nothing() {
    let ran = runs(
        "leak_loop",
        "use fs (create_directory, current_directory, list_directory)

def main():
    let made be create_directory(\"leak\")
    if made?:
        panic(made.message())
    for name in [\"one\", \"two\", \"three\"]:
        let written be write_file(f\"leak/{name}\", name)
        if written?:
            panic(written.message())
    let mutable total be 0
    for _ in 0..2000:
        let names, err be list_directory(\"leak\")
        if err?:
            panic(err.message())
        total be total + names.length()
        let missing, why be list_directory(\"leak/none\")
        if not why?:
            panic(\"a missing directory was listed\")
        total be total + missing.length()
        let here, bad be current_directory()
        if bad?:
            panic(bad.message())
        if here.is_empty():
            panic(\"no current directory\")
    print(total)
",
    );
    assert_eq!(ran.stdout, "6000\n");
    let leaks = std::path::Path::new("/usr/bin/leaks");
    if cfg!(target_os = "macos") && leaks.is_file() {
        let report = std::process::Command::new(leaks)
            .arg("--atExit")
            .arg("--")
            .arg(&ran.executable)
            .current_dir(&ran.work)
            .output()
            .expect("`leaks` runs");
        let text = String::from_utf8_lossy(&report.stdout);
        assert!(text.contains(" 0 leaks for 0 total leaked bytes"), "`leaks` found something:\n{text}");
    }
}
