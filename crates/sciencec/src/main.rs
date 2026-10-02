//! `sciencec` — the Science compiler's command-line driver.
//!
//! The front half of the compiler has been runnable from `cargo test` since it
//! was written; this is the first way to run it on a file. The binary is
//! deliberately thin: every command here is a call into an existing crate plus
//! the bookkeeping a shell needs — what goes to stdout, what goes to stderr,
//! and what the process exits with.
//!
//! ```text
//! sciencec check FILE...     run the front half; report what is wrong
//! sciencec build FILE...     check, then produce an executable
//! sciencec fmt FILE          print the file, formatted
//! sciencec fmt --write FILE...  format in place
//! sciencec tokens FILE       dump the token stream
//! sciencec ast FILE          dump the syntax tree
//! sciencec resolve FILE      dump the resolved crate
//! sciencec tools --json FILE  the JSON Schema of every `tool` in the file
//! sciencec run FILE [-- ARGS]  build the file, then run it
//! sciencec new NAME          scaffold a package
//! sciencec check | build | run | test   with no FILE: the package around
//!                            the working directory (`crate::package`)
//! ```
//!
//! # A file argument is an entry, and names a crate
//!
//! `check` and `build` take the file to be the **entry** of a crate
//! (`script-mode.md` §4.2 rule 1), whose root is the directory that file is
//! in, and compile it together with every module its `use` declarations reach.
//! Several files named on one command line are still several crates, one per
//! name; `driver::Session::check` argues why, and the short version is that a
//! crate has one entry and `sciencec check *.science` over a directory of
//! scripts must not turn all but one of them into a module.
//!
//! `fmt`, `tokens`, `ast` and `tools` read the file they were given and no
//! other. `resolve` is the exception, and deliberately: it dumps the crate,
//! because the module tree `use` built is the thing worth looking at.
//!
//! # Where output goes
//!
//! Diagnostics and the `3 errors, 1 warning` summary go to **stderr**; dumps
//! go to **stdout**. That is what makes `sciencec ast f.science > f.ast`
//! useful and `sciencec check f.science` quiet when the file is clean.
//!
//! # The exit code
//!
//! `0` when nothing of severity `Error` was reported, `1` otherwise. Warnings
//! alone do not fail, so a CI job can gate on the exit code without arguing
//! about lint levels. A file that cannot be read is an error like any other:
//! it is reported, it sets the exit code, and the remaining files are still
//! checked.
//!
//! # Colour
//!
//! There is none, so `NO_COLOR` is honoured trivially. `science-diagnostics`
//! renders plain text on purpose — the UI suite compares it byte for byte —
//! and adding escape codes here would mean re-rendering diagnostics in the
//! driver, which is exactly what this binary must not do.

mod childwait;
mod driver;
mod package;
mod tools;
mod report;

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use driver::{BuildFlags, Session};

const USAGE: &str = "\
sciencec — the Science compiler

Usage:
    sciencec check FILE...    run the front half; report what is wrong
    sciencec build FILE...    check, then produce an executable
    sciencec build --emit=llvm-ir FILE...
                              the same, and write the module's IR as FILE.ll
    sciencec test FILE...     build, then run the executable and report its exit
    sciencec fmt FILE         print the file, formatted, to stdout
    sciencec fmt --write F... format the files in place
    sciencec tokens FILE      dump the token stream
    sciencec ast FILE         dump the syntax tree
    sciencec resolve FILE     dump the resolved crate
    sciencec tools --json FILE
                              a JSON Schema for every `tool` in the file
    sciencec tools --json --all-functions FILE
                              the same over every top-level function: a
                              measurement of the type mapping, and not a list
                              of callables anything should be offered
    sciencec FILE [ARGS...]    the same as `sciencec run FILE -- ARGS...`
    sciencec run FILE [-- ARGS...]
                              build FILE, then run it with ARGS
    sciencec --version        print the version

Packages (a directory with a `science.toml`):
    sciencec new NAME         create the package NAME in ./NAME
    sciencec check            check the package around the working directory
    sciencec build            build it into target/
    sciencec run [-- ARGS...] build it, then run it with ARGS
    sciencec test             build and run each tests/*.science against it

`--module-path DIR` (repeatable) adds DIR to the module search path, after
the crate root and before `SCIENCE_PATH`; the bundled modules come last.
    sciencec --help           print this message

Diagnostics go to stderr, dumps to stdout.
Exits 0 when nothing was reported as an error, 1 otherwise; warnings alone
do not fail.

`check`, `build` and `resolve` treat FILE as the entry of a crate: the
directory it is in is the crate root, and every module its `use` declarations
name is compiled with it. Several files are several crates, one each.

`fmt` writes to stdout unless `--write` is given, and refuses any file that
does not already lex and parse — reformatting a file whose token stream is a
guess is how a formatter eats a program.";

fn main() -> ExitCode {
    let args = script_form(std::env::args_os().skip(1).collect());
    match run(&args) {
        Outcome::Clean => ExitCode::SUCCESS,
        Outcome::Failed => ExitCode::FAILURE,
        Outcome::Exit(code) => ExitCode::from(code),
    }
}

/// `sciencec FILE [ARGS...]` is `sciencec run FILE -- ARGS...`.
///
/// This is the form a shell uses for a script — `python3 analysis.py`, or a
/// `#!/usr/bin/env sciencec` line the lexer already skips — so a file runs
/// without the reader having to know the subcommand. It applies only when
/// the first operand ends in `.science` and is not a subcommand's name, so
/// every spelling that meant something before still means it.
fn script_form(args: Vec<OsString>) -> Vec<OsString> {
    let is_script = args
        .first()
        .and_then(|first| first.to_str())
        .is_some_and(|first| first.ends_with(".science") && !first.starts_with('-'));
    if !is_script {
        return args;
    }
    let mut rewritten = vec![OsString::from("run"), args[0].clone()];
    if args.len() > 1 {
        rewritten.push(OsString::from("--"));
        rewritten.extend(args[1..].iter().cloned());
    }
    rewritten
}

enum Outcome {
    Clean,
    Failed,
    /// `sciencec run`'s program ran, and this is what it exited with — passed
    /// through, so `sciencec run` is transparent to a shell.
    Exit(u8),
}

fn run(args: &[OsString]) -> Outcome {
    let Some((command, rest)) = args.split_first() else {
        eprintln!("{USAGE}");
        return Outcome::Failed;
    };

    match command.to_str() {
        Some("--help") | Some("-h") | Some("help") => {
            println!("{USAGE}");
            return Outcome::Clean;
        }
        Some("--version") | Some("-V") => {
            println!("sciencec {}", env!("CARGO_PKG_VERSION"));
            return Outcome::Clean;
        }
        _ => {}
    }

    // Two commands take a flag of their own, and both flags are taken off the
    // operands before they become paths. Doing it here rather than in
    // `collect_files` keeps that function's rule intact: outside these lines,
    // anything beginning with `-` is still an error rather than a file.
    //
    // `tools --json` accepts the flag and requires nothing of it: JSON is the
    // only format there is. It is spelled anyway because `mcp-servers.md`
    // §14.3 spells it, and because a second format — a human-readable listing
    // — is the obvious next thing to want.
    // `run` hands everything after the first `--` to the program, untouched:
    // `sciencec run -- --verbose` must reach the program as `--verbose`
    // rather than be refused here as an unknown option.
    let (operands, program_args) = match command.to_str() {
        Some("run") => match rest.iter().position(|arg| arg == "--") {
            Some(at) => (rest[..at].to_vec(), rest[at + 1..].to_vec()),
            None => (rest.to_vec(), Vec::new()),
        },
        _ => (rest.to_vec(), Vec::new()),
    };

    // `--module-path DIR`, for every command that builds a crate: step 3 of
    // `stdlib-shape-and-packages.md` §6.3's search path, ahead of
    // `SCIENCE_PATH` and the toolchain.
    let (operands, module_path) = match take_valued(&operands, "--module-path") {
        Ok(taken) => taken,
        Err(message) => return usage_error(&message),
    };

    let (rest, write) = match command.to_str() {
        Some("fmt") => take_flag(&operands, "--write"),
        Some("tools") => take_flag(&operands, "--json"),
        _ => (operands, false),
    };

    // `tools` takes a second flag, and it is spelled out rather than folded
    // into the line above because the two mean opposite things: `--json`
    // requires nothing of the command, and `--all-functions` changes what it
    // walks. See `crate::tools` for why that is not the default.
    let (rest, every_function) = match command.to_str() {
        Some("tools") => take_flag(&rest, "--all-functions"),
        _ => (rest, false),
    };

    // `build` and `test` take two flags of `codegen-and-linking.md`'s, taken off
    // the operands here for the same reason the two above are: outside these
    // lines `collect_files` still rejects anything beginning with `-`.
    //
    // **`--no-noalias` is unsupported and deliberately not in `USAGE`.** §4.4
    // asks for it as *"an unsupported debugging flag"*, and a debugging flag
    // listed beside `--help`'s real ones is a flag somebody will reach for
    // because it made their program work — which, for this one, means it hid
    // the bug rather than fixed it.
    let (rest, no_noalias) = match command.to_str() {
        Some("build") | Some("test") | Some("run") => take_flag(&rest, "--no-noalias"),
        _ => (rest, false),
    };
    let (rest, emit_ir) = match command.to_str() {
        Some("build") | Some("test") | Some("run") => take_flag(&rest, "--emit=llvm-ir"),
        _ => (rest, false),
    };
    let flags = BuildFlags { no_noalias, emit_ir };

    let files = match collect_files(&rest) {
        Ok(files) => files,
        Err(message) => {
            eprintln!("error: {message}");
            eprintln!("{USAGE}");
            return Outcome::Failed;
        }
    };

    let mut session = Session::new();
    session.set_search_dirs(science_package::search::search_dirs(
        &module_path,
        std::env::var_os("SCIENCE_PATH"),
    ));
    if matches!(command.to_str(), Some("check" | "build" | "test" | "run")) {
        package::mount_around(&mut session, &files);
    }
    match command.to_str() {
        // **No file named means the package around the working directory**
        // (`crate::package`'s §"When a command is a package command"). These
        // four used to refuse an empty operand list as a usage error; that
        // refusal is now `SP0060` when there is no manifest to read, which
        // names the same mistake and the two ways out of it.
        Some("check") => {
            if files.is_empty() {
                package::check(&mut session);
            } else {
                session.check(&files);
            }
        }
        Some("new") => {
            let [name] = files.as_slice() else {
                return usage_error("new expects exactly one package name");
            };
            package::new(&mut session, &name.to_string_lossy());
        }
        // `sciencec run [FILE] [-- ARGS...]`. The program's exit code is
        // passed through when it ran; the summary line, if the build warned,
        // is printed first so that it is not mistaken for the program's.
        Some("run") => {
            if files.len() > 1 {
                return usage_error("run expects at most one file; arguments for the program go after `--`");
            }
            let code = package::run(&mut session, files.first().map(PathBuf::as_path), &program_args, flags);
            let outcome = session.finish();
            return match code {
                Some(code) => Outcome::Exit(code),
                None => match outcome {
                    Outcome::Clean => Outcome::Failed,
                    other => other,
                },
            };
        }
        // `build` takes the same operands as `check`, plus the two flags read
        // off above. **`-O` and `--target-cpu` are still not spelled**:
        // `codegen-and-linking.md` §7.4 defines them, nothing reads them, and a
        // flag that is accepted and ignored is worse than one that is not there.
        // The two that are here are read: `--no-noalias` reaches
        // `abi::borrow_attrs` and `--emit=llvm-ir` writes a file.
        Some("build") => {
            if files.is_empty() {
                package::build(&mut session, flags);
            } else {
                session.build(&files, flags);
            }
        }
        // `test` takes the same operands as `build`: each file is an entry,
        // each entry is a crate, and there is no `test` item yet to select
        // among (`Session::test`'s own note says what that costs).
        //
        // **And the same two flags**, because `test` builds through the same
        // `emit_executable` and a debugging flag that worked for `build` and
        // silently did nothing for `test` is the shape of an hour lost.
        Some("test") => {
            if files.is_empty() {
                package::test(&mut session, flags);
            } else {
                session.test(&files, flags);
            }
        }
        Some("fmt") => {
            if write {
                if files.is_empty() {
                    return usage_error("fmt --write expects at least one file");
                }
            } else if files.len() != 1 {
                // Without `--write` the output is the file, and two files
                // concatenated are not a file. `--write` is how you ask for
                // more than one.
                return usage_error("fmt expects exactly one file, or --write");
            }
            session.format(&files, write);
        }
        Some(name @ ("tokens" | "ast" | "resolve" | "tools")) => {
            let [file] = files.as_slice() else {
                return usage_error(&format!("{name} expects exactly one file"));
            };
            match name {
                "tokens" => session.dump_tokens(file),
                "ast" => session.dump_ast(file),
                "tools" => session.dump_tools(file, every_function),
                _ => session.dump_resolved(file),
            }
        }
        other => {
            let shown = other.map(str::to_string).unwrap_or_else(|| command.to_string_lossy().into_owned());
            if shown.starts_with('-') {
                return usage_error(&format!("unknown option `{shown}`"));
            }
            return usage_error(&format!("unknown command `{shown}`"));
        }
    }

    session.finish()
}

/// Turns the operands into paths, rejecting anything that looks like a flag.
///
/// A path is not required to be valid UTF-8 — the filesystem's opinion wins
/// over the driver's — so operands stay `OsString` until they reach `Path`.
fn collect_files(args: &[OsString]) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::with_capacity(args.len());
    for arg in args {
        if let Some(text) = arg.to_str() {
            // A bare `-` is not stdin: a diagnostic has to name a file, and
            // `<stdin>` is a name no editor can jump to.
            if text.starts_with('-') && text.len() > 1 {
                return Err(format!("unknown option `{text}`"));
            }
        }
        files.push(PathBuf::from(arg));
    }
    Ok(files)
}

/// Takes `flag` out of the operands, saying whether it was there.
///
/// Repeating it is not an error: `--write --write` means what it says.
fn take_flag(args: &[OsString], flag: &str) -> (Vec<OsString>, bool) {
    let mut found = false;
    let mut rest = Vec::with_capacity(args.len());
    for arg in args {
        if arg.to_str() == Some(flag) {
            found = true;
        } else {
            rest.push(arg.clone());
        }
    }
    (rest, found)
}

/// Takes every `flag VALUE` and `flag=VALUE` out of the operands, in order.
///
/// Repeatable, because a search path is a list. A `flag` with nothing after
/// it is an error rather than a flag with an empty value: an empty directory
/// on a search path would be the working directory, which is the one input
/// `science_resolve::modules` refuses to let decide what a `use` means.
fn take_valued(args: &[OsString], flag: &str) -> Result<(Vec<OsString>, Vec<PathBuf>), String> {
    let prefix = format!("{flag}=");
    let mut values = Vec::new();
    let mut rest = Vec::with_capacity(args.len());
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.to_str() {
            Some(text) if text == flag => match iter.next() {
                Some(value) => values.push(PathBuf::from(value)),
                None => return Err(format!("`{flag}` expects a directory after it")),
            },
            Some(text) if text.starts_with(&prefix) && text.len() > prefix.len() => {
                values.push(PathBuf::from(&text[prefix.len()..]));
            }
            Some(text) if text.starts_with(&prefix) => {
                return Err(format!("`{flag}` expects a directory after it"))
            }
            _ => rest.push(arg.clone()),
        }
    }
    Ok((rest, values))
}

fn usage_error(message: &str) -> Outcome {
    eprintln!("error: {message}");
    eprintln!("{USAGE}");
    Outcome::Failed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn no_arguments_is_a_usage_error() {
        assert!(matches!(run(&args(&[])), Outcome::Failed));
    }

    #[test]
    fn help_and_version_succeed() {
        assert!(matches!(run(&args(&["--help"])), Outcome::Clean));
        assert!(matches!(run(&args(&["--version"])), Outcome::Clean));
    }

    #[test]
    fn unknown_command_fails() {
        assert!(matches!(run(&args(&["frobnicate", "x.science"])), Outcome::Failed));
    }

    #[test]
    fn check_without_files_fails() {
        assert!(matches!(run(&args(&["check"])), Outcome::Failed));
    }

    #[test]
    fn build_without_files_fails() {
        assert!(matches!(run(&args(&["build"])), Outcome::Failed));
    }

    #[test]
    fn build_is_a_known_command_even_though_it_cannot_yet_build() {
        // The distinction `SC0400` exists to make: `build` on a file that does
        // not exist fails the way `check` does — by naming the file — and not
        // with `unknown command`. A user who reads "unknown command `build`"
        // concludes the compiler has no back end planned; one who reads
        // `SC0400` learns which package to install.
        assert!(matches!(run(&args(&["build", "no/such/file.science"])), Outcome::Failed));
    }

    #[test]
    fn a_dump_command_takes_exactly_one_file() {
        assert!(matches!(run(&args(&["ast", "a.science", "b.science"])), Outcome::Failed));
    }

    #[test]
    fn fmt_needs_one_file_unless_it_is_writing() {
        assert!(matches!(run(&args(&["fmt"])), Outcome::Failed));
        assert!(matches!(run(&args(&["fmt", "a.science", "b.science"])), Outcome::Failed));
        assert!(matches!(run(&args(&["fmt", "--write"])), Outcome::Failed));
    }

    #[test]
    fn the_write_flag_is_only_taken_off_fmts_operands() {
        let (rest, write) = take_flag(&args(&["--write", "a.science"]), "--write");
        assert!(write);
        assert_eq!(rest, args(&["a.science"]));
        let (rest, write) = take_flag(&args(&["a.science"]), "--write");
        assert!(!write);
        assert_eq!(rest, args(&["a.science"]));
        // Every other command still refuses it, because it never reaches here.
        assert!(collect_files(&args(&["--write"])).is_err());
    }

    #[test]
    fn flags_are_not_mistaken_for_files() {
        assert!(collect_files(&args(&["--jobs"])).is_err());
        assert!(collect_files(&args(&["a.science"])).is_ok());
        // A lone `-` is a path, however odd a one.
        assert!(collect_files(&args(&["-"])).is_ok());
    }
}
