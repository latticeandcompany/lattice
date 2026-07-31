---
title: Dev servers and watchers
description: The daily workflow for running dev servers and watchers across workspaces.
group: Guides
order: 5
---

# Dev servers and watchers

Start every dev server the repo declares in one command, read their interleaved
output, start just one, and stop everything when you're done. For the mechanics
behind it — what `persistent: true` changes, and how the output mode is chosen —
see [Persistent tasks](/lattice/docs/persistent-tasks) and [Output and
logging](/lattice/docs/output-modes). For the full matching rules behind
`--filter`, see [Selecting what runs](/lattice/docs/filtering).

## Declaring a `dev` task

A dev server or watcher is a task like any other, with two fields set on it in
the root `tasks` map: `persistent: true` (it never exits, and its output streams
live) and, usually, `cache: false`. A persistent task is never cacheable
regardless, so `cache: false` only documents the intent. This is the actual
`dev` task from this repo's own `lattice.json`:

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

Only one workspace here — `web` (`apps/web`) — has anything to run for it: its
`package.json` declares `"dev": "astro dev"`. The seven Rust crates have no
`dev` script and no driver-inferred equivalent, so they're absent from the
graph:

```text
$ lattice run dev --dry-run
❖ lattice  dry run · dev
  → web:dev  npm run dev
```

An `auto` workspace with no command for a task is skipped silently, not an
error. A workspace declared `"auto": false` is stricter: asked to run `dev`
directly with no matching entry in its own `scripts` map, the run fails with a
named fix.

Declaring `dev` across several workspaces looks like this — a shared package
built once, a web app whose dev server needs it built first, and a Rust service
with its own watch command:

```json
{
  "workspaces": [
    { "name": "ui-kit", "path": "packages/ui-kit" },
    { "name": "web", "path": "apps/web", "dependsOn": ["ui-kit"] },
    { "name": "api", "path": "services/api", "scripts": { "dev": "cargo watch -x run" } }
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

`web` and `ui-kit` are ordinary JavaScript workspaces, where an npm/pnpm/yarn
driver infers `build` and `dev` from `package.json` scripts. `api`'s Rust driver
has no notion of a watch mode, so its `dev` command is declared explicitly. An
explicit `scripts` entry always wins over what a driver would have inferred.

## Running every dev server at once

```sh
lattice run dev
```

The closure of this run includes a `persistent` task, so Lattice uses raw,
line-by-line output for the entire run, even at a real terminal (see [Output and
logging](/lattice/docs/output-modes)). Every line is prefixed
`workspace:task:`, and with several dev servers up their output interleaves in
arrival order. For the three-workspace example above:

```text
ui-kit:build: running
ui-kit:build: done (0.41s)
web:build: running
web:build: done (1.02s)
web:dev: running
api:dev: running
web:dev: Local:   http://localhost:4321/
api:dev:    Compiling api v0.1.0
web:dev: Network: use --host to expose
api:dev:     Finished dev [unoptimized] target(s) in 1.8s
api:dev:      Running `target/debug/api`
api:dev: listening on 0.0.0.0:8080
```

`ui-kit:build` and `web:build` finish first because `web:dev` and `api:dev` both
`dependsOn` a `build`. Once those are done, both dev servers start and run side
by side. Neither `dev` task ever settles into a `done` line; persistent tasks
never get one. The invocation itself doesn't terminate — see [Stopping
everything](#stopping-everything).

## Running one dev server only

Add `--filter` to keep just the workspace you're working on:

```text
$ lattice run dev --filter web --dry-run
❖ lattice  dry run · dev
  → web:dev  npm run dev
```

`--filter` matches a workspace's `name` as a substring, before the dependency
graph is built. `ui-kit`'s `build` still runs if `web`'s `dev` depends on it,
but `api` is dropped entirely — not started, not waited on. A pattern that
matches nothing is a no-op, not a failure:

```text
$ lattice run dev --filter zzz
lattice: no workspaces matched filter 'zzz'.
```

See [Selecting what runs](/lattice/docs/filtering) for how filtering interacts
with a workspace's dependencies.

## Making a watcher wait for a build

A dev server that imports a shared package's build output, or that needs its own
codegen step to have run, declares that with `dependsOn` on the `dev` task
itself:

```json
{
  "tasks": {
    "dev": {
      "dependsOn": ["^build", "build"]
    }
  }
}
```

`^build` runs `build` in every workspace this one `dependsOn`; the bare `build`
runs this workspace's own `build` first. Both are ordinary dependency edges and
have to finish (or restore from cache) before the dev server starts (see [Task
graph](/lattice/docs/task-graph) for `^task` versus `task`).

The edge only runs in this direction. A persistent task must be a leaf, so
nothing may depend on it:

```text
persistent task 'dev' in workspace 'web' cannot be depended on by other tasks
```

If another task needs what a dev server produces, depend on the build step that
produces it.

## Stopping everything

Once every other task in the run has finished and the dev servers are up,
`lattice run` waits for `Ctrl-C`, streaming their output while it waits. One
`Ctrl-C` tears every still-running dev server down: on Unix, Lattice sends
`SIGKILL` to each one's whole process group, so a server launched through a shell
(and anything it spawned) dies with it. If nothing else in the run failed, this
exits `0` and prints the same run summary line as any other run.

## Marked persistent, but it already exited

Lattice never checks a persistent task's exit status — not when it spawns it,
not while it waits for `Ctrl-C`, not at shutdown. If the command behind a
`persistent: true` task quits immediately instead of staying up, Lattice reports
nothing wrong. A dev server that refuses to start because something is already
listening on its port still counts as running. Captured against this repo's own
`web:dev`, with an `astro` dev server already up on its port:

```text
$ lattice run dev --filter web
web:dev: running
web:dev:
web:dev: > @lattice/web@1.0.0-beta-2 dev
web:dev: > astro dev
web:dev:
web:dev: {"message":"Dev server already running at
http://localhost:4321 (pid 13616)","label":"SKIP_FORMAT","level":"info"}
^C
lattice: 1 tasks, 0 cached, 0 failed, 34.25s
```

The `npm run dev` process printed that message and exited immediately; its child
was a zombie within a second of starting. Lattice waited the next 34 seconds for
`Ctrl-C` as if the server were up, and reported the run as a success. A
persistent task has no `done` or `FAILED` line, so check that the thing you
expect to be listening actually is rather than reading the run summary for it.
See [Persistent tasks](/lattice/docs/persistent-tasks).

## It behaves like a server, but isn't marked persistent

If you leave `persistent: true` off a task whose command doesn't exit — a dev
server, a `--watch` build — Lattice treats it as an ordinary task and waits on
the process's exit code, which never comes. The run prints `workspace:task:
running` and then nothing else, indefinitely. Two consequences:

The task holds its concurrency permit for as long as the process runs; the permit
is only released when the task finishes, which it never does. With a small
`--concurrency`, one forgotten dev-server task can starve everything waiting for
a slot.

`Ctrl-C` doesn't get the teardown described above. Lattice only starts listening
for it once a task has registered as persistent, and this one never does. No
signal handler is installed while `lattice run` waits on an ordinary child, so
`Ctrl-C` falls through to the OS default and kills the `lattice run` process
itself with no chance to run the process-group cleanup. The server it spawned is
orphaned — still running, still bound to its port — until you kill it by hand.

If a task's command is meant to keep running, mark it `persistent: true`. If
it's meant to exit, leave `persistent` unset. The scheduler treats every task as
one or the other.
