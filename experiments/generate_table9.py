#!/usr/bin/env python3
"""Generate Table 9: Server per-epoch costs.

Usage:
    python3 experiments/generate_table9.py \
        --results-dir results/secret_recovery/<timestamp>
"""

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from parse_secret_recovery import (  # noqa: E402
    latex_bytes,
    latex_time,
    n_label,
    process_server_log,
)


def generate(server_stats, output_dir):
    cases = sorted(set(c for c, _ in server_stats.keys()))
    case = cases[0]
    n_values = sorted([n for c, n in server_stats.keys() if c == case])

    ss0 = server_stats[(case, n_values[0])]
    n_committee = ss0["Committee Size"]

    fixed_time = ss0["Handover Time"]["avg_time"] + ss0["Process Committee Time"]["avg_time"]
    fixed_comm = (
        n_committee * (ss0["C->S Handover"] + ss0["Max Commstate_2"]
                       + 3 * ss0["Committee"] + ss0["Prev State Coeffs"]
                       + ss0["Public State"])
        + n_committee * ss0["C->S Committee Selection"]
    )

    ncols = len(n_values)
    rec_header = f"\\multicolumn{{{ncols}}}{{c}}{{\\textbf{{--- Recovery Cost ---}}}}"
    col_spec = "lr" + "r" * ncols
    n_labels = " & ".join(n_label(n) for n in n_values)

    time_cols = []
    comm_cols = []
    for n in n_values:
        ss = server_stats[(case, n)]
        rec_time = ss["Process Recovery Request Time"]["avg_time"] + ss["Process Recovery Response Time"]["avg_time"]
        time_cols.append(latex_time(rec_time))

        expected_req = ss["Expected Requests"]
        rec_comm = (
            n_committee * ss["Recovery Request Batch"]
            + expected_req * ss["Recovery Request"]
            + n_committee * ss["C->S Recovery Response Batch"]
            + expected_req * ss["S->C Recovery Response"]
        )
        comm_cols.append(latex_bytes(rec_comm))

    lines = [
        r"\begin{table}[t]",
        r"    \centering",
        r"    \caption{The server per-epoch costs are dominated by a fixed cost independent of $N$, "
        r"and the recovery cost grows linearly with $N$ (assuming each client performs one recovery per year).}",
        r"    \label{tab:server-cost}",
        f"    \\begin{{tabular}}{{{col_spec}}}",
        r"    \toprule",
        f"     & \\textbf{{Fixed Cost}} & {rec_header} \\\\",
        f"    & & {n_labels} \\\\ \\midrule",
        f"    \\textbf{{Time}}  & {latex_time(fixed_time)} & {' & '.join(time_cols)} \\\\",
        f"    \\textbf{{Comm.}} & {latex_bytes(fixed_comm)} & {' & '.join(comm_cols)} \\\\",
        r"    \bottomrule",
        r"    \end{tabular}",
        r"    \end{table}",
    ]
    tex = "\n".join(lines) + "\n"
    out = output_dir / "table9.tex"
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

    server_stats = process_server_log(results_dir / "secret_recovery_server.log")

    if not server_stats:
        sys.exit("No benchmark data found in logs.")

    generate(server_stats, output_dir)
    print("Done.")


if __name__ == "__main__":
    main()
