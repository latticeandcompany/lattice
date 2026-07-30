---
title: Persistent tasks
description: What persistent true declares, and what it changes about caching, output, and shutdown.
group: Concepts
order: 7
---

# Persistent tasks

A persistent task is one that isn't supposed to exit: a dev server, a file
watcher, a log tailer. You declare it with `persistent: true` on the task in
`lattice.json`, and that one flag changes caching, output, scheduling, and
shutdown all at once. This page covers that semantics. For the day-to-day
workflow of running one, see [Dev servers and
watchers](/lattice/docs/dev-servers); for output mode detection in general,
see [Output and logging](/lattice/docs/output-modes).

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

`persistent` lives on the `PipelineTask` alongside `dependsOn`, `inputs`,
`outputs`, `env`, and `cache` (`crates/lattice-config/src/lib.rs:209-217`).
There's no per-workspace override — it's a property of the task itself,
true for every workspace that runs it.

## It is never cached

```rust
pub fn is_cacheable(&self) -> bool {
    !self.is_persistent() && self.cache != Some(false)
}
```

(`crates/lattice-config/src/lib.rs:225-228`.) `persistent: true` alone is
enough to disqualify a task from caching — you don't also need `cache:
false`. That's the right call anyway: a dev server has no output to hash,
and it doesn't produce a cacheable "result" in the first place, it produces a
running process.

## It forces raw, streaming output for the whole run

If any task in the transitive closure of what you're running is persistent,
`lattice run` drops the live TUI and uses the same line-by-line output as CI,
even on a terminal (`crates/dagger/src/lib.rs:62-70`,
`crates/lattice/src/commands/run.rs:95-103`). A spinner-based UI can't render
a process that streams forever, so Lattice doesn't try — it switches the
*entire* run, not just the persistent task's own output, before anything
starts.

Within that raw stream, a persistent task's output lines print unconditionally,
whether or not you passed `-l`/`--loquacious`; other tasks' output still only
streams live in loquacious mode, and is otherwise buffered and shown only on
failure (`crates/lattice-output/src/lib.rs:242-245` and `:348-359`). This is
the one place `persistent` reaches past the task itself and changes something
about the rest of the run.

## It runs after its dependencies, but doesn't block on them finishing

A persistent task's own `dependsOn` edges behave exactly like any other
task's — its dependencies still have to complete (or restore from cache)
before it starts. What's different is what happens once it *is* started: an
ordinary task holds its slot in the scheduler (and a concurrency permit) for
as long as the process runs, because the scheduler is waiting on its exit
code. A persistent task's process is spawned, detached, and handed off to a
background reader that streams its output; the scheduler then considers that
task done and moves on to the next work immediately
(`crates/lattice-runner/src/lib.rs:616-639`). It never occupies a
`--concurrency` slot for its actual lifetime, and the rest of the graph — and
the `lattice run` invocation as a whole — doesn't wait for it to exit.

## Nothing may depend on a persistent task

A persistent task must be a leaf of the graph. If another task lists it under
`dependsOn`, building the graph fails outright:

```text
persistent task 'dev' in workspace 'app' cannot be depended on by other tasks
```

(`crates/dagger/src/lib.rs:193-208`.) This makes sense once you think about
what a dependency edge means here: nothing can start "after" a process that
never finishes. If some other task genuinely needs the artifact a dev server
would produce, depend on the build step that produces it, not on the server.

## Stopping a run

An ordinary run (no persistent task in its closure) exits as soon as its
graph drains — there's no special signal handling involved.

A run that started a persistent task is different: once every other task has
finished and the persistent one has been spawned, `lattice run` waits for
Ctrl-C (`SIGINT`) before it will exit, streaming the persistent task's output
the whole time it waits (`crates/lattice-runner/src/lib.rs:441-461`). Press
Ctrl-C and Lattice tears every still-running persistent child down: on Unix
it sends `SIGKILL` to the child's whole process group, so a dev server
launched through a shell (and any grandchildren it spawned) dies with it, not
just the immediate child; on other platforms only the direct child process is
killed (`crates/lattice-runner/src/lib.rs:129-144`). Either way it's a hard
kill, not a `SIGTERM` — the process gets no chance to run its own shutdown
hooks. If nothing else in the run failed, this exits `0`: stopping a
persistent run with Ctrl-C is the normal way to end it, not a failure.

## A persistent task's exit is never observed

This is the sharp edge worth knowing about. When a task is persistent,
Lattice spawns it, detaches it, and immediately reports it as done
(`TaskOutcome::Persistent`) without ever looking at its exit status — not
when it's spawned, not while the run waits for Ctrl-C, and not at shutdown,
where the exit code from reaping it is discarded
(`crates/lattice-runner/src/lib.rs:616-639`, `:132-143`). There is no
`Finished` or `Failed` event for a persistent task, ever.

That's fine for a task that actually behaves like a dev server: it isn't
supposed to exit, so there's nothing to observe. It's a real problem if you
mark a task persistent that actually runs to completion — a build script, a
codegen step, anything that's supposed to exit. If that command fails, or
even just finishes and quits, Lattice doesn't notice either way: the run
doesn't fail, doesn't move on, and doesn't tell you. It sits there waiting
for Ctrl-C as if the process were still alive.

## What makes a good persistent task declaration

Only set `persistent: true` on a command whose job is to keep running: a dev
server, a `--watch` build, a log tailer. If the command is meant to exit —
even a slow one — leave `persistent` unset so Lattice can cache it and can
tell you when it fails. And because persistent tasks must be leaves, put the
setup work a dev server needs (installing dependencies, an initial build) in
its `dependsOn`, not the other way around — nothing should ever need to
depend on the server itself.
