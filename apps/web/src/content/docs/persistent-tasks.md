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
waiting on the exit code. A persistent task's process is spawned, detached, and
handed to a background reader that streams its output; the scheduler then treats
that task as done and moves on immediately. It never occupies a `--concurrency`
slot for its actual lifetime, and the rest of the graph doesn't wait for it to
exit.

## Nothing may depend on a persistent task

A persistent task must be a leaf of the graph. If another task lists it under
`dependsOn`, building the graph fails outright:

```text
persistent task 'dev' in workspace 'app' cannot be depended on by other tasks
```

Nothing can start after a process that never finishes. If another task needs the
artifact a dev server would produce, depend on the build step that produces it.

## Stopping a run

A run with no persistent task in its closure exits as soon as its graph drains.
No signal handling is involved.

A run that started a persistent task waits for Ctrl-C (`SIGINT`) once every
other task has finished, streaming the persistent task's output the whole time.
On Ctrl-C, Lattice tears every still-running persistent child down: on Unix it
sends `SIGKILL` to the child's whole process group, so a dev server launched
through a shell (and any grandchildren it spawned) dies with it; on other
platforms only the direct child is killed. Either way it's a hard kill, not a
`SIGTERM`, so the process gets no chance to run its own shutdown hooks. If
nothing else in the run failed, this exits `0`.

## A persistent task's exit is never observed

Lattice spawns a persistent task, detaches it, and reports it as started without
ever looking at its exit status — not when it's spawned, not while the run waits
for Ctrl-C, and not at shutdown, where the exit code from reaping it is
discarded. A persistent task never gets a completion line or a `FAILED` line.

For a command that stays up, there's nothing to observe. For a command that runs
to completion — a build script, a codegen step — marking it persistent means
Lattice never notices it failed, or even that it finished. The run doesn't fail,
doesn't move on, and doesn't say anything; it sits waiting for Ctrl-C as if the
process were still alive.

## Choosing the flag correctly

Set `persistent: true` only on a command whose job is to keep running: a dev
server, a `--watch` build, a log tailer. If the command is meant to exit — even
a slow one — leave `persistent` unset so Lattice can cache it and report its
failure. Because persistent tasks must be leaves, put the setup work a dev
server needs (installing dependencies, an initial build) in its `dependsOn`.
