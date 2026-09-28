# Cargo package-cache investigation for issue 188

Status as of 2026-09-28: the observed parent/descendant lock cycle has not been
reproduced. The initiating Cargo condition remains unresolved. Podbot's
repository-specific trigger is nested Cargo execution by the `trybuild`
compile-contract harness; the reason an outer Cargo process retained an
exclusive package-cache lock is not established.

## Original observation

On 2026-09-28, a dirty development checkout at
`90d3fd16bd01f40bf313dd2ed2c6ce36280735a7` ran `make test` on Rocky Linux 10.
The outer Cargo process (PID 1832225) held
`/home/leynos/.cargo/.package-cache-mutate` with `FLOCK WRITE`. A nested Cargo
process (PID 1855450) waited for `FLOCK READ`, blocked by the outer process.
The test harness was waiting for its child and no `rustc` process was running.
At the first inspection, 72 Cargo processes were blocked by the outer process.

The nested build had started at 02:54:50 CEST and was still waiting at 11:05.
These process start times establish the duration of the run, not the exact time
at which the lock was acquired. After the four identified processes were
terminated with operator authorization, the package-cache waiters disappeared
and unrelated Cargo work resumed. This establishes that cancelling that process
tree removed the obstruction; it does not establish why Cargo held the
exclusive lock.

The checkout pinned Rust 1.88.0 and Cargo 1.88.0. `Cargo.lock` resolved
`trybuild` 1.0.117; `Cargo.toml` requests 1.0.116 with the `diff` feature.

## Bounded Rust 1.88 attempt

The legacy Cargo command was run directly under the new process supervisor on
the same Rocky Linux 10.2 x86_64 host:

```sh
RUSTFLAGS='-D warnings' uv run --no-project --python 3.14 \
  python scripts/test_runner.py --supervise --timeout 180 \
  --watch-interval 15 -- cargo test --all-targets --all-features
```

The active toolchain was Rust 1.88.0
(`6b00bc3880198600130e1cf62b8f8a93494488cc`) and Cargo 1.88.0
(`873a0649350c486caf67be772828a4f36bb4734c`). `CARGO_HOME`, `CARGO_TARGET_DIR`,
`CARGO_BUILD_JOBS`, `RUSTC`, and both rustc wrapper variables were unset. Cargo
used the shared default home at `/home/leynos/.cargo`; the repository target
directory was the default. No user or repository Cargo configuration file was
present. The Make warning policy was reproduced with `RUSTFLAGS=-D warnings`.

The supervisor returned status 124 at its 180-second deadline. Its final
snapshot showed the outer Cargo process (PID 433931), the `compile_contract`
executable (PID 476879), its nested offline Cargo process (PID 476889), and
active `rustc` descendants. The nested Cargo had been running for about 43
seconds and was compiling fixture dependencies. At that snapshot, `/proc/locks`
showed shared `FLOCK READ` holders for an external Cargo process and the nested
Cargo process; it showed no waiting lock request and no owned outer-Cargo
holder on the mapped package-cache lock. The observed run therefore reached
nested Cargo, but did not reproduce the reported lock wait before the deadline.
The supervisor's subsequent process and lock check found none of those owned
processes or their package-cache locks.

This is inconclusive evidence. The timeout interrupted active compilation
before the nested process completed, and external Cargo activity appeared
during the run. It neither reproduces nor disproves the original cycle. No
toolchain change was evaluated; issue
[#186](https://github.com/leynos/podbot/issues/186) remains separate work.

## CI and coverage boundary

The two CI test lanes now use `make test`: the no-default-features boundary
lane selects `cli_feature_gating` and `compile_contract`, and the internal lane
uses `--features internal`. Existing `cargo check` and coverage steps remain
separate.

The pull-request coverage workflow pins shared-actions `generate-coverage` at
[`a5765019912a8ab6882b12db049c7cde635f3a85`](https://github.com/leynos/shared-actions/tree/a5765019912a8ab6882b12db049c7cde635f3a85/.github/actions/generate-coverage).
It supplies `features: internal` and disables nextest. At this revision, the
action defaults `all-targets` and `doctests` to false, so it does not select
the registered compile-contract target. Its `RUN_RUST_CARGO_WAIT_TIMEOUT` is
1800 seconds; its watchdog stops the Cargo process it started and does not
demonstrate process-tree cleanup. The current coverage selection therefore does
not invoke trybuild. Reassess it if target selection changes; this work does
not modify the shared action.

The review of shared-action behavior used the pinned
[`action.yml`](https://github.com/leynos/shared-actions/blob/a5765019912a8ab6882b12db049c7cde635f3a85/.github/actions/generate-coverage/action.yml),
[`run_rust.py`](https://github.com/leynos/shared-actions/blob/a5765019912a8ab6882b12db049c7cde635f3a85/.github/actions/generate-coverage/scripts/run_rust.py),
and
[`_cargo_runner.py`](https://github.com/leynos/shared-actions/blob/a5765019912a8ab6882b12db049c7cde635f3a85/.github/actions/generate-coverage/scripts/_cargo_runner.py).

## Mitigation and recovery

The supported `make test` path builds registered compile-contract targets with
`cargo test --no-run`, waits for Cargo to exit, and then launches the exact
current-build executable. This removes the outer Cargo process from the
nested-Cargo execution phase while retaining the shared default Cargo cache and
all registered trybuild fixtures.

For the timeout and interruption procedure, read the report emitted by the
runner and confirm that its owned process tree exited. Allow unrelated Cargo
work to finish before retrying. Never delete `.package-cache-mutate`, move to a
separate `CARGO_HOME`, or terminate processes outside the runner-owned tree. The
[developer's guide](developers-guide.md#23-supported-test-orchestration)
documents the supported commands and diagnostics.
