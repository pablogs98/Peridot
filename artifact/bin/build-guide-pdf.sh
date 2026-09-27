#!/usr/bin/env bash
# Renders the artifact guide (the repository README) to
# artifact/PERIDOT-ARTIFACT-GUIDE.pdf.
#
# Reviewers do not need this: the PDF is committed. It exists so the PDF can be
# regenerated from the README, which is the single source, rather than the two
# drifting apart.
#
# Needs pandoc and a LaTeX installation providing xelatex.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
OUT="$ROOT/artifact/PERIDOT-ARTIFACT-GUIDE.pdf"

# rsvg-convert is what pandoc shells out to for the SVG in the acknowledgements.
for tool in pandoc xelatex rsvg-convert; do
    command -v "$tool" >/dev/null || { echo "$tool not found on PATH" >&2; exit 1; }
done

# -f markdown-auto_identifiers+gfm_auto_identifiers-implicit_figures:
#   the markdown reader gives tables sensible relative column widths, which the
#   gfm reader does not, while gfm_auto_identifiers keeps heading anchors
#   identical to GitHub's so the guide's internal links resolve in both
#   renderings. implicit_figures is off because it would turn the funding logo
#   into a numbered, captioned "Figure 1", in a document whose other figure
#   references all point at the paper.
# --shift-heading-level-by=-1: the README's single H1 is the document title,
#   supplied below, so its H2s become the numbered top-level sections.
pandoc "$ROOT/README.md" \
    -f markdown-auto_identifiers+gfm_auto_identifiers-implicit_figures \
    -o "$OUT" \
    --pdf-engine=xelatex \
    --columns=90 \
    --toc --toc-depth=3 \
    --number-sections \
    --shift-heading-level-by=-1 \
    --include-in-header="$HERE/guide-header.tex" \
    -V documentclass=article \
    -V geometry:"a4paper,margin=2.4cm" \
    -V fontsize=10pt \
    -V colorlinks=true -V linkcolor=linkblue -V urlcolor=linkblue -V toccolor=black \
    -V title="Peridot: Artifact Guide" \
    -V subtitle="Middleware 2026 Artifact Evaluation" \
    -V date="$(date +%Y-%m-%d)"

echo "wrote $OUT"
