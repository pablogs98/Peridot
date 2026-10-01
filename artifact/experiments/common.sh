# Shared setup for every experiment script. Sourced, not executed.
#
# Layout inside the image (and when run from a checkout):
#   PERIDOT_BIN      the runtime            default: peridot on PATH
#   OVERSEER_BIN     the control plane      default: peridot-overseer on PATH
#   GUESTS           prebuilt .wasm modules default: ../guests
#   CONFIGS          YAML configs           default: ../configs
#   RESULTS          CSV output             default: ../results
#
# Scale: every experiment runs a reduced configuration by default so the whole
# suite fits in an artifact-evaluation time budget. Set PERIDOT_AE_FULL=1 for
# the paper-scale configuration, which needs the hardware described in the
# paper's experimental setup and takes hours.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"

PERIDOT_BIN="${PERIDOT_BIN:-$(command -v peridot || echo "$ROOT/../target/release/peridot")}"
OVERSEER_BIN="${OVERSEER_BIN:-$(command -v peridot-overseer || echo "$ROOT/../target/release/peridot-overseer")}"
GUESTS="${GUESTS:-$ROOT/guests}"
CONFIGS="${CONFIGS:-$ROOT/configs}"
RESULTS="${RESULTS:-$ROOT/results}"
WORK="${WORK:-${TMPDIR:-/tmp}/peridot-ae}"

FULL="${PERIDOT_AE_FULL:-0}"

mkdir -p "$RESULTS" "$WORK"

log()  { printf '\033[1;36m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[warn]\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31m[error]\033[0m %s\n' "$*" >&2; exit 1; }

# `scale small large` picks by PERIDOT_AE_FULL, so each script states both its
# reduced and its paper-scale parameter in one place.
scale() { [[ "$FULL" == "1" ]] && echo "$2" || echo "$1"; }

require_bin() {
    [[ -x "$PERIDOT_BIN" ]] || die "peridot binary not found at '$PERIDOT_BIN' (set PERIDOT_BIN)"
}

require_guest() {
    [[ -f "$GUESTS/$1" ]] || die "guest module '$1' not found in $GUESTS"
}

# Guests are given their own scratch directory because Peridot preopens the
# module's directory as '.', so anything a guest writes lands next to the
# .wasm unless we stage a copy elsewhere.
stage_guest() {
    local guest="$1" dir="$2"
    mkdir -p "$dir"
    cp "$GUESTS/$guest" "$dir/"
    [[ -d "$GUESTS/resources" ]] && cp -r "$GUESTS/resources" "$dir/" || true
}

banner() {
    echo
    printf '\033[1m%s\033[0m\n' "$1"
    printf '%s\n' "$(printf '%.0s-' {1..70})"
}
