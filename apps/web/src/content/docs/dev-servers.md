---
title: Run dev servers
description: Start every dev server the repo declares, read their output, and stop them.
group: Guides
order: 5
---

# Run dev servers

A dev server is a task with `persistent: true` on it. This guide covers starting
every one the repo declares, starting just one, making a server wait for a
build, and stopping everything.

For what `persistent` changes elsewhere, see [Persistent
tasks](/lattice/docs/persistent-tasks). For how the output mode is picked, see
[Output and logging](/lattice/docs/output-modes).

## Declare a `dev` task

Set two fields on the task in the root `tasks` map. `persistent: true` says it
never exits and its output streams live. `cache: false` documents the intent; a
persistent task is never cached whatever `cache` says.

```json
{
  "tasks": {
    "dev": {
      "persistent": true,
      "cache": false
    }
  }
}
```

The examples below use this three-workspace repo: a shared package built once, a
web app whose dev server needs it built first, and a service with its own watch
command.

```json
{
  "workspaces": [
    { "name": "ui-kit", "path": "packages/ui-kit" },
    { "name": "web", "path": "apps/web", "dependsOn": ["ui-kit"] },
    { "name": "api", "path": "services/api", "auto": false, "scripts": { "dev": "cargo watch -x run" } }
  ],
  "tasks": {
    "build": {
      "dependsOn": ["^build"],
      "outputs": ["dist/**", "target/**"]
    },
    "dev": {
      "dependsOn": ["^build", "build"],
      "persistent": true,
      "cache": false
    }
  }
}
```

`web` and `ui-kit` are ordinary JavaScript workspaces, so their `build` and
`dev` commands come from `package.json` scripts. A Rust driver has no notion of
a watch mode, so `api` declares its `dev` command explicitly. A `scripts` entry
always beats what a driver would have inferred.

A workspace driven by a task runner needs neither: `just`, `task`, `turbo`,
`nx`, `rake`, and `mix` run the tasks the repo declared to them, so `dev` in a
`turbo.json` workspace resolves to `turbo run dev` on its own. See [Persistent
tasks](/lattice/docs/persistent-tasks#where-its-command-comes-from).

Check what resolves before you start anything:

```text
$ lattice run dev --dry-run
❖ lattice  dry run · dev
  → ui-kit:build  npm run build
  → web:build  npm run build
  → api:dev  cargo watch -x run
  → web:dev  npm run dev
```

`ui-kit` has no `dev` script, so it contributes no `dev` node. An `auto`
workspace with no command for a task is skipped silently. A workspace declared
`"auto": false` is stricter: with no matching entry in its own `scripts` map, the
run fails and names the fix.

## Start every dev server at once

```sh
lattice run dev
```

Because the run pulls in a persistent task, Lattice uses raw line-by-line output
for the whole run, even at a terminal. The live display repaints in place and
cannot render a process that streams indefinitely. Every line is prefixed
`workspace:task:`, and with several servers up their output interleaves in
arrival order:

```text
api:dev: running
ui-kit:build: running
api:dev: listening on 0.0.0.0:8080
ui-kit:build: done (0.23s)
web:build: running
web:build: done (0.19s)
web:dev: running
web:dev: 
web:dev: > web@1.0.0 dev
web:dev: > echo 'Local:   http://localhost:4321/' && sleep 30
web:dev: 
web:dev: Local:   http://localhost:4321/
lattice: 4 tasks, 0 cached, 0 failed, 4.00s
```

`api:dev` starts immediately because `api` has no `build` to wait on.
`web:dev` waits for `ui-kit:build` and `web:build`. Neither `dev` task settles
into a `done` line: a persistent task gets a line only if it exits.

## Start one dev server only

```sh
lattice run dev --filter web
```

`--filter` matches a workspace's `name` as a substring, and its matches are the
roots of the run. Everything they depend on comes along, tagged
`(dependency)`. Everything else is dropped, not started and not waited on:

```text
$ lattice run dev --filter web --dry-run
❖ lattice  dry run · dev
  → ui-kit:build (dependency)  npm run build
  → web:build  npm run build
  → web:dev  npm run dev
```

A pattern that matches nothing is a no-op, not a failure:

```text
$ lattice run dev --filter zzz
lattice: no workspaces matched filter 'zzz'.
```

See [Selecting what runs](/lattice/docs/filtering).

## Make a server wait for a build

To make a dev server wait on a shared package's build output, or on its own
codegen step, put `dependsOn` on the `dev` task:

```json
{
  "tasks": {
    "dev": {
      "dependsOn": ["^build", "build"]
    }
  }
}
```

`^build` runs `build` in every workspace this one `dependsOn`. The bare `build`
runs this workspace's own. Both are ordinary edges and must finish, or restore
from cache, before the dev server starts. See [Task
graph](/lattice/docs/task-graph).

The edge only runs in that direction. A persistent task must be a leaf, so
nothing may depend on it:

```text
Error: task 'dev' in workspace 'web' is persistent, so no other task may depend on it
```

If another task needs what a dev server produces, depend on the build step that
produces it instead.

## Stop everything

Once the rest of the graph has drained and the servers are up, `lattice run`
waits, streaming their output. One `Ctrl-C` takes every still-running server
down. On Unix, Lattice sends `SIGTERM` to each one's whole process group, waits
up to five seconds, then sends `SIGKILL` to whatever is left. A server launched
through a shell dies with everything it spawned.

The first press is enough, whenever it lands. Lattice listens for the signal from
the moment the run starts, so a `Ctrl-C` while the builds ahead of the servers
are still going is the one that ends the run. It used to take a second press in
that case. The wait began listening only once the graph had drained, and by then
the first signal had come and gone.

`SIGTERM` ends the run the same way. A CI runner sends `SIGTERM` to cancel a
job.

A second `Ctrl-C` exits at once, without finishing the teardown. It exists for
the case the first press cannot cover: a launcher that starts its own children
in a fresh process group leaves them outside the group Lattice signals, and one
of them may hold the task's output open. Lattice waits half a second for that
output to close, warns that a process was left running, and exits anyway — so
the second press is a way out of a wait, not something a normal run needs.
`tauri dev` is the common launcher of this kind; it starts its
`beforeDevCommand` in a group of its own.

An interrupted run exits `130`, the shell's convention for `SIGINT`, so a CI
runner can tell a cancelled run from a failed one. The summary line still
prints, and a server killed this way is not reported as having exited or as
having failed.

Two other things end the run. One is the last persistent task exiting on its
own. Nothing is left to wait for, so Lattice prints the summary without a
`Ctrl-C`. The other is any task in the run failing. The failure stops the
scheduler, and Lattice takes down the servers already up rather than leave them
holding the run open with nothing left to schedule. A failed run used to wait for
a signal only a person was going to send.

## Read a server that exits

Lattice watches each server it starts. One that quits gets a line saying so, and
a non-zero exit counts as a failed task:

```text
$ lattice run dev --filter api
api:dev: running
api:dev: port 8080 already in use
api:dev: EXITED (code 1) after 0.01s
lattice: 1 tasks, 0 cached, 1 failed, 0.01s
```

The run reported it and exited non-zero instead of sitting there as if the
server were up.

An exit code of `0` is reported the same way in lowercase and does not fail the
run:

```text
api:dev: running
api:dev: done
api:dev: exited (code 0) after 0.01s
lattice: 1 tasks, 0 cached, 0 failed, 0.01s
```

That is usually the sign that a command marked `persistent: true` was never a
server to begin with.

With more than one server up, one exiting does not disturb the others. Their
output keeps streaming and the run keeps waiting. The failure shows up in the
summary at the end.

## A command that never exits without `persistent: true`

Leave `persistent: true` off a task whose command does not exit, such as a dev
server or a `--watch` build, and Lattice treats it as an ordinary task and waits
on an exit code that never comes. The run prints `workspace:task: running` and
then nothing, indefinitely. That costs you two things.

The task holds its concurrency permit for as long as the process runs, because
the permit is released when the task finishes. With a small `--concurrency`, one
forgotten dev-server task can starve everything waiting for a slot.

Its output stays collapsed. A run that pulls in a persistent task switches to
raw, line-by-line output, where a server's lines appear as it prints them. A task
Lattice does not know is persistent does not trigger the switch, so the live
display holds the task's output behind a spinner that never resolves. Pass `-v`
to see the output.

`Ctrl-C` still tears the process group down, because Lattice listens for the
signal for the whole run and not only while a persistent task is up. The server
does not outlive the run. What you lose is the run ever finishing on its own.

If a command is meant to keep running, mark it `persistent: true`. If it is
meant to exit, leave `persistent` unset. The scheduler treats every task as one
or the other.
