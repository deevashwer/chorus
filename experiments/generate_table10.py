#!/usr/bin/env python3
"""Generate Table 10: Parameter selection (n, threshold) vs. corruption and
offline fractions.

This is a pure computation — no benchmark logs are needed.
Parameters are read from config.json (experiments.table10).

The two security bounds (privacy / availability) are quadratic inequalities
with closed-form solutions, so we solve them analytically (no sympy needed).

Usage:
    python3 experiments/generate_table10.py --output-dir results/table10/<timestamp>
"""

import argparse
import json
import math
import sys
from pathlib import Path


REPO_DIR = Path(__file__).resolve().parent.parent
CONFIG = json.loads((REPO_DIR / "config.json").read_text())
TABLE10_CFG = CONFIG["experiments"]["table10"]

COMP_SEC = TABLE10_CFG["comp_sec"]
STAT_SEC = TABLE10_CFG["stat_sec"]
N_CLIENTS = TABLE10_CFG["n_clients"]
CORRUPTION_FRACTIONS = TABLE10_CFG["corruption_fractions"]
AVAILABILITY_FRACTIONS = TABLE10_CFG["availability_fractions"]

LOG2E = math.log2(math.e)


def privacy_bound(n: int, f: float):
    a = LOG2E * f * n
    if a <= 0:
        return None, None
    disc = COMP_SEC * COMP_SEC + 8 * a * COMP_SEC
    eps = (COMP_SEC + math.sqrt(disc)) / (2 * a)
    t = math.ceil(n * f * (1 + eps))
    return t, eps


def availability_bound(n: int, f_avail: float):
    a = LOG2E * (1 - f_avail) * n
    if a <= 0:
        return None, None
    eps = math.sqrt(2 * STAT_SEC / a)
    t = math.ceil(n * (1 - f_avail) * (1 - eps))
    return t, eps


def compute_params():
    """For each (corruption_f, availability_f) find the smallest n such that
    the privacy threshold t_priv <= availability threshold t_avail <= n."""
    results = {}
    for avail_f in AVAILABILITY_FRACTIONS:
        n = 1
        for corrupt_f in CORRUPTION_FRACTIONS:
            while True:
                t_avail, _ = availability_bound(n, avail_f)
                t_priv, _ = privacy_bound(n, corrupt_f)
                if (t_priv is not None and t_avail is not None
                        and t_priv <= t_avail <= n):
                    results[(corrupt_f, avail_f)] = (n, t_priv)
                    print(f"    corrupt={corrupt_f:.2f}  avail={avail_f:.2f}  →  n={n}, t={t_priv}")
                    break
                n += 1
    return results


def generate(output_dir: Path):
    print("  Computing parameters ...")
    params = compute_params()

    fail_pcts = [int(f * 100) for f in AVAILABILITY_FRACTIONS]
    ncols = len(AVAILABILITY_FRACTIONS)
    col_spec = "l" + "r" * ncols

    header = " & ".join(f"$\\fail = {p}\\%$" for p in fail_pcts)

    rows = []
    for corrupt_f in CORRUPTION_FRACTIONS:
        pct = int(corrupt_f * 100)
        cells = []
        for avail_f in AVAILABILITY_FRACTIONS:
            key = (corrupt_f, avail_f)
            if key in params:
                n, t = params[key]
                cells.append(f"$({n}, {t})$")
            else:
                cells.append("---")
        rows.append(f"$\\comp = {pct}\\%$  & {' & '.join(cells)} \\\\ ")

    lines = [
        r"\begin{table}[t]",
        r"\centering",
        r"\caption{Parameters $(n, \threshold)$ vs. corruption $\comp$ and offline $\fail$ fractions}",
        r"\label{tab:parameter-selection}",
        f"\\begin{{tabular}}{{{col_spec}}}",
        r"\toprule",
        f"& {header} \\\\ ",
        r"\midrule",
        *rows,
        r"\bottomrule",
        r"\end{tabular}",
        r"\end{table}",
    ]
    tex = "\n".join(lines) + "\n"
    out = output_dir / "table10.tex"
    out.write_text(tex)
    print(f"  Saved {out}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path, default=Path("."))
    args = parser.parse_args()

    output_dir = args.output_dir
    output_dir.mkdir(parents=True, exist_ok=True)

    generate(output_dir)
    print("Done.")


if __name__ == "__main__":
    main()
