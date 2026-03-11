# Chorus: Secret Recovery with Ephemeral Client Committees

**IEEE S&P 2026 Artifact**

This repository contains the Rust implementation and reproducibility
package for the Chorus paper.  It targets the IEEE S&P **Available**,
**Functional**, and **Reproduced** badges.

For the full constructions, formal definitions, and security proofs
deferred from the main paper, see
[chorus-artifact-material.pdf](./chorus-artifact-material.pdf).

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

Inside the screen session:

```bash
python3 ~/chorus/scripts/setup_eval.py
```

This script:

1. Auto-detects the GCP project and zone.
2. Creates a `c2d-standard-112` compute VM (112 vCPU, 448 GB RAM) with
   Ubuntu 22.04, same as the control VM.
3. Runs the same setup script to install Rust, C/C++ deps, and build.
4. Generates all benchmark state (precomputed key material).

The script is fully idempotent — re-running it skips completed steps.
First-time setup takes roughly 30–60 minutes.

---

## Step 3 — Run Experiments (on the control VM)

```bash
python3 ~/chorus/scripts/run_experiment.py
```

You'll see a numbered menu of available experiments with estimated
durations.  Pick one by number.

What happens:

- A **file lock** is acquired so no two experiments run at the same time.
  If someone else is running an experiment, the script tells you which
  one and how long it's been going.
- The experiment runs on the compute VM.  Output streams live to your
  terminal and is saved to a timestamped log file under `~/results/`.
- After completion, the script tells you where the log is and how to run
  the next experiment.

**Tip:** You can safely detach from screen (`Ctrl-A D`) while an
experiment is running.  The experiment continues in the background.  When
you reconnect (re-run `login.py`), scroll up to see the output, or check
the log file in `~/results/`.

Repeat this step for each experiment you want to run.

---

## Step 4 — Tear Down

After finishing all experiments, remove the compute VM to stop billing:

```bash
python3 ~/chorus/scripts/teardown.py
```

---

## Claims to Reproduce

The experiments in the interactive menu correspond to the main
performance results in the paper.  Specifically, they reproduce
**Table X**, **Figure Y**, and **Figure Z** (sections to be filled in by
the authors).
