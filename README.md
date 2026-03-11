# Chorus: Secret Recovery with Ephemeral Client Committees

**IEEE S&P 2026 Artifact**

Rust implementation and reproducibility package for the Chorus paper,
targeting the IEEE S&P **Available**, **Functional**, and **Reproduced**
badges.

### Important: Differences from the Paper

The paper's client experiments ran on an **Android phone** (Snapdragon 8 Gen 3, 8 cores, 8 GB RAM).  This artifact uses a **cloud VM** as
the client instead (8 vCPUs, 8 GB RAM) to make evaluation portable.
As a result:

- **Battery measurements cannot be reproduced** and are noted as skipped
  in generated tables.
- **Network conditions are emulated.** The paper's experiments used a
  commodity WiFi connection from the phone to a server in South Asia.
  Reproducing that during artifact evaluation is unclear, so we
  use Linux `tc` (netem + tbf) to emulate the same measured conditions
  (75 Mbps bandwidth, 280 ms RTT) between two co-located VMs.
- **Absolute timings may differ slightly** from the paper.  Relative
  comparisons and communication costs are unaffected.

---

## Quick Start

You will receive from the authors: an SSH key pair and the control VM's
IP address.  Your local machine needs Python 3 and an SSH client.

```bash
# 1. Log in to the control VM (saves connection details for next time)
python3 scripts/login.py

# 2. Set up the compute VM (idempotent — safe to re-run)
python3 ~/chorus/scripts/setup_eval.py

# 3. Run experiments (interactive menu)
python3 ~/chorus/scripts/run_experiment.py

# 4. Tear down when done
python3 ~/chorus/scripts/teardown.py
```

All commands run inside a **GNU screen** session.  If you disconnect,
experiments keep running.  Re-run `login.py` to reconnect.

---

## Experiments

All parameters are read from `config.json` — nothing is hardcoded.

| # | ID | Paper Reference | What runs |
|---|-----|-----------------|-----------|
| 1 | `figure5` | Figure 5: saVSS vs cgVSS | NIVSS benchmarks (both VMs) |
| 2 | `table9` | Table 9: Server per-epoch costs | Server benchmark only |
| 3 | `table6` | Table 6: Client secret-recovery costs | Server + client benchmarks (network throttled) |
| 4 | `table7` | Table 7: Committee-member costs | Server + client benchmarks (network throttled) |
| 5 | `figure8` | Figure 8: Client cost breakdown | Server + client benchmarks (network throttled) |
| 6 | `appendixA41` | Appendix A.4.1: DKG setup costs | Server + client benchmarks (network throttled) |
| 7 | `table10` | Table 10: Parameter selection | Local computation (no benchmarks) |

`table9` only needs the server benchmark — no network server or
throttling.  Experiments 3–6 additionally start a network server on the
compute VM, apply bandwidth/RTT limits (75 Mbps, 280 ms RTT), and run
the client benchmark over the throttled link.

Each experiment saves results (logs, `.tex` files, plots) to its own
timestamped directory under `results/<experiment_id>/`.

---

## Tear Down

```bash
python3 ~/chorus/scripts/teardown.py
```

Deletes the compute VM to stop billing.  The control VM is managed by
the authors.
