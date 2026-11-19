---
title: Output and logging
description: What lattice run prints on a terminal and in a log, and how to get more detail from either.
group: Concepts
order: 8
---

# Output and logging

`lattice run` prints one of two ways. The **live display** is what a terminal
gets: finished tasks settle into a line each while a region at the bottom
redraws in place with what is running now. The **stream** is one plain line per
event, in the order they arrive, with no cursor movement. Lattice picks one
before anything prints and keeps it for the run. `-v` adds detail to either.

Every transcript on this page is a run of the same repo: four workspaces named
`ui`, `web`, `api` and `docs`, where `web` and `api` depend on `ui`.

## Which one you get

The live display is what a terminal gets. The stream is used instead when:

- stdout or stderr is not a terminal: a pipe, a redirect, a CI step;
- `CI` is set to any value;
- you pass `--stream`;
- the run includes a persistent task, such as a dev server. A server prints for
  as long as it is up, which is a stream, not something to redraw. See [Run dev
  servers](/lattice/docs/dev-servers).

`-v` does not choose a mode. It adds the trace — each task's cache key, why it
missed, per-task notes — to whichever mode the run is in. `settings.loquacious`
in `lattice.json` does the same for every run in a repo.

## Read the live display

A build, mid-flight:

```text
❖ lattice  build  · 4 workspaces

  ● docs  cached · 3.02s saved
  ✓ ui    1.02s

  ⠹ api   0.84s
    │ Compiling api v0.1.0 (/repo/services/api)
    │ Compiling serde v1.0.210
  ⠼ web   0.61s
    │ vite v8.0.2 building for production...
    │ transforming (184) src/routes/index.tsx

  2/4 · 1 cached
```

Two parts.

**Above, settled tasks.** Each task gets one line when it ends, printed
permanently, in the order they finished: `✓` and its duration when it passed, a
teal `●` and the task time it saved when it came from cache, `✗` with its exit
code and duration when it failed, `○` when it was skipped or a persistent task
exited with code 0, and a red `✗` when one exited any other way. These lines
scroll away like any other output and stay in your terminal after the run.

**Below, the live region**, redrawn about ten times a second:

- each running task, with a spinner, its elapsed time, and the last three lines
  it printed, so you can see a slow task is compiling rather than stuck;
- up to three tasks still waiting, with what they wait on (`· web  waiting on
  ui`), then `+N more waiting`;
- a count: `2/4 · 1 cached`, and failures in red.

The region never grows taller than the terminal. When the rows do not fit,
output lines go first, then `+N more running` and `+N more waiting` stand in
for the rows that are left out.

Rows are labeled by workspace. A run of several tasks, such as `lattice run lint
test`, labels them `workspace:task`. A run started inside a workspace says so in
the header: `❖ lattice  build  · 3 workspaces  · in api, the workspace you are
in`.

When a task fails, its whole output prints under its line, in the order it
produced it across both streams, so nothing that explains a failure is left in
the part of the screen that gets redrawn:

```text
  ✗ ui    failed (code 3) 1.02s
    │ ui: building 3 files
    │ ui: cannot resolve module 'styles'
```

When the run ends, the live region is replaced by the summary:

```text
❖  4 tasks · 1 cached  3.09s · 3.02s saved
```

And when every task came back from cache, a banner follows it:

```text
❖ lattice  build  · 4 workspaces

  ● docs  cached · 3.02s saved
  ● ui    cached · 1.02s saved
  ● api   cached · 2.04s saved
  ● web   cached · 2.04s saved

❖  4 tasks · 4 cached  0.00s · 8.12s saved

❖❖❖ FULL POWER
```

It needs at least one task, no failures, and a hit for every one.

Notes and warnings print above the region as they happen: a dim
`· using node 22.11.0 · go 1.23.0` just under the header, a dim `· no 'lint'
task in docs, so they sit this one out`, or a `! ui: timed out after 30s and was
stopped` warning. With `-v`, each settled line also carries why the task missed
the cache, and per-task notes print as they come.

The live display draws on stderr, and needs stdout and stderr both to be a
terminal. `lattice run build > build.log` writes the stream to the log, and so
does `lattice run build 2> errors.log`.

## Read the stream

Pipe the same run and you get one line per event, prefixed `workspace:task:`:

```text
$ lattice run build | cat
ui:build: running
docs:build: cache hit [94477519]
ui:build: ui: built 3 files
ui:build: done (1.02s)
api:build: running
web:build: running
api:build:    Compiling api v0.1.0 (/repo/services/api)
web:build: vite v8.0.2 building for production...
api:build: done (2.04s)
web:build: done (2.04s)
lattice: 4 tasks, 1 cached, 0 failed, 3.09s, 3.02s saved
```

Every line a task prints is streamed under its label as it arrives, stdout to
stdout and stderr to stderr. Lines from parallel tasks interleave; the label is
what tells them apart. Nothing repaints and nothing is erased, which is what
makes the stream safe to grep or hand to another tool. A reader that stops
early, such as `| head`, does not stop the run: the lines it no longer reads are
dropped, and every task still runs to the end.

The events, as the stream prints them:

| Event | Line |
| --- | --- |
| A task starts | `ui:build: running` |
| It prints | `ui:build: <the line>` |
| It finishes | `ui:build: done (1.02s)` |
| It comes back from cache | `ui:build: cache hit [5341be25]` |
| It fails | `ui:build: FAILED (code 3) after 1.02s` |
| It overruns its `timeout` | `lattice: warning: ui:build: timed out after 30s and was stopped`, then `ui:build: FAILED after 30.00s` |
| A persistent task exits | `docs:dev: exited (code 0) after 2.02s`, or `EXITED (code 1)` on failure |
| A warning | `lattice: warning: <message>` |
| The run ends | `lattice: 4 tasks, 1 cached, 0 failed, 3.09s, 3.02s saved` |
| Every task came from cache | `lattice: full power, nothing to run` |

A failure line drops what it does not have. A task killed by a signal or its
`timeout` has no exit code, so it reads `FAILED after 30.00s`; a task that failed
before its command started (its key would not compute, its shell would not
spawn) reads `FAILED`, with its error printed under the label. A failed task's
output has already streamed, so it is not printed a second time.

`-v` adds the trace, each line prefixed `lattice:` because Lattice wrote it
rather than a task:

```text
$ lattice run build -v | cat
lattice: running `build` across 4 workspaces
lattice: using node 22.11.0
lattice: ui:build: hash 26be571e2ec773a7
lattice: ui:build: cache miss: inputs changed
ui:build: running
lattice: docs:build: hash 944775197435b927
docs:build: cache hit [94477519]
...
lattice: web:build: cache miss: dependencies, upstream changed
```

The `cache miss:` line names which parts of the key moved. Here an edit to `ui`
moved its `inputs`. `web` then missed on `dependencies`, because a
prerequisite's key is part of its own, and on `upstream`, which covers the files
of every workspace `web` depends on. For what each part covers, see [Cache
internals](/lattice/docs/cache-internals). `-v` also prints the tasks that sat
the run out, and `skipped (dependency failed)` under `--continue`.

## What the saved figure counts

A run with cache hits ends its summary with a saved figure: for each hit, how
long the run that wrote that entry spent producing it. It is task time, not wall
clock. Four cached one-minute tasks with nothing between them add up to `4m 00s
saved`, though running them would have cost about a minute of waiting.

Under a minute, both the elapsed time and the saved figure read as seconds
(`4.27s`). Past a minute the elapsed time switches to clock form (`4:07`,
`1:12:30`) and the saved figure to `4m 07s`, then `14h 22m`.

Each run's figure is appended to a ledger kept with the cache.
[`lattice stats`](/lattice/docs/cli#lattice-stats) adds them up.

## Color

Color follows the terminal. The stream colors each `workspace:task` label, one
of eight evenly weighted hues per task, so interleaved parallel output can be
followed by eye; nothing else on the line is styled, and `FAILED` is still the
word `FAILED`. The live display uses teal for the header, spinners, cache hits
and the saved figure, green `✓` and red `✗` for results. A pipe, a redirect, or
`NO_COLOR` set to any value gets no escapes at all.

## Keep progress and failures apart

In the stream:

| Line | Stream |
| --- | --- |
| `running`, `cache hit`, `done`, `exited (code 0)`, the summary, trace lines | stdout |
| `FAILED`, `EXITED (…)`, `lattice: warning: …`, and errors | stderr |
| A task's own output | whichever stream the task wrote it to |

## An invalid byte costs one character

A task's output is split on newlines and decoded as UTF-8, and anything invalid
becomes U+FFFD. A stray byte from a compiler costs that one character. A
trailing `\r` goes with the newline, and a progress line that rewrites itself
with `\r` shows only its last state in the live display.

## Related pages

- [Run dev servers](/lattice/docs/dev-servers) for the run that always streams.
- [Run Lattice in CI](/lattice/docs/continuous-integration) for what a build
  machine gets.
- [CLI reference](/lattice/docs/cli#global-flags) for `-v` and `--stream`.
