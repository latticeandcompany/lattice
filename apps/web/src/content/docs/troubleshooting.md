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
lattice run <task> -v          # hashes, cache decisions, and each task's output
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

### A task ran in some workspaces and not others

```text
warn web declares scripts but no "build", so the task was skipped. Did you mean "biuld"?
```

A workspace driven by `npm`, `pnpm`, `yarn`, `bun`, or `deno` can run only a task
its manifest declares as a script. A task the manifest does not declare drops out
of the graph, and the run carries on without it. A misspelled script name and a
workspace with nothing to do for the task look the same, so the warning fires
either way.

To fix a typo, correct the name in the manifest. To give the workspace a command
for the task, add the script to its manifest, or add a `scripts` entry in
`lattice.json`. If the workspace genuinely has nothing to do for that task, the
run is already correct and there is nothing to fix. Lattice used to invent
`npm run build` in that case and fail on it.

`--filter` does not narrow the warning, so a filtered run can name a workspace it
did not select. To see which workspaces the task resolved in, run
`lattice run <task> --dry-run`. See
[What a driver can run](/lattice/docs/drivers#what-a-driver-can-run).

### A task's resolved command is not the one you expected

An entry in a workspace's `scripts` map always wins. Only a task with no entry
there falls back to what the driver would infer, and inference is not universal:
a JavaScript-family driver needs the task name to exist in the manifest's own
scripts map, and a driver that takes the task name on its command line without
being a task runner — `cargo`, `go`, `gradle` — never infers a command for a
`persistent` task, because there is no `cargo dev`. A task runner does infer
one, since it runs the tasks the repo declared to it.

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

`--dry-run` returns before any toolchain is provisioned and before `PATH` is
assembled, so it shows the command as written, not as it will resolve at run
time — neither against a provisioned tool nor against a binary the project
installed under `node_modules/.bin` or `.venv/bin`.

### `command not found` for a tool the project installed

Lattice puts the project's dependency bin directories on a task's `PATH`, so a
task can name `eslint`, `pytest`, or `turbo` directly. It adds only the
directories that exist. Three things break that, in the order worth checking:

The dependencies are not installed. On a fresh clone there is no
`node_modules/.bin` to add. Run `lattice setup`, then the task.

The tool is not a dependency of the project and is not on the host `PATH`
either. Install it as a dependency, or declare it as an
[engine](/lattice/docs/engines) with an `installCmd` and let Lattice provision
it.

The install lives above the repo root. The walk goes from the workspace
directory up to the repo root and stops there, so a dependency directory outside
the repo is never added — only the inherited `PATH` can reach it.

`--dry-run` prints the command but not the `PATH` it will run under, so it
cannot tell these apart. See [Environment
variables](/lattice/docs/environment-variables#tools-the-project-installed) for
the directories and their order.

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

### A key appears twice in the same object

```text
Error: duplicate key `build` in tasks (lattice.json line 12, column 3)
Keep one of them: the second replaces the first, so only the last would take effect
```

Two entries in `tasks`, `engines`, a workspace's `engines`, or a workspace's
`scripts` share a key. Only the last of the two would survive, so the second
entry would drop the first one's fields. Delete whichever you do not want.

The position is where the parser finished the object, not where the repeat sits.
Read the whole container the message names rather than that one line. Keep
`"$schema": ".lattice/schema.json"` in the file and your editor underlines the
repeat as you type it.

### A command in `scripts` is never used

```text
Error: workspace 'core' declares a script 'biuld', but 'biuld' is not defined in `tasks`, so nothing would ever run it
Did you mean `build`?
Defined tasks: build, test
```

A `scripts` key supplies the command for the root task of the same name, so a key
that matches no task can never run. Correct the key, or add the task to `tasks`.
An earlier version accepted the typo, ran the command Lattice detected for the
workspace, and said nothing about the override you wrote.

## Caching

### A task misses the cache every run

Run it with `-v` and read the two lines Lattice prints per task. `-v` is the
only place they appear. The live display leaves them out, because a hit shows its
key on the task's own line and a miss shows up as the task running.

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

A second wording covers the near miss:

```text
lattice: warning: api:build: failed to cache outputs: outputs ["dist"] matched only empty directories, so nothing was cached. Check that the task writes its files where the patterns point
```

Here a pattern did match, and what it matched was a directory holding no files.
A bare `outputs: ["dist"]` covers `dist/` itself, so an empty `dist/` counts as
a match even when the task produced nothing. Run the task, then look inside the
directory. If it is empty, the command is writing somewhere else, or not writing
at all. For why Lattice refuses to store that archive, see [Archive format and
the output
digest](/lattice/docs/cache-internals#archive-format-and-the-output-digest).

### The run warns about restoring a cache entry

Restoring outputs into a workspace can fail on permissions or disk space. That
is a warning, not a failure: the task ran fresh instead, so what is on disk is
correct. A cache hit itself is all-or-nothing. The metadata has to parse, the
tarball has to open, and its digest has to match what was recorded, so a corrupt
entry is a miss and the task reruns rather than replaying something wrong.

### `lattice prune` leaves files behind

```text
❖ removed 0 artifacts, freed 0B
```

`lattice prune` reclaims a leftover from an interrupted run only once that
leftover has sat untouched for an hour. It leaves anything younger where it is,
and those bytes keep counting against `settings.maxCacheSize` until the hour is
up. To free the space now, delete the cache directory. Otherwise run
`lattice prune` again later.

The wait keeps two `lattice` processes sharing one cache directory from deleting
each other's writes. See [the one-hour grace
period](/lattice/docs/cache-internals#the-one-hour-grace-period).

### Every task misses the cache after an upgrade

Expected, once. The running Lattice version is part of every key, so a release
that changes what a key covers moves every key with it. The first run afterwards
re-runs everything. See [Why the Lattice version is part of every
key](/lattice/docs/caching#why-the-lattice-version-is-part-of-every-key).

The miss line says `environment changed`, or reports nothing cached for the task
yet. Both have the same cause. The old entries are still on disk, and they age
out under `settings.maxCacheSize` or `lattice prune` like any others.

### The saved figure is larger than the run took

Expected. The saved figure is task time, not wall clock. Each hit adds the
duration the run that wrote its entry spent, whether or not those tasks would
have run at the same moment, so four cached one-minute tasks on independent
branches report `4m 00s saved` on a run that took a second. The elapsed time
in front of it is the clock. See [Caching](/lattice/docs/caching).

### `lattice stats` says no runs are recorded

```text
No runs recorded yet. Run a task and this fills in — every run appends one line.
```

The ledger is a file inside the cache directory, so anything that clears the
cache clears the history too: deleting `.lattice`, moving `settings.cacheDir`, or
a CI job that started from an empty cache. A run also appends nothing when it
could not store — `--no-cache` — or when it scheduled no task at all, such as a
`--filter` that matched no workspace.

Run a task without `--no-cache` and `stats` fills in from that run onward. There
is nothing to recover: the history is a record, not an input, and losing it costs
the numbers rather than a rebuild.

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

### `lattice setup` says `dependencies up to date` but a dependency is missing

```text
● web dependencies up to date
```

The marker recording the last install is newer than every lockfile that governs
the workspace, so there is nothing to reinstall. A workspace's lockfiles are the
one in its own directory and every one above it up to the repo root. That is what
makes a hoisted npm, pnpm, or yarn tree work, where one root lockfile governs
every workspace under it.

Lattice used to check the workspace's own directory alone. In that everyday
layout a workspace directory holds no lockfile, so nothing could invalidate its
marker, and `lattice setup` reported `dependencies up to date` however far the
root lockfile had moved. On a version that still behaves that way, run
`lattice setup --force` to reinstall regardless.

### `lattice setup` fails on an installer that wants to prompt you

```text
lattice: warning: api: `poetry install` failed
```

`lattice setup` gives the install command no stdin, so an installer that stops
for a password, a token, or a confirmation reads end-of-file and exits non-zero.
Its own output, printed as the install runs, says what it wanted.

To supply the credential without a prompt, put a token in an environment
variable, configure a credential helper, or write a `.netrc`. Otherwise run that
installer once by hand outside Lattice, and let the marker cover it afterwards.
The installer used to inherit the terminal and block on a prompt nothing was
displaying, so the command hung with no output until something killed it.

## Output

### You expected the live display and got plain lines

Lattice uses raw, line-by-line output whenever stdout is not a terminal, `CI` is
set to any value, `-v`/`--verbose` was passed, or `settings.loquacious` is
`true`. A run that pulls a `persistent: true` task into its graph is raw too,
even at a terminal, so a dev server's output stays visible.

Redirecting or piping fails the terminal check, and that is the usual cause.
Nothing forces the live display back on. See [Output and
logging](/lattice/docs/output-modes).

### You wanted plain lines and got the live display

Pass `-v`, set `settings.loquacious` to `true` in `lattice.json`, or set `CI` in
the environment. Any one of them gives you the raw stream, which is also the
readable form when you are piping `lattice run` into something else.

### The live display shows no hash or cache-miss lines

It never prints them. The trace belongs to the raw stream under `-v`, where a
`hash` line and a `cache miss:` line print per task. In the live display a hit
already carries its abbreviated key on the task's own line, and a miss shows up
as the task running, so a dim copy above it said the same thing twice. Earlier
releases printed the trace in both modes.

### Color shows up where it should not

Color follows the terminal, not the output mode. A `-v` run at your shell has
colored `workspace:task` labels and the same run redirected to a file has none.
Escapes in something that is not a terminal mean whatever is running Lattice is
presenting itself as one.

Set `NO_COLOR` to any value to suppress color everywhere without changing the
layout or the mode.

### A run with a dev server in it never finishes

Once a run starts a `persistent: true` task it waits, streaming that task's
output, until you interrupt it. Everything else in the graph finishes first and
its results are visible above. No flag changes this: it is what a `dev` run is.

Four things end such a run:

- `Ctrl-C`, on the first press. The run exits `130` with no message about the
  interrupt. A second press exits immediately without finishing the teardown,
  which matters only when a task left a process holding its output open outside
  the process group Lattice signals.
- A `SIGTERM`, which a CI runner sends when someone cancels the job.
- Every persistent task in the run exiting on its own. A task marked persistent
  by mistake no longer blocks. It gets an `exited (code 0)` line, and the run
  prints its summary.
- Any task in the run failing. The run ends rather than leaving the dev server
  holding it open with nothing left to schedule.

The first two used to need a second press, or a force-kill from the runner.
Lattice started listening for a signal only once the graph had drained, so it
missed one that arrived while a build was still running. For a run that always
terminates, leave the persistent task out of the names you pass. See
[Run dev servers](/lattice/docs/dev-servers).

### The run warns that a process was left holding its output

```text
lattice: warning: a task left a process holding its output open; it is still running.
```

The task started something in its own process group. Lattice signals the group
it spawned, so that process is out of reach, and it still holds the pipe Lattice
reads the task's output through. Rather than wait for output that will never end,
Lattice gives it half a second, says so, and exits.

`tauri dev` does this with its `beforeDevCommand`, so a Tauri workspace whose
`dev` script is `tauri dev` will show this on every interrupt. The process it
leaves keeps its port. Find it by what it is, not by its parent:

```bash
lsof -ti:1420 | xargs kill
```

Nothing is wrong with your config. A launcher that manages its own process
groups is doing so deliberately, and Lattice has no handle on what it started.

### A task cannot depend on your `dev` task

```text
Error: task 'dev' in workspace 'web' is persistent, so no other task may depend on it
```

Nothing can wait on a task that never exits, so a persistent task has to be a
leaf. If another task needs what the dev server produces, depend on the build
step that produces it. See [Persistent tasks](/lattice/docs/persistent-tasks).

## Running

### A run stopped and nothing says why

```text
docs:build: running
ui:build: running
lattice: 2 tasks, 0 cached, 0 failed, 1.49s
```

The run was interrupted. `Ctrl-C`, or the `SIGTERM` a CI runner sends when
someone cancels the job, stops the scheduler and terminates each running task's
whole process group. Those children exit non-zero, and none of those exits is a
task failure. A task the interrupt stopped prints no `FAILED` line, and the
summary does not count it. The events and the summary agree. They previously did
not. Every task the interrupt stopped printed `FAILED` above a summary reporting
`0 failed`.

Nothing prints the word interrupted. The exit code is the only signal, and it is
`130`, so read `$?` rather than the summary. A build that genuinely broke exits
`1`. See [Run Lattice in CI](/lattice/docs/continuous-integration).

### A `FAILED` line says less than you expected

`ui:build: FAILED (code 3) after 1.02s` is the full form. The command ran, and
`code 3` is what it returned. Two kinds of failure cannot fill that in. A task a
signal killed, and a task Lattice stopped for overrunning its `timeout`, have no
exit code, so the line reads `ui:build: FAILED after 30.00s`. A task that failed
before its command ever started has neither a code nor a run to time, so the
line is the bare word `FAILED`. Its cache key would not compute, or its shell
would not spawn, and the captured output under the line says which.

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
| `lattice run <tasks> -v` | Every hash, every cache decision, and each task's full output. |

For the models behind these symptoms, see [Driver
detection](/lattice/docs/drivers), [Engines and
provisioning](/lattice/docs/engines), [Caching](/lattice/docs/caching), [Task
graph](/lattice/docs/task-graph), and [Output and
logging](/lattice/docs/output-modes). For the message-by-message reference, see
[Errors](/lattice/docs/errors).
