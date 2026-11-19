---
title: Selecting what runs
description: Narrow a run to one workspace, run from inside a workspace, pass arguments through, stack tasks, or change how the graph executes.
group: Concepts
order: 6
---

# Selecting what runs

`lattice run` starts from the full task graph for the tasks you name. The
directory you run it from, and the flags here, change which part of it executes
and how. For how the graph is built, see [Task graph](/lattice/docs/task-graph).

The examples on this page use four workspaces: `api` depends on `core`, `web`
depends on `api`, and `docs` stands alone.

## Run from inside a workspace

A run started from a workspace's directory, or any directory below it, is scoped
to that workspace and everything it depends on:

```text
$ cd packages/api
$ lattice run build --dry-run
❖ lattice  dry run · build
  → core:build (dependency)  npm run build
  → api:build  npm run build
```

The run says so as it starts: the live display's header reads `in api, the
workspace you are in`, and the stream's first line is `lattice: running in api,
the workspace you are in`. It works the way a package manager does inside a
package directory, and it means `cd` into what you are working on is enough.

From the repo root, a run covers every workspace. `--filter` replaces the
directory's scope. A run whose workspace has none of the tasks named fails,
the way an unknown task does, rather than running nothing.

## Narrow the run with `--filter`

`-f`/`--filter <PATTERN>` selects the workspaces whose name contains the
pattern, and runs the tasks you named there plus everything they depend on:

```sh
lattice run build --filter api
```

It matches the workspace's name, not its path, as a substring rather than a
glob: `--filter ap` matches `api` and `@acme/app` alike. Names come from
manifests, so a scoped package is matched by its full name. Only one `--filter`
is accepted per run.

A filter that matches nothing prints `lattice: no workspaces matched filter
'nonexistent'.` and exits `0`.

### Dependencies come along

The matched workspaces are the roots of the run. What they depend on is pulled
in and tagged:

```text
$ lattice run build --dry-run --filter web
❖ lattice  dry run · build
  → core:build (dependency)  npm run build
  → api:build (dependency)  npm run build
  → web:build  npm run build
```

Each prerequisite whose inputs have not changed comes back from cache, so a
filtered run costs a lookup per prerequisite, not a rebuild. The edges travel
one way only: `--filter core` runs `core` alone, even though `api` and `web`
depend on it.

A prerequisite pulled in is asked only for the tasks its dependents need.
`lattice run test --filter web` runs `web`'s test and the builds it waits on, not
`api`'s tests.

## Pass arguments to a task

Arguments after `--` go to the tasks you named:

```sh
lattice run test -- --watch=false
lattice run test --filter api -- --reporter=dot
```

They are appended to the command of each named task, in the workspaces the run is
for, and never to a prerequisite pulled in. A script run through `npm run` gets
npm's own `--` first, so the first line runs `npm run test -- --watch=false`.
Every other command gets them directly, which means a tool's own separator is
still yours to write: `lattice run test -- -- --nocapture` for Cargo's test
harness. Arguments are part of the command, so they are part of the cache key: a
run with different arguments is a different entry.

## Preview a run with `--dry-run`

`--dry-run` prints the resolved graph in one valid order and exits without
running or caching anything. Each line is `workspace:task` and the exact command
the shell would get. Top to bottom is an order the scheduler would honor, not a
timeline: independent branches run at once in a real run. It composes with
everything on this page, and it checks no engines.

## Keep going after a failure

By default one failure stops the run: nothing queued starts, and tasks already
running finish. `--continue` keeps starting everything that does not depend on
the failed task:

```sh
lattice run build --continue
```

A task downstream of a failure is marked skipped (`dependency failed`) and never
started. Either way, `lattice run` exits `1` if anything failed.

## Cap parallelism

`--concurrency N` caps how many tasks run at once across the whole run. Without
it the cap is the number of logical CPUs. It does not change the graph:
dependency order still holds. `--concurrency 0` is the default.

```sh
lattice run test --concurrency 4
```

## Name several tasks in one run

```sh
lattice run lint test build
```

Several names merge into one graph, so a node two of them share runs once. See
[Naming several tasks builds one
graph](/lattice/docs/task-graph#naming-several-tasks-builds-one-graph).

## Run each task to completion before the next

`-s`/`--sequentially` gives each named task its own graph, run to completion in
the order you listed:

```text
$ lattice run lint build --sequentially --dry-run
❖ lattice  dry run · lint (phase)
  → docs:lint  npm run lint
  → web:lint  npm run lint
❖ lattice  dry run · build (phase)
  → core:build  npm run build
  → docs:build  npm run build
  → api:build  npm run build
  → web:build  npm run build
```

`core` and `api` have no `lint` script, so they sit the lint phase out. Reach for
this when a later task must never overlap an earlier one, such as `clean` before
`build`. A failing phase stops the run unless `--continue` is set, in which case
every phase runs and the run exits `1` at the end.
