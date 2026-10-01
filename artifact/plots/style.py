"""Shared figure style for the Peridot artifact.

Every plot script imports from here so the figures form one consistent set,
and so the paper-facing defaults (font sizes, colours, output format) live in
one place rather than being repeated five times.
"""
from pathlib import Path

import matplotlib
matplotlib.use("Agg")           # no display inside a container
import matplotlib.pyplot as plt

ROOT = Path(__file__).resolve().parent.parent
RESULTS = ROOT / "results"
FIGURES = ROOT / "figures"

# Colour-blind-safe; also distinguishable when printed in greyscale, which
# matters because these end up in a PDF that reviewers may print.
BASELINE = "#4C72B0"    # the unmodified-runtime arm
PERIDOT = "#DD8452"     # the Peridot arm
ACCENT = "#55A868"
SERIES = [BASELINE, PERIDOT, ACCENT, "#C44E52", "#8172B3", "#937860"]

plt.rcParams.update({
    "figure.dpi": 150,
    "savefig.dpi": 300,
    "savefig.bbox": "tight",
    "font.size": 10,
    "axes.titlesize": 11,
    "axes.labelsize": 10,
    "legend.fontsize": 9,
    "legend.frameon": False,
    "axes.grid": True,
    "grid.alpha": 0.3,
    "grid.linestyle": "--",
    "axes.spines.top": False,
    "axes.spines.right": False,
})


def load(name):
    """Reads a results CSV, with a message a reviewer can act on if it is absent."""
    import pandas as pd

    path = RESULTS / name
    if not path.exists():
        raise SystemExit(
            f"missing {path}\n"
            f"Run the matching experiment first, or pass --reference to plot the\n"
            f"sample data from results/reference/."
        )
    return pd.read_csv(path)


def use_reference():
    """Repoints RESULTS at the shipped reference data."""
    global RESULTS
    RESULTS = ROOT / "results" / "reference"


def save(fig, stem):
    FIGURES.mkdir(exist_ok=True)
    for ext in ("pdf", "png"):
        out = FIGURES / f"{stem}.{ext}"
        fig.savefig(out)
        print(f"wrote {out}")
    plt.close(fig)
