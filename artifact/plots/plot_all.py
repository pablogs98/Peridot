#!/usr/bin/env python3
"""Regenerates every figure whose data is present, and says what is missing."""
import argparse
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
SCRIPTS = [
    ("fig3_overhead", "overhead.csv", "plot_overhead.py"),
    ("fig5_io", "io.csv", "plot_io.py"),
    ("fig6_batch", "batch.csv", "plot_batch.py"),
    ("fig7_hostcall_batching", "hostcall_batching.csv", "plot_hostcall_batching.py"),
]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--reference", action="store_true",
                    help="plot the shipped sample data instead of a local run")
    args = ap.parse_args()

    results = HERE.parent / "results" / ("reference" if args.reference else "")
    missing, failed = [], []

    for name, csv, script in SCRIPTS:
        if not (results / csv).exists():
            missing.append((name, csv))
            continue
        print(f"\n{'=' * 70}\n{name}\n{'=' * 70}")
        cmd = [sys.executable, str(HERE / script)] + (["--reference"] if args.reference else [])
        if subprocess.run(cmd).returncode != 0:
            failed.append(name)

    print()
    if missing:
        print("skipped (no data yet, run the matching experiment):")
        for name, csv in missing:
            print(f"  {name:26s} needs results/{csv}")
    if failed:
        print("failed:", ", ".join(failed))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
