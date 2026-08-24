---
title: Architecture
description: The crate layout, the dependency direction, and the path one lattice run takes through it.
group: Reference
order: 7
---

# Architecture

This page is for people about to change Lattice itself. It names crates,
modules, and types directly. The rest of the docs do not.

## Crate layout

Lattice is a Cargo workspace of ten crates under `crates/`, plus the desktop
app's backend at `apps/desktop/src-tauri`. Nine of the ten ship in the tool. The
tenth exists only for the test suites.

| Crate | Owns |
| --- | --- |
| `lattice-config` | `lattice.json` types (`LatticeConfig`, `WorkspaceConfig`, `PipelineTask`, `Settings`, `EngineSpec`), the `WELL_KNOWN_ENGINES`, `MANIFESTS`, and `LOCKFILES` tables, `load_config`, `find_root`, validation, and the bundled JSON Schema (`schema::ensure_schema`) |
| `lattice-workspace` | Workspace discovery, the driver evidence ladder (`DRIVERS`, `detect_drivers`, `infer_task_command`), and, in its `toolchain` submodule, the engine gradient and provisioning |
| `dagger` | Builds the cross-workspace `ExecutionGraph` from resolved workspaces plus the task map, and flattens it into a `Schedule` |
| `lattice-cache` | Cache identity (`compute_key`, `compute_key_detailed`, `KEY_COMPONENTS`) and storage (the `CacheStore` trait, `LocalStore`) |
| `lattice-runner` | The async scheduler (`execute_tasks`): spawns tasks, wires the cache, injects toolchain `PATH`s, manages persistent children and signal teardown |
| `lattice-events` | `TaskEvent`, the `Reporter` trait, `CacheMiss`, `OutputLine`. Depends on `serde` and nothing else |
| `lattice-output` | `OutputMode`, the two terminal reporters (`InteractiveReporter`, `CiReporter`), and splash rendering |
| `lattice-project` | One opened repo (`Project`) and the pipeline both front ends run against it: `plan`, `run`, `RunOutcome`, the `scaffold` module `lattice init` writes through, and the `view` wire types |
| `lattice` | The `lattice` binary: the clap surface, the subcommands, the version-pin handover |
| `lattice-testkit` | Dev-only. Task commands spelled for whichever shell will run them, so a test means the same thing on every platform, plus the stand-in programs the suites put on `PATH` |

The root `lattice.json` declares every crate plus `apps/web` and `apps/desktop`
as workspaces, so the repo runs its own tasks through itself.

## Dependency direction

The internal edges, non-dev, regenerated with:

```sh
cargo metadata --no-deps --format-version 1 \
  | jq -r '[.packages[].name] as $ws | .packages | sort_by(.name)[]
           | "\(.name) -> " + ([.dependencies[] | select(.kind == null)
             | select(.name | IN($ws[])) | .name] | sort | join(", ")
             | if . == "" then "(nothing internal)" else . end)'
```

```text
dagger            -> lattice-config, lattice-workspace
lattice           -> dagger, lattice-cache, lattice-config, lattice-output,
                     lattice-project, lattice-runner, lattice-workspace
lattice-cache     -> lattice-config
lattice-config    -> (nothing internal)
lattice-desktop   -> dagger, lattice-config, lattice-events, lattice-project,
                     lattice-runner, lattice-workspace
lattice-events    -> (nothing internal)
lattice-output    -> lattice-events
lattice-project   -> dagger, lattice-cache, lattice-config, lattice-events,
                     lattice-runner, lattice-workspace
lattice-runner    -> dagger, lattice-cache, lattice-config, lattice-events,
                     lattice-workspace
lattice-testkit   -> (nothing internal)
lattice-workspace -> lattice-config
```

This is a graph rather than a chain, which is why it is listed as edges and not
drawn as a tree. Three crates depend on nothing internal. `lattice-config` is
the base every other crate models on top of. `lattice-events` depends on `serde`
alone and is the only thing the runner and the reporters share.
`lattice-testkit` appears solely as a dev-dependency, of `lattice`,
`lattice-project`, `lattice-runner`, and `lattice-workspace`.

`lattice-cache` depends on `lattice-config` and nothing else, because
`compute_key` needs a `PipelineTask` and a filesystem path rather than workspace
discovery or a graph. Cache identity is therefore testable in isolation from how
a task came to exist.

Three edges are worth reading twice, because the crate names suggest the
opposite.

### `lattice-runner` does not depend on `petgraph`

`petgraph` is a dependency of `dagger` alone. `dagger` builds a
`petgraph::graph::DiGraph<TaskNode, ()>` internally, topologically sorts it, then
calls `build_schedule` to flatten it into a petgraph-independent `Schedule`:
`Vec<HashSet<usize>>` prerequisites, `Vec<Vec<usize>>` dependents, and a
`Vec<usize>` indegree count, all indexed by position rather than by petgraph's
`NodeIndex`.

`lattice-runner` imports `dagger::{build_schedule, ExecutionGraph, Schedule}`
and its scheduling loop in `execute_tasks` walks plain integer indices. Two
things follow. `dagger`'s own `Schedule` unit tests exercise scheduling shape
without a live process, and the graph library can be replaced without touching
the runner.

### `lattice-runner` does not depend on the crate that renders its output

The events and the `Reporter` trait live in `lattice-events`. The runner emits
typed `TaskEvent`s and calls `Reporter` hooks (`run_start`, `event`,
`surface_failure`, `run_summary`, `note`, `warn`, `task_note`, `task_warn`,
`finish`), and never touches `console`, `indicatif`, or `println!` for task
status.

Both lived in `lattice-output` alongside the renderers once, which meant the
async executor depended on a terminal stack for two type definitions. The
desktop app is the third `Reporter` implementation and would have had to link
`indicatif` to watch a run.

`InteractiveReporter` (an `indicatif::MultiProgress` TUI) and `CiReporter`
(plain, greppable lines) are the two implementations `make_reporter` picks
between. `ChannelReporter` in `apps/desktop/src-tauri` is the third, and adding
it required no change to the runner.

### Both front ends run the same pipeline

Opening a repo is four steps that always happen together: find the root, keep the
schema file present, load the config, resolve the workspaces. `Project::open`
and `Project::open_root` in `lattice-project` do all four, and running a task
goes through `lattice_project::plan` and `lattice_project::run` in the same
crate. Neither the CLI subcommand nor the Tauri command holds any of it, so the
window and the terminal cannot disagree about what a task is or whether it needs
to run.

`lattice_project::run` returns a `RunOutcome` whose `exit_code()` the CLI exits
with: `0` completed, `1` failed, `130` interrupted. Nothing in the crate renders
anything or ends the process.

## Data flow of `lattice run`

One invocation, end to end:

1. **Entry.** `Cli::parse()` (clap), then `Cli::execute()`.
2. **Version-pin handover.** Unless the subcommand is `upgrade`, `completions`,
   or `version` (`Cli::skips_pin_handover`), `Cli::execute` calls
   `crate::drift::repo_root()` then `drift::honor_pin`. That reads
   `latticeVersion` out of the `lattice.json` text directly rather than through
   `lattice-config`, so a schema the running binary cannot parse still yields a
   pin. If a managed binary under `.lattice/bin` does not match, it installs and
   `exec`s the pinned version instead.
3. **Open the repo.** `Project::open` walks up from the cwd with
   `lattice_config::find_root`, calls `lattice_config::schema::ensure_schema` to
   write `.lattice/schema.json` if absent, then `lattice_config::load_config`
   (which parses and calls `LatticeConfig::validate`) and
   `lattice_workspace::discover_workspaces`.
4. **Reject unknown tasks.** `Project::require_known_tasks` fails before anything
   is provisioned or spawned, listing the tasks that do exist.
5. **Output mode.** `crate::cli::detect_output_mode` consults the real TTY and
   the `CI` environment variable through `lattice_output::detect_mode`. If the
   requested tasks' transitive closure holds a persistent task
   (`dagger::includes_persistent_task`), an otherwise-`Interactive` run is forced
   to `Raw`.
6. **Workspace discovery and driver detection.** Inside step 3,
   `discover_workspaces` resolves each workspace's merged engine map
   (`lattice_config::resolve_engines`), walks the evidence ladder
   (`detect_drivers`) for `auto` workspaces, and resolves each root task's
   command (`infer_task_command`) unless a `scripts` entry supplies one. See
   [Driver detection](/lattice/docs/drivers) for the ladder.
7. **Graph construction.** `dagger::build_execution_graph_selected` expands the
   requested root tasks across the resolved workspaces into `TaskNode`s, wires
   `^task` and bare-`task` edges, rejects cycles and a persistent task with a
   dependent, and topologically sorts the result into an `ExecutionGraph`. Under
   `--filter`, the matched workspaces are the roots: the graph narrows to them
   plus their transitive prerequisites, flagged on the node so `--dry-run` can
   tag them.
8. **Schedule.** `dagger::build_schedule` flattens that graph into the
   petgraph-independent `Schedule` above.
9. **Scheduler execution.** `lattice_runner::execute_tasks` resolves each
   workspace's merged engines into a `PATH` prefix and an identity string through
   `lattice_workspace::toolchain::provision_and_resolve`, memoized so an
   identical engine spec provisions once. Then it drives the in-degree scheduler:
   a node at indegree zero is spawned as a Tokio task under a `Semaphore`-capped
   concurrency. Each spawned task computes its cache key with
   `lattice_cache::compute_key_detailed`, looks it up through the `CacheStore`,
   and either restores the artifact or runs the command through the platform
   shell (`sh -c` or `cmd /C`) with the resolved `PATH` prepended. On success it
   stores outputs back through the same `CacheStore`. See
   [Cache internals](/lattice/docs/cache-internals) and
   [Engines and provisioning](/lattice/docs/engines).
10. **Event emission and reporting.** Every state change (`Started`,
    `CacheHit`, `CacheMiss`, `Output`, `Finished`, `Failed`,
    `PersistentExited`, `Skipped`) is a `lattice_events::TaskEvent` sent over an
    internal `mpsc` channel and forwarded to the `Reporter` chosen in step 5.

## Tests

Three layers.

**Unit tests** are colocated as `#[cfg(test)] mod tests` at the bottom of the
file they cover. `dagger` covers graph construction, cycle and persistent-leaf
rejection, and `Schedule` shape. `lattice-cache` covers hash stability and
sensitivity plus the store, lookup, restore, corruption, and prune round trip.
`lattice-workspace` covers every rung of the evidence ladder and role
composition and conflict; its `toolchain` submodule covers `classify`,
`parse_version`, `satisfies`, and a provision-then-reuse round trip against a
fake installed tool. `lattice-runner` covers ordering, concurrency, fail-fast
against `--continue`, the cache round trip, and persistent tasks: not blocking
the graph, reporting an exit, and staying silent when the exit was a shutdown
kill. All of it runs against a `RecordingReporter` double. The `lattice` crate
covers version-nag gating and pin-handover decisions as pure functions.

**Black-box end-to-end tests** live under `crates/lattice/tests/`, each a
`#[test]` binary that builds a throwaway repo in a `tempfile::TempDir` through
the shared `Fixture` helper in `common/mod.rs` and drives the compiled binary
with `assert_cmd`:

| File | Covers |
| --- | --- |
| `e2e_run.rs` | Caching, filtering, dry-run, stacked and sequential phases, keep-going |
| `e2e_toolchain.rs` | Provisioning and `PATH` injection with a fake `installCmd` |
| `e2e_halves.rs` | The task-runner and toolchain-manager halves used independently, plus `prune` and `completions` |
| `e2e_init.rs` | Scaffolding |
| `e2e_passthrough.rs` | Wrapping a nested repo that has its own runner |
| `e2e_upgrade.rs` | Version pinning and self-update |
| `e2e_guardrails.rs` | Defects that produced a plausible-looking run: a misordered build from a misspelled name, a hit serving a stale artifact, a prune taking the toolchains with it |

**`scripts/stress-test.sh`** is a single dependency-free hermetic script, just
under 2,000 lines, that builds the release or debug binary, generates a
production-shaped multi-language monorepo plus focused sub-repos in a temp
directory, and exercises every command and flag against it: top-level surfaces,
`init`, `setup`, `run` (ordering, filtering, dry-run, concurrency), caching,
persistent tasks, failure handling, driver detection across roughly sixteen
ecosystems, error paths, `prune`, nested-repo passthrough, install and upgrade
and pinning, the shipped agent skill, and the desktop app's wiring. It needs no
network and no real language toolchains: driver detection is verified through
`--dry-run`, and provisioning through a fake locally-installed toolchain.
`AGENTS.md` requires updating it alongside any change to CLI surface or behavior.

## Extension seams

Where a change is most likely to land.

### Adding a driver

Add a `DriverSpec` to the `DRIVERS` array in `lattice-workspace`: `tool`,
`language`, `roles`, `fingerprint` files, `version_cmd`, and an `invoke_tpl`
holding a `{task}` placeholder. `roles` is a slice, because a tool may fill
several, and the highest-ranked one is the role it competes for as a driver.
Rank runs `Runtime` < `BuildTool` < `PackageManager` < `TaskRunner`. That
decides both drive rank and what the tool composes with or conflicts against.

If a fingerprint should read as a native declaration rather than a generic
lockfile, add it to `is_native_fingerprint`. The `language` slug is what the
desktop app keys its ecosystem artwork off, and a stress-test assertion fails if
a slug has no artwork. Regenerate the tables on
[Toolchains](/lattice/docs/toolchains) from the array rather than from memory.

### Adding a well-known engine

Add one `(name, version command)` row to `WELL_KNOWN_ENGINES` in
`lattice-config`. That one table answers both whether a string-form constraint on
the name is valid and how to read the tool's version, so the two cannot
disagree. A driver in `DRIVERS` must also have a row here with the same version
command; a test in each crate enforces both halves.

### Adding a subcommand

Add a variant to the `Commands` enum in the `lattice` crate's CLI module, an
`Args` struct and `execute` method in a new file under
`crates/lattice/src/commands/`, and a registration in that directory's `mod.rs`.
Decide whether the subcommand needs the version-pin handover (extend
`skips_pin_handover` if not) and whether it needs the version-drift nag (call
`maybe_emit_version_nag` if so, as `run` and `setup` do).

### Adding a cache backend

Implement the `CacheStore` trait from `lattice-cache`: `lookup`, `store`,
`restore`, `prune`, and `touch`. Five more have default bodies —
`record_fingerprint` and `last_fingerprint` for miss explanations, and
`record_run`, `recorded_runs`, and `usage` for what `lattice stats` reports — so
a backend that has no answer for them still compiles. `LocalStore` is the only
implementation today. The trait exists so a future remote backend can slot in
without `lattice-runner` changing, and `execute_tasks` holds its store behind
`Arc<dyn CacheStore>`.
