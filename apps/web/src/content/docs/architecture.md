---
title: Architecture
description: How Lattice is built, for contributors working on the codebase itself.
group: Reference
order: 7
---

# Architecture

This page is for people about to contribute to Lattice's own codebase, not for
people using it in a monorepo. It names crates, modules, and types directly —
the user-facing pages deliberately do not.

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

The root `lattice.json` declares every crate plus `apps/web` as workspaces —
the repo dogfoods itself.

## Dependency direction

Each crate's `Cargo.toml` gives the real edges (verified, not assumed):

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
only on `lattice-config` — `compute_key` needs a `PipelineTask` and a
filesystem path, not the workspace-discovery or graph machinery, so cache
identity can be reasoned about and tested in total isolation from how a task
came to exist.

Two edges are easy to get backwards from the crate names alone:

**`lattice-runner` does not depend on `petgraph`.** `dagger`'s `Cargo.toml`
lists `petgraph = "0.8"`; `lattice-runner`'s does not. `dagger` builds a
`petgraph::graph::DiGraph<TaskNode, ()>` internally, topologically sorts it,
then calls `build_schedule` to flatten it into a plain `Schedule` —
`Vec<HashSet<usize>>` prerequisites, `Vec<Vec<usize>>` dependents, a
`Vec<usize>` indegree count — indexed by position, not by petgraph's
`NodeIndex`. The module doc in `crates/dagger/src/lib.rs` calls this a
"petgraph-independent `Schedule`". `lattice-runner` only ever imports
`dagger::{build_schedule, ExecutionGraph, Schedule}`; its own scheduling loop
in `execute_tasks` (`crates/lattice-runner/src/lib.rs`) walks plain integer
indices and `HashSet`/`Vec` operations. One consequence: `dagger`'s own
`Schedule` unit tests (`schedule_two_node_chain`, `cross_workspace_caret_edges`,
…) exercise the scheduling shape without touching a live process, and the
graph library could be swapped without `lattice-runner` noticing.

**`lattice-output` is a true leaf.** Its `Cargo.toml` depends only on
`console` and `indicatif` — nothing else in the workspace. Every crate above
`lattice-config` may depend on it for reporting and branding, but it depends
on none of them back. This is what lets `lattice-runner` stay, in its own
module doc's words, "deliberately I/O-only with respect to presentation": it
emits typed `TaskEvent`s and calls `Reporter` hooks (`event`, `surface_failure`,
`run_summary`, `note`, `warn`, `finish`) and never touches `console`,
`indicatif`, or `println!` for task status itself. `InteractiveReporter` (a
live `indicatif::MultiProgress` TUI) and `CiReporter` (plain greppable lines)
are the two implementations `lattice_output::make_reporter` picks between; a
third implementation would not need to touch the runner at all.

## Data flow of `lattice run`

Tracing one invocation end to end, crate by crate:

1. **Entry** (`crates/lattice/src/main.rs`) — `Cli::parse()` (clap), then
   `Cli::execute()` (`crates/lattice/src/cli.rs`).
2. **Version-pin handover** — unless the subcommand is `upgrade`,
   `completions`, or `version`, `Cli::execute` calls `crate::drift::repo_root()`
   then `drift::honor_pin`, which reads `latticeVersion` directly out of
   `lattice.json` text (not through `lattice-config`, so a schema the running
   binary can't parse still yields a pin) and, if a managed `.lattice/bin`
   binary doesn't match, installs and `exec`s the pinned version instead.
3. **Root find + config load** (`crates/lattice/src/commands/run.rs`) —
   `lattice_config::find_root` walks up from the cwd for a `lattice.json`;
   `crate::schema::ensure_schema` writes `.lattice/schema.json` if absent;
   `lattice_config::load_config` parses and calls `LatticeConfig::validate`.
4. **Output mode** — `crate::cli::detect_output_mode` consults the real TTY
   and `CI` env via `lattice_output::detect_mode`. If the requested tasks'
   transitive closure includes a persistent task (`dagger::includes_persistent_task`),
   an otherwise-`Interactive` run is forced to `Raw`.
5. **Workspace discovery + driver detection** (`lattice_workspace::discover_workspaces`)
   — for each configured workspace, resolves the merged engine map
   (`lattice_config::resolve_engines`), walks the evidence ladder
   (`detect_drivers`) for `auto` workspaces, and resolves each root task's
   command (`infer_task_command`) unless a `scripts` override already supplies
   one. See [Driver detection](/lattice/docs/drivers) for the ladder itself.
6. **Graph construction** (`dagger::build_execution_graph_multi`) — expands
   the requested (possibly stacked) root tasks across the resolved workspaces
   into `TaskNode`s, wires `^task`/bare-`task` edges, rejects cycles and a
   persistent task with a dependent, and topologically sorts the result into
   an `ExecutionGraph`.
7. **Schedule** (`dagger::build_schedule`) — flattens that graph into the
   petgraph-independent `Schedule` described above.
8. **Scheduler execution** (`lattice_runner::execute_tasks`,
   `crates/lattice-runner/src/lib.rs`) — for each workspace up front, resolves
   its merged engines into a `PATH` prefix and identity string via
   `lattice_workspace::toolchain::provision_and_resolve` (memoized so an
   identical engine spec provisions once). Then it drives the in-degree
   scheduler: a task ready to run (indegree zero) is spawned as a Tokio task
   under a `Semaphore`-capped concurrency. Each spawned task
   (`run_one`) computes its cache key via `lattice_cache::compute_key`, looks
   it up through the `CacheStore` (`lattice_cache::LocalStore`), and either
   restores the cached artifact or runs the command through the platform
   shell (`sh -c` / `cmd /C`) with the resolved `PATH` prepended. On success it
   stores outputs back through the same `CacheStore`. See
   [Cache internals](/lattice/docs/cache-internals) and
   [Engines and provisioning](/lattice/docs/engines) for those two subsystems
   in depth.
9. **Event emission + reporting** — every state change (`Started`, `CacheHit`,
   `Output`, `Finished`, `Failed`, `Skipped`) is a `lattice_output::TaskEvent`
   sent over an internal `mpsc` channel and forwarded to the `Reporter` chosen
   in step 4, which renders it as a live TUI line or a plain log line.

## Tests

Tests live at two layers, plus one hermetic end-to-end script:

- **Unit tests**, colocated as `#[cfg(test)] mod tests` at the bottom of the
  same file they cover. `crates/dagger/src/lib.rs` covers graph construction,
  cycle/persistent-leaf rejection, and `Schedule` shape directly.
  `crates/lattice-cache/src/lib.rs` covers hash stability/sensitivity and the
  store/lookup/restore/corruption/prune round trip.
  `crates/lattice-workspace/src/lib.rs` covers every rung of the evidence
  ladder and role composition/conflict. `crates/lattice-workspace/src/toolchain.rs`
  covers `classify`, `parse_version`, `satisfies`, and a real provision-then-reuse
  round trip against a fake installed tool. `crates/lattice-runner/src/lib.rs`
  covers ordering, concurrency, fail-fast vs. `--continue`, the cache round
  trip, and persistent tasks not blocking the graph, all against a
  `RecordingReporter` test double. `crates/lattice/src/cli.rs` and
  `crates/lattice/src/drift.rs` cover the version-nag gating and pin-handover
  decision logic as pure functions.
- **Black-box e2e tests** under `crates/lattice/tests/`, each a `#[test]`
  binary that builds a throwaway repo in a `tempfile::TempDir` via the shared
  `Fixture` helper (`crates/lattice/tests/common/mod.rs`) and drives the
  compiled binary through `assert_cmd`. `e2e_run.rs` covers caching,
  filtering, dry-run, stacked/sequential phases, and keep-going.
  `e2e_toolchain.rs` covers provisioning and `PATH` injection with a fake
  `installCmd`. `e2e_halves.rs` covers running the task-runner and
  toolchain-manager halves independently, plus `prune` and `completions`.
  `e2e_init.rs`, `e2e_passthrough.rs`, and `e2e_upgrade.rs` cover scaffolding,
  wrapping a nested repo with its own runner, and version pinning.
- **`scripts/stress-test.sh`** — a single ~1000-line, dependency-free hermetic
  script that builds the release or debug binary, generates a production-shaped
  multi-language monorepo plus focused sub-repos in a temp directory, and
  exercises every command and flag against it: top-level surfaces, `init`,
  `setup`, `run` (ordering, filtering, dry-run, concurrency), caching, persistent
  tasks, failure handling, driver detection across roughly sixteen ecosystems,
  error paths, `prune`, nested-repo passthrough, and install/upgrade/pinning. It
  needs no network and no real language toolchains — driver detection is
  verified through `--dry-run`, and provisioning is exercised through a fake,
  locally-installed toolchain. `AGENTS.md` requires updating it alongside any
  change to CLI surface or behavior.

## Extension seams

Where a contributor is most likely to make a change:

**Adding a driver.** Add a `DriverSpec` to the `DRIVERS` array in
`crates/lattice-workspace/src/lib.rs`: `tool`, `role` (`Runtime`, `BuildTool`,
`PackageManager`, or `TaskRunner` — this decides both drive rank and what it
composes or conflicts with), `fingerprint` files, `version_cmd`, and an
`invoke_tpl` with a `{task}` placeholder. If any fingerprint should be read as
a native declaration rather than a generic lockfile, add it to
`is_native_fingerprint`. Regenerate the driver table on
[Toolchains](/lattice/docs/toolchains) from this array — never from memory.

**Adding a well-known engine.** Add one `(name, version command)` row to
`WELL_KNOWN_ENGINES` in `crates/lattice-config/src/lib.rs`. That single table
answers both questions — whether a string-form constraint on the name is valid,
and how to read the tool's version — so the two can never disagree. A driver in
`DRIVERS` must have a row here as well, with the same version command; a test in
each crate enforces both halves.

**Adding a subcommand.** Add a variant to the `Commands` enum in
`crates/lattice/src/cli.rs`, an `Args` struct plus `execute` method in a new
file under `crates/lattice/src/commands/`, and register the module in
`crates/lattice/src/commands/mod.rs`. Decide whether it needs the version-pin
handover (extend `skips_pin_handover` if not) and whether it needs the
version-drift nag (call `maybe_emit_version_nag` if so, as `run.rs` and
`setup.rs` do).

**Adding a cache backend.** Implement the `CacheStore` trait
(`lookup`, `store`, `restore`, `prune`, `touch`) from
`crates/lattice-cache/src/lib.rs` — `LocalStore` is the only implementation
today, and the trait exists so a future Lattice Cloud backend can slot in
without `lattice-runner` changing; `execute_tasks` holds its store behind
`Arc<dyn CacheStore>`.
