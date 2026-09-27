#!/usr/bin/env bash
# Figure 7 (Section 4.3): WASI hostcall batching.
#
# Claim: buffering fd_write hostcalls in user space and flushing them as one
# write syscall raises write IOPS by up to 4.90x, with the gain concentrated
# at small I/O sizes, and batch sizes above 128 giving little further benefit.
#
# Method: a guest performs sequential writes of a fixed size; the
# `syscall-batching` context is configured with num_writes from 1 (batching
# off, the baseline) up to 1024. One data point per (I/O size, batch size).
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

require_bin
require_guest iops.wasm

IO_SIZES="64 256 1024 4096"
BATCH_SIZES="1 2 4 8 16 32 64 128 256 512 1024"
WRITES="$(scale 200000 4194304)"   # paper uses DOT_NUM = 0x400000
RUNS="$(scale 3 30)"               # paper repeats each configuration 30 times

banner "Figure 7: hostcall batching (${WRITES} writes x ${RUNS} runs per point)"
log "I/O sizes: $IO_SIZES bytes"
log "batch sizes: $BATCH_SIZES"

dir="$WORK/hostcall_batching"
rm -rf "$dir"
stage_guest iops.wasm "$dir"

csv="$RESULTS/hostcall_batching.csv"
echo "io_size,batch_size,writes_per_run,runs,mean_kiops,stddev_kiops" > "$csv"

total=$(( $(wc -w <<<"$IO_SIZES") * $(wc -w <<<"$BATCH_SIZES") ))
i=0
for size in $IO_SIZES; do
    for batch in $BATCH_SIZES; do
        i=$((i + 1))
        cfg="$dir/b_${size}_${batch}.yaml"
        cat > "$cfg" <<EOF
args: ["iops", "$size", "$WRITES", "$RUNS"]
contexts:
  - name: syscall-batching
    config:
      num_writes: $batch
io: {demand: 150.0, max_bandwidth: 300.0}
cpu: {demand: 75.0, utilization: 0.85}
EOF
        log "[$i/$total] io_size=${size}B batch=${batch}"
        # The guest prints per-run lines and one summary line; we keep the
        # summary, which already carries the mean and standard deviation the
        # figure plots as error bars.
        summary="$("$PERIDOT_BIN" "$dir/iops.wasm" "$cfg" --log-level warn 2>/dev/null \
                   | grep '^summary,' || true)"
        if [[ -z "$summary" ]]; then
            warn "no summary for io_size=$size batch=$batch; recording as empty"
            echo "$size,$batch,$WRITES,$RUNS,," >> "$csv"
            continue
        fi
        IFS=, read -r _ bytes writes runs mean sd <<<"$summary"
        echo "$bytes,$batch,$writes,$runs,$mean,$sd" >> "$csv"
    done
done

log "wrote $csv"
