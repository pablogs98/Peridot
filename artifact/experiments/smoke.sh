#!/usr/bin/env bash
# Tier 0: proves the runtime, a context chain and a guest all work.
# No external service, no network, a few seconds.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

require_bin
require_guest io_writer.wasm

banner "Smoke test: counter context over a 64 MiB write"

dir="$WORK/smoke"
rm -rf "$dir"
stage_guest io_writer.wasm "$dir"
cp "$CONFIGS/smoke.yaml" "$dir/"

out="$("$PERIDOT_BIN" "$dir/io_writer.wasm" "$dir/smoke.yaml" --log-level info 2>&1)"
echo "$out"

echo
# The counter context prints the byte total it accumulated when the chain is
# torn down. Seeing it is the actual assertion: it means the chain was built,
# fd_write was intercepted, and shutdown ran.
if grep -q "Context chain: \[\"counter\"\]" <<<"$out" \
   && grep -q "Total bytes written" <<<"$out"; then
    total="$(grep -o 'Total bytes written: [0-9]*' <<<"$out" | grep -o '[0-9]*')"
    log "PASS - counter intercepted the write path, $total bytes counted"
    exit 0
fi

die "FAIL - expected a 'counter' chain and a byte total; see the output above"
