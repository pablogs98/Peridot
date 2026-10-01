#!/usr/bin/env bash
# Table 1 and Figure 6 (Section 4.2): storage tiering and batching.
#
# Claim: performing the offload inside the Wasm runtime beats an out-of-runtime
# FUSE mount, and making it asynchronous is what actually matters -- the async
# context overlaps tensor computation with I/O and pulls far ahead of the
# synchronous one. Batching the results into Parquet objects inside the runtime
# beats doing it from an external process at every batch size.
#
# The legacy application is the ImageNet pre-processing guest: it opens, writes
# and closes ordinary files, and is never told that any of this is happening.
#
# Which arms this script runs, and which it does not:
#   s3-async-ctx    yes  -- the `s3` context, uploads spawned (the default)
#   s3-sync-ctx     yes  -- the `s3` context with `sync: true`
#   parquet batch   yes  -- the `batch` context, against `tensor_batcher`
#   geds-ctx        no   -- needs a native IBM GEDS install; out of scope
#   s3fs            no   -- needs FUSE and a privileged container
# See README, "What this artifact does and does not reproduce".
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

require_bin
require_guest imagenet-preprocessing.wasm

: "${S3_ENDPOINT:?set S3_ENDPOINT (docker compose sets it for you)}"
: "${S3_BUCKET:=peridot-ae}"
: "${AWS_ACCESS_KEY_ID:?set AWS_ACCESS_KEY_ID}"
: "${AWS_SECRET_ACCESS_KEY:?set AWS_SECRET_ACCESS_KEY}"
export AWS_REGION="${AWS_REGION:-us-east-1}"
export AWS_ENDPOINT_URL="$S3_ENDPOINT"

BATCHER_BIN="${BATCHER_BIN:-$(command -v tensor_batcher || echo "$ROOT/../target/release/tensor_batcher")}"

NUM_IMAGES="$(scale 256 2048)"
REPS="$(scale 3 10)"
BATCH_SIZES="2 4 8 16 32 64 128"
TENSOR_BYTES=602112          # 224 x 224 x 3 channels x 4 bytes

banner "Table 1 + Figure 6: storage tiering and batching"
log "endpoint=$S3_ENDPOINT bucket=$S3_BUCKET images=$NUM_IMAGES reps=$REPS"

# The Wasmtime arm's finish line is "N more Parquet objects exist", which only
# works from a known starting point.
# The `batch` context and tensor_batcher both write batch_<uuid>.parquet at
# the bucket root, so the two arms need separate buckets for the object
# counts to be attributable.
WASMTIME_BUCKET="${S3_BUCKET}-wasmtime"
# ~600 KB per tensor, two arms in flight, plus the object store itself.
avail_mb="$(df -Pm "$WORK" | awk 'NR==2 {print $4}')"
if [[ -n "$avail_mb" && "$avail_mb" -lt 4096 ]]; then
    warn "only ${avail_mb} MiB free on $WORK; the storage sweep wants ~4 GiB"
    warn "on Docker Desktop, raise the disk limit or run: docker system prune -af"
fi

log "emptying s3://$S3_BUCKET and s3://$WASMTIME_BUCKET"
S3_BUCKET="$S3_BUCKET" python3 "$HERE/s3_reset.py" || die "could not reset the bucket; is the object store up?"
S3_BUCKET="$WASMTIME_BUCKET" python3 "$HERE/s3_reset.py" || die "could not reset the wasmtime bucket"

dir="$WORK/storage"
rm -rf "$dir"

# Every arm is timed the same way: wall-clock from launching the runtime to
# the point where all of this run's data is durably in the object store. The
# guest's own "Preprocessed images in N ms" line is deliberately not used --
# for the asynchronous arms it stops before the uploads do.
total_bytes=$(( NUM_IMAGES * TENSOR_BYTES ))
now_s() { python3 -c 'import time; print(time.time())'; }
elapsed_since() { python3 -c "print(f'{$(now_s) - $1:.3f}')"; }
throughput() { python3 -c "print(f'{$total_bytes / max($1, 1e-9) / 1e6:.2f}')"; }

# ---------------------------------------------------------------- Table 1 ---
table="$RESULTS/storage_table1.csv"
echo "setup,rep,images,seconds,throughput_mbs" > "$table"

for setup in s3-sync-ctx s3-async-ctx; do
    for rep in $(seq 1 "$REPS"); do
        mdir="$dir/table/$setup/$rep"
        stage_guest imagenet-preprocessing.wasm "$mdir"
        sync_flag=$([[ "$setup" == "s3-sync-ctx" ]] && echo true || echo false)
        cat > "$mdir/config.yaml" <<EOF
args: ["imagenet-preprocessing", "false", "$NUM_IMAGES", "s3://$S3_BUCKET/tensors/$setup/$rep"]

contexts:
  - name: s3
    config:
      sync: $sync_flag

io: {demand: 150.0, max_bandwidth: 300.0}
cpu: {demand: 75.0, utilization: 0.85}
EOF
        log "[table1] $setup rep $rep/$REPS"
        t0="$(now_s)"
        "$PERIDOT_BIN" "$mdir/imagenet-preprocessing.wasm" "$mdir/config.yaml" \
            --log-level warn >"$mdir/out.log" 2>&1 || warn "$setup rep $rep exited non-zero"
        secs="$(elapsed_since "$t0")"
        echo "$setup,$rep,$NUM_IMAGES,$secs,$(throughput "$secs")" >> "$table"
        # Each run writes NUM_IMAGES tensors of ~600 KB; across the sweep that
        # is several GB, which is enough to fill a container's writable layer.
        rm -rf "$mdir"
    done
done
log "wrote $table"

# --------------------------------------------------------------- Figure 6 ---
#
# Both arms are timed to the same finish line: every batch durably in the
# object store. Timing only the guest would flatter the Wasmtime arm, whose
# batcher is a separate process that keeps uploading after the guest exits,
# so its work would fall outside the measurement entirely.
csv="$RESULTS/batch.csv"
echo "arm,batch_size,rep,images,seconds,throughput_mbs" > "$csv"

for batch in $BATCH_SIZES; do
    # Number of Parquet objects a complete run must produce.
    expected=$(( (NUM_IMAGES + batch - 1) / batch ))

    for rep in $(seq 1 "$REPS"); do
        # -------- Peridot arm: the `batch` context uploads from inside the
        # runtime, and drains pending uploads during shutdown, so the runtime
        # exiting already means "everything is in S3".
        mdir="$dir/batch/peridot/$batch/$rep"
        stage_guest imagenet-preprocessing.wasm "$mdir"
        mkdir -p "$mdir/tensors/tensors"
        cat > "$mdir/config.yaml" <<EOF
args: ["imagenet-preprocessing", "false", "$NUM_IMAGES", "./tensors/tensors"]

contexts:
  - name: batch
    config:
      batch_size: $batch
      bucket: "$S3_BUCKET"

io: {demand: 150.0, max_bandwidth: 300.0}
cpu: {demand: 75.0, utilization: 0.85}
EOF
        S3_BUCKET="$S3_BUCKET" python3 "$HERE/s3_reset.py" >/dev/null

        log "[fig7] peridot batch=$batch rep $rep/$REPS"
        t0="$(now_s)"
        "$PERIDOT_BIN" "$mdir/imagenet-preprocessing.wasm" "$mdir/config.yaml" \
            --log-level warn >"$mdir/out.log" 2>&1 || warn "peridot batch=$batch rep=$rep non-zero"
        secs="$(elapsed_since "$t0")"
        echo "peridot,$batch,$rep,$NUM_IMAGES,$secs,$(throughput "$secs")" >> "$csv"
        rm -rf "$mdir"

        # -------- Wasmtime arm: no context. The guest writes tensors to local
        # disk; a separate tensor_batcher process packs and uploads them. The
        # clock stops once the expected number of Parquet objects has landed.
        wdir="$dir/batch/wasmtime/$batch/$rep"
        stage_guest imagenet-preprocessing.wasm "$wdir"
        mkdir -p "$wdir/tensors/tensors"
        cat > "$wdir/config.yaml" <<EOF
args: ["imagenet-preprocessing", "false", "$NUM_IMAGES", "./tensors/tensors"]

contexts: []

io: {demand: 150.0, max_bandwidth: 300.0}
cpu: {demand: 75.0, utilization: 0.85}
EOF
        S3_BUCKET="$WASMTIME_BUCKET" python3 "$HERE/s3_reset.py" >/dev/null

        batcher_pid=""
        if [[ -x "$BATCHER_BIN" ]]; then
            # tensor_batcher writes its intermediate Parquet files into its
            # working directory, so give it one inside the scratch tree
            # instead of letting it litter wherever the script was invoked.
            mkdir -p "$wdir/batcher"
            ( cd "$wdir/batcher" && exec "$BATCHER_BIN" \
                --bucket "$WASMTIME_BUCKET" --batch-size "$batch" \
                --input-dir "$wdir/tensors/tensors" ) >"$wdir/batcher.log" 2>&1 &
            batcher_pid=$!
        else
            warn "tensor_batcher not found at '$BATCHER_BIN'; skipping the Wasmtime arm"
            echo "wasmtime,$batch,$rep,$NUM_IMAGES,," >> "$csv"
            continue
        fi

        log "[fig7] wasmtime batch=$batch rep $rep/$REPS"
        t0="$(now_s)"
        "$PERIDOT_BIN" "$wdir/imagenet-preprocessing.wasm" "$wdir/config.yaml" \
            --log-level warn >"$wdir/out.log" 2>&1 || warn "wasmtime batch=$batch rep=$rep non-zero"
        guest_secs="$(elapsed_since "$t0")"

        # The bucket was emptied just before this run, so the finish line is
        # this run's objects alone. s3_wait reports how long the batcher kept
        # working after the guest exited; its quiescence window is detection
        # latency, not work, and is excluded from that figure.
        wait_secs="$(WAIT_T0="$(now_s)" S3_BUCKET="$WASMTIME_BUCKET" python3 "$HERE/s3_wait.py" \
                        "batch_" "$expected" 120 2>/dev/null | head -1)"
        if [[ -z "$wait_secs" ]]; then
            warn "wasmtime batch=$batch rep=$rep: objects did not settle within 120s"
            wait_secs=0
        fi
        secs="$(python3 -c "print(f'{$guest_secs + $wait_secs:.3f}')")"

        kill "$batcher_pid" 2>/dev/null || true
        wait "$batcher_pid" 2>/dev/null || true

        echo "wasmtime,$batch,$rep,$NUM_IMAGES,$secs,$(throughput "$secs")" >> "$csv"
        rm -rf "$wdir"
    done
done

log "wrote $csv"
