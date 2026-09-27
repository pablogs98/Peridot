#!/usr/bin/env python3
"""Table 1 and Figure 6: storage tiering and batching.

Figure 6 is the throughput-against-batch-size comparison. Table 1 is printed
to stdout rather than drawn, because that is how it appears in the paper.
"""
import argparse

import style


def print_table1():
    try:
        df = style.load("storage_table1.csv")
    except SystemExit as e:
        print(e)
        return
    df = df.dropna(subset=["throughput_mbs"])
    if df.empty:
        print("no Table 1 data")
        return

    g = df.groupby("setup")["throughput_mbs"].agg(["mean", "std"])
    # The paper's speedup column is relative to s3fs, which this artifact does
    # not run; we normalise to the slowest arm present and say so.
    base_name = g["mean"].idxmin()
    base = g.loc[base_name, "mean"]

    print("\nTable 1: throughput per setup (batching disabled)")
    print(f"{'Setup':<16}{'mean MB/s':>11}{'sigma':>9}{'speedup':>10}")
    print("-" * 46)
    for setup, row in g.sort_values("mean").iterrows():
        print(f"{setup:<16}{row['mean']:>11.2f}{(row['std'] or 0):>9.2f}"
              f"{row['mean'] / base:>9.2f}x")
    print(f"(speedup relative to {base_name}; the paper normalises to s3fs, "
          f"which this artifact does not run)")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--reference", action="store_true")
    args = ap.parse_args()
    if args.reference:
        style.use_reference()

    print_table1()

    df = style.load("batch.csv").dropna(subset=["throughput_mbs"])
    g = df.groupby(["arm", "batch_size"])["throughput_mbs"].agg(["mean", "std"]).reset_index()

    fig, ax = style.plt.subplots(figsize=(6.4, 3.6))
    for arm, label, colour in [
        ("wasmtime", "Wasmtime + external batcher", style.BASELINE),
        ("peridot", "Peridot (batch context)", style.PERIDOT),
    ]:
        sub = g[g["arm"] == arm].sort_values("batch_size")
        if sub.empty:
            continue
        ax.errorbar(sub["batch_size"], sub["mean"], yerr=sub["std"].fillna(0),
                    marker="o", markersize=4, capsize=2, linewidth=1.5,
                    color=colour, label=label)

    ax.set_xscale("log", base=2)
    ax.set_xlabel("Batch size (images per Parquet object)")
    ax.set_ylabel("Throughput (MB/s)")
    ax.set_title("ImageNet pre-processing, batched to Parquet on S3")
    ax.legend()

    print("\nFigure 7: Peridot vs Wasmtime by batch size")
    piv = g.pivot(index="batch_size", columns="arm", values="mean")
    if {"peridot", "wasmtime"} <= set(piv.columns):
        for bs, row in piv.iterrows():
            if row["wasmtime"]:
                print(f"  batch={bs:4d}  wasmtime {row['wasmtime']:7.2f}  "
                      f"peridot {row['peridot']:7.2f}  "
                      f"({row['peridot'] / row['wasmtime']:.2f}x)")

    style.save(fig, "fig6_batch")


if __name__ == "__main__":
    main()
