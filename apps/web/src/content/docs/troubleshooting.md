---
title: Troubleshooting
description: Symptom, cause, and fix for the failure modes you actually hit.
group: Guides
order: 7
---

# Troubleshooting

Symptoms, causes, fixes. For the exact text of every error Lattice prints, see
[Errors](/lattice/docs/errors).

## Detection and configuration

### A workspace halts with "ambiguous or undeclared task driver"

Lattice gathers candidate tools for the workspace from your `engines`
declaration, native files like `packageManager` or `.nvmrc`, and tool-unique
lockfiles. It halts when there are no candidates, when the only candidates are
runtimes, or when two candidates hold the same role (`bun.lockb` and
`pnpm-lock.yaml`, say). See [Driver detection](/lattice/docs/drivers) for the
model and [Errors](/lattice/docs/errors#ambiguous-or-undeclared-task-driver) for
the message shapes.

Fix: add an `engines` entry to that workspace in `lattice.json` naming the tool
that should run its tasks — a package manager, build tool, or task runner. The
line the error suggests resolves the halt, but it isn't always the tool you want,
so check it against what the workspace actually uses. Where no candidate could
drive tasks, the suggestion is the `auto: false` plus `scripts` form instead,
since no `engines` entry would help there.

### A task's resolved command isn't what you expected

An explicit entry in a workspace's `scripts` map always wins. Only a task with no
entry there falls back to the driver's inferred invocation. For JavaScript-family
drivers, inference requires the task name to exist in the manifest's own
`scripts`/`tasks` map. For direct-invoke drivers such as `cargo` and `go`,
inference never produces a command for a `persistent` task — there is no universal
`cargo dev` — so a persistent task on one of those needs an explicit `scripts`
entry.

Fix: run `lattice run <task> --dry-run`. It prints every `workspace:task` that
would run, in order, next to its fully resolved command, including
cross-workspace dependencies, and executes nothing:

```sh
lattice run build --dry-run
```

```text
❖ lattice  dry run · build
  → web:build   pnpm run build
  → cli:build   cargo build --release
```

If the command shown isn't the one you meant, add or edit that workspace's
`scripts` entry for the task.

`--dry-run` returns before toolchains are provisioned or `PATH` is adjusted, so it
shows the command as written, not as it would resolve once a provisioned tool is
first on `PATH`.

### A config validation error on `lattice.json`

Duplicate workspace names, an empty `path`, a bad `maxCacheSize` string, an
`engines` entry using the version-only string form for a tool Lattice can't
version-check — all are caught before any task runs, with a message naming the
field and value.

Fix: see [Errors](/lattice/docs/errors#config-loading-and-validation) for every
shape, and [Configuration](/lattice/docs/configuration) for the field reference.

### `unknown field` on a key you expected to work

```text
Error: unknown field `output` in tasks.build (lattice.json line 5, column 14)
Did you mean `outputs`?
Fields accepted here: dependsOn, inputs, outputs, ignore, env, persistent, cache
```

A key that is not part of the config at that level. Nothing ran. Two cases
account for most of these: a near miss the message already names, and a key that
used to be accepted and no longer is — `settings.logging`, or a `glob` on a
workspace entry.

Fix: delete the key, or write the one the message names. There is no setting that
relaxes this. If a key you believe is valid is being rejected, your binary is
older than the config — check `latticeVersion` against `lattice --version`.

### A cycle in the task graph

```text
cycle detected in task dependency graph
```

Two or more tasks depend on each other, directly or transitively, through
`dependsOn` — same-workspace or `^`-prefixed cross-workspace edges. There is no
valid run order, so nothing runs.

Fix: the error names the graph, not the edge, and `--dry-run` prints no order for
a graph that can't be scheduled, so read each involved task's `dependsOn` in
`lattice.json` and trace the loop. See [Task graph](/lattice/docs/task-graph) for
how `^task` and `task` build edges.

## Caching

### A task never hits the cache

The cache key hashes the task name, its resolved command, every file matched by
its `inputs` globs, tool-unique lockfiles in the workspace, the current values of
any env vars listed in `env`, the resolved toolchain identity, and the Lattice
version. A key that changes every run usually means something is feeding the hash
that shouldn't: an `inputs` glob matching a file that legitimately changes on
every run (a timestamp, a `.DS_Store`), or an `env` entry whose value differs
across shells and machines (an absolute path, a session id).

Fix: narrow `inputs`, add the file to `ignore`, or drop the `env` entry unless the
command's output genuinely depends on it. Run with `-l`/`--loquacious` to see the
truncated key and hit/miss per task:

```text
web:build: hash a1b2c3d4e5f6a7b8
web:build: cache miss
```

A different hash prefix on back-to-back runs with no source change points at one
of those causes. Loquacious mode shows the outcome, not which field changed the
hash, so narrow it by elimination.

### A task hits the cache when it shouldn't

The build reads a file, or an env var, that isn't declared. `inputs` matches only
what its globs say; a file read outside that set is invisible to the key, so
editing it changes nothing and the stale result replays. `env` is the same — only
the names you list are read and hashed. Neither case raises a warning.

Fix: widen the `inputs` glob or add the missing name to `env`. See
[Caching](/lattice/docs/caching) for what belongs in each. Use
`lattice run <task> --force` or `--no-cache` to force a fresh run while you
investigate.

### `--force`/`--no-cache` doesn't help, or the output looks wrong after a hit

A cache hit counts only if the metadata parses, the tarball opens, and its digest
matches what's recorded. Anything else is a miss and the task reruns, never a
false hit. If restoring outputs into the workspace fails partway — permissions,
disk space — that is a non-fatal warning and the task still ran fresh, so the
result on disk is correct even though the warning printed. See
[Errors](/lattice/docs/errors#cache-operations) for the warning text.

## Toolchains

### An engine version check fails on a teammate's machine but not yours

The constraint is in validate-only mode: a version range with no `installCmd`, so
Lattice checks whatever is on that machine's `PATH`. Machines with different tools
installed disagree. See [Errors](/lattice/docs/errors#validate-only-failures) for
the exact messages — command not found, version doesn't parse, version doesn't
satisfy the range.

Fix: get every machine's host tool onto the same version, or add an `installCmd`
to move the constraint into provision mode, where Lattice installs its own copy
into `.lattice/toolchains/` per machine. See [Engines and
provisioning](/lattice/docs/engines).

### `installCmd` provisioning fails partway

Provisioning stages the install into a temporary directory, runs `installCmd`,
version-checks the result, then renames the staged directory to its final
content-addressed path. A failure at any of those steps is fatal and names the
stage. See [Errors](/lattice/docs/errors#provisioning-failures).

Fix: rerun the same command. Nothing is left pinned on a partial failure, so
provisioning retries from scratch and there is no partial state to clean up by
hand.

### Toolchain resolution isn't shown by `--dry-run`

`--dry-run` prints resolved commands and returns before any toolchain is
provisioned or validated. An engine failure surfaces only when a task actually
runs, or via `lattice setup`, which provisions root engines up front.

Fix: to confirm a teammate's toolchain resolves before handing them a task, have
them run `lattice setup`.

## Output

### The interactive TUI doesn't show up

Lattice picks `Raw` output whenever stdout isn't a real terminal, a `CI`
environment variable is set, or `-l`/`--loquacious` was passed. On top of that,
`lattice run` forces `Raw` whenever the tasks being run pull a `persistent` task
into their closure, so the dev server's streaming output stays visible.
Redirecting output to a file or a pipe fails the TTY check, which is the most
common cause. See [Output and logging](/lattice/docs/output-modes).

### You wanted plain lines but got the interactive view

Fix: pass `-l`/`--loquacious`, set `settings.loquacious: true` in `lattice.json`,
or set `CI` in the environment. Any one forces `Raw`, which is also the readable
form when piping `lattice run` into another tool or a CI log viewer.

### Colored output shows up somewhere it shouldn't

Color is emitted only when stdout is a real terminal, in both modes. An `-l` run
at your shell has colored `workspace:task` labels; the same run piped or
redirected has none. If you see escapes in something that isn't a terminal,
whatever is running Lattice is presenting itself as one.

Fix: set `NO_COLOR=1` (any value) to suppress color everywhere without changing
the output mode.

### A persistent task never lets `lattice run` finish

Once a run starts a `persistent: true` task — a dev server, a watcher — that task
is detached and the run waits on a shutdown signal before exiting.
Non-persistent prerequisites still run to completion and their results are visible
first, but the process blocks until you send `Ctrl-C`. No flag makes a persistent
task's own exit end the run, since the task is defined by not exiting.

Fix: for day-to-day use, run the persistent task on its own in one terminal and
everything else separately — see [Persistent
tasks](/lattice/docs/persistent-tasks) and [Dev servers and
watchers](/lattice/docs/dev-servers). To have a run fail fast instead of waiting,
leave the persistent task out of the requested list and run its prerequisites
alone.

## Running

### A `--filter` ran more or fewer workspaces than you expected

`--filter <pattern>` matches workspaces whose **name** contains `pattern` — a
substring match, not a glob and not a path match. The matches are the roots of
the run, so the graph also holds everything they depend on, transitively. Those
extra nodes are tagged `(dependency)` under `--dry-run`. Nothing that depends on
a match is included. A filter matching nothing prints
`lattice: no workspaces matched filter '<pattern>'.` and exits 0.

Fix: match on the workspace `name` as declared in `lattice.json`, not a directory
name or a glob. If a prerequisite you expected is missing, check that the
depending workspace lists it in its `dependsOn` and that the task's `dependsOn`
carries the `^`. See [Selecting what runs](/lattice/docs/filtering).

### `.lattice/schema.json` is missing or your editor shows a stale schema

`lattice run`, `lattice setup`, and `lattice prune` each write the bundled schema
when `.lattice/schema.json` is absent. An existing file is left alone, even an old
one, so a copy you customized or committed is never churned.

Fix: delete the file and rerun any command that loads the config — `lattice run
... --dry-run` is enough — or rewrite it outright:

```sh
lattice init --force
```

`schema.json` is meant to be committed. It's the one thing under `.lattice/` that
`lattice init` does not add to `.gitignore`.

## Clean slate and gathering information

Everything under `.lattice/` is derived state:

| Path | What deleting it costs |
| --- | --- |
| `.lattice/cache/` | Cached task outputs, keyed by content. Every task reruns cold on the next `lattice run`. |
| `.lattice/toolchains/` | Provisioned engines. Any engine with an `installCmd` reprovisions next time it's needed — a network fetch, not a config change. |
| `.lattice/bin/` | Lattice's own self-managed versions, present when this repo pins `latticeVersion` to something other than the binary on your `PATH`. The next command re-downloads and re-verifies the pinned release. |
| `.lattice/schema.json` | The one file here meant to be committed. If it's gone from disk but still tracked, `git status` shows it deleted; if it's genuinely gone, the next `run`/`setup`/`prune` rewrites it. |

`rm -rf .lattice` is a complete reset. Nothing under it is required for
correctness, only for speed and for not reprovisioning tools you already have. It
never touches `lattice.json`, which lives at the repo root. You do not need to
rerun `lattice init` afterward.

Before filing an issue or asking a teammate for help, gather:

| Command | What it tells you |
| --- | --- |
| `lattice version` | The exact binary version that ran, useful when a repo pins `latticeVersion`. |
| `lattice run <tasks> --dry-run` | The graph and every resolved command, without running or caching anything. |
| `lattice run <tasks> -l` | Raw output plus cache hash and hit/miss lines, for caching questions and for full output to paste into a report. |

For the models behind these symptoms, see [Driver
detection](/lattice/docs/drivers), [Engines and
provisioning](/lattice/docs/engines), [Caching](/lattice/docs/caching), [Task
graph](/lattice/docs/task-graph), and [Output and
logging](/lattice/docs/output-modes). For the message-by-message reference, see
[Errors](/lattice/docs/errors).
