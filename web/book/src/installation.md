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

## Build and install the compiler

```shell
git clone https://github.com/Angel-Zeledon/sciencec
cd sciencec
./install.sh
```

`install.sh` builds the compiler and its runtime and installs them for your
user, with `sciencec` in `~/.local/bin`. If that directory is not on your
`PATH` yet, the script tells you the line to add. Then:

```shell
sciencec --version
```

Run `./install.sh` again after `git pull` to update.

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
sciencec hello.science
```

```output
It works!
```

If you see that, you are ready for the next chapter.
