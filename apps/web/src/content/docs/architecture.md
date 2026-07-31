---
title: Architecture
description: How Lattice is built, for contributors working on the codebase itself.
group: Reference
order: 7
---

# Architecture

This page is for people about to contribute to Lattice's own codebase, not for
people using it in a monorepo. It names crates, modules, and types directly; the
user-facing pages do not.

## Crate layout

Lattice is a Cargo workspace of seven crates under `crates/`. Each one owns a
single concern:

| Crate | Owns |
| --- | --- |
| `lattice-config` | `lattice.json` types (`LatticeConfig`, `WorkspaceConfig`, `PipelineTask`, `Settings`, `EngineSpec`), `WELL_KNOWN_ENGINES`, `load_config`, `find_root`, schema validation |
| `lattice-workspace` | Workspace discovery, the driver evidence ladder (`DRIVERS`, `detect_drivers`), and (in its `toolchain` submodule) the engine gradient and toolchain provisioning |
| `dagger` | Builds the cross-workspace `ExecutionGraph` from resolved workspaces + the task map, and flattens it into a `Schedule` |
| `lattice-cache` | Cache identity (`compute_key`) and storage (the `CacheStore` trait, `LocalStore`) |
| `lattice-runner` | The async scheduler (`execute_tasks`): spawns tasks, wires the cache, injects toolchain `PATH`s, manages persistent tasks |
| `lattice-output` | `OutputMode`, `TaskEvent`, the `Reporter` trait (`InteractiveReporter`, `CiReporter`), and brand/splash rendering |
| `lattice` | The `lattice` binary: the clap CLI surface, subcommands, version-pin handover, the bundled JSON Schema |

The root `lattice.json` declares every crate plus `apps/web` as workspaces — the
repo dogfoods itself.

## Dependency direction

The edges between them:

```text
lattice-config
  ├── lattice-workspace  (+ lattice-config)
  ├── lattice-cache      (+ lattice-config)
  └── dagger             (+ lattice-config, lattice-workspace)
        └── lattice-runner (+ lattice-cache, lattice-config, dagger,
                              lattice-output, lattice-workspace)
              └── lattice   (+ all of the above)

lattice-output — no internal dependencies
```

`lattice-config` is the base: every other crate models on top of its schema
types, and it depends on nothing else in the workspace. `lattice-cache` depends
only on `lattice-config`, because `compute_key` needs a `PipelineTask` and a
filesystem path, not workspace discovery or the graph. Cache identity is
therefore testable in isolation from how a task came to exist.

Two edges are easy to get backwards from the crate names alone.

### `lattice-runner` does not depend on `petgraph`

`petgraph` is a dependency of `dagger` alone. `dagger` builds a
`petgraph::graph::DiGraph<TaskNode, ()>` internally, topologically sorts it,
then calls `build_schedule` to flatten it into a petgraph-independent `Schedule`
— `Vec<HashSet<usize>>` prerequisites, `Vec<Vec<usize>>` dependents, and a
`Vec<usize>` indegree count, all indexed by position rather than by petgraph's
`NodeIndex`. `lattice-runner` imports only
`dagger::{build_schedule, ExecutionGraph, Schedule}`, and its scheduling loop in
`execute_tasks` walks plain integer indices. That is what lets `dagger`'s own
`Schedule` unit tests
exercise scheduling shape without a live process, and lets the graph library be
swapped without touching the runner.

### `lattice-output` is a true leaf

It depends only on `console` and `indicatif`. Every crate above `lattice-config`
may depend on it for reporting and branding; it depends on none of them back.
That is what keeps `lattice-runner` I/O-only with respect to presentation: it
emits typed `TaskEvent`s and calls `Reporter` hooks (`run_start`, `event`,
`surface_failure`, `run_summary`, `note`, `warn`, `finish`), and never touches
`console`, `indicatif`, or `println!` for task status. `InteractiveReporter` (a
live `indicatif::MultiProgress` TUI) and `CiReporter` (plain greppable lines)
are the two implementations `make_reporter` picks between; a third would not
need to touch the runner.

## Data flow of `lattice run`

Tracing one invocation end to end, crate by crate:

1. Entry — `Cli::parse()` (clap), then `Cli::execute()`.
2. Version-pin handover — unless the subcommand is `upgrade`, `completions`, or
   `version` (`Cli::skips_pin_handover`), `Cli::execute` calls
   `crate::drift::repo_root()` then `drift::honor_pin`, which reads
   `latticeVersion` directly out of `lattice.json` text rather than through
   `lattice-config`, so a schema the running binary cannot parse still yields a
   pin. If a managed `.lattice/bin` binary does not match, it installs and
   `exec`s the pinned version instead.
3. Root find + config load — `lattice_config::find_root` walks up from the cwd
   for a `lattice.json`; `crate::schema::ensure_schema` writes
   `.lattice/schema.json` if absent; `lattice_config::load_config` parses and
   calls `LatticeConfig::validate`.
4. Output mode — `crate::cli::detect_output_mode` consults the real TTY and `CI`
   env via `lattice_output::detect_mode`. If the requested tasks' transitive
   closure includes a persistent task (`dagger::includes_persistent_task`), an
   otherwise-`Interactive` run is forced to `Raw`.
5. Workspace discovery + driver detection
   (`lattice_workspace::discover_workspaces`) — for each configured workspace,
   resolves the merged engine map (`lattice_config::resolve_engines`), walks the
   evidence ladder (`detect_drivers`) for `auto` workspaces, and resolves each
   root task's command (`infer_task_command`) unless a `scripts` override
   supplies one. See [Driver detection](/lattice/docs/drivers) for the ladder
   itself.
6. Graph construction (`dagger::build_execution_graph_selected`) — expands the
   requested (possibly stacked) root tasks across the resolved workspaces into
   `TaskNode`s, wires `^task`/bare-`task` edges, rejects cycles and a persistent
   task with a dependent, and topologically sorts the result into an
   `ExecutionGraph`. Under `--filter`, the matched workspaces are the roots: the
   graph is narrowed to them plus their transitive prerequisites, which are
   flagged on the node so `--dry-run` can tag them.
7. Schedule (`dagger::build_schedule`) — flattens that graph into the
   petgraph-independent `Schedule` described above.
8. Scheduler execution (`lattice_runner::execute_tasks`) — for each workspace up
   front, resolves its merged engines into a `PATH` prefix and identity string
   via `lattice_workspace::toolchain::provision_and_resolve`, memoized so an
   identical engine spec provisions once. Then it drives the in-degree
   scheduler: a task at indegree zero is spawned as a Tokio task under a
   `Semaphore`-capped concurrency. Each spawned task (`run_one`) computes its
   cache key via `lattice_cache::compute_key`, looks it up through the
   `CacheStore` (`lattice_cache::LocalStore`), and either restores the cached
   artifact or runs the command through the platform shell (`sh -c` / `cmd /C`)
   with the resolved `PATH` prepended. On success it stores outputs back through
   the same `CacheStore`. See [Cache internals](/lattice/docs/cache-internals)
   and [Engines and provisioning](/lattice/docs/engines) for those two
   subsystems in depth.
9. Event emission + reporting — every state change (`Started`, `CacheHit`,
   `Output`, `Finished`, `Failed`, `Skipped`) is a `lattice_output::TaskEvent`
   sent over an internal `mpsc` channel and forwarded to the `Reporter` chosen
   in step 4, which renders it as a live TUI line or a plain log line.

## Tests

Tests live at two layers, plus one hermetic end-to-end script.

Unit tests are colocated as `#[cfg(test)] mod tests` at the bottom of the file
they cover. `dagger` covers graph construction, cycle/persistent-leaf rejection,
and `Schedule` shape. `lattice-cache` covers hash stability and sensitivity plus
the store/lookup/restore/corruption/prune round trip. `lattice-workspace` covers
every rung of the evidence ladder and role composition/conflict, and its
`toolchain` submodule covers `classify`, `parse_version`, `satisfies`, and a
provision-then-reuse round trip against a fake installed tool. `lattice-runner`
covers ordering, concurrency, fail-fast vs. `--continue`, the cache round trip,
and persistent tasks: not blocking the graph, reporting an exit, and staying
silent when the exit was a shutdown kill. All of it runs against a
`RecordingReporter` test double. The `lattice` crate covers version-nag gating and pin-handover
decision logic as pure functions.

Black-box e2e tests live under `crates/lattice/tests/`, each a `#[test]` binary
that builds a throwaway repo in a `tempfile::TempDir` via the shared `Fixture`
helper in `common/mod.rs` and drives the compiled binary through `assert_cmd`.
`e2e_run.rs` covers caching, filtering, dry-run, stacked/sequential phases, and
keep-going. `e2e_toolchain.rs` covers provisioning and `PATH` injection with a
fake `installCmd`. `e2e_halves.rs` covers running the task-runner and
toolchain-manager halves independently, plus `prune` and `completions`.
`e2e_init.rs`, `e2e_passthrough.rs`, and `e2e_upgrade.rs` cover scaffolding,
wrapping a nested repo with its own runner, and version pinning.

`scripts/stress-test.sh` is a single ~1,300-line, dependency-free hermetic
script that builds the release or debug binary, generates a production-shaped
multi-language monorepo plus focused sub-repos in a temp directory, and
exercises every command and flag against it: top-level surfaces, `init`,
`setup`, `run` (ordering, filtering, dry-run, concurrency), caching, persistent
tasks, failure handling, driver detection across roughly sixteen ecosystems,
error paths, `prune`, nested-repo passthrough, and install/upgrade/pinning. It
needs no network and no real language toolchains: driver detection is verified
through `--dry-run`, and provisioning is exercised through a fake,
locally-installed toolchain. `AGENTS.md` requires updating it alongside any
change to CLI surface or behavior.

## Extension seams

Where a contributor is most likely to make a change.

### Adding a driver

Add a `DriverSpec` to the `DRIVERS` array in `lattice-workspace`: `tool`,
`roles`, `fingerprint` files, `version_cmd`, and an `invoke_tpl` with a `{task}`
placeholder. `roles` is a slice — a tool may fill several — and the
highest-ranked one, ranking `Runtime` < `BuildTool` < `PackageManager` <
`TaskRunner`, is the role it competes for as a driver. That decides both drive
rank and what it composes with or conflicts against. If a fingerprint should be
read as a native declaration rather than a generic lockfile, add it to
`is_native_fingerprint`. Regenerate the driver table on
[Toolchains](/lattice/docs/toolchains) from this array — never from memory.

### Adding a well-known engine

Add one `(name, version command)` row to `WELL_KNOWN_ENGINES` in
`lattice-config`. That single table answers both whether a string-form
constraint on the name is valid and how to read the tool's version, so the two
cannot disagree. A driver in `DRIVERS` must have a row here as well, with the
same version command; a test in each crate enforces both halves.

### Adding a subcommand

Add a variant to the `Commands` enum in the `lattice` crate's CLI module, an
`Args` struct plus `execute` method in
a new file under `crates/lattice/src/commands/`, and register it in that
directory's `mod.rs`. Decide whether the subcommand needs the version-pin
handover (extend `skips_pin_handover` if not) and whether it needs the
version-drift nag (call `maybe_emit_version_nag` if so, as `run` and `setup`
do).

### Adding a cache backend

Implement the `CacheStore` trait — `lookup`, `store`, `restore`, `prune`,
`touch` — from `lattice-cache`. `LocalStore` is the only implementation today;
the trait exists so a future Lattice Cloud backend can slot in without
`lattice-runner` changing, and `execute_tasks` holds its store behind
`Arc<dyn CacheStore>`.
