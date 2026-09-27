#!/usr/bin/env bash
# Figure 5 (Section 4.1): dynamic I/O resource provisioning.
#
# Claim: with Peridot, disk bandwidth is allocated per module according to a
# declared demand and is recalculated automatically as modules start and
# finish -- neither of which an unmodified runtime can do, where every module
# simply gets the same share.
#
# Method: four collocated modules write concurrently under a global 1 Gbps
# policy with per-module demands of 100/200/300/400 Mbps. Each module writes a
# different volume, so they finish at staggered times and the reallocation is
# visible as the survivors speed up.
#
#   peridot arm   the Overseer runs max-min fair share and pushes `token_rate`
#                 to each module's token bucket once per interval.
#   baseline arm  no Overseer and a fixed, equal 250 Mbps bucket per module.
#                 This stands in for the paper's "scheduling-free runtime with
#                 1 Gbps allocated": it reproduces the fixed aggregate cap and
#                 the equal split without needing cgroups or a privileged
#                 container. See README, "Deviations from the paper".
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

require_bin
require_guest io_writer.wasm
[[ -x "$OVERSEER_BIN" ]] || die "overseer binary not found at '$OVERSEER_BIN' (set OVERSEER_BIN)"

PORT="${OVERSEER_PORT:-50051}"
TOTAL_BW=125000000          # 1 Gbps in bytes/s
# The paper's demands, which sum to exactly the 1 Gbps policy. Max-min fair
# share therefore grants every module precisely its demand, and a module
# finishing frees capacity that nobody else is asking for -- so the allocation
# is demand-proportional and flat, rather than stepping up over time.
#
# To watch the Overseer actually reallocate, oversubscribe the policy, e.g.
#     IO_DEMANDS_MBPS="400 400 400 400" experiments/exp_io.sh
# Every module is then clamped below its demand, and each time one finishes
# the survivors' clamps loosen.
DEMANDS_MBPS="${IO_DEMANDS_MBPS:-100 200 300 400}"
# Paper volumes are 60/30/22/18 GiB. The reduced run keeps the ratios and the
# staggered finish order, at 1/20th the volume (~4 minutes instead of ~80).
VOLUMES_MIB="${IO_VOLUMES_MIB:-$(scale "3072 1536 1126 922" "61440 30720 22528 18432")}"

banner "Figure 5: dynamic I/O provisioning (4 modules, 1 Gbps global)"
log "demands (Mbps): $DEMANDS_MBPS"
log "volumes (MiB):  $VOLUMES_MIB"

dir="$WORK/io"
rm -rf "$dir"

csv="$RESULTS/io.csv"
echo "arm,module,demand_mbps,t_seconds,written_bytes" > "$csv"

run_arm() {
    local arm="$1" overseer_pid="" i=0
    local -a pids=()

    if [[ "$arm" == "peridot" ]]; then
        log "starting overseer on port $PORT (global $TOTAL_BW B/s)"
        "$OVERSEER_BIN" "$PORT" "$TOTAL_BW" -u 1 -l warn >"$dir/overseer.log" 2>&1 &
        overseer_pid=$!
        # Give the gRPC server a moment to bind before modules register.
        sleep 2
        kill -0 "$overseer_pid" 2>/dev/null || die "overseer failed to start; see $dir/overseer.log"
    fi

    for demand in $DEMANDS_MBPS; do
        i=$((i + 1))
        local vol; vol="$(echo "$VOLUMES_MIB" | cut -d' ' -f"$i")"
        local mdir="$dir/$arm/module$i"
        stage_guest io_writer.wasm "$mdir"

        # Bytes/s from Mbps. In the baseline arm every module is pinned to an
        # equal quarter of the global cap instead of to its own demand.
        local bps=$(( demand * 1000000 / 8 ))
        local bucket=$bps
        local overseer_line=""
        if [[ "$arm" == "peridot" ]]; then
            overseer_line="overseer_address: \"http://127.0.0.1:$PORT\""
        else
            bucket=$(( TOTAL_BW / 4 ))
        fi

        cat > "$mdir/config.yaml" <<EOF
$overseer_line
args: ["io_writer", "out.bin", "$vol"]

contexts:
  - name: token
    config:
      max_bandwidth: $bucket

io:
  demand: $bps.0
  max_bandwidth: $TOTAL_BW.0

cpu:
  demand: 75.0
  utilization: 0.85
EOF
        log "[$arm] module$i  demand=${demand}Mbps  volume=${vol}MiB"
        "$PERIDOT_BIN" "$mdir/io_writer.wasm" "$mdir/config.yaml" --log-level warn \
            >"$mdir/out.log" 2>&1 &
        pids+=($!)
    done

    log "[$arm] $((i)) modules running; waiting for completion"
    local rc=0
    for p in "${pids[@]}"; do wait "$p" || rc=1; done
    [[ "$rc" == "0" ]] || warn "[$arm] at least one module exited non-zero; check $dir/$arm/*/out.log"

    [[ -n "$overseer_pid" ]] && { kill "$overseer_pid" 2>/dev/null || true; wait "$overseer_pid" 2>/dev/null || true; }

    # Each module's stdout carries `progress t=<s> written=<bytes>` once a
    # second; that is the time series the figure plots.
    i=0
    for demand in $DEMANDS_MBPS; do
        i=$((i + 1))
        grep -o 'progress t=[0-9.]* written=[0-9]*' "$dir/$arm/module$i/out.log" 2>/dev/null \
            | sed 's/progress t=//; s/ written=/,/' \
            | while IFS=, read -r t w; do
                  echo "$arm,module$i,$demand,$t,$w" >> "$csv"
              done
    done
}

run_arm baseline
run_arm peridot

log "wrote $csv"
