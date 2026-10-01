Science is installed by building its compiler, `sciencec`, from source.
That takes a Rust toolchain and LLVM 18, and about five minutes.

## Requirements

- **Rust** 1.80 or newer, from [rustup.rs](https://rustup.rs).
- **LLVM 18**, which `sciencec` uses to generate machine code.
- A C linker. On macOS that is the Xcode command-line tools
  (`xcode-select --install`); on Linux, `cc` from your distribution.

## macOS

Install LLVM 18 with Homebrew, then point the build at it:

```shell
brew install llvm@18
export SCIENCE_LLVM_PREFIX="$(brew --prefix llvm@18)"
```

## Linux

Most distributions package LLVM 18. On Debian or Ubuntu:

```shell
sudo apt install llvm-18-dev clang-18
export SCIENCE_LLVM_PREFIX=/usr/lib/llvm-18
```

## Build the compiler

```shell
git clone https://github.com/Angel-Zeledon/sciencec
cd sciencec
cargo build --release -p science-rt -p sciencec --features llvm
```

The compiler is now at `target/release/sciencec`. Put it on your `PATH` so
you can call it from anywhere:

```shell
export PATH="$PWD/target/release:$PATH"
sciencec --version
```

> **Note:** the `--features llvm` flag matters. Without it you get a
> compiler that can check programs but not build them, and every `build` or
> `run` stops with error `SC0400`.

## Editor support

The repository ships a VS Code extension in `editors/vscode` with syntax
highlighting. Install it from the `.vsix` file with
*Extensions → … → Install from VSIX*.

## Check that it works

Make a file called `hello.science` containing one line:

```science run
print("It works!")
```

and run it:

```shell
sciencec run hello.science
```

```output
It works!
```

If you see that, you are ready for the next chapter.
