# Reference results

CSVs here let `plots/plot_all.py --reference` regenerate every figure without
running a single experiment, which is the fastest way to check that the
plotting path works.

**Provenance matters, so read this before comparing numbers.**

These were produced on the packaging machine, **not** on the paper's
hardware:

| | |
|---|---|
| Machine | Apple M-series laptop, macOS 15, arm64 |
| Where | inside the artifact container (Linux, arm64), via `experiments/run_all.sh` |
| Object store | MinIO in a local container, loopback network |
| Scale | reduced (`PERIDOT_AE_FULL` unset); whole suite took 13 minutes |
| Paper's setup | AWS EC2 `m7i.2xlarge` and `t3.xlarge`, Ubuntu 22.04, real Amazon S3 |

These come from a single clean run of the full suite inside the container, so
the Overseer was live for `io.csv`. That matters: the Overseer's metrics
producer reads `/proc/<pid>/io`, so on a host without `/proc` its thread dies
and each module simply keeps the rate from its own configuration file. The
resulting plot looks the same but demonstrates nothing, and an earlier version
of these CSVs had exactly that defect.

Consequences, spelled out because they are large:

- **Absolute numbers do not match the paper and are not meant to.** The
  hostcall-batching speedups here reach 514x against the paper's 4.90x. That
  is not Peridot doing better; it is the non-batching baseline doing far
  worse. A host `write` syscall inside Docker Desktop's VM on macOS costs far
  more than on the paper's EC2 instances, so the baseline collapses to ~16 K
  IOPS and every ratio measured against it inflates. The shape of the curve
  is the claim; the multiplier is a property of the host.
- **The storage numbers shift with the object store.** Local MinIO has none
  of the network latency of Amazon S3, so the async-vs-sync gap is smaller
  here (2.2x) than in the paper (5.1x), while Figure 6's margin is wider
  (1.4x-2.1x versus 1.15x at the smallest batch). The orderings hold; the
  magnitudes do not. See the top-level artifact README, "What this artifact
  does and does not reproduce".

Authors: replace these with a paper-scale run on the reference hardware before
submitting, and update this file. Until then, treat them as a plumbing check
rather than as evidence for any claim.
