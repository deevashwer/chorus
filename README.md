# Chorus: Secret Recovery with Ephemeral Client Committees

**IEEE S&P 2026 Artifact**

Rust implementation and reproducibility package for the Chorus paper,
targeting the IEEE S&P **Available**, **Functional**, and **Reproduced**
badges.

### Hardware

The artifact uses two GCP VMs in the same zone:

- **Compute VM (server):** `c2d-standard-112` — 112 vCPUs (AMD EPYC Milan), 448 GB RAM, 200 GB disk.
- **Control VM (client):** `e2-standard-8` — 8 vCPUs, 8 GB RAM.

Both run Ubuntu 22.04.  The compute VM is created and destroyed by the
setup/teardown scripts; the control VM is pre-provisioned by the authors.

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

| Step | Command | Est. time |
|------|---------|-----------|
| Log in to the control VM | `python3 scripts/login.py` | ~30 s |
| Set up the compute VM (idempotent) | `python3 ~/chorus/scripts/setup_eval.py` | ~3 h 10 min* |
| Run all experiments | `python3 ~/chorus/scripts/run_experiment.py all` | ~3 h |
| Tear down compute VM | `python3 ~/chorus/scripts/teardown.py` | ~1 min |

\* Most of the setup time (~3 h 5 min) is spent preprocessing
state; VM setup and building the artifact takes only ~5 min.

You can also run `run_experiment.py` without arguments for an
interactive menu, or pass a single experiment ID (e.g. `table6`).

Benchmark logs are cached in `results/` and reused across runs since
they take a long time to produce.  To force a full re-run from scratch:

```bash
python3 ~/chorus/scripts/run_experiment.py all --force
```

All commands run inside a **GNU screen** session.  If you disconnect,
experiments keep running.  Re-run `login.py` to reconnect.

---

## Experiments

All parameters are read from `config.json` — nothing is hardcoded.
Each experiment saves results to `results/<experiment_id>/<timestamp>/`.

| # | ID | Paper Reference | Est. time |
|---|-----|-----------------|-----------|
| 1 | `figure5` | Figure 5: saVSS vs cgVSS | ~3 h 45 min |
| 2 | `table9` | Table 9: Server per-epoch costs | ~30 min |
| 3 | `table6` | Table 6: Client secret-recovery costs | ~15 min* |
| 4 | `table7` | Table 7: Committee-member costs | < 1 min* |
| 5 | `figure8` | Figure 8: Client cost breakdown | < 1 min* |
| 6 | `appendixA41` | Appendix A.4.1: DKG setup costs | < 1 min* |
| 7 | `server_cost` | Server dollar-cost estimation | < 1 min* |
| 8 | `table10` | Table 10: Parameter selection | < 1 min |

\* Experiments 2–7 share benchmark logs.  Once `table9` and `table6`
have run, experiments 4–7 reuse those logs and finish instantly.
