#!/usr/bin/env python3
"""Generate server dollar-cost estimation breakdown table.

Reads the server benchmark log and config.json cost parameters to produce
a LaTeX table showing compute, network, and storage costs per month.

Usage:
    python3 experiments/generate_server_cost.py \
        --results-dir results/secret_recovery/<timestamp> \
        --output-dir results/server_cost/<timestamp>
"""

import argparse
import json
import sys
from pathlib import Path

REPO_DIR = Path(__file__).resolve().parent.parent
CONFIG = json.loads((REPO_DIR / "config.json").read_text())
COST_CFG = CONFIG["experiments"]["server_cost"]

sys.path.insert(0, str(Path(__file__).resolve().parent))
from parse_secret_recovery import (  # noqa: E402
    convert_bytes,
    latex_bytes,
    latex_time,
    n_label,
    process_server_log,
)


def dollar(amount):
    if amount < 0.01:
        return f"\\${amount:.4f}"
    if amount < 1:
        return f"\\${amount:.2f}"
    if amount < 100:
        return f"\\${amount:.2f}"
    return f"\\${amount:,.0f}"


def generate(server_stats, output_dir):
    recoveries_per_user_per_year = COST_CFG["recoveries_per_user_per_year"]
    compute_cost = COST_CFG["compute_cost_per_month"]
    epoch_minutes = COST_CFG["epoch_interval_minutes"]
    cost_per_gb = COST_CFG["network_cost_per_gb"]
    storage_per_client = COST_CFG["storage_per_client_bytes"]
    storage_cost_per_gb = COST_CFG["storage_cost_per_gb_month"]
    epochs_per_month = (60 * 24 * 30) / epoch_minutes
    epochs_per_year = (60 * 24 * 365) / epoch_minutes

    cases = sorted(set(c for c, _ in server_stats.keys()))
    case = cases[0]
    n_values = sorted([n for c, n in server_stats.keys() if c == case])

    rows = []
    for n_clients in n_values:
        ss = server_stats[(case, n_clients)]
        n_committee = ss["Committee Size"]

        handover_comm = n_committee * (
            ss["C->S Handover"] + ss["Max Commstate_2"]
            + 3 * ss["Committee"] + ss["Prev State Coeffs"]
            + ss["Public State"]
        )
        handover_comm_egress = handover_comm - n_committee * ss["C->S Handover"]

        recoveries_per_epoch = n_clients * recoveries_per_user_per_year / epochs_per_year

        bench_expected_req = ss["Expected Requests"]
        if bench_expected_req > 0:
            scale = recoveries_per_epoch / bench_expected_req
        else:
            scale = 1.0
        request_batch_egress = n_committee * ss["Recovery Request Batch"] * scale
        response_egress = recoveries_per_epoch * ss["S->C Recovery Response"]

        network_gb_per_month = (
            (handover_comm_egress + request_batch_egress + response_egress)
            / 1e9
        ) * epochs_per_month
        network_cost = network_gb_per_month * cost_per_gb

        storage_gb = (storage_per_client * n_clients) / 1e9
        storage_cost = storage_gb * storage_cost_per_gb

        total = compute_cost + network_cost + storage_cost

        rows.append({
            "n_clients": n_clients,
            "recoveries_per_epoch": recoveries_per_epoch,
            "compute": compute_cost,
            "network": network_cost,
            "network_gb": network_gb_per_month,
            "storage": storage_cost,
            "storage_gb": storage_gb,
            "total": total,
        })

    ncols = len(n_values)
    n_headers = " & ".join(n_label(n) for n in n_values)
    col_spec = "l" + "r" * ncols

    compute_cells = " & ".join(dollar(r["compute"]) for r in rows)
    network_cells = " & ".join(
        f'{dollar(r["network"])} ({r["network_gb"]:.1f} GB)' for r in rows
    )
    storage_cells = " & ".join(
        f'{dollar(r["storage"])} ({r["storage_gb"]:.1f} GB)' for r in rows
    )
    total_cells = " & ".join(dollar(r["total"]) for r in rows)

    lines = [
        r"\begin{table}[t]",
        r"    \centering",
        r"    \caption{Estimated monthly server dollar costs assuming "
        + f"{recoveries_per_user_per_year} recovery/user/year. "
        + f"Compute: \\texttt{{{CONFIG['compute_vm']['machine_type']}}} (us-central1). "
        + f"Network: \\${cost_per_gb}/GB. "
        + f"Storage: \\${storage_cost_per_gb}/GB/month. "
        + f"Epoch interval: {epoch_minutes} min.}}",
        r"    \label{tab:server-dollar-cost}",
        f"    \\begin{{tabular}}{{{col_spec}}}",
        r"    \toprule",
        f"    & {n_headers} \\\\ \\midrule",
        f"    \\textbf{{Compute}} & {compute_cells} \\\\",
        f"    \\textbf{{Network}} & {network_cells} \\\\",
        f"    \\textbf{{Storage}} & {storage_cells} \\\\ \\midrule",
        f"    \\textbf{{Total}}   & {total_cells} \\\\",
        r"    \bottomrule",
        r"    \end{tabular}",
        r"\end{table}",
    ]
    tex = "\n".join(lines) + "\n"
    out = output_dir / "server_cost.tex"
    out.write_text(tex)
    print(f"  Saved {out}")

    for r in rows:
        print(f"    N={r['n_clients']:>12,}  "
              f"rec/epoch={r['recoveries_per_epoch']:.1f}  "
              f"compute={dollar(r['compute'])}  "
              f"network={dollar(r['network'])}  "
              f"storage={dollar(r['storage'])}  "
              f"total={dollar(r['total'])}")


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
        sys.exit("No benchmark data found in server log.")

    generate(server_stats, output_dir)
    print("Done.")


if __name__ == "__main__":
    main()
