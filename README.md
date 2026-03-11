# Chorus: Secret Recovery with Ephemeral Client Committees

**IEEE S&P 2026 Artifact**

This repository contains the Rust implementation and reproducibility
package for the Chorus paper.  It targets the IEEE S&P **Available**,
**Functional**, and **Reproduced** badges.

---

## Prerequisites

You will receive from the authors:

- An SSH **private key** file (e.g. `chorus_eval_key`)
- The matching **public key** file (e.g. `chorus_eval_key.pub`)
- The **IP address** of the control VM

You do **not** need a GCP account or any cloud credentials.

Your local machine needs: Python 3 and an SSH client (both standard on
macOS and Linux).

---

## Step 1 — Log In (your local machine)

> **Estimated time:** TODO minutes

```bash
python3 scripts/login.py
```

The script will interactively ask you for:

1. The control VM's IP address
2. The path to the SSH private key
3. The path to the SSH public key

It then connects you via SSH and opens a persistent **GNU screen**
session on the control VM.

**Key points:**

- Your connection details are saved locally (`~/.chorus-eval.json`), so
  next time you run `login.py` it reconnects immediately.
- Each local machine gets its own screen session.  If you disconnect
  (close your terminal, lose WiFi, etc.), any running experiment
  **keeps going**.  Just re-run `python3 scripts/login.py` to pick up
  where you left off.
- Screen quick reference:
  - **Detach** without stopping anything: `Ctrl-A`, then `D`
  - **Scroll up** to see past output: `Ctrl-A`, then `Esc` (arrow keys
    to scroll, `Esc` again to exit scroll mode)

---

## Step 2 — Set Up the Compute VM (on the control VM)

> **Estimated time:** TODO minutes

Inside the screen session:

```bash
python3 ~/chorus/scripts/setup_eval.py
```

This script orchestrates the full setup of the compute VM:

1. Auto-detects the GCP project, zone, and network from the control VM.
2. Creates a compute VM (machine type, disk, and image are read from
   `config.json`).
3. Copies only git-tracked source files to the compute VM.
4. Installs system packages, Rust, and builds the project.
5. **Generates benchmark state for all configured cases and client
   counts** by running each benchmark case locally on the compute VM
   in `SAVE_STATE` mode.  This pre-computes key material, committee
   data, and serialised state so that the actual experiments can
   replay from a specific client's perspective without repeating the
   expensive setup.

The cases and client counts are defined in `config.json` — no values
are hardcoded.  The script is fully idempotent — re-running it skips
completed steps.

Breakdown of sub-steps:

| Sub-step | Time |
|----------|------|
| VM creation | 45 seconds |
| Copy repo to compute VM | 52 seconds |
| Install system packages & Rust | 49 seconds |
| Compile Chorus artifact | 1 minute 47 seconds |
| Generating benchmark state | TODO |

**Logging into the compute VM directly:**  If you need to inspect the
compute VM (e.g. check files, look at logs, debug), you can SSH into it
from the control VM:

```bash
python3 ~/chorus/scripts/login_compute.py
```

---

## Step 3 — Run Experiments (on the control VM)

> **Estimated time per experiment:** TODO

```bash
python3 ~/chorus/scripts/run_experiment.py
```

You'll see a numbered menu of available experiments with estimated
durations.  Pick one by number.

What happens:

- A **file lock** is acquired so no two experiments run at the same time.
  If someone else is running an experiment, the script tells you which
  one and how long it's been going.
- Each experiment may run steps on **both** VMs (e.g. server benchmarks
  on the compute VM, client benchmarks on the control VM).  Output
  streams live to your terminal and is saved to timestamped log files
  under `results/<experiment>/`.
- After completion, the script automatically generates plots (PNG) and
  tells you where all artifacts are saved.

**Tip:** You can safely detach from screen (`Ctrl-A D`) while an
experiment is running.  The experiment continues in the background.  When
you reconnect (re-run `login.py`), scroll up to see the output, or check
the log file in `results/`.

Repeat this step for each experiment you want to run.

### Available Experiments

| # | ID | Description | What it does |
|---|-----|-------------|-------------|
| 1 | `figure5` | saVSS vs cgVSS (Figure 5) | Runs `sa_nivss` server benchmark on the compute VM, then `sa_nivss` and `pv_nivss` client benchmarks on the control VM. Parses all three logs and generates `nivss-time.png`, `nivss-comm.png`, and `nivss-legend.png`. |

The cases for the NIVSS benchmarks are defined in `config.json` under
`nivss_cases`.  All experiment definitions live under
`config.json → experiments`.

---

## Step 4 — Tear Down

> **Estimated time:** 1 minute

After finishing all experiments, remove the compute VM to stop billing:

```bash
python3 ~/chorus/scripts/teardown.py
```

---

## Claims to Reproduce

The experiments in the interactive menu correspond to the main
performance results in the paper:

| Experiment | Paper Reference |
|------------|-----------------|
| `figure5`  | **Figure 5** — saVSS vs cgVSS runtime and communication |
