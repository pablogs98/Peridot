#!/usr/bin/env python3
"""Figure 7: WASI hostcall batching.

One line per I/O size, IOPS against batch size, with the batch_size=1 point
being the non-batching baseline each speedup is measured from.
"""
import argparse

import style


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--reference", action="store_true")
    args = ap.parse_args()
    if args.reference:
        style.use_reference()

    df = style.load("hostcall_batching.csv").dropna(subset=["mean_kiops"])

    fig, ax = style.plt.subplots(figsize=(6.4, 3.6))
    sizes = sorted(df["io_size"].unique())

    for colour, size in zip(style.SERIES, sizes):
        sub = df[df["io_size"] == size].sort_values("batch_size")
        label = f"{size} B" if size < 1024 else f"{size // 1024} KiB"
        ax.errorbar(sub["batch_size"], sub["mean_kiops"],
                    yerr=sub["stddev_kiops"], marker="o", markersize=4,
                    capsize=2, linewidth=1.5, color=colour, label=label)

    ax.set_xscale("log", base=2)
    ax.set_xlabel("Hostcall batch size (num_writes)")
    ax.set_ylabel("Write IOPS (thousands)")
    ax.set_title("Hostcall batching")
    ax.legend(title="I/O size", ncol=2)

    print("\nspeedup over no batching (num_writes = 1):")
    for size in sizes:
        sub = df[df["io_size"] == size].set_index("batch_size")["mean_kiops"]
        if 1 not in sub.index or not sub[1]:
            continue
        best = sub.max()
        print(f"  {size:5d} B: baseline {sub[1]:9.1f} K -> best {best:9.1f} K "
              f"at num_writes={sub.idxmax():4d}  ({best / sub[1]:.2f}x)")

    style.save(fig, "fig7_hostcall_batching")


if __name__ == "__main__":
    main()
