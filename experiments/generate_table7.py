#!/usr/bin/env python3
"""Generate Table 7: Client committee-member costs and sortition frequency.

Usage:
    python3 experiments/generate_table7.py \
        --results-dir results/secret_recovery/<timestamp>
"""

import argparse
import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from parse_secret_recovery import (  # noqa: E402
    latex_bytes,
    latex_time,
    n_label,
    process_client_log,
    process_server_log,
    readable_time,
)


def generate(client_stats, server_stats, output_dir):
    cases = sorted(set(c for c, _ in client_stats.keys()))
    case = cases[0]
    n_values = sorted([n for c, n in client_stats.keys() if c == case])
    cs0 = client_stats[(case, n_values[0])]
    committee_size = cs0["Committee Size"]
    threshold = cs0["Threshold"]

    ncols = len(n_values)
    col_spec = "l" + "r" * ncols
    header_cols = " & ".join(n_label(n) for n in n_values)

    freq_cols, time_cols, comm_cols = [], [], []

    for n in n_values:
        cs = client_stats[(case, n)]
        ss = server_stats.get((case, n))

        prob = committee_size / float(n)
        expected_epochs = math.ceil(1 / prob)
        freq_cols.append(readable_time(2 * expected_epochs))

        ht = cs["Typical Handover Time"]
        time_cols.append(latex_time(ht["avg_time"]) if ht else "---")

        if ss:
            km_comm = (
                ss["C->S Handover"]
                + ss["Max Commstate_2"]
                + ss["Prev State Coeffs"]
                + 3 * ss["Committee"]
                + ss["C->S Committee Selection"]
                + ss["Recovery Request Batch"]
                + ss["C->S Recovery Response Batch"]
            )
            comm_cols.append(latex_bytes(km_comm))
        else:
            comm_cols.append("---")

    lines = [
        r"\begin{table}[t]",
        r"    \centering",
        f"    \\caption{{The overheads for a client selected for a committee. "
        f"Battery measurement skipped (not running on a mobile phone). "
        f"We use committee size $n={committee_size}$ and threshold $\\threshold={threshold}$.}} "
        r"\label{tab:km-client-frequency}",
        f"    \\begin{{tabular}}{{{col_spec}}}",
        r"    \toprule",
        f"    & {header_cols} \\\\ \\midrule",
        f"    \\textbf{{Frequency}} & {' & '.join(freq_cols)} \\\\",
        f"    \\textbf{{Time}} & {' & '.join(time_cols)} \\\\",
        f"    \\textbf{{Comm.}} & {' & '.join(comm_cols)} \\\\",
        r"    \bottomrule",
        r"    \end{tabular}",
        r"    \end{table}",
    ]
    tex = "\n".join(lines) + "\n"
    out = output_dir / "table7.tex"
    out.write_text(tex)
    print(f"  Saved {out}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--results-dir", required=True, type=Path)
    parser.add_argument("--output-dir", type=Path, default=None)
    args = parser.parse_args()

    results_dir = args.results_dir
    output_dir = args.output_dir or results_dir
    output_dir.mkdir(parents=True, exist_ok=True)

    client_stats = process_client_log(results_dir / "secret_recovery_client.log")
    server_stats = process_server_log(results_dir / "secret_recovery_server.log")

    if not client_stats or not server_stats:
        sys.exit("No benchmark data found in logs.")

    generate(client_stats, server_stats, output_dir)
    print("Done.")


if __name__ == "__main__":
    main()
