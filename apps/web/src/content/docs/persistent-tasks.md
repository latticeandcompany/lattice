---
title: Persistent tasks
description: What persistent true declares, and what it changes about caching, output, and shutdown.
group: Concepts
order: 7
---

# Persistent tasks

A persistent task is one that isn't supposed to exit: a dev server, a file
watcher, a log tailer. You declare it with `persistent: true` on the task in
`lattice.json`. That one field changes caching, output, scheduling, and
shutdown. For the day-to-day workflow of running one, see [Dev servers and
watchers](/lattice/docs/dev-servers); for output mode detection in general, see
[Output and logging](/lattice/docs/output-modes).

## Declaring one

```json
{
  "tasks": {
    "build": { "outputs": ["dist/**"] },
    "dev": {
      "dependsOn": ["build"],
      "persistent": true
    }
  }
}
```

`persistent` sits on the task definition alongside `dependsOn`, `inputs`,
`outputs`, `env`, and `cache`. There's no per-workspace override: it's a
property of the task itself, true for every workspace that runs it.

## It is never cached

A task is cacheable only when it is not persistent and has not set
`cache: false`. So `persistent: true` alone disqualifies a task from caching —
you don't also need `cache: false`. A persistent task produces a running process,
not a result to store.

## It forces raw, streaming output for the whole run

If any task in the transitive closure of what you're running is persistent,
`lattice run` drops the live TUI and uses the same line-by-line output as CI,
even on a terminal. The switch applies to the *entire* run, not just the
persistent task's own output, and happens before anything starts.

Within that raw stream, a persistent task's output lines print unconditionally,
whether or not you passed `-l`/`--loquacious`. Other tasks' output still streams
live only in loquacious mode, and is otherwise buffered and shown on failure.

## Its dependencies run first, but it never holds a concurrency slot

A persistent task's own `dependsOn` edges behave like any other task's: its
dependencies have to complete (or restore from cache) before it starts.

What differs is what happens once it *is* started. An ordinary task holds a
concurrency permit for as long as its process runs, because the scheduler is
waiting on the exit code. A persistent task's process is spawned and handed to
two background tasks, one streaming its output and one waiting on it; the
scheduler then treats that task as started and moves on immediately. It never
occupies a `--concurrency` slot for its actual lifetime, and the rest of the
graph doesn't wait for it to exit.

## Nothing may depend on a persistent task

A persistent task must be a leaf of the graph. If another task lists it under
`dependsOn`, building the graph fails outright:

```text
persistent task 'dev' in workspace 'app' cannot be depended on by other tasks
```

Nothing can start after a process that never finishes. If another task needs the
artifact a dev server would produce, depend on the build step that produces it.

## Its exit is reported

Lattice watches every persistent child it starts. When one ends without being
asked to, the run prints a line for it under that task's label:

```text
web:dev: EXITED (code 1) after 2.14s
```

Any exit that isn't a clean `0` counts as a failed task in the run summary, and
the run exits non-zero. A child a signal ended reads `EXITED (killed by signal)`
and counts the same. Both go to stderr, like every other failure line.

An exit code of `0` is reported too, on stdout, and counts as nothing:

```text
web:dev: exited (code 0) after 0.31s
```

You asked for a process that keeps running and no longer have one, so the run
tells you either way.

Once a persistent task has exited it stops holding the run open. If it was the
only one, `lattice run` prints its summary and exits rather than waiting for a
Ctrl-C with nothing left to stop. With other persistent tasks still up, the run
keeps waiting and keeps streaming them.

The exit also takes down whatever the command left behind: on Unix, Lattice
signals the rest of that child's process group, so a server the command
backgrounded before quitting isn't left holding a port.

## Stopping a run

A run with no persistent task in its closure exits as soon as its graph drains.
No signal handling is involved.

A run whose persistent tasks are all still up waits for Ctrl-C (`SIGINT`) once
every other task has finished, streaming their output the whole time. On Ctrl-C,
Lattice tears every still-running persistent child down: on Unix it sends
`SIGKILL` to the child's whole process group, so a dev server launched through a
shell (and any grandchildren it spawned) dies with it; on other platforms only
the direct child is killed. Either way it's a hard kill, not a `SIGTERM`, so the
process gets no chance to run its own shutdown hooks.

A child killed this way is not reported as an exit and never counts as a
failure. If nothing else in the run failed, Ctrl-C exits `0`.

## Choosing the flag correctly

Set `persistent: true` only on a command whose job is to keep running: a dev
server, a `--watch` build, a log tailer. A command that's meant to exit will run
and be reported either way, but marking it persistent costs you the things an
ordinary task gets. It can't be cached, so it re-runs every time. It can't be
depended on, so nothing can be sequenced after it. It holds no concurrency slot,
so `--concurrency` stops bounding it. And a failure in it never stops the rest
of the graph, because by the time the exit is known the scheduler has moved on.

Because persistent tasks must be leaves, put the setup work a dev server needs
(installing dependencies, an initial build) in its `dependsOn`.
