---
title: Glossary
description: Every term the rest of the docs use, defined once, linked from everywhere.
group: Reference
order: 8
---

# Glossary

### Ambiguity

The state in which Lattice cannot name a single [driver](#driver) for an `auto`
[workspace](#workspace), either because two tools of the same [role](#role) both
have evidence or because the workspace shows only a bare ecosystem marker with
no tool-unique signal. The run halts before any task starts and prints a
copy-pasteable `engines` fix. See [Driver detection](/lattice/docs/drivers).

### Cache hit

The outcome in which a task's [cache key](#cache-key) matches a stored entry and
that entry passes its integrity check, so Lattice restores the stored outputs
instead of running the command. The check has three parts: the metadata parses,
the tarball opens, and the tarball's sha256 matches its recorded [output
digest](#output-digest). See [Caching](/lattice/docs/caching).

### Cache key

The sha256 hash that identifies one task's result, computed over everything that
can change what the task produces. Ten components feed it: the run environment
(Lattice version, platform, shell, workspace name, task name), the resolved
command, the [toolchain](#toolchain) identity, the cache keys of the task's
prerequisites, the `inputs`, `outputs`, and `ignore` pattern lists, the resolved
`env` and `globalEnv` values, the contents of every matched input file, the
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

### `dependsOn`

The name of two different fields. On a [workspace](#workspace) it lists other
workspaces by name, and it takes effect only where a task's `dependsOn` uses the
[caret prefix](#caret-prefix). On a [task](#task) it lists other tasks, with `^`
for a cross-workspace edge and a bare name for a same-workspace edge. See [Task
graph](/lattice/docs/task-graph).

### Drift

The gap between the `latticeVersion` pinned in the nearest `lattice.json` and
the version of the binary that is running. A binary Lattice installed under
`.lattice/bin` resolves drift by installing the pinned version and handing the
invocation to it before anything else runs. For a binary Lattice did not
install, the same check prints an advisory nag on a terminal and changes
nothing. See [Upgrading](/lattice/docs/upgrading).

### Driver

The tool that turns a workspace's named task into a real shell command, such as
`pnpm`, `cargo`, `go`, or `gradle`. Lattice ships 34 drivers, all but `pip` and
`kotlin` identified by a fingerprint file in the workspace directory. A driver
runs tasks. An [engine](#engine) is a versioned tool a task needs, and the two
resolve independently. See [Driver detection](/lattice/docs/drivers).

### Engine

A versioned tool named under `engines`, written either as a bare constraint
string for one of the 40 well-known names or as an object with `version`,
`versionCmd`, `installCmd`, and `bin`. The shape of the constraint, not its
name, selects [host mode](#host-mode), [validate-only](#validate-only), or
[provisioning](#provisioning). See [Engines and
provisioning](/lattice/docs/engines).

### Eviction

The removal of cache entries to bring the cache under a size limit, oldest
`lastUsed` first. `lattice prune` evicts against `--max-size` or
`settings.maxCacheSize`, and a run enforces `settings.maxCacheSize` after it
finishes. See [Cache internals](/lattice/docs/cache-internals).

### Filter

The `-f`/`--filter <PATTERN>` flag on `lattice run`, which selects the
workspaces whose `name` contains `PATTERN` as a substring. The matched
workspaces are the roots of the run: the graph also holds everything they depend
on, transitively, and nothing that depends on them. A filter that matches
nothing is not an error. See [Selecting what runs](/lattice/docs/filtering).

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

A lockfile or wrapper file that only one tool produces, such as
`pnpm-lock.yaml`, `bun.lockb`, `Cargo.lock`, `poetry.lock`, or `turbo.json`. It
is the lowest rung of the [driver](#driver) evidence ladder, below a declaration
and a native file. The same files also feed every task's [cache
key](#cache-key), so a dependency bump invalidates the cache even when the
lockfile is not listed in `inputs`. See [Driver
detection](/lattice/docs/drivers).

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

Which of two presentations `lattice run` uses: **interactive**, a live terminal
display that settles into a summary, or **raw**, a plain stream of
`workspace:task:` lines. Lattice picks raw when stdout is not a terminal, when
`CI` is set, when `-v`/`--verbose` or `settings.loquacious` applies, or when
the run pulls in a [persistent task](#persistent-task). See [Output and
logging](/lattice/docs/output-modes).

### Outputs

The `outputs` field on a task: glob patterns for the files a successful run
captures into the cached artifact, and the files a later [cache hit](#cache-hit)
restores. A file the command produces that `outputs` does not match is never
saved, and a failed task is never cached whatever it matched. See
[Caching](/lattice/docs/caching).

### Persistent task

A task declared with `persistent: true`, meaning it is not expected to exit. It
is never cached whatever `cache` says, it must be a leaf in the [task
graph](#task-graph) because nothing can wait on a task that never finishes, and
pulling one into a run forces [raw output](#output-mode) so its stream stays
visible. Lattice does not hold the scheduler for it, but it does watch it: an
exit is reported, and a non-zero exit fails the run. See [Persistent
tasks](/lattice/docs/persistent-tasks).

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

The [engine](#engine) mode that applies when a constraint has an `installCmd`,
in which Lattice runs that command into
`.lattice/toolchains/<engine>/<version>-<installHash>/`, version-checks the
result, writes a [pin](#pin), and prepends the resulting `bin` directory to the
task's `PATH`. Reusing an existing pin means one install per distinct
`installCmd`, not one per run. See [Engines and
provisioning](/lattice/docs/engines).

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

The single `lattice.json` at the repo root, which declares every
[workspace](#workspace), the root `engines`, the `tasks` map,
`globalDependencies`, `globalEnv`, and repo-wide `settings`. Lattice walks up
from the current directory to find it, so any subdirectory can run `lattice`
commands. A workspace may add its own `scripts` and `engines`, and nothing else.
See [Configuration](/lattice/docs/configuration).

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

### Stacked run

A single `lattice run` invocation naming more than one task, such as
`lattice run lint test build`. The named tasks merge into one combined [task
graph](#task-graph), so a dependency two of them share runs once and independent
work still parallelizes. `--sequentially` opts out and runs each task's graph to
completion before starting the next. See [Selecting what
runs](/lattice/docs/filtering).

### Task

A named unit of work declared under `tasks` in the [root config](#root-config),
resolved to one concrete shell command per workspace by that workspace's
[driver](#driver) or its own `scripts` entry. `build`, `test`, and `lint` are
names you choose, and Lattice reserves none. See [Task
graph](/lattice/docs/task-graph).

### Task graph

The expansion of one or more named tasks across every resolved workspace into a
directed graph of concrete task instances, built from each task's `dependsOn`
and each workspace's `dependsOn`. Lattice runs the graph in dependency order, in
parallel wherever the graph allows. See [Task graph](/lattice/docs/task-graph).

### Toolchain

The set of [engines](#engine) Lattice has resolved for a run, recording for each
whether it came from [host mode](#host-mode), [validate-only](#validate-only),
or [provisioning](#provisioning), and combined into a `PATH` prefix plus one
identity string that feeds every affected task's [cache key](#cache-key).
Resolution happens once per distinct merged engine map and is reused by every
workspace that shares one. See [Engines and
provisioning](/lattice/docs/engines).

### Validate-only

The [engine](#engine) mode that applies when a constraint has a version but no
`installCmd`, in which Lattice runs a version command against whatever is on
`PATH` and fails before any task starts if the result does not satisfy the
constraint. Nothing is installed either way. See [Engines and
provisioning](/lattice/docs/engines).

### Workspace

One directory Lattice runs tasks in: a directory with its own manifest, declared
by `name` and a literal `path` in the [root config](#root-config)'s `workspaces`
list. A workspace is the unit of [driver](#driver) detection, task running, and
caching, and it is one part of a [project](#project) rather than the whole repo.
`auto: false` opts a workspace out of driver detection, leaving `scripts` as the
only source of its commands. See [Workspaces](/lattice/docs/workspaces).
