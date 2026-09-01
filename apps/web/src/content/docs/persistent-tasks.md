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

## Where its command comes from

Declaring the task is half of it. Each workspace still has to produce a command
for `dev`, and a persistent task is the one case where Lattice is careful about
inferring one:

| The workspace's driver | What happens |
| --- | --- |
| Reads a script map — `npm`, `pnpm`, `yarn`, `bun`, `deno` | Runs the script if the manifest declares one by that name. No script, no `dev` node in that workspace |
| Is a task runner — `just`, `task`, `turbo`, `nx`, `rake`, `mix` | Infers the command. `dev` in a `turbo.json` workspace resolves to `turbo run dev` |
| Anything else — `cargo`, `go`, `gradle`, `poetry`, `composer` | Infers nothing. There is no `cargo dev`, so guessing would produce a command that fails |

The last row is why a Rust or Go workspace needs the command spelled out:

```json
{
  "workspaces": [
    {
      "name": "api",
      "path": "services/api",
      "scripts": { "dev": "cargo watch -x run" }
    }
  ]
}
```

A `scripts` entry always beats inference, so it works in any of the three rows.
A workspace that produces no command is skipped and the run carries on with the
ones that did — a `dev` task can cover the two workspaces that have a dev server
and leave the rest alone.

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
without `-v`. Other tasks stream only under `-v` and are otherwise
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

A run with no persistent task in its closure exits when its graph drains.

A run whose persistent tasks are still up keeps waiting once every other task
has finished, streaming output the whole time. Four things end that wait: you
press `Ctrl-C`, a `SIGTERM` arrives, the last persistent task exits, or any task
in the run fails. A failure ends the run rather than leaving the servers to hold
it open with nothing left to schedule.

Lattice listens for the signal for the whole run, not only while it is waiting,
so the first `Ctrl-C` ends the run wherever it lands, including during the builds
that run ahead of the servers. It used to take a second press in that case, and
a `SIGTERM` never registered at all. A cancelled CI job hung until the runner
force-killed it.

On Unix, Lattice sends `SIGTERM` to each still-running child's process group,
waits five seconds, then sends `SIGKILL` to whatever is left. On Windows there is
no process group to signal, so the child and its descendants are taken down
directly and there is nothing a grace period would achieve.

A process the task started in a *different* process group is outside that reach.
If it also holds the task's output open, Lattice waits half a second for the
output to close, then warns that a process was left running and exits without
it. A second `Ctrl-C` skips even that wait and exits immediately. Both exist
because watching for an interrupt takes over what `SIGINT` and `SIGTERM` mean for
the whole process: without them, a run stuck at this point could not be ended by
any signal short of `SIGKILL`.

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
