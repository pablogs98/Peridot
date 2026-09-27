# Superseded by `artifact/`

The scripts in this directory predate the current runtime and **do not work**:

- `run_iops.sh` invokes `peridot "$job" -n "$n"`, but the CLI takes
  `peridot <MODULE_PATH> <CONFIG_PATH>` and has no `-n` flag.
- `job_*.yaml` use an obsolete configuration schema (the module path as the
  top-level key, with a `peridot_config` block of `demand` and `priority`)
  against hardcoded `/home/ubuntu/...` paths. The current schema is `args`,
  `contexts`, `io` and `cpu`, documented in the top-level README.

The hostcall-batching experiment they were written for now lives in
[`../artifact/experiments/exp_hostcall_batching.sh`](../artifact/experiments/exp_hostcall_batching.sh),
which runs against the current CLI and writes a CSV that
`artifact/plots/plot_hostcall_batching.py` turns into Figure 7.

These files are kept only as a record of the earlier measurement setup. Do not
run them.
