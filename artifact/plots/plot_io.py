#!/usr/bin/env python3
"""Figure 5: dynamic I/O resource provisioning.

Two stacked subplots sharing an x axis, matching the paper's layout: the
unmodified runtime on top, Peridot below. Each line is one module's achieved
bandwidth over time, derived from its cumulative-bytes time series.

What to look for: in the upper subplot the four lines sit on top of each other
regardless of declared demand; in the lower they separate in proportion to it,
and step upward as modules finish and their share is redistributed.
"""
import argparse

import style


# Samples closer together than this are dropped before differentiating. A
# guest reports roughly once a second; a pair of points a few milliseconds
# apart carries no real interval, and dividing by it produces a spike that
# dominates the y axis and hides the actual allocation.
MIN_INTERVAL_S = 0.2


def bandwidth(sub):
    """Differentiates the cumulative byte counts into Mbps."""
    sub = sub.sort_values("t_seconds")
    dt = sub["t_seconds"].diff()
    dbytes = sub["written_bytes"].diff()
    mbps = (dbytes * 8 / dt) / 1e6
    keep = dt >= MIN_INTERVAL_S
    return sub["t_seconds"][keep], mbps[keep]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--reference", action="store_true")
    args = ap.parse_args()
    if args.reference:
        style.use_reference()

    df = style.load("io.csv")

    fig, axes = style.plt.subplots(2, 1, figsize=(6.4, 5.0), sharex=True)
    arms = [("baseline", "Unmodified Wasmtime"), ("peridot", "Peridot")]

    for ax, (arm, title) in zip(axes, arms):
        arm_df = df[df["arm"] == arm]
        for colour, module in zip(style.SERIES, sorted(arm_df["module"].unique())):
            sub = arm_df[arm_df["module"] == module]
            demand = int(sub["demand_mbps"].iloc[0])
            t, mbps = bandwidth(sub)
            if len(t) == 0:
                # Finished inside one reporting interval: no time series to
                # draw, but its mean still appears in the printed summary.
                continue
            ax.plot(t, mbps, marker="." if len(t) < 8 else None,
                    linewidth=1.4, color=colour,
                    label=f"{module} ({demand} Mbps)")
        ax.set_title(title, loc="left")
        ax.set_ylabel("Bandwidth (Mbps)")
        ax.legend(ncol=2, fontsize=8)

    axes[-1].set_xlabel("Time (s)")
    fig.suptitle("Dynamic I/O provisioning under a 1 Gbps policy", y=0.98)

    print("\nmean achieved bandwidth per module:")
    for arm, _ in arms:
        arm_df = df[df["arm"] == arm]
        print(f"  {arm}:")
        for module in sorted(arm_df["module"].unique()):
            sub = arm_df[arm_df["module"] == module].sort_values("t_seconds")
            if len(sub) < 2:
                continue
            total = sub["written_bytes"].iloc[-1]
            elapsed = sub["t_seconds"].iloc[-1]
            demand = int(sub["demand_mbps"].iloc[0])
            print(f"    {module}  demand={demand:4d} Mbps  "
                  f"achieved={total * 8 / elapsed / 1e6:7.1f} Mbps")

    style.save(fig, "fig5_io")


if __name__ == "__main__":
    main()
