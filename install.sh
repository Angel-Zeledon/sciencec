#!/bin/sh
# Build sciencec and its runtime in release mode and install them for this user.
#
#     ./install.sh                 # into ~/.local (sciencec lands in ~/.local/bin)
#     PREFIX=/opt/science ./install.sh
#
# sciencec finds libscience_rt.a beside its own executable, so both go into
# $PREFIX/lib/science, and $PREFIX/bin/sciencec is a two-line launcher for it.
# Needs LLVM 18: set SCIENCE_LLVM_PREFIX, or have Homebrew's llvm@18 installed.
set -eu

PREFIX="${PREFIX:-$HOME/.local}"
if [ -z "${SCIENCE_LLVM_PREFIX:-}" ]; then
    for candidate in "$HOME/.homebrew/opt/llvm@18" /opt/homebrew/opt/llvm@18 /usr/local/opt/llvm@18 /usr/lib/llvm-18; do
        if [ -d "$candidate" ]; then SCIENCE_LLVM_PREFIX="$candidate"; break; fi
    done
fi
: "${SCIENCE_LLVM_PREFIX:?LLVM 18 not found; set SCIENCE_LLVM_PREFIX}"
export SCIENCE_LLVM_PREFIX

cd "$(dirname "$0")"
cargo build --release -p science-rt -p sciencec --features llvm

mkdir -p "$PREFIX/lib/science" "$PREFIX/bin"
cp target/release/sciencec target/release/libscience_rt.a "$PREFIX/lib/science/"
cat > "$PREFIX/bin/sciencec" <<LAUNCHER
#!/bin/sh
exec "$PREFIX/lib/science/sciencec" "\$@"
LAUNCHER
chmod +x "$PREFIX/bin/sciencec"

echo "installed: $PREFIX/bin/sciencec"
case ":$PATH:" in
    *":$PREFIX/bin:"*) ;;
    *) echo "add it to your PATH:  export PATH=\"$PREFIX/bin:\$PATH\"" ;;
esac
