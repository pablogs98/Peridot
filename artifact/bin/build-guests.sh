#!/usr/bin/env bash
# Rebuilds every guest module shipped in artifact/guests/.
#
# Reviewers do NOT need this: the .wasm files are committed and baked into the
# image. It exists so the binaries in guests/ can be regenerated from source,
# which is what makes them auditable rather than opaque blobs.
#
# Requires wasi-sdk (WASI_SDK_PATH, default /opt/wasi-sdk) and, for the
# ImageNet app, the wasm32-wasip1 Rust target.
set -euo pipefail

WASI_SDK_PATH="${WASI_SDK_PATH:-/opt/wasi-sdk}"
CLANG="$WASI_SDK_PATH/bin/clang"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
OUT="$HERE/guests"

if [[ ! -x "$CLANG" ]]; then
    echo "wasi-sdk clang not found at $CLANG" >&2
    echo "Install wasi-sdk and set WASI_SDK_PATH." >&2
    exit 1
fi

echo "wasi-sdk: $(cat "$WASI_SDK_PATH/VERSION" 2>/dev/null | head -1)"
mkdir -p "$OUT/resources"

# C guests. Each is a single translation unit; -lm is needed by the two that
# use sqrt/pow.
for guest in hostcall_latency iops io_writer read_write helloworld; do
    src="$REPO/c-wasm/$guest/main.c"
    [[ -f "$src" ]] || { echo "missing $src" >&2; exit 1; }
    echo "building $guest.wasm"
    "$CLANG" --target=wasm32-wasip1 -O2 "$src" -lm -o "$OUT/$guest.wasm"
done

# The ImageNet pre-processing app is the paper's "legacy application". It is a
# workspace member, so it builds against the same lockfile as everything else.
echo "building imagenet-preprocessing.wasm"
cargo build --release --locked --manifest-path "$REPO/Cargo.toml" \
    --target wasm32-wasip1 -p imagenet-preprocessing
cp "$REPO/target/wasm32-wasip1/release/imagenet-preprocessing.wasm" "$OUT/"

# Its input images travel with it: the guest reads ./resources at run time.
cp "$REPO/crates/contexts/imagenet-preprocessing/resources/"*.jpg \
   "$REPO/crates/contexts/imagenet-preprocessing/resources/"*.png "$OUT/resources/"

echo
echo "guests rebuilt into $OUT:"
ls -l "$OUT"/*.wasm
