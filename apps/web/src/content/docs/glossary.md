---
title: Glossary
description: Every term the rest of the docs use, defined once, linked from everywhere.
group: Reference
order: 8
---

# Glossary

### Ambiguity

The state in which Lattice cannot name a single [driver](#driver) for a
[workspace](#workspace): a `package.json` or `pyproject.toml` with no lockfile
or `packageManager` field here or above, or two tools of the same [role](#role)
tied on equal evidence. The run halts before any task starts and prints the
[override](#override) that fixes it. See [Driver
detection](/lattice/docs/drivers#when-lattice-stops-to-ask).

### Cache hit

The outcome in which a task's [cache key](#cache-key) matches a stored entry and
that entry passes its integrity check, so Lattice restores the stored outputs
instead of running the command. The check has three parts: the metadata parses,
the tarball opens, and the tarball's sha256 matches its recorded [output
digest](#output-digest). See [Caching](/lattice/docs/caching).

### Cache key

The sha256 hash that identifies one task's result, computed over everything that
can change what the task produces. Eleven components feed it: the run environment
(Lattice version, platform, shell, workspace name, task name), the resolved
command, the [toolchain](#toolchain) identity, the cache keys of the task's
prerequisites, the files of every workspace it depends on, directly or not,
the `inputs`, `outputs`, and `ignore` pattern lists, the resolved `env` values,
the resolved `globalEnv` values, the contents of every matched input file, the
[manifests](#manifest) and lockfiles present in the workspace and at the repo
root, and the digest of `globalDependencies`. See [Cache
internals](/lattice/docs/cache-internals).

### Cache miss

Anything that is not a [cache hit](#cache-hit): no stored entry for the key,
missing metadata, a tarball that will not open, or a tarball whose digest does
not match the recorded one. A miss runs the command. See
[Caching](/lattice/docs/caching).

### Caret prefix

The `^` that turns a task's `dependsOn` entry into a cross-workspace edge, so
`"dependsOn": ["^build"]` means the `build` task of every workspace this one
depends on. A `dependsOn` entry with no `^` names a task in the same workspace.
See [Task graph](/lattice/docs/task-graph).

### Check mode

The [engine](#engine) mode that applies when a constraint has a version but no
`installCmd`, in which Lattice runs a version command against whatever is on
`PATH` and fails before any task starts if the result does not satisfy the
constraint. Nothing is installed. See [Engines and
provisioning](/lattice/docs/engines).

### `dependsOn`

The name of two different fields. On a [task](#task) it lists other tasks: `^`
for the same task in each workspace this one depends on, a bare name for a task
in the same workspace. In an [override](#override) it lists workspaces this one
depends on beyond what its manifest says, such as an edge between languages.
Edges within one ecosystem come from the manifests. See [Task
graph](/lattice/docs/task-graph).

### Drift

The gap between the `latticeVersion` pinned in the nearest `lattice.json` and
the version of the binary that is running. A binary Lattice installed under
`.lattice/bin` resolves drift by installing the pinned version and handing the
invocation to it before anything else runs. For a binary Lattice did not
install, the same check prints an advisory nag on a terminal and changes
nothing. See [Upgrading](/lattice/docs/upgrading).

### Driver

The tool that runs a workspace's tasks, such as `pnpm`, `cargo`, `go`, or
`gradle`. Lattice ships 34 and picks one per workspace from the files it holds,
from a lockfile above it for a JavaScript or Python package, or from a manifest
only one tool reads. `overrides.<ws>.driver` names one outright. The driver
decides which tasks a workspace has and what command each runs. An
[engine](#engine) is a versioned tool, and never selects a driver. See [Driver
detection](/lattice/docs/drivers).

### Engine

A versioned tool named under `engines`, written as a bare constraint string for
one of the 40 well-known names or as an object with `version`, `versionCmd`,
`installCmd`, and `bin`. The shape, not the name, selects [host
mode](#host-mode), [check mode](#check-mode), or [provisioning](#provisioning).
Root `engines` apply to every workspace; an [override](#override)'s win per name.
See [Engines and provisioning](/lattice/docs/engines).

### Eviction

The removal of cache entries to bring the cache under a size limit, oldest
`lastUsed` first. `lattice prune` evicts against `--max-size` or
`settings.maxCacheSize`, and a run enforces `settings.maxCacheSize` after it
finishes. See [Cache internals](/lattice/docs/cache-internals).

### Filter

The `-f`/`--filter <PATTERN>` flag on `lattice run`, which selects the
workspaces whose name contains `PATTERN` as a substring. The matched workspaces
are the roots of the run: the graph also holds everything they depend on, and
nothing that depends on them. It replaces the [scope](#scope) a run gets from
its directory. See [Selecting what runs](/lattice/docs/filtering).

### Host mode

The [engine](#engine) mode that applies when a constraint has neither a version
nor an `installCmd`, in which Lattice trusts whatever the task finds on `PATH`
and checks nothing. See [Engines and provisioning](/lattice/docs/engines).

### Inputs

The `inputs` field on a task: glob patterns for the files whose contents feed
the [cache key](#cache-key). With `inputs` omitted, every file in the workspace
is hashed except what `.gitignore` excludes and what the task's own `outputs`
match. See [Caching](/lattice/docs/caching).

### Lockfile evidence

A lockfile or file only one tool produces, such as `pnpm-lock.yaml`,
`bun.lockb`, `Cargo.lock`, `poetry.lock`, or `turbo.json`, used to pick a
workspace's [driver](#driver). A file a developer wrote to name a tool, such as a
`packageManager` field, outranks it. Lockfiles also feed every task's [cache
key](#cache-key). See [Driver detection](/lattice/docs/drivers).

### Manifest

A file that defines what a task command actually does, such as `package.json`,
`Cargo.toml`, `Makefile`, or `Taskfile.yml`. A resolved command is usually an
indirection, so every manifest present in a workspace feeds that workspace's
[cache keys](#cache-key). See [Cache internals](/lattice/docs/cache-internals).

### Output digest

The sha256 hex of a cached artifact's tarball bytes, recorded in its
`.meta.json` as `output_digest` when the entry is written. A lookup recomputes
the digest from the tarball on disk and reports a hit only if the two match, so
a corrupted or partly written artifact is a [miss](#cache-miss) rather than a
false hit. See [Cache internals](/lattice/docs/cache-internals).

### Output mode

Which of two presentations `lattice run` uses: the **live display**, which
settles finished tasks into lines while a region redraws with running tasks and
their latest output, or the **stream**, one plain `workspace:task:` line per
event. The stream is used when stdout or stderr is not a terminal, when `CI` is
set, with `--stream`, or when the run includes a [persistent
task](#persistent-task). `-v` adds detail to either. See [Output and
logging](/lattice/docs/output-modes).

### Outputs

The `outputs` field on a task: glob patterns for the files a successful run
captures into the cached artifact, and the files a later [cache hit](#cache-hit)
restores. A file the command produces that `outputs` does not match is never
saved, and a failed task is never cached whatever it matched. See
[Caching](/lattice/docs/caching).

### Persistent task

A task that is not expected to exit, such as a dev server. `dev`, `start`,
`serve` and `watch` are persistent unless told otherwise; any task becomes one
with `persistent: true`. It is never cached, it must be a leaf in the [task
graph](#task-graph), and a run that includes one uses the stream. Lattice
watches it: an exit is reported, and a non-zero exit fails the run. See
[Persistent tasks](/lattice/docs/persistent-tasks).

### Override

An entry under `overrides` in the [root config](#root-config), keyed by a
workspace name or by a glob over workspace paths, holding settings for the
workspaces it matches: a `name`, a `driver`, `engines`, extra `dependsOn`, and
`tasks` written out as commands or settings. See
[Configuration](/lattice/docs/configuration#overrides).

### Pin

A version fixed against [drift](#drift), in two unrelated places.
`latticeVersion` in the [root config](#root-config) pins which Lattice binary
the repo runs. Each provisioned [engine](#engine) writes its own `pins.json`,
recording the version installed and the hash of the `installCmd` that produced
it, so a later run reuses that install. See [Upgrading](/lattice/docs/upgrading)
and [Engines and provisioning](/lattice/docs/engines).

### Project

One opened repo with a `lattice.json` at its root, together with the config and
the resolved workspaces read from it. A project holds many
[workspaces](#workspace), and the two are never the same thing. The desktop app
opens a project. See [The desktop app](/lattice/docs/desktop-app).

### Provisioning

The [engine](#engine) mode that applies when an engine has an `installCmd`: Lattice
runs that command into `.lattice/toolchains/<engine>/<version>-<installHash>/`,
checks the version of what it installed, writes a [pin](#pin), and prepends the
install's `bin` to each task's `PATH`. One install per distinct `installCmd`. See
[Engines and provisioning](/lattice/docs/engines).

### Role

The kind of job a [driver](#driver) does, one of runtime, build tool, package
manager, or task runner, ranked in that order. A driver declares every role it
fills and competes with its highest-ranked one, so `deno` competes as a task
runner and `bun` as a package manager. Drivers holding different roles compose
into one stack and the highest-ranked one drives. Drivers holding the same role
are an [ambiguity](#ambiguity) unless a declaration names one. A driver whose
only role is runtime cannot drive tasks. See [Driver
detection](/lattice/docs/drivers).

### Root config

The single `lattice.json` at the repo root. It holds what the repo's own
manifests cannot say: extra workspace directories, `exclude`, `engines`, the
`tasks` map, [overrides](#override), `globalDependencies`, `globalEnv`, and
`settings`. Every key is optional. Lattice walks up from the current directory
to find it. See [Configuration](/lattice/docs/configuration).

### Run ledger

`stats.jsonl` in the cache directory, holding one appended JSON line per finished
run: when it ended, how many tasks it scheduled, how many hit the cache, how many
failed, the [saved time](#saved-time), and the elapsed time. `lattice stats`
reads it. A run appends a line only when it could store to the cache and
scheduled at least one task. The file is per-machine, never committed, untouched
by `lattice prune`, and deleted with the cache directory. See [Cache
internals](/lattice/docs/cache-internals).

### Saved time

The task time a run's [cache hits](#cache-hit) skipped, summed from the duration
each entry's metadata recorded when that entry was written. Reported at the end
of the summary line on any run with hits, and totalled by `lattice stats`. Task
time rather than wall clock: hits that would have run at the same moment each
add their own duration. See [Caching](/lattice/docs/caching).

### Schedule

The runner-facing form of the [task graph](#task-graph), recording for each task
which tasks must finish first, which tasks move closer to ready when it
finishes, and how many prerequisites are still outstanding. The runner drives
execution from the schedule alone and never touches the graph. See [Task
graph](/lattice/docs/task-graph).

### Scope

The workspace a run is narrowed to because it was started inside that
workspace's directory. The run covers the workspace and everything it depends
on, and says so. When the workspace has none of the tasks named, the run fails
rather than running nothing. A [filter](#filter) replaces it. See [Selecting
what runs](/lattice/docs/filtering#run-from-inside-a-workspace).

### Stacked run

A single `lattice run` invocation naming more than one task, such as
`lattice run lint test build`. The named tasks merge into one combined [task
graph](#task-graph), so a dependency two of them share runs once and independent
work still parallelizes. `--sequentially` opts out and runs each task's graph to
completion before starting the next. See [Selecting what
runs](/lattice/docs/filtering).

### Task

A name such as `build`, resolved to one shell command in each workspace that
has it: a script its manifest declares, a verb its [driver](#driver) has, or a
command an [override](#override) writes out. A task needs no entry in the `tasks`
map to run; that map configures how it behaves. `build`, `dev`, `start`, `serve`,
`watch`, `clean`, `deploy`, `publish` and `release` come with defaults. See [Task
graph](/lattice/docs/task-graph).

### Task graph

The expansion of one or more named tasks across every workspace that has them
into a directed graph of `workspace:task` nodes, linked by each task's
`dependsOn` and the dependencies between workspaces. Lattice runs it in
dependency order, in parallel wherever it allows. See [Task
graph](/lattice/docs/task-graph).

### Toolchain

The set of [engines](#engine) Lattice resolved for a workspace, recording for
each whether it came from [host mode](#host-mode), [check mode](#check-mode), or
[provisioning](#provisioning), combined into a `PATH` prefix and an identity
string that feeds every affected task's [cache key](#cache-key). See [Engines
and provisioning](/lattice/docs/engines).


### Workspace

One directory Lattice runs tasks in, named by a workspace file at the repo root
(`package.json` `workspaces`, `pnpm-workspace.yaml`, Cargo `[workspace]`,
`go.work`, uv, deno) or by `workspaces` in the [root config](#root-config), or
the repo root itself when nothing names any. It is named by its manifest. A
workspace is the unit of [driver](#driver) detection, task running, and caching,
and a Gradle, Maven or .NET build is one workspace however many subprojects it
has. See [Workspaces](/lattice/docs/workspaces).
