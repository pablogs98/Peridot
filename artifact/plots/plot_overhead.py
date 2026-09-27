#!/usr/bin/env python3
"""Figure 3: WASI interposition overhead.

Grouped bars, one pair per hostcall, baseline against Peridot. The claim is
that the two bars are the same height within noise, so the error bars are the
point of the figure rather than decoration.
"""
import argparse

import numpy as np
import style


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--reference", action="store_true",
                    help="plot the shipped sample data instead of a local run")
    args = ap.parse_args()
    if args.reference:
        style.use_reference()

    df = style.load("overhead.csv")
    ops = ["fd_write", "fd_pwrite", "fd_read", "fd_pread"]
    df = df[df["op"].isin(ops)]

    # Each repetition contributes one mean; we plot the mean of those and use
    # their spread as the error bar, so the bar reflects run-to-run variation
    # rather than within-run jitter.
    g = df.groupby(["arm", "op"])["mean_us"].agg(["mean", "std"]).reset_index()

    fig, ax = style.plt.subplots(figsize=(6.4, 3.2))
    x = np.arange(len(ops))
    width = 0.38

    for i, (arm, label, colour) in enumerate([
        ("baseline", "Unmodified Wasmtime", style.BASELINE),
        ("peridot", "Peridot (context installed)", style.PERIDOT),
    ]):
        sub = g[g["arm"] == arm].set_index("op").reindex(ops)
        ax.bar(x + (i - 0.5) * width, sub["mean"], width,
               yerr=sub["std"].fillna(0), capsize=3,
               label=label, color=colour, edgecolor="white", linewidth=0.6)

    ax.set_xticks(x)
    ax.set_xticklabels([o.replace("fd_", "") for o in ops])
    ax.set_xlabel("WASI hostcall (1 KiB per operation)")
    ax.set_ylabel("Duration (µs)")
    ax.set_title("Interposition overhead")
    ax.legend()

    # State the result numerically as well; a reviewer should not have to
    # eyeball whether the bars match.
    b = g[g["arm"] == "baseline"].set_index("op")["mean"]
    p = g[g["arm"] == "peridot"].set_index("op")["mean"]
    print("\nper-hostcall overhead (Peridot vs unmodified):")
    for op in ops:
        if op in b and op in p and b[op]:
            print(f"  {op:10s} {b[op]:7.3f} -> {p[op]:7.3f} us  "
                  f"({(p[op] / b[op] - 1) * 100:+.1f}%)")

    style.save(fig, "fig3_overhead")


if __name__ == "__main__":
    main()
