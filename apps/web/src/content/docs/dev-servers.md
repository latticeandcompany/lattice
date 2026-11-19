---
title: Run dev servers
description: Start every dev server the repo declares, read their output, and stop them.
group: Guides
order: 5
---

# Run dev servers

`dev`, `start`, `serve` and `watch` are persistent tasks: Lattice starts them,
streams their output, and keeps the run open until you stop it. This guide covers
starting every dev server in the repo, starting one, making a server wait for a
build, and stopping everything.

For what `persistent` changes elsewhere, see [Persistent
tasks](/lattice/docs/persistent-tasks).

## Where the servers come from

The examples use three workspaces: a shared package, a web app that depends on
it, and a Rust service.

```text
packages/ui-kit/package.json   scripts: build
apps/web/package.json          scripts: build, dev   dependencies: ui-kit
services/api/Cargo.toml
```

`web` already has a `dev` script, so it has a `dev` task. `api` is a Cargo crate,
and Cargo has no dev verb, so its command is written out. The `dev` task waits
for the builds it needs:

```json
{
  "workspaces": ["services/*"],
  "tasks": {
    "dev": { "dependsOn": ["^build", "build"] }
  },
  "overrides": {
    "api": { "tasks": { "dev": "cargo watch -x run" } }
  }
}
```

A workspace driven by a task runner needs nothing: `dev` in a `turbo.json`
workspace runs `turbo run dev`, and a justfile's `dev` recipe runs as `just dev`.

Check what resolves before starting anything:

```text
$ lattice run dev --dry-run
❖ lattice  dry run · dev
  → ui-kit:build  npm run build
  → web:build  npm run build
  → api:dev  cargo watch -x run
  → web:dev  npm run dev
```

`ui-kit` has no `dev` script, so it sits this one out; its `build` is here
because `web:dev` waits on it.

## Start every dev server

```sh
lattice run dev
```

A run with a persistent task uses the plain stream, even at a terminal, because
a server prints for as long as it is up. Every line carries its
`workspace:task:` label, and several servers interleave:

```text
api:dev: running
ui-kit:build: running
api:dev: listening on 0.0.0.0:8080
ui-kit:build: done (0.23s)
web:build: running
web:build: done (0.19s)
web:dev: running
web:dev: Local:   http://localhost:4321/
```

`api:dev` starts at once because it waits on nothing. `web:dev` waits for
`ui-kit:build` and `web:build`. A server gets a closing line only if it exits.

## Start one dev server

From inside the app's directory:

```sh
cd apps/web
lattice run dev
```

A run inside a workspace covers that workspace and what it depends on, so this
starts `web`'s server after the builds it needs, and leaves `api` alone. From
anywhere, `--filter` does the same:

```text
$ lattice run dev --filter web --dry-run
❖ lattice  dry run · dev
  → ui-kit:build (dependency)  npm run build
  → web:build  npm run build
  → web:dev  npm run dev
```

To pass flags to the server, put them after `--`:

```sh
lattice run dev --filter web -- --port 5000
```

`web:dev` runs `npm run dev -- --port 5000`; the builds it waits on get nothing.

## Make a server wait for a build

`dependsOn` on the `dev` task orders it after other tasks. `^build` means the
build of every workspace this one depends on; bare `build` means its own. Both
finish, or come back from cache, before the server starts.

The edge only goes that way. Nothing may depend on a persistent task:

```text
error: task 'dev' in workspace 'web' is persistent, so no other task may depend on it
```

If another task needs what a server produces, depend on the build step that
produces it.

## Stop everything

Once the rest of the graph is done and the servers are up, `lattice run` waits,
streaming their output. One `Ctrl-C` takes every server down: on Unix, Lattice
sends `SIGTERM` to each one's whole process group, waits up to five seconds, then
sends `SIGKILL`, so a server launched through a shell dies with everything it
started. `SIGTERM`, which is how a CI runner cancels a job, does the same. The
run exits `130`.

The first press works whenever it lands, including while the builds ahead of the
servers are still running. A second press exits at once, for the case a
launcher starts children in a process group of its own (`tauri dev` does this
with its `beforeDevCommand`): one of those can hold the output open, and Lattice
waits half a second for it, warns that a process was left running, and exits.

Two other things end the run: the last server exiting on its own, and any task
failing. A failure takes the other servers down rather than leave them holding
the run open.

## A server that exits

A server that quits gets a line, and a non-zero exit fails the run:

```text
api:dev: running
api:dev: port 8080 already in use
api:dev: EXITED (code 1) after 0.01s
lattice: 1 tasks, 0 cached, 1 failed, 0.01s
```

`exited (code 0)` is reported too, and does not fail the run; it usually means a
command was never a server. With several servers up, one exiting leaves the
others running.

## A server cannot read the keyboard

Tasks run with an empty stdin and no terminal, so a server's keyboard shortcuts
(`r` to restart, `o` to open a browser) do nothing. Use the server's file
watching, or stop and restart the run.

## A long-running command that is not persistent

A command that never exits, under a name that is not `dev`, `start`, `serve` or
`watch` — a `docs:preview`, a `build --watch` — is an ordinary task unless you
mark it. Lattice waits on its exit code forever, holds a concurrency slot for it,
and shows it behind a spinner in the live display. Mark it:

```json
{ "tasks": { "preview": { "persistent": true } } }
```

A `start` that is a one-shot command, such as a CLI's `start` script that exits,
needs the reverse, `"start": { "persistent": false }`, or nothing can depend on
it.
