---
title: Task graph
description: How Lattice expands a task across workspaces and orders the result.
group: Concepts
order: 2
---

# Task graph

`lattice run build` does not run `build` once. It expands `build` into one node
per workspace that defines it, links those nodes by dependency, and runs the
independent ones in parallel.

## From a task name to nodes

The keys under `tasks` in `lattice.json` are names you choose, not commands. For
each requested task, Lattice asks every workspace whether it has a command for
that name. An `auto` workspace answers from its detected driver; an
`"auto": false` workspace answers only from its `scripts` map (see
[Workspaces](/lattice/docs/workspaces) and
[Driver detection](/lattice/docs/drivers) for how a command is resolved).

A workspace with a command becomes one node — `workspace:task`, holding the
resolved command. An `auto` workspace without one is skipped silently. An
`"auto": false` workspace without one halts the run:

```text
Error: workspace 'docs' is "auto": false but declares no command for task
'test'; add it under this workspace's "scripts" map in lattice.json
```

## The two `dependsOn` tokens

A *workspace*'s `dependsOn` names other workspaces. A *task*'s `dependsOn` names
other tasks, in one of two forms:

| Token | Reads as | Edge it creates |
| --- | --- | --- |
| `task` (bare) | "that task, in this same workspace" | `workspace:task` → `workspace:this-task`, for every workspace that has both |
| `^task` | "that task, in each workspace this one depends on" | `depWorkspace:task` → `workspace:this-task`, following that workspace's own `dependsOn` list |

These are the only two forms the graph builder understands: there is no glob and
no `workspace#task` addressing. The token after `^` need not repeat the current
task's name, though in practice it almost always does: a `build` task depending
on `^build` means "build every dependency first." A task with an empty or absent
`dependsOn` has no incoming edges, so every workspace's node for it is a graph
root.

A workspace with no `dependsOn` contributes no `^`-edges anywhere. Narrowing the
workspace set with `--filter` (see
[Selecting what runs](/lattice/docs/filtering)) has the same effect on excluded
workspaces: they contribute no nodes, and a `^`-edge that would have pointed at
one is absent rather than an error.

## Worked example

Four workspaces: `core` has no dependencies; `api` depends on `core`; `web`
depends on `api`; `docs` stands alone.

```json
{
  "workspaces": [
    { "name": "core", "path": "core", "auto": false,
      "scripts": { "build": "echo build core", "test": "echo test core" } },
    { "name": "api", "path": "api", "auto": false, "dependsOn": ["core"],
      "scripts": { "build": "echo build api", "test": "echo test api" } },
    { "name": "web", "path": "web", "auto": false, "dependsOn": ["api"],
      "scripts": { "build": "echo build web", "test": "echo test web" } },
    { "name": "docs", "path": "docs", "auto": false,
      "scripts": { "build": "echo build docs", "test": "echo test docs" } }
  ],
  "tasks": {
    "build": { "dependsOn": ["^build"] },
    "test": { "dependsOn": ["build"] }
  }
}
```

`lattice run build --dry-run` lists every node in one valid dependency order
without running anything:

```text
❖ lattice  dry run · build
  → docs:build  echo build docs
  → core:build  echo build core
  → api:build  echo build api
  → web:build  echo build web
```

Running it for real (`lattice run build -l --no-cache`) shows what that order
means. `docs:build` and `core:build` have no prerequisites, so they start
together; `api:build` waits on `core:build`; `web:build` waits on `api:build`:

```text
lattice: running `build` across 4 workspace(s)
docs:build: running
core:build: running
core:build: build core
docs:build: build docs
docs:build: done (0.00s)
core:build: done (0.00s)
api:build: running
api:build: build api
api:build: done (0.00s)
web:build: running
web:build: build web
web:build: done (0.00s)
lattice: 4 tasks, 0 cached, 0 failed, 0.01s
```

`lattice run test --dry-run` pulls in `build` transitively, so the same four
`build` nodes appear ahead of the four `test` nodes:

```text
❖ lattice  dry run · test
  → docs:build  echo build docs
  → core:build  echo build core
  → api:build  echo build api
  → web:build  echo build web
  → docs:test  echo test docs
  → web:test  echo test web
  → api:test  echo test api
  → core:test  echo test core
```

## Stacked tasks share one graph

`lattice run lint test build` builds one combined graph, not three back to back.
Every requested task's transitive task-name closure is collected first, then all
of it — every task, every workspace that defines it — becomes a single
`DiGraph`. A dependency two stacked tasks have in common is one node: `test`
depends on `build` and `build` is also requested directly, so each workspace's
`build` node appears exactly once, and `lint` parallelizes around it wherever
the graph allows.

## `--sequentially`: one graph per task, in order

`--sequentially` builds a separate full graph per requested task and runs each
start-to-finish before the next begins. Within a phase, tasks still run in
dependency order and in parallel; only the phases no longer overlap. Each
phase's graph is built fresh from that one task's transitive `dependsOn`, so a
later phase can reintroduce nodes an earlier phase already ran:

```text
❖ lattice  dry run · lint (phase)
  → docs:lint  echo lint docs
  → web:lint  echo lint web
  → api:lint  echo lint api
  → core:lint  echo lint core
❖ lattice  dry run · test (phase)
  → docs:build  echo build docs
  → core:build  echo build core
  → api:build  echo build api
  → web:build  echo build web
  → docs:test  echo test docs
  → web:test  echo test web
  → api:test  echo test api
  → core:test  echo test core
```

Reintroduced nodes come back as cache hits in a real run rather than repeated
work (see [Caching](/lattice/docs/caching)). A failing phase stops the run
before the next phase starts, unless you pass `--continue`, which records the
failure, moves on to the next phase, and exits non-zero at the end.

## Parallel execution and its bound

`build_schedule` flattens the graph into an in-degree count per node plus each
node's prerequisites and dependents. The runner keeps a ready set of nodes whose
in-degree has reached zero and spawns all of them at once, bounded by a
semaphore. That bound, not the graph, is what limits parallelism: it defaults to
the number of logical CPUs (`std::thread::available_parallelism`), and
`--concurrency N` caps it at `N`. `--concurrency 0` is ignored rather than
treated as unbounded, so the default takes over. Flag syntax for
`--concurrency`, `--filter`, and `--continue` is in
[Selecting what runs](/lattice/docs/filtering).

As each node finishes, its dependents' in-degree drops by one, and a dependent
reaching zero joins the ready set on the next pass. When a task fails outside
`--continue` mode, no further nodes are spawned and the first failure is
reported. In `--continue` mode, every transitive dependent of the failed node is
marked skipped instead of run.

## Cycles and other graph-construction errors

Lattice topologically sorts the whole graph once before anything runs, so a
cycle — two tasks depending on each other, directly or through a chain — is
rejected before a single command starts:

```text
Error: cycle detected in task dependency graph
```

A requested task with no entry under `tasks` also fails immediately, listing the
tasks that do exist:

```text
Error: task 'nope' is not defined in lattice.json; available tasks: a, b
```

And a [persistent](/lattice/docs/persistent-tasks) task can never be a
prerequisite, since it streams until stopped and never reaches "done" for a
dependent to wait on:

```text
Error: persistent task 'dev' in workspace 'app' cannot be depended on by
other tasks
```
