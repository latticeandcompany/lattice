---
title: Glossary
description: Terms of art the rest of the docs use, defined once, linked from everywhere.
group: Reference
order: 8
---

# Glossary

One entry per term, alphabetically. Each links to the page that covers it in
depth — read this page for the precise meaning, then follow the link for the
worked example.

### Ambiguity

The failure mode when Lattice cannot pick a single [driver](#driver) for an
[auto](#workspace) workspace: two tools of the same [role](#role) both have
evidence, or a workspace shows only a bare generic marker (a lone
`package.json`, `pom.xml`, …) with no tool-unique signal. Lattice never
guesses in this case — it halts before any task runs and prints a
copy-pasteable `engines` fix (`AmbiguityError`,
`crates/lattice-workspace/src/lib.rs:354-386`). See [Driver
detection](/lattice/docs/drivers).

### Cache hit

The outcome when a task's [cache key](#cache-key) matches a stored entry
**and** that entry passes the integrity check: its metadata parses, its
tarball opens, and the tarball's sha256 matches the digest recorded when it
was written. Only then does Lattice restore the stored outputs instead of
running the command. See [Caching](/lattice/docs/caching).

### Cache key

The sha256 hash that identifies a task's result: a domain-separated,
length-prefixed digest over the task name, its resolved command, every file
matched by `inputs` (minus `ignore`, contents included), any tool-unique
[lockfile evidence](#lockfile-evidence) present in the workspace, the
resolved value of every `env` variable the task names, the
[toolchain](#toolchain) identity, and the running Lattice version
(`compute_key`, `crates/lattice-cache/src/lib.rs:353-408`). Change any one
input and the key changes. See [Cache internals](/lattice/docs/cache-internals).

### Cache miss

Anything that isn't a [cache hit](#cache-hit): no stored entry for the key,
missing metadata, a tarball that won't open, or a tarball whose digest
doesn't match the recorded one. A miss only ever costs a re-run — it never
hands back the wrong output. See [Caching](/lattice/docs/caching).

### Drift

The gap between the `latticeVersion` pinned in the nearest `lattice.json` and
the binary actually running. When a managed binary under `.lattice/bin`
drifts from the pin, Lattice installs the pinned version and hands the
invocation to it before anything runs; the interactive-only version-drift nag
is the advisory version of the same check for a binary Lattice didn't install
(`crates/lattice/src/drift.rs`). See [Upgrading](/lattice/docs/upgrading).

### Driver

The tool that turns a workspace's named task into a real shell command —
`pnpm`, `cargo`, `go`, `gradle`, and around 30 others Lattice recognizes by
fingerprint (`DRIVERS`, `crates/lattice-workspace/src/lib.rs:70-288`). Not to
be confused with an [engine](#engine): a driver *runs* tasks, an engine is a
*versioned tool* a workspace needs on its `PATH` or provisioned. The same
name can be both — `cargo` is a driver (it invokes `cargo build`) and also
appears in `WELL_KNOWN_ENGINES` (its version can be constrained) — but the
two concepts are resolved independently. See [Driver
detection](/lattice/docs/drivers).

### Engine

A versioned tool named under `engines` — `node`, `rust`, `go`, or anything
else with a version constraint that matters, string-form for a well-known
name or object-form (`version`, `versionCmd`, `installCmd`, `bin`) for
anything else (`EngineSpec`, `crates/lattice-config/src/lib.rs:33-83`). An
engine's *shape* — not its name — selects [host mode](#host-mode),
[validate-only](#validate-only), or [provisioning](#provisioning). Not to be
confused with a [driver](#driver): an engine is what Lattice makes sure is
the right version; a driver is what Lattice invokes to run a task. See
[Engines and provisioning](/lattice/docs/engines).

### Filter

The `-f`/`--filter <PATTERN>` flag on `lattice run`, which keeps only
workspaces whose `name` contains `PATTERN` (a substring match, not a glob)
before the task graph is built. Filtering removes a non-matching workspace
from the graph entirely — it never pulls in that workspace's dependencies,
and it is never an error, even when nothing matches
(`crates/lattice/src/commands/run.rs:39,107-111`). See [Selecting what
runs](/lattice/docs/filtering).

### Host mode

The engine mode when a constraint has neither a version nor an `installCmd`:
Lattice trusts whatever the [driver](#driver) or task finds on `PATH` and
checks nothing (`EngineMode::HostPath`,
`crates/lattice-workspace/src/toolchain.rs:19-20`; selected by `classify`,
`crates/lattice-workspace/src/toolchain.rs:85-100`). See [Engines and
provisioning](/lattice/docs/engines).

### Inputs

The `inputs` field on a task: glob patterns for the files whose *contents*
enter the [cache key](#cache-key). A task with no `inputs` has no files in
its key at all, so an unchanged command with an unchanged environment always
hits, even after its source changes (`crates/lattice-config/src/lib.rs:113`).
See [Caching](/lattice/docs/caching).

### Lockfile evidence

A lockfile or wrapper file only one tool produces — `pnpm-lock.yaml`,
`bun.lockb`, `Cargo.lock`, `poetry.lock`, `turbo.json`, and similar. It is the
lowest rung of the [driver](#driver) evidence ladder (rung 3, below a
declaration and a native file) and, independently, one of the inputs hashed
into every task's [cache key](#cache-key) so a dependency bump invalidates
the cache even when you forgot to list the lockfile in `inputs`
(`LOCKFILES`, `crates/lattice-cache/src/lib.rs:19-30`). See [Driver
detection](/lattice/docs/drivers) and [Caching](/lattice/docs/caching).

### Outputs

The `outputs` field on a task: glob patterns for the files a successful run
captures into the cached artifact, and what a later [cache hit](#cache-hit)
restores. A file the command produces but `outputs` doesn't match is simply
never saved. Only a successful run is ever stored — a failed task is never
cached, matching `outputs` or not. See [Caching](/lattice/docs/caching).

### Output digest

The sha256 hex of a cached artifact's tarball bytes, recorded in its
`.meta.json` as `output_digest` when the entry is written
(`crates/lattice-cache/src/lib.rs:46`). A lookup recomputes this digest from
the tarball on disk and only reports a hit if the two match — this is what
turns a corrupted or partially-written artifact into a miss instead of a
false hit. See [Cache internals](/lattice/docs/cache-internals).

### Persistent task

A task declared with `persistent: true` — a dev server, watcher, or anything
not meant to exit. It is never cached regardless of `cache`, it must be a
leaf in the [task graph](#task-graph) (nothing may depend on it, since it
never completes), and pulling one into a run's closure forces raw/CI output
so its streaming output stays visible instead of being collapsed behind a
live TUI (`crates/lattice-config/src/lib.rs:121-124`,
`crates/dagger/src/lib.rs:193-208`). See [Persistent
tasks](/lattice/docs/persistent-tasks).

### Pin

A version fixed against drift, in two distinct places. `latticeVersion` in
the root `lattice.json` pins which Lattice binary a repo runs, and a running
invocation switches to it automatically (see [Drift](#drift)). Separately,
each provisioned [engine](#engine) writes its own `pins.json` — the exact
version installed and the hash of the `installCmd` that produced it — into
its toolchain directory, so a later run reuses that install instead of
repeating it (`ToolchainPins`, `crates/lattice-workspace/src/toolchain.rs:111-118`).
See [Upgrading](/lattice/docs/upgrading) and [Engines and
provisioning](/lattice/docs/engines).

### Provisioning

The engine mode when a constraint has an `installCmd`: Lattice runs that
command itself into a content-addressed directory under
`.lattice/toolchains/<engine>/<version>-<installHash>/`, version-checks the
result, writes a [pin](#pin), and prepends the resulting `bin` directory to
the task's `PATH` (`EngineMode::Provisioned`,
`crates/lattice-workspace/src/toolchain.rs:242-398`). Reusing an existing pin
means installation happens once per distinct `installCmd` content, not once
per run. See [Engines and provisioning](/lattice/docs/engines).

### Role

The kind of job a [driver](#driver) does: `Runtime`, `BuildTool`,
`PackageManager`, or `TaskRunner`, ranked in that order
(`crates/lattice-workspace/src/lib.rs:29-47`). Drivers with *different* roles
compose into one stack for a workspace — a Node runtime plus a pnpm package
manager is not a conflict, and the higher-ranked role drives named tasks.
Drivers competing for the *same* role are an [ambiguity](#ambiguity) unless a
declaration names one. A pure `Runtime` can never drive tasks on its own. See
[Driver detection](/lattice/docs/drivers).

### Root config

The single `lattice.json` at the repository root — the one file that
declares every [workspace](#workspace), the root `engines`, the `tasks` map,
and repo-wide `settings` (`LatticeConfig`,
`crates/lattice-config/src/lib.rs:261-273`). `find_root` walks up from the
current directory looking for it, so any subdirectory can run `lattice`
commands. A workspace may add its own `scripts` and `engines`, but the task
graph, cache settings, and workspace list live only in the root config. See
[Configuration](/lattice/docs/configuration).

### Schedule

The runner-facing shape of the [task graph](#task-graph), built from it but
independent of the graph library: for each task, which other tasks must
finish first (`prerequisites`), which tasks move closer to ready once it
finishes (`dependents`), and how many prerequisites are still outstanding
(`indegree`) (`Schedule`, `crates/dagger/src/lib.rs:230-278`). The runner
drives execution from the schedule alone; it never touches the graph
directly. See [Task graph](/lattice/docs/task-graph).

### Stacked run

Naming more than one task in a single `lattice run` invocation, e.g. `lattice
run lint test build`. The tasks are merged into one combined
[task graph](#task-graph) rather than run as separate graphs, so a dependency
two of them share executes once and independent work still parallelizes.
`--sequentially` opts out: it runs each named task's graph to completion
before starting the next (`build_execution_graph_multi`,
`crates/dagger/src/lib.rs:94-214`). See [Selecting what
runs](/lattice/docs/filtering).

### Task

A named unit of work under `tasks` in `lattice.json` — `build`, `test`,
`lint`, or anything else — resolved to a concrete shell command per workspace
by that workspace's [driver](#driver) or its own `scripts` override
(`PipelineTask`, `crates/lattice-config/src/lib.rs:110-118`). A task's
`dependsOn` names other *tasks* (see [workspace `dependsOn`](#workspace) for
the field of the same name that means something different). See [Task
graph](/lattice/docs/task-graph).

### Task graph

The expansion of one or more root tasks across every resolved workspace into
a directed graph of concrete task instances, built from each task's
`dependsOn` (`^task` for a cross-workspace edge, a bare `task` for a
same-workspace edge) and each workspace's own `dependsOn`
(`build_execution_graph_multi`, `crates/dagger/src/lib.rs:94-214`). Lattice
runs it in dependency order, in parallel where the graph allows. See [Task
graph](/lattice/docs/task-graph).

### Toolchain

The set of engines Lattice has resolved for a run: for each, whether it came
from [host mode](#host-mode), [validate-only](#validate-only), or
[provisioning](#provisioning), combined into a `PATH` prefix and a single
identity string that feeds every affected task's [cache key](#cache-key)
(`ResolvedToolchains`, `crates/lattice-workspace/src/toolchain.rs:102-108`).
Resolution happens once per distinct merged engine map and is reused across
workspaces that share one. See [Engines and provisioning](/lattice/docs/engines).

### Validate-only

The engine mode when a constraint has a version but no `installCmd`: Lattice
runs a version command against whatever is already on `PATH` and fails
before any task starts if the result doesn't satisfy the constraint. Nothing
is installed either way (`EngineMode::ValidateOnly`,
`crates/lattice-workspace/src/toolchain.rs:21-23`). See [Engines and
provisioning](/lattice/docs/engines).

### Workspace

A single project directory — the unit of task running and caching. Declared
explicitly by `name` and a literal `path` (never a glob) under the root
`lattice.json`'s `workspaces` list (`WorkspaceConfig`,
`crates/lattice-config/src/lib.rs:89-105`). A workspace's own `dependsOn`
names other *workspaces* by name and only takes effect where a task's
`dependsOn` uses a `^`-prefixed token — on its own it declares nothing about
scheduling (see [task `dependsOn`](#task) for the field of the same name that
means something different). `auto: false` opts a workspace out of
[driver](#driver) detection entirely: you declare its `scripts` and `engines`
yourself and nothing is inferred. See [Workspaces](/lattice/docs/workspaces).
