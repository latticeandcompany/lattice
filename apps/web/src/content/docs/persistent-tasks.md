---
title: Persistent tasks
description: Why a task that never exits needs its own rules, and what those rules cost a task that did not need them.
group: Concepts
order: 7
---

# Persistent tasks

A scheduler is built around a question: has this task finished? Everything
downstream of a task waits on the answer, the exit code decides success, and the
output is worth collapsing because there will be a last line eventually.

A dev server never answers. Neither does a file watcher or a log tailer. Set
`persistent: true` on a task and you are telling Lattice that this one is in that
category, and four separate assumptions have to be dropped for it.

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

`persistent` sits on the task definition, and there is no per-workspace
override. A task either is the kind of thing that keeps running or it is not, and
that is a property of the task rather than of where it runs.

## It cannot be cached, and not because caching was hard

A persistent task produces a running process. There is no artifact to store and
no result to restore, so `persistent: true` disqualifies a task from caching on
its own. You do not also need `cache: false`.

The interesting part is what a hit would even mean. Restoring a cached dev server
would mean not starting the server, which is the one thing the run exists to do.
Caching here is not merely useless; a hit would be a wrong answer.

## It cannot be depended on, and that shapes your config

A persistent task has to be a leaf. Listing one under another task's `dependsOn`
fails while the graph is being built, before anything starts:

```text
task 'dev' in workspace 'app' is persistent, so no other task may depend on it
```

There is no clever version of this. A dependent waits for its prerequisite to
finish, and this prerequisite has promised not to. Any workaround would mean
inventing a signal for "started enough", which is a definition Lattice would
have to guess at for every server anyone ever runs.

The consequence for your config is that setup work goes in the *other*
direction. A dev server that needs dependencies installed and an initial build
puts both in its own `dependsOn`, and they run to completion before it starts.
That is the arrangement Lattice can actually guarantee.

## It gives up its concurrency slot immediately

An ordinary task holds a concurrency permit for as long as its process runs,
because the scheduler is sitting there waiting on the exit code. A persistent
task is spawned, handed to a pair of background tasks that stream its output and
wait on it, and then reported as started so the scheduler moves on.

So a persistent task never occupies a `--concurrency` slot for its actual
lifetime, and nothing behind it in the graph waits for it. This is the only way
the arithmetic works. Three dev servers on a machine with `--concurrency 2` would
otherwise deadlock the run permanently, and nothing would ever tell you why.

The cost of this is that a persistent task's failure arrives too late to stop
anything. By the time an exit code exists, the scheduler has long since moved
past that node, so a dead dev server never halts the rest of the graph. It is
reported, and the run exits non-zero, but the fail-fast behavior you get from an
ordinary task does not apply.

## It forces raw output for the entire run

If any task in the transitive closure of what you asked for is persistent,
Lattice drops the live TUI and uses the same line-by-line output CI gets, even
on a terminal. The decision is made before anything starts and it applies to the
whole run rather than to the persistent task alone.

A progress display works by owning a region of the terminal and redrawing it.
Streaming output works by appending lines forever. You cannot have both in one
terminal, and a dev server's log is the reason the run is open, so the log wins.

Inside that raw stream, a persistent task's lines print unconditionally, with or
without `-l`. Other tasks stream only in loquacious mode and are otherwise
buffered and shown on failure. See
[Output and logging](/lattice/docs/output-modes).

## An unexpected exit is reported either way

Lattice watches every persistent child it starts. A non-zero exit is a failed
task in the summary, printed to stderr, and the run exits non-zero:

```text
web:dev: EXITED (code 1) after 0.31s
```

A child a signal ended reads `EXITED (killed by signal)` and counts the same.

A clean exit is also reported, on stdout, and counts as nothing:

```text
web:ok: exited (code 0) after 0.21s
```

Reporting the clean case looks redundant and is not. You asked for a process that
keeps running, and you no longer have one. Whether that was a crash or an orderly
shutdown, the fact that the thing you started is gone is information, and
silence would have you staring at a terminal waiting for a server that stopped
four minutes ago.

Once a persistent task has exited it stops holding the run open. If it was the
only one, the run prints its summary and exits rather than waiting for a Ctrl-C
with nothing left to stop. With others still up, it keeps waiting and keeps
streaming them. Either way, the exit takes down whatever the command left
behind: on Unix, Lattice signals the rest of that child's process group, so a
server the command backgrounded before quitting is not left holding a port.

## Stopping a run

A run with no persistent task in its closure exits when its graph drains. No
signal handling is involved.

A run whose persistent tasks are still up waits for Ctrl-C once every other task
has finished, streaming output the whole time. On Ctrl-C on Unix, Lattice sends
`SIGTERM` to each still-running child's process group, waits five seconds, then
sends `SIGKILL` to whatever is left. On Windows there is no process group to
signal, so the child and its descendants are taken down directly and there is
nothing a grace period would achieve.

The grace period is there because a dev server usually holds something an
abrupt kill would strand: a TCP port, a socket file, a lock. Signalling the group
rather than the process is there because the command you wrote goes to a shell,
so the process Lattice can see is often not the process doing the work.

A child killed this way is not reported as an exit and does not count as a
failure. The run itself exits `130` rather than `0`, because the run was stopped
rather than completed, and a CI job that gets `0` from a cancelled build has
been told something untrue.

## When not to reach for it

Set `persistent: true` only on a command whose job is to keep running. A command
meant to exit will run and be reported either way, so the flag is not a
correctness problem, but every rule on this page is a subtraction:

- It cannot be cached, so it re-runs every time.
- It cannot be depended on, so nothing can be sequenced after it.
- It holds no concurrency slot, so `--concurrency` stops bounding it.
- Its failure never stops the graph.

A one-shot task marked persistent loses all four and gains nothing. The
signal to watch for is `--watch` in the command, or a port, or the absence of a
plausible last line of output.

## Where to look next

Running a dev server day to day is
[Dev servers and watchers](/lattice/docs/dev-servers). Why the output mode
switched is [Output and logging](/lattice/docs/output-modes). The exact text of
every message here is in [Errors](/lattice/docs/errors).
