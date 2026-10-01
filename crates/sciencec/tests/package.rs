//! The package commands end to end: real directories on disk, the real
//! binary run inside them, and assertions on stdout, stderr, the exit code
//! and the files left behind.
//!
//! `cli.rs` runs every command from the repository root; these cannot, because
//! a package command *is* a function of the working directory — it finds the
//! package by walking up from it. So each test builds its packages under its
//! own directory and runs `sciencec` there.
//!
//! **Under `CARGO_TARGET_TMPDIR`, except for the one test that needs no
//! manifest above it.** Walking up from the target directory reaches the
//! repository, and a checkout may carry a `science.toml` at its root (this
//! one's does, untracked). Every package below has its own manifest, so the
//! walk stops there; the missing-manifest test goes to the system temporary
//! directory instead, where nothing is above it.

use std::path::{Path, PathBuf};
use std::process::Command;

struct Run {
    command: String,
    code: i32,
    stdout: String,
    stderr: String,
}

/// Runs `sciencec` with `dir` as its working directory.
fn sciencec_in(dir: &Path, args: &[&str]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_sciencec"))
        .current_dir(dir)
        .args(args)
        .env_remove("SCIENCE_PATH")
        .output()
        .expect("the sciencec binary must be runnable");
    let stderr = String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n");
    assert!(!stderr.contains("panicked at"), "`sciencec {}` panicked:\n{stderr}", args.join(" "));
    Run {
        command: format!("sciencec {} (in {})", args.join(" "), dir.display()),
        code: output.status.code().expect("sciencec must exit normally"),
        stdout: String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
        stderr,
    }
}

impl Run {
    #[track_caller]
    fn succeeded(&self) -> &Self {
        assert_eq!(self.code, 0, "`{}` should exit 0\nstdout:\n{}\nstderr:\n{}", self.command, self.stdout, self.stderr);
        self
    }

    #[track_caller]
    fn failed(&self) -> &Self {
        assert_eq!(self.code, 1, "`{}` should exit 1\nstdout:\n{}\nstderr:\n{}", self.command, self.stdout, self.stderr);
        self
    }

    #[track_caller]
    fn stderr_has(&self, text: &str) -> &Self {
        assert!(self.stderr.contains(text), "`{}`: stderr should contain {text:?}\nstderr:\n{}", self.command, self.stderr);
        self
    }
}

/// A fresh, empty directory for one test, under the target directory.
fn workspace(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("packages").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the target directory is writable");
    dir
}

/// Writes `files` under `dir`, creating directories as needed.
fn write(dir: &Path, files: &[(&str, &str)]) {
    for (path, text) in files {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().expect("a file has a parent")).expect("writable");
        std::fs::write(&file, text).expect("writable");
    }
}

#[test]
fn new_then_run_prints_the_greeting_and_writes_target_and_lock() {
    let dir = workspace("new_then_run");
    sciencec_in(&dir, &["new", "hello"]).succeeded();
    let hello = dir.join("hello");
    for file in ["science.toml", "src/main.science", "src/greeting.science", "tests/greeting.science", ".gitignore"] {
        assert!(hello.join(file).is_file(), "`sciencec new` should write {file}");
    }

    let run = sciencec_in(&hello, &["run"]);
    run.succeeded();
    assert_eq!(run.stdout, "Hello, world!\n");

    let exe = if cfg!(windows) { "target/hello.exe" } else { "target/hello" };
    assert!(hello.join(exe).is_file(), "the executable goes in target/");
    let lock = std::fs::read_to_string(hello.join("science.lock")).expect("the lock is written");
    assert!(lock.contains("[[package]]\nname = \"hello\"\nversion = \"0.1.0\"\n"), "{lock}");
    assert!(lock.contains("[build]\nsciencec = "), "{lock}");

    // A second build changes nothing in the lock: it is rewritten only when
    // its text would differ.
    let before = std::fs::metadata(hello.join("science.lock")).unwrap().modified().unwrap();
    sciencec_in(&hello, &["build"]).succeeded();
    let after = std::fs::metadata(hello.join("science.lock")).unwrap().modified().unwrap();
    assert_eq!(before, after, "an unchanged lock is not rewritten");
}

#[test]
fn a_package_is_found_from_a_directory_inside_it() {
    let dir = workspace("from_inside");
    sciencec_in(&dir, &["new", "inner"]).succeeded();
    let run = sciencec_in(&dir.join("inner").join("src"), &["run"]);
    run.succeeded();
    assert_eq!(run.stdout, "Hello, world!\n");
}

#[test]
fn new_refuses_an_illegal_name_and_an_existing_directory() {
    let dir = workspace("new_refusals");
    sciencec_in(&dir, &["new", "my-app"])
        .failed()
        .stderr_has("error[SP0004]: `my-app` is not a legal package name")
        .stderr_has("`myapp` is the same name, made legal");
    assert!(!dir.join("my-app").exists(), "nothing is written for an illegal name");

    sciencec_in(&dir, &["new", "taken"]).succeeded();
    sciencec_in(&dir, &["new", "taken"]).failed().stderr_has("`taken` already exists");
}

#[test]
fn test_reports_each_file_and_fails_when_one_fails() {
    let dir = workspace("test_pass_fail");
    sciencec_in(&dir, &["new", "checked"]).succeeded();
    let package = dir.join("checked");

    let passing = sciencec_in(&package, &["test"]);
    passing.succeeded();
    assert_eq!(passing.stdout, "test tests/greeting.science ... ok\n");

    write(
        &package,
        &[(
            "tests/wrong.science",
            "use checked.greeting (greeting)\n\ndef main():\n    assert(greeting() is \"Goodbye\", \"the greeting is not a farewell\")\n",
        )],
    );
    let failing = sciencec_in(&package, &["test"]);
    failing.failed();
    assert!(failing.stdout.starts_with("test tests/greeting.science ... ok\ntest tests/wrong.science ... FAILED"), "{}", failing.stdout);
    assert!(failing.stderr.contains("the greeting is not a farewell"), "{}", failing.stderr);
}

/// Three packages: `app` depends on `util`, `util` on `base`. `app` and
/// `util` each have a module `greeting` defining a function `greeting`, both
/// called, so a mangling that did not keep packages apart would link one of
/// them twice and print its answer twice.
#[test]
fn path_dependencies_are_reached_by_use_and_keep_their_symbols_apart() {
    let dir = workspace("path_dependencies");
    write(
        &dir,
        &[
            ("base/science.toml", "[package]\nname = \"base\"\nversion = \"1.2.3\"\n"),
            ("base/src/mod.science", "public def exclaim(text: String) -> String:\n    f\"{text}!\"\n"),
            (
                "util/science.toml",
                "[package]\nname = \"util\"\nversion = \"0.2.0\"\n\n[dependencies]\nbase = { path = \"../base\" }\n",
            ),
            ("util/src/mod.science", "public def twice(n: Int) -> Int:\n    n * 2\n"),
            (
                "util/src/loud.science",
                "use util.greeting (greeting)\n\npublic def shout() -> String:\n    greeting()\n",
            ),
            (
                "util/src/greeting.science",
                "use base (exclaim)\n\npublic def greeting() -> String:\n    exclaim(\"util\")\n",
            ),
            (
                "app/science.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies.util]\npath = \"../util\"\n",
            ),
            ("app/src/greeting.science", "public def greeting() -> String:\n    \"app\"\n"),
            (
                "app/src/main.science",
                "use app.greeting (greeting)\nuse util (twice)\nuse util.loud (shout)\nuse os (args)\n\n\
                 def main():\n    print(greeting())\n    print(shout())\n    print(twice(21))\n    \
                 let all be args()\n    for index in 1..all.length():\n        let argument be all.get(index)\n        \
                 if argument?:\n            print(f\"[{argument}]\")\n",
            ),
        ],
    );
    let app = dir.join("app");
    let run = sciencec_in(&app, &["run", "--", "first", "two words", "--flag"]);
    run.succeeded();
    assert_eq!(run.stdout, "app\nutil!\n42\n[first]\n[two words]\n[--flag]\n");

    let lock = std::fs::read_to_string(app.join("science.lock")).expect("the lock is written");
    let order: Vec<&str> = lock.lines().filter(|l| l.starts_with("name = ") || l.starts_with("source = ")).collect();
    assert_eq!(
        order,
        [
            "name = \"app\"",
            "name = \"base\"",
            "source = \"path+../base\"",
            "name = \"util\"",
            "source = \"path+../util\"",
        ]
    );
}

#[test]
fn a_mounted_package_without_the_module_is_not_searched_past() {
    let dir = workspace("mount_is_final");
    write(
        &dir,
        &[
            ("util/science.toml", "[package]\nname = \"util\"\nversion = \"0.1.0\"\n"),
            ("util/src/mod.science", "public def one() -> Int:\n    1\n"),
            (
                "app/science.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\n[dependencies]\nutil = { path = \"../util\" }\n",
            ),
            // A sibling file that *would* answer `util/missing.science` if the
            // lookup fell through the mount to the crate root.
            ("app/src/util/missing.science", "public def two() -> Int:\n    2\n"),
            ("app/src/main.science", "use util.missing (two)\n\ndef main():\n    print(two())\n"),
        ],
    );
    sciencec_in(&dir.join("app"), &["build"]).failed().stderr_has("error[SC0202]");
}

#[test]
fn run_passes_the_programs_exit_status_through() {
    let dir = workspace("run_status");
    write(
        &dir,
        &[
            ("aborts/science.toml", "[package]\nname = \"aborts\"\nversion = \"0.1.0\"\n"),
            ("aborts/src/main.science", "def main():\n    let one be 1\n    assert(one > 2, \"one is not more than two\")\n"),
        ],
    );
    let run = sciencec_in(&dir.join("aborts"), &["run"]);
    assert_ne!(run.code, 0, "{}", run.stderr);
    assert!(run.stderr.contains("one is not more than two"), "{}", run.stderr);
    if cfg!(unix) {
        assert_eq!(run.code, 128 + 6, "SIGABRT is reported the way a shell does: {}", run.stderr);
    }
}

#[test]
fn a_named_file_is_still_a_script_inside_a_package() {
    let dir = workspace("script_inside");
    sciencec_in(&dir, &["new", "host"]).succeeded();
    let package = dir.join("host");
    write(&package, &[("scripts/plot.science", "def main():\n    print(\"plotted\")\n")]);
    let run = sciencec_in(&package, &["run", "scripts/plot.science"]);
    run.succeeded();
    assert_eq!(run.stdout, "plotted\n");
    assert!(!package.join("science.lock").exists(), "a script build reads no manifest and writes no lock");
}

#[test]
fn a_missing_manifest_is_named_with_the_way_out() {
    let dir = std::env::temp_dir().join(format!("sciencec-no-manifest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    if dir.ancestors().any(|d| d.join("science.toml").is_file()) {
        eprintln!("skipped: a science.toml sits above {}", dir.display());
        return;
    }
    for command in ["build", "run", "test", "check"] {
        sciencec_in(&dir, &[command])
            .failed()
            .stderr_has("error[SP0060]: no `science.toml` in")
            .stderr_has("`sciencec new NAME` creates a package");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unknown_dependency_is_pointed_at_in_the_manifest() {
    let dir = workspace("unknown_dependency");
    write(
        &dir,
        &[
            (
                "app/science.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nghost = { path = \"../ghost\" }\n",
            ),
            ("app/src/main.science", "def main():\n    print(1)\n"),
        ],
    );
    sciencec_in(&dir.join("app"), &["build"])
        .failed()
        .stderr_has("error[SP0061]: the dependency `ghost` names no package\n --> science.toml:6:")
        .stderr_has("nothing exists at this path");
    assert!(!dir.join("app/science.lock").exists(), "a failed walk writes no lock");
}

#[test]
fn a_dependency_cycle_is_reported_with_its_path() {
    let dir = workspace("cycle");
    write(
        &dir,
        &[
            (
                "app/science.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\n[dependencies]\nutil = { path = \"../util\" }\n",
            ),
            ("app/src/main.science", "def main():\n    print(1)\n"),
            (
                "util/science.toml",
                "[package]\nname = \"util\"\nversion = \"0.1.0\"\n[dependencies]\napp = { path = \"../app\" }\n",
            ),
        ],
    );
    sciencec_in(&dir.join("app"), &["build"])
        .failed()
        .stderr_has("error[SP0013]: a cycle among `path` dependencies\n --> ../util/science.toml:5:1")
        .stderr_has("the cycle is app -> util -> app");
}

#[test]
fn manifest_mistakes_carry_their_sp_codes() {
    let dir = workspace("manifest_mistakes");
    write(
        &dir,
        &[
            (
                "app/science.toml",
                "[package]\nname = \"app\"\nverison = \"0.1.0\"\n\n[dependencies]\nlinalg = \"0.7.0\"\nother = { name = \"x\" }\n",
            ),
            ("app/src/main.science", "def main():\n    print(1)\n"),
            ("named/science.toml", "[package]\nname = \"named\"\nversion = \"0.1.0\"\n"),
            ("named/src/mod.science", ""),
            (
                "mismatch/science.toml",
                "[package]\nname = \"mismatch\"\nversion = \"0.1.0\"\n[dependencies]\nalias = { path = \"../named\" }\n",
            ),
            ("mismatch/src/main.science", "def main():\n    print(1)\n"),
            ("nomain/science.toml", "[package]\nname = \"nomain\"\nversion = \"0.1.0\"\n"),
            ("broken/science.toml", "[package\nname = \"broken\"\n"),
        ],
    );
    sciencec_in(&dir.join("app"), &["build"])
        .failed()
        .stderr_has("error[SP0002]: unknown key `verison` in `science.toml`")
        .stderr_has("help: a key with a similar name exists: `version`")
        .stderr_has("error[SP0003]: `[package]` has no `version`")
        .stderr_has("error[SP0063]: a `version` dependency is designed but not implemented by this `sciencec`")
        .stderr_has("error[SP0002]: unknown key `name` in `science.toml`");
    sciencec_in(&dir.join("mismatch"), &["build"])
        .failed()
        .stderr_has("error[SP0062]: the dependency `alias` is a package named `named`");
    sciencec_in(&dir.join("nomain"), &["run"])
        .failed()
        .stderr_has("error[SP0064]: the package `nomain` has no entry file");
    sciencec_in(&dir.join("broken"), &["build"])
        .failed()
        .stderr_has("error[SP0001]: `science.toml` is not valid TOML: a table header is closed by `]`");
}

#[test]
fn module_path_reaches_a_directory_of_modules_from_a_script() {
    let dir = workspace("module_path");
    write(
        &dir,
        &[
            ("lib/shapes.science", "public def sides() -> Int:\n    4\n"),
            ("prog/main.science", "use shapes (sides)\n\ndef main():\n    print(sides())\n"),
        ],
    );
    let prog = dir.join("prog");
    sciencec_in(&prog, &["check", "main.science"]).failed().stderr_has("error[SC0202]");
    let run = sciencec_in(&prog, &["run", "--module-path", "../lib", "main.science"]);
    run.succeeded();
    assert_eq!(run.stdout, "4\n");
}
