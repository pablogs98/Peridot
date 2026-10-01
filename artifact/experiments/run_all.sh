#!/usr/bin/env bash
# Runs every reproducible experiment, then regenerates every figure.
#
# Default (reduced) scale is sized for an artifact-evaluation sitting.
# PERIDOT_AE_FULL=1 switches to the paper's parameters everywhere.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

started="$(date +%s)"
declare -a ran=() skipped=()

banner "Peridot artifact: full run"
if [[ "$FULL" == "1" ]]; then
    log "PAPER SCALE (PERIDOT_AE_FULL=1) - this takes hours"
else
    log "reduced scale - set PERIDOT_AE_FULL=1 for the paper's parameters"
fi

run_step() {
    local name="$1" script="$2"
    banner "$name"
    if "$HERE/$script"; then
        ran+=("$name")
    else
        warn "$name failed"
        skipped+=("$name (failed)")
    fi
}

run_step "Smoke test"                                 smoke.sh
run_step "Figure 3 - interposition overhead"          exp_overhead.sh
run_step "Figure 7 - hostcall batching"               exp_hostcall_batching.sh
run_step "Figure 5 - dynamic I/O provisioning"        exp_io.sh

# The storage experiments are the only ones needing an object store. Skipped
# with an explanation rather than failing, so a reviewer without one still
# gets the other three figures.
if [[ -n "${S3_ENDPOINT:-}" ]]; then
    run_step "Table 1 + Figure 6 - storage tiering and batching" exp_storage.sh
else
    warn "S3_ENDPOINT is unset; skipping the storage experiments"
    warn "Bring up the bundled object store with: docker compose up -d minio"
    skipped+=("Table 1 + Figure 6 (no S3_ENDPOINT)")
fi

banner "Generating figures"
python3 "$ROOT/plots/plot_all.py" || warn "some figures could not be generated"

banner "Summary"
printf 'completed in %d min\n\n' $(( ($(date +%s) - started) / 60 ))
for r in "${ran[@]}"; do printf '  ran      %s\n' "$r"; done
for s in "${skipped[@]}"; do printf '  skipped  %s\n' "$s"; done
echo
log "CSVs:    $RESULTS"
log "figures: $ROOT/figures"
