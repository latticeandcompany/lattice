---
title: Dev servers and watchers
description: The daily workflow for running dev servers and watchers across workspaces.
group: Guides
order: 5
---

# Dev servers and watchers

This page is the morning routine: start every dev server the repo declares in
one command, read their interleaved output, start just one, and stop the
whole thing when you're done. The underlying mechanics — what `persistent:
true` actually changes, and how Lattice decides between the live display and
a plain stream — are covered in full on [Persistent
tasks](/lattice/docs/persistent-tasks) and [Output and
logging](/lattice/docs/output-modes); this page assumes you've skimmed those
and just want the commands. Scoping a run to one workspace is [Selecting what
runs](/lattice/docs/filtering)'s job in depth — here it's one flag, `--filter`.

## Declaring a `dev` task

A dev server or watcher is a task like any other, with two fields set on it in
the root `tasks` map: `persistent: true` (it never exits, and its output
streams live) and, usually, `cache: false` (it wouldn't matter — a persistent
task is never cacheable regardless — but it documents the intent). This is the
actual `dev` task from this repo's own `lattice.json`:

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
`package.json` declares `"dev": "astro dev"`. The seven Rust crates in this
repo have no `dev` script and no driver-inferred equivalent, so they're simply
absent from the graph. Confirmed by running the real dry run against this
repo:

```text
$ lattice run dev --dry-run
❖ lattice  dry run · dev
  → web:dev  npm run dev
```

That's the whole rule: an `auto` workspace with no command for a task is
silently skipped, not an error. A workspace declared `"auto": false` is held
to a stricter bar — if it's asked to run `dev` directly and has no matching
entry in its own `scripts` map, the run fails with a named fix instead of
silently doing nothing (`crates/dagger/src/lib.rs:135-141`).

Declaring `dev` across several workspaces looks like this — a shared package
built once, a web app whose dev server needs it built first, and a Rust
service with its own watch command:

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

`web` and `ui-kit` are ordinary JavaScript workspaces (an npm/pnpm/yarn driver
infers `build` and `dev` from their `package.json` scripts); `api`'s Rust
driver has no notion of a watch mode, so its `dev` command is declared
explicitly — an explicit `scripts` entry always wins over whatever a driver
would have inferred (`crates/lattice-workspace/src/lib.rs:846-849`).

## Running every dev server at once

```sh
lattice run dev
```

Because the closure of what you're running includes a `persistent` task,
Lattice forces raw, line-by-line output for the *entire* run, even sitting at
a real terminal — a live spinner display can't render a process that streams
forever (see [Output and logging](/lattice/docs/output-modes)). Every line is
prefixed `workspace:task:`, so with several dev servers running at once their
output interleaves in whatever order it actually arrives. For the three-
workspace example above, that looks something like this:

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

`ui-kit:build` and `web:build` run and finish first because `web:dev` and
`api:dev` both `dependsOn` a `build` — once those are done, both dev servers
start and run side by side, each line tagged with which server it came from.
Nothing settles into a `done` line for either `dev` task; persistent tasks
never get one (see the gotcha below). This is `lattice run` genuinely
launching two long-running processes and never terminating on its own — see
[Stopping everything](#stopping-everything) for how the invocation ends.

## Running one dev server only

Add `--filter` to keep just the workspace you're working on:

```text
$ lattice run dev --filter web --dry-run
❖ lattice  dry run · dev
  → web:dev  npm run dev
```

`--filter` matches on the workspace's `name`, as a substring, before the
dependency graph is even built — so `ui-kit`'s `build` still runs first if
`web`'s `dev` depends on it, but `api` is dropped entirely, not started and
not waited on. A pattern that matches nothing is a clean no-op, not a
failure — confirmed against this repo:

```text
$ lattice run dev --filter zzz
lattice: no workspaces matched filter 'zzz'.
```

See [Selecting what runs](/lattice/docs/filtering) for the full matching
rules and how filtering interacts with a workspace's dependencies.

## Making a watcher wait for a build

A dev server that imports a shared package's build output, or that needs its
own codegen step to have run, declares that with `dependsOn` on the `dev` task
itself — not the other way around:

```json
{
  "tasks": {
    "dev": {
      "dependsOn": ["^build", "build"]
    }
  }
}
```

`^build` runs `build` in every workspace this one `dependsOn` (the shared
`ui-kit` package, in the example above); the bare `build` runs this
workspace's own `build` task first. Both are ordinary dependency edges: they
have to finish (or restore from cache) before the dev server starts, exactly
like any other task's `dependsOn` (see [Task graph](/lattice/docs/task-graph)
for `^task` versus `task` in general).

The edge can only run in this direction. A persistent task must be a leaf —
nothing may depend on it, because nothing can start "after" a process that
never finishes:

```text
persistent task 'dev' in workspace 'web' cannot be depended on by other tasks
```

(`crates/dagger/src/lib.rs:193-208`.) If some other task genuinely needs what
a dev server produces, depend on the build step that produces it, never on
the server.

## Stopping everything

Once every other task in the run has finished and the dev servers are up,
`lattice run` doesn't exit — it waits for `Ctrl-C`, streaming their output the
whole time. One `Ctrl-C` tears every still-running dev server down: on Unix,
Lattice sends `SIGKILL` to each one's whole process group, so a server
launched through a shell (and anything it spawned) dies with it
(`crates/lattice-runner/src/lib.rs:441-461`, `:129-144`). If nothing else in
the run failed, this exits `0` — stopping a dev-server run with `Ctrl-C` is
the normal way to end it, printing the same run summary line as any other
run once every server is torn down. The next section captures one of these
exits for real.

## The gotcha: marked persistent, but it already exited

Lattice never checks a persistent task's exit status — not when it spawns it,
not while it waits for `Ctrl-C`, not at shutdown. If the command behind a
`persistent: true` task quits right away instead of staying up, Lattice has
no way to know and reports nothing wrong. This is easy to hit by accident: a
dev server that refuses to start because something is already listening on
its port still counts as running, from Lattice's side. Captured for real
against this repo's own `web:dev`, with an `astro` dev server already up on
its port:

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

The `npm run dev` process printed that message and exited immediately —
verified separately: its child process was a zombie within a second of
starting. Lattice never noticed. It sat there for the next 34 seconds,
waiting for `Ctrl-C`, exactly as if the server were still up, and reported the
run as a plain success once stopped. There's no `done` or `FAILED` line to
tell you otherwise, ever, for a persistent task — check that the thing you
expect to be listening actually is, don't trust the run summary for it. See
[Persistent tasks](/lattice/docs/persistent-tasks) for why this is true by
design rather than a bug.

## The reverse gotcha: it behaves like a server, but isn't marked persistent

The other direction is worse. If you forget `persistent: true` on a task whose
command doesn't exit — a dev server, a `--watch` build — Lattice treats it
like any ordinary task: it waits on the process's exit code
(`crates/lattice-runner/src/lib.rs:641-647`), which never comes. The run
never finishes on its own; it prints `workspace:task: running` and then
nothing else, forever. Two things follow from that, both verified directly:

The task keeps its concurrency permit for as long as the process runs, since
the permit isn't released until the task's function returns — which it never
does. With a small `--concurrency`, a single forgotten dev-server task can
starve everything else waiting for a slot.

`Ctrl-C` doesn't get the graceful teardown described above, because Lattice
only starts listening for it once a task has actually registered as
persistent — and this one never does. Nothing in `lattice run` installs a
signal handler while it's just waiting on an ordinary child, so `Ctrl-C` falls
through to the OS default: it kills the `lattice run` process itself, with no
chance to run the process-group cleanup. The server process it spawned is not
part of that process group teardown and is not killed — it's simply orphaned,
still running, still bound to its port, until you find and kill it by hand.

If a task's command is meant to keep running, mark it `persistent: true`. If
it's meant to exit, leave `persistent` unset. There's no in-between: the
scheduler treats every task as one or the other, and getting it backwards
either hides a dead server behind a "successful" run, or leaves a live one
running after you thought you'd stopped it.
