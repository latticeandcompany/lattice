---
title: Troubleshooting
description: Symptoms you actually hit, with the command that shows you what went wrong.
group: Guides
order: 7
---

# Troubleshooting

Symptom, cause, fix. For the exact text of every message Lattice prints, see
[Errors](/lattice/docs/errors).

Three commands answer most of these, so reach for them before reading further:

```sh
lattice run <task> --dry-run   # the graph and every resolved command, run nothing
lattice run <task> -l          # hashes, cache decisions, and each task's output
lattice version                # which binary actually ran
```

## Detection and configuration

### A run halts before any task starts, naming a workspace

```text
Error: workspace 'web' has an ambiguous or undeclared driver.
Candidate drivers: pnpm, npm, yarn, bun
Declare the driver in lattice.json, under this workspace:
  "engines": { "pnpm": ">=0.0.0" }
```

Lattice found more than one tool that could run that workspace's tasks and will
not guess between them. It halts the same way when it finds none, and when the
only candidates are runtimes, which cannot drive a named task on their own.

To fix it, add the `engines` entry to that workspace naming the tool that should
run its tasks. The line in the message resolves the halt, but it names the first
candidate that could drive tasks rather than the one your repo uses, so check it
before pasting it. Where nothing could have driven tasks, the message suggests
`"auto": false` plus a `scripts` map instead, because no `engines` entry would
help. See [Driver detection](/lattice/docs/drivers).

### A workspace with `"auto": false` halts on one task

```text
Error: workspace 'api' has "auto": false and declares no command for task 'build'. Add the command under this workspace's "scripts" map in lattice.json
```

Turning detection off means the workspace's `scripts` map is the only source of
its commands, and there is no entry for the task you named. An `auto` workspace
with no command for a task is skipped quietly; a declared one is an error,
because you said what it runs.

Add the entry, or drop `"auto": false` and let detection find the tool. See
[Workspaces](/lattice/docs/workspaces).

### A task's resolved command is not the one you expected

An entry in a workspace's `scripts` map always wins. Only a task with no entry
there falls back to what the driver would infer, and inference is not universal:
a JavaScript-family driver needs the task name to exist in the manifest's own
scripts map, and a direct-invoke driver such as `cargo` or `go` never infers a
command for a `persistent` task, because there is no `cargo dev`.

Print what would run:

```text
$ lattice run build --dry-run
❖ lattice  dry run · build
  → ui:build  pnpm run build
  → api:build  cargo build --release
```

Each line is `workspace:task` and the exact command the runner would hand to
`sh -c`, or to `cmd /C` on Windows. If it is not the command you meant, add or
edit that workspace's `scripts` entry.

`--dry-run` returns before any toolchain is provisioned, so it shows the command
as written, not as it will resolve once a provisioned tool is first on `PATH`.

### `unknown field` on a key you believe is valid

```text
Error: unknown field `output` in tasks.build (lattice.json line 3, column 31)
Did you mean `outputs`?
Fields accepted here: dependsOn, inputs, outputs, ignore, env, persistent, cache, timeout
```

The key is not part of the config at that level, and nothing ran. Most of these
are the near miss the message already names. The rest are keys that used to be
accepted and are not any more.

Delete the key, or write the one the message names. No setting relaxes this. If
the key really is valid, your binary is older than the config: compare
`latticeVersion` in `lattice.json` against `lattice version`.

### `the task graph has a cycle`

```text
Error: the task graph has a cycle
```

Two or more tasks depend on each other, directly or through other tasks, so
there is no order to run them in. Nothing ran.

The message names the graph rather than the edge, and `--dry-run` prints no
order for a graph it cannot schedule, so trace it by reading `dependsOn` on each
task involved. A same-workspace edge is a bare task name; a cross-workspace edge
carries `^`. See [Task graph](/lattice/docs/task-graph).

### A validation error names a field in `lattice.json`

Duplicate workspace names, an empty `path`, an unparseable `maxCacheSize`, and a
version-only engine string for a tool Lattice cannot version-check are all
caught before any task starts, with the field and the value in the message. See
[Errors](/lattice/docs/errors) for every shape and
[Configuration](/lattice/docs/configuration) for the field reference.

## Caching

### A task misses the cache every run

Run it with `-l` and read the two lines Lattice prints per task:

```text
lattice: ui:build: hash 26be571e2ec773a7
lattice: ui:build: cache miss: inputs changed
```

The `cache miss:` line names which part of the key moved, which is where to
look. `inputs changed` with no source edit means an `inputs` glob is matching
something that rewrites itself on every run: a timestamp, a log, a
`.DS_Store`. `environment changed` on an otherwise identical run usually means
the Lattice version moved. `dependencies changed` is not a problem on its own,
because an upstream change is supposed to reach downstream.

Narrow the glob, add the file to `ignore`, or drop the `env` entry whose value
differs between machines. For what each component name covers, see [Cache
internals](/lattice/docs/cache-internals).

Two misses name no component at all. `cache miss (nothing cached for this task
yet)` means the task has never finished here. `cache miss (the entry for this
key is no longer in the cache)` means the key is unchanged and the entry went:
evicted under `settings.maxCacheSize`, swept by `lattice prune`, or rejected as
corrupt.

### A task hits the cache when the result is stale

The task reads a file, or an environment variable, that nothing declares.
`inputs` matches only what its globs say, and `env` reads only the names you
list, so anything outside both is invisible to the key and a stale result
replays. Neither case warns.

Widen the `inputs` glob or add the name to `env`. While you investigate, force a
fresh run with `--force`, which also replaces the stored entry, or `--no-cache`,
which stores nothing.

A file above the workspace cannot be named by `inputs`, because `inputs` is
relative to the workspace directory. A base `tsconfig.json`, a shared schema
directory, or a root `.env` goes in `globalDependencies` instead, and a
repo-wide variable goes in `globalEnv`:

```json
{
  "globalDependencies": ["tsconfig.base.json", "proto/**"],
  "globalEnv": ["NODE_ENV"]
}
```

### A task warns about its outputs and is never cached

```text
lattice: warning: api:build: failed to cache outputs: no files matched outputs ["dist/**"], so nothing was cached. Check that the patterns are relative to the workspace, and that the task writes there
```

The task declares `outputs` and produced none of them. Lattice refuses to store
an empty artifact, so the task succeeds, nothing is cached, and it runs again
next time. The warning repeats every run.

Three things cause it. The patterns are relative to the workspace directory and
not to the repo root, so `outputs` on a task every workspace shares has to match
in all of them. The command may be writing somewhere else, which a look at the
directory after a run will settle. Or the command genuinely produces nothing on
this machine, which is common for a build that skips its native step when the
compiler is missing; drop `outputs` from that task and let the cache entry
record only that it succeeded.

### The run warns about restoring a cache entry

Restoring outputs into a workspace can fail on permissions or disk space. That
is a warning, not a failure: the task ran fresh instead, so what is on disk is
correct. A cache hit itself is all-or-nothing. The metadata has to parse, the
tarball has to open, and its digest has to match what was recorded, so a corrupt
entry is a miss and the task reruns rather than replaying something wrong.

## Toolchains

### An engine check fails on one machine and passes on another

```text
Error: engine 'node' on PATH is 26.0.0, which does not satisfy the constraint '>=999'
```

The constraint has a version and no `installCmd`, so Lattice checks whatever is
on that machine's `PATH` and installs nothing. Machines with different tools
disagree.

Either get every machine's host tool onto the same version, or add an
`installCmd` so Lattice installs its own copy under `.lattice/toolchains/` per
machine. See [Engines and provisioning](/lattice/docs/engines).

### An engine is rejected before anything is checked

```text
Error: engine 'frobnicate' in root uses the string form, which carries only a version. 'frobnicate' is not a well-known engine, so Lattice cannot version-check it on its own. Use the object form with a `versionCmd`, like this: "frobnicate": { "version": ">=1.0.0", "versionCmd": "frobnicate --version" }
```

The bare-string form of an `engines` entry works only for the tools Lattice
already knows how to version-check. For anything else, use the object form and
give it a `versionCmd`. Guessing the flag would be worse than asking.

### `installCmd` fails partway through

Provisioning stages the install into a temporary directory, runs `installCmd`,
version-checks the result, and only then renames the staging directory to its
final content-addressed path. A failure at any step is fatal and names the step.

Rerun the same command. Nothing was pinned, so provisioning starts over and
there is no half-installed toolchain to clean up by hand.

### `--dry-run` says nothing about toolchains

`--dry-run` resolves commands and returns before any engine is validated or
provisioned. An engine problem surfaces when a task actually runs, or up front
from `lattice setup`, which provisions the root engines first.

To check that a teammate's toolchain resolves before handing them work, have
them run `lattice setup`.

## Output

### You expected the live display and got plain lines

Lattice uses raw, line-by-line output whenever stdout is not a terminal, `CI` is
set to any value, `-l`/`--loquacious` was passed, or `settings.loquacious` is
`true`. A run that pulls a `persistent: true` task into its graph is raw too,
even at a terminal, so a dev server's output stays visible.

Redirecting or piping fails the terminal check, and that is the usual cause.
Nothing forces the live display back on. See [Output and
logging](/lattice/docs/output-modes).

### You wanted plain lines and got the live display

Pass `-l`, set `settings.loquacious` to `true` in `lattice.json`, or set `CI` in
the environment. Any one of them gives you the raw stream, which is also the
readable form when you are piping `lattice run` into something else.

### Color shows up where it should not

Color follows the terminal, not the output mode. An `-l` run at your shell has
colored `workspace:task` labels and the same run redirected to a file has none.
Escapes in something that is not a terminal mean whatever is running Lattice is
presenting itself as one.

Set `NO_COLOR` to any value to suppress color everywhere without changing the
layout or the mode.

### A run with a dev server in it never finishes

Once a run starts a `persistent: true` task it waits, streaming that task's
output, until you interrupt it. Everything else in the graph finishes first and
its results are visible above. No flag changes this: it is what a `dev` run is.

`Ctrl-C` stops it, and the run exits `130` with no message about the interrupt.
A run also ends on its own once every persistent task in it has exited, so a task marked persistent by mistake
no longer blocks: it gets an `exited (code 0)` line and the run prints its
summary. For a run that always terminates, leave the persistent task out of the
names you pass. See [Run dev servers](/lattice/docs/dev-servers).

### A task cannot depend on your `dev` task

```text
Error: task 'dev' in workspace 'web' is persistent, so no other task may depend on it
```

Nothing can wait on a task that never exits, so a persistent task has to be a
leaf. If another task needs what the dev server produces, depend on the build
step that produces it. See [Persistent tasks](/lattice/docs/persistent-tasks).

## Running

### The run printed `FAILED` lines but the summary says `0 failed`

```text
docs:build: running
ui:build: running
docs:build: FAILED
ui:build: FAILED
lattice: 2 tasks, 0 cached, 0 failed, 1.49s
```

The run was interrupted. `Ctrl-C`, or the `SIGTERM` a CI runner sends when
someone cancels the job, stops the scheduler and terminates each running task's
whole process group. Those children exit non-zero, so each task reports `FAILED`,
and none of them is counted as a failure.

Nothing prints the word interrupted. The exit code is the only signal, and it is
`130`, so read `$?` rather than the summary. A build that genuinely broke exits
`1`. See [Run Lattice in CI](/lattice/docs/continuous-integration).

### `--filter` ran more or fewer workspaces than you expected

`--filter <pattern>` is a substring match against a workspace's `name`, not
against its path and not a glob. The matches are the roots of the run, so the
graph also holds everything they depend on, transitively, tagged `(dependency)`
under `--dry-run`. Nothing that depends on a match is included. A pattern that
matches nothing prints `lattice: no workspaces matched filter '<pattern>'.` and
exits `0`.

Match on the `name` as declared in `lattice.json`. If a prerequisite you
expected is missing, check that the depending workspace lists it in its
`dependsOn` and that the task's `dependsOn` entry carries the `^`. See
[Selecting what runs](/lattice/docs/filtering).

### `lattice prune` refuses to run

```text
Error: no cache size limit set. Pass --max-size, or set settings.maxCacheSize in lattice.json
```

`prune` evicts entries until the cache is under a limit, and it will not invent
one. Pass `--max-size 2GB`, or set `settings.maxCacheSize` so every run holds
itself to the same budget:

```json
{
  "settings": {
    "maxCacheSize": "2GB"
  }
}
```

### `.lattice/schema.json` is missing, or your editor shows a stale schema

Any command that opens the project writes the bundled schema when
`.lattice/schema.json` is absent, and leaves an existing file alone however old
it is, so a copy you customized is never churned.

To pick up a newer schema, delete the file and run any command that loads the
config, such as `lattice run build --dry-run`. Or rewrite everything:

```sh
lattice init --force
```

`schema.json` is meant to be committed. It is the one thing under `.lattice/`
that `lattice init` does not add to `.gitignore`.

## Start over

Everything under `.lattice/` is derived, apart from the schema copy:

| Path | What deleting it costs |
| --- | --- |
| `.lattice/cache/` | Cached task results. Every task reruns cold once. |
| `.lattice/toolchains/` | Provisioned engines. Any engine with an `installCmd` reinstalls the next time it is needed, which is a network fetch and not a config change. |
| `.lattice/bin/` | The Lattice versions Lattice manages, present when the repo pins a `latticeVersion` other than the binary on your `PATH`. The next command re-downloads and re-verifies the pinned release. |
| `.lattice/schema.json` | The one file here meant to be committed. If it is gone from disk but still tracked, `git status` shows it deleted; if it is genuinely gone, the next command that loads the config rewrites it. |

`rm -rf .lattice` is a complete reset. Nothing under it is needed for
correctness, only for speed and for not reinstalling tools you already have. It
never touches `lattice.json` at the repo root, and you do not need to rerun
`lattice init` afterward.

## Gather information for a bug report

| Command | What it gives you |
| --- | --- |
| `lattice version` | The version of the binary that actually ran, which matters when the repo pins one. |
| `lattice run <tasks> --dry-run` | The graph and every resolved command, without running or caching anything. |
| `lattice run <tasks> -l` | Every hash, every cache decision, and each task's full output. |

For the models behind these symptoms, see [Driver
detection](/lattice/docs/drivers), [Engines and
provisioning](/lattice/docs/engines), [Caching](/lattice/docs/caching), [Task
graph](/lattice/docs/task-graph), and [Output and
logging](/lattice/docs/output-modes). For the message-by-message reference, see
[Errors](/lattice/docs/errors).
