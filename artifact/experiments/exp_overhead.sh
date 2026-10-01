#!/usr/bin/env bash
# Figure 3 (Section 3.2): WASI interposition overhead.
#
# Claim: interposing a context on the hostcall path costs essentially nothing,
# because only the hostcall implementation is replaced -- the rest of the
# critical path is unchanged, and metrics leave the critical path entirely.
#
# Method: time fd_write / fd_pwrite / fd_read / fd_pread at 1 KiB from inside
# the guest, with an empty chain (an unmodified runtime) and with a context
# installed. The context is `syscall-batching` at num_writes=1, which disables
# batching, so what is left is interposition and nothing else.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

require_bin
require_guest hostcall_latency.wasm

ITERS="$(scale 20000 200000)"
BYTES=1024
REPS="$(scale 3 10)"

banner "Figure 3: interposition overhead (${BYTES} B ops, ${ITERS} iterations, ${REPS} repetitions)"

dir="$WORK/overhead"
rm -rf "$dir"
stage_guest hostcall_latency.wasm "$dir"

csv="$RESULTS/overhead.csv"
echo "arm,rep,op,bytes,iterations,mean_us,p50_us,p99_us,stddev_us" > "$csv"

for rep in $(seq 1 "$REPS"); do
    for arm in baseline peridot; do
        cfg="$dir/$arm.yaml"
        # The two configs differ only in the `contexts` list.
        if [[ "$arm" == "baseline" ]]; then
            ctx="contexts: []"
        else
            ctx=$'contexts:\n  - name: syscall-batching\n    config:\n      num_writes: 1'
        fi
        cat > "$cfg" <<EOF
args: ["hostcall_latency", "$BYTES", "$ITERS"]
$ctx
io: {demand: 150.0, max_bandwidth: 300.0}
cpu: {demand: 75.0, utilization: 0.85}
EOF
        log "rep $rep/$REPS  arm=$arm"
        "$PERIDOT_BIN" "$dir/hostcall_latency.wasm" "$cfg" --log-level warn 2>/dev/null \
            | grep '^op,' \
            | while IFS=, read -r _ op bytes iters mean p50 p99 sd; do
                  echo "$arm,$rep,$op,$bytes,$iters,$mean,$p50,$p99,$sd" >> "$csv"
              done
    done
done

log "wrote $csv"
